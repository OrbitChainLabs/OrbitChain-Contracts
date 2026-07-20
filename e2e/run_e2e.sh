#!/usr/bin/env bash
# e2e/run_e2e.sh — End-to-end lifecycle test harness (issue #116).
#
# Boots a Soroban sandbox (stellar/quickstart in Docker), deploys the
# canonical orbitchain-campaign contract, and walks the full donor
# lifecycle against a REAL network — sequence numbers, ledger clock, event
# ordering, and an actual token transfer on release:
#
#   init → donate → milestone unlock → release → balance assertion
#
# Usage:
#   make e2e                       # local sandbox via Docker (default)
#   bash e2e/run_e2e.sh local
#   bash e2e/run_e2e.sh futurenet  # same lifecycle against Futurenet
#                                  # (no Docker; used by the scheduled CI
#                                  # workflow — see issue #124)
#
# Environment overrides:
#   E2E_QUICKSTART_IMAGE  docker image (default stellar/quickstart:latest)
#   E2E_RPC_PORT          host port for the sandbox (default 8000)
#   E2E_SKIP_BUILD=1      reuse an existing campaign wasm artifact
set -uo pipefail

NETWORK_KIND="${1:-${E2E_NETWORK:-local}}"
QUICKSTART_IMAGE="${E2E_QUICKSTART_IMAGE:-stellar/quickstart:latest}"
RPC_PORT="${E2E_RPC_PORT:-8000}"
CONTAINER_NAME="orbitchain-e2e-sandbox"
WASM="target/wasm32v1-none/release/orbitchain_campaign.wasm"
KEY_PREFIX="orbitchain-e2e"

# Lifecycle numbers: one milestone whose target equals the goal, so a single
# donation reaches the goal, flips status to GoalReached, and unlocks the
# milestone (initialize requires last milestone.target_amount == goal).
GOAL=1000
DONATION=1000
MIN_DONATION=10

# ── Reporting ─────────────────────────────────────────────────────────────────
STEPS=()
step()  { printf '\n▶ %s\n' "$1"; STEPS+=("⏳ $1"); }
ok()    { STEPS[${#STEPS[@]}-1]="✅ ${STEPS[${#STEPS[@]}-1]#⏳ }"; printf '  ✅ %s\n' "${1:-ok}"; }
fail()  { STEPS[${#STEPS[@]}-1]="❌ ${STEPS[${#STEPS[@]}-1]#⏳ }"; printf '  ❌ %s\n' "$1"; report 1; }

report() {
  local code="${1:-0}"
  printf '\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
  printf 'E2E lifecycle report (%s)\n' "$NETWORK_KIND"
  printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
  local s
  for s in "${STEPS[@]}"; do printf '  %s\n' "$s"; done
  if [ "$code" = "0" ]; then
    printf '\n🎉 E2E PASSED — full lifecycle (init → donate → unlock → release) verified on-chain.\n'
  else
    printf '\n💥 E2E FAILED — see the first ❌ above.\n'
  fi
  exit "$code"
}

cleanup() {
  if [ "$NETWORK_KIND" = "local" ]; then
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

# Run a stellar CLI command, capturing stdout; on failure print stderr and fail.
STELLAR_ERR=$(mktemp)
run() {
  local out
  if ! out=$(stellar "$@" 2>"$STELLAR_ERR"); then
    printf '  stderr: %s\n' "$(tail -4 "$STELLAR_ERR")"
    return 1
  fi
  printf '%s' "$out"
}

# ── [1] Preflight ─────────────────────────────────────────────────────────────
step "Preflight: required tooling"
command -v stellar >/dev/null 2>&1 || fail "stellar CLI not found — install with 'cargo install --locked stellar-cli' or 'brew install stellar-cli'"
command -v curl >/dev/null 2>&1 || fail "curl not found"
case "$NETWORK_KIND" in
  local)
    command -v docker >/dev/null 2>&1 || fail "docker not found — required to boot the sandbox (or run against futurenet)"
    ;;
  futurenet) ;;
  *) fail "Unknown network '$NETWORK_KIND' (use local | futurenet)" ;;
esac
ok "stellar CLI $(stellar --version | head -1 | awk '{print $2}'), $NETWORK_KIND mode"

# ── [2] Network up ────────────────────────────────────────────────────────────
if [ "$NETWORK_KIND" = "local" ]; then
  step "Boot Soroban sandbox (Docker: $QUICKSTART_IMAGE)"
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
  docker run -d --rm -p "$RPC_PORT:8000" --name "$CONTAINER_NAME" \
    "$QUICKSTART_IMAGE" --local >/dev/null || fail "docker run failed"
  RPC_URL=""
  PASSPHRASE="Standalone Network ; February 2017"
  FRIENDBOT_URL="http://localhost:$RPC_PORT/friendbot"
  # RPC path moved between quickstart releases; probe both.
  for i in $(seq 1 120); do
    for path in rpc soroban/rpc; do
      if curl -sf -X POST "http://localhost:$RPC_PORT/$path" \
           -H 'Content-Type: application/json' \
           -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q '"healthy"'; then
        RPC_URL="http://localhost:$RPC_PORT/$path"
        break 2
      fi
    done
    sleep 2
  done
  [ -n "$RPC_URL" ] || fail "sandbox RPC did not become healthy within 240s (docker logs $CONTAINER_NAME)"
  ok "sandbox healthy at $RPC_URL"
else
  step "Use Futurenet"
  RPC_URL="https://rpc-futurenet.stellar.org"
  PASSPHRASE="Test SDF Future Network ; October 2022"
  FRIENDBOT_URL="https://friendbot-futurenet.stellar.org"
  curl -sf -X POST "$RPC_URL" -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' | grep -q '"healthy"' \
    || fail "Futurenet RPC is not reachable/healthy"
  ok "Futurenet RPC healthy"
fi
NET=(--rpc-url "$RPC_URL" --network-passphrase "$PASSPHRASE")

# ── [3] Accounts ──────────────────────────────────────────────────────────────
step "Create + fund test accounts (creator, donor, beneficiary)"
for who in creator donor beneficiary; do
  run keys generate "$KEY_PREFIX-$who" --overwrite >/dev/null 2>&1 \
    || stellar keys generate "$KEY_PREFIX-$who" --overwrite >/dev/null 2>&1 \
    || fail "keys generate failed for $who"
  addr=$(run keys address "$KEY_PREFIX-$who") || fail "keys address failed for $who"
  # bash-3.2-portable (macOS default shell has no ${var^^})
  case "$who" in
    creator) CREATOR_ADDR="$addr" ;;
    donor) DONOR_ADDR="$addr" ;;
    beneficiary) BENEFICIARY_ADDR="$addr" ;;
  esac
  # Friendbot funding with retries (the sandbox friendbot can lag RPC health).
  funded=""
  for i in $(seq 1 30); do
    if curl -sf "$FRIENDBOT_URL?addr=$addr" >/dev/null 2>&1; then funded=1; break; fi
    sleep 2
  done
  [ -n "$funded" ] || fail "friendbot could not fund $who ($addr)"
done
ok "creator=$CREATOR_ADDR donor=$DONOR_ADDR beneficiary=$BENEFICIARY_ADDR"

# ── [4] Build the contract ────────────────────────────────────────────────────
step "Build orbitchain-campaign wasm"
if [ "${E2E_SKIP_BUILD:-}" = "1" ] && [ -f "$WASM" ]; then
  ok "reusing existing $WASM (E2E_SKIP_BUILD=1)"
else
  cargo build -p orbitchain-campaign --release --target wasm32v1-none >/dev/null 2>&1 \
    || fail "cargo build -p orbitchain-campaign --target wasm32v1-none failed"
  [ -f "$WASM" ] || fail "expected artifact missing: $WASM"
  ok "built $WASM"
fi

# ── [5] Deploy the native-XLM Stellar Asset Contract ─────────────────────────
step "Deploy/resolve the native XLM SAC"
SAC_ID=$(run contract asset deploy --asset native --source-account "$KEY_PREFIX-creator" "${NET[@]}") \
  || SAC_ID=$(run contract id asset --asset native "${NET[@]}") \
  || fail "could not deploy or resolve the native SAC"
ok "SAC: $SAC_ID"

# ── [6] Deploy the campaign contract ─────────────────────────────────────────
step "Deploy orbitchain-campaign"
CONTRACT_ID=$(run contract deploy --wasm "$WASM" --source-account "$KEY_PREFIX-creator" "${NET[@]}") \
  || fail "contract deploy failed"
ok "contract: $CONTRACT_ID"

invoke() { # invoke <source-key> -- <fn and args...>
  local src="$1"; shift
  run contract invoke --id "$CONTRACT_ID" --source-account "$src" "${NET[@]}" "$@"
}

# ── [7] initialize (creator-authorized) ──────────────────────────────────────
step "initialize: goal=$GOAL, 1 milestone, XLM accepted"
END_TIME=$(( $(date +%s) + 86400 ))
DESC_HASH=$(printf 'orbitchain e2e milestone 0' | shasum -a 256 | awk '{print $1}')
MILESTONES='[{"index":0,"target_amount":"'"$GOAL"'","released_amount":"0","description_hash":"'"$DESC_HASH"'","status":"Locked","released_at":null,"released_at_ledger":null,"release_tx":null,"released_to":null}]'
ASSETS='[{"asset_code":"XLM","issuer":"'"$SAC_ID"'"}]'
invoke "$KEY_PREFIX-creator" -- initialize \
  --creator "$CREATOR_ADDR" \
  --goal_amount "$GOAL" \
  --end_time "$END_TIME" \
  --accepted_assets "$ASSETS" \
  --milestones "$MILESTONES" \
  --min_donation_amount "$MIN_DONATION" >/dev/null \
  || fail "initialize failed"
ok "initialized"

# ── [8] Smoke: hello + version ───────────────────────────────────────────────
step "Smoke: hello() and version()"
HELLO=$(invoke "$KEY_PREFIX-creator" -- hello) || fail "hello() failed"
VERSION=$(invoke "$KEY_PREFIX-creator" -- version) || fail "version() failed"
ok "hello=$HELLO version=$VERSION"

# ── [9] donate (donor-authorized) ────────────────────────────────────────────
step "donate: $DONATION from donor (reaches goal → unlocks milestone)"
invoke "$KEY_PREFIX-donor" -- donate \
  --donor "$DONOR_ADDR" \
  --amount "$DONATION" \
  --asset '{"Stellar":"'"$SAC_ID"'"}' >/dev/null \
  || fail "donate failed"
ok "donated"

# ── [10] Assert post-donation state ──────────────────────────────────────────
step "Assert: total raised, campaign status, milestone unlocked"
RAISED=$(invoke "$KEY_PREFIX-creator" -- get_total_raised) || fail "get_total_raised failed"
echo "$RAISED" | grep -q "$DONATION" || fail "total raised: expected $DONATION, got $RAISED"
STATUS=$(invoke "$KEY_PREFIX-creator" -- get_campaign_status) || fail "get_campaign_status failed"
echo "$STATUS" | grep -q "GoalReached" || fail "campaign status: expected GoalReached, got: $STATUS"
MILESTONE=$(invoke "$KEY_PREFIX-creator" -- get_milestone_view --index 0) || fail "get_milestone_view failed"
echo "$MILESTONE" | grep -q "Unlocked" || fail "milestone 0: expected Unlocked, got: $MILESTONE"
ok "raised=$DONATION status=GoalReached milestone=Unlocked"

# ── [11] Fund the contract's XLM balance ─────────────────────────────────────
# donate() records the pledge on-ledger but does not custody tokens; release
# pays out from the contract's own SAC balance, so the donor settles here.
step "Settle pledge: SAC transfer donor → contract ($DONATION stroops)"
run contract invoke --id "$SAC_ID" --source-account "$KEY_PREFIX-donor" "${NET[@]}" -- transfer \
  --from "$DONOR_ADDR" --to "$CONTRACT_ID" --amount "$DONATION" >/dev/null \
  || fail "SAC transfer to contract failed"
ok "contract funded"

# ── [12] release_milestone (creator-authorized) ──────────────────────────────
step "release_milestone: index 0 → beneficiary"
invoke "$KEY_PREFIX-creator" -- release_milestone \
  --milestone_index 0 --recipient "$BENEFICIARY_ADDR" >/dev/null \
  || fail "release_milestone failed"
ok "released"

# ── [13] Assert release: milestone state + real token balance ────────────────
step "Assert: milestone Released, beneficiary received funds, release count"
MILESTONE=$(invoke "$KEY_PREFIX-creator" -- get_milestone_view --index 0) || fail "get_milestone_view failed"
echo "$MILESTONE" | grep -q "Released" || fail "milestone 0: expected Released, got: $MILESTONE"
BALANCE=$(run contract invoke --id "$SAC_ID" --source-account "$KEY_PREFIX-creator" "${NET[@]}" -- balance --id "$BENEFICIARY_ADDR") \
  || fail "SAC balance query failed"
echo "$BALANCE" | grep -q "$DONATION" || fail "beneficiary balance: expected $DONATION, got $BALANCE"
RELEASES=$(invoke "$KEY_PREFIX-creator" -- get_release_count) || fail "get_release_count failed"
echo "$RELEASES" | grep -q "1" || fail "release count: expected 1, got $RELEASES"
ok "milestone=Released beneficiary_balance=$DONATION releases=$RELEASES"

report 0
