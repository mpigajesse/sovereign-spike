//! Trait d'abstraction du stockage métier (ports & adapters).
//!
//! Le cœur Rust ne connaît pas le moteur de stockage — il parle à ce trait.
//! Deux implémentations concrètes :
//!   - `SqliteBusinessStore`   : production PME (SQLite embarqué, léger)
//!   - `PostgresBusinessStore` : spike Phase 0 (PostgreSQL, réplication native)
//!
//! Ce découplage permet de changer le moteur sans toucher à la logique métier.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("article inconnu : {0}")]
    NotFound(String),
    #[error("stock insuffisant : disponible={disponible}, demandé={demande}")]
    StockInsuffisant { disponible: i64, demande: i64 },
    #[error("erreur de stockage : {0}")]
    Db(String),
}

/// Résultat d'une opération métier appliquée.
#[derive(Debug, Clone)]
pub struct StockEntry {
    pub item_id:  String,
    pub quantity: i64,
}

/// Trait principal du stockage métier.
///
/// Une implémentation doit être Send + Sync (utilisée dans un contexte async multi-thread).
/// Les méthodes &self permettent l'utilisation derrière un Arc<dyn BusinessStore>.
#[async_trait]
pub trait BusinessStore: Send + Sync {
    /// Retourne le stock d'un article (0 si inexistant).
    async fn get_stock(&self, item_id: &str) -> Result<i64, StoreError>;

    /// Enregistre une vente : décrémente le stock.
    /// Retourne `StockInsuffisant` si stock < quantite.
    async fn apply_sale(&self, item_id: &str, quantity: i64) -> Result<i64, StoreError>;

    /// Ajuste le stock (delta positif ou négatif).
    /// Retourne le nouveau stock.
    async fn apply_adjust(&self, item_id: &str, delta: i64) -> Result<i64, StoreError>;

    /// Liste tous les articles avec leur stock.
    async fn list_stock(&self) -> Result<Vec<StockEntry>, StoreError>;
}
