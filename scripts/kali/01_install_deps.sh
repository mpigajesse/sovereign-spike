#!/usr/bin/env bash
# =============================================================================
# 01_install_deps.sh — Installation Rust + libsodium sur Kali
# Machine : Kali Linux — 192.168.200.128
# À exécuter UNE SEULE FOIS.
# =============================================================================
set -euo pipefail

SPIKE_SRC="/opt/sovereign-spike"

echo "=== sovereign-spike :: Installation Kali (Relais) ==="
echo ""

# ── Dépendances système ────────────────────────────────────────────────────────
echo "[1/3] Dépendances système..."
sudo apt-get update -q
sudo apt-get install -y \
    build-essential pkg-config curl \
    libssl-dev libsodium-dev
echo "OK"

# ── Rust ──────────────────────────────────────────────────────────────────────
echo ""
echo "[2/3] Installation de Rust..."
if ! command -v cargo &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    source "$HOME/.cargo/env"
    echo "Rust installé : $(rustc --version)"
else
    echo "Rust déjà installé : $(rustc --version)"
fi

# ── Code source + compilation ─────────────────────────────────────────────────
echo ""
echo "[3/3] Code source et compilation..."
echo "  Commande à exécuter sur WINDOWS pour copier le code :"
echo ""
echo "  scp -r 'D:\\PFE-FINAL\\pfe\\sovereign-spike' kali@192.168.200.128:/opt/"
echo ""
echo "  Appuyer sur Entrée une fois copié dans /opt/sovereign-spike ..."
read -r

source "$HOME/.cargo/env"
cd "$SPIKE_SRC"
cargo build --release --bin sovereign-relay
echo "OK — binaire : $SPIKE_SRC/target/release/sovereign-relay"

echo ""
echo "=== Installation Kali terminée ==="
echo "Prochain script : 02_start_relay.sh"
