#!/usr/bin/env bash
# =============================================================================
# 03_start_passive.sh — Démarrage du nœud passif souverain (Ubuntu STANDBY)
# Machine : Ubuntu 26.04 — 192.168.200.130
# Ports   : HTTP 3001 (nœud passif lecture seule)
# =============================================================================
set -euo pipefail

SPIKE_DIR="/opt/sovereign-spike"
ENV_FILE="$SPIKE_DIR/scripts/shared.env"

echo "=== sovereign-spike :: Nœud PASSIF (Ubuntu 192.168.200.130) ==="
echo ""

# ── Chargement des variables d'environnement ─────────────────────────────────
if [ -f "$ENV_FILE" ]; then
    # shellcheck source=/dev/null
    set -a; source "$ENV_FILE"; set +a
    echo "shared.env chargé."
else
    echo "ERREUR : $ENV_FILE introuvable."
    echo "Créer shared.env depuis shared.env.example et renseigner SOVEREIGN_DEK_HEX."
    exit 1
fi

# Validation de la DEK
if [ -z "${SOVEREIGN_DEK_HEX:-}" ] || [ "$SOVEREIGN_DEK_HEX" = "REMPLACER_PAR_LA_DEK_HEX_64_CARACTERES" ]; then
    echo "ERREUR : SOVEREIGN_DEK_HEX non définie dans shared.env."
    echo "Récupérer la valeur depuis les logs du nœud actif Windows et la copier."
    exit 1
fi

# Variables spécifiques au passif
export ACTIVE_NODE_URL="${ACTIVE_NODE_URL:-http://192.168.200.1:3000}"
export PASSIVE_DB_PATH="${PASSIVE_DB_PATH:-/tmp/sovereign_passive.db}"
export PASSIVE_LISTEN_ADDR="${PASSIVE_LISTEN_ADDR:-0.0.0.0:3001}"
export SYNC_INTERVAL_SECS="${SYNC_INTERVAL_SECS:-5}"
export RUST_LOG="${RUST_LOG:-sovereign_passive=info}"

echo "Config :"
echo "  ACTIVE_NODE_URL   = $ACTIVE_NODE_URL"
echo "  PASSIVE_DB_PATH   = $PASSIVE_DB_PATH"
echo "  PASSIVE_LISTEN_ADDR = $PASSIVE_LISTEN_ADDR"
echo "  SYNC_INTERVAL_SECS  = $SYNC_INTERVAL_SECS"
echo "  DEK               = ${SOVEREIGN_DEK_HEX:0:8}..."
echo ""
echo "Endpoints disponibles :"
echo "  GET  http://192.168.200.130:3001/health"
echo "  GET  http://192.168.200.130:3001/stock/{item_id}"
echo "  GET  http://192.168.200.130:3001/sync/status"
echo ""
echo "Ctrl+C pour arrêter."
echo "─────────────────────────────────────────────────────────────"

cd "$SPIKE_DIR"
./target/release/sovereign-node-passive
