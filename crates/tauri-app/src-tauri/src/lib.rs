//! Backend Tauri — commandes exposées au frontend React.
//!
//! Architecture : le frontend appelle les endpoints HTTP du nœud actif directement
//! (via fetch dans le navigateur WebView). Les commandes Tauri ici servent pour :
//!   - Démarrer / arrêter le nœud actif en sidecar
//!   - Lire la configuration locale (shared.env)
//!   - Opérations nécessitant l'accès au système de fichiers

use std::process::{Child, Command};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

// ── État de l'application ──────────────────────────────────────────────────────

struct NodeProcess(Mutex<Option<Child>>);

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    pub dek_hex:    String,
    pub relay_url:  String,
    pub relay_key:  String,
    pub listen_addr: String,
    pub db_url:      String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            dek_hex:     String::new(),
            relay_url:   String::new(),
            relay_key:   "sovereign-spike-relay-key-2026".into(),
            listen_addr: "0.0.0.0:3000".into(),
            db_url:      "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active".into(),
        }
    }
}

// ── Commandes Tauri ───────────────────────────────────────────────────────────

/// Charge la configuration depuis shared.env.
#[tauri::command]
fn load_config(app: tauri::AppHandle) -> Result<NodeConfig, String> {
    let env_path = app
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("scripts").join("shared.env"))
        .filter(|p| p.exists());

    let mut cfg = NodeConfig::default();

    if let Some(path) = env_path {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Lecture shared.env : {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || !line.contains('=') { continue; }
            let mut parts = line.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim();
            let val = parts.next().unwrap_or("").trim();
            match key {
                "SOVEREIGN_DEK_HEX" => cfg.dek_hex    = val.to_string(),
                "RELAY_URL"         => cfg.relay_url   = val.to_string(),
                "RELAY_API_KEY"     => cfg.relay_key   = val.to_string(),
                _ => {}
            }
        }
    }

    Ok(cfg)
}

/// Démarre le nœud actif en sous-processus.
#[tauri::command]
fn start_node(
    config: NodeConfig,
    state:  State<'_, NodeProcess>,
    app:    tauri::AppHandle,
) -> Result<String, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok("Nœud actif déjà démarré".into());
    }

    // Trouver le binaire sovereign-node-active (sidecar ou dans PATH)
    let bin = app
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("sovereign-node-active.exe"))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "sovereign-node-active".into());

    let child = Command::new(&bin)
        .env("DATABASE_URL",      &config.db_url)
        .env("LISTEN_ADDR",       &config.listen_addr)
        .env("SOVEREIGN_DEK_HEX", &config.dek_hex)
        .env("RELAY_URL",         &config.relay_url)
        .env("RELAY_API_KEY",     &config.relay_key)
        .spawn()
        .map_err(|e| format!("Impossible de démarrer {bin} : {e}"))?;

    *guard = Some(child);
    Ok(format!("Nœud actif démarré ({})", bin))
}

/// Arrête le nœud actif.
#[tauri::command]
fn stop_node(state: State<'_, NodeProcess>) -> Result<String, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = guard.take() {
        child.kill().map_err(|e| format!("Arrêt échoué : {e}"))?;
        Ok("Nœud actif arrêté".into())
    } else {
        Ok("Nœud actif n'était pas démarré".into())
    }
}

/// Vérifie si le nœud actif répond.
#[tauri::command]
async fn check_node_health() -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;

    match client.get("http://127.0.0.1:3000/health").send().await {
        Ok(r) => Ok(r.status().is_success()),
        Err(_) => Ok(false),
    }
}

// ── Point d'entrée Tauri ──────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(NodeProcess(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            load_config,
            start_node,
            stop_node,
            check_node_health,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur lors du démarrage de l'application Tauri");
}
