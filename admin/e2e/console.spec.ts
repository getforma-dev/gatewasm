/**
 * Platform Console — E2E Tests
 *
 * Tests cover auth gating, the current tenant-hub console shell, account
 * controls, and layout stability. Uses auth-helper to register a test user
 * and set the refresh cookie before navigating to the console.
 */
import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import {
  collectErrors,
  expectNoConsoleErrors,
  expectNoLayoutOverflow,
} from './helpers';

// --- Auth Gating ---

test('unauthenticated user redirects to /platform/login', async ({ page }) => {
  await page.goto('/platform/console');
  // JS boots, silentRefresh fails, redirects to login
  await page.waitForURL('**/platform/login', { timeout: 5000 });
  expect(page.url()).toContain('/platform/login');
});

// --- Authenticated Tests ---

test('authenticated: console loads with sidebar and tenants view', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await expect(page.locator('.sidebar')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.sidebar').getByText('Tenants')).toBeVisible();
    await expect(page.getByRole('button', { name: /Quick Switch/i })).toBeVisible();
    await expect(page.getByText('Your Tenants')).toBeVisible();
    await expect(page.getByLabel('App Name')).toBeVisible();
    await expect(page.getByLabel('Tenant Slug')).toBeVisible();
    await expect(page.locator('.sidebar-account-trigger')).toBeVisible();
  } finally {
    await context.close();
  }
});

test('authenticated: no console errors', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const errors = collectErrors(page);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(3000);
    await expectNoConsoleErrors(errors);
  } finally {
    await context.close();
  }
});

test('authenticated: tenant create panel shows current required fields', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await expect(page.locator('.tenant-create-panel .card-label')).toContainText(/Create (your first tenant|another tenant)/);
    await expect(page.getByLabel('App Name')).toBeVisible();
    await expect(page.getByLabel('Tenant Slug')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Create Tenant' })).toBeVisible();
  } finally {
    await context.close();
  }
});

test('authenticated: tenant slug preview updates from the current form fields', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.getByLabel('App Name').fill('Console Smoke Tenant');
    await expect(page.getByText('/t/console-smoke-tenant')).toBeVisible();

    await page.getByLabel('Tenant Slug').fill('custom-slug');
    await expect(page.getByText('/t/custom-slug')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Create Tenant' })).toBeEnabled();
  } finally {
    await context.close();
  }
});

test('authenticated: sign out redirects to login', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.sidebar-account-trigger').click();
    await expect(page.getByRole('menuitem', { name: 'Sign Out' })).toBeVisible();
    await page.getByRole('menuitem', { name: 'Sign Out' }).click();
    await page.waitForURL('**/platform/login', { timeout: 5000 });
    expect(page.url()).toContain('/platform/login');
  } finally {
    await context.close();
  }
});

// --- Layout ---

test('authenticated: no layout overflow', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(3000);
    await expectNoLayoutOverflow(page);
  } finally {
    await context.close();
  }
});
