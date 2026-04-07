import { Page, expect } from '@playwright/test';

export const BASE = process.env.BASE_URL || 'http://127.0.0.1:3102';

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
export async function expectNoConsoleErrors(errors: string[]) {
  expect(errors).toEqual([]);
}

/** Assert no FormaJS hydration warnings. */
export async function expectNoHydrationWarnings(warnings: string[]) {
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
 * Waits up to 3s for hydration to complete.
 */
export async function expectIslandsActive(page: Page) {
  await page.waitForTimeout(2000);
  const pending = await page.evaluate(() =>
    document.querySelectorAll('[data-forma-status="pending"]').length
  );
  expect(pending).toBe(0);
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
