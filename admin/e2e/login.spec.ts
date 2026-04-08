/**
 * Platform Login Page — E2E Tests
 *
 * Tests cover boot-shell correctness, hydration integrity, interactive behavior,
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

// --- Boot Shell Correctness (JS disabled) ---

test('login page serves the boot shell with JS disabled @smoke @regression', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/platform/login');

  const shell = await page.evaluate(() => ({
    hasAppRoot: document.getElementById('app') !== null,
    hasStylesheet: document.querySelector('link[rel="stylesheet"][href*="forma-platform"]') !== null,
    hasModuleEntry: document.querySelector('script[type="module"][src*="platform-login"]') !== null,
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

test('password field is masked once the login UI hydrates @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login');
  const passwordType = await page.locator('#password').getAttribute('type');
  expect(passwordType).toBe('password');
});

// --- Hydration Integrity ---

test('login: no console errors after hydration @smoke @regression', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/login', { waitUntil: 'networkidle' });
  expectNoConsoleErrors(errors);
});

test('login: no FormaJS warnings for registered islands @smoke @regression', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto('/platform/login', { waitUntil: 'networkidle' });
  // Sub-islands (island_1, island_2, etc.) emit expected warnings since they are
  // inline helpers not registered in activateIslands. Filter to only check
  // warnings for the 2 registered islands.
  const registeredWarnings = warnings.filter(w =>
    !w.includes('island_1') && !w.includes('island_2') &&
    !w.includes('island_3') && !w.includes('island_4')
  );
  expectNoHydrationWarnings(registeredWarnings);
});

// --- Form Interaction ---

test('login: email input accepts typing @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login');
  await page.locator('#email').waitFor();

  await page.fill('#email', 'test@example.com');
  const value = await page.locator('#email').inputValue();
  expect(value).toBe('test@example.com');
});

test('login: password toggle switches between password and text @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login');
  await page.locator('#password').waitFor();

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

test('login: submit empty form shows error message @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login');
  await page.locator('button[type="submit"]').waitFor();

  // Submit with empty fields
  await page.locator('button[type="submit"]').click();

  // Error alert should become visible
  const errorAlert = page.locator('.alert.error.show');
  await expect(errorAlert).toBeVisible({ timeout: 3000 });
});

// --- Navigation ---

test('"New here?" link navigates to /platform/onboarding @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login');
  await page.locator('a[href="/platform/onboarding"]').waitFor();

  const link = page.locator('a[href="/platform/onboarding"]');
  await expect(link).toBeVisible();
  await link.click();
  await page.waitForURL('**/platform/onboarding');
  expect(page.url()).toContain('/platform/onboarding');
});

// --- Layout ---

test('login: no layout overflow @smoke @regression', async ({ page }) => {
  await page.goto('/platform/login', { waitUntil: 'networkidle' });
  await expectNoLayoutOverflow(page);
});

test('login: no DOM doubling after hydration @smoke @regression', async ({ browser, page }) => {
  await page.goto('/platform/login', { waitUntil: 'networkidle' });
  await expect(page.locator('#email')).toHaveCount(1);
  await expect(page.locator('#password')).toHaveCount(1);
  await expect(page.locator('.card')).toHaveCount(1);
});
