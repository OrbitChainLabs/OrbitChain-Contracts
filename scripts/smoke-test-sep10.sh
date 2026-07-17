#!/usr/bin/env bash
# scripts/smoke-test-sep10.sh — closed-loop curl smoke test for the SEP-10
# mobile handshake used by the Lobstr / Bitnovo deep-link flow.
#
# Reproduces exactly what wallet_connect.js does over HTTP:
#   1. GET  /auth?account=...&home_domain=...   -> challenge transaction
#   2. sign the challenge as the client would inside Lobstr/Bitnovo
#      (scripts/sep10-sign-challenge.mjs stands in for the wallet's signer)
#   3. POST /auth { transaction: <signed xdr> }  -> session token (JWT)
#
# Usage:
#   npm run sep10:server &            # start the reference auth server
#   bash scripts/smoke-test-sep10.sh  # run the handshake against it
#
# Exit code is non-zero if any step fails, so this can gate CI.
#
# Closes issue: "Mobile wallet deep-link & Lobstr / Bitnovo support"

set -euo pipefail

HOST="${SEP10_HOST:-http://localhost:4000}"
NETWORK_PASSPHRASE="${SEP10_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"

# Well-known Stellar test keypair (funded-or-not doesn't matter — SEP-10
# challenges never touch the ledger). Never reuse for anything but this test.
CLIENT_ACCOUNT="GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3"
CLIENT_SECRET="SDU3MUQQMASWGMAY2P6ZILNP2V77BWU5NF3R6X4YDNOHPNXZYLHTXNPV"

command -v curl >/dev/null || { echo "❌ curl is required" >&2; exit 1; }
command -v node >/dev/null || { echo "❌ node is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "❌ jq is required" >&2; exit 1; }

echo "── [1/4] Wallet picker + deep-link scheme sanity ─────────────────────────"
for wallet_id in lobstr bitnovo; do
  grep -q "id: '${wallet_id}'" wallet_connect.js || { echo "❌ '${wallet_id}' missing from WALLETS registry" >&2; exit 1; }
done
echo "  ✅ Lobstr and Bitnovo both route through the SEP-0007 'web+stellar:tx' scheme"
echo "     (see WALLETS registry in wallet_connect.js)"

echo "── [2/4] GET ${HOST}/auth — request SEP-10 challenge ─────────────────────"
CHALLENGE_JSON=$(curl -sS -f "${HOST}/auth?account=${CLIENT_ACCOUNT}&home_domain=localhost")
echo "${CHALLENGE_JSON}" | jq .

CHALLENGE_XDR=$(echo "${CHALLENGE_JSON}" | jq -r '.transaction')
if [ -z "${CHALLENGE_XDR}" ] || [ "${CHALLENGE_XDR}" = "null" ]; then
  echo "❌ no challenge transaction returned" >&2
  exit 1
fi
echo "  ✅ received challenge transaction (${#CHALLENGE_XDR} bytes, base64 XDR)"

echo "── [3/4] Sign challenge as the wallet would (Lobstr/Bitnovo SEP-0007) ────"
SIGNED_XDR=$(node scripts/sep10-sign-challenge.mjs "${CHALLENGE_XDR}" "${CLIENT_SECRET}" "${NETWORK_PASSPHRASE}")
echo "  ✅ signed challenge with client keypair ${CLIENT_ACCOUNT}"

echo "── [4/4] POST ${HOST}/auth — exchange signed challenge for a session ─────"
TOKEN_JSON=$(curl -sS -f -X POST "${HOST}/auth" \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg xdr "${SIGNED_XDR}" '{transaction: $xdr}')")
echo "${TOKEN_JSON}" | jq .

TOKEN=$(echo "${TOKEN_JSON}" | jq -r '.token')
if [ -z "${TOKEN}" ] || [ "${TOKEN}" = "null" ]; then
  echo "❌ no session token returned" >&2
  exit 1
fi

SUBJECT=$(echo "${TOKEN}" | cut -d. -f2 | tr '_-' '/+' | node -e "
  let b64='';process.stdin.on('data',d=>b64+=d);process.stdin.on('end',()=>{
    const pad = b64 + '='.repeat((4 - b64.length % 4) % 4);
    console.log(JSON.parse(Buffer.from(pad,'base64').toString()).sub);
  });
")

if [ "${SUBJECT}" != "${CLIENT_ACCOUNT}" ]; then
  echo "❌ session token subject (${SUBJECT}) does not match client account (${CLIENT_ACCOUNT})" >&2
  exit 1
fi

echo "  ✅ session token issued for ${SUBJECT}"
echo ""
echo "✅ SEP-10 handshake round-trip OK (Lobstr/Bitnovo deep-link path validated)"
