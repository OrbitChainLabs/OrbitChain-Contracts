// playwright.config.js — CI/local runner for the wallet_connect.html mobile
// deep-link flow. See docs/tutorials/dapp-integration.md and
// tests/e2e/wallet-connect-mobile.spec.js.
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:8055',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'mobile-safari',
      use: { ...devices['iPhone 13'] },
    },
    {
      name: 'mobile-chrome',
      use: { ...devices['Pixel 7'] },
    },
    {
      name: 'desktop-chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: [
    {
      // Plain static file server for wallet_connect.html/js — no extra
      // devDependency needed, python3 ships on the CI image already.
      command: 'python3 -m http.server 8055',
      url: 'http://localhost:8055/wallet_connect.html',
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
    {
      command: 'node scripts/sep10-server.mjs',
      url: 'http://localhost:4000/auth?account=GATOACHAPPG72R2KKG5K47ORQVZKGBQ4UYVWLIYITEKMNFXQLNPJFJI3',
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
  ],
});
