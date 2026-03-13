/**
 * Platform Login Page — E2E Tests
 *
 * Tests cover SSR correctness, hydration integrity, interactive behavior,
 * and layout stability. Auth flow (actual login, MFA, OAuth) is NOT tested
 * here since it requires seeded credentials and external services.
 */
import { test, expect } from '@playwright/test';
import {
  collectErrors,
  collectWarnings,
  expectNoConsoleErrors,
  expectNoHydrationWarnings,
  expectNoLayoutOverflow,
} from './helpers';

// --- SSR Correctness (JS disabled) ---

test('SSR renders login form with JS disabled', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/login');

  // #app should have real children (not empty shell)
  const childCount = await page.evaluate(() =>
    document.getElementById('app')?.children.length ?? 0
  );
  expect(childCount).toBeGreaterThan(0);

  // Hero section visible
  const hero = await page.locator('.hero').count();
  expect(hero).toBe(1);

  // Auth card visible
  const card = await page.locator('.card').count();
  expect(card).toBe(1);

  await context.close();
});

test('SSR: password field has type="password" (no leak)', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/login');

  const passwordType = await page.locator('#password').getAttribute('type');
  expect(passwordType).toBe('password');

  await context.close();
});

// --- Hydration Integrity ---

test('initial hydration: no console errors', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/login');
  await page.waitForTimeout(2000);
  await expectNoConsoleErrors(errors);
});

test('initial hydration: no FormaJS warnings for registered islands', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto('/platform/login');
  await page.waitForTimeout(2000);
  // Sub-islands (island_1, island_2, etc.) emit expected warnings since they are
  // inline helpers not registered in activateIslands. Filter to only check
  // warnings for the 2 registered islands.
  const registeredWarnings = warnings.filter(w =>
    !w.includes('island_1') && !w.includes('island_2') &&
    !w.includes('island_3') && !w.includes('island_4')
  );
  await expectNoHydrationWarnings(registeredWarnings);
});

// --- Form Interaction ---

test('email input accepts typing', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  await page.fill('#email', 'test@example.com');
  const value = await page.locator('#email').inputValue();
  expect(value).toBe('test@example.com');
});

test('password toggle switches between password and text', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  // Initially type="password"
  const typeBefore = await page.locator('#password').getAttribute('type');
  expect(typeBefore).toBe('password');

  // Click toggle
  await page.locator('.password-toggle').first().click();
  const typeAfter = await page.locator('#password').getAttribute('type');
  expect(typeAfter).toBe('text');

  // Click toggle again — back to password
  await page.locator('.password-toggle').first().click();
  const typeReset = await page.locator('#password').getAttribute('type');
  expect(typeReset).toBe('password');
});

test('submit empty form shows error message', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  // Submit with empty fields
  await page.locator('button[type="submit"]').click();
  await page.waitForTimeout(1000);

  // Error alert should become visible
  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
});

// --- Navigation ---

test('"New here?" link navigates to /platform/onboarding', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  const link = page.locator('a[href="/platform/onboarding"]');
  await expect(link).toBeVisible();
  await link.click();
  await page.waitForURL('**/platform/onboarding');
  expect(page.url()).toContain('/platform/onboarding');
});

// --- Layout ---

test('no layout overflow', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);
  await expectNoLayoutOverflow(page);
});

test('no DOM doubling after hydration', async ({ browser, page }) => {
  // Count SSR elements
  const ssrCtx = await browser.newContext({ javaScriptEnabled: false });
  const ssrPage = await ssrCtx.newPage();
  await ssrPage.goto('/platform/login');
  const ssrCount = await ssrPage.evaluate(() => document.querySelectorAll('*').length);
  await ssrCtx.close();

  // Count after JS hydration
  await page.goto('/platform/login');
  await page.waitForTimeout(2000);
  const jsCount = await page.evaluate(() => document.querySelectorAll('*').length);

  // JS count should not be more than 2x SSR count
  expect(jsCount).toBeLessThan(ssrCount * 2);
});
