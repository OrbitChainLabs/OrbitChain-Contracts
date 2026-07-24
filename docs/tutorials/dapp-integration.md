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
production. **Both override paths are gated by an origin allowlist**; see
[Page hardening](#page-hardening-csp-sri-strkey-network-allowlists) below.

### SEP-45 note

The issue references SEP-45-style redirects. SEP-45 is Stellar's
web-authentication spec **for smart-wallet / contract accounts**
(passkeys, Soroban `Address` accounts). OrbitChain's campaign contract
currently operates against classic Stellar accounts (`G...` keys), so
SEP-10 is the correct spec today. SEP-45 support is a natural follow-up
once campaign donations accept Soroban smart-wallet signers — tracked
against the same umbrella issue (#37) rather than bundled here.

## Page hardening: CSP, SRI, StrKey, network allowlists

Issue #141. `wallet_connect.html` is the browser front door to a fund-handling
protocol, so four independent controls sit between a hostile page/network and a
signature. **Three of the four are environment-specific and a real deployment
must edit them** — the list below says exactly what to change.

### 1. Content Security Policy

Declared as a `<meta http-equiv="Content-Security-Policy">` in
`wallet_connect.html`:

```
default-src 'none'; script-src 'self' https://cdnjs.cloudflare.com;
style-src 'self'; img-src 'self' data:;
connect-src 'self' http://localhost:4000;
form-action 'none'; base-uri 'none'
```

`default-src 'none'` denies every fetch type; each directive re-opens exactly
one. Two things to know before deploying:

- **`connect-src` hard-codes the dev auth-server origin.** The page is served
  on `:8055` and the reference SEP-10 server on `:4000`, so `'self'` alone
  breaks the handshake. A `<meta>` CSP cannot be templated per environment —
  **replace `http://localhost:4000` with your production auth origin.**
- **`frame-ancestors` is deliberately absent.** Browsers ignore it (along with
  `report-uri` and `sandbox`) when the policy arrives via `<meta>`, so listing
  it would buy nothing and log a console warning. Send it as a real HTTP header
  instead — clickjacking protection for a wallet page is worth the server
  config:

  ```
  Content-Security-Policy: frame-ancestors 'none'
  ```

Because `style-src` is `'self'` with no hashes and no `'unsafe-inline'`, all
CSS lives in `wallet_connect.css`. Setting `el.style.display` from JS (which
`createDomUi` does throughout) is CSSOM, **not** an inline style attribute, so
it is unaffected — `tests/e2e/wallet-connect-csp.spec.js` verifies that in a
real browser rather than assuming it.

### 2. Subresource integrity on the Freighter CDN script

```html
<script
  src="https://cdnjs.cloudflare.com/ajax/libs/stellar-freighter-api/6.0.1/index.min.js"
  integrity="sha256-15dCnPl/mmfxmZkQgwqJ3AHVZa7euvtlGhGx7QA6fp0="
  crossorigin="anonymous"
  defer
></script>
```

If cdnjs ever serves different bytes the browser refuses to execute the script
and the page fails closed to manual entry. `crossorigin="anonymous"` is
mandatory for SRI on a cross-origin script; cdnjs answers with
`Access-Control-Allow-Origin: *`, which the e2e spec asserts rather than
assumes.

The same hash is pinned in `wallet_connect.test.js` (`FREIGHTER_SRI_HASH`) so
drift fails CI loudly instead of silently disabling Freighter. To re-verify or
to bump the pinned version:

```bash
curl -s https://cdnjs.cloudflare.com/ajax/libs/stellar-freighter-api/6.0.1/index.min.js \
  | openssl dgst -sha256 -binary | openssl base64 -A
```

Update the hash in **both** places, and only after reviewing what changed.

### 3. Real StrKey validation

`isValidPublicKey` decodes the address instead of pattern-matching it: strict
RFC 4648 base32 → 35 bytes → SEP-23 version byte (`6 << 3` for `G`) →
CRC16-XModem checksum over the first 33 bytes, compared little-endian against
the last two. The previous `/^G[A-Z2-7]{55}$/` regex accepted any 56-character
base32 string, including every mistyped or truncated paste.

Secret keys (`S…`) decode successfully and are **rejected**, with their own
error message — a wallet-connect page must never accept a secret. Both the
desktop manual form and the mobile SEP-10 entry point route through this one
validator; `startMobileConnect` previously checked only the first character and
the length.

No dependency was added: the decoder is ~60 lines of vanilla JS (the page has
no bundler). `wallet_connect.test.js` cross-validates it against
`@stellar/stellar-sdk`'s `StrKey` over a fixture table plus 400 generated keys
and single-character corruptions, so correctness is proven against the
reference implementation without shipping it to the browser.

> **Migration note.** Sessions saved in `localStorage` under the old regex may
> now fail validation on restore. `readSession` discards them, so the effect is
> "the user reconnects" — the correct degradation, and covered by a test.

### 4. Network allowlists

Deliberately redundant with `connect-src`: CSP is the browser's enforcement,
these are the page's own. Same defence-in-depth idiom as
`assert_asset_is_accepted` in `campaign/src/asset_auth.rs`.

| Allowlist | Where | Behaviour when it fails |
|---|---|---|
| `AUTH_ORIGIN_ALLOWLIST` | `wallet_connect.js`, next to `resolveAuthServer()` | Falls back to the default auth server, logs to the console, and surfaces `Error: …` in the `#status` live region. The rejected origin is **never fetched.** |
| `NETWORK_PASSPHRASE_ALLOWLIST` | `Sep10.fetchChallenge` | Throws before the challenge reaches a wallet. |

**Pointing the page at a non-default auth server** takes two edits, and the
unit tests will tell you if you only make one:

1. Add the origin to `AUTH_ORIGIN_ALLOWLIST` in `wallet_connect.js`. The list
   ships with `http://localhost:4000`, `http://127.0.0.1:4000`, and
   `https://auth.orbitchain.invalid` — the last is a **placeholder**, using the
   RFC 2606 reserved `.invalid` TLD precisely so it can never resolve. Replace
   it with the real origin once the production auth server exists.
2. Add the same origin to `connect-src` in the CSP meta tag, or the browser
   blocks the request even though the JS allowed it.

Only the *origin* is matched, so a path-carrying endpoint
(`https://auth.example.com/sep10`) is fine.

`NETWORK_PASSPHRASE_ALLOWLIST` holds Testnet and Public. A challenge whose
`network_passphrase` is missing, empty, or anything else is refused rather than
guessed at: signing an unknown network's transaction is how a malicious auth
server would obtain a signature to replay where the user never agreed. The
reference server returns the passphrase already (`scripts/sep10-server.mjs`);
`fetchChallenge` used to discard it.

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

[`tests/e2e/wallet-connect-csp.spec.js`](../../tests/e2e/wallet-connect-csp.spec.js)
covers the hardening above by registering a `securitypolicyviolation` listener
before page scripts run and asserting **zero** violations across page load,
desktop connect, the chooser, manual entry, the mobile picker, the SEP-10 fetch
and the deep-link navigation. It also carries a negative control that injects a
blocked inline `<style>` and a foreign script and asserts violations *are*
reported — otherwise "zero violations" would pass just as happily with no
policy at all.

[`tests/e2e/wallet-connect-desktop-multi.spec.js`](../../tests/e2e/wallet-connect-desktop-multi.spec.js)
covers the desktop extension registry. Note that every account fixture in the
e2e specs must now be checksum-valid: since #141 the page decodes addresses, so
a mnemonic placeholder like `GFREIGHTERSHOULDNOTBEUSED…` is rejected as
malformed and would stop proving whatever it was there to prove.

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
