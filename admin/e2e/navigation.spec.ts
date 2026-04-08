/**
 * Cross-page Navigation Tests
 *
 * Verifies auth-gated redirects, post-login navigation, and session
 * continuity across all page types (platform, tenant, owner).
 */
import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { collectErrors, expectNoConsoleErrors } from './helpers';
import { Account } from './selectors';

// ── Auth redirect: unauthenticated users go to login ────────────────────

test('tenant dashboard redirects to tenant login when unauthenticated @smoke @regression', async ({ page }) => {
  await page.goto('/t/platform/dashboard');
  await page.waitForURL('**/t/platform/login', { timeout: 8000 });
  expect(page.url()).toContain('/t/platform/login');
});

test('tenant admin redirects to tenant login when unauthenticated @smoke @regression', async ({ page }) => {
  await page.goto('/t/platform/admin');
  await page.waitForURL('**/t/platform/login', { timeout: 8000 });
  expect(page.url()).toContain('/t/platform/login');
});

// ── Authenticated cross-page navigation ─────────────────────────────────

test('authenticated: can navigate from console to login and back @smoke @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  // Start at console
  await page.goto('/platform/console');
  await expect(page.locator(Account.shell)).toBeVisible({ timeout: 8000 });

  // Navigate to login page (should still work even when authenticated)
  await page.goto('/platform/login', { waitUntil: 'networkidle' });

  // Navigate back to console (auth cookie should still be valid)
  await page.goto('/platform/console');
  await expect(page.locator(Account.shell)).toBeVisible({ timeout: 8000 });

  await context.close();
});

test('authenticated: no console errors across platform pages @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const errors = collectErrors(page);

  // Visit console
  await page.goto('/platform/console', { waitUntil: 'networkidle' });

  // Visit onboarding
  await page.goto('/platform/onboarding', { waitUntil: 'networkidle' });

  // Visit benchmark
  await page.goto('/platform/benchmark', { waitUntil: 'networkidle' });

  expectNoConsoleErrors(errors);
  await context.close();
});

// ── Page-specific config injection ──────────────────────────────────────

test('tenant dashboard receives snake_case config @regression', async ({ page }) => {
  // Block external scripts to prevent the client-side auth redirect that
  // races with page.evaluate. __TENANT__ is set by an inline <script> during
  // HTML parsing — it doesn't need module scripts to be available.
  await page.route('**/_assets/**', route => route.abort());
  await page.goto('/t/platform/dashboard', { waitUntil: 'domcontentloaded' });

  const config = await page.evaluate(() => (window as any).__TENANT__);
  expect(config).toBeTruthy();
  expect(config.slug).toBe('platform');
  expect(config.server_url).toBe('/t/platform');
  // Verify no camelCase keys leaked
  expect(config.serverUrl).toBeUndefined();
  expect(config.appName).toBeUndefined();
  expect(config.adminConsoleMode).toBeUndefined();
});

test('tenant admin receives admin_console_mode config @regression', async ({ page }) => {
  await page.goto('/t/platform/admin', { waitUntil: 'domcontentloaded' });

  const config = await page.evaluate(() => (window as any).__TENANT__);
  expect(config).toBeTruthy();
  expect(config.slug).toBe('platform');
  expect(config.admin_console_mode).toBe(true);
  expect(config.initial_view).toBe('stats');
});

test('tenant login receives correct tenant config @regression', async ({ page }) => {
  await page.goto('/t/platform/login', { waitUntil: 'domcontentloaded' });

  const config = await page.evaluate(() => (window as any).__TENANT__);
  expect(config).toBeTruthy();
  expect(config.slug).toBe('platform');
  expect(config.server_url).toBe('/t/platform');
  expect(config.providers).toBeTruthy();
});

// ── Tenant pages load without console errors (not covered by smoke.spec.ts) ──

const TENANT_PAGES = [
  '/t/platform/dashboard',
  '/t/platform/admin',
];

for (const path of TENANT_PAGES) {
  test(`cross-page: ${path} loads without errors @regression`, async ({ page }) => {
    const errors = collectErrors(page);
    await page.goto(path, { waitUntil: 'networkidle' });
    expectNoConsoleErrors(errors);
  });
}
