"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");

const {
  WALLET_SESSION_KEY,
  createDomUi,
  createWalletSession,
  initializeWalletPage,
  isValidPublicKey,
  readSession,
} = require("./wallet_connect.js");

const ADDRESS_A = `G${"A".repeat(55)}`;
const ADDRESS_B = `G${"B".repeat(55)}`;

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

test("validates the public Stellar address shape", () => {
  assert.equal(isValidPublicKey(ADDRESS_A), true);
  assert.equal(isValidPublicKey(`S${"A".repeat(55)}`), false);
  assert.equal(isValidPublicKey("G123"), false);
  assert.equal(isValidPublicKey(null), false);
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
