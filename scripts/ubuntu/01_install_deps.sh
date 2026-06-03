#!/usr/bin/env bash
# =============================================================================
# 01_install_deps.sh — Installation des dépendances sur Ubuntu (STANDBY)
# Machine : Ubuntu 26.04 — 192.168.200.130
# À exécuter UNE SEULE FOIS.
# =============================================================================
set -euo pipefail

PRIMARY_IP="192.168.200.1"
SPIKE_SRC="/opt/sovereign-spike"

echo "=== sovereign-spike :: Installation Ubuntu Standby ==="
echo "Machine : $(hostname) — $(hostname -I | awk '{print $1}')"
echo ""

# ── 1. Dépendances système ────────────────────────────────────────────────────
echo "[1/5] Mise à jour et dépendances système..."
sudo apt-get update -q
sudo apt-get install -y \
    build-essential pkg-config curl git \
    libssl-dev libsodium-dev \
    postgresql-client-16

echo "OK"

# ── 2. PostgreSQL 16 serveur ──────────────────────────────────────────────────
echo ""
echo "[2/5] Installation PostgreSQL 16 serveur..."
if ! command -v pg_basebackup &>/dev/null; then
    sudo apt-get install -y \
        postgresql-16 \
        postgresql-contrib-16
    # Arrêter le service PostgreSQL par défaut (le standby démarre via pg_basebackup)
    sudo systemctl stop postgresql
    sudo systemctl disable postgresql
    echo "PostgreSQL 16 installé (service arrêté — sera démarré comme standby)."
else
    echo "PostgreSQL déjà installé."
fi

# ── 3. Rust ───────────────────────────────────────────────────────────────────
echo ""
echo "[3/5] Installation de Rust..."
if ! command -v cargo &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    source "$HOME/.cargo/env"
    echo "Rust installé : $(rustc --version)"
else
    echo "Rust déjà installé : $(rustc --version)"
fi

# ── 4. Récupération du code source depuis Windows ────────────────────────────
echo ""
echo "[4/5] Récupération du code source depuis Windows ($PRIMARY_IP)..."
echo "  Méthode : SCP depuis Windows"
echo "  Commande à exécuter sur WINDOWS (PowerShell) :"
echo ""
echo "  scp -r D:\\PFE-FINAL\\pfe\\sovereign-spike ubuntu@192.168.200.130:/opt/"
echo ""
echo "  Ou avec un mot de passe :"
echo "  scp -r 'D:\\PFE-FINAL\\pfe\\sovereign-spike' ubuntu@192.168.200.130:/opt/"
echo ""
echo "  Appuyer sur Entrée une fois le code copié dans /opt/sovereign-spike ..."
read -r

if [ ! -d "$SPIKE_SRC" ]; then
    echo "ERREUR : /opt/sovereign-spike non trouvé. Copier le code d'abord."
    exit 1
fi
echo "Code source trouvé : $SPIKE_SRC"

# ── 5. Compilation ────────────────────────────────────────────────────────────
echo ""
echo "[5/5] Compilation du nœud passif..."
source "$HOME/.cargo/env"
cd "$SPIKE_SRC"
cargo build --release --bin sovereign-node-passive
echo "OK — binaire : $SPIKE_SRC/target/release/sovereign-node-passive"

echo ""
echo "=== Installation terminée ==="
echo "Prochain script : 02_setup_standby.sh"
