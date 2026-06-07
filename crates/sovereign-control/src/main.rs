//! sovereign-control — service éditeur d'autorité (licences / MAJ / métriques).
//!
//! ⚠ INTENTIONNEL : ce service est SÉPARÉ du relais aveugle (sovereign-relay).
//!   - Le relais tire sa crédibilité de sa cécité totale : il ne dépend même pas
//!     de sovereign-core et ne voit jamais ni données ni identité.
//!   - sovereign-control détient au contraire une autorité (clé privée Ed25519 de
//!     licence) et dépend du cœur pour signer — mais ne stocke, ne transmet et ne
//!     déchiffre AUCUN blob ni DEK métier.
//!
//!   Deux binaires = deux propriétés séparément prouvables en soutenance.
//!
//! Rôle (squelette Phase 0) : émettre des jetons de licence signés Ed25519. Le nœud
//! actif les vérifie ensuite localement, hors-ligne — la licence est « soft » : son
//! expiration ou sa révocation bloque les MAJ/le support, jamais l'accès aux données.
//!
//! Variables d'environnement :
//!   CONTROL_AUTHORITY_SK_HEX — clé privée Ed25519 de l'autorité (hex, 64 octets) ;
//!                              si absente, une paire éphémère est générée et sa clé
//!                              publique est loguée (à redistribuer aux nœuds actifs)
//!   CONTROL_LISTEN_ADDR      — adresse d'écoute HTTP (défaut : 0.0.0.0:5000)

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sovereign_core::license::{issue_token, LicenseAuthorityKeypair, LicenseClaims, LicenseToken};

/// État partagé du service — l'autorité de licence est la seule chose secrète ici.
struct AppState {
    authority: LicenseAuthorityKeypair,
}

#[derive(Debug, Deserialize)]
struct IssueLicenseRequest {
    tenant_id:     String,
    plan:          String,
    #[serde(default = "default_validity_days")]
    validity_days: i64,
}

const fn default_validity_days() -> i64 {
    365
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status:                &'static str,
    authority_pubkey_hex:  String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_control=info,tower_http=info".parse().unwrap()),
        )
        .init();

    sovereign_core::crypto::init().expect("libsodium init");

    let authority = load_or_generate_authority();
    tracing::info!(
        authority_pubkey_hex = %authority.public_hex(),
        "autorité de licence prête — distribuer cette clé publique aux nœuds actifs pour la vérification locale"
    );

    let state = Arc::new(AppState { authority });

    let app = Router::new()
        .route("/health", get(health))
        .route("/licenses", post(issue_license))
        .with_state(state);

    let listen_addr = std::env::var("CONTROL_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".to_string());
    tracing::info!("sovereign-control à l'écoute sur {listen_addr}");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Charge l'autorité depuis l'environnement, ou en génère une éphémère pour le squelette/dev.
/// En production, la clé privée serait chargée depuis un secret manager — jamais du code.
fn load_or_generate_authority() -> LicenseAuthorityKeypair {
    match std::env::var("CONTROL_AUTHORITY_SK_HEX") {
        Ok(secret_hex) => LicenseAuthorityKeypair::from_secret_hex(&secret_hex).unwrap_or_else(|| {
            panic!("CONTROL_AUTHORITY_SK_HEX invalide : attendu 64 octets en hex (clé privée Ed25519)")
        }),
        Err(_) => {
            let authority = LicenseAuthorityKeypair::generate();
            tracing::warn!(
                secret_hex = %authority.secret_hex(),
                "CONTROL_AUTHORITY_SK_HEX non défini — autorité éphémère générée (perdue au redémarrage). \
                 Définir la variable avec cette clé pour la persistance."
            );
            authority
        }
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status:               "ok",
        authority_pubkey_hex: state.authority.public_hex(),
    })
}

async fn issue_license(
    State(state): State<Arc<AppState>>,
    Json(request): Json<IssueLicenseRequest>,
) -> Result<Json<LicenseToken>, (StatusCode, Json<ErrorResponse>)> {
    if request.tenant_id.trim().is_empty() || request.plan.trim().is_empty() {
        return Err(bad_request("tenant_id et plan sont requis et non vides"));
    }
    if request.validity_days <= 0 {
        return Err(bad_request("validity_days doit être strictement positif"));
    }

    let issued_at = Utc::now();
    let claims = LicenseClaims {
        tenant_id:  request.tenant_id,
        plan:       request.plan,
        issued_at,
        expires_at: issued_at + Duration::days(request.validity_days),
    };

    match issue_token(&state.authority, &claims) {
        Ok(token) => {
            tracing::info!(tenant_id = %claims.tenant_id, plan = %claims.plan, "jeton de licence émis");
            Ok(Json(token))
        }
        Err(e) => {
            tracing::error!(error = %e, "échec d'émission du jeton de licence");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: "émission du jeton échouée".to_string() }),
            ))
        }
    }
}

fn bad_request(message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: message.to_string() }))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn router_de_test() -> Router {
        sovereign_core::crypto::init().expect("libsodium init");
        let state = Arc::new(AppState { authority: LicenseAuthorityKeypair::generate() });
        Router::new()
            .route("/health", get(health))
            .route("/licenses", post(issue_license))
            .with_state(state)
    }

    #[tokio::test]
    async fn health_renvoie_la_cle_publique_de_lautorite() {
        let reponse = router_de_test()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(reponse.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn emet_un_jeton_pour_une_requete_valide() {
        let corps = serde_json::json!({ "tenant_id": "al-baraa-demo", "plan": "pro" });
        let reponse = router_de_test()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/licenses")
                    .header("content-type", "application/json")
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(reponse.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejette_une_requete_avec_tenant_vide() {
        let corps = serde_json::json!({ "tenant_id": "", "plan": "pro" });
        let reponse = router_de_test()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/licenses")
                    .header("content-type", "application/json")
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejette_une_validite_negative_ou_nulle() {
        let corps = serde_json::json!({ "tenant_id": "al-baraa-demo", "plan": "pro", "validity_days": 0 });
        let reponse = router_de_test()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/licenses")
                    .header("content-type", "application/json")
                    .body(Body::from(corps.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    }
}
