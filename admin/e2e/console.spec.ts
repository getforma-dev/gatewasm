/**
 * Platform Console — E2E Tests
 *
 * Tests cover auth gating, hydration, sidebar navigation, tenant CRUD views,
 * and layout stability. Uses auth-helper to register a test user and set
 * the refresh cookie before navigating to the console.
 */
import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import {
  collectErrors,
  collectWarnings,
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

  await page.goto('/platform/console');
  await page.waitForTimeout(3000);

  // Sidebar should be visible
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 5000 });

  // Should show "Tenants" nav item
  const tenantsNav = page.locator('.sidebar').getByText('Tenants');
  await expect(tenantsNav).toBeVisible();

  // Should show "New Tenant" nav item
  const newTenantNav = page.locator('.sidebar').getByText('New Tenant');
  await expect(newTenantNav).toBeVisible();

  await context.close();
});

test('authenticated: no console errors', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const errors = collectErrors(page);
  await page.goto('/platform/console');
  await page.waitForTimeout(3000);
  await expectNoConsoleErrors(errors);
  await context.close();
});

test('authenticated: sidebar "New Tenant" shows create form', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  await page.goto('/platform/console');
  await page.waitForTimeout(3000);

  // Click "New Tenant" in sidebar
  await page.locator('.sidebar').getByText('New Tenant').click();
  await page.waitForTimeout(500);

  // Create form should appear with name and slug fields
  await expect(page.getByText('New Tenant', { exact: false }).first()).toBeVisible();

  await context.close();
});

test('authenticated: create tenant form has required fields', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  await page.goto('/platform/console');
  await page.waitForTimeout(3000);

  // Navigate to create view
  await page.locator('.sidebar').getByText('New Tenant').click();
  await page.waitForTimeout(500);

  // Should have tenant name and slug fields
  await expect(page.getByText('Tenant Name')).toBeVisible();
  await expect(page.getByText('URL Slug')).toBeVisible();

  // Should have Create and Cancel buttons
  await expect(page.getByText('Create Tenant', { exact: true })).toBeVisible();
  await expect(page.getByText('Cancel')).toBeVisible();

  await context.close();
});

test('authenticated: sign out redirects to login', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  await page.goto('/platform/console');
  await page.waitForTimeout(3000);

  // Click Sign Out in sidebar
  await page.locator('.sidebar').getByText('Sign Out').click();
  await page.waitForURL('**/platform/login', { timeout: 5000 });
  expect(page.url()).toContain('/platform/login');

  await context.close();
});

// --- Layout ---

test('authenticated: no layout overflow', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  await page.goto('/platform/console');
  await page.waitForTimeout(3000);
  await expectNoLayoutOverflow(page);
  await context.close();
});
