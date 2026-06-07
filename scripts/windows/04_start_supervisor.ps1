# =============================================================================
# 04_start_supervisor.ps1 - Superviseur de quorum (failover automatique, LOT 5)
#
# A lancer sur CHAQUE machine du cluster (PC primary + VM1 + VM2), APRES que
# le noeud actif/standby PostgreSQL de la machine tourne deja.
#
# Le superviseur :
#   - surveille le primary par heartbeat,
#   - demande un vote de quorum aux pairs si le primary tombe,
#   - ne promeut QUE si la majorite stricte est atteinte (anti-split-brain),
#   - declenche pg_ctl promote + increment d'epoque (fencing) sur le gagnant.
#
# Pas de Patroni, pas d'etcd : quorum natif Rust, 100% Windows.
#
# Usage :
#   .\04_start_supervisor.ps1 -Role primary -NodeId pc
#   .\04_start_supervisor.ps1 -Role standby -NodeId vm1 -Rank 0
#   .\04_start_supervisor.ps1 -Role standby -NodeId vm2 -Rank 1
#
# NOTE: ASCII pur (pas d'accents) pour eviter toute corruption d'encodage.
# =============================================================================

param(
    [Parameter(Mandatory = $true)][ValidateSet("primary", "standby")]
    [string]$Role,

    [Parameter(Mandatory = $true)]
    [string]$NodeId,

    [int]$Rank = 0,

    # IP du primary (la machine qui detient le PostgreSQL primary au demarrage)
    [string]$PrimaryIp = "192.168.200.1",

    # Pairs : "node_id@host:port" separes par des virgules.
    # Par defaut : le cluster a 3 noeuds (pc, vm1, vm2) sur le port superviseur 3100.
    # On laisse vide ici pour forcer une saisie explicite et eviter les erreurs.
    [string]$Peers = "",

    # Adresse d'ecoute du superviseur sur CETTE machine.
    [string]$ListenAddr = "0.0.0.0:3100",

    # Seuil d'echecs consecutifs avant de juger le primary mort.
    [int]$FailThreshold = 3,

    # Periode de heartbeat en secondes.
    [int]$HeartbeatSecs = 3
)

$SPIKE_DIR = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Write-Host "=== sovereign-spike :: Superviseur de quorum (LOT 5) ===" -ForegroundColor Cyan
Write-Host "  Node ID    : $NodeId"
Write-Host "  Role       : $Role (rang $Rank)"
Write-Host "  Primary IP : $PrimaryIp"
Write-Host "  Listen     : $ListenAddr"
Write-Host ""

if ([string]::IsNullOrWhiteSpace($Peers)) {
    Write-Warning "Aucun pair (-Peers) fourni. Le superviseur fonctionnera en cluster d'1 noeud."
    Write-Warning "Exemple : -Peers 'vm1@192.168.200.2:3100,vm2@192.168.200.3:3100'"
}

# ---- Build release ----------------------------------------------------------
Write-Host "[1/3] Compilation du superviseur (release)..." -ForegroundColor Yellow
Push-Location $SPIKE_DIR
cargo build --release --bin sovereign-supervisor 2>&1
if ($LASTEXITCODE -ne 0) { Write-Error "Compilation echouee"; Pop-Location; exit 1 }
Write-Host "  OK" -ForegroundColor Green

# ---- Variables d'environnement ----------------------------------------------
Write-Host "[2/3] Configuration..." -ForegroundColor Yellow

$env:SUP_NODE_ID        = $NodeId
$env:SUP_ROLE           = $Role
$env:SUP_RANK           = "$Rank"
$env:SUP_PEERS          = $Peers
$env:SUP_PRIMARY_HEALTH = "http://${PrimaryIp}:3000/health"
$env:SUP_LOCAL_NODE_URL = "http://127.0.0.1:3000"
$env:SUP_LISTEN_ADDR    = $ListenAddr
$env:SUP_FAIL_THRESHOLD = "$FailThreshold"
$env:SUP_HEARTBEAT_SECS = "$HeartbeatSecs"
$env:PG_BIN             = "C:\Program Files\PostgreSQL\18\bin"
$env:PG_DATA            = "C:\Program Files\PostgreSQL\18\data"

Write-Host "  SUP_PRIMARY_HEALTH = $env:SUP_PRIMARY_HEALTH"
Write-Host "  SUP_PEERS          = $env:SUP_PEERS"
Write-Host "  Seuil de panne     = $FailThreshold echecs x $HeartbeatSecs s"
Write-Host "  OK" -ForegroundColor Green

# ---- Demarrage --------------------------------------------------------------
Write-Host ""
Write-Host "[3/3] Demarrage du superviseur..." -ForegroundColor Yellow
Write-Host "  Endpoints :"
Write-Host "    GET  http://<cette-machine>:3100/health"
Write-Host "    GET  http://<cette-machine>:3100/supervisor/status"
Write-Host "    POST http://<cette-machine>:3100/vote   (interne au cluster)"
Write-Host ""
Write-Host "  Ctrl+C pour arreter." -ForegroundColor Yellow
Write-Host "-------------------------------------------------------------" -ForegroundColor DarkGray

.\target\release\sovereign-supervisor.exe

Pop-Location
