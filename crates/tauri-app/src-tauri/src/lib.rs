//! Backend Tauri — Sovereign Data Agent.
//!
//! Au démarrage, l'app :
//!   1. Cherche sovereign-node-active (sidecar embarqué ou dans PATH)
//!   2. Vérifie si PostgreSQL local est disponible
//!   3. Lance le nœud actif automatiquement si PostgreSQL est là
//!   4. Sinon : mode CLIENT — se connecte à un nœud actif distant
//!
//! La PME n'a rien à faire manuellement.

use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

// ── État ──────────────────────────────────────────────────────────────────────

struct NodeProcess(Mutex<Option<Child>>);

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
        message:    "Mode client — configurez l'URL du nœud actif".into(),
        active_url: "http://192.168.200.1:3000".into(),
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
    // Déjà démarré ?
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Ok(StartupStatus {
                mode:       "local".into(),
                message:    "Nœud actif déjà en cours".into(),
                active_url: "http://127.0.0.1:3000".into(),
            });
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
            active_url: "http://192.168.200.1:3000".into(),
        });
    };

    // Vérifier PostgreSQL (tentative de connexion TCP sur port 5432)
    let pg_ok = check_pg_available().await;
    if !pg_ok {
        return Ok(StartupStatus {
            mode:       "remote".into(),
            message:    "PostgreSQL non détecté localement — mode client. Vérifiez que PostgreSQL 18 est installé et démarré.".into(),
            active_url: "http://192.168.200.1:3000".into(),
        });
    }

    // Lancer le nœud actif
    let child = Command::new(&bin)
        .env("DATABASE_URL",      if db_url.is_empty() { "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active" } else { &db_url })
        .env("LISTEN_ADDR",       "0.0.0.0:3000")
        .env("SOVEREIGN_DEK_HEX", &dek_hex)
        .env("RELAY_URL",         &relay_url)
        .env("RELAY_API_KEY",     if relay_key.is_empty() { "sovereign-spike-relay-key-2026" } else { &relay_key })
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

/// Détecte le sous-réseau LAN réel de la machine (ex: "192.168.200").
/// Astuce : on "connecte" un socket UDP vers une IP du réseau VMnet1 ; aucun
/// paquet n'est envoyé, mais l'OS choisit l'interface sortante, ce qui donne
/// l'IP locale de cette interface (ex: 192.168.200.1). On exclut le loopback.
fn detect_lan_subnet() -> Option<String> {
    use std::net::UdpSocket;
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Cible VMnet1 ; route le socket vers l'interface 192.168.200.x sans trafic réel.
    sock.connect("192.168.200.1:9").ok()?;
    let ip = sock.local_addr().ok()?.ip().to_string();
    if ip.starts_with("127.") {
        return None;
    }
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

/// Découverte réseau : scanne le sous-réseau VMnet1 (192.168.200.0/24)
/// pour trouver les nœuds souverains actifs, passifs et relais.
///
/// Conforme au document de cadrage §6 : la découverte locale automatique évite
/// à la PME de saisir des adresses IP. (Le spike scanne au lieu d'utiliser mDNS,
/// explicitement hors-périmètre Phase 0 §7.5 — même résultat ergonomique.)
#[tauri::command]
async fn discover_nodes(subnet: Option<String>) -> Vec<DiscoveredNode> {
    // Ne JAMAIS scanner le loopback (127.x) : sous Windows tout 127.0.0.0/8 est
    // du loopback et un noeud lie sur 0.0.0.0 repond sur chaque 127.0.0.x -> 254
    // faux positifs. Si le sous-reseau demande est vide/loopback, on auto-detecte
    // le vrai sous-reseau LAN de la machine (interface VMnet1).
    let base = match subnet {
        Some(s) if !s.is_empty() && !s.starts_with("127.") => s,
        _ => detect_lan_subnet().unwrap_or_else(|| "192.168.200".to_string()),
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
    found.sort_by(|a, b| a.port.cmp(&b.port));
    found
}

/// Démarre le nœud SOLO (SQLite, sans PostgreSQL).
/// Mode TPE/PME mono-poste : aucune dépendance serveur.
#[tauri::command]
async fn start_solo_node(
    dek_hex: String,
    state:   State<'_, NodeProcess>,
    app:     tauri::AppHandle,
) -> Result<StartupStatus, String> {
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Ok(StartupStatus {
                mode:       "solo".into(),
                message:    "Nœud solo déjà en cours".into(),
                active_url: "http://127.0.0.1:3000".into(),
            });
        }
    }

    // Trouver le binaire solo (sidecar embarqué)
    let bin = find_solo_binary(&app).ok_or_else(||
        "Binaire sovereign-node-solo introuvable dans le bundle".to_string())?;

    // Base SQLite dans le dossier de données utilisateur (persistance)
    let db_path = app.path().app_data_dir().ok()
        .map(|d| { let _ = std::fs::create_dir_all(&d); d.join("sovereign_solo.db").to_string_lossy().to_string() })
        .unwrap_or_else(|| "sovereign_solo.db".to_string());

    let child = Command::new(&bin)
        .env("SOLO_DB_PATH",      &db_path)
        .env("LISTEN_ADDR",       "127.0.0.1:3000")
        .env("SOVEREIGN_DEK_HEX", &dek_hex)
        .spawn()
        .map_err(|e| format!("Impossible de démarrer le nœud solo : {e}"))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }

    // Attendre que le nœud réponde
    for _ in 0..16 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if ping_node("http://127.0.0.1:3000").await {
            return Ok(StartupStatus {
                mode:       "solo".into(),
                message:    "✓ Nœud solo démarré (SQLite, sans PostgreSQL)".into(),
                active_url: "http://127.0.0.1:3000".into(),
            });
        }
    }

    Ok(StartupStatus {
        mode:       "solo".into(),
        message:    "Nœud solo lancé (démarrage en cours…)".into(),
        active_url: "http://127.0.0.1:3000".into(),
    })
}

fn find_solo_binary(app: &tauri::AppHandle) -> Option<String> {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("sovereign-node-solo.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if let Ok(exe) = std::env::current_exe() {
        let p = exe.parent().unwrap_or(std::path::Path::new("."))
            .join("sovereign-node-solo.exe");
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    if Command::new("sovereign-node-solo").arg("--help").output().is_ok() {
        return Some("sovereign-node-solo".into());
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
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", tmp.to_str().unwrap_or(""),
        ])
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
    Command::new("sovereign-node-active")
        .arg("--help")
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
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()
        .map(|c| c.get(&health_url).send())
        .map(|f| tokio::runtime::Handle::current().block_on(async { f.await.map(|r| r.status().is_success()).unwrap_or(false) }))
        .unwrap_or(false)
}

// ── Setup hook — auto-démarrage ───────────────────────────────────────────────

fn auto_start_node(app: &tauri::AppHandle) {
    // Lire la config depuis localStorage n'est pas accessible ici (WebView pas encore chargé).
    // On détermine : si PostgreSQL local disponible → tenter de démarrer.
    // Le frontend est notifié via l'état get_startup_status.
    let app = app.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if !check_pg_available().await {
                return; // Mode client — le frontend gérera
            }
            let bin = find_binary(&app);
            let Some(bin) = bin else { return; };

            // Lire la DEK depuis shared.env si disponible
            let dek = read_dek_from_env(&app).unwrap_or_default();

            let child = Command::new(&bin)
                .env("DATABASE_URL",      "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active")
                .env("LISTEN_ADDR",       "0.0.0.0:3000")
                .env("SOVEREIGN_DEK_HEX", &dek)
                .spawn();

            if let Ok(child) = child {
                let state = app.state::<NodeProcess>();
                let mut guard = state.0.lock().unwrap();
                *guard = Some(child);
                eprintln!("[sovereign-tauri] Nœud actif démarré automatiquement");
            }
        });
    });
}

fn read_dek_from_env(app: &tauri::AppHandle) -> Option<String> {
    // Chercher shared.env à côté de l'exe
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidates = [
        exe_dir.join("shared.env"),
        exe_dir.join("scripts").join("shared.env"),
        app.path().resource_dir().ok()?.join("shared.env"),
    ];

    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("SOVEREIGN_DEK_HEX=") {
                    return Some(val.trim().to_string());
                }
            }
        }
    }
    None
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(NodeProcess(Mutex::new(None)))
        .setup(|app| {
            // Auto-démarrage du nœud actif si PostgreSQL local est disponible
            auto_start_node(app.handle());
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
            start_solo_node,
            discover_nodes,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur lors du démarrage Tauri");
}
