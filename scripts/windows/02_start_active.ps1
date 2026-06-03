# =============================================================================
# 02_start_active.ps1 — Démarrage du nœud actif souverain (Windows PRIMARY)
# Machine : Windows 11 physique — 192.168.200.1
# Ports   : HTTP 3000 (nœud actif)
# =============================================================================

$SPIKE_DIR = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$ENV_FILE  = "$PSScriptRoot\..\shared.env"

Write-Host "=== sovereign-spike :: Nœud ACTIF (Windows 192.168.200.1) ===" -ForegroundColor Cyan

# ── Build release ─────────────────────────────────────────────────────────────
Write-Host "`n[1/3] Compilation en mode release..." -ForegroundColor Yellow
Push-Location $SPIKE_DIR
cargo build --release --bin sovereign-node-active 2>&1
if ($LASTEXITCODE -ne 0) { Write-Error "Compilation échouée"; exit 1 }
Write-Host "OK" -ForegroundColor Green

# ── Chargement des variables d'environnement ─────────────────────────────────
Write-Host "`n[2/3] Chargement des variables d'environnement..." -ForegroundColor Yellow

if (Test-Path $ENV_FILE) {
    Get-Content $ENV_FILE | Where-Object { $_ -match '^\s*[^#]' -and $_ -match '=' } | ForEach-Object {
        $parts = $_ -split '=', 2
        [System.Environment]::SetEnvironmentVariable($parts[0].Trim(), $parts[1].Trim(), "Process")
    }
    Write-Host "shared.env chargé." -ForegroundColor Green
} else {
    Write-Warning "shared.env introuvable. La DEK sera générée automatiquement (mode premier démarrage)."
    Write-Warning "Récupérer la valeur 'dek_hex=...' dans les logs et la copier dans shared.env."
}

# Variables d'environnement du nœud actif
$env:DATABASE_URL  = "postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active"
$env:LISTEN_ADDR   = "0.0.0.0:3000"
# SOVEREIGN_DEK_HEX : chargée depuis shared.env (ou générée si absente)

Write-Host "Config :"
Write-Host "  DATABASE_URL  = $env:DATABASE_URL"
Write-Host "  LISTEN_ADDR   = $env:LISTEN_ADDR"
if ($env:SOVEREIGN_DEK_HEX) {
    Write-Host "  DEK           = $($env:SOVEREIGN_DEK_HEX.Substring(0,8))..." -ForegroundColor Green
} else {
    Write-Host "  DEK           = (génération automatique)" -ForegroundColor Yellow
}

# ── Démarrage ────────────────────────────────────────────────────────────────
Write-Host "`n[3/3] Démarrage du nœud actif..." -ForegroundColor Yellow
Write-Host "  Endpoints disponibles :"
Write-Host "    GET  http://192.168.200.1:3000/health"
Write-Host "    POST http://192.168.200.1:3000/write"
Write-Host "    GET  http://192.168.200.1:3000/stock/{item_id}"
Write-Host "    GET  http://192.168.200.1:3000/journal"
Write-Host "    GET  http://192.168.200.1:3000/epoch"
Write-Host ""
Write-Host "Ctrl+C pour arrêter." -ForegroundColor Yellow
Write-Host "─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray

.\target\release\sovereign-node-active.exe
