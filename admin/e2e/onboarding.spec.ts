/**
 * Platform Onboarding Page — E2E Tests
 *
 * Tests cover SSR correctness, hydration integrity, form interactions,
 * and layout stability. Actual registration (POST /auth/register) is NOT
 * tested here — it requires seeded credentials.
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

test('SSR renders onboarding page with JS disabled', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/onboarding');

  // Brand header
  await expect(page.locator('.brand-header')).toBeVisible();

  // Hero section
  await expect(page.locator('.hero-section')).toBeVisible();
  const h1 = await page.locator('.hero-heading').textContent();
  expect(h1).toContain('Auth infrastructure');

  // Capabilities grid — 4 cards
  const capCards = await page.locator('.cap-card').count();
  expect(capCards).toBe(4);

  // Auth card with heading
  await expect(page.locator('.card-heading')).toBeVisible();

  // Footer
  await expect(page.locator('.page-footer')).toBeVisible();

  await context.close();
});

test('SSR: password fields have type="password" (no leak)', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/onboarding');

  const pwType = await page.locator('#reg-password').getAttribute('type');
  expect(pwType).toBe('password');

  const confirmType = await page.locator('#reg-confirm').getAttribute('type');
  expect(confirmType).toBe('password');

  await context.close();
});

// --- Hydration Integrity ---

test('initial hydration: no console errors', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(2000);
  await expectNoConsoleErrors(errors);
});

test('initial hydration: no FormaJS warnings for registered islands', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(2000);
  // Sub-islands (island_1, etc.) from inline createShow emit expected warnings.
  const registeredWarnings = warnings.filter(w =>
    !w.includes('island_1') && !w.includes('island_2') &&
    !w.includes('island_3') && !w.includes('island_4')
  );
  await expectNoHydrationWarnings(registeredWarnings);
});

// --- Form Interaction ---

test('email input accepts typing', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  await page.fill('#reg-email', 'test@example.com');
  const value = await page.locator('#reg-email').inputValue();
  expect(value).toBe('test@example.com');
});

test('password toggle switches between password and text', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  // Initially type="password"
  const typeBefore = await page.locator('#reg-password').getAttribute('type');
  expect(typeBefore).toBe('password');

  // Click the toggle button adjacent to the password input
  const pwWrap = page.locator('#reg-password').locator('..');
  await pwWrap.locator('.password-toggle').click();
  await page.waitForTimeout(200);

  const typeAfter = await page.locator('#reg-password').getAttribute('type');
  expect(typeAfter).toBe('text');
});

test('submit empty form shows email required error', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  await page.locator('button[type="submit"]').click();
  await page.waitForTimeout(500);

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('Email is required');
});

test('submit with short password shows validation error', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  await page.fill('#reg-email', 'test@example.com');
  await page.fill('#reg-password', 'short');
  await page.fill('#reg-confirm', 'short');
  await page.locator('button[type="submit"]').click();
  await page.waitForTimeout(500);

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('at least 8 characters');
});

test('submit with mismatched passwords shows validation error', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  await page.fill('#reg-email', 'test@example.com');
  await page.fill('#reg-password', 'password123');
  await page.fill('#reg-confirm', 'different123');
  await page.locator('button[type="submit"]').click();
  await page.waitForTimeout(500);

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('do not match');
});

// --- Navigation ---

test('sign-in link navigates to /platform/login', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  const link = page.locator('a[href="/platform/login"]');
  await expect(link).toBeVisible();
  await link.click();
  await page.waitForURL('**/platform/login');
  expect(page.url()).toContain('/platform/login');
});

test('CTA links to auth section', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);

  // CTA is an anchor to #auth-section
  const cta = page.locator('.cta-btn');
  await expect(cta).toBeVisible();
  const href = await cta.getAttribute('href');
  expect(href).toBe('#auth-section');
});

// --- Layout ---

test('no layout overflow', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.waitForTimeout(1000);
  await expectNoLayoutOverflow(page);
});

test('no DOM doubling after hydration', async ({ browser, page }) => {
  const ssrCtx = await browser.newContext({ javaScriptEnabled: false });
  const ssrPage = await ssrCtx.newPage();
  await ssrPage.goto('/platform/onboarding');
  const ssrCount = await ssrPage.evaluate(() => document.querySelectorAll('*').length);
  await ssrCtx.close();

  await page.goto('/platform/onboarding');
  await page.waitForTimeout(2000);
  const jsCount = await page.evaluate(() => document.querySelectorAll('*').length);

  expect(jsCount).toBeLessThan(ssrCount * 2);
});
