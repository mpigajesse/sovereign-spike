#!/usr/bin/env bash
# =============================================================================
# 00_install_postgres_client_pgadmin.sh
# Installation PostgreSQL 18 client + pgAdmin 4 sur Kali Linux
# Machine : Kali — 192.168.200.128
#
# Note : On installe uniquement le CLIENT PostgreSQL sur Kali (pas le serveur).
# Kali sert à inspecter le trafic réseau et à héberger le relais souverain.
#
# Exécuter en tant qu'utilisateur normal (sudo sera demandé).
# =============================================================================
set -euo pipefail

echo ""
echo "======================================================"
echo "  Installation PostgreSQL client + pgAdmin 4 — Kali"
echo "  Machine : $(hostname) — $(hostname -I | awk '{print $1}')"
echo "======================================================"
echo ""

# ── 1. Mise à jour ────────────────────────────────────────────────────────────
echo "[1/5] Mise à jour du système..."
sudo apt-get update -q
echo "OK"

# ── 2. Dépendances ────────────────────────────────────────────────────────────
echo ""
echo "[2/5] Dépendances de base..."
sudo apt-get install -y \
    curl wget gnupg2 lsb-release \
    ca-certificates apt-transport-https
echo "OK"

# ── 3. Dépôt officiel PostgreSQL (PGDG) ──────────────────────────────────────
echo ""
echo "[3/5] Ajout du dépôt PostgreSQL officiel..."

curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
    | sudo gpg --dearmor -o /usr/share/keyrings/postgresql.gpg

# Kali retourne 'kali-rolling' mais PGDG n'a pas ce nom — forcer bookworm (base Debian)
DISTRO="bookworm"
echo "  Distribution : $DISTRO"

echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] https://apt.postgresql.org/pub/repos/apt ${DISTRO}-pgdg main" \
    | sudo tee /etc/apt/sources.list.d/pgdg.list > /dev/null

sudo apt-get update -q
echo "Dépôt PGDG ajouté."

# ── 4. PostgreSQL 18 client uniquement ───────────────────────────────────────
echo ""
echo "[4/5] Installation PostgreSQL 18 client tools..."
# Client seulement (pas le serveur) — pour se connecter au primary et observer
sudo apt-get install -y \
    postgresql-client-18

echo "  Installé : $(psql --version)"
echo "OK"

# ── 5. pgAdmin 4 ──────────────────────────────────────────────────────────────
echo ""
echo "[5/5] Installation pgAdmin 4..."

curl -fsSL https://www.pgadmin.org/static/packages_pgadmin_org.pub \
    | sudo gpg --dearmor -o /usr/share/keyrings/pgadmin.gpg

echo "deb [signed-by=/usr/share/keyrings/pgadmin.gpg] https://ftp.postgresql.org/pub/pgadmin/pgadmin4/apt/${DISTRO} pgadmin4 main" \
    | sudo tee /etc/apt/sources.list.d/pgadmin4.list > /dev/null

sudo apt-get update -q
sudo apt-get install -y pgadmin4-web || sudo apt-get install -y pgadmin4

if [ -f /usr/pgadmin4/bin/setup-web.sh ]; then
    echo ""
    echo "  Configuration de pgAdmin 4 web..."
    sudo /usr/pgadmin4/bin/setup-web.sh
fi

echo "OK"

# ── Vérifications + accès Wireshark ──────────────────────────────────────────
echo ""
echo "  Installation de Wireshark pour l'inspection réseau..."
sudo apt-get install -y wireshark tcpdump 2>/dev/null || true
# Autoriser Wireshark sans sudo
sudo usermod -aG wireshark "$USER" 2>/dev/null || true

echo ""
echo "======================================================"
echo "  Installation Kali terminée !"
echo ""
echo "  PostgreSQL client : $(psql --version 2>/dev/null || echo 'verifier installation')"
echo "  pgAdmin 4         : http://192.168.200.128/pgadmin4"
echo ""
echo "  Pour se connecter aux nœuds depuis pgAdmin (Kali) :"
echo ""
echo "  Nœud ACTIF (Windows - primary) :"
echo "    Host : 192.168.200.1 | Port : 5432"
echo "    User : postgres      | Password : admin"
echo "    DB   : sovereign_active"
echo ""
echo "  Nœud PASSIF (Ubuntu - standby) :"
echo "    Host : 192.168.200.130 | Port : 5432"
echo "    User : postgres        | Password : admin"
echo "    DB   : sovereign_active"
echo ""
echo "  Commandes d'inspection réseau :"
echo "    sudo tcpdump -i eth1 port 5432 -A     # trafic PostgreSQL"
echo "    sudo tcpdump -i eth1 port 3000 -A     # trafic nœud actif"
echo "    sudo tcpdump -i eth1 port 4000 -A     # trafic relais"
echo "    wireshark                             # interface graphique"
echo ""
echo "  Prochain script : 01_install_deps.sh (Rust + relais)"
echo "======================================================"
