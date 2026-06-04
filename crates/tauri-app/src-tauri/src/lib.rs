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

/// Exécute le script de configuration standby (embarqué dans le bundle).
/// Le script PowerShell est copié dans un fichier temporaire puis exécuté.
#[tauri::command]
async fn run_standby_setup(primary_ip: String, app: tauri::AppHandle) -> Result<String, String> {
    // Chercher le script embarqué dans les ressources Tauri
    let script_content = find_standby_script(&app)?;

    // Écrire dans un fichier temporaire
    let tmp = std::env::temp_dir().join("sovereign_setup_standby.ps1");
    std::fs::write(&tmp, &script_content)
        .map_err(|e| format!("Impossible d'écrire le script temporaire : {e}"))?;

    // Injecter l'IP du primary si fournie
    let content_with_ip = script_content.replace(
        r#"$PRIMARY_IP   = "192.168.200.1""#,
        &format!(r#"$PRIMARY_IP   = "{primary_ip}""#),
    );
    std::fs::write(&tmp, content_with_ip)
        .map_err(|e| format!("Erreur écriture script : {e}"))?;

    // Exécuter PowerShell avec le script
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", tmp.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(format!("Script terminé avec succès.\n{stdout}"))
    } else {
        Err(format!("Script échoué :\n{stderr}\n{stdout}"))
    }
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
        ])
        .run(tauri::generate_context!())
        .expect("Erreur lors du démarrage Tauri");
}
