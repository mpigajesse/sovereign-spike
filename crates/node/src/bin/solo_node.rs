//! Nœud SOLO souverain — tout-en-un sur SQLite, sans PostgreSQL.
//!
//! Cible : TPE / PME mono-poste. Une seule machine, aucune réplication.
//!
//! ── DÉCISION D'ARCHITECTURE ────────────────────────────────────────────────
//! Contrairement au nœud actif (PostgreSQL), le mode solo place le métier (stock)
//! ET le journal chiffré dans UNE SEULE base SQLite. L'atomicité est donc
//! préservée — c'est le même principe que l'actif (mono-base), simplement avec
//! SQLite au lieu de PostgreSQL. Aucun problème de transaction inter-bases car
//! il n'y a qu'une base. Pas de fencing nécessaire : une seule machine ne peut
//! pas créer de split-brain.
//!
//! Expose la MÊME API HTTP que le nœud actif → le frontend Tauri fonctionne
//! à l'identique, qu'il parle à un actif PostgreSQL ou à un solo SQLite.
//!
//! Variables d'environnement :
//!   SOLO_DB_PATH        — chemin SQLite (défaut : sovereign_solo.db)
//!   SOVEREIGN_DEK_HEX   — DEK hex (générée + loguée si absente)
//!   LISTEN_ADDR         — adresse d'écoute (défaut : 0.0.0.0:3000)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use sovereign_core::{
    encrypt,
    journal::{encode_cbor, Operation, OpType, Payload},
    Dek,
};

// ── État ──────────────────────────────────────────────────────────────────────

struct AppState {
    pool: SqlitePool,
    dek:  Dek,
}

// ── DTOs (identiques au nœud actif pour compat frontend) ──────────────────────

#[derive(Debug, Deserialize)]
struct WriteRequest {
    op_type:  String,
    item_id:  String,
    quantity: i64,
    op_id:    Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct WriteResponse {
    seq:    i64,
    op_id:  Uuid,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct StockResponse {
    item_id:  String,
    quantity: i64,
}

#[derive(Debug, Serialize)]
struct JournalEntryDto {
    seq:             i64,
    op_id:           Uuid,
    blob_nonce:      String,
    blob_ciphertext: String,
}

#[derive(Debug, Serialize)]
struct EpochResponse {
    epoch:        i64,
    primary_host: String,
}

#[derive(Debug, Deserialize)]
struct JournalQuery {
    after_seq: Option<i64>,
    limit:     Option<i64>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

type ApiResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrorResponse>)>;

// ── Point d'entrée ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_solo=info".parse().unwrap()),
        )
        .init();

    sovereign_core::crypto::init().expect("libsodium init");

    let db_path = std::env::var("SOLO_DB_PATH")
        .unwrap_or_else(|_| "sovereign_solo.db".to_string());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    // ── DEK ───────────────────────────────────────────────────────────────────
    let dek = match std::env::var("SOVEREIGN_DEK_HEX") {
        Ok(hex_str) => {
            let bytes = hex::decode(hex_str.trim())
                .map_err(|e| anyhow::anyhow!("SOVEREIGN_DEK_HEX invalide : {e}"))?;
            Dek::from_bytes(&bytes)
                .ok_or_else(|| anyhow::anyhow!("DEK invalide (taille)"))?
        }
        Err(_) => {
            let dek = Dek::generate();
            tracing::warn!(
                dek_hex = %hex::encode(dek.as_bytes()),
                "SOVEREIGN_DEK_HEX non défini — DEK éphémère. Définir pour persistance."
            );
            dek
        }
    };

    // ── SQLite (WAL + busy_timeout pour robustesse mono-poste) ─────────────────
    let db_url = format!("sqlite:{db_path}?mode=rwc");
    tracing::info!("ouverture SQLite solo : {db_url}");
    let pool = SqlitePool::connect(&db_url).await?;

    sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
    sqlx::query("PRAGMA busy_timeout=5000").execute(&pool).await?;
    apply_schema(&pool).await?;
    tracing::info!("schéma SQLite solo OK");

    let state = Arc::new(AppState { pool, dek });

    let app = Router::new()
        .route("/health",         get(health))
        .route("/write",          post(handle_write))
        .route("/stock/:item_id", get(handle_get_stock))
        .route("/journal",        get(handle_get_journal))
        .route("/epoch",          get(handle_get_epoch))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);

    tracing::info!("nœud SOLO souverain (SQLite) en écoute sur {listen_addr}");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Schéma ────────────────────────────────────────────────────────────────────

async fn apply_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock (
             item_id  TEXT    PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 0 CHECK (quantity >= 0)
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS operations_journal (
             seq             INTEGER PRIMARY KEY,
             op_id           TEXT    NOT NULL UNIQUE,
             blob_nonce      TEXT    NOT NULL,
             blob_ciphertext TEXT    NOT NULL
         )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok (solo)"
}

/// POST /write — sérialise une écriture métier dans une transaction SQLite unique.
///
/// SQLite verrouille la base en écriture (un seul writer) → la vérification de
/// stock et l'écriture du journal sont atomiques. Même garantie que l'actif PG.
async fn handle_write(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<WriteRequest>,
) -> ApiResult<WriteResponse> {
    let op_type = match req.op_type.as_str() {
        "sale"         => OpType::Sale,
        "stock_adjust" => OpType::StockAdjust,
        other => return Err(bad_request(format!("op_type inconnu : '{other}'"))),
    };

    if matches!(op_type, OpType::Sale) && req.quantity <= 0 {
        return Err(bad_request("quantity doit être > 0 pour une vente".into()));
    }

    let op_id = req.op_id.unwrap_or_else(Uuid::new_v4);

    // Transaction SQLite (write-lock global → sérialisation des écritures)
    let mut tx = state.pool.begin().await.map_err(internal)?;

    // 1. Idempotence
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT seq FROM operations_journal WHERE op_id = ?1")
            .bind(op_id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;

    if let Some((seq,)) = existing {
        tx.rollback().await.map_err(internal)?;
        return Ok((StatusCode::OK, Json(WriteResponse { seq, op_id, status: "already_processed" })));
    }

    // 2. Anti-survente
    if matches!(op_type, OpType::Sale) {
        let dispo: Option<(i64,)> =
            sqlx::query_as("SELECT quantity FROM stock WHERE item_id = ?1")
                .bind(&req.item_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(internal)?;

        let dispo = dispo.map(|(q,)| q).unwrap_or(0);
        if dispo < req.quantity {
            tx.rollback().await.map_err(internal)?;
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!("stock insuffisant : disponible={dispo}, demandé={}", req.quantity),
                }),
            ));
        }
    }

    // 3. Séquence (MAX+1 dans la transaction — sûr car write-lock)
    let (seq,): (i64,) =
        sqlx::query_as("SELECT COALESCE(MAX(seq), 0) + 1 FROM operations_journal")
            .fetch_one(&mut *tx)
            .await
            .map_err(internal)?;

    // 4. Construction + chiffrement CBOR
    let op = Operation::new(
        seq as u64,
        op_type.clone(),
        Payload { item_id: req.item_id.clone(), quantity: req.quantity },
    );
    let cbor = encode_cbor(&op).map_err(|e| internal(format!("CBOR : {e}")))?;
    let blob = encrypt(&cbor, &state.dek);

    // 5. Mise à jour du stock
    match op_type {
        OpType::Sale => {
            sqlx::query("UPDATE stock SET quantity = quantity - ?1 WHERE item_id = ?2")
                .bind(req.quantity)
                .bind(&req.item_id)
                .execute(&mut *tx)
                .await
                .map_err(internal)?;
        }
        OpType::StockAdjust => {
            sqlx::query(
                "INSERT INTO stock (item_id, quantity) VALUES (?1, MAX(0, ?2))
                 ON CONFLICT (item_id) DO UPDATE SET quantity = MAX(0, stock.quantity + ?2)",
            )
            .bind(&req.item_id)
            .bind(req.quantity)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
        }
        // Le mode solo ne gère que le stock (CRUD métier hors-périmètre solo).
        _ => return Err(bad_request("opération non supportée en mode solo".into())),
    }

    // 6. Journal chiffré
    sqlx::query(
        "INSERT INTO operations_journal (seq, op_id, blob_nonce, blob_ciphertext)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(seq)
    .bind(op_id.to_string())
    .bind(hex::encode(blob.nonce))
    .bind(hex::encode(&blob.ciphertext))
    .execute(&mut *tx)
    .await
    .map_err(internal)?;

    tx.commit().await.map_err(internal)?;

    tracing::info!(seq, %op_id, item_id = %req.item_id, "écriture solo sérialisée");
    Ok((StatusCode::CREATED, Json(WriteResponse { seq, op_id, status: "committed" })))
}

async fn handle_get_stock(
    State(state):  State<Arc<AppState>>,
    Path(item_id): Path<String>,
) -> ApiResult<StockResponse> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT quantity FROM stock WHERE item_id = ?1")
            .bind(&item_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(internal)?;

    let quantity = row.map(|(q,)| q).unwrap_or(0);
    Ok((StatusCode::OK, Json(StockResponse { item_id, quantity })))
}

async fn handle_get_journal(
    State(state):  State<Arc<AppState>>,
    Query(params): Query<JournalQuery>,
) -> ApiResult<Vec<JournalEntryDto>> {
    let after_seq = params.after_seq.unwrap_or(0);
    let limit     = params.limit.unwrap_or(100).min(1000);

    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT seq, op_id, blob_nonce, blob_ciphertext
         FROM operations_journal WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2",
    )
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;

    let entries = rows
        .into_iter()
        .filter_map(|(seq, op_id, nonce, ct)| {
            Uuid::parse_str(&op_id).ok().map(|op_id| JournalEntryDto {
                seq, op_id, blob_nonce: nonce, blob_ciphertext: ct,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(entries)))
}

/// GET /epoch — en mode solo l'époque est toujours 1 (une seule machine).
async fn handle_get_epoch() -> ApiResult<EpochResponse> {
    Ok((StatusCode::OK, Json(EpochResponse { epoch: 1, primary_host: "solo".into() })))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    tracing::error!("erreur interne solo : {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "erreur interne".into() }))
}

fn bad_request(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg }))
}
