#!/usr/bin/env bash
# =============================================================================
# 02_setup_standby.sh — Configuration du standby PostgreSQL sur Ubuntu
# Machine  : Ubuntu 26.04 — 192.168.200.130
# Primary  : Windows 11  — 192.168.200.1:5432
# À exécuter UNE SEULE FOIS (après 01_install_deps.sh).
#
# Ce script :
#   1. Clone le primary via pg_basebackup
#   2. Configure postgresql.auto.conf pour la réplication streaming
#   3. Crée standby.signal
#   4. Démarre PostgreSQL en mode standby
# =============================================================================
set -euo pipefail

PRIMARY_IP="192.168.200.1"
PRIMARY_PORT="5432"
STANDBY_NAME="sovereign_standby"    # doit correspondre à synchronous_standby_names sur primary
PG_DATA="/var/lib/postgresql/18/main"
PG_SLOT="sovereign_slot"

echo "=== sovereign-spike :: Setup PostgreSQL STANDBY ==="
echo "Primary : $PRIMARY_IP:$PRIMARY_PORT"
echo "Standby data dir : $PG_DATA"
echo ""

# ── Vérifier la connectivité avec le primary ──────────────────────────────────
echo "[1/5] Test de connectivité avec le primary..."
if ! pg_isready -h "$PRIMARY_IP" -p "$PRIMARY_PORT" -U replicator 2>/dev/null; then
    echo "  ATTENTION : le primary n'est pas encore accessible."
    echo "  Vérifier que PostgreSQL est démarré sur Windows (192.168.200.1:5432)"
    echo "  et que pg_hba.conf autorise 192.168.200.130."
    echo "  Continuer quand même ? (o/N)"
    read -r answer
    [[ "$answer" =~ ^[Oo]$ ]] || exit 1
fi
echo "OK"

# ── Arrêter PostgreSQL si déjà démarré ───────────────────────────────────────
echo ""
echo "[2/5] Arrêt du PostgreSQL local (si actif)..."
sudo systemctl stop postgresql@18-main 2>/dev/null || true
# Vider le répertoire de données existant (pg_basebackup l'exige vide)
if [ -d "$PG_DATA" ] && [ "$(ls -A $PG_DATA)" ]; then
    echo "  ATTENTION : $PG_DATA n'est pas vide."
    echo "  Supprimer et recréer ? TOUTES LES DONNÉES LOCALES SERONT PERDUES. (o/N)"
    read -r confirm
    [[ "$confirm" =~ ^[Oo]$ ]] || exit 1
    sudo rm -rf "$PG_DATA"
    sudo mkdir -p "$PG_DATA"
    sudo chown postgres:postgres "$PG_DATA"
    sudo chmod 700 "$PG_DATA"
fi
echo "OK"

# ── pg_basebackup : copie du primary ─────────────────────────────────────────
echo ""
echo "[3/5] pg_basebackup depuis le primary ($PRIMARY_IP)..."
echo "  (Entrer le mot de passe du rôle 'replicator' quand demandé : replicator_spike)"
sudo -u postgres pg_basebackup \
    -h "$PRIMARY_IP" \
    -p "$PRIMARY_PORT" \
    -U replicator \
    -D "$PG_DATA" \
    --wal-method=stream \
    --slot="$PG_SLOT" \
    --progress \
    --verbose
echo "pg_basebackup terminé."

# ── Configuration de la réplication streaming ────────────────────────────────
echo ""
echo "[4/5] Configuration de postgresql.auto.conf..."
sudo -u postgres tee "$PG_DATA/postgresql.auto.conf" > /dev/null << EOF
# Généré par sovereign-spike setup
primary_conninfo = 'host=$PRIMARY_IP port=$PRIMARY_PORT user=replicator password=replicator_spike application_name=$STANDBY_NAME'
primary_slot_name = '$PG_SLOT'
hot_standby = on
hot_standby_feedback = on
EOF

# Créer le signal de mode standby (PostgreSQL 12+)
sudo -u postgres touch "$PG_DATA/standby.signal"
echo "OK — standby.signal créé"

# ── Démarrage du standby ──────────────────────────────────────────────────────
echo ""
echo "[5/5] Démarrage de PostgreSQL en mode standby..."
sudo systemctl start postgresql@18-main
sleep 3

# Vérification
if sudo -u postgres pg_isready -h localhost -p 5432; then
    echo "PostgreSQL standby démarré."
else
    echo "ERREUR : PostgreSQL n'a pas démarré. Consulter :"
    echo "  sudo journalctl -u postgresql@18-main -n 50"
    exit 1
fi

echo ""
echo "=== Setup Standby terminé ==="
echo "Vérifications :"
echo "  Sur Windows pgAdmin → SELECT * FROM pg_stat_replication;"
echo "  → doit afficher sovereign_standby avec state=streaming"
echo ""
echo "Prochain script : 03_start_passive.sh"
