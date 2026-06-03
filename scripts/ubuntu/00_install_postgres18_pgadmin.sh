#!/usr/bin/env bash
# =============================================================================
# 00_install_postgres18_pgadmin.sh
# Installation PostgreSQL 18 + pgAdmin 4 sur Ubuntu
# Machine : Ubuntu — 192.168.200.130
#
# Exécuter en tant qu'utilisateur normal (sudo sera demandé).
# Durée estimée : 5-10 minutes selon la connexion internet.
# =============================================================================
set -euo pipefail

echo ""
echo "======================================================"
echo "  Installation PostgreSQL 18 + pgAdmin 4 — Ubuntu"
echo "  Machine : $(hostname) — $(hostname -I | awk '{print $1}')"
echo "======================================================"
echo ""

# ── 1. Mise à jour du système ─────────────────────────────────────────────────
echo "[1/6] Mise à jour du système..."
sudo apt-get update -q
sudo apt-get upgrade -y -q
echo "OK"

# ── 2. Dépendances de base ────────────────────────────────────────────────────
echo ""
echo "[2/6] Installation des dépendances de base..."
sudo apt-get install -y \
    curl wget gnupg2 lsb-release \
    ca-certificates apt-transport-https \
    software-properties-common
echo "OK"

# ── 3. Ajout du dépôt officiel PostgreSQL (PGDG) ─────────────────────────────
echo ""
echo "[3/6] Ajout du dépôt officiel PostgreSQL 18..."

# Clé GPG officielle PostgreSQL
curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    | sudo gpg --dearmor -o /usr/share/keyrings/postgresql.gpg

# Dépôt PGDG pour la distribution courante
DISTRO=$(lsb_release -cs)
echo "  Distribution détectée : $DISTRO"
echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] https://apt.postgresql.org/pub/repos/apt ${DISTRO}-pgdg main" \
    | sudo tee /etc/apt/sources.list.d/pgdg.list > /dev/null

sudo apt-get update -q
echo "Dépôt PGDG ajouté."

# ── 4. Installation PostgreSQL 18 ─────────────────────────────────────────────
echo ""
echo "[4/6] Installation de PostgreSQL 18..."
sudo apt-get install -y \
    postgresql-18 \
    postgresql-client-18 \
    postgresql-contrib-18

# Vérification
PG_VERSION=$(psql --version 2>/dev/null | head -1)
echo "  Installé : $PG_VERSION"

# Arrêt du service (sera configuré comme standby dans le script suivant)
sudo systemctl stop postgresql
sudo systemctl disable postgresql
echo "  Service PostgreSQL arrêté (sera démarré comme standby ensuite)."
echo "OK"

# ── 5. Installation de pgAdmin 4 ──────────────────────────────────────────────
echo ""
echo "[5/6] Installation de pgAdmin 4..."

# Clé GPG officielle pgAdmin
curl -fsSL https://www.pgadmin.org/static/packages_pgadmin_org.pub \
    | sudo gpg --dearmor -o /usr/share/keyrings/pgadmin.gpg

# Dépôt pgAdmin pour la distribution courante
echo "deb [signed-by=/usr/share/keyrings/pgadmin.gpg] https://ftp.postgresql.org/pub/pgadmin/pgadmin4/apt/${DISTRO} pgadmin4 main" \
    | sudo tee /etc/apt/sources.list.d/pgadmin4.list > /dev/null

sudo apt-get update -q

# Installer pgAdmin 4 web (accessible via navigateur) + desktop si disponible
sudo apt-get install -y pgadmin4-web || sudo apt-get install -y pgadmin4

# Configurer le serveur web pgAdmin
if command -v /usr/pgadmin4/bin/setup-web.sh &>/dev/null; then
    echo ""
    echo "  Configuration de pgAdmin 4 web..."
    echo "  (Vous serez invité à créer un compte email/mot de passe pour pgAdmin)"
    sudo /usr/pgadmin4/bin/setup-web.sh
fi

echo "OK"

# ── 6. Vérifications finales ──────────────────────────────────────────────────
echo ""
echo "[6/6] Vérifications..."

echo "  PostgreSQL 18 :"
pg_lsclusters 2>/dev/null || echo "    (pg_lsclusters non disponible)"
psql --version

echo ""
echo "  pgAdmin 4 :"
if command -v pgadmin4 &>/dev/null; then
    echo "    pgadmin4 installé"
elif [ -d /usr/pgadmin4 ]; then
    echo "    pgAdmin 4 web installé dans /usr/pgadmin4"
    echo "    Accès : http://localhost/pgadmin4  (ou http://192.168.200.130/pgadmin4)"
fi

echo ""
echo "======================================================"
echo "  Installation terminée !"
echo ""
echo "  PostgreSQL 18 : installé (service arrêté volontairement)"
echo "  pgAdmin 4     : accessible sur http://192.168.200.130/pgadmin4"
echo ""
echo "  Pour se connecter au PRIMARY (Windows 192.168.200.1) depuis pgAdmin :"
echo "    Host    : 192.168.200.1"
echo "    Port    : 5432"
echo "    User    : postgres"
echo "    Password: admin"
echo "    DB      : sovereign_active"
echo ""
echo "  Prochain script : 01_install_deps.sh"
echo "======================================================"
