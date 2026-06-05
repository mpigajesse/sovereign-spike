# =============================================================================
# 03_setup_standby_win.ps1 - Configuration du standby PostgreSQL sur Windows 11 VM
# Machine  : Windows 11 VM
# Primary  : Windows 11 physique - 192.168.200.1:5432
#
# A executer UNE SEULE FOIS sur la VM Windows 11.
# Le script s'auto-eleve en administrateur si necessaire (UAC).
#
# NOTE: Script volontairement en ASCII pur (sans accents) pour eviter toute
# corruption d'encodage quand il est ecrit dans un fichier temporaire et
# execute par Windows PowerShell 5.1.
# =============================================================================

# ---- Auto-elevation administrateur (UAC) ------------------------------------
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
            ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Elevation administrateur requise - une fenetre UAC va s'ouvrir..." -ForegroundColor Yellow
    $scriptPath = $MyInvocation.MyCommand.Definition
    $argList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-File", "`"$scriptPath`"")
    try {
        Start-Process -FilePath "powershell.exe" -Verb RunAs -ArgumentList $argList
    } catch {
        Write-Error "Elevation refusee. Relancez PowerShell en administrateur puis reexecutez."
    }
    exit
}

$PRIMARY_IP   = "192.168.200.1"
$PRIMARY_PORT = "5432"
$STANDBY_NAME = "sovereign_standby_win"
$PG_BIN       = "C:\Program Files\PostgreSQL\18\bin"
$PG_DATA      = "C:\Program Files\PostgreSQL\18\data"
$PG_SLOT      = "sovereign_slot_win"

# Adresse "primary:port" assemblee sans piege de scope PowerShell
$PRIMARY_ADDR = "${PRIMARY_IP}:${PRIMARY_PORT}"

# ---- Detecter l'IP locale (VMnet1) ------------------------------------------
$STANDBY_IP = (Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -like "192.168.200.*" } |
    Select-Object -First 1).IPAddress

if (-not $STANDBY_IP) {
    Write-Error "Aucune adresse 192.168.200.x trouvee. Verifier la connexion VMnet1."
    exit 1
}

Write-Host "=== Sovereign-Spike :: Setup PostgreSQL STANDBY (Windows 11 VM) ===" -ForegroundColor Cyan
Write-Host "Primary : $PRIMARY_ADDR"
Write-Host "Standby : $STANDBY_IP (cette machine)"
Write-Host "Slot    : $PG_SLOT"
Write-Host ""

# ---- Verifier psql ----------------------------------------------------------
Write-Host "[1/5] Verification de PostgreSQL 18..." -ForegroundColor Yellow
$psql = "$PG_BIN\psql.exe"
if (-not (Test-Path $psql)) {
    Write-Error "PostgreSQL 18 non trouve dans $PG_BIN"
    Write-Host "  Telecharger depuis https://www.postgresql.org/download/windows/"
    exit 1
}
Write-Host "  OK - $(& $psql --version)"

# ---- Verifier la connectivite au primary ------------------------------------
Write-Host ""
Write-Host "[2/5] Test de connectivite avec le primary ($PRIMARY_ADDR)..." -ForegroundColor Yellow
$conn = Test-NetConnection -ComputerName $PRIMARY_IP -Port $PRIMARY_PORT -InformationLevel Quiet
if (-not $conn) {
    Write-Error "Impossible de joindre $PRIMARY_ADDR"
    Write-Host "  Verifier que PostgreSQL tourne sur Win11 physique et que le pare-feu autorise le port 5432."
    exit 1
}
Write-Host "  OK - primary joignable"

# ---- Creer le slot de replication sur le primary ----------------------------
Write-Host ""
Write-Host "[2b] Creation du slot de replication '$PG_SLOT' sur le primary..." -ForegroundColor Yellow
$env:PGPASSWORD = "replicator_spike"
try {
    & $psql -U replicator -h $PRIMARY_IP -p $PRIMARY_PORT -d "replication" `
        -c "SELECT pg_create_physical_replication_slot('$PG_SLOT');" 2>&1 | Out-Null
    Write-Host "  Slot '$PG_SLOT' cree (ou deja existant)"
} catch {
    Write-Host "  Slot peut-etre deja existant - OK" -ForegroundColor Yellow
}

# ---- Arreter PostgreSQL local -----------------------------------------------
Write-Host ""
Write-Host "[3/5] Arret du PostgreSQL local..." -ForegroundColor Yellow
Stop-Service -Name "postgresql-x64-18" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
Write-Host "  OK"

# ---- Vider le repertoire data (pg_basebackup l'exige vide) ------------------
Write-Host ""
Write-Host "[3b] Nettoyage du repertoire data..." -ForegroundColor Yellow
if (Test-Path $PG_DATA) {
    $confirm = Read-Host "  ATTENTION : $PG_DATA sera efface. Continuer ? (o/N)"
    if ($confirm -ne "o" -and $confirm -ne "O") { exit 1 }
    Remove-Item "$PG_DATA\*" -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host "  OK"

# ---- pg_basebackup ----------------------------------------------------------
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
    Write-Error "pg_basebackup echoue (code $LASTEXITCODE)"
    exit 1
}
Write-Host "  pg_basebackup termine." -ForegroundColor Green

# ---- Configurer la replication streaming ------------------------------------
Write-Host ""
Write-Host "[5/5] Configuration de postgresql.auto.conf..." -ForegroundColor Yellow

$autoconf = "$PG_DATA\postgresql.auto.conf"
$conninfo = "host=$PRIMARY_IP port=$PRIMARY_PORT user=replicator password=replicator_spike application_name=$STANDBY_NAME"

@"
# Genere par sovereign-spike setup (Windows standby)
primary_conninfo = '$conninfo'
primary_slot_name = '$PG_SLOT'
hot_standby = on
hot_standby_feedback = on
"@ | Set-Content $autoconf -Encoding UTF8

New-Item -Path "$PG_DATA\standby.signal" -ItemType File -Force | Out-Null
Write-Host "  standby.signal cree"

# ---- Demarrer PostgreSQL en mode standby ------------------------------------
Start-Service -Name "postgresql-x64-18"
Start-Sleep -Seconds 4

$env:PGPASSWORD = "admin"
& "$PG_BIN\pg_isready.exe" -h localhost -p 5432 | Out-Null
if ($LASTEXITCODE -eq 0) {
    Write-Host "  PostgreSQL standby demarre." -ForegroundColor Green
} else {
    Write-Host "  PostgreSQL pas encore pret - verifier les logs." -ForegroundColor Yellow
}

# ---- Resume -----------------------------------------------------------------
Write-Host ""
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host "  Setup Standby Windows termine !" -ForegroundColor Green
Write-Host ""
Write-Host "  IP de ce standby : $STANDBY_IP"
Write-Host ""
Write-Host "  Verifier sur Win11 physique :"
Write-Host "    SELECT client_addr, application_name, state FROM pg_stat_replication;"
Write-Host "    -> doit afficher $STANDBY_NAME avec state=streaming"
Write-Host ""
Write-Host "  Pour tester le failover (sur cette VM) :"
Write-Host "    & '$PG_BIN\pg_ctl.exe' promote -D '$PG_DATA'"
Write-Host "===================================================" -ForegroundColor Cyan

Write-Host ""
Write-Host "Appuyez sur Entree pour fermer..."
Read-Host | Out-Null
