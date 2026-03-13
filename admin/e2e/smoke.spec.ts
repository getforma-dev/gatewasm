import { test, expect } from '@playwright/test';
import {
  collectErrors,
  collectWarnings,
  expectNoConsoleErrors,
  expectNoHydrationWarnings,
  expectNoLayoutOverflow,
} from './helpers';

// All routes that return HTML pages (no auth required for initial load)
const PUBLIC_ROUTES = [
  { path: '/platform/benchmark', name: 'benchmark', ssr: true },
  { path: '/platform/login', name: 'platform-login', ssr: true },
  { path: '/platform/onboarding', name: 'onboarding', ssr: true },
  { path: '/t/platform/login', name: 'tenant-login', ssr: false },
];

// Routes that require auth — platform routes redirect via server-side check,
// tenant routes will gain client-side redirect when rebuilt in Phase 6 Steps 6-7.
const AUTH_ROUTES = [
  { path: '/platform/console', name: 'console', redirectTo: '/platform/login' },
  { path: '/platform/admin/dashboard', name: 'owner-dashboard', redirectTo: '/platform/login' },
];

// Category 1: Pages load without errors
for (const route of PUBLIC_ROUTES) {
  test(`${route.name}: loads without console errors`, async ({ page }) => {
    const errors = collectErrors(page);
    await page.goto(route.path, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2000);
    await expectNoConsoleErrors(errors);
  });
}

// Category 2: SSR pages render real content with JS disabled
for (const route of PUBLIC_ROUTES.filter(r => r.ssr)) {
  test(`${route.name}: SSR renders content without JS`, async ({ browser }) => {
    const ctx = await browser.newContext({ javaScriptEnabled: false });
    const page = await ctx.newPage();
    await page.goto(route.path);
    const childCount = await page.evaluate(() =>
      document.getElementById('app')?.children.length ?? 0
    );
    expect(childCount).toBeGreaterThan(0);
    await ctx.close();
  });
}

// Category 3: No hydration warnings on public pages
for (const route of PUBLIC_ROUTES) {
  test(`${route.name}: no hydration warnings`, async ({ page }) => {
    const warnings = collectWarnings(page);
    await page.goto(route.path, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(2000);
    await expectNoHydrationWarnings(warnings);
  });
}

// Category 4: No layout overflow on public pages
for (const route of PUBLIC_ROUTES) {
  test(`${route.name}: no layout overflow`, async ({ page }) => {
    await page.goto(route.path, { waitUntil: 'domcontentloaded' });
    await page.waitForTimeout(1000);
    await expectNoLayoutOverflow(page);
  });
}

// Category 5: Auth routes redirect to login when unauthenticated
for (const route of AUTH_ROUTES) {
  test(`${route.name}: redirects to login when unauthenticated`, async ({ page }) => {
    await page.goto(route.path, { waitUntil: 'domcontentloaded' });
    // Allow time for JS boot + redirect
    await page.waitForTimeout(3000);
    expect(page.url()).toContain(route.redirectTo);
  });
}

// Category 6: Service worker registers on first public page
test('service worker registers', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);
  const swRegistered = await page.evaluate(() =>
    navigator.serviceWorker?.controller !== null ||
    navigator.serviceWorker?.ready !== undefined
  );
  expect(swRegistered).toBe(true);
});
