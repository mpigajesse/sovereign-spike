//! Couche métier — CRUD Produits & Clients (LOT 2), schéma `business`.
//!
//! Chaque écriture métier suit le MÊME chemin souverain que les ventes :
//!   1. fencing (assert_primary) — seul l'actif écrit
//!   2. allocation d'une séquence atomique (journal_seq)
//!   3. opération CRUD générique chiffrée (XChaCha20-Poly1305) → operations_journal
//!   4. application dans le schéma `business` (produits / clients)
//!   5. COMMIT atomique (journal + métier ensemble)
//!   6. push best-effort vers le relais « Amane » (blob opaque)
//!
//! Le CRUD pur n'a pas d'invariant inter-lignes → pas de vérification de stock (≠ vente).
//! Mais il reste journalisé + chiffré + répliqué + sauvegardé au relais : le métier est
//! aussi souverain que le stock.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sovereign_core::{
    encrypt,
    journal::{encode_cbor, BusinessData, Operation, OpType},
};

use crate::active::AppState;

type BizResult<T> = Result<(StatusCode, Json<T>), (StatusCode, Json<ErrBody>)>;

#[derive(Serialize)]
pub struct ErrBody {
    pub error: String,
}
fn err(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrBody>) {
    (code, Json(ErrBody { error: msg.into() }))
}
fn ise<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrBody>) {
    tracing::error!("erreur métier : {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne du serveur")
}

// ── Identité du tenant (requise pour estampiller les données) ──────────────────

async fn current_tenant_id(pool: &sqlx::PgPool) -> Result<Uuid, (StatusCode, Json<ErrBody>)> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT tenant_id FROM tenant WHERE id = 1")
        .fetch_optional(pool)
        .await
        .map_err(ise)?;
    row.map(|(t,)| t)
        .ok_or_else(|| err(StatusCode::PRECONDITION_REQUIRED, "créez d'abord votre compte (aucun tenant)"))
}

// ── Journalisation générique d'une opération métier ────────────────────────────
//
// Alloue la séquence, chiffre l'opération CRUD et l'écrit dans operations_journal,
// le tout DANS la transaction fournie. Retourne (seq, nonce_hex, ct_hex) pour le push.
async fn append_business_op(
    tx:       &mut sqlx::Transaction<'_, sqlx::Postgres>,
    state:    &AppState,
    op_type:  OpType,
    business: BusinessData,
) -> Result<(i64, String, String), (StatusCode, Json<ErrBody>)> {
    // Fencing : seul le primary à l'époque courante peut écrire.
    state.epoch_guard.assert_primary(&mut **tx).await.map_err(|e| {
        tracing::error!("fencing déclenché : {e}");
        err(StatusCode::SERVICE_UNAVAILABLE, format!("nœud dégradé — {e}"))
    })?;

    let (seq,): (i64,) = sqlx::query_as("SELECT nextval('journal_seq')")
        .fetch_one(&mut **tx)
        .await
        .map_err(ise)?;

    let op = Operation::new_business(seq as u64, op_type, business, None);
    let cbor = encode_cbor(&op).map_err(|e| ise(format!("CBOR : {e}")))?;
    let dek = state.dek.read().map_err(ise)?.clone();
    let blob = encrypt(&cbor, &dek);

    sqlx::query(
        "INSERT INTO operations_journal (seq, op_id, blob_nonce, blob_ciphertext) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(seq)
    .bind(op.op_id)
    .bind(blob.nonce.as_slice())
    .bind(blob.ciphertext.as_slice())
    .execute(&mut **tx)
    .await
    .map_err(ise)?;

    Ok((seq, hex::encode(blob.nonce), hex::encode(&blob.ciphertext)))
}

/// Push best-effort du blob vers le relais « Amane » (après commit), cloisonné par tenant.
fn spawn_push(state: &Arc<AppState>, tenant_id: Uuid, seq: i64, nonce_hex: String, ct_hex: String) {
    if let (Some(url), Some(key)) = (&state.relay_url, &state.relay_key) {
        let (url, key, tid) = (url.clone(), key.clone(), tenant_id.to_string());
        tokio::spawn(async move {
            crate::active::push_to_relay(&url, &key, &tid, seq, &nonce_hex, &ct_hex).await;
        });
    }
}

// ═══════════════════════════════════ PRODUITS ═══════════════════════════════════

#[derive(Deserialize)]
pub struct ProduitUpsertReq {
    pub id:         Option<Uuid>, // absent = création ; présent = mise à jour
    pub sku:        String,
    pub nom:        String,
    pub prix_cents: i64,
}

#[derive(Serialize)]
pub struct ProduitDto {
    pub id:         Uuid,
    pub sku:        String,
    pub nom:        String,
    pub prix_cents: i64,
}

/// POST /produits — créer ou mettre à jour un produit (journalisé).
pub async fn upsert_produit(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<ProduitUpsertReq>,
) -> BizResult<ProduitDto> {
    if req.sku.trim().is_empty() || req.nom.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "sku et nom sont requis"));
    }
    if req.prix_cents < 0 {
        return Err(err(StatusCode::BAD_REQUEST, "le prix ne peut pas être négatif"));
    }
    let tenant_id = current_tenant_id(&state.pool).await?;
    let id = req.id.unwrap_or_else(Uuid::new_v4);

    let mut fields = BTreeMap::new();
    fields.insert("sku".into(), req.sku.trim().to_string());
    fields.insert("nom".into(), req.nom.trim().to_string());
    fields.insert("prix_cents".into(), req.prix_cents.to_string());

    let mut tx = state.pool.begin().await.map_err(ise)?;

    let (seq, nonce_hex, ct_hex) = append_business_op(
        &mut tx, &state, OpType::ProduitUpsert,
        BusinessData { entity_id: id.to_string(), fields },
    ).await?;

    sqlx::query(
        "INSERT INTO business.produits (id, tenant_id, sku, nom, prix_cents) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (id) DO UPDATE SET sku = $3, nom = $4, prix_cents = $5, updated_at = NOW()",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(req.sku.trim())
    .bind(req.nom.trim())
    .bind(req.prix_cents)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // Violation d'unicité (tenant_id, sku) → message clair.
        if e.to_string().contains("produits_tenant_id_sku_key") {
            err(StatusCode::CONFLICT, format!("un produit avec le SKU '{}' existe déjà", req.sku.trim()))
        } else { ise(e) }
    })?;

    tx.commit().await.map_err(ise)?;
    tracing::info!(seq, %id, sku = %req.sku.trim(), "produit upsert (journalisé)");
    spawn_push(&state, tenant_id, seq, nonce_hex, ct_hex);

    Ok((StatusCode::OK, Json(ProduitDto {
        id, sku: req.sku.trim().into(), nom: req.nom.trim().into(), prix_cents: req.prix_cents,
    })))
}

/// GET /produits — liste des produits du tenant.
pub async fn list_produits(State(state): State<Arc<AppState>>) -> BizResult<Vec<ProduitDto>> {
    let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(
        "SELECT id, sku, nom, prix_cents FROM business.produits ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ise)?;
    let produits = rows.into_iter()
        .map(|(id, sku, nom, prix_cents)| ProduitDto { id, sku, nom, prix_cents })
        .collect();
    Ok((StatusCode::OK, Json(produits)))
}

/// DELETE /produits/:id — supprimer un produit (journalisé).
pub async fn delete_produit(
    State(state): State<Arc<AppState>>,
    Path(id):     Path<Uuid>,
) -> BizResult<serde_json::Value> {
    let tenant_id = current_tenant_id(&state.pool).await?;
    let mut tx = state.pool.begin().await.map_err(ise)?;

    let deleted = sqlx::query("DELETE FROM business.produits WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(ise)?
        .rows_affected();
    if deleted == 0 {
        return Err(err(StatusCode::NOT_FOUND, "produit introuvable"));
    }

    let (seq, nonce_hex, ct_hex) = append_business_op(
        &mut tx, &state, OpType::ProduitDelete,
        BusinessData { entity_id: id.to_string(), fields: BTreeMap::new() },
    ).await?;

    tx.commit().await.map_err(ise)?;
    tracing::info!(seq, %id, "produit supprimé (journalisé)");
    spawn_push(&state, tenant_id, seq, nonce_hex, ct_hex);

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}

// ═══════════════════════════════════ CLIENTS ════════════════════════════════════

#[derive(Deserialize)]
pub struct ClientUpsertReq {
    pub id:        Option<Uuid>,
    pub nom:       String,
    #[serde(default)]
    pub email:     String,
    #[serde(default)]
    pub telephone: String,
}

#[derive(Serialize)]
pub struct ClientDto {
    pub id:        Uuid,
    pub nom:       String,
    pub email:     String,
    pub telephone: String,
}

/// POST /clients — créer ou mettre à jour un client (journalisé).
pub async fn upsert_client(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<ClientUpsertReq>,
) -> BizResult<ClientDto> {
    if req.nom.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "le nom du client est requis"));
    }
    let tenant_id = current_tenant_id(&state.pool).await?;
    let id = req.id.unwrap_or_else(Uuid::new_v4);

    let mut fields = BTreeMap::new();
    fields.insert("nom".into(), req.nom.trim().to_string());
    fields.insert("email".into(), req.email.trim().to_string());
    fields.insert("telephone".into(), req.telephone.trim().to_string());

    let mut tx = state.pool.begin().await.map_err(ise)?;

    let (seq, nonce_hex, ct_hex) = append_business_op(
        &mut tx, &state, OpType::ClientUpsert,
        BusinessData { entity_id: id.to_string(), fields },
    ).await?;

    sqlx::query(
        "INSERT INTO business.clients (id, tenant_id, nom, email, telephone) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (id) DO UPDATE SET nom = $3, email = $4, telephone = $5, updated_at = NOW()",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(req.nom.trim())
    .bind(req.email.trim())
    .bind(req.telephone.trim())
    .execute(&mut *tx)
    .await
    .map_err(ise)?;

    tx.commit().await.map_err(ise)?;
    tracing::info!(seq, %id, nom = %req.nom.trim(), "client upsert (journalisé)");
    spawn_push(&state, tenant_id, seq, nonce_hex, ct_hex);

    Ok((StatusCode::OK, Json(ClientDto {
        id, nom: req.nom.trim().into(), email: req.email.trim().into(), telephone: req.telephone.trim().into(),
    })))
}

/// GET /clients — liste des clients du tenant.
pub async fn list_clients(State(state): State<Arc<AppState>>) -> BizResult<Vec<ClientDto>> {
    let rows: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, nom, email, telephone FROM business.clients ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ise)?;
    let clients = rows.into_iter()
        .map(|(id, nom, email, telephone)| ClientDto { id, nom, email, telephone })
        .collect();
    Ok((StatusCode::OK, Json(clients)))
}

/// DELETE /clients/:id — supprimer un client (journalisé).
pub async fn delete_client(
    State(state): State<Arc<AppState>>,
    Path(id):     Path<Uuid>,
) -> BizResult<serde_json::Value> {
    let tenant_id = current_tenant_id(&state.pool).await?;
    let mut tx = state.pool.begin().await.map_err(ise)?;

    let deleted = sqlx::query("DELETE FROM business.clients WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(ise)?
        .rows_affected();
    if deleted == 0 {
        return Err(err(StatusCode::NOT_FOUND, "client introuvable"));
    }

    let (seq, nonce_hex, ct_hex) = append_business_op(
        &mut tx, &state, OpType::ClientDelete,
        BusinessData { entity_id: id.to_string(), fields: BTreeMap::new() },
    ).await?;

    tx.commit().await.map_err(ise)?;
    tracing::info!(seq, %id, "client supprimé (journalisé)");
    spawn_push(&state, tenant_id, seq, nonce_hex, ct_hex);

    Ok((StatusCode::OK, Json(serde_json::json!({ "deleted": id }))))
}
