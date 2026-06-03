# =============================================================================
# 01_setup_pg.ps1 — Configuration PostgreSQL 18 sur Windows (PRIMARY)
# Machine : Windows 11 physique — 192.168.200.1
# À exécuter UNE SEULE FOIS en tant qu'administrateur.
# =============================================================================

$PG_BIN   = "C:\Program Files\PostgreSQL\18\bin"
$PG_DATA  = "C:\Program Files\PostgreSQL\18\data"
$PG_CONF  = "$PG_DATA\postgresql.conf"

Write-Host "=== sovereign-spike :: Setup PostgreSQL Primary ===" -ForegroundColor Cyan

# ── 1. Vérifier que PostgreSQL est accessible ──────────────────────────────
Write-Host "`n[1/5] Vérification de psql..." -ForegroundColor Yellow
$psql = "$PG_BIN\psql.exe"
if (-not (Test-Path $psql)) {
    Write-Error "psql non trouvé : $psql`nVérifier le chemin d'installation PG 18."
    exit 1
}
& $psql --version
Write-Host "OK" -ForegroundColor Green

# ── 2. Créer la base et les rôles ────────────────────────────────────────────
Write-Host "`n[2/5] Création de la base sovereign_active et des rôles..." -ForegroundColor Yellow
# Utiliser l'utilisateur postgres (superuser PG, différent du login pgAdmin)
# Si votre superuser PG n'est pas 'postgres', ajuster -U ci-dessous.
$env:PGPASSWORD = "admin"   # mot de passe du superuser PG

& $psql -U postgres -h 127.0.0.1 -p 5432 -f "..\..\config\setup_replication.sql" 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Warning "Certaines instructions ont peut-être déjà été exécutées (normal si réexécution)."
}
Write-Host "OK" -ForegroundColor Green

# ── 3. Configurer postgresql.conf pour la réplication ────────────────────────
Write-Host "`n[3/5] Configuration de postgresql.conf pour réplication synchrone..." -ForegroundColor Yellow
$pg_conf_additions = @"

# ── Ajouté par sovereign-spike setup ──────────────────────────────
wal_level                    = replica
max_wal_senders              = 3
wal_keep_size                = 64
synchronous_commit           = on
synchronous_standby_names    = 'sovereign-standby'
log_replication_commands     = on
"@

# Vérifier si déjà configuré
$current_conf = Get-Content $PG_CONF -Raw
if ($current_conf -notmatch "sovereign-spike") {
    Add-Content -Path $PG_CONF -Value $pg_conf_additions
    Write-Host "postgresql.conf mis à jour." -ForegroundColor Green
} else {
    Write-Host "postgresql.conf déjà configuré." -ForegroundColor Green
}

# ── 4. Configurer pg_hba.conf pour la réplication depuis Ubuntu ─────────────
Write-Host "`n[4/5] Configuration de pg_hba.conf pour le standby Ubuntu..." -ForegroundColor Yellow
$pg_hba = "$PG_DATA\pg_hba.conf"
$hba_lines = @"

# sovereign-spike : réplication depuis Ubuntu standby (192.168.200.130)
host    replication     replicator      192.168.200.130/32    scram-sha-256
# sovereign-spike : accès applicatif local
host    sovereign_active  sovereign     127.0.0.1/32          scram-sha-256
host    sovereign_active  sovereign     192.168.200.0/24      scram-sha-256
"@

$current_hba = Get-Content $pg_hba -Raw
if ($current_hba -notmatch "sovereign-spike") {
    Add-Content -Path $pg_hba -Value $hba_lines
    Write-Host "pg_hba.conf mis à jour." -ForegroundColor Green
} else {
    Write-Host "pg_hba.conf déjà configuré." -ForegroundColor Green
}

# ── 5. Redémarrer PostgreSQL pour prendre en compte les changements ──────────
Write-Host "`n[5/5] Redémarrage du service PostgreSQL..." -ForegroundColor Yellow
Restart-Service -Name "postgresql-x64-18" -ErrorAction SilentlyContinue
if ($LASTEXITCODE -ne 0) {
    # Essai avec le nom court
    Restart-Service -Name "postgresql*" -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Seconds 3

Write-Host "`n=== Setup PostgreSQL terminé ===" -ForegroundColor Green
Write-Host "Vérifier dans pgAdmin :" -ForegroundColor Cyan
Write-Host "  - Base 'sovereign_active' existe"
Write-Host "  - Rôle 'sovereign' existe"
Write-Host "  - Rôle 'replicator' existe"
Write-Host "  - SELECT * FROM pg_replication_slots; → sovereign_slot"
Write-Host ""
Write-Host "Prochain script : 02_start_active.ps1" -ForegroundColor Yellow
