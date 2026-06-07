//! Point d'entrée du nœud souverain.
//!
//! Variables d'environnement :
//!   DATABASE_URL        — connexion PostgreSQL (défaut : postgres://sovereign:sovereign@localhost/sovereign_active)
//!   SOVEREIGN_DEK_HEX   — DEK hex-encodée 32 octets (si absente : DEK éphémère générée + loguée)
//!   LISTEN_ADDR         — adresse d'écoute HTTP (défaut : 0.0.0.0:3000)
//!   LICENSE_AUTHORITY_PUBKEY_HEX / LICENSE_TOKEN_PATH — licence soft (cf. license.rs) ;
//!                        absentes ⇒ mode autonome, AUCUN impact sur les routes métier

mod active;
mod business;
mod devices;
mod failover;
mod license;
mod tenant;

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_node=info,tower_http=info".parse().unwrap()),
        )
        .init();

    sovereign_core::crypto::init().expect("libsodium init");

    // ── Connexion PostgreSQL ──────────────────────────────────────────────────
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://sovereign:sovereign@localhost/sovereign_active".to_string()
    });

    tracing::info!("connexion à PostgreSQL : {database_url}");
    let pool = sqlx::PgPool::connect(&database_url).await?;

    // ── Migrations ────────────────────────────────────────────────────────────
    tracing::info!("application des migrations…");
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("migrations OK");

    // ── Chargement de la DEK ──────────────────────────────────────────────────
    // DEK d'amorçage : depuis l'env (ou éphémère). Elle ne sert qu'à initialiser la
    // génération 1 au tout premier démarrage ; ensuite la DEK courante vient de la base
    // (table dek_state), pour que les rotations (dé-enrôlement) survivent aux redémarrages.
    let env_dek = match std::env::var("SOVEREIGN_DEK_HEX") {
        Ok(hex_str) => {
            let bytes = hex::decode(hex_str.trim())
                .map_err(|e| anyhow::anyhow!("SOVEREIGN_DEK_HEX invalide (hex) : {e}"))?;
            sovereign_core::Dek::from_bytes(&bytes)
                .ok_or_else(|| anyhow::anyhow!("SOVEREIGN_DEK_HEX invalide : taille incorrecte (attendu 32 octets)"))?
        }
        Err(_) => {
            let dek = sovereign_core::Dek::generate();
            // En production, la DEK serait chargée depuis le trousseau sécurisé.
            // Pour le spike, on logue la DEK éphémère pour permettre les tests.
            tracing::warn!(
                dek_hex = %hex::encode(dek.as_bytes()),
                "SOVEREIGN_DEK_HEX non défini — DEK éphémère. \
                 Définir la variable pour la persistance entre redémarrages."
            );
            dek
        }
    };

    // DEK opérationnelle : amorce la génération 1 si besoin, sinon charge la plus récente.
    let (dek, generation) = devices::load_or_seed_dek(&pool, env_dek).await
        .map_err(|e| anyhow::anyhow!("chargement DEK (dek_state) : {e}"))?;
    tracing::info!(generation, "DEK opérationnelle chargée (génération courante)");

    // ── Démarrage du serveur ──────────────────────────────────────────────────
    // ── Chargement de l'époque (fencing token) ───────────────────────────────
    let epoch = failover::load_epoch(&pool).await
        .map_err(|e| anyhow::anyhow!("impossible de charger l'époque de fencing : {e}"))?;
    let epoch_guard = failover::EpochGuard::new(epoch);
    tracing::info!(epoch, "époque de fencing chargée");

    // RELAY_URL vide ⇒ pas de relais (évite un push vers une URL "/blobs" invalide).
    let relay_url = std::env::var("RELAY_URL").ok().filter(|s| !s.trim().is_empty());
    let relay_key = std::env::var("RELAY_API_KEY").ok();
    if let Some(ref url) = relay_url {
        tracing::info!(relay = %url, "push automatique vers le relais activé");
    }

    // ── Licence (soft — informatif uniquement, ne bloque jamais l'accès aux données) ──
    let license_authority_pubkey = license::trusted_authority_pubkey_from_env();
    let license_token_path = license::token_path_from_env();
    let license_status = license::current_status(license_authority_pubkey.as_deref(), &license_token_path);
    match &license_status {
        license::LicenseStatus::Valid { claims } => tracing::info!(
            tenant_id = %claims.tenant_id, plan = %claims.plan, expires_at = %claims.expires_at,
            "licence valide"
        ),
        license::LicenseStatus::Expired { claims } => tracing::warn!(
            tenant_id = %claims.tenant_id, expired_at = %claims.expires_at,
            "licence expirée — MAJ/support suspendus, les données restent pleinement accessibles"
        ),
        license::LicenseStatus::Invalid => tracing::warn!(
            "jeton de licence invalide (signature ou autorité inattendue) — \
             MAJ/support suspendus, les données restent pleinement accessibles"
        ),
        license::LicenseStatus::NotConfigured => tracing::info!(
            "aucune licence configurée — mode autonome, les données restent pleinement accessibles"
        ),
    }

    let state = Arc::new(active::AppState {
        pool,
        dek: std::sync::RwLock::new(dek),
        epoch_guard,
        relay_url,
        relay_key,
        license_authority_pubkey,
        license_token_path,
        license_status: std::sync::RwLock::new(license_status),
    });

    // Ré-évaluation périodique : capte à la fois l'expiration au fil du temps et le
    // renouvellement (remplacement du fichier jeton), sans jamais toucher aux routes
    // métier — la licence reste un simple indicateur lu par le tableau de bord.
    tokio::spawn(spawn_license_refresh(state.clone()));

    let app   = active::router(state);

    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    tracing::info!("nœud actif souverain en écoute sur {listen_addr}");

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Ré-évalue périodiquement l'état de licence (expiration au fil du temps,
/// renouvellement via remplacement du fichier jeton). Tâche d'arrière-plan
/// purement informative : sa panne ou son arrêt n'affecte aucune route métier.
async fn spawn_license_refresh(state: Arc<active::AppState>) {
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

    let mut interval = tokio::time::interval(REFRESH_INTERVAL);
    interval.tick().await; // ignore le premier déclenchement immédiat (déjà loggé au démarrage)

    loop {
        interval.tick().await;

        let nouveau_statut = license::current_status(state.license_authority_pubkey.as_deref(), &state.license_token_path);
        let mut verrou = state.license_status.write().expect("license_status RwLock empoisonné");
        *verrou = nouveau_statut;
    }
}
