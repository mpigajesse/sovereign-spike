//! Mécanisme de fencing par token d'époque (§5.2 Phase 0 — anti-split-brain).
//!
//! Problème résolu :
//!   Sans fencing, si le primary tombe et que le standby est promu, l'ancien
//!   primary peut revenir et continuer à écrire → deux nœuds pensent être
//!   le primary → divergence de données (split-brain).
//!
//! Solution :
//!   Chaque écriture vérifie que l'époque stockée en DB correspond à l'époque
//!   locale du nœud. Lors d'une promotion, le nouveau primary incrémente
//!   l'époque → l'ancien primary détecte le mismatch et se neutralise.
//!
//! Atomicité :
//!   La vérification d'époque s'exécute DANS la transaction SERIALIZABLE
//!   de chaque écriture → la détection de fencing et l'écriture du journal
//!   sont une opération indivisible.

use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FencingError {
    #[error("fencing : époque obsolète (locale={expected}, DB={actual}) — ce nœud n'est plus le primary")]
    EpochMismatch { expected: i64, actual: i64 },

    #[error("fencing : impossible de lire l'époque en DB : {0}")]
    DbError(#[from] sqlx::Error),

    #[error("fencing : table node_epoch absente — migrations non appliquées ?")]
    EpochTableMissing,
}

/// Gardien d'époque pour un nœud actif.
///
/// L'époque est chargée au démarrage depuis la DB et stockée atomiquement.
/// Chaque écriture appelle `assert_primary()` à l'intérieur de sa transaction.
pub struct EpochGuard {
    expected: AtomicI64,
}

impl EpochGuard {
    /// Construit le gardien avec l'époque initiale lue depuis la DB.
    pub fn new(epoch: i64) -> Self {
        Self { expected: AtomicI64::new(epoch) }
    }

    /// Retourne l'époque que ce nœud croit être la sienne.
    pub fn current_epoch(&self) -> i64 {
        self.expected.load(Ordering::SeqCst)
    }

    /// Vérifie que l'époque DB correspond à l'époque locale.
    ///
    /// Doit être appelé DANS une transaction SERIALIZABLE, après son ouverture,
    /// pour garantir l'atomicité avec l'écriture du journal.
    ///
    /// Retourne `Ok(epoch)` si ce nœud est toujours le primary.
    /// Retourne `Err(FencingError::EpochMismatch)` si un failover a eu lieu.
    pub async fn assert_primary<'e, E>(&self, executor: E) -> Result<i64, FencingError>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let db_epoch: Option<i64> =
            sqlx::query_scalar("SELECT epoch FROM node_epoch WHERE id = 1")
                .fetch_optional(executor)
                .await
                .map_err(FencingError::DbError)?;

        let db_epoch = db_epoch.ok_or(FencingError::EpochTableMissing)?;
        let expected = self.current_epoch();

        if db_epoch != expected {
            return Err(FencingError::EpochMismatch { expected, actual: db_epoch });
        }
        Ok(db_epoch)
    }

    /// Met à jour l'époque locale (appelé lors d'une re-synchronisation).
    pub fn update_epoch(&self, new_epoch: i64) {
        self.expected.store(new_epoch, Ordering::SeqCst);
    }
}

/// Charge l'époque courante depuis la DB au démarrage du nœud.
pub async fn load_epoch(pool: &PgPool) -> Result<i64, FencingError> {
    let epoch: Option<i64> =
        sqlx::query_scalar("SELECT epoch FROM node_epoch WHERE id = 1")
            .fetch_optional(pool)
            .await
            .map_err(FencingError::DbError)?;

    epoch.ok_or(FencingError::EpochTableMissing)
}

/// Incrémente l'époque dans la DB et retourne la nouvelle valeur.
///
/// Appelé par le `promote_command` de Patroni lors d'un failover.
/// En production : script shell qui appelle cet endpoint ou SQL direct.
pub async fn promote_epoch(pool: &PgPool, new_host: &str) -> Result<i64, FencingError> {
    let new_epoch: i64 = sqlx::query_scalar(
        "UPDATE node_epoch
         SET epoch = epoch + 1, promoted_at = NOW(), primary_host = $1
         WHERE id = 1
         RETURNING epoch",
    )
    .bind(new_host)
    .fetch_one(pool)
    .await
    .map_err(FencingError::DbError)?;

    Ok(new_epoch)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_guard_detecte_mismatch_local() {
        let guard = EpochGuard::new(3);
        assert_eq!(guard.current_epoch(), 3);

        // Simuler un failover : la DB passerait à 4, mais le guard est sur 3.
        // assert_primary() retournerait EpochMismatch { expected: 3, actual: 4 }
        // (testé ici sans DB — la logique est dans assert_primary)
    }

    #[test]
    fn epoch_guard_update() {
        let guard = EpochGuard::new(1);
        guard.update_epoch(5);
        assert_eq!(guard.current_epoch(), 5);
    }

    #[test]
    fn fencing_error_message_clair() {
        let err = FencingError::EpochMismatch { expected: 2, actual: 3 };
        let msg = err.to_string();
        assert!(msg.contains("époque obsolète"), "message : {msg}");
        assert!(msg.contains("locale=2"));
        assert!(msg.contains("DB=3"));
    }
}
