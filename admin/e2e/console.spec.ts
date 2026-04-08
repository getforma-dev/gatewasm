/**
 * Platform Console — E2E Tests
 *
 * Tests cover auth gating, the current tenant-hub console shell, account
 * controls, and layout stability. Uses auth-helper to register a test user
 * and set the refresh cookie before navigating to the console.
 */
import { test, expect } from './fixtures';
import {
  collectErrors,
  expectNoConsoleErrors,
  expectNoLayoutOverflow,
} from './helpers';
import { Account, Workspace, Modal } from './selectors';

// --- Auth Gating ---

test('unauthenticated user redirects to /platform/login @smoke @regression', async ({ page }) => {
  await page.goto('/platform/console');
  // JS boots, silentRefresh fails, redirects to login
  await page.waitForURL('**/platform/login', { timeout: 5000 });
  expect(page.url()).toContain('/platform/login');
});

// --- Authenticated Tests ---

test('authenticated: console loads with account shell and workspaces view @smoke @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');

  // Account mode: renders .account-shell with tabs
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 8000 });
  await expect(authedPage.locator(Account.tabs).getByRole('button', { name: 'Tenants' })).toBeVisible();
  await expect(authedPage.getByText('Your Workspaces')).toBeVisible();

  // Avatar button visible in account shell
  await expect(authedPage.locator(`${Account.shell} ${Workspace.avatar}`)).toBeVisible();
});

test('authenticated: no console errors @smoke @regression', async ({ authedPage }) => {
  const errors = collectErrors(authedPage);
  await authedPage.goto('/platform/console', { waitUntil: 'networkidle' });
  expectNoConsoleErrors(errors);
});

test('authenticated: tenant create modal shows current required fields @smoke @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 8000 });

  // Open the create workspace modal
  await authedPage.getByRole('button', { name: '+ New Workspace' }).click();
  const modal = authedPage.locator(Modal.content);
  await expect(modal).toBeVisible({ timeout: 5000 });

  await expect(authedPage.getByLabel('App Name')).toBeVisible();
  await expect(authedPage.locator('label').filter({ hasText: 'Tenant Slug' })).toBeVisible();
  await expect(modal.getByRole('button', { name: 'Create Workspace' })).toBeVisible();
});

test('authenticated: tenant slug preview updates from the current form fields @smoke @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 8000 });

  // Open the create workspace modal
  await authedPage.getByRole('button', { name: '+ New Workspace' }).click();
  const modal = authedPage.locator(Modal.content);
  await expect(modal).toBeVisible({ timeout: 5000 });

  await authedPage.getByLabel('App Name').fill('Console Smoke Tenant');
  await expect(authedPage.getByText('/t/console-smoke-tenant')).toBeVisible();

  await authedPage.locator('#field-tenant-slug').fill('custom-slug');
  await expect(authedPage.getByText('/t/custom-slug')).toBeVisible();
  // Wait for slug availability check to resolve (debounced 500ms + network)
  await expect(modal.locator(`${Modal.slugAvailable}, ${Modal.slugTaken}, ${Modal.slugInvalid}`)).toBeVisible({ timeout: 5000 });
  // Verify the submit button is visible inside the modal
  await expect(modal.getByRole('button', { name: 'Create Workspace' })).toBeVisible();
});

test('authenticated: sign out redirects to login @smoke @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 8000 });

  // Click the avatar button to open the account menu
  await authedPage.locator(`${Account.shell} ${Workspace.avatar}`).click();
  await expect(authedPage.getByRole('menuitem', { name: 'Sign Out' })).toBeVisible();
  await authedPage.getByRole('menuitem', { name: 'Sign Out' }).click();
  await authedPage.waitForURL('**/platform/login', { timeout: 5000 });
  expect(authedPage.url()).toContain('/platform/login');
});

// --- Layout ---

test('authenticated: no layout overflow @smoke @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console', { waitUntil: 'networkidle' });
  await expectNoLayoutOverflow(authedPage);
});
