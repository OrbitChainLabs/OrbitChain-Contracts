#!/usr/bin/env bash
# Issue #117 – WASM size regression baseline.
#
# Compares each wasm-opt'd contract against a checked-in byte budget and fails
# if any exceeds it. Budgets are the measured optimized sizes at the time this
# gate landed plus ~5% headroom, so ordinary growth fits but a regression that
# would erase the optimization win (or a broken wasm-opt invocation, which
# typically shows up as a near-unoptimized size) fails loudly.
#
# Measured baselines (wasm-opt 130, -Oz, 2026-07-29):
#   orbitchain_campaign      85322 -> 63758  (-25.3%)
#   orbitchain_core          38767 -> 25919  (-33.1%)
#   orbitchain_batch_donor   18072 -> 13260  (-26.6%)
#   orbitchain_token_bridge   1454 ->  1010  (-30.5%)
#   orbitchain_common          817 ->   791  ( -3.2%)
#
# To raise a budget intentionally, change it here in the same PR that grows
# the contract, so the growth is visible in review.
set -euo pipefail

DIR="target/wasm32v1-none/release"

# Budgets were measured with binaryen 130 — older wasm-opt produces larger
# output and will trip them. CI pins the version; report it for debugging.
echo "ℹ️  $(wasm-opt --version)"

check() {
  local name="$1" budget="$2"
  local f="$DIR/${name}.optimized.wasm"
  if [[ ! -f "$f" ]]; then
    echo "❌ $f missing — run 'make optimize' first"
    exit 1
  fi
  local size
  size=$(wc -c < "$f" | tr -d ' ')
  if (( size > budget )); then
    echo "❌ ${name}: ${size}B exceeds budget ${budget}B"
    FAILED=1
  else
    printf "✅ %-28s %7dB (budget %7dB)\n" "$name" "$size" "$budget"
  fi
}

FAILED=0
check orbitchain_campaign     67000
check orbitchain_core         27500
check orbitchain_batch_donor  14000
check orbitchain_token_bridge  1100
check orbitchain_common         850

if (( FAILED )); then
  echo "❌ WASM size budget exceeded — see docs/wasm-size.md"
  exit 1
fi
echo "✅ All optimized WASM binaries within budget"
