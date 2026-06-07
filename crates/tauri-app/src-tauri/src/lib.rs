//! Backend Tauri — Sovereign Data Agent.
//!
//! Au démarrage, l'app :
//!   1. Cherche sovereign-node-active (sidecar embarqué ou dans PATH)
//!   2. Vérifie si PostgreSQL local est disponible
//!   3. Lance le nœud actif automatiquement si PostgreSQL est là
//!   4. Sinon : mode CLIENT — se connecte à un nœud actif distant
//!
//! La PME n'a rien à faire manuellement.

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sovereign_core::DeviceKeypair;
use tauri::{Manager, State};

// ── Anti-fenêtre console (Windows) ─────────────────────────────────────────────
//
// L'app GUI est compilée avec `windows_subsystem = "windows"` : elle n'a AUCUNE
// console. Sous Windows, lancer un processus enfant de type console (sidecars
// node/relais, psql, powershell) depuis un parent sans console fait apparaître
// une NOUVELLE fenêtre terminal à chaque spawn. Comme le Dashboard interroge
// `cluster_status` (→ plusieurs psql) toutes les 5 s, des fenêtres clignotaient
// « en boucle ». Le flag CREATE_NO_WINDOW supprime ces fenêtres parasites.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Applique CREATE_NO_WINDOW sous Windows (no-op ailleurs). Tous les `Command`
/// du backend passent par ici pour ne jamais ouvrir de console visible.
fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// ── État ──────────────────────────────────────────────────────────────────────

struct NodeProcess(Mutex<Option<Child>>);

/// Processus du superviseur de quorum (failover automatique, LOT 5).
/// Slot séparé du nœud actif : superviseur et nœud tournent en parallèle.
struct SupervisorProcess(Mutex<Option<Child>>);

/// Coffre des paires de clés des « appareils virtuels » créés depuis l'UI pour la
/// démo d'enrôlement. La clé privée X25519 ne quitte JAMAIS le backend Rust : l'UI
/// ne reçoit que la clé publique (le « QR ») et un handle = cette clé publique hex.
/// (Volatile : remis à zéro au redémarrage de l'app — suffisant pour une démo live.)
struct DeviceVault(Mutex<HashMap<String, DeviceKeypair>>);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StartupStatus {
    pub mode:         String, // "local" | "remote" | "starting" | "error"
    pub message:      String,
    pub active_url:   String,
}

// ── Commandes Tauri ───────────────────────────────────────────────────────────

/// Statut de démarrage — appelé par le frontend au chargement.
#[tauri::command]
async fn get_startup_status(state: State<'_, NodeProcess>) -> Result<StartupStatus, String> {
    let running = state.0.lock().map(|g| g.is_some()).unwrap_or(false);
    if running {
        return Ok(StartupStatus {
            mode:       "local".into(),
            message:    "Nœud actif démarré localement".into(),
            active_url: "http://127.0.0.1:3000".into(),
        });
    }
    Ok(StartupStatus {
        mode:       "remote".into(),
        // Aucune IP en dur : l'URL du nœud actif est saisie par l'utilisateur
        // (Installation / Configuration) et lue côté frontend depuis localStorage.
        message:    "Mode client — configurez l'URL du nœud actif".into(),
        active_url: String::new(),
    })
}

/// Démarre le nœud actif (sidecar embarqué).
/// Retourne le statut : "started" | "already_running" | "pg_unavailable" | "binary_not_found"
#[tauri::command]
async fn start_active_node(
    db_url:    String,
    dek_hex:   String,
    relay_url: String,
    relay_key: String,
    state:     State<'_, NodeProcess>,
    app:       tauri::AppHandle,
) -> Result<StartupStatus, String> {
    // Déjà démarré ET toujours vivant ? Si l'enfant est mort, on nettoie le slot
    // et on relance (sinon un enfant mort « poison » l'état et bloque tout
    // redémarrage du nœud — bug observé en 0.1.6).
    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => {
                    return Ok(StartupStatus {
                        mode:       "local".into(),
                        message:    "Nœud actif déjà en cours".into(),
                        active_url: "http://127.0.0.1:3000".into(),
                    });
                }
                _ => { *guard = None; } // mort ou erreur → on relance
            }
        }
    }

    // Trouver le binaire :
    // 1. Sidecar embarqué dans le bundle Tauri
    // 2. Même dossier que l'exe
    // 3. PATH système
    let bin_path = find_binary(&app);

    let Some(bin) = bin_path else {
        return Ok(StartupStatus {
            mode:       "remote".into(),
            message:    "Binaire non trouvé — mode client uniquement".into(),
            active_url: String::new(),
        });
    };

    // Vérifier PostgreSQL (tentative de connexion TCP sur port 5432)
    let pg_ok = check_pg_available().await;
    if !pg_ok {
        return Ok(StartupStatus {
            mode:       "remote".into(),
            message:    "PostgreSQL non détecté localement — mode client. Vérifiez que PostgreSQL 18 est installé et démarré.".into(),
            active_url: String::new(),
        });
    }

    // Défauts du spike : quand l'app relance le nœud actif (primary-restart),
    // elle passe dek_hex/relay_url vides. On fournit donc des défauts pour que
    //   - la DEK persistante du spike soit toujours utilisée (sinon le nœud ne
    //     peut pas déchiffrer le journal existant) ;
    //   - le push vers le relais aveugle soit automatique (relais co-localisé).
    // Une valeur non vide (saisie en Configuration) reste prioritaire.
    const SPIKE_DEK: &str = "174835f0e063680d4b4652c7edf9472a1db0626388dbbe4342d84a7c9bce035b";
    let dek:   &str = if dek_hex.trim().is_empty()   { SPIKE_DEK }              else { dek_hex.as_str()   };
    let relay: &str = if relay_url.trim().is_empty() { "http://127.0.0.1:4000" } else { relay_url.as_str() };

    // Lancer le nœud actif
    let mut cmd = Command::new(&bin);
    cmd.env("DATABASE_URL",      if db_url.is_empty() { "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active" } else { &db_url })
        .env("LISTEN_ADDR",       "0.0.0.0:3000")
        .env("SOVEREIGN_DEK_HEX", dek)
        .env("RELAY_URL",         relay)
        .env("RELAY_API_KEY",     if relay_key.is_empty() { "sovereign-spike-relay-key-2026" } else { &relay_key });
    let child = no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("Impossible de démarrer {bin}: {e}"))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }

    // Attendre que le nœud réponde (max 8s)
    for _ in 0..16 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if ping_node("http://127.0.0.1:3000").await {
            return Ok(StartupStatus {
                mode:       "local".into(),
                message:    "✓ Nœud actif démarré — prêt".into(),
                active_url: "http://127.0.0.1:3000".into(),
            });
        }
    }

    Ok(StartupStatus {
        mode:       "local".into(),
        message:    "Nœud actif lancé (démarrage en cours…)".into(),
        active_url: "http://127.0.0.1:3000".into(),
    })
}

/// Arrête le nœud actif local.
#[tauri::command]
fn stop_active_node(state: State<'_, NodeProcess>) -> Result<String, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = guard.take() {
        child.kill().map_err(|e| format!("Arrêt : {e}"))?;
        Ok("Nœud actif arrêté".into())
    } else {
        Ok("Nœud actif n'était pas démarré".into())
    }
}

/// Ping health du nœud (actif ou distant).
#[tauri::command]
async fn ping_health(url: String) -> bool {
    ping_node(&url).await
}

/// Vérifie si PostgreSQL est disponible localement (port 5432).
#[tauri::command]
async fn check_pg_local() -> bool {
    check_pg_available().await
}

/// Nœud découvert sur le réseau local.
#[derive(Debug, Serialize)]
pub struct DiscoveredNode {
    pub ip:   String,
    pub port: u16,
    pub role: String, // "actif" | "passif" | "relais"
    pub url:  String,
}

/// Découverte réseau : scanne un sous-réseau /24 fourni par l'utilisateur
/// (déduit de l'URL du nœud actif qu'il a saisie) pour trouver les nœuds.
///
/// AUCUNE IP n'est codée en dur : si l'utilisateur n'a saisi aucune adresse,
/// la découverte ne scanne rien (le mapping du parc est une donnée du client,
/// cf. cadrage §6). Le spike scanne au lieu de mDNS (hors-périmètre §7.5).
#[tauri::command]
async fn discover_nodes(subnet: Option<String>) -> Vec<DiscoveredNode> {
    // Ne jamais scanner le loopback. Sans sous-réseau fourni → rien à scanner
    // (l'utilisateur saisit ses adresses manuellement).
    let base = match subnet {
        Some(s) if !s.is_empty() && !s.starts_with("127.") => s,
        _ => return Vec::new(),
    };

    // Ports et rôles à sonder
    let probes: &[(u16, &str, &str)] = &[
        (3000, "actif",  "/health"),
        (3001, "passif", "/health"),
        (4000, "relais", "/health"),
    ];

    let mut handles = Vec::new();

    // Scanner les hôtes 1..=254 en parallèle
    for host in 1u8..=254 {
        let ip = format!("{base}.{host}");
        for &(port, role, path) in probes {
            let ip = ip.clone();
            let role = role.to_string();
            let path = path.to_string();
            handles.push(tokio::spawn(async move {
                let url = format!("http://{ip}:{port}");
                let probe_url = format!("{url}{path}");
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_millis(400))
                    .build()
                    .ok()?;
                match client.get(&probe_url).send().await {
                    Ok(r) if r.status().is_success() => Some(DiscoveredNode {
                        ip, port, role, url,
                    }),
                    _ => None,
                }
            }));
        }
    }

    let mut found = Vec::new();
    for h in handles {
        if let Ok(Some(node)) = h.await {
            found.push(node);
        }
    }
    found.sort_by_key(|a| a.port);
    found
}

/// Démarre le RELAIS aveugle (sovereign-relay) sur cette machine.
/// Le relais ne détient AUCUNE clé : il ne stocke que des blobs chiffrés opaques
/// (zero-knowledge par construction — cf. crates/relay, sans sovereign-core).
#[tauri::command]
async fn start_relay_node(
    state: State<'_, NodeProcess>,
    app:   tauri::AppHandle,
) -> Result<StartupStatus, String> {
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Ok(StartupStatus {
                mode:       "relay".into(),
                message:    "Relais déjà en cours".into(),
                active_url: "http://127.0.0.1:4000".into(),
            });
        }
    }

    let bin = find_relay_binary(&app).ok_or_else(||
        "Binaire sovereign-relay introuvable dans le bundle".to_string())?;

    // Store SQLite persistant dans le dossier de données utilisateur
    let db_path = app.path().app_data_dir().ok()
        .map(|d| { let _ = std::fs::create_dir_all(&d); d.join("sovereign_relay.db").to_string_lossy().to_string() })
        .unwrap_or_else(|| "sovereign_relay.db".to_string());

    let mut cmd = Command::new(&bin);
    cmd.env("RELAY_LISTEN_ADDR", "0.0.0.0:4000")
        .env("RELAY_API_KEY",     "sovereign-spike-relay-key-2026")
        .env("RELAY_DB_PATH",     &db_path);
    let child = no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("Impossible de démarrer le relais : {e}"))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }

    for _ in 0..16 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if ping_node("http://127.0.0.1:4000").await {
            return Ok(StartupStatus {
                mode:       "relay".into(),
                message:    "✓ Relais aveugle démarré (zero-knowledge)".into(),
                active_url: "http://127.0.0.1:4000".into(),
            });
        }
    }

    Ok(StartupStatus {
        mode:       "relay".into(),
        message:    "Relais lancé (démarrage en cours…)".into(),
        active_url: "http://127.0.0.1:4000".into(),
    })
}

fn find_relay_binary(app: &tauri::AppHandle) -> Option<String> {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("sovereign-relay.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("sovereign-relay.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if no_window(Command::new("sovereign-relay").arg("--help")).output().is_ok() {
        return Some("sovereign-relay".into());
    }
    None
}

/// Exécute le script de configuration standby (embarqué dans le bundle).
/// Le script PowerShell est copié dans un fichier temporaire puis exécuté.
#[tauri::command]
async fn run_standby_setup(primary_ip: String, app: tauri::AppHandle) -> Result<String, String> {
    // Chercher le script embarqué dans les ressources Tauri
    let script_content = find_standby_script(&app)?;

    // Normaliser l'entrée : on attend une IP nue. On retire un éventuel
    // schéma "http://" et tout ":port" / "/chemin" collés par erreur, sinon
    // PostgreSQL/PowerShell tenterait de résoudre "http://x" comme un hôte.
    let host = primary_ip
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_string();

    // Injecter l'IP du primary fournie par l'utilisateur
    let content = if !host.is_empty() {
        script_content.replace(
            r#"$PRIMARY_IP   = "192.168.200.1""#,
            &format!(r#"$PRIMARY_IP   = "{host}""#),
        )
    } else {
        script_content
    };

    // Écrire le fichier temporaire AVEC BOM UTF-8 pour que Windows PowerShell 5.1
    // interprète correctement l'encodage (évite la corruption des caractères).
    let tmp = std::env::temp_dir().join("sovereign_setup_standby.ps1");
    let mut bytes = vec![0xEF, 0xBB, 0xBF]; // BOM UTF-8
    bytes.extend_from_slice(content.as_bytes());
    std::fs::write(&tmp, &bytes)
        .map_err(|e| format!("Impossible d'écrire le script temporaire : {e}"))?;

    // Lancer le script. Il s'auto-élève (UAC) : une fenêtre admin s'ouvrira et
    // exécutera la configuration. On lance sans capturer (la fenêtre élevée est
    // indépendante) et on rend la main immédiatement.
    // CREATE_NO_WINDOW sur le lanceur : le script s'auto-élève (UAC) et ouvre
    // sa PROPRE fenêtre admin visible — inutile d'afficher la console du lanceur.
    let mut ps = Command::new("powershell");
    ps.args([
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", tmp.to_str().unwrap_or(""),
    ]);
    no_window(&mut ps)
        .spawn()
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))?;

    Ok("Configuration lancée. Une fenêtre administrateur (UAC) va s'ouvrir : \
        cliquez « Oui » et suivez la progression du pg_basebackup dans cette fenêtre. \
        Revenez ensuite au tableau de bord.".to_string())
}

fn find_standby_script(app: &tauri::AppHandle) -> Result<String, String> {
    // 1. Dans les ressources Tauri (bundle)
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("scripts").join("windows").join("03_setup_standby_win.ps1");
        if p.exists() {
            return std::fs::read_to_string(&p).map_err(|e| e.to_string());
        }
    }

    // 2. Même dossier que l'exe
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("03_setup_standby_win.ps1");
        if p.exists() {
            return std::fs::read_to_string(&p).map_err(|e| e.to_string());
        }
    }

    Err("Script de configuration standby non trouvé dans le bundle.".to_string())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn find_binary(app: &tauri::AppHandle) -> Option<String> {
    // 1. Sidecar dans le bundle Tauri (resource dir)
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("sovereign-node-active.exe");
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }

    // 2. Même dossier que l'exécutable courant
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("sovereign-node-active.exe");
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }

    // 3. PATH système (dev)
    if which_sovereign().is_some() {
        return Some("sovereign-node-active".into());
    }

    None
}

fn which_sovereign() -> Option<()> {
    no_window(Command::new("sovereign-node-active").arg("--help"))
        .output()
        .ok()
        .map(|_| ())
}

async fn check_pg_available() -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect("127.0.0.1:5432")
        .await
        .is_ok()
}

async fn ping_node(url: &str) -> bool {
    let health_url = format!("{url}/health");
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(&health_url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// ── Gestion du parc côté UI : crypto d'appareil (enrôlement / preuves) ─────────
//
// Ces commandes exécutent la crypto sodiumoxide (impossible en JS) pour rendre la
// démo d'enrôlement/rotation/récupération vérifiable depuis le tableau de bord.

/// Crée un « nouvel appareil » : génère sa paire X25519 et renvoie sa clé publique
/// (le « QR » à présenter au nœud actif pour l'enrôlement). La clé privée reste ici.
#[tauri::command]
fn dev_generate_keypair(vault: State<'_, DeviceVault>) -> Result<String, String> {
    let kp = DeviceKeypair::generate();
    let public_hex = kp.public_hex();
    vault.0.lock().map_err(|e| e.to_string())?.insert(public_hex.clone(), kp);
    Ok(public_hex)
}

/// L'appareil (identifié par sa clé publique) ouvre la sealed box reçue à l'enrôlement
/// et récupère la DEK (hex). Preuve du critère #8 : la clé n'a jamais transité en clair.
#[tauri::command]
fn dev_unwrap_dek(
    public_hex: String,
    sealed_hex: String,
    vault:      State<'_, DeviceVault>,
) -> Result<String, String> {
    let guard = vault.0.lock().map_err(|e| e.to_string())?;
    let kp = guard
        .get(&public_hex)
        .ok_or_else(|| "appareil inconnu (clé privée absente du coffre local)".to_string())?;
    kp.unwrap_dek_hex(&sealed_hex).map_err(|e| e.to_string())
}

/// Teste si une DEK (hex) déchiffre un blob (nonce + ciphertext hex).
/// Preuve du critère #9 : l'ancienne DEK d'un appareil dé-enrôlé échoue sur un blob
/// écrit APRÈS la rotation, alors qu'un appareil restant (nouvelle DEK) réussit.
#[tauri::command]
fn try_decrypt_blob(dek_hex: String, nonce_hex: String, ct_hex: String) -> bool {
    sovereign_core::try_decrypt_hex(&dek_hex, &nonce_hex, &ct_hex)
}

// ── Administration du cluster (failover manuel + mode de réplication) ──────────
//
// Ces commandes pilotent PostgreSQL via psql (couche base). Elles répondent à
// deux besoins PME explicitement manuels (cf. cadrage §4.6 bascule manuelle) :
//   - basculer un standby en primary (promotion) sur décision humaine ;
//   - choisir le mode de réplication synchrone/asynchrone selon le besoin.
// Aucune IP en dur : tout est local (127.0.0.1:5432, loopback de la machine).

/// Localise psql.exe (chemins d'installation usuels de PostgreSQL sous Windows).
fn find_psql() -> Option<String> {
    for ver in ["18", "17", "16", "15"] {
        let p = format!(r"C:\Program Files\PostgreSQL\{ver}\bin\psql.exe");
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    if no_window(Command::new("psql").arg("--version")).output().is_ok() {
        return Some("psql".into());
    }
    None
}

/// Exécute une requête scalaire sur le PostgreSQL LOCAL et renvoie la valeur brute.
fn psql_scalar(sql: &str) -> Result<String, String> {
    let psql = find_psql().ok_or_else(|| "psql introuvable (PostgreSQL non installé ?)".to_string())?;
    // Mot de passe superuser du spike (documenté, non-production).
    let pw = std::env::var("SOVEREIGN_PG_ADMIN_PW").unwrap_or_else(|_| "admin".to_string());
    let mut cmd = Command::new(&psql);
    cmd.env("PGPASSWORD", pw)
        .args(["-h", "127.0.0.1", "-U", "postgres", "-d", "sovereign_active",
               "-t", "-A", "-c", sql]);
    let out = no_window(&mut cmd)
        .output()
        .map_err(|e| format!("exécution psql : {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Debug, Serialize)]
pub struct ClusterStatus {
    pub role:              String, // "primary" | "standby"
    pub in_recovery:       bool,
    pub sync_enabled:      bool,   // synchronous_standby_names non vide
    pub sync_state:        String, // "sync" | "async" | "aucun" | "n/a"
    pub standby_connected: bool,
    pub standby_addr:      String, // IP du standby connecté (vu depuis le primary)
}

/// État du cluster vu depuis le PostgreSQL local (pour piloter les boutons et
/// afficher la VRAIE santé du standby = réplication PG, pas un HTTP fantôme).
#[tauri::command]
async fn cluster_status() -> Result<ClusterStatus, String> {
    let in_recovery = psql_scalar("SELECT pg_is_in_recovery()")? == "t";
    let ssn = psql_scalar("SHOW synchronous_standby_names").unwrap_or_default();
    let sync_enabled = !ssn.trim().is_empty();

    let (sync_state, standby_connected, standby_addr) = if in_recovery {
        ("n/a".to_string(), false, String::new())
    } else {
        let st = psql_scalar(
            "SELECT COALESCE(string_agg(DISTINCT sync_state, ','), '') FROM pg_stat_replication",
        ).unwrap_or_default();
        let addr = psql_scalar(
            "SELECT COALESCE(string_agg(host(client_addr), ','), '') FROM pg_stat_replication",
        ).unwrap_or_default();
        if st.is_empty() { ("aucun".to_string(), false, String::new()) } else { (st, true, addr) }
    };

    Ok(ClusterStatus {
        role: if in_recovery { "standby".into() } else { "primary".into() },
        in_recovery,
        sync_enabled,
        sync_state,
        standby_connected,
        standby_addr,
    })
}

/// Bascule le mode de réplication (bouton PME). `sync=true` → synchrone (zéro
/// perte, plus lent) ; `sync=false` → asynchrone (rapide, perte possible au
/// failover). `'*'` = n'importe quel standby (aucun nom de machine en dur).
#[tauri::command]
async fn set_replication_mode(sync: bool) -> Result<String, String> {
    let val = if sync { "*" } else { "" };
    psql_scalar(&format!("ALTER SYSTEM SET synchronous_standby_names = '{val}'"))?;
    psql_scalar("SELECT pg_reload_conf()")?;
    Ok(if sync {
        "Réplication SYNCHRONE activée — chaque écriture attend l'accusé d'un standby (zéro perte).".into()
    } else {
        "Réplication ASYNCHRONE activée — écritures confirmées immédiatement (plus rapide, perte possible au failover).".into()
    })
}

/// Promotion manuelle (bouton PME) : ce standby devient primary. Relâche la
/// réplication synchrone (plus de standby rattaché) et incrémente l'époque de
/// fencing pour neutraliser l'ancien primary (anti-split-brain).
#[tauri::command]
async fn promote_node() -> Result<String, String> {
    if psql_scalar("SELECT pg_is_in_recovery()")? != "t" {
        return Err("Cette machine est déjà primary (pas en réplication). Promotion inutile.".into());
    }
    psql_scalar("SELECT pg_promote(wait => true)")?;
    // Relâcher la sync : le nouveau primary n'a pas encore de standby rattaché.
    let _ = psql_scalar("ALTER SYSTEM SET synchronous_standby_names = ''");
    let _ = psql_scalar("SELECT pg_reload_conf()");
    // Incrémenter l'époque (fencing) — l'ancien primary deviendra obsolète.
    let epoch = psql_scalar(
        "UPDATE node_epoch SET epoch = epoch + 1, primary_host = 'promu-manuel', \
         promoted_at = NOW() WHERE id = 1 RETURNING epoch",
    )?;
    Ok(format!(
        "✓ Promotion réussie — cette machine est le nouveau primary (époque {epoch}). \
         Démarrez le nœud actif si nécessaire."
    ))
}

// ── Superviseur de quorum (failover automatique, LOT 5) ─────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SupervisorStatus {
    pub node_id:       String,
    pub role:          String,
    pub rank:          u32,
    pub cluster_size:  usize,
    pub promoted:      bool,
    pub primary_alive: bool,
    pub current_term:  u64,
    pub peers:         Vec<String>,
}

fn find_supervisor_binary(app: &tauri::AppHandle) -> Option<String> {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("sovereign-supervisor.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("sovereign-supervisor.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if no_window(Command::new("sovereign-supervisor").arg("--help")).output().is_ok() {
        return Some("sovereign-supervisor".into());
    }
    None
}

/// Démarre le superviseur de quorum (sidecar embarqué).
/// `role` = "primary" | "standby". `rank` = priorité de succession (0 = 1er).
/// `peers` = CSV "node_id@host:port". `primary_ip` = IP du primary actuel.
#[tauri::command]
async fn start_supervisor(
    node_id:    String,
    role:       String,
    rank:       u32,
    peers:      String,
    primary_ip: String,
    state:      State<'_, SupervisorProcess>,
    app:        tauri::AppHandle,
) -> Result<String, String> {
    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok("Superviseur déjà en cours".into()),
                _ => { *guard = None; } // mort → on relance
            }
        }
    }

    let role = match role.to_lowercase().as_str() {
        "primary" | "standby" => role.to_lowercase(),
        other => return Err(format!("rôle invalide : '{other}' (attendu primary|standby)")),
    };
    let host = primary_ip.trim()
        .trim_start_matches("https://").trim_start_matches("http://")
        .split(['/', ':']).next().unwrap_or("").to_string();
    if host.is_empty() {
        return Err("IP du primary requise".into());
    }

    let bin = find_supervisor_binary(&app)
        .ok_or_else(|| "Binaire sovereign-supervisor introuvable dans le bundle".to_string())?;

    let mut cmd = Command::new(&bin);
    cmd.env("SUP_NODE_ID",        &node_id)
        .env("SUP_ROLE",           &role)
        .env("SUP_RANK",           rank.to_string())
        .env("SUP_PEERS",          &peers)
        .env("SUP_PRIMARY_HEALTH", format!("http://{host}:3000/health"))
        .env("SUP_LOCAL_NODE_URL", "http://127.0.0.1:3000")
        .env("SUP_LISTEN_ADDR",    "0.0.0.0:3100");
    let child = no_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("Impossible de démarrer le superviseur : {e}"))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }
    Ok(format!("✓ Superviseur '{node_id}' démarré (rôle {role}, rang {rank})"))
}

/// Arrête le superviseur de quorum local.
#[tauri::command]
async fn stop_supervisor(state: State<'_, SupervisorProcess>) -> Result<String, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        Ok("Superviseur arrêté".into())
    } else {
        Ok("Superviseur déjà arrêté".into())
    }
}

/// Interroge le statut du superviseur local (HTTP sur :3100).
#[tauri::command]
async fn supervisor_status() -> Result<SupervisorStatus, String> {
    let r = reqwest::Client::new()
        .get("http://127.0.0.1:3100/supervisor/status")
        .timeout(Duration::from_secs(3))
        .send().await
        .map_err(|e| format!("superviseur injoignable : {e}"))?;
    r.json::<SupervisorStatus>().await.map_err(|e| e.to_string())
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // libsodium doit être initialisé avant toute opération crypto (gestion du parc).
    let _ = sovereign_core::crypto::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(NodeProcess(Mutex::new(None)))
        .manage(SupervisorProcess(Mutex::new(None)))
        .manage(DeviceVault(Mutex::new(HashMap::new())))
        .setup(|_app| {
            // Pas d'auto-start ici : le démarrage du nœud (actif / passif / relais)
            // est piloté par le frontend selon le RÔLE choisi (cf. App.tsx), avec
            // la bonne DEK. Un spawn générique sans DEK valide ferait planter le
            // nœud et « poisonnerait » l'état (slot occupé par un enfant mort).
            Ok(())
        })
        .on_window_event(|_window, event| {
            // Arrêter le nœud actif proprement à la fermeture de l'app
            if let tauri::WindowEvent::Destroyed = event {
                // Le processus enfant sera tué avec le parent (comportement OS)
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_startup_status,
            start_active_node,
            stop_active_node,
            ping_health,
            check_pg_local,
            run_standby_setup,
            start_relay_node,
            discover_nodes,
            cluster_status,
            set_replication_mode,
            promote_node,
            start_supervisor,
            stop_supervisor,
            supervisor_status,
            dev_generate_keypair,
            dev_unwrap_dek,
            try_decrypt_blob,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur lors du démarrage Tauri");
}
