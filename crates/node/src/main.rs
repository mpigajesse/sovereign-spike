//! Point d'entrée du nœud souverain.
//!
//! Variables d'environnement :
//!   DATABASE_URL        — connexion PostgreSQL (défaut : postgres://sovereign:sovereign@localhost/sovereign_active)
//!   SOVEREIGN_DEK_HEX   — DEK hex-encodée 32 octets (si absente : DEK éphémère générée + loguée)
//!   LISTEN_ADDR         — adresse d'écoute HTTP (défaut : 0.0.0.0:3000)

mod active;
mod failover;

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
    let dek = match std::env::var("SOVEREIGN_DEK_HEX") {
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

    // ── Démarrage du serveur ──────────────────────────────────────────────────
    // ── Chargement de l'époque (fencing token) ───────────────────────────────
    let epoch = failover::load_epoch(&pool).await
        .map_err(|e| anyhow::anyhow!("impossible de charger l'époque de fencing : {e}"))?;
    let epoch_guard = failover::EpochGuard::new(epoch);
    tracing::info!(epoch, "époque de fencing chargée");

    let state = Arc::new(active::AppState { pool, dek, epoch_guard });
    let app   = active::router(state);

    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    tracing::info!("nœud actif souverain en écoute sur {listen_addr}");

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
