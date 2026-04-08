/**
 * Tenant Login Page — E2E Tests
 *
 * Tests cover island activation, form interaction, mode switching,
 * password toggle, and layout stability. Uses /t/platform/login (the
 * seeded "platform" tenant). Auth flow (actual login, OAuth) is NOT
 * tested here since it requires valid credentials and external services.
 *
 * Note: Tenant login is Phase1ClientMount (CSR), so no SSR content
 * tests — the page is an empty #app shell until JS loads.
 */
import { test, expect } from '@playwright/test';
import {
  collectErrors,
  collectWarnings,
  expectNoConsoleErrors,
  expectNoHydrationWarnings,
  expectNoLayoutOverflow,
} from './helpers';
import { TenantLogin } from './selectors';

const TENANT_LOGIN = '/t/platform/login';

// --- Page Load ---

test('tenant login renders login form after JS loads @smoke @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  // Wait for island activation
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  // Login form visible
  const emailInput = page.locator('input[type="email"]');
  await expect(emailInput).toBeVisible();

  const passwordInput = page.locator('input[type="password"]');
  await expect(passwordInput).toBeVisible();

  const submitBtn = page.locator('button[type="submit"]');
  await expect(submitBtn).toBeVisible();
  await expect(submitBtn).toHaveText('Sign in');
});

test('tenant login: no console errors @smoke @regression', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto(TENANT_LOGIN, { waitUntil: 'networkidle' });
  expectNoConsoleErrors(errors);
});

test('tenant login: no hydration warnings @regression', async ({ page }) => {
  const warnings = collectWarnings(page);
  await page.goto(TENANT_LOGIN, { waitUntil: 'networkidle' });
  expectNoHydrationWarnings(warnings);
});

// --- Branding ---

test('tenant login shows app name @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  const appName = page.locator(TenantLogin.appName);
  await expect(appName).toBeVisible();
  // "forma" tenant name (seeded as "Forma" or slug "forma")
  const text = await appName.textContent();
  expect(text!.length).toBeGreaterThan(0);
});

test('tenant login shows powered-by footer @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  const footer = page.locator(TenantLogin.footer);
  await expect(footer).toBeVisible();
  await expect(footer).toContainText('fAuth');
});

// --- Form Interaction ---

test('tenant login: email input accepts typing @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  const emailInput = page.locator('input[type="email"]');
  await emailInput.fill('test@example.com');
  const value = await emailInput.inputValue();
  expect(value).toBe('test@example.com');
});

test('tenant login: password toggle switches between password and text @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  // Initially type="password"
  const pwField = page.locator(TenantLogin.passwordField);
  const typeBefore = await pwField.getAttribute('type');
  expect(typeBefore).toBe('password');

  // Click toggle
  await page.locator(TenantLogin.passwordToggle).first().click();
  const typeAfter = await pwField.getAttribute('type');
  expect(typeAfter).toBe('text');

  // Click toggle again — back to password
  await page.locator(TenantLogin.passwordToggle).first().click();
  const typeReset = await pwField.getAttribute('type');
  expect(typeReset).toBe('password');
});

// --- Mode Switching ---

test('toggle between login and register modes @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN);
  await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

  // Start in login mode
  const subtitle = page.locator(TenantLogin.subtitle);
  await expect(subtitle).toHaveText('Sign in to your account');

  // Click "Sign up" link
  const toggleLink = page.locator(TenantLogin.toggleLink);
  await toggleLink.click();

  // Should switch to register mode
  await expect(subtitle).toHaveText('Create your account');

  // Name field should appear in register mode
  const nameInput = page.locator('input[type="text"][placeholder="Your name"]');
  await expect(nameInput).toBeVisible();

  // Click "Sign in" to go back
  await toggleLink.click();
  await expect(subtitle).toHaveText('Sign in to your account');
});

// --- Layout ---

test('tenant login: no layout overflow @regression', async ({ page }) => {
  await page.goto(TENANT_LOGIN, { waitUntil: 'networkidle' });
  await expectNoLayoutOverflow(page);
});
