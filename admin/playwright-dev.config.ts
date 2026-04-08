import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  retries: 0,
  workers: 1,  // Sequential: tests share a single auth server with rate limits
  use: {
    baseURL: 'http://localhost:3001',
  },
  // No globalSetup/globalTeardown — assumes server is already running on :3001
});
