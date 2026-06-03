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

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

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
    pub seq:             i64,
    pub blob_nonce:      String,       // hex, 24 octets — opaque
    pub blob_ciphertext: String,       // hex, longueur variable — opaque
    pub received_at:     DateTime<Utc>,
}

/// Corps de la requête POST /blobs (envoyé par le nœud actif).
#[derive(Debug, Deserialize)]
pub struct PushBlobRequest {
    pub seq:             i64,
    pub blob_nonce:      String,
    pub blob_ciphertext: String,
}

/// Paramètres de pagination pour GET /blobs.
#[derive(Debug, Deserialize)]
pub struct FetchQuery {
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
pub struct HealthResponse {
    pub role:       &'static str,
    pub blob_count: usize,
    pub status:     &'static str,
}

// ── État partagé ──────────────────────────────────────────────────────────────

pub struct RelayState {
    /// Store en mémoire pour le spike (BTreeMap ordonne par seq automatiquement).
    blobs:     Mutex<BTreeMap<i64, BlobRecord>>,
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
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_health(State(state): State<SharedState>) -> Json<HealthResponse> {
    let count = state.blobs.lock().unwrap().len();
    Json(HealthResponse {
        role:       "relay-aveugle",
        blob_count: count,
        status:     "ok",
    })
}

/// POST /blobs — reçoit un blob chiffré du nœud actif et le stocke.
///
/// Le relais ne valide pas le contenu — il ne peut pas le comprendre.
/// Il vérifie seulement que le hex est bien formé et que seq est positif.
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

    let record = BlobRecord {
        seq:             req.seq,
        blob_nonce:      req.blob_nonce,
        blob_ciphertext: req.blob_ciphertext,
        received_at:     Utc::now(),
    };

    let mut store = state.blobs.lock().unwrap();

    // Idempotence : même seq déjà présent → OK sans écraser
    if store.contains_key(&req.seq) {
        return Ok((StatusCode::OK, Json(PushResponse { seq: req.seq, status: "already_stored" })));
    }

    if store.len() >= state.max_blobs {
        return Err((
            StatusCode::INSUFFICIENT_STORAGE,
            Json(ErrorResponse {
                error: format!("store plein ({} blobs max)", state.max_blobs),
            }),
        ));
    }

    store.insert(req.seq, record);
    tracing::info!(seq = req.seq, "blob stocké — contenu opaque, non interprété");

    Ok((StatusCode::CREATED, Json(PushResponse { seq: req.seq, status: "stored" })))
}

/// GET /blobs?after_seq={n}&limit={m} — récupère les blobs depuis after_seq.
///
/// Appelé par les nœuds passifs pour synchroniser leur réplica SQLite.
/// Les passifs déchiffrent localement avec leur DEK — le relais ne déchiffre jamais.
async fn handle_fetch_blobs(
    State(state):  State<SharedState>,
    Query(params): Query<FetchQuery>,
) -> ApiResult<Vec<BlobRecord>> {
    let after_seq = params.after_seq.unwrap_or(0);
    let limit     = params.limit.unwrap_or(100).min(1000);

    let store = state.blobs.lock().unwrap();

    let blobs: Vec<BlobRecord> = store
        .range((after_seq + 1)..)
        .take(limit)
        .map(|(_, b)| b.clone())
        .collect();

    Ok((StatusCode::OK, Json(blobs)))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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
        .map_err(|_| anyhow::anyhow!("RELAY_API_KEY requise (secret partagé actif↔relais)"))?;

    let max_blobs = std::env::var("RELAY_MAX_BLOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000usize);

    let listen_addr = std::env::var("RELAY_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4000".to_string());

    let state = Arc::new(RelayState {
        blobs:     Mutex::new(BTreeMap::new()),
        api_key,
        max_blobs,
    });

    let app = router(state);

    tracing::info!(
        addr = %listen_addr,
        max_blobs,
        "relais souverain aveugle démarré — aucune clé crypto chargée"
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

    fn make_state(api_key: &str) -> SharedState {
        Arc::new(RelayState {
            blobs:     Mutex::new(BTreeMap::new()),
            api_key:   api_key.to_string(),
            max_blobs: 1000,
        })
    }

    #[tokio::test]
    async fn push_et_fetch_blob_roundtrip() {
        let app = router(make_state("test-key"));

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
        let app = router(make_state("secret"));

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
        let app = router(make_state("k"));
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
        let state = make_state("k");
        {
            let mut store = state.blobs.lock().unwrap();
            for seq in 1i64..=5 {
                store.insert(seq, BlobRecord {
                    seq,
                    blob_nonce:      "00".repeat(24),
                    blob_ciphertext: "ff".repeat(32),
                    received_at:     Utc::now(),
                });
            }
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
        let app = router(make_state("k"));
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
