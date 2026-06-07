//! Identité du tenant (la PME) — LOT 1 « création de compte ».
//!
//! Endpoints du nœud actif :
//!   POST /tenant/bootstrap {nom, gerant?, email?}  — crée le compte (1ère fois)
//!   GET  /tenant                                    — identité du tenant courant
//!
//! Création auto-souveraine : le tenant_id est généré LOCALEMENT (aucun serveur central
//! éditeur). Il identifie la PME, estampille les données métier et permet au relais
//! « Amane » de séparer les blobs de plusieurs PME sans jamais déchiffrer.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::active::AppState;

type TenantResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrBody>)>;

#[derive(Serialize)]
pub struct ErrBody {
    pub error: String,
}

fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrBody>) {
    (code, Json(ErrBody { error: msg.into() }))
}
fn ise<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrBody>) {
    tracing::error!("erreur tenant : {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne du serveur")
}

// ── DTOs ───────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BootstrapRequest {
    pub nom: String,
    #[serde(default)]
    pub gerant: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Serialize)]
pub struct TenantDto {
    pub tenant_id: Uuid,
    pub nom:       String,
    pub gerant:    String,
    pub email:     String,
    pub created_at: String,
}

// ── Handlers ───────────────────────────────────────────────────────────────────

/// POST /tenant/bootstrap — crée le compte du tenant (la PME) si aucun n'existe.
/// Idempotent : si le compte existe déjà, renvoie l'identité existante (200) au lieu
/// d'en créer un second (un cluster ne sert qu'UN tenant).
pub async fn bootstrap(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<BootstrapRequest>,
) -> TenantResult<TenantDto> {
    if req.nom.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "le nom de l'entreprise est requis"));
    }

    // Compte déjà créé ? → renvoyer l'existant (idempotence, pas d'écrasement).
    if let Some(existing) = fetch_tenant(&state.pool).await.map_err(ise)? {
        return Ok((StatusCode::OK, Json(existing)));
    }

    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenant (id, tenant_id, nom, gerant, email) VALUES (1, $1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(req.nom.trim())
    .bind(req.gerant.trim())
    .bind(req.email.trim())
    .execute(&state.pool)
    .await
    .map_err(ise)?;

    tracing::info!(%tenant_id, nom = %req.nom.trim(), "compte tenant créé (auto-souverain)");

    let dto = fetch_tenant(&state.pool)
        .await
        .map_err(ise)?
        .ok_or_else(|| ise("tenant introuvable après création"))?;
    Ok((StatusCode::CREATED, Json(dto)))
}

/// GET /tenant — identité du tenant courant (404 si le compte n'est pas encore créé).
pub async fn get(State(state): State<Arc<AppState>>) -> TenantResult<TenantDto> {
    match fetch_tenant(&state.pool).await.map_err(ise)? {
        Some(dto) => Ok((StatusCode::OK, Json(dto))),
        None => Err(err(StatusCode::NOT_FOUND, "aucun compte tenant créé sur ce cluster")),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn fetch_tenant(pool: &sqlx::PgPool) -> Result<Option<TenantDto>, sqlx::Error> {
    // created_at casté en texte ISO (évite une dépendance chrono dans ce crate).
    let row: Option<(Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT tenant_id, nom, gerant, email, \
         to_char(created_at, 'YYYY-MM-DD\"T\"HH24:MI:SSOF') \
         FROM tenant WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(tenant_id, nom, gerant, email, created_at)| TenantDto {
        tenant_id,
        nom,
        gerant,
        email,
        created_at,
    }))
}
