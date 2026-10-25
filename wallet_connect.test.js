"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");

// The reference StrKey implementation. Used only to cross-validate the vanilla
// decoder in wallet_connect.js — the SDK is a devDependency and is never
// shipped to the browser (issue #141: "Dependencies: None").
const { StrKey } = require("@stellar/stellar-sdk");

const {
  WALLET_SESSION_KEY,
  createDomUi,
  createWalletSession,
  initializeWalletPage,
  isValidPublicKey,
  isSecretKey,
  publicKeyError,
  readSession,
} = require("./wallet_connect.js");

// Requiring wallet_connect.js installs the mobile/SEP-10 module on globalThis
// (there is no `window` under node), which is where the network allowlists live.
const orbit = globalThis.OrbitWalletConnect;

// Deterministic, checksum-valid test keys (all-0x11 and all-0x22 seeds).
// The previous `G` + "A".repeat(55) / "B".repeat(55) fixtures are shape-valid
// but checksum-INVALID — they only ever passed because the old validator was a
// regex. Both are cross-checked against @stellar/stellar-sdk's StrKey below.
const ADDRESS_A = "GAIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCF6M";
const ADDRESS_B = "GARCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCEIRCFRVX";

function createStorage(initial = {}) {
  const values = new Map(Object.entries(initial));

  return {
    getItem(key) {
      return values.has(key) ? values.get(key) : null;
    },
    setItem(key, value) {
      values.set(key, String(value));
    },
    removeItem(key) {
      values.delete(key);
    },
  };
}

function createUi() {
  return {
    state: null,
    showConnecting() {
      this.state = { view: "connecting" };
    },
    showReconnecting() {
      this.state = { view: "reconnecting" };
    },
    showConnected(publicKey, statusText) {
      this.state = { view: "connected", publicKey, statusText };
    },
    showDisconnected({ reconnect, statusText }) {
      this.state = { view: "disconnected", reconnect, statusText };
    },
  };
}

function createHarness({
  storage = createStorage(),
  provider = null,
  promptForAddress = () => null,
} = {}) {
  const ui = createUi();
  const events = [];
  const session = createWalletSession({
    storage,
    provider,
    promptForAddress,
    ui,
    emit(name, detail) {
      events.push({ name, detail });
    },
  });

  return { events, session, storage, ui };
}

function createFakeDocument() {
  const elements = {
    button: {
      textContent: "Connect Wallet",
      disabled: false,
      listener: null,
      addEventListener(type, listener) {
        if (type === "click") this.listener = listener;
      },
    },
    walletInfo: { style: { display: "none" } },
    walletAddress: { textContent: "" },
    status: { textContent: "" },
  };
  const byId = {
    "connect-btn": elements.button,
    "wallet-info": elements.walletInfo,
    "wallet-address": elements.walletAddress,
    status: elements.status,
  };

  return {
    document: {
      getElementById(id) {
        return byId[id] ?? null;
      },
    },
    elements,
  };
}

// ─── StrKey validation (issue #141) ───────────────────────────────────────

// Real accounts used elsewhere in the repo (e2e specs, SEP-10 smoke test).
const REAL_ACCOUNT_1 = "GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3";
const REAL_ACCOUNT_2 = "GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W";
// Checksum-valid secret seed (the SEP-10 test client's, already public in
// tests/e2e/wallet-connect-mobile.spec.js — it is a well-known throwaway).
const REAL_SECRET = "SDU3MUQQMASWGMAY2P6ZILNP2V77BWU5NF3R6X4YDNOHPNXZYLHTXNPV";

test("accepts checksum-valid Stellar public keys", () => {
  for (const key of [ADDRESS_A, ADDRESS_B, REAL_ACCOUNT_1, REAL_ACCOUNT_2]) {
    assert.equal(isValidPublicKey(key), true, `${key} should be valid`);
  }
});

test("rejects addresses that are shape-valid but checksum-invalid", () => {
  // Exactly the class of input the old /^G[A-Z2-7]{55}$/ regex let through.
  assert.equal(isValidPublicKey(`G${"A".repeat(55)}`), false);
  assert.equal(isValidPublicKey(`G${"B".repeat(55)}`), false);
  assert.equal(
    isValidPublicKey("GFREIGHTERSHOULDNOTBEUSEDINTHISSPECAAAAAAAAAAAAAAAAAAAAA"),
    false,
  );
  // One character flipped in an otherwise real key.
  assert.equal(isValidPublicKey(`${REAL_ACCOUNT_1.slice(0, 55)}4`), false);
});

test("rejects bad charset, wrong length, and non-string input", () => {
  const cases = [
    ADDRESS_A.toLowerCase(), // lowercase is not in the RFC 4648 alphabet
    ADDRESS_A.replace("I", "1"), // 1 / 0 / 8 / 9 are excluded from base32
    ADDRESS_A.replace("A", "0"),
    ADDRESS_A.replace("R", "8"),
    `${ADDRESS_A}=`, // padding
    ADDRESS_A.slice(0, 55), // too short
    `${ADDRESS_A}A`, // too long
    "G123",
    "",
    "   ",
    // Unicode lookalike: Cyrillic А (U+0410) in place of Latin A.
    ADDRESS_A.replace("A", "А"),
    null,
    undefined,
    42,
    {},
    [],
  ];

  for (const value of cases) {
    assert.equal(isValidPublicKey(value), false, `${String(value)} should be rejected`);
  }
});

test("recognises but never accepts secret keys", () => {
  assert.equal(isSecretKey(REAL_SECRET), true);
  assert.equal(isValidPublicKey(REAL_SECRET), false);
  // A shape-only 'S' string is neither.
  assert.equal(isSecretKey(`S${"A".repeat(55)}`), false);
  assert.equal(isValidPublicKey(`S${"A".repeat(55)}`), false);
});

test("a pasted secret key gets its own error message", () => {
  assert.match(publicKeyError(REAL_SECRET), /secret key/i);
  assert.match(publicKeyError("nonsense"), /valid Stellar public key/);
});

test("agrees with @stellar/stellar-sdk StrKey across the fixture table", () => {
  const fixtures = [
    ADDRESS_A,
    ADDRESS_B,
    REAL_ACCOUNT_1,
    REAL_ACCOUNT_2,
    REAL_SECRET,
    `G${"A".repeat(55)}`,
    `G${"B".repeat(55)}`,
    `S${"A".repeat(55)}`,
    "GFREIGHTERSHOULDNOTBEUSEDINTHISSPECAAAAAAAAAAAAAAAAAAAAA",
    "GAZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTGMZTHCM6",
    ADDRESS_A.toLowerCase(),
    ADDRESS_A.slice(0, 55),
    `${ADDRESS_A}A`,
    `${REAL_ACCOUNT_2.slice(0, 55)}A`,
    "G123",
    "",
  ];

  for (const value of fixtures) {
    assert.equal(
      isValidPublicKey(value),
      StrKey.isValidEd25519PublicKey(value),
      `disagreed with StrKey on ${JSON.stringify(value)}`,
    );
  }
});

test("agrees with StrKey over randomly generated keys", () => {
  // 200 real keys plus 200 single-character corruptions of them: the corruption
  // is what a mistyped or truncated paste actually looks like.
  for (let i = 0; i < 200; i += 1) {
    const key = StrKey.encodeEd25519PublicKey(
      Buffer.from(Array.from({ length: 32 }, () => Math.floor(Math.random() * 256))),
    );
    assert.equal(isValidPublicKey(key), true);

    const at = 1 + Math.floor(Math.random() * 55);
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let replacement = key[at];
    while (replacement === key[at]) {
      replacement = alphabet[Math.floor(Math.random() * alphabet.length)];
    }
    const corrupted = `${key.slice(0, at)}${replacement}${key.slice(at + 1)}`;
    assert.equal(
      isValidPublicKey(corrupted),
      StrKey.isValidEd25519PublicKey(corrupted),
      `disagreed with StrKey on corrupted key ${corrupted}`,
    );
  }
});

// ─── Network allowlists (issue #141) ──────────────────────────────────────

test("auth-server origin allowlist accepts allowlisted origins", () => {
  const { pickAuthServer, isAllowedAuthOrigin } = orbit._internal;

  assert.equal(isAllowedAuthOrigin("http://localhost:4000"), true);
  assert.equal(isAllowedAuthOrigin("http://localhost:4000/auth"), true);
  assert.equal(isAllowedAuthOrigin("http://127.0.0.1:4000"), true);

  assert.deepEqual(pickAuthServer("http://localhost:4000"), {
    server: "http://localhost:4000",
    rejection: null,
  });
});

test("auth-server origin allowlist refuses an attacker origin and falls back", () => {
  const { pickAuthServer, isAllowedAuthOrigin } = orbit._internal;

  const hostile = [
    "https://evil.example.com",
    "http://localhost:4001", // right host, wrong port
    "https://localhost:4000", // right host and port, wrong scheme
    "http://localhost.evil.example.com:4000",
    "//evil.example.com",
    "not a url",
    "",
  ];

  for (const candidate of hostile) {
    assert.equal(isAllowedAuthOrigin(candidate), false, `${candidate} should be refused`);
  }

  const result = pickAuthServer("https://evil.example.com");
  assert.equal(result.server, "http://localhost:4000");
  assert.match(result.rejection, /not on the allowed origin list/);
});

test("no configured auth server resolves to the default without a rejection", () => {
  assert.deepEqual(orbit._internal.pickAuthServer(null), {
    server: "http://localhost:4000",
    rejection: null,
  });
  assert.equal(orbit.authServerRejection(), null);
});

test("network-passphrase allowlist accepts Testnet and Public", () => {
  const { validateChallenge } = orbit._internal;

  for (const passphrase of [
    "Test SDF Network ; September 2015",
    "Public Global Stellar Network ; September 2015",
  ]) {
    const challenge = { transaction: "AAAA", network_passphrase: passphrase };
    assert.deepEqual(validateChallenge(challenge), challenge);
  }
});

test("network-passphrase allowlist rejects unknown, missing, and empty networks", () => {
  const { validateChallenge } = orbit._internal;

  for (const passphrase of [
    "Standalone Network ; February 2017",
    "Test SDF Future Network ; October 2022",
    "",
    undefined,
    null,
  ]) {
    assert.throws(
      () => validateChallenge({ transaction: "AAAA", network_passphrase: passphrase }),
      /unrecognised Stellar network/,
    );
  }
});

test("a challenge with no transaction is rejected before any wallet sees it", () => {
  const { validateChallenge } = orbit._internal;
  const passphrase = "Test SDF Network ; September 2015";

  assert.throws(() => validateChallenge(null), /no transaction/);
  assert.throws(() => validateChallenge({ network_passphrase: passphrase }), /no transaction/);
  assert.throws(
    () => validateChallenge({ transaction: "", network_passphrase: passphrase }),
    /no transaction/,
  );
});

test("startMobileConnect refuses an invalid key without touching the network", async () => {
  const wallet = orbit.walletsFor("mobile")[0];
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  globalThis.fetch = () => {
    fetchCalls += 1;
    return Promise.reject(new Error("should never be called"));
  };

  try {
    // Passes the old charAt(0)/length check, fails the checksum.
    await assert.rejects(
      orbit.startMobileConnect(wallet, `G${"A".repeat(55)}`),
      /valid Stellar public key/,
    );
    await assert.rejects(
      orbit.startMobileConnect(wallet, REAL_SECRET),
      /secret key/i,
    );
  } finally {
    globalThis.fetch = originalFetch;
  }

  assert.equal(fetchCalls, 0);
});

test("connect persists only public session data and emits wallet:connected", async () => {
  const provider = {
    requestAccess: async () => ({ address: ADDRESS_A }),
  };
  const { events, session, storage, ui } = createHarness({ provider });

  assert.equal(await session.connect(), ADDRESS_A);
  assert.deepEqual(JSON.parse(storage.getItem(WALLET_SESSION_KEY)), {
    publicKey: ADDRESS_A,
    source: "freighter",
  });
  assert.deepEqual(ui.state, {
    view: "connected",
    publicKey: ADDRESS_A,
    statusText: "",
  });
  assert.deepEqual(events, [
    {
      name: "wallet:connected",
      detail: { publicKey: ADDRESS_A, source: "freighter" },
    },
  ]);
});

test("connect remains compatible with the existing Freighter provider shape", async () => {
  let connectCalls = 0;
  const provider = {
    isConnected: async () => false,
    connect: async () => {
      connectCalls += 1;
    },
    getAddress: async () => ({ address: ADDRESS_A }),
  };
  const { session } = createHarness({ provider });

  assert.equal(await session.connect(), ADDRESS_A);
  assert.equal(connectCalls, 1);
});

test("connect uses the manual fallback only when Freighter is unavailable", async () => {
  let promptCalls = 0;
  const { events, session, storage } = createHarness({
    promptForAddress() {
      promptCalls += 1;
      return ADDRESS_A;
    },
  });

  assert.equal(await session.connect(), ADDRESS_A);
  assert.equal(promptCalls, 1);
  assert.deepEqual(readSession(storage), {
    publicKey: ADDRESS_A,
    source: "manual",
  });
  assert.equal(events[0].detail.source, "manual");
});

test("initialize re-fetches and replaces a Freighter public key", async () => {
  const storage = createStorage({
    [WALLET_SESSION_KEY]: JSON.stringify({
      publicKey: ADDRESS_A,
      source: "freighter",
    }),
  });
  const provider = {
    isConnected: async () => ({ isConnected: true }),
    getAddress: async () => ({ address: ADDRESS_B }),
  };
  const { events, session, ui } = createHarness({ storage, provider });

  assert.equal(await session.initialize(), ADDRESS_B);
  assert.deepEqual(ui.state, {
    view: "connected",
    publicKey: ADDRESS_B,
    statusText: "",
  });
  assert.deepEqual(readSession(storage), {
    publicKey: ADDRESS_B,
    source: "freighter",
  });
  assert.deepEqual(events, [
    {
      name: "wallet:reconnected",
      detail: { publicKey: ADDRESS_B, source: "freighter" },
    },
  ]);
});

test("initialize offers reconnect for a saved manual session", async () => {
  const storage = createStorage({
    [WALLET_SESSION_KEY]: JSON.stringify({
      publicKey: ADDRESS_A,
      source: "manual",
    }),
  });
  let promptCalls = 0;
  const { events, session, ui } = createHarness({
    storage,
    promptForAddress() {
      promptCalls += 1;
      return ADDRESS_A;
    },
  });

  assert.equal(await session.initialize(), null);
  assert.equal(promptCalls, 0);
  assert.deepEqual(ui.state, {
    view: "disconnected",
    reconnect: true,
    statusText: "Previous wallet found. Reconnect to continue.",
  });
  assert.deepEqual(events, []);
});

test("failed Freighter restoration keeps reconnect intent without trusting cache", async () => {
  const storage = createStorage({
    [WALLET_SESSION_KEY]: JSON.stringify({
      publicKey: ADDRESS_A,
      source: "freighter",
    }),
  });
  const provider = {
    isConnected: async () => ({ isConnected: true }),
    getAddress: async () => ({ address: "" }),
  };
  const { events, session, ui } = createHarness({ storage, provider });

  assert.equal(await session.initialize(), null);
  assert.deepEqual(ui.state, {
    view: "disconnected",
    reconnect: true,
    statusText: "Unable to restore wallet. Reconnect to continue.",
  });
  assert.deepEqual(readSession(storage), {
    publicKey: ADDRESS_A,
    source: "freighter",
  });
  assert.deepEqual(events, []);
});

test("disconnect clears storage, resets UI, and emits wallet:disconnected", async () => {
  const provider = {
    requestAccess: async () => ({ address: ADDRESS_A }),
  };
  const { events, session, storage, ui } = createHarness({ provider });
  await session.connect();
  events.length = 0;

  session.disconnect();

  assert.equal(storage.getItem(WALLET_SESSION_KEY), null);
  assert.deepEqual(ui.state, {
    view: "disconnected",
    reconnect: false,
    statusText: "Disconnected.",
  });
  assert.deepEqual(events, [
    { name: "wallet:disconnected", detail: {} },
  ]);
});

test("invalid stored data is discarded", async () => {
  const storage = createStorage({
    [WALLET_SESSION_KEY]: "not-json",
  });
  const { session, ui } = createHarness({ storage });

  assert.equal(await session.initialize(), null);
  assert.equal(storage.getItem(WALLET_SESSION_KEY), null);
  assert.deepEqual(ui.state, {
    view: "disconnected",
    reconnect: false,
    statusText: "",
  });
});

test("a session saved under the old regex is discarded on restore", async () => {
  // Tightening validation invalidates addresses the previous validator
  // accepted. The correct degradation is "user reconnects", not "page trusts a
  // key it can no longer verify" — the stale entry must also be evicted.
  const storage = createStorage({
    [WALLET_SESSION_KEY]: JSON.stringify({
      publicKey: `G${"A".repeat(55)}`,
      source: "freighter",
    }),
  });
  const { session, ui } = createHarness({ storage });

  assert.equal(await session.initialize(), null);
  assert.equal(storage.getItem(WALLET_SESSION_KEY), null);
  assert.deepEqual(ui.state, {
    view: "disconnected",
    reconnect: false,
    statusText: "",
  });
});

test("a storage write failure does not discard the live connection", async () => {
  const storage = createStorage();
  storage.setItem = () => {
    throw new Error("storage blocked");
  };
  const provider = {
    requestAccess: async () => ({ address: ADDRESS_A }),
  };
  const { events, session, ui } = createHarness({ storage, provider });

  assert.equal(await session.connect(), ADDRESS_A);
  assert.deepEqual(ui.state, {
    view: "connected",
    publicKey: ADDRESS_A,
    statusText: "Wallet connected, but this session could not be saved.",
  });
  assert.equal(events[0].name, "wallet:connected");
});

test("DOM UI renders reconnect, connected, and disconnected states", () => {
  const { document, elements } = createFakeDocument();
  const ui = createDomUi(document);

  ui.showDisconnected({
    reconnect: true,
    statusText: "Reconnect required.",
  });
  assert.equal(elements.button.textContent, "Reconnect Wallet");
  assert.equal(elements.button.disabled, false);
  assert.equal(elements.walletInfo.style.display, "none");
  assert.equal(elements.walletAddress.textContent, "");
  assert.equal(elements.status.textContent, "Reconnect required.");

  ui.showReconnecting();
  assert.equal(elements.button.textContent, "Reconnecting…");
  assert.equal(elements.button.disabled, true);

  ui.showConnected(ADDRESS_A, "");
  assert.equal(elements.button.textContent, "Disconnect");
  assert.equal(elements.button.disabled, false);
  assert.equal(elements.walletInfo.style.display, "block");
  assert.equal(elements.walletAddress.textContent, ADDRESS_A);

  ui.showDisconnected({ reconnect: false, statusText: "Disconnected." });
  assert.equal(elements.button.textContent, "Connect Wallet");
  assert.equal(elements.walletInfo.style.display, "none");
  assert.equal(elements.walletAddress.textContent, "");
});

test("page bootstrap wires global actions and browser lifecycle events", async () => {
  const { document, elements } = createFakeDocument();
  const dispatchedEvents = [];
  class FakeCustomEvent {
    constructor(type, options) {
      this.type = type;
      this.detail = options.detail;
    }
  }
  const fakeWindow = {
    CustomEvent: FakeCustomEvent,
    document,
    freighter: null,
    localStorage: createStorage(),
    prompt: () => ADDRESS_A,
    dispatchEvent(event) {
      dispatchedEvents.push(event);
    },
  };

  const controller = await initializeWalletPage(fakeWindow);

  assert.ok(controller);
  assert.equal(typeof fakeWindow.connectWallet, "function");
  assert.equal(typeof fakeWindow.disconnectWallet, "function");
  assert.equal(typeof elements.button.listener, "function");

  assert.equal(await fakeWindow.connectWallet(), ADDRESS_A);
  assert.equal(dispatchedEvents[0].type, "wallet:connected");
  assert.deepEqual(dispatchedEvents[0].detail, {
    publicKey: ADDRESS_A,
    source: "manual",
  });

  fakeWindow.disconnectWallet();
  assert.equal(dispatchedEvents[1].type, "wallet:disconnected");
});

test("HTML loads the external wallet script without inline click handlers", () => {
  const html = readFileSync(join(__dirname, "wallet_connect.html"), "utf8");
  const freighterScript =
    "https://cdnjs.cloudflare.com/ajax/libs/stellar-freighter-api/6.0.1/index.min.js";
  const freighterScriptPosition = html.indexOf(freighterScript);
  const walletScriptPosition = html.indexOf('src="wallet_connect.js"');

  assert.notEqual(freighterScriptPosition, -1);
  assert.ok(freighterScriptPosition < walletScriptPosition);
  assert.match(
    html,
    /<script src="wallet_connect\.js" defer><\/script>/,
  );
  assert.doesNotMatch(html, /onclick=/);
  assert.match(html, /<p id="status"[^>]*aria-live="polite"[^>]*><\/p>/);
});

// ─── Static page hardening (issue #141) ───────────────────────────────────

// Pinned here as well as in the markup: if cdnjs ever serves different bytes
// for this URL, `curl … | openssl dgst -sha256 -binary | openssl base64 -A`
// stops matching and this constant is the single place to update — after
// reviewing what changed. See docs/tutorials/dapp-integration.md.
const FREIGHTER_SRI_HASH = "sha256-15dCnPl/mmfxmZkQgwqJ3AHVZa7euvtlGhGx7QA6fp0=";

function readHtml() {
  return readFileSync(join(__dirname, "wallet_connect.html"), "utf8");
}

function cspDirectives(html) {
  const match = html.match(
    /<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]+)"/,
  );
  assert.ok(match, "wallet_connect.html must carry a CSP meta tag");

  const directives = new Map();
  for (const part of match[1].split(";")) {
    const [name, ...values] = part.trim().split(/\s+/);
    if (name) directives.set(name, values);
  }
  return directives;
}

test("HTML declares a deny-by-default Content Security Policy", () => {
  const directives = cspDirectives(readHtml());

  assert.deepEqual(directives.get("default-src"), ["'none'"]);
  assert.deepEqual(directives.get("script-src"), [
    "'self'",
    "https://cdnjs.cloudflare.com",
  ]);
  assert.deepEqual(directives.get("style-src"), ["'self'"]);
  assert.deepEqual(directives.get("img-src"), ["'self'", "data:"]);
  assert.deepEqual(directives.get("connect-src"), [
    "'self'",
    "http://localhost:4000",
  ]);
  assert.deepEqual(directives.get("form-action"), ["'none'"]);
  assert.deepEqual(directives.get("base-uri"), ["'none'"]);
});

test("CSP never weakens script-src or style-src with unsafe keywords", () => {
  const directives = cspDirectives(readHtml());

  for (const name of ["script-src", "style-src", "default-src"]) {
    const values = directives.get(name) || [];
    for (const unsafe of ["'unsafe-inline'", "'unsafe-eval'", "*", "data:"]) {
      assert.ok(
        !values.includes(unsafe),
        `${name} must not include ${unsafe}`,
      );
    }
  }
});

test("CSP connect-src covers every origin the SEP-10 client can reach", () => {
  const connectSrc = cspDirectives(readHtml()).get("connect-src");

  // Anything on the JS origin allowlist must also be reachable under CSP,
  // otherwise a correctly-configured auth server is blocked by the browser.
  // The .invalid placeholder is excluded by design — it is documentation, not
  // a deployable origin, and listing it in the policy would imply otherwise.
  for (const origin of orbit._internal.AUTH_ORIGIN_ALLOWLIST) {
    if (origin.endsWith(".invalid")) continue;
    assert.ok(
      connectSrc.includes(origin) ||
        // 127.0.0.1 and localhost are the same dev server; the page is served
        // from one of them, so 'self' covers same-origin requests.
        origin === "http://127.0.0.1:4000",
      `connect-src should allow ${origin}`,
    );
  }
});

test("the Freighter CDN script is pinned with SRI and crossorigin", () => {
  const html = readHtml();
  const tag = html.match(/<script[^>]*cdnjs\.cloudflare\.com[^>]*>/s);
  assert.ok(tag, "expected a cdnjs script tag");

  assert.match(html, new RegExp(`integrity="${FREIGHTER_SRI_HASH.replace(/[/+]/g, "\\$&")}"`));
  assert.match(html, /crossorigin="anonymous"/);

  // integrity without crossorigin silently fails the CORS check and blocks the
  // script; assert both live on the same tag, not merely somewhere in the file.
  const scriptBlock = html.slice(
    html.indexOf("<script", html.indexOf("cdnjs.cloudflare.com") - 400),
    html.indexOf("</script>", html.indexOf("cdnjs.cloudflare.com")),
  );
  assert.ok(scriptBlock.includes("integrity="));
  assert.ok(scriptBlock.includes('crossorigin="anonymous"'));
  assert.ok(scriptBlock.includes("defer"));
});

test("all CSS lives in an external stylesheet so style-src can stay 'self'", () => {
  const html = readHtml();

  assert.match(html, /<link rel="stylesheet" href="wallet_connect\.css" \/>/);
  assert.doesNotMatch(html, /<style[\s>]/);
  assert.doesNotMatch(html, /\sstyle="/);

  // The extraction must be a move, not a rewrite: the a11y-tuned tokens and
  // media queries have to still be there.
  const css = readFileSync(join(__dirname, "wallet_connect.css"), "utf8");
  for (const needle of [
    "--focus-ring",
    ":focus-visible",
    "prefers-color-scheme: dark",
    "prefers-reduced-motion: no-preference",
    "prefers-contrast: more",
    "#wallet-picker-overlay.open",
    "max-width: 90vw",
    "min-width: 320px",
    "box-sizing: border-box",
  ]) {
    assert.ok(css.includes(needle), `wallet_connect.css lost ${needle}`);
  }
});
