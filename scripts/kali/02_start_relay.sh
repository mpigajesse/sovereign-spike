#!/usr/bin/env bash
# =============================================================================
# 02_start_relay.sh — Démarrage du relais aveugle souverain (Kali)
# Machine : Kali Linux — 192.168.200.128
# Port    : HTTP 4000 (relais éditeur aveugle)
#
# Propriété zero-knowledge : le relais ne charge AUCUNE clé crypto.
# Il stocke et redistribue des blobs opaques uniquement.
# =============================================================================
set -euo pipefail

SPIKE_DIR="/opt/sovereign-spike"
ENV_FILE="$SPIKE_DIR/scripts/shared.env"

echo "=== sovereign-spike :: Relais AVEUGLE (Kali 192.168.200.128) ==="
echo ""

# ── Chargement des variables ──────────────────────────────────────────────────
if [ -f "$ENV_FILE" ]; then
    set -a; source "$ENV_FILE"; set +a
    echo "shared.env chargé."
else
    echo "ATTENTION : $ENV_FILE introuvable. Utilisation des valeurs par défaut."
fi

export RELAY_API_KEY="${RELAY_API_KEY:-sovereign-spike-relay-key-2026}"
export RELAY_LISTEN_ADDR="${RELAY_LISTEN_ADDR:-0.0.0.0:4000}"
export RELAY_MAX_BLOBS="${RELAY_MAX_BLOBS:-100000}"
export RUST_LOG="${RUST_LOG:-sovereign_relay=info}"

echo "Config :"
echo "  RELAY_LISTEN_ADDR = $RELAY_LISTEN_ADDR"
echo "  RELAY_MAX_BLOBS   = $RELAY_MAX_BLOBS"
echo "  RELAY_API_KEY     = ${RELAY_API_KEY:0:12}..."
echo ""
echo "  NOTE ZERO-KNOWLEDGE : aucune DEK chargée sur ce nœud."
echo "  Le relais ne peut pas interpréter le contenu des blobs."
echo ""
echo "Endpoints disponibles :"
echo "  GET  http://192.168.200.128:4000/health"
echo "  POST http://192.168.200.128:4000/blobs  (nœud actif → relais)"
echo "  GET  http://192.168.200.128:4000/blobs  (passifs → relais)"
echo ""
echo "Pour inspecter le trafic réseau (optionnel) :"
echo "  sudo tcpdump -i eth1 port 4000 -A    # voir les requêtes HTTP"
echo "  wireshark -i eth1 -f 'port 4000'     # GUI Wireshark"
echo ""
echo "Ctrl+C pour arrêter."
echo "─────────────────────────────────────────────────────────────"

cd "$SPIKE_DIR"
./target/release/sovereign-relay
