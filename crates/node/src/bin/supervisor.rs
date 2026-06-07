//! Superviseur de quorum souverain — failover automatique (LOT 5, Windows).
//!
//! Un superviseur tourne sur CHAQUE machine du cluster (PC actif + VM1 + VM2).
//! Il ne remplace ni PostgreSQL ni le nœud actif : il les surveille et orchestre
//! la bascule automatique en s'appuyant sur des briques DÉJÀ prouvées :
//!   - réplication WAL PostgreSQL  (03_setup_standby_win.ps1)
//!   - promotion `pg_ctl promote`  (failover manuel prouvé)
//!   - incrément d'époque + fencing (EpochGuard, critère #9)
//!
//! Apport du LOT 5 : la DÉCISION automatique de promotion, protégée par un
//! quorum à majorité stricte → pas de split-brain en cas de partition réseau.
//!
//! Pourquoi pas Patroni/etcd : cible 100% Windows (Patroni y est instable),
//! refus d'un orchestrateur tiers + 2 daemons, et le fencing par époque est
//! déjà notre garde-fou. Le quorum natif Rust est plus simple à déployer,
//! testable, et défendable en soutenance.
//!
//! Variables d'environnement :
//!   SUP_NODE_ID        — identifiant unique de ce nœud (ex : "pc", "vm1", "vm2")
//!   SUP_ROLE           — "primary" | "standby"
//!   SUP_RANK           — priorité de succession (0 = premier successeur). standby only.
//!   SUP_PEERS          — superviseurs pairs, CSV "node_id@host:port"
//!                        (ex : "vm1@192.168.200.2:3100,vm2@192.168.200.3:3100")
//!   SUP_PRIMARY_HEALTH — URL santé du nœud actif primary (ex : http://192.168.200.1:3000/health)
//!   SUP_LOCAL_NODE_URL — URL du nœud actif LOCAL à promouvoir (POST /epoch/promote)
//!   SUP_LISTEN_ADDR    — adresse d'écoute du superviseur (défaut : 0.0.0.0:3100)
//!   SUP_HEARTBEAT_SECS — période de heartbeat (défaut : 3)
//!   SUP_FAIL_THRESHOLD — échecs consécutifs avant de juger le primary mort (défaut : 3)
//!   PG_BIN             — dossier des binaires PostgreSQL (défaut : C:\Program Files\PostgreSQL\18\bin)
//!   PG_DATA            — dossier data PostgreSQL local (défaut : C:\Program Files\PostgreSQL\18\data)

#[path = "../quorum.rs"]
mod quorum;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use axum::{extract::State, http::StatusCode, response::Json, routing::{get, post}, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time;

use quorum::{
    is_preferred_candidate, promotion_authorized, FailureDetector, VoteLedger,
};

// ── Configuration (immuable après le démarrage) ────────────────────────────────

#[derive(Debug, Clone)]
struct Peer {
    node_id: String,
    addr:    String, // host:port
}

#[derive(Debug, Clone)]
struct Config {
    node_id:          String,
    role:             Role,
    rank:             u32,
    peers:            Vec<Peer>,
    primary_health:   String,
    local_node_url:   String,
    heartbeat:        Duration,
    fail_threshold:   u32,
    pg_bin:           String,
    pg_data:          String,
    /// Taille totale du cluster = pairs + soi-même.
    cluster_size:     usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Primary,
    Standby,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let node_id = env_required("SUP_NODE_ID")?;
        let role = match env_required("SUP_ROLE")?.to_lowercase().as_str() {
            "primary" => Role::Primary,
            "standby" => Role::Standby,
            other => anyhow::bail!("SUP_ROLE invalide : '{other}' (attendu primary|standby)"),
        };
        let rank = std::env::var("SUP_RANK").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let peers = parse_peers(&std::env::var("SUP_PEERS").unwrap_or_default());
        let primary_health = env_required("SUP_PRIMARY_HEALTH")?;
        let local_node_url = std::env::var("SUP_LOCAL_NODE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        let heartbeat = Duration::from_secs(
            std::env::var("SUP_HEARTBEAT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(3),
        );
        let fail_threshold =
            std::env::var("SUP_FAIL_THRESHOLD").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
        let pg_bin = std::env::var("PG_BIN")
            .unwrap_or_else(|_| r"C:\Program Files\PostgreSQL\18\bin".to_string());
        let pg_data = std::env::var("PG_DATA")
            .unwrap_or_else(|_| r"C:\Program Files\PostgreSQL\18\data".to_string());

        let cluster_size = peers.len() + 1;

        Ok(Self {
            node_id, role, rank, peers, primary_health, local_node_url,
            heartbeat, fail_threshold, pg_bin, pg_data, cluster_size,
        })
    }
}

fn env_required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("variable d'environnement requise : {key}"))
}

/// Parse "vm1@192.168.200.2:3100,vm2@192.168.200.3:3100" → Vec<Peer>.
fn parse_peers(raw: &str) -> Vec<Peer> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|entry| {
            let (node_id, addr) = entry.split_once('@')?;
            let node_id = node_id.trim();
            let addr = addr.trim();
            if node_id.is_empty() || addr.is_empty() {
                return None;
            }
            Some(Peer { node_id: node_id.to_string(), addr: addr.to_string() })
        })
        .collect()
}

// ── État partagé (mutable) ─────────────────────────────────────────────────────

struct SupervisorState {
    cfg:      Config,
    ledger:   Mutex<VoteLedger>,
    /// Ce nœud a-t-il déjà été promu primary ? (idempotence — une seule promotion)
    promoted: AtomicBool,
    /// Dernière fois (epoch heartbeat tick) où ce superviseur a vu le primary vivant.
    primary_alive: AtomicBool,
    /// Terme d'élection observé/initié par ce nœud.
    term:     AtomicU64,
}

// ── DTOs HTTP ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct VoteRequest {
    term:         u64,
    candidate_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VoteReply {
    granted:   bool,
    term:      u64,
    voter_id:  String,
    /// Le votant voit-il encore le primary vivant ? (transparence / debug)
    primary_seen_alive: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatusReply {
    node_id:        String,
    role:           String,
    rank:           u32,
    cluster_size:   usize,
    promoted:       bool,
    primary_alive:  bool,
    current_term:   u64,
    peers:          Vec<String>,
}

// ── Point d'entrée ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "supervisor=info".parse().unwrap()),
        )
        .init();

    let cfg = Config::from_env()?;
    tracing::info!(
        node_id = %cfg.node_id,
        role = ?cfg.role,
        rank = cfg.rank,
        cluster_size = cfg.cluster_size,
        peers = cfg.peers.len(),
        "superviseur de quorum démarré"
    );

    let state = Arc::new(SupervisorState {
        cfg: cfg.clone(),
        ledger: Mutex::new(VoteLedger::new()),
        promoted: AtomicBool::new(cfg.role == Role::Primary),
        primary_alive: AtomicBool::new(true),
        term: AtomicU64::new(0),
    });

    // ── Boucle de surveillance (standby uniquement) ──────────────────────────
    if cfg.role == Role::Standby {
        let watch_state = Arc::clone(&state);
        tokio::spawn(async move { watch_loop(watch_state).await; });
    } else {
        tracing::info!("rôle primary : pas de boucle de surveillance (ce nœud EST le primary)");
    }

    // ── Serveur HTTP du superviseur ──────────────────────────────────────────
    let app = Router::new()
        .route("/health", get(|| async { "ok (supervisor)" }))
        .route("/vote", post(handle_vote))
        .route("/supervisor/status", get(handle_status))
        .with_state(Arc::clone(&state));

    let listen_addr = std::env::var("SUP_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3100".to_string());
    tracing::info!("superviseur en écoute sur {listen_addr}");
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Boucle de surveillance + élection ──────────────────────────────────────────

async fn watch_loop(state: Arc<SupervisorState>) {
    let cfg = &state.cfg;
    let mut detector = FailureDetector::new(cfg.fail_threshold);
    let mut interval = time::interval(cfg.heartbeat);
    let client = reqwest::Client::new();

    loop {
        interval.tick().await;

        // Si ce nœud a déjà été promu, il EST le nouveau primary → plus de surveillance.
        if state.promoted.load(Ordering::SeqCst) {
            continue;
        }

        // 1. Heartbeat du primary
        let alive = ping(&client, &cfg.primary_health).await;
        state.primary_alive.store(alive, Ordering::SeqCst);
        if alive {
            detector.record_success();
            continue;
        }
        detector.record_failure();
        tracing::warn!(
            failures = detector.consecutive_failures(),
            threshold = cfg.fail_threshold,
            "heartbeat primary échoué"
        );
        if !detector.is_primary_down() {
            continue; // pas encore le seuil — on patiente
        }

        // 2. Primary jugé mort. Suis-je le candidat préféré (rang le plus faible) ?
        let reachable = reachable_standbys(&client, cfg).await;
        if !is_preferred_candidate(cfg.rank, &cfg.node_id, &reachable) {
            tracing::info!(
                rang = cfg.rank,
                "primary mort mais un standby plus prioritaire est joignable → j'attends"
            );
            continue;
        }

        // 3. Lancer une élection (nouveau terme).
        let term = state.term.fetch_add(1, Ordering::SeqCst) + 1;
        tracing::warn!(term, candidate = %cfg.node_id, "lancement d'une élection de promotion");
        if try_get_elected(&client, cfg, term).await {
            tracing::warn!(term, "QUORUM ATTEINT — promotion de ce nœud en primary");
            match promote_self(&client, cfg).await {
                Ok(epoch) => {
                    state.promoted.store(true, Ordering::SeqCst);
                    tracing::warn!(epoch, "promotion réussie — ce nœud est désormais le primary actif");
                }
                Err(e) => tracing::error!("promotion échouée : {e}"),
            }
        } else {
            tracing::warn!(term, "quorum NON atteint (minorité ou partition) — promotion refusée (anti-split-brain)");
        }
    }
}

/// Sonde HTTP simple : true si la requête aboutit avec un statut 2xx.
async fn ping(client: &reqwest::Client, url: &str) -> bool {
    matches!(
        client.get(url).timeout(Duration::from_secs(2)).send().await,
        Ok(r) if r.status().is_success()
    )
}

/// Interroge les pairs pour savoir lesquels sont des standbys joignables.
/// Renvoie (rang, node_id) de chaque pair standby vivant.
async fn reachable_standbys(client: &reqwest::Client, cfg: &Config) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for peer in &cfg.peers {
        let url = format!("http://{}/supervisor/status", peer.addr);
        if let Ok(resp) = client.get(&url).timeout(Duration::from_secs(2)).send().await {
            if let Ok(status) = resp.json::<StatusReply>().await {
                if status.role == "standby" && !status.promoted {
                    out.push((status.rank, status.node_id));
                }
            }
        }
    }
    out
}

/// Demande un vote à chaque pair et agrège jusqu'à atteindre (ou non) le quorum.
async fn try_get_elected(client: &reqwest::Client, cfg: &Config, term: u64) -> bool {
    let mut granters: HashSet<String> = HashSet::new();
    for peer in &cfg.peers {
        let url = format!("http://{}/vote", peer.addr);
        let body = VoteRequest { term, candidate_id: cfg.node_id.clone() };
        match client.post(&url).json(&body).timeout(Duration::from_secs(2)).send().await {
            Ok(resp) => {
                if let Ok(reply) = resp.json::<VoteReply>().await {
                    if reply.granted {
                        tracing::info!(voter = %reply.voter_id, "voix accordée");
                        granters.insert(reply.voter_id);
                    } else {
                        tracing::info!(voter = %reply.voter_id, "voix refusée");
                    }
                }
            }
            Err(e) => tracing::warn!(peer = %peer.node_id, "pair injoignable pour le vote : {e}"),
        }
    }
    promotion_authorized(&granters, &cfg.node_id, cfg.cluster_size)
}

/// Promotion locale : `pg_ctl promote` puis incrément d'époque via le nœud actif local.
async fn promote_self(client: &reqwest::Client, cfg: &Config) -> anyhow::Result<i64> {
    // 1. Promouvoir le PostgreSQL standby local → primary (réplication WAL prouvée).
    let pg_ctl = format!(r"{}\pg_ctl.exe", cfg.pg_bin);
    tracing::info!(pg_ctl = %pg_ctl, data = %cfg.pg_data, "promotion PostgreSQL (pg_ctl promote)");
    let output = tokio::process::Command::new(&pg_ctl)
        .args(["promote", "-D", &cfg.pg_data])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => tracing::info!("pg_ctl promote OK"),
        Ok(o) => {
            // Non bloquant : PG peut déjà être primary (promotion rejouée). On loggue.
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&o.stderr),
                "pg_ctl promote a renvoyé un code non nul (déjà primary ?)"
            );
        }
        Err(e) => tracing::error!("impossible d'exécuter pg_ctl : {e} — promotion d'époque tout de même tentée"),
    }

    // 2. Incrémenter l'époque (fencing) via le nœud actif local — bloque l'ancien primary.
    let url = format!("{}/epoch/promote", cfg.local_node_url.trim_end_matches('/'));
    let resp = client.post(&url).timeout(Duration::from_secs(5)).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("POST {url} → HTTP {}", resp.status());
    }
    #[derive(Deserialize)]
    struct EpochResp { epoch: i64 }
    let epoch: EpochResp = resp.json().await?;
    Ok(epoch.epoch)
}

// ── Handlers HTTP ───────────────────────────────────────────────────────────────

async fn handle_vote(
    State(state): State<Arc<SupervisorState>>,
    Json(req): Json<VoteRequest>,
) -> (StatusCode, Json<VoteReply>) {
    // Un votant n'accorde sa voix que s'il considère LUI AUSSI le primary mort.
    let primary_seen_alive = state.primary_alive.load(Ordering::SeqCst);
    let mut ledger = state.ledger.lock().await;
    let granted = ledger.grant_vote(req.term, &req.candidate_id, primary_seen_alive);
    // S'aligner sur le terme courant pour les futures élections.
    state.term.store(ledger.current_term(), Ordering::SeqCst);
    tracing::info!(
        term = req.term,
        candidate = %req.candidate_id,
        granted,
        primary_seen_alive,
        "vote traité"
    );
    (
        StatusCode::OK,
        Json(VoteReply {
            granted,
            term: ledger.current_term(),
            voter_id: state.cfg.node_id.clone(),
            primary_seen_alive,
        }),
    )
}

async fn handle_status(
    State(state): State<Arc<SupervisorState>>,
) -> Json<StatusReply> {
    let ledger = state.ledger.lock().await;
    Json(StatusReply {
        node_id:       state.cfg.node_id.clone(),
        role:          if state.cfg.role == Role::Primary { "primary".to_string() } else { "standby".to_string() },
        rank:          state.cfg.rank,
        cluster_size:  state.cfg.cluster_size,
        promoted:      state.promoted.load(Ordering::SeqCst),
        primary_alive: state.primary_alive.load(Ordering::SeqCst),
        current_term:  ledger.current_term(),
        peers:         state.cfg.peers.iter().map(|p| p.node_id.clone()).collect(),
    })
}

// ── Tests (parsing config — la logique de quorum est testée dans quorum.rs) ─────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_peers_format_csv() {
        let peers = parse_peers("vm1@192.168.200.2:3100, vm2@192.168.200.3:3100");
        assert_eq!(peers.len(), 2);
        assert_eq!(peers[0].node_id, "vm1");
        assert_eq!(peers[0].addr, "192.168.200.2:3100");
        assert_eq!(peers[1].node_id, "vm2");
    }

    #[test]
    fn parse_peers_ignore_entrees_vides_et_malformees() {
        let peers = parse_peers(",, vm1@host:1 , bad_no_at , @nohost , id@ ");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "vm1");
    }

    #[test]
    fn parse_peers_vide_donne_liste_vide() {
        assert!(parse_peers("").is_empty());
        assert!(parse_peers("   ").is_empty());
    }
}
