// tests/e2e/wallet-connect-csp.spec.js
//
// Playwright coverage for the page hardening added in issue #141: the
// Content Security Policy, the SRI-pinned Freighter script, and the CSS
// extraction that `style-src 'self'` depends on.
//
// The acceptance criterion is "CSP-tested", so this spec does not assert the
// policy *string* (wallet_connect.test.js does that statically) — it drives the
// real flows in a real browser and asserts the browser raised **zero**
// `securitypolicyviolation` events. Three specific things the plan flagged as
// theoretical-but-unverified are checked here rather than assumed:
//
//   1. `el.style.display = …` from JS is CSSOM, not an inline style attribute,
//      so `style-src 'self'` must not block it (createDomUi does it ~10 times).
//   2. `web+stellar:` / custom-scheme deep links are top-level navigations, not
//      fetches, so no directive governs them.
//   3. `crossorigin="anonymous"` changes the cdnjs request into a CORS request;
//      if cdnjs's headers or the pinned hash were wrong the script would be
//      blocked, so the spec asserts `window.freighterApi` actually exists.

import { test, expect } from '@playwright/test';

const CLIENT_ACCOUNT = 'GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3';
const AUTH_SERVER = 'http://localhost:4000';

function isMobileProject(testInfo) {
  return testInfo.project.name !== 'desktop-chromium';
}

/**
 * Collect CSP violations from before the first byte of page script runs.
 * `securitypolicyviolation` bubbles to window, so one listener covers
 * violations reported against any element or the document itself.
 */
async function collectViolations(page) {
  await page.addInitScript(() => {
    window.__cspViolations = [];
    window.addEventListener('securitypolicyviolation', (event) => {
      window.__cspViolations.push({
        directive: event.effectiveDirective || event.violatedDirective,
        blockedURI: event.blockedURI,
        sample: event.sample,
      });
    });
  });
}

async function violations(page) {
  return page.evaluate(() => window.__cspViolations || []);
}

async function expectNoViolations(page, context) {
  expect(await violations(page), `CSP violations during ${context}`).toEqual([]);
}

test.describe('CSP + SRI hardening', () => {
  test.beforeEach(async ({ page }) => {
    await collectViolations(page);
  });

  test('page load is CSP-clean and the external stylesheet is applied', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await expect(page.locator('#connect-btn')).toBeVisible();

    await expectNoViolations(page, 'page load');

    // The stylesheet is a separate request now; prove it actually landed
    // rather than trusting that <link> in <head> is render-blocking.
    const card = page.locator('main.card');
    await expect(card).toHaveCSS('border-radius', '12px');
    await expect(page.locator('body')).toHaveCSS('display', 'flex');
    // A token from the theme block — if the CSS 404'd this would be the
    // UA default, not the palette value.
    const codeBg = await page
      .locator('#wallet-address')
      .evaluate((el) => getComputedStyle(el).fontFamily);
    expect(codeBg).toContain('monospace');
  });

  test('the SRI-pinned Freighter script executed (hash and CORS both correct)', async ({ page }) => {
    await page.goto('/wallet_connect.html');

    // Blocked-by-SRI leaves the global undefined. This is the check that turns
    // a silent CDN hash drift into a red build.
    await expect
      .poll(() => page.evaluate(() => typeof window.freighterApi))
      .toBe('object');

    await expectNoViolations(page, 'Freighter script load');
  });

  // Negative control. Every other test in this file asserts an *empty*
  // violation list, which would also pass if the policy were absent or the
  // listener never wired up. This proves the harness has teeth.
  test('the policy is actually enforced (inline style and foreign script are blocked)', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await expectNoViolations(page, 'baseline before the control');

    await page.evaluate(() => {
      const style = document.createElement('style');
      style.textContent = 'body { outline: 1px solid red; }';
      document.head.appendChild(style);

      const script = document.createElement('script');
      script.src = 'https://example.com/not-allowed.js';
      document.head.appendChild(script);
    });

    await expect.poll(() => violations(page).then((v) => v.length)).toBeGreaterThan(1);
    const reported = await violations(page);
    // Browsers report the *effective* directive, which for elements is the
    // more specific `-elem` form falling back to our `style-src`/`script-src`.
    const directives = reported.map((v) => v.directive);
    expect(directives.some((d) => d.startsWith('style-src'))).toBe(true);
    expect(directives.some((d) => d.startsWith('script-src'))).toBe(true);
  });

  test('the cdnjs request is a CORS request and succeeds', async ({ page }) => {
    const [response] = await Promise.all([
      page.waitForResponse((res) => res.url().includes('cdnjs.cloudflare.com')),
      page.goto('/wallet_connect.html'),
    ]);

    expect(response.status()).toBe(200);
    // crossorigin="anonymous" makes this a CORS request; cdnjs must answer it.
    expect(response.headers()['access-control-allow-origin']).toBe('*');
  });
});

test.describe('CSP across the desktop flows', () => {
  test.beforeEach(async ({ page }, testInfo) => {
    test.skip(isMobileProject(testInfo), 'desktop-only flows');
    await collectViolations(page);
  });

  test('manual-entry fallback toggles display and stays CSP-clean', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');

    // #manual-form is display:none in the stylesheet and shown by
    // `manualForm.style.display = "block"` — the CSSOM-vs-style-src check.
    await expect(page.locator('#manual-form')).toBeVisible();

    await page.fill('#manual-address', CLIENT_ACCOUNT);
    await page.click('#manual-form button[type="submit"]');
    await expect(page.locator('#wallet-address')).toHaveText(CLIENT_ACCOUNT);
    await expect(page.locator('#wallet-info')).toBeVisible();

    await expectNoViolations(page, 'manual entry');
  });

  test('the multi-wallet chooser stays CSP-clean', async ({ page }) => {
    await page.addInitScript((addr) => {
      window.freighter = {
        isConnected: async () => true,
        connect: async () => {},
        getAddress: async () => ({ address: addr }),
      };
      window.rabet = { connect: async () => ({ publicKey: addr }) };
    }, CLIENT_ACCOUNT);

    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');
    await expect(page.locator('#desktop-wallets')).toBeVisible();

    await page.locator('#desktop-wallet-options button', { hasText: 'Rabet' }).click();
    await expect(page.locator('#wallet-address')).toHaveText(CLIENT_ACCOUNT);

    await expectNoViolations(page, 'desktop chooser');
  });

  test('a form submit is not silently blocked by form-action', async ({ page }) => {
    // The submit handler preventDefault()s, so no navigation is ever attempted
    // and `form-action 'none'` should never fire. If the handler regressed,
    // the page would navigate — this pins both halves.
    await page.goto('/wallet_connect.html');
    const urlBefore = page.url();

    await page.click('#connect-btn');
    await page.fill('#manual-address', CLIENT_ACCOUNT);
    await page.press('#manual-address', 'Enter');

    await expect(page.locator('#wallet-address')).toHaveText(CLIENT_ACCOUNT);
    expect(page.url()).toBe(urlBefore);
    await expectNoViolations(page, 'form submit');
  });
});

test.describe('CSP across the mobile SEP-10 flow', () => {
  test.beforeEach(async ({ page }, testInfo) => {
    test.skip(!isMobileProject(testInfo), 'mobile-only flow');
    await collectViolations(page);
  });

  test('connect-src permits the SEP-10 challenge fetch to the auth server', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');
    await expect(page.locator('#wallet-picker-overlay')).toHaveClass(/open/);

    await expectNoViolations(page, 'wallet picker');

    await page.getByText('Lobstr', { exact: true }).click();
    await page.fill('#pubkey-input', CLIENT_ACCOUNT);

    const [response] = await Promise.all([
      page.waitForResponse((res) => res.url().startsWith(`${AUTH_SERVER}/auth?account=`)),
      page.getByRole('button', { name: 'Continue to Lobstr' }).click(),
    ]);

    // A connect-src violation would abort the fetch before a response exists.
    expect(response.status()).toBe(200);
    await expectNoViolations(page, 'SEP-10 challenge fetch');
  });

  test('the SEP-0007 deep-link navigation is not blocked', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');
    await page.getByText('Lobstr', { exact: true }).click();
    await page.fill('#pubkey-input', CLIENT_ACCOUNT);
    await page.getByRole('button', { name: 'Continue to Lobstr' }).click();

    // The challenge fetch resolves, then wallet_connect.js assigns
    // location.href = 'web+stellar:tx?…'. No wallet app is registered in the
    // test browser so nothing happens — the point is that CSP raised nothing.
    await page.waitForTimeout(1000);
    await expectNoViolations(page, 'deep-link navigation');
  });
});
