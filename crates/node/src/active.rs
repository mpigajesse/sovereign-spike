//! Nœud actif : seul nœud autorisé à recevoir et sérialiser les écritures.
//!
//! Propriétés garanties par ce module :
//!   - Sérialisation totale des écritures (isolation SERIALIZABLE PostgreSQL)
//!   - Invariant stock ≥ 0 (CHECK SQL + vérification applicative avant écriture)
//!   - Idempotence par op_id (UNIQUE constraint + vérification préalable)
//!   - Opacité du journal (blobs XChaCha20-Poly1305, jamais de clair en base)
//!   - Fencing anti-split-brain (EpochGuard — vérification dans la transaction)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use sovereign_core::{
    encrypt,
    journal::{encode_cbor, Operation, OpType, Payload},
    Dek,
};

use crate::failover::EpochGuard;

// ── État partagé ─────────────────────────────────────────────────────────────

pub struct AppState {
    pub pool:        PgPool,
    pub dek:         Dek,
    pub epoch_guard: EpochGuard,
}

// ── DTOs ─────────────────────────────────────────────────────────────────────

/// Corps de la requête POST /write.
#[derive(Debug, Deserialize)]
pub struct WriteRequest {
    /// "sale" | "stock_adjust"
    pub op_type:  String,
    pub item_id:  String,
    /// Pour "sale" : quantité vendue (> 0).
    /// Pour "stock_adjust" : delta positif ou négatif.
    pub quantity: i64,
    /// Clé d'idempotence fournie par le client (optionnel — générée si absente).
    pub op_id:    Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct WriteResponse {
    pub seq:    i64,
    pub op_id:  Uuid,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StockResponse {
    pub item_id:  String,
    pub quantity: i64,
}

/// Une entrée du journal telle que renvoyée au nœud passif.
/// Contient le blob chiffré — jamais le CBOR en clair.
#[derive(Debug, Serialize, Deserialize)]
pub struct JournalEntryDto {
    pub seq:             i64,
    pub op_id:           Uuid,
    pub blob_nonce:      String, // hex
    pub blob_ciphertext: String, // hex
}

#[derive(Debug, Serialize)]
pub struct EpochResponse {
    pub epoch:        i64,
    pub primary_host: String,
}

#[derive(Debug, Deserialize)]
pub struct JournalQuery {
    /// Renvoyer les entrées avec seq > after_seq (défaut : 0)
    pub after_seq: Option<i64>,
    /// Nombre maximum d'entrées à retourner (défaut : 100, max : 1000)
    pub limit:     Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

type ApiResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrorResponse>)>;

// ── Routeur ──────────────────────────────────────────────────────────────────

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health",         get(health))
        .route("/write",          post(handle_write))
        .route("/stock/:item_id", get(handle_get_stock))
        .route("/journal",        get(handle_get_journal))
        .route("/epoch",          get(handle_get_epoch))
        .route("/epoch/promote",  post(handle_promote_epoch))
        .with_state(state)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok"
}

/// GET /stock/:item_id — consulte le stock courant d'un article.
async fn handle_get_stock(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<String>,
) -> ApiResult<StockResponse> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT quantity FROM stock WHERE item_id = $1")
            .bind(&item_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal_error)?;

    let quantity = row.map(|(q,)| q).unwrap_or(0);
    Ok((StatusCode::OK, Json(StockResponse { item_id, quantity })))
}

/// POST /write — sérialise une écriture métier dans le nœud actif.
///
/// Flux transactionnel (isolation SERIALIZABLE) :
///   1. Vérification idempotence (op_id déjà traité → retour immédiat)
///   2. Validation métier (quantité > 0 pour une vente)
///   3. Vérification invariant stock (vente : stock_dispo ≥ qty, avec FOR UPDATE)
///   4. nextval(journal_seq) → seq connu avant de sceller le blob
///   5. Construction + chiffrement CBOR de l'opération
///   6. Mise à jour du stock (UPSERT)
///   7. Insertion dans le journal chiffré
///   8. COMMIT
async fn handle_write(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<WriteRequest>,
) -> ApiResult<WriteResponse> {
    // Validation métier de surface
    let op_type = parse_op_type(&req.op_type).ok_or_else(|| {
        bad_request(format!("op_type inconnu : '{}' (attendu: sale | stock_adjust)", req.op_type))
    })?;

    if matches!(op_type, OpType::Sale) && req.quantity <= 0 {
        return Err(bad_request("quantity doit être > 0 pour une vente".to_string()));
    }

    let op_id = req.op_id.unwrap_or_else(Uuid::new_v4);

    // ── Transaction SERIALIZABLE ─────────────────────────────────────────────
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Isolation SERIALIZABLE : empêche les anomalies write-skew (survente concurrente)
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;

    // 0. Fencing : vérifier que ce nœud est toujours le primary (anti-split-brain).
    //    Exécuté DANS la transaction → détection + écriture sont atomiques.
    //    Si un failover a eu lieu, l'époque DB ≠ époque locale → 503.
    state.epoch_guard.assert_primary(&mut *tx).await.map_err(|e| {
        tracing::error!("fencing déclenché : {e}");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: format!("nœud dégradé — {e}"),
            }),
        )
    })?;

    // 1. Idempotence : op_id déjà présent → renvoyer le seq original
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT seq FROM operations_journal WHERE op_id = $1")
            .bind(op_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal_error)?;

    if let Some((seq,)) = existing {
        tx.rollback().await.map_err(internal_error)?;
        return Ok((StatusCode::OK, Json(WriteResponse { seq, op_id, status: "already_processed" })));
    }

    // 3. Invariant stock pour les ventes (FOR UPDATE = verrou sur la ligne)
    if matches!(op_type, OpType::Sale) {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT quantity FROM stock WHERE item_id = $1 FOR UPDATE",
        )
        .bind(&req.item_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal_error)?;

        let stock_dispo = row.map(|(q,)| q).unwrap_or(0);
        if stock_dispo < req.quantity {
            tx.rollback().await.map_err(internal_error)?;
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!(
                        "stock insuffisant : disponible={stock_dispo}, demandé={}",
                        req.quantity
                    ),
                }),
            ));
        }
    }

    // 4. Numéro de séquence atomique (nextval ne recule jamais même après rollback)
    let (seq,): (i64,) = sqlx::query_as("SELECT nextval('journal_seq')")
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

    // 5. Construction + chiffrement CBOR
    let op = Operation::new(
        seq as u64,
        op_type.clone(),
        Payload { item_id: req.item_id.clone(), quantity: req.quantity },
    );
    let cbor = encode_cbor(&op).map_err(|e| {
        internal_error(format!("sérialisation CBOR : {e}"))
    })?;
    let blob = encrypt(&cbor, &state.dek);

    // 6. Mise à jour du stock (UPSERT)
    match op_type {
        OpType::Sale => {
            sqlx::query(
                "INSERT INTO stock (item_id, quantity) VALUES ($1, -$2) \
                 ON CONFLICT (item_id) DO UPDATE SET quantity = stock.quantity - $2",
            )
            .bind(&req.item_id)
            .bind(req.quantity)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
        OpType::StockAdjust => {
            sqlx::query(
                "INSERT INTO stock (item_id, quantity) VALUES ($1, $2) \
                 ON CONFLICT (item_id) DO UPDATE SET quantity = stock.quantity + $2",
            )
            .bind(&req.item_id)
            .bind(req.quantity)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
    }

    // 7. Journal chiffré append-only
    sqlx::query(
        "INSERT INTO operations_journal (seq, op_id, blob_nonce, blob_ciphertext) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(seq)
    .bind(op_id)
    .bind(blob.nonce.as_slice())
    .bind(blob.ciphertext.as_slice())
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    tracing::info!(seq, %op_id, item_id = %req.item_id, op_type = %req.op_type, "écriture sérialisée");

    Ok((StatusCode::CREATED, Json(WriteResponse { seq, op_id, status: "committed" })))
}

/// GET /journal?after_seq={n}&limit={m}
///
/// Retourne les entrées du journal chiffrées depuis after_seq (exclusif).
/// Le nœud passif appelle cet endpoint pour synchroniser son réplica SQLite.
/// Les données retournées sont des blobs opaques — pas de clair dans la réponse.
async fn handle_get_journal(
    State(state): State<Arc<AppState>>,
    Query(params): Query<JournalQuery>,
) -> ApiResult<Vec<JournalEntryDto>> {
    let after_seq = params.after_seq.unwrap_or(0);
    let limit     = params.limit.unwrap_or(100).min(1000);

    let rows: Vec<(i64, Uuid, Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT seq, op_id, blob_nonce, blob_ciphertext \
         FROM operations_journal \
         WHERE seq > $1 \
         ORDER BY seq ASC \
         LIMIT $2",
    )
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let entries = rows
        .into_iter()
        .map(|(seq, op_id, nonce, ciphertext)| JournalEntryDto {
            seq,
            op_id,
            blob_nonce:      hex::encode(&nonce),
            blob_ciphertext: hex::encode(&ciphertext),
        })
        .collect();

    Ok((StatusCode::OK, Json(entries)))
}

/// GET /epoch — retourne l'époque courante (utile pour monitoring et tests).
async fn handle_get_epoch(
    State(state): State<Arc<AppState>>,
) -> ApiResult<EpochResponse> {
    let (epoch, host): (i64, String) = sqlx::query_as(
        "SELECT epoch, primary_host FROM node_epoch WHERE id = 1",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::OK, Json(EpochResponse { epoch, primary_host: host })))
}

/// POST /epoch/promote — simule la promotion Patroni pour les tests du spike.
///
/// En production, cette opération est déclenchée par le `promote_command`
/// de Patroni sur le nouveau primary — pas par l'API HTTP.
async fn handle_promote_epoch(
    State(state): State<Arc<AppState>>,
) -> ApiResult<EpochResponse> {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());

    let new_epoch = crate::failover::promote_epoch(&state.pool, &hostname)
        .await
        .map_err(internal_error)?;

    // Mettre à jour l'époque locale du nœud qui vient d'être promu
    state.epoch_guard.update_epoch(new_epoch);

    tracing::info!(epoch = new_epoch, host = %hostname, "nœud promu primary — époque incrémentée");

    Ok((
        StatusCode::OK,
        Json(EpochResponse { epoch: new_epoch, primary_host: hostname }),
    ))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse_op_type(s: &str) -> Option<OpType> {
    match s {
        "sale"         => Some(OpType::Sale),
        "stock_adjust" => Some(OpType::StockAdjust),
        _              => None,
    }
}

fn internal_error<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    tracing::error!("erreur interne : {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: "erreur interne du serveur".to_string() }),
    )
}

fn bad_request(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg }))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_op_type_reconnait_les_types_valides() {
        assert!(matches!(parse_op_type("sale"), Some(OpType::Sale)));
        assert!(matches!(parse_op_type("stock_adjust"), Some(OpType::StockAdjust)));
        assert!(parse_op_type("delete").is_none());
        assert!(parse_op_type("").is_none());
    }

    #[test]
    fn write_request_deserialization() {
        let json = r#"{"op_type":"sale","item_id":"PROD-001","quantity":3}"#;
        let req: WriteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.op_type, "sale");
        assert_eq!(req.quantity, 3);
        assert!(req.op_id.is_none());
    }

    #[test]
    fn write_request_avec_op_id_fourni() {
        let id = Uuid::new_v4();
        let json = format!(
            r#"{{"op_type":"sale","item_id":"X","quantity":1,"op_id":"{id}"}}"#
        );
        let req: WriteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.op_id, Some(id));
    }
}
