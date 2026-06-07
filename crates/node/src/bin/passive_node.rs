//! Nœud passif souverain — réplica SQLite lecture seule.
//!
//! Rôle : synchroniser les entrées chiffrées du journal depuis le nœud actif,
//! les déchiffrer localement avec la DEK, et maintenir un réplica SQLite
//! disponible pour les lectures hors-ligne.
//!
//! Variables d'environnement :
//!   ACTIVE_NODE_URL     — URL du nœud actif (défaut : http://127.0.0.1:3000)
//!   PASSIVE_DB_PATH     — chemin du fichier SQLite (défaut : passive.db)
//!   SOVEREIGN_DEK_HEX   — DEK hex-encodée (requise)
//!   SYNC_INTERVAL_SECS  — intervalle de polling en secondes (défaut : 5)
//!   PASSIVE_LISTEN_ADDR — adresse d'écoute HTTP (défaut : 0.0.0.0:3001)

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::time;

use sovereign_core::{
    crypto::{decrypt, EncryptedBlob},
    journal::{decode_cbor, OpType},
    Dek,
};

// ── Structures de réponse du nœud actif ──────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct JournalEntryDto {
    seq:             i64,
    blob_nonce:      String,
    blob_ciphertext: String,
}

// ── État partagé ──────────────────────────────────────────────────────────────

struct PassiveState {
    pool:            SqlitePool,
    dek:             Dek,
    active_node_url: String,
}

// ── DTOs locaux ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct StockResponse {
    item_id:  String,
    quantity: i64,
}

#[derive(Debug, Serialize)]
struct SyncStatus {
    last_seq:      i64,
    active_node:   String,
    role:          &'static str,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_passive=info".parse().unwrap()),
        )
        .init();

    sovereign_core::crypto::init().expect("libsodium init");

    let db_path = std::env::var("PASSIVE_DB_PATH")
        .unwrap_or_else(|_| "passive.db".to_string());
    let active_node_url = std::env::var("ACTIVE_NODE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
    let sync_interval = std::env::var("SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5);
    let listen_addr = std::env::var("PASSIVE_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3001".to_string());

    // DEK requise pour le passif (doit avoir été enrôlé)
    let dek_hex = std::env::var("SOVEREIGN_DEK_HEX")
        .map_err(|_| anyhow::anyhow!("SOVEREIGN_DEK_HEX est requis pour le nœud passif"))?;
    let dek_bytes = hex::decode(dek_hex.trim())
        .map_err(|e| anyhow::anyhow!("DEK hex invalide : {e}"))?;
    let dek = Dek::from_bytes(&dek_bytes)
        .ok_or_else(|| anyhow::anyhow!("DEK invalide (taille incorrecte)"))?;

    // ── Connexion SQLite ──────────────────────────────────────────────────────
    let db_url = format!("sqlite:{db_path}?mode=rwc");
    tracing::info!("ouverture SQLite : {db_url}");
    let pool = SqlitePool::connect(&db_url).await?;

    // Migrations SQLite inline (évite sqlx-cli pour le spike)
    apply_passive_schema(&pool).await?;
    tracing::info!("schéma SQLite OK");

    let state = Arc::new(PassiveState {
        pool: pool.clone(),
        dek,
        active_node_url: active_node_url.clone(),
    });

    // ── Boucle de synchronisation en arrière-plan ─────────────────────────────
    {
        let state_sync = Arc::clone(&state);
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(sync_interval));
            loop {
                interval.tick().await;
                if let Err(e) = sync_once(&state_sync).await {
                    tracing::warn!("sync échouée : {e}");
                }
            }
        });
    }

    // ── Serveur HTTP lecture seule ────────────────────────────────────────────
    let app = Router::new()
        .route("/health",         get(health))
        .route("/stock/:item_id", get(handle_get_stock))
        .route("/sync/status",    get(handle_sync_status))
        .with_state(state);

    tracing::info!("nœud passif souverain en écoute sur {listen_addr}");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Synchronisation ───────────────────────────────────────────────────────────

/// Une passe de synchronisation : récupère les nouvelles entrées du journal
/// depuis le nœud actif, les déchiffre, et les applique au réplica SQLite.
async fn sync_once(state: &PassiveState) -> anyhow::Result<()> {
    let last_seq: i64 = sqlx::query_scalar("SELECT last_seq FROM sync_state WHERE id = 1")
        .fetch_one(&state.pool)
        .await?;

    let url = format!(
        "{}/journal?after_seq={}&limit=100",
        state.active_node_url, last_seq
    );

    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        anyhow::bail!("nœud actif HTTP {} sur GET /journal", resp.status());
    }

    let entries: Vec<JournalEntryDto> = resp.json().await?;
    if entries.is_empty() {
        return Ok(());
    }

    let count = entries.len();

    // Appliquer chaque entrée dans une transaction SQLite
    let mut tx = state.pool.begin().await?;

    for entry in entries {
        apply_entry(&mut tx, &entry, &state.dek).await?;

        // Avancer le pointeur de sync
        sqlx::query("UPDATE sync_state SET last_seq = $1 WHERE id = 1 AND last_seq < $1")
            .bind(entry.seq)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    tracing::info!(count, last_seq_now = last_seq + count as i64, "sync OK");
    Ok(())
}

/// Déchiffre une entrée de journal et applique l'opération au réplica SQLite.
async fn apply_entry(
    tx:    &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry: &JournalEntryDto,
    dek:   &Dek,
) -> anyhow::Result<()> {
    // Décoder le blob hex → EncryptedBlob
    let nonce_bytes = hex::decode(&entry.blob_nonce)
        .map_err(|e| anyhow::anyhow!("nonce hex invalide (seq={}) : {e}", entry.seq))?;
    let ciphertext = hex::decode(&entry.blob_ciphertext)
        .map_err(|e| anyhow::anyhow!("ciphertext hex invalide (seq={}) : {e}", entry.seq))?;

    if nonce_bytes.len() != 24 {
        anyhow::bail!("nonce invalide : {} octets (attendu 24)", nonce_bytes.len());
    }
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&nonce_bytes);

    let blob = EncryptedBlob { nonce, ciphertext };

    // Déchiffrement local avec la DEK
    let cbor = decrypt(&blob, dek)
        .map_err(|e| anyhow::anyhow!("déchiffrement échoué (seq={}) : {e}", entry.seq))?;

    // Désérialisation CBOR → opération
    let op = decode_cbor(&cbor)
        .map_err(|e| anyhow::anyhow!("CBOR invalide (seq={}) : {e}", entry.seq))?;

    // Application de l'opération au stock SQLite
    match op.op_type {
        OpType::Sale => {
            sqlx::query(
                "INSERT INTO stock_replica (item_id, quantity) VALUES (?1, -?2)
                 ON CONFLICT (item_id) DO UPDATE SET quantity = stock_replica.quantity - ?2",
            )
            .bind(&op.payload.item_id)
            .bind(op.payload.quantity)
            .execute(&mut **tx)
            .await?;
        }
        OpType::StockAdjust => {
            sqlx::query(
                "INSERT INTO stock_replica (item_id, quantity) VALUES (?1, ?2)
                 ON CONFLICT (item_id) DO UPDATE SET quantity = stock_replica.quantity + ?2",
            )
            .bind(&op.payload.item_id)
            .bind(op.payload.quantity)
            .execute(&mut **tx)
            .await?;
        }
        // Opérations CRUD métier (produits / clients) : ce réplica SQLite ne reconstruit
        // que le stock. Dans le cluster réel, le métier (schéma `business`) est répliqué
        // par la réplication WAL PostgreSQL vers le standby — pas par ce chemin.
        OpType::ProduitUpsert | OpType::ProduitDelete
        | OpType::ClientUpsert | OpType::ClientDelete => {}
    }

    Ok(())
}

/// Crée le schéma SQLite si nécessaire (inline pour le spike).
async fn apply_passive_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stock_replica (
             item_id  TEXT    PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 0
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sync_state (
             id       INTEGER PRIMARY KEY CHECK (id = 1),
             last_seq INTEGER NOT NULL DEFAULT 0
         )",
    )
    .execute(pool)
    .await?;

    sqlx::query("INSERT OR IGNORE INTO sync_state (id, last_seq) VALUES (1, 0)")
        .execute(pool)
        .await?;

    Ok(())
}

// ── Handlers HTTP ─────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok (passif)"
}

async fn handle_get_stock(
    State(state): State<Arc<PassiveState>>,
    Path(item_id): Path<String>,
) -> Result<Json<StockResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT quantity FROM stock_replica WHERE item_id = ?1")
            .bind(&item_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e.to_string() }),
                )
            })?;

    let quantity = row.map(|(q,)| q).unwrap_or(0);
    Ok(Json(StockResponse { item_id, quantity }))
}

async fn handle_sync_status(
    State(state): State<Arc<PassiveState>>,
) -> Result<Json<SyncStatus>, (StatusCode, Json<ErrorResponse>)> {
    let last_seq: i64 = sqlx::query_scalar("SELECT last_seq FROM sync_state WHERE id = 1")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e.to_string() }),
            )
        })?;

    Ok(Json(SyncStatus {
        last_seq,
        active_node: state.active_node_url.clone(),
        role: "passive",
    }))
}
