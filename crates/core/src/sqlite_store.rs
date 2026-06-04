//! Implémentation SQLite du stockage métier.
//!
//! Utilisée sur :
//!   - Le nœud actif (données métier locales, légères)
//!   - Le nœud passif (réplique locale read-only après replay du journal)
//!
//! PostgreSQL ne gère plus que le journal chiffré + la réplication WAL.
//! SQLite gère l'état métier (stock, numérotation, etc.).
//!
//! Avantages :
//!   - Aucun serveur PostgreSQL requis sur les postes opérateurs
//!   - Fichier unique, portable, sauvegardable simplement
//!   - Ultra-léger (< 1 Mo pour des années de données PME typiques)

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::business_store::{BusinessStore, StoreError, StockEntry};

pub struct SqliteBusinessStore {
    pool: SqlitePool,
}

impl SqliteBusinessStore {
    /// Ouvre (ou crée) la base SQLite métier et applique les migrations.
    pub async fn open(db_path: &str) -> Result<Self, StoreError> {
        let url = if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{db_path}?mode=rwc")
        };

        let pool = SqlitePool::connect(&url)
            .await
            .map_err(|e| StoreError::Db(e.to_string()))?;

        // Migrations inline — pas besoin de fichiers externes
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS stock (
                item_id  TEXT    PRIMARY KEY,
                quantity INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0)
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| StoreError::Db(e.to_string()))?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl BusinessStore for SqliteBusinessStore {
    async fn get_stock(&self, item_id: &str) -> Result<i64, StoreError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT quantity FROM stock WHERE item_id = $1")
                .bind(item_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StoreError::Db(e.to_string()))?;

        Ok(row.map(|(q,)| q).unwrap_or(0))
    }

    async fn apply_sale(&self, item_id: &str, quantity: i64) -> Result<i64, StoreError> {
        // Lire le stock disponible
        let disponible = self.get_stock(item_id).await?;
        if disponible < quantity {
            return Err(StoreError::StockInsuffisant { disponible, demande: quantity });
        }

        // Décrémenter atomiquement (SQLite sérialise les écritures)
        let new_qty: i64 = sqlx::query_scalar(
            "UPDATE stock SET quantity = quantity - $1 WHERE item_id = $2 RETURNING quantity",
        )
        .bind(quantity)
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StoreError::Db(e.to_string()))?;

        Ok(new_qty)
    }

    async fn apply_adjust(&self, item_id: &str, delta: i64) -> Result<i64, StoreError> {
        // Upsert en deux temps pour respecter la contrainte CHECK quantity >= 0
        // INSERT avec max(0, delta) pour les nouvelles entrées
        // UPDATE avec la somme pour les entrées existantes
        let new_qty: i64 = sqlx::query_scalar(
            "INSERT INTO stock (item_id, quantity) VALUES ($1, MAX(0, $2))
             ON CONFLICT (item_id)
             DO UPDATE SET quantity = MAX(0, stock.quantity + $2)
             RETURNING quantity",
        )
        .bind(item_id)
        .bind(delta)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StoreError::Db(e.to_string()))?;

        Ok(new_qty)
    }

    async fn list_stock(&self) -> Result<Vec<StockEntry>, StoreError> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT item_id, quantity FROM stock ORDER BY item_id")
                .fetch_all(&self.pool)
                .await
                .map_err(|e| StoreError::Db(e.to_string()))?;

        Ok(rows.into_iter().map(|(item_id, quantity)| StockEntry { item_id, quantity }).collect())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> SqliteBusinessStore {
        SqliteBusinessStore::open(":memory:").await.expect("sqlite :memory:")
    }

    #[tokio::test]
    async fn ajustement_et_lecture_stock() {
        let s = store().await;
        let q = s.apply_adjust("PANTALON-L", 100).await.expect("adjust");
        assert_eq!(q, 100);
        assert_eq!(s.get_stock("PANTALON-L").await.unwrap(), 100);
    }

    #[tokio::test]
    async fn vente_reduit_le_stock() {
        let s = store().await;
        s.apply_adjust("CHEMISE-M", 50).await.unwrap();
        let apres = s.apply_sale("CHEMISE-M", 10).await.expect("vente");
        assert_eq!(apres, 40);
    }

    #[tokio::test]
    async fn anti_survente_bloque() {
        let s = store().await;
        s.apply_adjust("ITEM-X", 5).await.unwrap();
        let err = s.apply_sale("ITEM-X", 10).await;
        assert!(matches!(err, Err(StoreError::StockInsuffisant { disponible: 5, demande: 10 })));
        // Stock inchangé après refus
        assert_eq!(s.get_stock("ITEM-X").await.unwrap(), 5);
    }

    #[tokio::test]
    async fn article_inconnu_retourne_zero() {
        let s = store().await;
        assert_eq!(s.get_stock("INCONNU").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_stock_ordonne() {
        let s = store().await;
        s.apply_adjust("Z-ITEM", 3).await.unwrap();
        s.apply_adjust("A-ITEM", 7).await.unwrap();
        let list = s.list_stock().await.unwrap();
        assert_eq!(list[0].item_id, "A-ITEM");
        assert_eq!(list[1].item_id, "Z-ITEM");
    }

    #[tokio::test]
    async fn plusieurs_ajustements_cumulatifs() {
        let s = store().await;
        s.apply_adjust("PROD-A", 100).await.unwrap();
        s.apply_adjust("PROD-A", -20).await.unwrap();
        assert_eq!(s.get_stock("PROD-A").await.unwrap(), 80);
    }
}
