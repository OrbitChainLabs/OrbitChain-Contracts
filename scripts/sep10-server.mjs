#!/usr/bin/env node
// scripts/sep10-server.mjs — Reference SEP-10 auth server for local dev, the
// curl smoke test, and the Playwright CI job.
//
// This is NOT the production OrbitChain auth server (none exists yet in this
// repo) — it is a minimal, spec-correct SEP-10 challenge/verify endpoint
// that wallet_connect.js talks to, so the mobile deep-link handshake can be
// exercised end-to-end without a real wallet or a live backend.
//
// Endpoints:
//   GET  /auth?account=<G...>&home_domain=<domain>
//        -> { transaction: "<base64 xdr>", network_passphrase }
//   POST /auth   body: { "transaction": "<base64 xdr signed by the client>" }
//        -> { token: "<jwt>" }
//
// Env:
//   SEP10_PORT               default 4000
//   SEP10_HOME_DOMAIN         default "localhost"
//   SEP10_SERVER_SECRET       server signing keypair secret (Stellar S...);
//                             a fixed dev keypair is used if unset — do not
//                             reuse that default outside local dev/CI.
//   SEP10_JWT_SECRET          HMAC secret for session tokens; a fixed dev
//                             value is used if unset, same caveat as above.
//   SEP10_NETWORK_PASSPHRASE  default Networks.TESTNET
//
// Closes issue: "Mobile wallet deep-link & Lobstr / Bitnovo support"

import { createServer } from 'node:http';
import { createHmac } from 'node:crypto';
import { Keypair, Networks, StrKey, WebAuth } from '@stellar/stellar-sdk';

const PORT = Number(process.env.SEP10_PORT || 4000);
const HOME_DOMAIN = process.env.SEP10_HOME_DOMAIN || 'localhost';
const NETWORK_PASSPHRASE = process.env.SEP10_NETWORK_PASSPHRASE || Networks.TESTNET;
const CHALLENGE_TIMEOUT_SECONDS = 300;
const SESSION_TTL_SECONDS = 3600;

// Fixed dev-only defaults so `npm run sep10:server` works with zero setup.
// DEV_SECRET below is a well-known, publicly-committed test keypair — never
// point this server at mainnet or reuse this secret for anything real.
const DEV_SERVER_SECRET = 'SCSAY7YXJF7Q2OVPVPFRSHTIAUGL5E3ANMV3C2OHOUC47BHTL42JZSXP';
const SERVER_KEYPAIR = Keypair.fromSecret(process.env.SEP10_SERVER_SECRET || DEV_SERVER_SECRET);
const JWT_SECRET = process.env.SEP10_JWT_SECRET || 'orbitchain-dev-sep10-jwt-secret-do-not-use-in-prod';

function base64url(input) {
  return Buffer.from(input).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function issueJwt(subject) {
  const header = base64url(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
  const now = Math.floor(Date.now() / 1000);
  const payload = base64url(JSON.stringify({
    iss: `http://localhost:${PORT}/auth`,
    sub: subject,
    iat: now,
    exp: now + SESSION_TTL_SECONDS,
  }));
  const signature = base64url(createHmac('sha256', JWT_SECRET).update(`${header}.${payload}`).digest());
  return `${header}.${payload}.${signature}`;
}

function sendJson(res, status, body) {
  const data = JSON.stringify(body);
  res.writeHead(status, { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' });
  res.end(data);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = '';
    req.on('data', (chunk) => { data += chunk; });
    req.on('end', () => resolve(data));
    req.on('error', reject);
  });
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`);

  if (req.method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    });
    return res.end();
  }

  if (url.pathname !== '/auth') {
    return sendJson(res, 404, { error: 'not found' });
  }

  try {
    if (req.method === 'GET') {
      const account = url.searchParams.get('account');
      const homeDomain = url.searchParams.get('home_domain') || HOME_DOMAIN;

      if (!account || !StrKey.isValidEd25519PublicKey(account)) {
        return sendJson(res, 400, { error: 'missing or invalid "account" query param' });
      }

      const transaction = WebAuth.buildChallengeTx(
        SERVER_KEYPAIR,
        account,
        homeDomain,
        CHALLENGE_TIMEOUT_SECONDS,
        NETWORK_PASSPHRASE,
        homeDomain, // web_auth_domain: this reference server only ever serves one domain
      );

      return sendJson(res, 200, { transaction, network_passphrase: NETWORK_PASSPHRASE });
    }

    if (req.method === 'POST') {
      const body = await readBody(req);
      let transaction;
      try {
        ({ transaction } = JSON.parse(body));
      } catch {
        return sendJson(res, 400, { error: 'invalid JSON body' });
      }
      if (!transaction) {
        return sendJson(res, 400, { error: 'missing "transaction" field' });
      }

      const { tx, clientAccountID } = WebAuth.readChallengeTx(
        transaction,
        SERVER_KEYPAIR.publicKey(),
        NETWORK_PASSPHRASE,
        [HOME_DOMAIN],
        HOME_DOMAIN,
      );

      WebAuth.verifyChallengeTxSigners(
        transaction,
        SERVER_KEYPAIR.publicKey(),
        NETWORK_PASSPHRASE,
        [clientAccountID],
        [HOME_DOMAIN],
        HOME_DOMAIN,
      );

      void tx; // validated above; only clientAccountID is needed for the token
      const token = issueJwt(clientAccountID);
      return sendJson(res, 200, { token });
    }

    sendJson(res, 405, { error: 'method not allowed' });
  } catch (err) {
    sendJson(res, 400, { error: err.message || 'invalid challenge' });
  }
});

server.listen(PORT, () => {
  console.log(`[sep10-server] listening on http://localhost:${PORT}`);
  console.log(`[sep10-server] server account: ${SERVER_KEYPAIR.publicKey()}`);
  console.log(`[sep10-server] home_domain:    ${HOME_DOMAIN}`);
  console.log(`[sep10-server] network:        ${NETWORK_PASSPHRASE}`);
});
