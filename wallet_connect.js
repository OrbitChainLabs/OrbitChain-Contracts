/**
 * OrbitChain Wallet Connect — mobile wallet deep-link + SEP-10 auth client.
 *
 * Desktop path: Freighter browser extension (window.freighter), unchanged.
 * Mobile path:  navigator.userAgent detection -> wallet picker (Lobstr,
 *               Bitnovo, or any other SEP-0007-compatible wallet) -> SEP-10
 *               challenge fetch -> SEP-0007 `web+stellar:tx` deep link with a
 *               callback URL -> wallet signs and redirects back -> signed
 *               challenge is POSTed to the auth server for a session token.
 *
 * Spec references:
 *   SEP-0007  https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0007.md
 *   SEP-0010  https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md
 *
 * See docs/tutorials/dapp-integration.md for the full write-up, including why
 * mobile wallets need a two-step (identify, then sign) bootstrap instead of
 * the single `window.freighter` call the desktop path uses.
 */
(function (global) {
  'use strict';

  // ── Config ────────────────────────────────────────────────────────────────
  // Overridable via <script data-auth-server="https://auth.example.com">
  // or window.ORBITCHAIN_AUTH_SERVER before this file loads.
  function resolveAuthServer() {
    if (global.ORBITCHAIN_AUTH_SERVER) return global.ORBITCHAIN_AUTH_SERVER;
    var thisScript = document.currentScript;
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
