//! Gestion du parc de machines — endpoints HTTP du nœud actif.
//!
//! Rend démontrables EN LIVE les critères §7.4 du cadrage Phase 0 :
//!   #8  POST /devices/enroll   — enrôle un appareil sans jamais exposer la clé au relais
//!   #9  POST /devices/revoke   — dé-enrôle + ROTATION de DEK (re-scelle pour les restants)
//!   #10 GET  /devices          — liste + alerte quorum (failover auto sûr si ≥ 3 machines)
//!   #11 POST /recovery/setup|restore — code de récupération (Argon2id)
//!
//! Persistance : PostgreSQL local (tables enrolled_devices, dek_state, recovery_blob).
//! La DEK opérationnelle (écritures du journal) est mutable : la rotation la remplace
//! réellement → un appareil dé-enrôlé ne peut plus déchiffrer les écritures suivantes.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sovereign_core::{
    recovery_unwrap, recovery_wrap, wrap_dek_hex, Dek, RecoveryBlob,
};

use crate::active::AppState;

// Seuil de quorum pour un failover automatique sûr (cadrage §4.5 : ≥ 3 machines).
const QUORUM_AUTO_FAILOVER: usize = 3;

type DevResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrBody>)>;

#[derive(Serialize)]
pub struct ErrBody {
    pub error: String,
}

fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrBody>) {
    (code, Json(ErrBody { error: msg.into() }))
}
fn ise<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrBody>) {
    tracing::error!("erreur parc : {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne du serveur")
}

// ── DEK opérationnelle : chargement / rotation persistés ───────────────────────

/// Au démarrage : charge la DEK de génération la plus haute. Si la table est vide,
/// l'amorce avec la DEK fournie (env SOVEREIGN_DEK_HEX) en génération 1.
pub async fn load_or_seed_dek(pool: &sqlx::PgPool, seed: Dek) -> anyhow::Result<(Dek, i64)> {
    let latest: Option<(i64, String)> =
        sqlx::query_as("SELECT generation, dek_hex FROM dek_state ORDER BY generation DESC LIMIT 1")
            .fetch_optional(pool)
            .await?;

    if let Some((gen, dek_hex)) = latest {
        let bytes = hex::decode(dek_hex.trim())?;
        let dek = Dek::from_bytes(&bytes)
            .ok_or_else(|| anyhow::anyhow!("dek_state génération {gen} : DEK invalide"))?;
        Ok((dek, gen))
    } else {
        sqlx::query("INSERT INTO dek_state (generation, dek_hex) VALUES (1, $1)")
            .bind(hex::encode(seed.as_bytes()))
            .execute(pool)
            .await?;
        Ok((seed, 1))
    }
}

// ── Enrôlement (#8) ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EnrollRequest {
    /// Clé publique X25519 (hex, 32 octets) présentée par le nouvel appareil.
    pub public_key: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Serialize)]
pub struct EnrollResponse {
    pub device_id:  Uuid,
    /// DEK courante scellée pour cet appareil (sealed box hex, opaque).
    pub sealed_dek: String,
    pub generation: i64,
}

/// POST /devices/enroll — emballe la DEK courante pour la clé publique fournie.
pub async fn enroll(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<EnrollRequest>,
) -> DevResult<EnrollResponse> {
    let dek = { state.dek.read().map_err(ise)?.clone() };
    let sealed_dek = wrap_dek_hex(&dek, &req.public_key)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "clé publique invalide (hex 32 octets attendu)"))?;

    let pubkey_bytes = hex::decode(req.public_key.trim())
        .map_err(|_| err(StatusCode::BAD_REQUEST, "clé publique non hex"))?;
    let sealed_bytes = hex::decode(&sealed_dek).map_err(ise)?;

    let device_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO enrolled_devices (device_id, label, public_key, sealed_dek) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(device_id)
    .bind(&req.label)
    .bind(&pubkey_bytes)
    .bind(&sealed_bytes)
    .execute(&state.pool)
    .await
    .map_err(ise)?;

    let generation = current_generation(&state.pool).await.map_err(ise)?;
    tracing::info!(%device_id, label = %req.label, "appareil enrôlé (DEK scellée)");

    Ok((StatusCode::CREATED, Json(EnrollResponse { device_id, sealed_dek, generation })))
}

// ── Liste + quorum (#10) ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DeviceDto {
    pub device_id:   Uuid,
    pub label:       String,
    /// sealed box hex de la DEK courante (re-scellée à chaque rotation) — opaque.
    pub sealed_dek:  String,
    pub enrolled_at: String,
}

#[derive(Serialize)]
pub struct DeviceList {
    pub devices:            Vec<DeviceDto>,
    pub count:              usize,
    pub generation:         i64,
    /// Failover automatique par quorum sûr seulement si ≥ 3 machines (§4.5).
    pub auto_failover_safe: bool,
}

/// GET /devices — liste des appareils enrôlés + état du quorum.
pub async fn list(State(state): State<Arc<AppState>>) -> DevResult<DeviceList> {
    // enrolled_at casté en texte (to_char ISO) pour éviter une dépendance chrono dans ce crate.
    let rows: Vec<(Uuid, String, Vec<u8>, String)> = sqlx::query_as(
        "SELECT device_id, label, sealed_dek, to_char(enrolled_at, 'YYYY-MM-DD\"T\"HH24:MI:SSOF') \
         FROM enrolled_devices ORDER BY enrolled_at ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ise)?;

    let devices: Vec<DeviceDto> = rows
        .into_iter()
        .map(|(device_id, label, sealed, enrolled_at)| DeviceDto {
            device_id,
            label,
            sealed_dek: hex::encode(&sealed),
            enrolled_at,
        })
        .collect();

    let count = devices.len();
    let generation = current_generation(&state.pool).await.map_err(ise)?;
    Ok((
        StatusCode::OK,
        Json(DeviceList {
            devices,
            count,
            generation,
            auto_failover_safe: count >= QUORUM_AUTO_FAILOVER,
        }),
    ))
}

// ── Révocation + rotation de DEK (#9, #10) ─────────────────────────────────────

#[derive(Deserialize)]
pub struct RevokeRequest {
    pub device_id: Uuid,
}

#[derive(Serialize)]
pub struct RevokeResponse {
    pub revoked:            Uuid,
    pub new_generation:     i64,
    pub remaining:          usize,
    pub auto_failover_safe: bool,
    pub quorum_warning:     Option<String>,
}

/// POST /devices/revoke — retire l'appareil PUIS effectue une rotation de DEK :
/// génère une nouvelle DEK, la re-scelle pour les SEULS appareils restants, et
/// remplace la DEK opérationnelle. L'appareil retiré garde l'ancienne DEK et ne
/// peut donc plus déchiffrer les écritures postérieures (critère #9).
pub async fn revoke(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<RevokeRequest>,
) -> DevResult<RevokeResponse> {
    // 1. Retrait
    let deleted = sqlx::query("DELETE FROM enrolled_devices WHERE device_id = $1")
        .bind(req.device_id)
        .execute(&state.pool)
        .await
        .map_err(ise)?
        .rows_affected();
    if deleted == 0 {
        return Err(err(StatusCode::NOT_FOUND, format!("appareil inconnu : {}", req.device_id)));
    }

    // 2. Rotation : nouvelle DEK + nouvelle génération
    let new_dek = Dek::generate();
    let new_gen = current_generation(&state.pool).await.map_err(ise)? + 1;
    sqlx::query("INSERT INTO dek_state (generation, dek_hex) VALUES ($1, $2)")
        .bind(new_gen)
        .bind(hex::encode(new_dek.as_bytes()))
        .execute(&state.pool)
        .await
        .map_err(ise)?;

    // 3. Re-sceller pour les appareils restants uniquement
    let remaining_devices: Vec<(Uuid, Vec<u8>)> =
        sqlx::query_as("SELECT device_id, public_key FROM enrolled_devices")
            .fetch_all(&state.pool)
            .await
            .map_err(ise)?;
    for (id, pubkey) in &remaining_devices {
        let sealed = wrap_dek_hex(&new_dek, &hex::encode(pubkey)).ok_or_else(ise_static)?;
        let sealed_bytes = hex::decode(&sealed).map_err(ise)?;
        sqlx::query("UPDATE enrolled_devices SET sealed_dek = $1 WHERE device_id = $2")
            .bind(&sealed_bytes)
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(ise)?;
    }

    // 4. Remplacer la DEK opérationnelle (les écritures suivantes l'utilisent)
    {
        let mut guard = state.dek.write().map_err(ise)?;
        *guard = new_dek;
    }

    let remaining = remaining_devices.len();
    let safe = remaining >= QUORUM_AUTO_FAILOVER;
    let quorum_warning = if !safe {
        Some(format!(
            "Quorum insuffisant : {remaining} machine(s) enrôlée(s). Le failover automatique \
             sûr exige au moins {QUORUM_AUTO_FAILOVER} machines — repassez en bascule manuelle."
        ))
    } else {
        None
    };

    tracing::warn!(
        revoked = %req.device_id, new_gen, remaining,
        "appareil dé-enrôlé + DEK tournée (re-scellée pour les restants)"
    );

    Ok((
        StatusCode::OK,
        Json(RevokeResponse {
            revoked: req.device_id,
            new_generation: new_gen,
            remaining,
            auto_failover_safe: safe,
            quorum_warning,
        }),
    ))
}

// ── Code de récupération (#11) ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RecoveryRequest {
    pub passphrase: String,
}

#[derive(Serialize)]
pub struct RecoverySetupResponse {
    pub status: &'static str,
}

/// POST /recovery/setup — emballe la DEK courante sous une passphrase (à imprimer).
pub async fn recovery_setup(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<RecoveryRequest>,
) -> DevResult<RecoverySetupResponse> {
    if req.passphrase.trim().len() < 8 {
        return Err(err(StatusCode::BAD_REQUEST, "passphrase trop courte (8 caractères minimum)"));
    }
    let dek = { state.dek.read().map_err(ise)?.clone() };
    let blob = recovery_wrap(&dek, req.passphrase.as_bytes()).map_err(ise)?;

    sqlx::query(
        "INSERT INTO recovery_blob (id, salt, wrapped_nonce, wrapped_ct) VALUES (1, $1, $2, $3) \
         ON CONFLICT (id) DO UPDATE SET salt = $1, wrapped_nonce = $2, wrapped_ct = $3, created_at = NOW()",
    )
    .bind(&blob.salt)
    .bind(&blob.nonce)
    .bind(&blob.ciphertext)
    .execute(&state.pool)
    .await
    .map_err(ise)?;

    tracing::info!("code de récupération configuré (DEK courante emballée sous passphrase)");
    Ok((StatusCode::OK, Json(RecoverySetupResponse { status: "configured" })))
}

#[derive(Serialize)]
pub struct RecoveryRestoreResponse {
    /// DEK restaurée (hex). Exposée pour PROUVER la restauration dans le spike.
    pub dek_hex: String,
    pub matches_current: bool,
}

/// POST /recovery/restore — restaure la DEK depuis la passphrase (preuve critère #11).
pub async fn recovery_restore(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<RecoveryRequest>,
) -> DevResult<RecoveryRestoreResponse> {
    let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT salt, wrapped_nonce, wrapped_ct FROM recovery_blob WHERE id = 1")
            .fetch_optional(&state.pool)
            .await
            .map_err(ise)?;
    let Some((salt, nonce, ciphertext)) = row else {
        return Err(err(StatusCode::NOT_FOUND, "aucun code de récupération configuré"));
    };

    let dek = recovery_unwrap(&RecoveryBlob { salt, nonce, ciphertext }, req.passphrase.as_bytes())
        .map_err(|_| err(StatusCode::UNAUTHORIZED, "passphrase incorrecte"))?;

    let dek_hex = hex::encode(dek.as_bytes());
    let current_hex = { hex::encode(state.dek.read().map_err(ise)?.as_bytes()) };
    Ok((
        StatusCode::OK,
        Json(RecoveryRestoreResponse {
            matches_current: dek_hex == current_hex,
            dek_hex,
        }),
    ))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn current_generation(pool: &sqlx::PgPool) -> Result<i64, sqlx::Error> {
    let g: Option<i64> = sqlx::query_scalar("SELECT MAX(generation) FROM dek_state")
        .fetch_one(pool)
        .await?;
    Ok(g.unwrap_or(1))
}

fn ise_static() -> (StatusCode, Json<ErrBody>) {
    err(StatusCode::INTERNAL_SERVER_ERROR, "re-scellement DEK échoué")
}
