# =============================================================================
# 00_install_all.ps1 — Installeur one-click Sovereign Data Agent (Windows)
# Exécuter en tant qu'Administrateur.
#
# Ce script installe et configure tout automatiquement :
#   - PostgreSQL 18 (si absent)
#   - Rust toolchain (si absent)
#   - Base de données sovereign_active
#   - Service Windows (démarrage automatique)
#   - Nœud actif sovereign-node-active
#
# La PME n'a besoin d'aucune connaissance technique.
# =============================================================================
#Requires -RunAsAdministrator
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$SPIKE_DIR    = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$PG_VERSION   = "18"
$PG_BIN       = "C:\Program Files\PostgreSQL\$PG_VERSION\bin"
$PG_DATA      = "C:\Program Files\PostgreSQL\$PG_VERSION\data"
$SERVICE_NAME = "SovereignDataAgent"
$INSTALL_LOG  = "$env:TEMP\sovereign_install.log"

function Write-Step($n, $msg) {
    Write-Host "`n[$n] $msg" -ForegroundColor Cyan
}
function Write-OK   { Write-Host "  OK" -ForegroundColor Green }
function Write-Skip { Write-Host "  Déjà installé — ignoré" -ForegroundColor Yellow }

Start-Transcript -Path $INSTALL_LOG -Append | Out-Null

Write-Host @"
╔══════════════════════════════════════════════════════╗
║    Sovereign Data Agent — Installation automatique   ║
║    Framework Coffre-Fort Data Souverain pour PME     ║
╚══════════════════════════════════════════════════════╝
"@ -ForegroundColor Magenta

# ── Étape 1 : Vérifier PostgreSQL ─────────────────────────────────────────────
Write-Step "1/6" "Vérification de PostgreSQL $PG_VERSION..."

if (Test-Path "$PG_BIN\psql.exe") {
    Write-Skip
} else {
    Write-Host "  PostgreSQL $PG_VERSION absent — téléchargement..." -ForegroundColor Yellow

    $pg_installer = "$env:TEMP\postgresql-18-installer.exe"
    $pg_url = "https://get.enterprisedb.com/postgresql/postgresql-18.0-1-windows-x64.exe"

    Write-Host "  Téléchargement de $pg_url ..."
    $wc = New-Object System.Net.WebClient
    $wc.DownloadFile($pg_url, $pg_installer)

    Write-Host "  Installation silencieuse (peut prendre 2-3 minutes)..."
    $pg_pass = "sovereign_admin_$(Get-Random -Max 9999)"
    Start-Process -FilePath $pg_installer -ArgumentList @(
        "--mode", "unattended",
        "--superpassword", $pg_pass,
        "--serverport", "5432",
        "--unattendedmodeui", "minimal"
    ) -Wait

    # Sauvegarder le mot de passe admin généré
    Set-Content "$SPIKE_DIR\pg_admin_password.txt" $pg_pass
    Write-Host "  ⚠ Mot de passe PostgreSQL sauvegardé dans : $SPIKE_DIR\pg_admin_password.txt"
    Write-OK
}

# ── Étape 2 : Vérifier Rust ───────────────────────────────────────────────────
Write-Step "2/6" "Vérification de Rust..."

$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (Test-Path $cargo) {
    Write-Skip
} else {
    Write-Host "  Rust absent — installation via rustup..." -ForegroundColor Yellow
    $rustup = "$env:TEMP\rustup-init.exe"
    $wc = New-Object System.Net.WebClient
    $wc.DownloadFile("https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe", $rustup)
    Start-Process -FilePath $rustup -ArgumentList "-y", "--no-modify-path" -Wait
    Write-OK
}

# ── Étape 3 : Compiler le nœud actif ──────────────────────────────────────────
Write-Step "3/6" "Compilation du nœud souverain (peut prendre 2-5 minutes)..."

$binary = "$SPIKE_DIR\target\release\sovereign-node-active.exe"
if (Test-Path $binary) {
    Write-Skip
} else {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    Push-Location $SPIKE_DIR
    & cargo build --release --bin sovereign-node-active
    Pop-Location
    Write-OK
}

# ── Étape 4 : Configurer PostgreSQL ───────────────────────────────────────────
Write-Step "4/6" "Configuration de la base de données..."

# Lire le mot de passe admin
$pg_pass_file = "$SPIKE_DIR\pg_admin_password.txt"
if (Test-Path $pg_pass_file) {
    $pg_admin_pass = Get-Content $pg_pass_file
} else {
    # PostgreSQL déjà installé — demander le mot de passe
    $pg_admin_pass = Read-Host "Mot de passe du superuser PostgreSQL (postgres)"
}

$env:PGPASSWORD = $pg_admin_pass
$psql = "$PG_BIN\psql.exe"

# Générer la DEK si absente
$env_file = "$SPIKE_DIR\scripts\shared.env"
if (-not (Test-Path $env_file)) {
    $dek = -join ((1..32) | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
    $relay_key = "sovereign-$(Get-Random -Max 999999)"
    @"
SOVEREIGN_DEK_HEX=$dek
RELAY_API_KEY=$relay_key
ACTIVE_NODE_URL=http://localhost:3000
RELAY_URL=
"@ | Set-Content $env_file -Encoding UTF8
    Write-Host "  DEK générée et sauvegardée dans shared.env" -ForegroundColor Green
}

# Exécuter le script SQL
try {
    & $psql -U postgres -h 127.0.0.1 -p 5432 `
        -f "$SPIKE_DIR\config\setup_replication.sql" 2>&1 | Out-Null
} catch {
    Write-Host "  (Base déjà configurée — OK)" -ForegroundColor Yellow
}

Write-OK

# ── Étape 5 : Créer le service Windows ────────────────────────────────────────
Write-Step "5/6" "Création du service Windows (démarrage automatique)..."

# Charger les variables d'environnement
Get-Content $env_file | Where-Object { $_ -match '=' -and $_ -notmatch '^#' } | ForEach-Object {
    $parts = $_ -split '=', 2
    [System.Environment]::SetEnvironmentVariable($parts[0].Trim(), $parts[1].Trim(), "Machine")
}

$existing = Get-Service -Name $SERVICE_NAME -ErrorAction SilentlyContinue
if ($existing) {
    Stop-Service -Name $SERVICE_NAME -Force -ErrorAction SilentlyContinue
    & sc.exe delete $SERVICE_NAME | Out-Null
}

# Créer un wrapper de lancement
$launcher = "$SPIKE_DIR\scripts\windows\start_service.bat"
@"
@echo off
set DATABASE_URL=postgres://sovereign:sovereign@127.0.0.1:5432/sovereign_active
set LISTEN_ADDR=0.0.0.0:3000
for /f "tokens=2 delims==" %%a in ('findstr SOVEREIGN_DEK_HEX "$env_file"') do set SOVEREIGN_DEK_HEX=%%a
for /f "tokens=2 delims==" %%a in ('findstr RELAY_URL "$env_file"') do set RELAY_URL=%%a
for /f "tokens=2 delims==" %%a in ('findstr RELAY_API_KEY "$env_file"') do set RELAY_API_KEY=%%a
"$binary"
"@ | Set-Content $launcher -Encoding ASCII

New-Service `
    -Name        $SERVICE_NAME `
    -DisplayName "Sovereign Data Agent — Nœud Actif" `
    -Description "Framework souveraineté données PME — EIGSI/AL BARAA CONSULTING" `
    -BinaryPathName $launcher `
    -StartupType Automatic | Out-Null

Start-Service -Name $SERVICE_NAME
Start-Sleep -Seconds 3

Write-OK

# ── Étape 6 : Validation ──────────────────────────────────────────────────────
Write-Step "6/6" "Validation du déploiement..."

try {
    $health = Invoke-RestMethod http://localhost:3000/health -TimeoutSec 10
    if ($health -eq "ok") {
        Write-Host "  Nœud actif : OK" -ForegroundColor Green
    }
} catch {
    Write-Host "  ⚠ Nœud actif pas encore disponible — vérifier dans 10s" -ForegroundColor Yellow
}

# Règles pare-feu
try {
    New-NetFirewallRule -DisplayName "Sovereign-Active-3000" `
        -Direction Inbound -Protocol TCP -LocalPort 3000 -Action Allow `
        -ErrorAction SilentlyContinue | Out-Null
    New-NetFirewallRule -DisplayName "Sovereign-PG-5432" `
        -Direction Inbound -Protocol TCP -LocalPort 5432 -Action Allow `
        -ErrorAction SilentlyContinue | Out-Null
} catch {}

# ── Résumé ─────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Installation terminée !" -ForegroundColor Green
Write-Host ""
Write-Host "  Nœud actif    : http://localhost:3000/health"
Write-Host "  Service       : $SERVICE_NAME (démarrage automatique)"
Write-Host "  Logs          : $INSTALL_LOG"
Write-Host "  Config        : $env_file"
Write-Host ""
Write-Host "  Pour ajouter un nœud passif (Ubuntu) :"
Write-Host "    sudo bash /opt/sovereign-spike/scripts/ubuntu/02_setup_standby.sh"
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Magenta

Stop-Transcript | Out-Null
