// tests/e2e/wallet-connect-desktop-multi.spec.js
//
// Playwright coverage for the desktop multi-wallet registry (issue #142)
// added to wallet_connect.js/wallet_connect.html: extension detection via
// injected globals, direct connect when exactly one wallet is present, the
// inline chooser when several are, manual-entry fallback when none are —
// and the invariant that the mobile picker overlay never opens on desktop.
//
// Wallet extensions cannot be installed in the test browser, so each spec
// injects a minimal fake of the vendor-documented API surface with
// addInitScript before the page loads — the same seam the adapters detect.
//
// Runs against the `desktop-chromium` project only; the mobile deep-link
// flow keeps its own coverage in wallet-connect-mobile.spec.js.

import { test, expect } from '@playwright/test';

const FREIGHTER_ACCOUNT = 'GAMX62ZD4FWIKMWGVPEDR6WNL2TYTPQMO2ZJEAZUAON7VCZ5G2GWDF7W';
const RABET_ACCOUNT = 'GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3';

function isDesktopProject(testInfo) {
  return testInfo.project.name === 'desktop-chromium';
}

test.describe('desktop multi-wallet registry', () => {
  test.beforeEach(async ({}, testInfo) => {
    test.skip(!isDesktopProject(testInfo), 'desktop-only flow');
  });

  test('exactly one detected wallet connects directly (historical UX)', async ({ page }) => {
    await page.addInitScript((addr) => {
      window.freighter = {
        isConnected: async () => true,
        connect: async () => {},
        getAddress: async () => ({ address: addr }),
      };
    }, FREIGHTER_ACCOUNT);
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');

    await expect(page.locator('#wallet-address')).toHaveText(FREIGHTER_ACCOUNT);
    await expect(page.locator('#connect-btn')).toHaveText('Disconnect');
    await expect(page.locator('#desktop-wallets')).toBeHidden();
    await expect(page.locator('#wallet-picker-overlay')).not.toHaveClass(/open/);
  });

  test('several detected wallets show the inline chooser, never the mobile overlay', async ({ page }) => {
    await page.addInitScript((addr) => {
      window.freighter = {
        isConnected: async () => true,
        connect: async () => {},
        getAddress: async () => ({ address: 'GFREIGHTERSHOULDNOTBEUSEDINTHISSPECAAAAAAAAAAAAAAAAAAAAA' }),
      };
      window.rabet = { connect: async () => ({ publicKey: addr }) };
    }, RABET_ACCOUNT);
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');

    const chooser = page.locator('#desktop-wallets');
    await expect(chooser).toBeVisible();
    await expect(page.locator('#desktop-wallet-options')).toContainText('Freighter');
    await expect(page.locator('#desktop-wallet-options')).toContainText('Rabet');
    await expect(page.locator('#wallet-picker-overlay')).not.toHaveClass(/open/);

    await page.locator('#desktop-wallet-options button', { hasText: 'Rabet' }).click();
    await expect(page.locator('#wallet-address')).toHaveText(RABET_ACCOUNT);
    await expect(page.locator('#connect-btn')).toHaveText('Disconnect');
    await expect(chooser).toBeHidden();
  });

  test('chooser cancel restores the connect button', async ({ page }) => {
    await page.addInitScript(() => {
      window.freighter = { isConnected: async () => true, getAddress: async () => ({ address: 'G' }) };
      window.rabet = { connect: async () => ({ publicKey: 'G' }) };
    });
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');
    await expect(page.locator('#desktop-wallets')).toBeVisible();

    await page.click('#desktop-wallets-cancel');
    await expect(page.locator('#desktop-wallets')).toBeHidden();
    await expect(page.locator('#connect-btn')).toBeEnabled();
  });

  test('no detected wallets fall back to the labelled manual-entry form', async ({ page }) => {
    await page.goto('/wallet_connect.html');
    await page.click('#connect-btn');

    await expect(page.locator('#manual-form')).toBeVisible();
    await expect(page.locator('#wallet-picker-overlay')).not.toHaveClass(/open/);

    await page.fill('#manual-address', RABET_ACCOUNT);
    await page.click('#manual-form button[type="submit"]');
    await expect(page.locator('#wallet-address')).toHaveText(RABET_ACCOUNT);
  });
});
