# dApp Integration: Wallet Connect (Desktop + Mobile)

This tutorial covers how `wallet_connect.html` authenticates a Stellar
account, on both desktop and mobile, and how to test each path locally and
in CI.

- **Desktop**: [Freighter](https://www.freighter.app/) browser extension —
  unchanged, see the `window.freighter` branch in `wallet_connect.html`.
- **Mobile**: no browser extension exists on iOS/Android, so
  `wallet_connect.js` routes to a wallet picker (Lobstr, Bitnovo, or any
  other [SEP-0007](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0007.md)-compatible
  wallet) and completes a [SEP-0010](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md)
  web-authentication handshake via deep link.

Tracked under the mobile wallet deep-link issue, part of the umbrella
tracker [issue #37](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/37);
depends on the CLI/contract stabilization work in #54 and #55.

## Why mobile needs a different flow

`window.freighter` only exists because Freighter injects itself into the
page as a desktop browser extension. Mobile browsers have no equivalent
injection point, and the Stellar mobile wallets that matter here — Lobstr
and Bitnovo — are apps, not extensions. The bridge between "web page" and
"wallet app" on mobile is a **deep link**: the page redirects the OS to a
URI the wallet app has registered itself to handle
(`web+stellar:tx?xdr=...`, per SEP-0007), the wallet shows the user what
they're signing, and it redirects back to a callback URL with the result.

That means the mobile flow is two round trips instead of one function call:

1. **Identify** — the browser has no way to ask an app "what's your public
   key" up front, so the user supplies the Stellar public key they intend to
   authenticate with (copied from the wallet's "Receive" screen). This is
   the same manual-entry pattern `wallet_connect.html` already used as its
   Freighter-not-found fallback; the mobile flow reuses it deliberately
   instead of inventing a second UX for the same problem.
2. **Challenge / sign / verify** — once the account is known, the page can
   request a real SEP-10 challenge, hand it to the wallet app via deep link,
   and verify the signature when the wallet redirects back.

## Detecting mobile

`wallet_connect.js` checks `navigator.userAgent` against a conservative
regex (`Android|iPhone|iPad|iPod|Mobile|IEMobile|BlackBerry|webOS`). Desktop
user agents never match, so the existing Freighter path is completely
unaffected — this is additive, not a rewrite. See `isMobile()` in
[`wallet_connect.js`](../../wallet_connect.js).

## The wallet picker

When `connectWallet()` detects a mobile UA, it opens the `#wallet-picker`
modal instead of calling `window.freighter`. The wallet registry
(`WALLETS` in `wallet_connect.js`) currently lists:

| Wallet | Deep link scheme | Notes |
|---|---|---|
| Lobstr | `web+stellar:tx?xdr=...&callback=url:...` | SEP-0007 `tx` request; [Lobstr SEP-0007 docs](https://lobstr.co/) |
| Bitnovo Wallet | `web+stellar:tx?xdr=...&callback=url:...` | SEP-0007 `tx` request; fallback custom scheme `bitnovowallet://sep0007?...` if the universal link doesn't resolve to an installed app |
| Other SEP-0007 wallet | `web+stellar:tx?xdr=...&callback=url:...` | Generic entry for any wallet that implements the spec (xBull, Rabet mobile, etc.) |

Both `buildDeepLink` scheme strings are the standards-track SEP-0007 URI.
The vendor-specific `fallbackScheme` entries are best-effort — confirm
against each wallet's current developer docs before depending on them in
production, since undocumented custom schemes can change without notice.

## The SEP-10 handshake

```
Browser (mobile UA)                 Auth server                Wallet app (Lobstr/Bitnovo)
       |                                  |                              |
       |-- 1. GET /auth?account=G...  --->|                              |
       |<--------- challenge XDR ---------|                              |
       |                                  |                              |
       |-- 2. redirect: web+stellar:tx?xdr=<challenge>&callback=url:<returnUrl> ---------->|
       |                                                                 |
       |                                          user reviews & signs  |
       |<----------------- redirect: <returnUrl>?xdr=<signed> ----------|
       |                                  |                              |
       |-- 3. POST /auth {transaction} -->|                              |
       |<---------- { token } ------------|                              |
```

1. `Sep10.fetchChallenge(account)` — `GET {AUTH_SERVER}/auth?account=...&home_domain=...`
   returns `{ transaction, network_passphrase }` (a `ManageData`-based
   challenge transaction per SEP-10, signed by the server so the client can
   trust its origin).
2. `startMobileConnect()` builds the SEP-0007 deep link for the chosen
   wallet, embedding the challenge XDR and a callback URL that points back
   at `wallet_connect.html` with a marker query param
   (`?orbitchain_xdr=1`), then does `window.location.href = deepLink`. The
   mobile OS switches to the wallet app; JS execution on the page pauses
   here.
3. The wallet app shows the transaction (a zero-amount, zero-effect
   `ManageData` op — nothing is transferred), the user approves, and the
   wallet redirects back to the callback URL with `?xdr=<signed-xdr>`
   appended, per the SEP-0007 `callback=url:` response convention.
4. On page load, `handleMobileReturn()` detects `orbitchain_xdr=1` in the
   URL, `POST`s the signed transaction to `{AUTH_SERVER}/auth`, and the
   server verifies the signature belongs to the claimed account
   (`WebAuth.verifyChallengeTxSigners`) before issuing a JWT session token.
   The page then decodes the token's `sub` claim to show the connected
   address, mirroring `showConnected()` on the desktop path.

`AUTH_SERVER` defaults to `http://localhost:4000` and is overridable via
`window.ORBITCHAIN_AUTH_SERVER` or `<script data-auth-server="...">` on the
`wallet_connect.js` tag — point it at your real SEP-10 endpoint in
production.

### SEP-45 note

The issue references SEP-45-style redirects. SEP-45 is Stellar's
web-authentication spec **for smart-wallet / contract accounts**
(passkeys, Soroban `Address` accounts). OrbitChain's campaign contract
currently operates against classic Stellar accounts (`G...` keys), so
SEP-10 is the correct spec today. SEP-45 support is a natural follow-up
once campaign donations accept Soroban smart-wallet signers — tracked
against the same umbrella issue (#37) rather than bundled here.

## Reference SEP-10 auth server (dev/test only)

This repo has no production auth backend yet, so
[`scripts/sep10-server.mjs`](../../scripts/sep10-server.mjs) provides a
minimal, spec-correct SEP-10 challenge/verify server for local development,
the curl smoke test, and the Playwright CI job. It is **not** meant for
production — swap `AUTH_SERVER` to a real endpoint before deploying.

```bash
npm install
npm run sep10:server
# [sep10-server] listening on http://localhost:4000
# [sep10-server] server account: GDTXR3JTIVAWQRBCR35PMRZAFT2ISBBJXEWRW2NHBY7SH4S75AUDVA7B
# [sep10-server] home_domain:    localhost
# [sep10-server] network:        Test SDF Network ; September 2015
```

Then open `wallet_connect.html` (e.g. `python3 -m http.server 8055`) from a
mobile device or a mobile emulation profile in your browser's dev tools.

## Closed-loop curl smoke test

[`scripts/smoke-test-sep10.sh`](../../scripts/smoke-test-sep10.sh)
reproduces the exact handshake `wallet_connect.js` performs over HTTP, using
[`scripts/sep10-sign-challenge.mjs`](../../scripts/sep10-sign-challenge.mjs)
to stand in for the wallet's signature step (Lobstr/Bitnovo would show a
signing prompt here; this script signs with a well-known test keypair
instead so the loop can run headless):

```bash
npm run sep10:server &
bash scripts/smoke-test-sep10.sh
```

### Expected output

```
── [1/4] Wallet picker + deep-link scheme sanity ─────────────────────────
  ✅ Lobstr and Bitnovo both route through the SEP-0007 'web+stellar:tx' scheme
     (see WALLETS registry in wallet_connect.js)
── [2/4] GET http://localhost:4000/auth — request SEP-10 challenge ─────────────────────
{
  "transaction": "AAAAAgAAAAD...",
  "network_passphrase": "Test SDF Network ; September 2015"
}
  ✅ received challenge transaction (508 bytes, base64 XDR)
── [3/4] Sign challenge as the wallet would (Lobstr/Bitnovo SEP-0007) ────
  ✅ signed challenge with client keypair GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3
── [4/4] POST http://localhost:4000/auth — exchange signed challenge for a session ─────
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...."
}
  ✅ session token issued for GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3

✅ SEP-10 handshake round-trip OK (Lobstr/Bitnovo deep-link path validated)
```

A non-zero exit code means the handshake broke somewhere (bad challenge,
signature verification failure, token mismatch) — safe to wire into CI as a
hard gate, and it is (`mobile-wallet-e2e` job in
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)).

## Playwright mobile-viewport tests

[`tests/e2e/wallet-connect-mobile.spec.js`](../../tests/e2e/wallet-connect-mobile.spec.js)
drives `wallet_connect.html` in real mobile browser contexts
(`devices['iPhone 13']` and `devices['Pixel 7']`, via
[`playwright.config.js`](../../playwright.config.js)), plus a
`desktop-chromium` project for the regression check. It covers:

- the wallet picker appearing on mobile UAs and listing Lobstr + Bitnovo;
- picking either wallet firing a real `GET /auth?account=...` SEP-10
  challenge request for the entered account;
- invalid public keys being rejected client-side before any network call;
- the desktop UA never seeing the picker (Freighter path unaffected);
- the full return handshake: a pre-signed challenge is appended to the URL
  exactly as a wallet's callback redirect would, and the page is expected
  to show "Wallet Connected" with the matching address.

```bash
npm install
npx playwright install --with-deps chromium webkit
npm run test:e2e
```

`playwright.config.js` starts both the static file server (`python3 -m
http.server`) and the reference SEP-10 server automatically, so no manual
setup is needed beyond `npm install` + browser install.

## Manual testing on a real device

1. Serve the repo root over HTTPS or on the same LAN as your phone (Lobstr
   and Bitnovo both require the deep link to be triggered from an actual
   page load, not `file://`).
2. Start `npm run sep10:server` on a host reachable from your phone, and set
   `window.ORBITCHAIN_AUTH_SERVER` in `wallet_connect.html` accordingly.
3. Open the page on your phone, tap **Connect Wallet**, pick Lobstr or
   Bitnovo, paste your account's public key, and confirm the app opens with
   a signing prompt for a `manage_data` operation (not a payment — SEP-10
   challenges never move funds).
4. Approve in the wallet app and confirm you're redirected back to
   `wallet_connect.html` showing **Wallet Connected**.
