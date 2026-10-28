import { defineConfig } from '@playwright/test';

// Reuse an explicitly started synthetic server. Never start/stop other services.
if (!process.env.BASE_URL) throw new Error('Set BASE_URL to the owned local synthetic auth server');
export default defineConfig({
  testDir: './e2e',
  testMatch: 'hosted-signup.spec.ts',
  timeout: 45_000,
  retries: 0,
  workers: 1,
  use: { baseURL: process.env.BASE_URL, colorScheme: 'dark' },
  projects: ['chromium', 'firefox', 'webkit'].map(browserName => ({
    name: browserName,
    use: { browserName: browserName as 'chromium' | 'firefox' | 'webkit' },
  })),
});
