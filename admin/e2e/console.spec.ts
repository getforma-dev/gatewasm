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

test('authenticated: console loads with account shell and workspaces view', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    // Account mode: renders .account-shell with tabs
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.account-tabs').getByRole('button', { name: 'Tenants' })).toBeVisible();
    await expect(page.getByText('Your Workspaces')).toBeVisible();

    // Avatar button visible in account shell
    await expect(page.locator('.account-shell .plane-avatar')).toBeVisible();
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

test('authenticated: tenant create modal shows current required fields', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    // Open the create workspace modal
    await page.getByRole('button', { name: '+ New Workspace' }).click();
    const modal = page.locator('.modal-content');
    await expect(modal).toBeVisible({ timeout: 5000 });

    await expect(page.getByLabel('App Name')).toBeVisible();
    await expect(page.locator('label').filter({ hasText: 'Tenant Slug' })).toBeVisible();
    await expect(modal.getByRole('button', { name: 'Create Workspace' })).toBeVisible();
  } finally {
    await context.close();
  }
});

test('authenticated: tenant slug preview updates from the current form fields', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    // Open the create workspace modal
    await page.getByRole('button', { name: '+ New Workspace' }).click();
    const modal = page.locator('.modal-content');
    await expect(modal).toBeVisible({ timeout: 5000 });

    await page.getByLabel('App Name').fill('Console Smoke Tenant');
    await expect(page.getByText('/t/console-smoke-tenant')).toBeVisible();

    await page.locator('#field-tenant-slug').fill('custom-slug');
    await expect(page.getByText('/t/custom-slug')).toBeVisible();
    // Wait for slug availability check to resolve (debounced 500ms + network)
    await expect(modal.locator('.slug-status-available, .slug-status-taken, .slug-status-invalid')).toBeVisible({ timeout: 5000 });
    // Verify the submit button is visible inside the modal
    await expect(modal.getByRole('button', { name: 'Create Workspace' })).toBeVisible();
  } finally {
    await context.close();
  }
});

test('authenticated: sign out redirects to login', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  try {
    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    // Click the avatar button to open the account menu
    await page.locator('.account-shell .plane-avatar').click();
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
