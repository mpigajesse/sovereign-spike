# =============================================================================
# 03_setup_standby_win.ps1 — Configuration du standby PostgreSQL sur Windows 11 VM
# Machine  : Windows 11 VM — à adapter selon l'IP réelle (remplacer STANDBY_IP)
# Primary  : Windows 11 physique — 192.168.200.1:5432
#
# À exécuter UNE SEULE FOIS en tant qu'Administrateur sur la VM Windows 11.
# Ce script :
#   1. Vérifie que PostgreSQL 18 est installé sur la VM
#   2. Clone le primary via pg_basebackup
#   3. Configure postgresql.auto.conf pour la réplication streaming
#   4. Crée standby.signal
#   5. Démarre PostgreSQL en mode standby
# =============================================================================
#Requires -RunAsAdministrator

$PRIMARY_IP   = "192.168.200.1"
$PRIMARY_PORT = "5432"
$STANDBY_NAME = "sovereign_standby_win"   # nom unique pour ce standby Windows
$PG_BIN       = "C:\Program Files\PostgreSQL\18\bin"
$PG_DATA      = "C:\Program Files\PostgreSQL\18\data"
$PG_SLOT      = "sovereign_slot_win"      # slot dédié à ce standby

# ── Vérifier l'IP locale (VMnet1) ─────────────────────────────────────────────
$STANDBY_IP = (Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -like "192.168.200.*" } |
    Select-Object -First 1).IPAddress

if (-not $STANDBY_IP) {
    Write-Error "Aucune adresse 192.168.200.x trouvée. Vérifier la connexion VMnet1."
    exit 1
}

Write-Host "=== Sovereign-Spike :: Setup PostgreSQL STANDBY (Windows 11 VM) ===" -ForegroundColor Cyan
Write-Host "Primary : $PRIMARY_IP:$PRIMARY_PORT"
Write-Host "Standby : $STANDBY_IP (cette machine)"
Write-Host "Slot    : $PG_SLOT"
Write-Host ""

# ── Vérifier psql ─────────────────────────────────────────────────────────────
Write-Host "[1/5] Vérification de PostgreSQL 18..." -ForegroundColor Yellow
$psql = "$PG_BIN\psql.exe"
if (-not (Test-Path $psql)) {
    Write-Error "PostgreSQL 18 non trouvé dans $PG_BIN"
    Write-Host "  Télécharger depuis https://www.postgresql.org/download/windows/"
    exit 1
}
Write-Host "  OK — $(& $psql --version)"

# ── Vérifier la connectivité au primary ───────────────────────────────────────
Write-Host ""
Write-Host "[2/5] Test de connectivité avec le primary ($PRIMARY_IP:$PRIMARY_PORT)..." -ForegroundColor Yellow
$conn = Test-NetConnection -ComputerName $PRIMARY_IP -Port $PRIMARY_PORT -InformationLevel Quiet
if (-not $conn) {
    Write-Error "Impossible de joindre $PRIMARY_IP:$PRIMARY_PORT"
    Write-Host "  Vérifier que PostgreSQL tourne sur Win11 physique et que le pare-feu autorise le port 5432."
    exit 1
}
Write-Host "  OK — primary joignable"

# ── Créer le slot de réplication sur le primary ────────────────────────────────
Write-Host ""
Write-Host "[2b] Création du slot de réplication '$PG_SLOT' sur le primary..." -ForegroundColor Yellow
$env:PGPASSWORD = "replicator_spike"
try {
    & $psql -U replicator -h $PRIMARY_IP -p $PRIMARY_PORT `
        -d "replication" `
        -c "SELECT pg_create_physical_replication_slot('$PG_SLOT');" `
        2>&1 | Out-Null
    Write-Host "  Slot '$PG_SLOT' créé (ou déjà existant)"
} catch {
    Write-Host "  Slot peut-être déjà existant — OK" -ForegroundColor Yellow
}

# ── Arrêter PostgreSQL local ──────────────────────────────────────────────────
Write-Host ""
Write-Host "[3/5] Arrêt du PostgreSQL local..." -ForegroundColor Yellow
Stop-Service -Name "postgresql-x64-18" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
Write-Host "  OK"

# ── Vider le répertoire data (pg_basebackup l'exige vide) ─────────────────────
Write-Host ""
Write-Host "[3b] Nettoyage du répertoire data..." -ForegroundColor Yellow
if (Test-Path $PG_DATA) {
    $confirm = Read-Host "  ATTENTION : $PG_DATA sera effacé. Continuer ? (o/N)"
    if ($confirm -ne "o" -and $confirm -ne "O") { exit 1 }
    Remove-Item "$PG_DATA\*" -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host "  OK"

# ── pg_basebackup ─────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "[4/5] pg_basebackup depuis le primary ($PRIMARY_IP)..." -ForegroundColor Yellow
Write-Host "  Mot de passe : replicator_spike"

$env:PGPASSWORD = "replicator_spike"
& "$PG_BIN\pg_basebackup.exe" `
    -h $PRIMARY_IP `
    -p $PRIMARY_PORT `
    -U replicator `
    -D $PG_DATA `
    --wal-method=stream `
    --slot=$PG_SLOT `
    --progress `
    --verbose

if ($LASTEXITCODE -ne 0) {
    Write-Error "pg_basebackup échoué (code $LASTEXITCODE)"
    exit 1
}
Write-Host "  pg_basebackup terminé." -ForegroundColor Green

# ── Configurer la réplication streaming ───────────────────────────────────────
Write-Host ""
Write-Host "[5/5] Configuration de postgresql.auto.conf..." -ForegroundColor Yellow

$autoconf = "$PG_DATA\postgresql.auto.conf"
$conninfo = "host=$PRIMARY_IP port=$PRIMARY_PORT user=replicator password=replicator_spike application_name=$STANDBY_NAME"

@"
# Généré par sovereign-spike setup (Windows standby)
primary_conninfo = '$conninfo'
primary_slot_name = '$PG_SLOT'
hot_standby = on
hot_standby_feedback = on
"@ | Set-Content $autoconf -Encoding UTF8

# Créer standby.signal (PostgreSQL 12+)
New-Item -Path "$PG_DATA\standby.signal" -ItemType File -Force | Out-Null
Write-Host "  standby.signal créé"

# ── Démarrer PostgreSQL en mode standby ───────────────────────────────────────
Start-Service -Name "postgresql-x64-18"
Start-Sleep -Seconds 4

$env:PGPASSWORD = "admin"
$ready = & "$PG_BIN\pg_isready.exe" -h localhost -p 5432
if ($LASTEXITCODE -eq 0) {
    Write-Host "  PostgreSQL standby démarré." -ForegroundColor Green
} else {
    Write-Host "  PostgreSQL pas encore prêt — vérifier les logs :" -ForegroundColor Yellow
    Write-Host "  Get-EventLog -LogName Application -Source 'postgresql*' -Newest 20"
}

# ── Mettre à jour pg_hba.conf sur le PRIMARY ─────────────────────────────────
Write-Host ""
Write-Host "ACTION REQUISE sur Windows 11 physique :" -ForegroundColor Magenta
Write-Host "  Ajouter dans C:\Program Files\PostgreSQL\18\data\pg_hba.conf :"
Write-Host ""
Write-Host "  host    replication  replicator  $STANDBY_IP/32  scram-sha-256" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Puis recharger : SELECT pg_reload_conf();"
Write-Host ""

# ── Résumé ────────────────────────────────────────────────────────────────────
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Setup Standby Windows terminé !" -ForegroundColor Green
Write-Host ""
Write-Host "  Vérifications :"
Write-Host "  Sur Win11 physique (pgAdmin ou psql) :"
Write-Host "    SELECT client_addr, application_name, state"
Write-Host "    FROM pg_stat_replication;"
Write-Host "    → doit afficher $STANDBY_NAME avec state=streaming"
Write-Host ""
Write-Host "  Pour tester le failover :"
Write-Host "    1. Arrêter Win11 physique"
Write-Host "    2. Sur cette VM : & '$PG_BIN\pg_ctl.exe' promote -D '$PG_DATA'"
Write-Host "    3. L'époque s'incrémente → ancien actif fencé"
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
