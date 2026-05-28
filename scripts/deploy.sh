#!/usr/bin/env bash
# scripts/deploy.sh — Deploy StellarAid core contract to a Soroban network.
#
# Usage:
#   bash scripts/deploy.sh [testnet|sandbox|mainnet]
#
# Environment variables (loaded from .env if present):
#   SOROBAN_ADMIN_SECRET_KEY  — funded account secret key
#   SOROBAN_NETWORK           — fallback network name (default: testnet)
#
# Closes #159, #160

set -euo pipefail

# ── Load .env if present ──────────────────────────────────────────────────────
if [ -f ".env" ]; then
  # shellcheck disable=SC1091
  set -o allexport
  source .env
  set +o allexport
fi

# ── Resolve network ───────────────────────────────────────────────────────────
NETWORK="${1:-${SOROBAN_NETWORK:-testnet}}"

# ── Network-specific RPC / passphrase ────────────────────────────────────────
case "$NETWORK" in
  testnet)
    RPC_URL="${SOROBAN_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org:443}"
    PASSPHRASE="${SOROBAN_TESTNET_PASSPHRASE:-Test SDF Network ; September 2015}"
    ;;
  mainnet)
    RPC_URL="${SOROBAN_MAINNET_RPC_URL:-https://soroban-rpc.mainnet.stellar.gateway.fm}"
    PASSPHRASE="${SOROBAN_MAINNET_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
    ;;
  sandbox)
    RPC_URL="${SOROBAN_SANDBOX_RPC_URL:-http://localhost:8000/soroban/rpc}"
    PASSPHRASE="${SOROBAN_SANDBOX_PASSPHRASE:-Standalone Network ; February 2017}"
    ;;
  *)
    echo "❌ Unknown network: $NETWORK (use testnet | sandbox | mainnet)"
    exit 1
    ;;
esac

# ── Paths ─────────────────────────────────────────────────────────────────────
WASM_PATH="target/wasm32-unknown-unknown/release/stellaraid_core.wasm"
DEPLOYMENTS_DIR="deployments"
DEPLOYMENT_FILE="${DEPLOYMENTS_DIR}/${NETWORK}.json"

# ── Validate prerequisites ────────────────────────────────────────────────────
if [ ! -f "$WASM_PATH" ]; then
  echo "❌ WASM not found at $WASM_PATH — run 'make build-wasm' first"
  exit 1
fi

if [ -z "${SOROBAN_ADMIN_SECRET_KEY:-}" ]; then
  echo "❌ SOROBAN_ADMIN_SECRET_KEY is not set. Add it to .env or export it."
  exit 1
fi

# ── Idempotency check ─────────────────────────────────────────────────────────
if [ -f "$DEPLOYMENT_FILE" ]; then
  EXISTING_ID=$(python3 -c "import json,sys; d=json.load(open('$DEPLOYMENT_FILE')); print(d.get('contract_id',''))" 2>/dev/null || true)
  if [ -n "$EXISTING_ID" ]; then
    echo "ℹ️  Contract already deployed on $NETWORK: $EXISTING_ID"
    echo "   Delete $DEPLOYMENT_FILE to force a re-deploy."
    exit 0
  fi
fi

# ── Deploy ────────────────────────────────────────────────────────────────────
echo "🚀 Deploying to $NETWORK..."
echo "   RPC: $RPC_URL"
echo "   WASM: $WASM_PATH"

CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source "$SOROBAN_ADMIN_SECRET_KEY" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE")

echo "✅ Contract deployed!"
echo "📝 Contract ID: $CONTRACT_ID"

# ── Persist deployment record ─────────────────────────────────────────────────
mkdir -p "$DEPLOYMENTS_DIR"
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
cat > "$DEPLOYMENT_FILE" <<EOF
{
  "network": "$NETWORK",
  "contract_id": "$CONTRACT_ID",
  "rpc_url": "$RPC_URL",
  "deployed_at": "$TIMESTAMP",
  "wasm": "$WASM_PATH"
}
EOF

echo "💾 Deployment record saved to $DEPLOYMENT_FILE"

# Also write the plain contract-ID file for backward compatibility
echo "$CONTRACT_ID" > .stellaraid_contract_id
echo "✅ Contract ID stored in .stellaraid_contract_id"
