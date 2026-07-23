"use strict";

/**
 * OrbitChain Wallet Connect.
 *
 * Two cooperating modules live in this file:
 *
 * 1. `OrbitWalletConnect` — mobile wallet deep-link + SEP-10 auth client.
 *    Mobile path: navigator.userAgent detection -> wallet picker (Lobstr,
 *    Bitnovo, or any other SEP-0007-compatible wallet) -> SEP-10 challenge
 *    fetch -> SEP-0007 `web+stellar:tx` deep link with a callback URL ->
 *    wallet signs and redirects back -> signed challenge is POSTed to the
 *    auth server for a session token.
 *
 * 2. The wallet session controller (`OrbitChainWalletSession`) — owns the
 *    page UI, persists the connected session in localStorage, restores it on
 *    load, and routes a connect action to Freighter (desktop), manual entry
 *    (no extension), or the mobile deep-link flow above.
 *
 * Spec references:
 *   SEP-0007  https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0007.md
 *   SEP-0010  https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md
 *
 * See docs/tutorials/dapp-integration.md for the full write-up, including why
 * mobile wallets need a two-step (identify, then sign) bootstrap instead of
 * the single `window.freighter` call the desktop path uses.
 */

// ─── Mobile deep-link + SEP-10 module ───────────────────────────────────────
(function (global) {
  'use strict';

  // ── Config ────────────────────────────────────────────────────────────────
  // Overridable via <script data-auth-server="https://auth.example.com">
  // or window.ORBITCHAIN_AUTH_SERVER before this file loads.
  function resolveAuthServer() {
    if (global.ORBITCHAIN_AUTH_SERVER) return global.ORBITCHAIN_AUTH_SERVER;
    var thisScript = typeof document !== 'undefined' ? document.currentScript : null;
    if (thisScript && thisScript.dataset && thisScript.dataset.authServer) {
      return thisScript.dataset.authServer;
    }
    return 'http://localhost:4000';
  }

  var AUTH_SERVER = resolveAuthServer();
  var HOME_DOMAIN = global.location ? global.location.hostname || 'localhost' : 'localhost';
  var RETURN_PARAM = 'orbitchain_xdr';

  // ── Mobile detection ─────────────────────────────────────────────────────
  // Deliberately conservative: only iOS/Android/mobile-webview UAs route to
  // the deep-link flow. Desktop always gets the existing Freighter path.
  var MOBILE_UA_RE = /Android|iPhone|iPad|iPod|Mobile|IEMobile|BlackBerry|webOS/i;

  function isMobile(userAgent) {
    var ua = typeof userAgent === 'string' ? userAgent : (global.navigator && global.navigator.userAgent) || '';
    return MOBILE_UA_RE.test(ua);
  }

  // ── Wallet registry ──────────────────────────────────────────────────────
  // `buildDeepLink(xdr, callbackUrl)` returns the URI to redirect the browser
  // to. Both Lobstr and Bitnovo advertise SEP-0007 (`web+stellar:`) support
  // for transaction signing, so that is the primary scheme. `fallbackScheme`
  // is the vendor's own custom URI, used only if `web+stellar:` fails to
  // resolve an installed app (best-effort; confirm against each wallet's
  // current docs before relying on it in production).
  var WALLETS = [
    {
      id: 'lobstr',
      name: 'Lobstr',
      platforms: ['mobile'],
      icon: '🦞',
      buildDeepLink: function (xdr, callbackUrl) {
        return sep7TxUri(xdr, callbackUrl);
      },
      fallbackScheme: function (xdr, callbackUrl) {
        return 'https://lobstr.co/sep0007?' + sep7Query(xdr, callbackUrl);
      },
    },
    {
      id: 'bitnovo',
      name: 'Bitnovo Wallet',
      platforms: ['mobile'],
      icon: '🟣',
      buildDeepLink: function (xdr, callbackUrl) {
        return sep7TxUri(xdr, callbackUrl);
      },
      fallbackScheme: function (xdr, callbackUrl) {
        return 'bitnovowallet://sep0007?' + sep7Query(xdr, callbackUrl);
      },
    },
    {
      id: 'other-sep7',
      name: 'Other SEP-7 wallet',
      platforms: ['mobile'],
      icon: '⭐',
      buildDeepLink: function (xdr, callbackUrl) {
        return sep7TxUri(xdr, callbackUrl);
      },
    },
    // Desktop browser-extension adapters (issue #142). Uniform interface:
    // `detect()` — is the extension injected into this page right now?
    // `connect()` — resolve the user's public key (G…) via the extension's
    // own approval flow. Each adapter uses the API documented by its vendor;
    // detection is by injected global, same pattern the Freighter path
    // always used.
    {
      id: 'freighter',
      name: 'Freighter',
      platforms: ['desktop'],
      icon: '🚀',
      detect: function () {
        return typeof global.freighter !== 'undefined';
      },
      connect: function () {
        return Promise.resolve()
          .then(function () { return global.freighter.isConnected(); })
          .then(function (connected) {
            if (!connected) return global.freighter.connect();
          })
          .then(function () { return global.freighter.getAddress(); })
          .then(function (res) { return res.address; });
      },
    },
    {
      id: 'albedo',
      name: 'Albedo',
      platforms: ['desktop'],
      icon: '🔆',
      detect: function () {
        return typeof global.albedo !== 'undefined' && typeof global.albedo.publicKey === 'function';
      },
      connect: function () {
        return global.albedo.publicKey({}).then(function (res) { return res.pubkey; });
      },
    },
    {
      id: 'rabet',
      name: 'Rabet',
      platforms: ['desktop'],
      icon: '🦊',
      detect: function () {
        return typeof global.rabet !== 'undefined' && typeof global.rabet.connect === 'function';
      },
      connect: function () {
        return global.rabet.connect().then(function (res) { return res.publicKey; });
      },
    },
    {
      id: 'xbull',
      name: 'xBull',
      platforms: ['desktop'],
      icon: '🐂',
      detect: function () {
        return typeof global.xBullSDK !== 'undefined';
      },
      connect: function () {
        return global.xBullSDK
          .connect({ canRequestPublicKey: true, canRequestSign: true })
          .then(function () { return global.xBullSDK.getPublicKey(); });
      },
    },
    {
      id: 'lobstr-extension',
      name: 'LOBSTR Extension',
      platforms: ['desktop'],
      icon: '🦞',
      detect: function () {
        return typeof global.lobstrApi !== 'undefined' && typeof global.lobstrApi.getPublicKey === 'function';
      },
      connect: function () {
        return Promise.resolve()
          .then(function () {
            if (typeof global.lobstrApi.connect === 'function') return global.lobstrApi.connect();
          })
          .then(function () { return global.lobstrApi.getPublicKey(); });
      },
    },
  ];

  function sep7Query(xdr, callbackUrl) {
    return 'xdr=' + encodeURIComponent(xdr) + '&callback=' + encodeURIComponent('url:' + callbackUrl);
  }

  function sep7TxUri(xdr, callbackUrl) {
    return 'web+stellar:tx?' + sep7Query(xdr, callbackUrl);
  }

  function walletsFor(platform) {
    return WALLETS.filter(function (w) {
      return w.platforms.indexOf(platform) !== -1;
    });
  }

  /**
   * Desktop wallets whose browser extension is actually injected into this
   * page right now (issue #142). The page uses this to decide between
   * connecting directly (one wallet), showing a chooser (several), or
   * falling back to manual entry (none).
   */
  function detectedDesktopWallets() {
    return walletsFor('desktop').filter(function (w) {
      try {
        return typeof w.detect === 'function' && w.detect();
      } catch (e) {
        return false;
      }
    });
  }

  // ── SEP-10 client ────────────────────────────────────────────────────────
  var Sep10 = {
    /**
     * GET {AUTH_SERVER}/auth?account=...&home_domain=...
     * -> { transaction, network_passphrase }
     */
    fetchChallenge: function (account) {
      var url = AUTH_SERVER + '/auth?account=' + encodeURIComponent(account) +
        '&home_domain=' + encodeURIComponent(HOME_DOMAIN);
      return fetch(url).then(function (res) {
        if (!res.ok) throw new Error('SEP-10 challenge request failed: HTTP ' + res.status);
        return res.json();
      });
    },

    /**
     * POST {AUTH_SERVER}/auth with the wallet-signed challenge transaction.
     * -> { token }  (JWT session token)
     */
    submitSignedChallenge: function (signedXdr) {
      return fetch(AUTH_SERVER + '/auth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ transaction: signedXdr }),
      }).then(function (res) {
        if (!res.ok) throw new Error('SEP-10 verification failed: HTTP ' + res.status);
        return res.json();
      });
    },
  };

  // ── Mobile deep-link flow ────────────────────────────────────────────────
  /**
   * Step 1 (identify): the browser has no extension to call on mobile, so the
   * user supplies the public key they intend to authenticate — the same
   * manual-entry pattern the existing desktop fallback already uses, applied
   * consistently here rather than inventing a new UX.
   * Step 2 (challenge): fetch the SEP-10 challenge for that account.
   * Step 3 (sign): redirect to the wallet's SEP-0007 `web+stellar:tx` deep
   * link with a callback URL pointing back at this page.
   * Step 4 (return): on load, if the callback query param is present, POST
   * the signed challenge and complete the handshake.
   */
  function startMobileConnect(wallet, account) {
    if (!account || account.charAt(0) !== 'G' || account.length !== 56) {
      return Promise.reject(new Error('Enter a valid Stellar public key (starts with G, 56 chars).'));
    }

    return Sep10.fetchChallenge(account).then(function (challenge) {
      var returnUrl = stripReturnParam(global.location.href);
      var separator = returnUrl.indexOf('?') === -1 ? '?' : '&';
      var callbackUrl = returnUrl + separator + RETURN_PARAM + '=1';

      var deepLink = wallet.buildDeepLink(challenge.transaction, callbackUrl);
      global.location.href = deepLink;
      // Execution effectively pauses here — the mobile OS switches apps.
      // The flow resumes in handleMobileReturn() on next page load.
      return { redirected: true, wallet: wallet.id };
    });
  }

  /**
   * Called on page load. SEP-0007 wallets return control by redirecting back
   * to the callback URL with the signed transaction appended as `xdr=`
   * (per SEP-0007 §Response, "url:" callback variant).
   */
  function handleMobileReturn() {
    if (!global.location) return null;
    var params = new URLSearchParams(global.location.search);
    if (!params.has(RETURN_PARAM)) return null;

    var signedXdr = params.get('xdr');
    if (!signedXdr) {
      return Promise.reject(new Error('Wallet returned without a signed transaction.'));
    }

    return Sep10.submitSignedChallenge(signedXdr).then(function (result) {
      var cleanUrl = stripReturnParam(global.location.href);
      global.history.replaceState({}, document.title, cleanUrl);
      return result; // { token }
    });
  }

  function stripReturnParam(href) {
    var url = new URL(href);
    url.searchParams.delete(RETURN_PARAM);
    url.searchParams.delete('xdr');
    return url.origin + url.pathname + (url.search ? url.search : '') + url.hash;
  }

  // ── Public API ────────────────────────────────────────────────────────────
  global.OrbitWalletConnect = {
    isMobile: isMobile,
    walletsFor: walletsFor,
    detectedDesktopWallets: detectedDesktopWallets,
    WALLETS: WALLETS,
    Sep10: Sep10,
    startMobileConnect: startMobileConnect,
    handleMobileReturn: handleMobileReturn,
    _internal: { sep7TxUri: sep7TxUri, stripReturnParam: stripReturnParam, AUTH_SERVER: AUTH_SERVER },
  };
})(typeof window !== 'undefined' ? window : globalThis);

// ─── Wallet session management ──────────────────────────────────────────────

const WALLET_SESSION_KEY = "orbitchain.walletSession";
const VALID_SOURCES = new Set(["freighter", "manual", "mobile"]);

function isValidPublicKey(value) {
  return typeof value === "string" && /^G[A-Z2-7]{55}$/.test(value);
}

function discardStoredSession(storage) {
  try {
    storage?.removeItem(WALLET_SESSION_KEY);
  } catch (_error) {
    // Storage can be unavailable in private or restricted browser contexts.
  }
}

function readSession(storage) {
  if (!storage) return null;

  let rawSession;
  try {
    rawSession = storage.getItem(WALLET_SESSION_KEY);
  } catch (_error) {
    return null;
  }

  if (rawSession === null) return null;

  try {
    const session = JSON.parse(rawSession);
    if (
      session &&
      isValidPublicKey(session.publicKey) &&
      VALID_SOURCES.has(session.source)
    ) {
      return {
        publicKey: session.publicKey,
        source: session.source,
      };
    }
  } catch (_error) {
    // Invalid JSON is treated like any other invalid persisted session.
  }

  discardStoredSession(storage);
  return null;
}

function writeSession(storage, publicKey, source) {
  if (!storage) return false;

  try {
    storage.setItem(
      WALLET_SESSION_KEY,
      JSON.stringify({ publicKey, source }),
    );
    return true;
  } catch (_error) {
    return false;
  }
}

function clearSession(storage) {
  if (!storage) return false;

  try {
    storage.removeItem(WALLET_SESSION_KEY);
    return true;
  } catch (_error) {
    return false;
  }
}

function errorMessage(error, fallback) {
  if (typeof error === "string" && error) return error;
  if (error && typeof error.message === "string" && error.message) {
    return error.message;
  }
  return fallback;
}

function throwResponseError(response) {
  if (response && typeof response === "object" && response.error) {
    throw new Error(errorMessage(response.error, "Freighter request failed."));
  }
}

function extractAddress(response) {
  throwResponseError(response);

  const address =
    typeof response === "string" ? response : response?.address;
  if (!isValidPublicKey(address)) {
    throw new Error("Freighter did not return a valid public key.");
  }

  return address;
}

function connectedFromResponse(response) {
  throwResponseError(response);
  if (typeof response === "boolean") return response;
  return response?.isConnected === true;
}

function decodeJwtSubject(token) {
  try {
    const payload = JSON.parse(
      atob(token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/")),
    );
    return typeof payload.sub === "string" ? payload.sub : null;
  } catch (_error) {
    return null;
  }
}

async function requestFreighterAddress(provider) {
  if (typeof provider.requestAccess === "function") {
    return extractAddress(await provider.requestAccess());
  }

  if (typeof provider.isConnected === "function") {
    const connected = connectedFromResponse(await provider.isConnected());
    if (!connected) {
      if (typeof provider.connect !== "function") {
        throw new Error("Freighter access is not available.");
      }
      await provider.connect();
    }
  }

  if (typeof provider.getAddress !== "function") {
    throw new Error("Freighter does not expose a public-key API.");
  }

  return extractAddress(await provider.getAddress());
}

async function restoreFreighterAddress(provider) {
  if (!provider || typeof provider.getAddress !== "function") {
    throw new Error("Freighter is not available.");
  }

  if (typeof provider.isConnected === "function") {
    const connected = connectedFromResponse(await provider.isConnected());
    if (!connected) throw new Error("Freighter is not connected.");
  }

  return extractAddress(await provider.getAddress());
}

function createWalletSession({
  storage,
  provider = null,
  promptForAddress = () => null,
  ui,
  emit = () => {},
  mobile = null,
  desktop = null,
}) {
  let connected = false;

  async function initialize() {
    // A pending SEP-0007 callback takes priority over any saved session —
    // the user is mid-handshake with a mobile wallet.
    if (mobile) {
      const pendingReturn = mobile.handleReturn();
      if (pendingReturn) return finishMobileReturn(pendingReturn);
    }

    const savedSession = readSession(storage);
    if (!savedSession) {
      ui.showDisconnected({ reconnect: false, statusText: "" });
      return null;
    }

    ui.showReconnecting();

    if (savedSession.source !== "freighter") {
      // Manual and mobile sessions cannot be restored without the user
      // re-authenticating, so only offer a reconnect.
      ui.showDisconnected({
        reconnect: true,
        statusText: "Previous wallet found. Reconnect to continue.",
      });
      return null;
    }

    try {
      const publicKey = await restoreFreighterAddress(provider);
      const persisted = writeSession(storage, publicKey, "freighter");
      connected = true;
      ui.showConnected(
        publicKey,
        persisted
          ? ""
          : "Wallet reconnected, but this session could not be saved.",
      );
      emit("wallet:reconnected", { publicKey, source: "freighter" });
      return publicKey;
    } catch (_error) {
      connected = false;
      ui.showDisconnected({
        reconnect: true,
        statusText: "Unable to restore wallet. Reconnect to continue.",
      });
      return null;
    }
  }

  async function finishMobileReturn(pendingReturn) {
    ui.showReconnecting();
    ui.setStatus("Verifying signed challenge…");

    try {
      const { token } = await pendingReturn;
      const publicKey = decodeJwtSubject(token);
      if (!isValidPublicKey(publicKey)) {
        throw new Error("Auth server session token has no valid account.");
      }

      const persisted = writeSession(storage, publicKey, "mobile");
      connected = true;
      ui.showConnected(
        publicKey,
        persisted
          ? ""
          : "Wallet connected, but this session could not be saved.",
      );
      emit("wallet:connected", { publicKey, source: "mobile" });
      return publicKey;
    } catch (error) {
      connected = false;
      ui.showDisconnected({
        reconnect: readSession(storage) !== null,
        statusText: `❌ ${errorMessage(error, "Unable to verify the signed challenge.")}`,
      });
      return null;
    }
  }

  function connectMobile() {
    ui.showConnecting();
    ui.openWalletPicker(mobile.wallets(), {
      async onConfirm(wallet, account) {
        try {
          ui.setStatus("Requesting SEP-10 challenge…");
          await mobile.start(wallet, account);
          // mobile.start() redirects the browser to the wallet app; if we
          // reach this line the redirect didn't happen (e.g. blocked), so
          // the picker stays open for the user to retry.
        } catch (error) {
          ui.setStatus(`❌ ${errorMessage(error, "Unable to open the wallet app.")}`);
        }
      },
      onCancel() {
        ui.showDisconnected({
          reconnect: readSession(storage) !== null,
          statusText: "",
        });
      },
    });
    return null;
  }

  // Persist + surface a successful connection. Shared by every connect path
  // (Freighter provider, desktop extension registry, manual entry, mobile).
  function completeConnection(publicKey, source) {
    const persisted = writeSession(storage, publicKey, source);
    connected = true;
    ui.showConnected(
      publicKey,
      persisted ? "" : "Wallet connected, but this session could not be saved.",
    );
    emit("wallet:connected", { publicKey, source });
    return publicKey;
  }

  function failConnection(error) {
    connected = false;
    ui.showDisconnected({
      reconnect: readSession(storage) !== null,
      statusText: `❌ ${errorMessage(error, "Unable to connect wallet.")}`,
    });
    return null;
  }

  async function connect() {
    if (mobile && mobile.isMobile() && ui.supportsWalletPicker) {
      return connectMobile();
    }

    // Desktop wallet registry (issue #142): when the page exposes the
    // extension registry, route through it — direct connect for one detected
    // wallet, an inline chooser for several, the accessible manual form for
    // none. Without the registry (e.g. unit harness), keep the historical
    // Freighter-provider / prompt fallback below.
    if (desktop) {
      return connectDesktop();
    }

    return connectViaProvider();
  }

  async function connectViaProvider() {
    ui.showConnecting();

    try {
      const source = provider ? "freighter" : "manual";
      let publicKey;

      if (provider) {
        publicKey = await requestFreighterAddress(provider);
      } else {
        const promptedAddress = await promptForAddress();
        if (!isValidPublicKey(promptedAddress)) {
          throw new Error("Invalid or cancelled address input.");
        }
        publicKey = promptedAddress;
      }

      return completeConnection(publicKey, source);
    } catch (error) {
      return failConnection(error);
    }
  }

  function connectDesktop() {
    ui.showConnecting();

    let detected = [];
    try {
      detected = desktop.detected() || [];
    } catch (_error) {
      detected = [];
    }

    // Exactly one extension keeps the historical one-click UX.
    if (detected.length === 1) {
      void finishDesktopWallet(detected[0]);
      return null;
    }

    // Several extensions: let the user pick. Fall back to the first if the
    // page lacks the chooser markup.
    if (detected.length > 1) {
      if (ui.supportsDesktopChooser) {
        ui.openDesktopChooser(detected, {
          onSelect(wallet) {
            void finishDesktopWallet(wallet);
          },
          onCancel() {
            ui.showDisconnected({
              reconnect: readSession(storage) !== null,
              statusText: "",
            });
          },
        });
        return null;
      }
      void finishDesktopWallet(detected[0]);
      return null;
    }

    // No extension detected: accessible manual entry (issue #148), or the
    // window.prompt() fallback when the form markup is absent.
    if (ui.supportsManualForm) {
      ui.openManualForm({
        submit(address) {
          if (!isValidPublicKey(address)) {
            return "Enter a valid Stellar public key (starts with G, 56 chars).";
          }
          void completeConnection(address, "manual");
          return null;
        },
        onCancel() {
          ui.showDisconnected({
            reconnect: readSession(storage) !== null,
            statusText: "",
          });
        },
      });
      return null;
    }

    return connectViaProvider();
  }

  async function finishDesktopWallet(wallet) {
    try {
      ui.setStatus(`Connecting with ${wallet.name}…`);
      const publicKey = await wallet.connect();
      if (!isValidPublicKey(publicKey)) {
        throw new Error(`${wallet.name} did not return a valid public key.`);
      }
      // Only Freighter has a silent-restore path (restoreFreighterAddress);
      // every other extension is persisted as a generic session that offers
      // an explicit reconnect on the next load.
      const source = wallet.id === "freighter" ? "freighter" : "manual";
      return completeConnection(publicKey, source);
    } catch (error) {
      return failConnection(error);
    }
  }

  function disconnect() {
    connected = false;
    const cleared = clearSession(storage);
    ui.showDisconnected({
      reconnect: false,
      statusText: cleared
        ? "Disconnected."
        : "Disconnected, but the saved session could not be cleared.",
    });
    emit("wallet:disconnected", {});
  }

  async function handleAction() {
    if (connected) {
      disconnect();
      return undefined;
    }
    return connect();
  }

  return {
    initialize,
    connect,
    disconnect,
    handleAction,
  };
}

function createDomUi(document) {
  const button = document.getElementById("connect-btn");
  const walletInfo = document.getElementById("wallet-info");
  const walletAddress = document.getElementById("wallet-address");
  const status = document.getElementById("status");

  if (!button || !walletInfo || !walletAddress || !status) {
    throw new Error("Wallet connect markup is incomplete.");
  }

  // The mobile wallet picker markup is optional — pages without it simply
  // never offer the mobile deep-link flow.
  const pickerOverlay = document.getElementById("wallet-picker-overlay");
  const pickerOptions = document.getElementById("wallet-options");
  const pickerInput = document.getElementById("pubkey-input");
  // Optional accessible wrapper (issue #148): a labelled container around the
  // public-key input. When present it — not the bare input — carries the
  // hidden/shown state, so toggle whichever the markup provides.
  const pickerField = document.getElementById("pubkey-field");
  const pickerCancel = document.getElementById("wallet-picker-cancel");
  const supportsWalletPicker = Boolean(
    pickerOverlay && pickerOptions && pickerInput,
  );

  let onPickerCancel = null;

  function hideWalletInfo() {
    walletInfo.style.display = "none";
    walletAddress.textContent = "";
  }

  function closeWalletPicker() {
    if (!supportsWalletPicker) return;
    pickerOverlay.classList.remove("open");
    if (pickerField) pickerField.style.display = "none";
    pickerInput.style.display = "none";
    pickerInput.value = "";
    pickerInput.onkeydown = null;
    onPickerCancel = null;
  }

  if (pickerCancel) {
    pickerCancel.addEventListener("click", () => {
      const cancelHandler = onPickerCancel;
      closeWalletPicker();
      if (cancelHandler) cancelHandler();
    });
  }

  function showPickerConfirm(wallet, onConfirm) {
    if (pickerField) pickerField.style.display = "block";
    pickerInput.style.display = "block";
    pickerInput.focus();
    const confirm = () => onConfirm(wallet, pickerInput.value.trim());
    pickerInput.onkeydown = (event) => {
      if (event.key === "Enter") confirm();
    };

    pickerOptions.textContent = "";
    const hint = document.createElement("p");
    hint.className = "hint";
    hint.append("Connecting with ");
    const walletName = document.createElement("strong");
    walletName.textContent = wallet.name;
    hint.append(
      walletName,
      ". Enter the public key to authenticate, then continue in the app.",
    );
    const confirmButton = document.createElement("button");
    confirmButton.className = "wallet-option";
    confirmButton.textContent = `Continue to ${wallet.name}`;
    confirmButton.addEventListener("click", confirm);
    pickerOptions.append(hint, confirmButton);
  }

  // The desktop multi-wallet chooser (issue #142) and accessible manual-entry
  // form (issue #148) are both optional — pages without the markup degrade to
  // direct connect / window.prompt via the controller's fallbacks.
  const desktopWallets = document.getElementById("desktop-wallets");
  const desktopWalletOptions = document.getElementById("desktop-wallet-options");
  const desktopWalletsCancel = document.getElementById("desktop-wallets-cancel");
  const supportsDesktopChooser = Boolean(desktopWallets && desktopWalletOptions);

  const manualForm = document.getElementById("manual-form");
  const manualInput = document.getElementById("manual-address");
  const manualCancel = document.getElementById("manual-cancel");
  const supportsManualForm = Boolean(manualForm && manualInput);

  let onDesktopSelect = null;
  let onDesktopCancel = null;
  let onManualSubmit = null;
  let onManualCancel = null;

  function closeDesktopChooser() {
    if (!supportsDesktopChooser) return;
    desktopWallets.style.display = "none";
    desktopWalletOptions.textContent = "";
    onDesktopSelect = null;
    onDesktopCancel = null;
  }

  function closeManualForm() {
    if (!supportsManualForm) return;
    manualForm.style.display = "none";
    manualInput.value = "";
    onManualSubmit = null;
    onManualCancel = null;
  }

  if (desktopWalletsCancel) {
    desktopWalletsCancel.addEventListener("click", () => {
      const cancelHandler = onDesktopCancel;
      closeDesktopChooser();
      if (cancelHandler) cancelHandler();
    });
  }

  if (manualForm) {
    manualForm.addEventListener("submit", (event) => {
      event.preventDefault();
      if (!onManualSubmit) return;
      const error = onManualSubmit(manualInput.value.trim());
      if (error) {
        status.textContent = `Error: ${error}`;
        manualInput.focus();
      }
      // On success the controller drives showConnected(), which closes the
      // form; nothing to do here.
    });
  }

  if (manualCancel) {
    manualCancel.addEventListener("click", () => {
      const cancelHandler = onManualCancel;
      closeManualForm();
      if (cancelHandler) cancelHandler();
    });
  }

  return {
    supportsWalletPicker,
    supportsDesktopChooser,
    supportsManualForm,
    showConnecting() {
      hideWalletInfo();
      button.textContent = "Connecting…";
      button.disabled = true;
      status.textContent = "Connecting…";
    },
    showReconnecting() {
      hideWalletInfo();
      button.textContent = "Reconnecting…";
      button.disabled = true;
      status.textContent = "Reconnecting wallet…";
    },
    showConnected(publicKey, statusText) {
      closeWalletPicker();
      closeDesktopChooser();
      closeManualForm();
      walletAddress.textContent = publicKey;
      walletInfo.style.display = "block";
      button.textContent = "Disconnect";
      button.disabled = false;
      status.textContent = statusText;
    },
    showDisconnected({ reconnect, statusText }) {
      closeWalletPicker();
      closeDesktopChooser();
      closeManualForm();
      hideWalletInfo();
      button.textContent = reconnect ? "Reconnect Wallet" : "Connect Wallet";
      button.disabled = false;
      status.textContent = statusText;
    },
    setStatus(text) {
      status.textContent = text;
    },
    openWalletPicker(wallets, { onConfirm, onCancel }) {
      if (!supportsWalletPicker) return false;

      onPickerCancel = onCancel || null;
      pickerOptions.textContent = "";
      wallets.forEach((wallet) => {
        const option = document.createElement("button");
        option.className = "wallet-option";
        const icon = document.createElement("span");
        icon.className = "icon";
        icon.textContent = wallet.icon || "⭐";
        const name = document.createElement("span");
        name.textContent = wallet.name;
        option.append(icon, name);
        option.addEventListener("click", () =>
          showPickerConfirm(wallet, onConfirm),
        );
        pickerOptions.appendChild(option);
      });

      pickerOverlay.classList.add("open");
      status.textContent = "";
      return true;
    },
    closeWalletPicker,
    openDesktopChooser(wallets, { onSelect, onCancel }) {
      if (!supportsDesktopChooser) return false;

      onDesktopSelect = onSelect || null;
      onDesktopCancel = onCancel || null;
      desktopWalletOptions.textContent = "";
      wallets.forEach((wallet) => {
        const option = document.createElement("button");
        option.type = "button";
        option.className = "wallet-option";
        const icon = document.createElement("span");
        icon.className = "icon";
        icon.setAttribute("aria-hidden", "true");
        icon.textContent = wallet.icon || "⭐";
        const name = document.createElement("span");
        name.textContent = wallet.name;
        option.append(icon, name);
        option.addEventListener("click", () => {
          if (onDesktopSelect) onDesktopSelect(wallet);
        });
        desktopWalletOptions.appendChild(option);
      });

      desktopWallets.style.display = "block";
      status.textContent =
        "Several wallet extensions detected. Choose one to connect.";
      const firstOption = desktopWalletOptions.querySelector("button");
      if (firstOption) firstOption.focus();
      return true;
    },
    closeDesktopChooser,
    openManualForm({ submit, onCancel }) {
      if (!supportsManualForm) return false;

      onManualSubmit = submit || null;
      onManualCancel = onCancel || null;
      manualInput.value = "";
      manualForm.style.display = "block";
      status.textContent =
        "No wallet extension detected. Enter your Stellar public key below.";
      manualInput.focus();
      return true;
    },
    closeManualForm,
    onAction(handler) {
      button.addEventListener("click", handler);
    },
  };
}

async function initializeWalletPage(browserWindow) {
  const provider =
    browserWindow.freighterApi || browserWindow.freighter || null;
  let storage = null;
  try {
    storage = browserWindow.localStorage;
  } catch (_error) {
    // The controller can still operate for the current page without storage.
  }

  const mobileApi = browserWindow.OrbitWalletConnect || null;

  const ui = createDomUi(browserWindow.document);
  const controller = createWalletSession({
    storage,
    provider,
    promptForAddress: () =>
      browserWindow.prompt(
        "Freighter not detected.\nEnter your Stellar public key to continue:",
      ),
    ui,
    emit: (name, detail) =>
      browserWindow.dispatchEvent(
        new browserWindow.CustomEvent(name, { detail }),
      ),
    mobile: mobileApi
      ? {
          isMobile: () => mobileApi.isMobile(),
          wallets: () => mobileApi.walletsFor("mobile"),
          start: (wallet, account) =>
            mobileApi.startMobileConnect(wallet, account),
          handleReturn: () => mobileApi.handleMobileReturn(),
        }
      : null,
    desktop: mobileApi
      ? {
          detected: () => mobileApi.detectedDesktopWallets(),
        }
      : null,
  });

  ui.onAction(() => controller.handleAction());
  browserWindow.connectWallet = controller.connect;
  browserWindow.disconnectWallet = controller.disconnect;

  await controller.initialize();
  return controller;
}

const walletSessionApi = {
  WALLET_SESSION_KEY,
  createDomUi,
  createWalletSession,
  initializeWalletPage,
  isValidPublicKey,
  readSession,
};

if (typeof module !== "undefined" && module.exports) {
  module.exports = walletSessionApi;
}

if (typeof window !== "undefined" && typeof document !== "undefined") {
  window.OrbitChainWalletSession = walletSessionApi;
  void initializeWalletPage(window);
}
