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
  { path: '/platform/benchmark', name: 'benchmark' },
  { path: '/platform/login', name: 'platform-login' },
  { path: '/platform/onboarding', name: 'onboarding' },
  { path: '/t/platform/login', name: 'tenant-login' },
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

// Category 2: Public pages ship a boot shell with assets/config even before JS runs
for (const route of PUBLIC_ROUTES) {
  test(`${route.name}: serves the boot shell without JS`, async ({ browser }) => {
    const ctx = await browser.newContext({ javaScriptEnabled: false });
    const page = await ctx.newPage();
    await page.goto(route.path);
    const shell = await page.evaluate(() => ({
      hasAppRoot: document.getElementById('app') !== null,
      hasStylesheet: document.querySelector('link[rel="stylesheet"][href*="/_assets/"]') !== null,
      hasModuleEntry: document.querySelector('script[type="module"][src*="/_assets/"]') !== null,
      hasInlineConfig: Array.from(document.scripts).some((script) =>
        (script.textContent || '').includes('window.__')
      ),
    }));
    expect(shell.hasAppRoot).toBe(true);
    expect(shell.hasStylesheet).toBe(true);
    expect(shell.hasModuleEntry).toBe(true);
    expect(shell.hasInlineConfig).toBe(true);
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

// Category 6: Debug builds should not emit or keep a service worker registration
test('debug build does not register a service worker', async ({ page }) => {
  await page.goto('/platform/benchmark');
  await page.waitForTimeout(2000);
  const hasRegistrationScript = await page.evaluate(() =>
    Array.from(document.scripts).some((script) =>
      (script.textContent || '').includes('serviceWorker.register')
    )
  );
  const registrationCount = await page.evaluate(async () => {
    if (!('serviceWorker' in navigator)) {
      return 0;
    }
    const registrations = await navigator.serviceWorker.getRegistrations();
    return registrations.length;
  });
  expect(hasRegistrationScript).toBe(false);
  expect(registrationCount).toBe(0);
});
