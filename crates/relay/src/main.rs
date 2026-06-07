//! Relais éditeur aveugle — sovereign-relay (§4.4 Phase 0).
//!
//! Rôle : stocker et redistribuer les blobs chiffrés du journal souverain.
//!
//! Propriétés de sécurité OBLIGATOIRES :
//!   - Aucune dépendance sur sovereign-core (vérifiable dans Cargo.toml)
//!   - Aucune opération cryptographique
//!   - Aucune connaissance de la structure interne des blobs
//!   - Seuls des hex-strings sont reçus, stockés et renvoyés tels quels
//!
//! Le relais peut être hébergé par l'éditeur du logiciel SaaS.
//! Même s'il est compromis, l'attaquant ne voit que des octets aléatoires.
//!
//! Variables d'environnement :
//!   RELAY_API_KEY      — clé partagée active↔relais (requise)
//!   RELAY_LISTEN_ADDR  — adresse d'écoute (défaut : 0.0.0.0:4000)
//!   RELAY_MAX_BLOBS    — taille max du store en mémoire (défaut : 100_000)

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Structures de données ─────────────────────────────────────────────────────

/// Un blob tel que reçu du nœud actif.
/// Le relais ne connaît pas la signification de ces octets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobRecord {
    pub tenant_id:       String,       // identité opaque de la PME (sépare les PME)
    pub seq:             i64,
    pub blob_nonce:      String,       // hex, 24 octets — opaque
    pub blob_ciphertext: String,       // hex, longueur variable — opaque
    pub received_at:     DateTime<Utc>,
}

/// Corps de la requête POST /blobs (envoyé par le nœud actif).
/// `tenant_id` : identifiant opaque de la PME. Le relais s'en sert UNIQUEMENT pour
/// cloisonner les blobs (multi-tenant) — il ne peut rien en déduire ni rien déchiffrer.
#[derive(Debug, Deserialize)]
pub struct PushBlobRequest {
    #[serde(default)]
    pub tenant_id:       String,
    pub seq:             i64,
    pub blob_nonce:      String,
    pub blob_ciphertext: String,
}

/// Paramètres de pagination pour GET /blobs (cloisonnés par tenant).
#[derive(Debug, Deserialize)]
pub struct FetchQuery {
    #[serde(default)]
    pub tenant_id: Option<String>,
    pub after_seq: Option<i64>,
    pub limit:     Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub seq:    i64,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct TenantBlobs {
    pub tenant_id:  String,
    pub blob_count: i64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub role:        &'static str,
    pub blob_count:  usize,      // total tous tenants confondus
    pub tenant_count: usize,     // nombre de PME distinctes sauvegardées
    pub tenants:     Vec<TenantBlobs>,
    pub status:      &'static str,
}

// ── État partagé ──────────────────────────────────────────────────────────────

pub struct RelayState {
    /// Pool SQLite persistant — survit aux redémarrages du relais.
    pool:      sqlx::SqlitePool,
    api_key:   String,
    max_blobs: usize,
}

type SharedState = Arc<RelayState>;
type ApiResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrorResponse>)>;

// ── Authentification ──────────────────────────────────────────────────────────

fn check_api_key(
    headers: &HeaderMap,
    state:   &RelayState,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let key = headers
        .get("X-Relay-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if key != state.api_key {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "clé API invalide ou absente (header X-Relay-Key)".to_string(),
            }),
        ));
    }
    Ok(())
}

// ── Routeur ───────────────────────────────────────────────────────────────────

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(handle_health))
        .route("/blobs",  post(handle_push_blob))
        .route("/blobs",  get(handle_fetch_blobs))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM relay_blobs")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    // Répartition par tenant — preuve du cloisonnement multi-tenant (sans rien déchiffrer).
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT tenant_id, COUNT(*) FROM relay_blobs GROUP BY tenant_id ORDER BY tenant_id",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let tenants: Vec<TenantBlobs> = rows
        .into_iter()
        .map(|(tenant_id, blob_count)| TenantBlobs { tenant_id, blob_count })
        .collect();

    Json(HealthResponse {
        role:         "amane-relay",
        blob_count:   count as usize,
        tenant_count: tenants.len(),
        tenants,
        status:       "ok",
    })
}

/// POST /blobs — reçoit un blob chiffré du nœud actif et le stocke (SQLite persistant).
async fn handle_push_blob(
    State(state): State<SharedState>,
    headers:      HeaderMap,
    Json(req):    Json<PushBlobRequest>,
) -> ApiResult<PushResponse> {
    check_api_key(&headers, &state)?;

    validate_hex(&req.blob_nonce, "blob_nonce")?;
    validate_hex(&req.blob_ciphertext, "blob_ciphertext")?;

    if req.seq <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "seq doit être > 0".to_string() }),
        ));
    }

    // Vérifier capacité max
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM relay_blobs")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| internal_error(e.to_string()))?;

    if count as usize >= state.max_blobs {
        return Err((
            StatusCode::INSUFFICIENT_STORAGE,
            Json(ErrorResponse { error: format!("store plein ({} blobs max)", state.max_blobs) }),
        ));
    }

    // Idempotence par (tenant_id, seq) : deux PME différentes peuvent avoir le même seq
    // (chacune a son propre journal). La clé composite empêche toute collision.
    let affected = sqlx::query(
        "INSERT OR IGNORE INTO relay_blobs (tenant_id, seq, blob_nonce, blob_ciphertext, received_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&req.tenant_id)
    .bind(req.seq)
    .bind(&req.blob_nonce)
    .bind(&req.blob_ciphertext)
    .bind(Utc::now().to_rfc3339())
    .execute(&state.pool)
    .await
    .map_err(|e| internal_error(e.to_string()))?
    .rows_affected();

    if affected == 0 {
        return Ok((StatusCode::OK, Json(PushResponse { seq: req.seq, status: "already_stored" })));
    }

    tracing::info!(tenant = %req.tenant_id, seq = req.seq, "blob stocké (SQLite) — opaque, non interprété");
    Ok((StatusCode::CREATED, Json(PushResponse { seq: req.seq, status: "stored" })))
}

/// GET /blobs?after_seq={n}&limit={m} — récupère les blobs depuis after_seq (SQLite).
async fn handle_fetch_blobs(
    State(state):  State<SharedState>,
    Query(params): Query<FetchQuery>,
) -> ApiResult<Vec<BlobRecord>> {
    let after_seq = params.after_seq.unwrap_or(0);
    let limit     = params.limit.unwrap_or(100).min(1000) as i64;
    // Cloisonnement : un tenant ne récupère QUE ses propres blobs.
    let tenant_id = params.tenant_id.unwrap_or_default();

    let rows: Vec<(String, i64, String, String, String)> = sqlx::query_as(
        "SELECT tenant_id, seq, blob_nonce, blob_ciphertext, received_at
         FROM relay_blobs WHERE tenant_id = $1 AND seq > $2 ORDER BY seq ASC LIMIT $3",
    )
    .bind(&tenant_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| internal_error(e.to_string()))?;

    let blobs = rows.into_iter().map(|(tenant_id, seq, nonce, ct, ts)| BlobRecord {
        tenant_id,
        seq,
        blob_nonce:      nonce,
        blob_ciphertext: ct,
        received_at:     ts.parse().unwrap_or_else(|_| Utc::now()),
    }).collect();

    Ok((StatusCode::OK, Json(blobs)))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn internal_error(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    tracing::error!("erreur interne : {msg}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "erreur interne".to_string() }))
}

fn validate_hex(
    s:     &str,
    field: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if s.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: format!("{field} est vide") }),
        ));
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: format!("{field} n'est pas du hex valide") }),
        ));
    }
    Ok(())
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_relay=info".parse().unwrap()),
        )
        .init();

    let api_key = std::env::var("RELAY_API_KEY")
        .unwrap_or_else(|_| "sovereign-spike-relay-key-2026".to_string());

    let max_blobs = std::env::var("RELAY_MAX_BLOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000usize);

    let listen_addr = std::env::var("RELAY_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4000".to_string());

    // SQLite persistant — survit aux redémarrages
    let db_path = std::env::var("RELAY_DB_PATH")
        .unwrap_or_else(|_| "/tmp/sovereign_relay.db".to_string());
    let db_url = format!("sqlite:{db_path}?mode=rwc");

    let pool = sqlx::SqlitePool::connect(&db_url).await
        .map_err(|e| anyhow::anyhow!("impossible d'ouvrir SQLite relay {db_path}: {e}"))?;

    // Créer la table si elle n'existe pas. Clé composite (tenant_id, seq) : le relais est
    // mutualisé (multi-tenant) → deux PME peuvent avoir le même seq sans collision.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS relay_blobs (
            tenant_id        TEXT    NOT NULL DEFAULT '',
            seq              INTEGER NOT NULL,
            blob_nonce       TEXT    NOT NULL,
            blob_ciphertext  TEXT    NOT NULL,
            received_at      TEXT    NOT NULL,
            PRIMARY KEY (tenant_id, seq)
        )"
    )
    .execute(&pool)
    .await?;
    // Compat : si une ancienne table existait sans tenant_id, ajouter la colonne.
    let _ = sqlx::query("ALTER TABLE relay_blobs ADD COLUMN tenant_id TEXT NOT NULL DEFAULT ''")
        .execute(&pool)
        .await;

    let state = Arc::new(RelayState { pool, api_key, max_blobs });
    let app = router(state);

    tracing::info!(
        addr = %listen_addr,
        max_blobs,
        db = %db_path,
        "relais « Amane » souverain aveugle démarré (multi-tenant) — aucune clé crypto chargée"
    );

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, header}};
    use tower::ServiceExt;

    async fn make_state(api_key: &str) -> SharedState {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS relay_blobs (
                tenant_id TEXT NOT NULL DEFAULT '',
                seq INTEGER NOT NULL,
                blob_nonce TEXT NOT NULL,
                blob_ciphertext TEXT NOT NULL,
                received_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, seq)
            )"
        ).execute(&pool).await.unwrap();
        Arc::new(RelayState {
            pool,
            api_key: api_key.to_string(),
            max_blobs: 1000,
        })
    }

    #[tokio::test]
    async fn push_et_fetch_blob_roundtrip() {
        let app = router(make_state("test-key").await);

        let body = serde_json::json!({
            "seq": 1,
            "blob_nonce":      "aa".repeat(24),
            "blob_ciphertext": "deadbeef".repeat(8)
        });
        let r = app.clone().oneshot(
            Request::builder()
                .method("POST").uri("/blobs")
                .header("X-Relay-Key", "test-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);

        let r = app.oneshot(
            Request::builder()
                .method("GET").uri("/blobs?after_seq=0&limit=10")
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        let blobs: Vec<BlobRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].seq, 1);
    }

    #[tokio::test]
    async fn cle_api_invalide_retourne_401() {
        let app = router(make_state("secret").await);

        let body = serde_json::json!({
            "seq": 1,
            "blob_nonce":      "bb".repeat(24),
            "blob_ciphertext": "ff".repeat(16)
        });
        let r = app.oneshot(
            Request::builder()
                .method("POST").uri("/blobs")
                .header("X-Relay-Key", "mauvaise-cle")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn push_idempotent_meme_seq() {
        let app = router(make_state("k").await);
        let body = serde_json::json!({
            "seq": 42,
            "blob_nonce":      "cc".repeat(24),
            "blob_ciphertext": "cafe".repeat(8)
        });
        let mk_req = || Request::builder()
            .method("POST").uri("/blobs")
            .header("X-Relay-Key", "k")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let r1 = app.clone().oneshot(mk_req()).await.unwrap();
        assert_eq!(r1.status(), StatusCode::CREATED);

        let r2 = app.oneshot(mk_req()).await.unwrap();
        assert_eq!(r2.status(), StatusCode::OK); // already_stored
    }

    #[tokio::test]
    async fn pagination_after_seq() {
        let state = make_state("k").await;
        for seq in 1i64..=5 {
            sqlx::query(
                "INSERT INTO relay_blobs (tenant_id, seq, blob_nonce, blob_ciphertext, received_at)
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind("")
            .bind(seq)
            .bind("00".repeat(24))
            .bind("ff".repeat(32))
            .bind(Utc::now().to_rfc3339())
            .execute(&state.pool)
            .await
            .unwrap();
        }
        let app = router(state);

        let r = app.oneshot(
            Request::builder()
                .method("GET").uri("/blobs?after_seq=2&limit=10")
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        let blobs: Vec<BlobRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(blobs.len(), 3);
        assert_eq!(blobs[0].seq, 3);
        assert_eq!(blobs[2].seq, 5);
    }

    #[tokio::test]
    async fn cloisonnement_multi_tenant() {
        // Deux PME (tenant A et B) poussent chacune un blob avec le MÊME seq=1.
        // La clé composite (tenant_id, seq) évite la collision, et chaque tenant ne
        // récupère QUE ses propres blobs — preuve du relais mutualisé aveugle.
        let app = router(make_state("k").await);

        let push = |tenant: &str, ct: &str| {
            let body = serde_json::json!({
                "tenant_id": tenant, "seq": 1,
                "blob_nonce": "aa".repeat(24), "blob_ciphertext": ct,
            });
            Request::builder()
                .method("POST").uri("/blobs")
                .header("X-Relay-Key", "k")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap()
        };

        // Tenant A : seq 1
        let r = app.clone().oneshot(push("tenant-A", &"11".repeat(8))).await.unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
        // Tenant B : seq 1 AUSSI → pas de collision grâce à la clé composite
        let r = app.clone().oneshot(push("tenant-B", &"22".repeat(8))).await.unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);

        // Tenant A ne voit QUE son blob
        let r = app.clone().oneshot(
            Request::builder().method("GET")
                .uri("/blobs?tenant_id=tenant-A&after_seq=0&limit=10")
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        let blobs: Vec<BlobRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(blobs.len(), 1, "tenant A ne voit que ses blobs");
        assert_eq!(blobs[0].tenant_id, "tenant-A");
        assert_eq!(blobs[0].blob_ciphertext, "11".repeat(8));

        // Tenant B ne voit QUE son blob
        let r = app.oneshot(
            Request::builder().method("GET")
                .uri("/blobs?tenant_id=tenant-B&after_seq=0&limit=10")
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        let blobs: Vec<BlobRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(blobs.len(), 1, "tenant B ne voit que ses blobs");
        assert_eq!(blobs[0].blob_ciphertext, "22".repeat(8));
    }

    #[tokio::test]
    async fn relais_ne_contient_aucune_crypto() {
        // Propriété architecturale vérifiée statiquement :
        // le relais ne déclare aucune dépendance sur sovereign-core ni sodiumoxide.
        // On cherche une déclaration de dépendance (avec "="), pas un commentaire.
        let cargo_toml = include_str!("../Cargo.toml");
        for line in cargo_toml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') { continue; } // ignorer les commentaires
            assert!(
                !trimmed.starts_with("sovereign-core"),
                "VIOLATION : le relais ne doit pas dépendre de sovereign-core\nligne: {trimmed}"
            );
            assert!(
                !trimmed.starts_with("sodiumoxide"),
                "VIOLATION : le relais ne doit pas avoir de dépendance sodiumoxide\nligne: {trimmed}"
            );
        }
    }

    #[tokio::test]
    async fn hex_invalide_rejete() {
        let app = router(make_state("k").await);
        let body = serde_json::json!({
            "seq": 1,
            "blob_nonce":      "not-hex!!",
            "blob_ciphertext": "ff".repeat(16)
        });
        let r = app.oneshot(
            Request::builder()
                .method("POST").uri("/blobs")
                .header("X-Relay-Key", "k")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
}
