/**
 * E2E Auth Helper — register/login a test user for auth-gated page tests.
 *
 * Usage in Playwright:
 *   const page = await loginAsTestUser(browser);
 *   await page.goto('/platform/console');
 *
 * Uses the API directly (POST /auth/register or /login with X-Tenant-Slug header)
 * which sets the httpOnly refresh cookie in the browser context.
 * Subsequent page navigations can silentRefresh() using that cookie.
 *
 * Note: Uses X-Tenant-Slug header instead of /t/{slug}/ URL path because
 * the slug_dispatch path rewriter can interfere with POST body forwarding.
 */
import type { Browser, Page, BrowserContext } from '@playwright/test';

const BASE = 'http://127.0.0.1:3101';
const TENANT_HEADER = { 'X-Tenant-Slug': 'platform' };
const TEST_EMAIL = 'e2e-test-' + Date.now() + '@test.local';
const TEST_PASSWORD = 'E2eTestPassword123!';

/**
 * Create a browser context with a logged-in test user.
 * Returns { context, page } — caller must close context when done.
 */
export async function loginAsTestUser(
  browser: Browser,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext();
  const page = await context.newPage();

  // Try register first, fall back to login if user exists
  const registerRes = await context.request.post(BASE + '/auth/register', {
    headers: TENANT_HEADER,
    data: { email: TEST_EMAIL, password: TEST_PASSWORD },
  });

  if (!registerRes.ok()) {
    // User might already exist — try login
    const loginRes = await context.request.post(BASE + '/auth/login', {
      headers: TENANT_HEADER,
      data: { email: TEST_EMAIL, password: TEST_PASSWORD },
    });
    if (!loginRes.ok()) {
      throw new Error('Auth helper: could not register or login test user');
    }
  }

  return { context, page };
}
