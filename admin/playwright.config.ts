import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  retries: 0,
  workers: 1,  // Sequential: tests share a single auth server with rate limits
  use: {
    baseURL: 'http://127.0.0.1:3102',
    // First-visit theme follows prefers-color-scheme, so the runner's own
    // setting would otherwise decide which theme the suite (and the visual
    // baselines) see. Pin it to the product's primary presentation; light
    // mode gets asserted explicitly where it matters.
    colorScheme: 'dark',
  },
  globalSetup: './e2e/global-setup.ts',
  globalTeardown: './e2e/global-teardown.ts',
});
