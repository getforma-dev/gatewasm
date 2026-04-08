/**
 * Platform Onboarding Page — E2E Tests
 *
 * Tests cover boot-shell correctness, hydration integrity, form interactions,
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

// --- Boot Shell Correctness (JS disabled) ---

test('onboarding page serves the boot shell with JS disabled @smoke @regression', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/onboarding');

  const shell = await page.evaluate(() => ({
    hasAppRoot: document.getElementById('app') !== null,
    hasStylesheet: document.querySelector('link[rel="stylesheet"][href*="forma-platform"]') !== null,
    hasModuleEntry: document.querySelector('script[type="module"][src*="platform-onboarding"]') !== null,
    hasPlatformConfig: Array.from(document.scripts).some((script) =>
      (script.textContent || '').includes('__PLATFORM_CONFIG__')
    ),
  }));
  expect(shell.hasAppRoot).toBe(true);
  expect(shell.hasStylesheet).toBe(true);
  expect(shell.hasModuleEntry).toBe(true);
  expect(shell.hasPlatformConfig).toBe(true);

  await context.close();
});

test('password fields stay masked after the onboarding UI hydrates @smoke @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');

  const pwType = await page.locator('#reg-password').getAttribute('type');
  expect(pwType).toBe('password');

  const confirmType = await page.locator('#reg-confirm').getAttribute('type');
  expect(confirmType).toBe('password');
});

// --- Hydration Integrity ---

test('onboarding: no console errors after hydration @smoke @regression', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/onboarding', { waitUntil: 'networkidle' });
  expectNoConsoleErrors(errors);
});

test('onboarding: no FormaJS warnings for registered islands @regression', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto('/platform/onboarding', { waitUntil: 'networkidle' });
  // Sub-islands (island_1, etc.) from inline createShow emit expected warnings.
  const registeredWarnings = warnings.filter(w =>
    !w.includes('island_1') && !w.includes('island_2') &&
    !w.includes('island_3') && !w.includes('island_4')
  );
  expectNoHydrationWarnings(registeredWarnings);
});

// --- Form Interaction ---

test('onboarding: email input accepts typing @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('#reg-email').waitFor();

  await page.fill('#reg-email', 'test@example.com');
  const value = await page.locator('#reg-email').inputValue();
  expect(value).toBe('test@example.com');
});

test('onboarding: password toggle switches between password and text @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('#reg-password').waitFor();

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

test('onboarding: submit empty form shows email required error @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('button[type="submit"]').waitFor();

  await page.locator('button[type="submit"]').click();

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('Email is required');
});

test('onboarding: submit with short password shows validation error @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('#reg-email').waitFor();

  await page.fill('#reg-email', 'test@example.com');
  await page.fill('#reg-password', 'short');
  await page.fill('#reg-confirm', 'short');
  await page.locator('button[type="submit"]').click();

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('at least 8 characters');
});

test('onboarding: submit with mismatched passwords shows validation error @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('#reg-email').waitFor();

  await page.fill('#reg-email', 'test@example.com');
  await page.fill('#reg-password', 'password123');
  await page.fill('#reg-confirm', 'different123');
  await page.locator('button[type="submit"]').click();

  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
  const text = await errorAlert.textContent();
  expect(text).toContain('do not match');
});

// --- Navigation ---

test('sign-in link navigates to /platform/login @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('a[href="/platform/login"]').waitFor();

  const link = page.locator('a[href="/platform/login"]');
  await expect(link).toBeVisible();
  await link.click();
  await page.waitForURL('**/platform/login');
  expect(page.url()).toContain('/platform/login');
});

test('CTA links to auth section @regression', async ({ page }) => {
  await page.goto('/platform/onboarding');
  await page.locator('.cta-btn').waitFor();

  // CTA is an anchor to #auth-section
  const cta = page.locator('.cta-btn');
  await expect(cta).toBeVisible();
  const href = await cta.getAttribute('href');
  expect(href).toBe('#auth-section');
});

// --- Layout ---

test('onboarding: no layout overflow @regression', async ({ page }) => {
  await page.goto('/platform/onboarding', { waitUntil: 'networkidle' });
  await expectNoLayoutOverflow(page);
});

test('onboarding: no DOM doubling after hydration @regression', async ({ browser, page }) => {
  await page.goto('/platform/onboarding', { waitUntil: 'networkidle' });
  await expect(page.locator('#reg-email')).toHaveCount(1);
  await expect(page.locator('#reg-password')).toHaveCount(1);
  await expect(page.locator('#reg-confirm')).toHaveCount(1);
});
