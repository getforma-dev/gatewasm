import { Page, expect, APIRequestContext, BrowserContext } from '@playwright/test';

export const BASE = process.env.BASE_URL || 'http://127.0.0.1:3102';

/**
 * POST with automatic 429 retry and exponential backoff.
 * Works with both APIRequestContext (from test fixtures) and
 * BrowserContext.request (from browser-based tests).
 */
export async function postWithRetry(
  request: APIRequestContext,
  url: string,
  options: { headers?: Record<string, string>; data?: unknown },
  attempts = 6,
): Promise<any> {
  let response: any;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    response = await request.post(url, options);
    if (response.status() !== 429) return response;
    const retryAfter = Number(response.headers()['retry-after'] || '2');
    await new Promise(r => setTimeout(r, Math.max(1000, retryAfter * 1000 + 500)));
  }
  return response;
}

/**
 * Exchange the refresh cookie for a short-lived access token.
 * Retries on 429 and tries multiple token paths for compatibility.
 */
export async function getAccessToken(context: BrowserContext): Promise<string> {
  for (const path of ['/t/platform/auth/token', '/auth/token']) {
    const headers: Record<string, string> = {};
    if (path === '/auth/token') headers['X-Tenant-Slug'] = 'platform';
    for (let attempt = 0; attempt < 3; attempt++) {
      const res = await context.request.post(BASE + path, {
        headers,
        data: { grant_type: 'refresh_token' },
      });
      if (res.ok()) {
        const json = await res.json();
        return json.access_token as string;
      }
      if (res.status() === 429) {
        const retryAfter = Number(res.headers()['retry-after'] || '2');
        await new Promise(r => setTimeout(r, Math.max(1000, retryAfter * 1000 + 500)));
        continue;
      }
      break; // non-retryable error, try next path
    }
  }
  throw new Error('getAccessToken: failed to obtain token from any path');
}

/**
 * Collect console errors during page load. Filters known CSP violations.
 * Call BEFORE page.goto().
 */
export function collectErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', msg => {
    if (msg.type() === 'error') {
      const text = msg.text();
      // Filter known browser-level noise (not app errors)
      if (!text.includes('Content Security Policy') &&
          !text.includes('Failed to load resource')) {
        errors.push(text);
      }
    }
  });
  return errors;
}

/**
 * Collect [FormaJS] hydration warnings.
 * Call BEFORE page.goto().
 *
 * CSR fallback warnings ("falling back to CSR") are expected for islands
 * that don't have full SSR content yet. These are collected separately
 * and excluded from the "no hydration warnings" assertion.
 */
export function collectWarnings(page: Page): string[] {
  const warnings: string[] = [];
  page.on('console', msg => {
    if (msg.type() === 'warning' && msg.text().includes('[forma]')) {
      // CSR fallback is expected for islands without full SSR content
      // "No hydrate function for island_N" are compiler false positives
      // (internal helper functions detected as islands)
      if (!msg.text().includes('falling back to CSR') &&
          !(/No hydrate function for island "island_\d+"/.test(msg.text()))) {
        warnings.push(msg.text());
      }
    }
  });
  return warnings;
}

/** Assert no console errors after page load. */
export function expectNoConsoleErrors(errors: string[]) {
  expect(errors).toEqual([]);
}

/** Assert no FormaJS hydration warnings. */
export function expectNoHydrationWarnings(warnings: string[]) {
  expect(warnings).toEqual([]);
}

/**
 * Load route with JS disabled, assert real HTML content exists.
 * Only meaningful for SSR pages (Phase 2).
 */
export async function expectSSRContent(page: Page, route: string, browser: any) {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const noJsPage = await context.newPage();
  await noJsPage.goto(route);
  // #app should have real children (not empty shell)
  const childCount = await noJsPage.evaluate(() =>
    document.getElementById('app')?.children.length ?? 0
  );
  expect(childCount).toBeGreaterThan(0);
  await context.close();
}

/**
 * Assert all islands reached status="active" (no pending islands remain).
 * Polls until no pending islands exist, up to 5s timeout.
 */
export async function expectIslandsActive(page: Page) {
  await page.waitForFunction(
    () => document.querySelectorAll('[data-forma-status="pending"]').length === 0,
    { timeout: 5000 },
  );
}

/**
 * Assert no horizontal overflow (layout isn't broken).
 */
export async function expectNoLayoutOverflow(page: Page) {
  const overflow = await page.evaluate(() =>
    document.documentElement.scrollWidth > document.documentElement.clientWidth
  );
  expect(overflow).toBe(false);
}

/**
 * Assert DOM element count doesn't more than double after JS loads.
 * Catches hydration doubling (mount() creating fresh DOM alongside SSR).
 */
export async function expectNoDOMDoubling(
  ssrCount: number,
  jsCount: number,
) {
  // Allow some growth (islands add interactive elements) but not 2x
  expect(jsCount).toBeLessThan(ssrCount * 2);
}
