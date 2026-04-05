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
import type { APIResponse, Browser, Page, BrowserContext } from '@playwright/test';

const BASE = process.env.BASE_URL || 'http://127.0.0.1:3102';
const TENANT_HEADER = { 'X-Tenant-Slug': 'platform' };
const TEST_PASSWORD = 'E2eTestPassword123!';

async function postWithRetry(
  context: BrowserContext,
  url: string,
  data: Record<string, unknown>,
  attempts = 4,
): Promise<APIResponse> {
  let lastResponse: APIResponse | null = null;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const response = await context.request.post(url, {
      headers: TENANT_HEADER,
      data,
    });
    lastResponse = response;
    if (response.status() !== 429) {
      return response;
    }
    const retryAfter = Number(response.headers()['retry-after'] || '1');
    await new Promise((resolve) => setTimeout(resolve, Math.max(500, retryAfter * 1000)));
  }

  if (!lastResponse) {
    throw new Error('Auth helper: no response received');
  }

  return lastResponse;
}

/**
 * Create a browser context with a logged-in test user.
 * Returns { context, page } — caller must close context when done.
 */
export async function loginAsTestUser(
  browser: Browser,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext();
  const page = await context.newPage();
  const testEmail = 'e2e-test-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8) + '@test.local';

  // Try register first, fall back to login if user exists
  const registerRes = await postWithRetry(context, BASE + '/auth/register', {
    email: testEmail,
    password: TEST_PASSWORD,
    name: 'E2E Test User',
  });

  if (!registerRes.ok()) {
    // User might already exist — try login
    const loginRes = await postWithRetry(context, BASE + '/auth/login', {
      email: testEmail,
      password: TEST_PASSWORD,
    });
    if (!loginRes.ok()) {
      throw new Error('Auth helper: could not register or login test user');
    }
  }

  return { context, page };
}
