// tests/e2e/wallet-connect-mobile.spec.js
//
// Playwright coverage for the mobile wallet deep-link flow added to
// wallet_connect.html: UA-based routing to the wallet picker, SEP-10
// challenge requests for Lobstr/Bitnovo, the desktop-path regression check,
// and the full SEP-10 return handshake.
//
// Runs against the `mobile-safari` (iPhone 13) and `mobile-chrome`
// (Pixel 7) Playwright device projects for the mobile-only specs, and
// `desktop-chromium` for the regression check — see playwright.config.js.
//
// Closes issue: "Mobile wallet deep-link & Lobstr / Bitnovo support"

import { test, expect } from '@playwright/test';
import { Keypair, Networks, Transaction } from '@stellar/stellar-sdk';

const CLIENT_ACCOUNT = 'GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3';
const CLIENT_SECRET = 'SDU3MUQQMASWGMAY2P6ZILNP2V77BWU5NF3R6X4YDNOHPNXZYLHTXNPV';
const NETWORK_PASSPHRASE = Networks.TESTNET;
const AUTH_SERVER = 'http://localhost:4000';

function isMobileProject(testInfo) {
  return testInfo.project.name !== 'desktop-chromium';
}

test.describe('mobile wallet picker', () => {
  test.beforeEach(async ({ page }, testInfo) => {
    test.skip(!isMobileProject(testInfo), 'mobile-only flow');
    await page.goto('/wallet_connect.html');
  });

  test('shows Lobstr and Bitnovo when a mobile UA connects', async ({ page }) => {
    await page.click('#connect-btn');

    const overlay = page.locator('#wallet-picker-overlay');
    await expect(overlay).toHaveClass(/open/);
    await expect(page.locator('#wallet-options')).toContainText('Lobstr');
    await expect(page.locator('#wallet-options')).toContainText('Bitnovo Wallet');
  });

  test('picking Lobstr requests a real SEP-10 challenge for the entered account', async ({ page }) => {
    await page.click('#connect-btn');
    await page.getByText('Lobstr', { exact: true }).click();
    await page.fill('#pubkey-input', CLIENT_ACCOUNT);

    const [request] = await Promise.all([
      page.waitForRequest((req) => req.url().startsWith(`${AUTH_SERVER}/auth?account=`)),
      page.getByRole('button', { name: 'Continue to Lobstr' }).click(),
    ]);

    expect(request.url()).toContain(CLIENT_ACCOUNT);
  });

  test('picking Bitnovo requests a real SEP-10 challenge for the entered account', async ({ page }) => {
    await page.click('#connect-btn');
    await page.getByText('Bitnovo Wallet', { exact: true }).click();
    await page.fill('#pubkey-input', CLIENT_ACCOUNT);

    const [request] = await Promise.all([
      page.waitForRequest((req) => req.url().startsWith(`${AUTH_SERVER}/auth?account=`)),
      page.getByRole('button', { name: 'Continue to Bitnovo Wallet' }).click(),
    ]);

    expect(request.url()).toContain(CLIENT_ACCOUNT);
  });

  test('rejects an invalid public key before ever calling the auth server', async ({ page }) => {
    await page.click('#connect-btn');
    await page.getByText('Lobstr', { exact: true }).click();
    await page.fill('#pubkey-input', 'not-a-valid-key');
    await page.getByRole('button', { name: 'Continue to Lobstr' }).click();

    await expect(page.locator('#status')).toContainText('Enter a valid Stellar public key');
  });
});

test.describe('desktop path regression', () => {
  test.beforeEach(async ({ page }, testInfo) => {
    test.skip(isMobileProject(testInfo), 'desktop-only check');
    await page.goto('/wallet_connect.html');
  });

  test('desktop UA never sees the mobile wallet picker', async ({ page }) => {
    page.once('dialog', (dialog) => dialog.dismiss());
    await page.click('#connect-btn');
    await expect(page.locator('#wallet-picker-overlay')).not.toHaveClass(/open/);
  });
});

test.describe('SEP-10 return handshake', () => {
  test.beforeEach(async ({}, testInfo) => {
    test.skip(!isMobileProject(testInfo), 'mobile-only flow');
  });

  test('completes the loop when the wallet redirects back with a signed challenge', async ({ page, request }) => {
    const challengeRes = await request.get(
      `${AUTH_SERVER}/auth?account=${CLIENT_ACCOUNT}&home_domain=localhost`,
    );
    const { transaction } = await challengeRes.json();

    const tx = new Transaction(transaction, NETWORK_PASSPHRASE);
    tx.sign(Keypair.fromSecret(CLIENT_SECRET));
    const signedXdr = tx.toXDR();

    await page.goto(`/wallet_connect.html?orbitchain_xdr=1&xdr=${encodeURIComponent(signedXdr)}`);

    await expect(page.locator('#wallet-info')).toBeVisible();
    await expect(page.locator('#wallet-address')).toHaveText(CLIENT_ACCOUNT);
    await expect(page.locator('#connect-btn')).toHaveText('Disconnect');
  });
});
