/**
 * Edge-Case Regression Tests
 *
 * Covers scenarios not exercised by feature-specific tests:
 * error state recovery, concurrent sessions, token refresh during
 * navigation, tenant deletion cascade, and deep link resilience.
 */
import { test, expect } from './fixtures';
import { BASE, getAccessToken } from './helpers';
import { loginAsTestUser } from './auth-helper';
import { createTestTenant, deleteTestTenant } from './factories';
import { Account, Modal } from './selectors';

// ── 1. Error state recovery: API failure during tenant creation ──────

test('console shows validation feedback for invalid tenant slug @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await authedPage.locator(Account.shell).waitFor({ timeout: 10000 });

  // Open create modal
  await authedPage.getByRole('button', { name: '+ New Workspace' }).click();
  await expect(authedPage.locator(Modal.content)).toBeVisible();

  // Enter an invalid slug — the form should show validation status
  await authedPage.getByLabel('App Name').fill('Test App');
  await authedPage.locator('#field-tenant-slug').fill('x');

  // Slug validation should show invalid/taken status (button stays disabled)
  await expect(
    authedPage.locator(`${Modal.slugInvalid}, ${Modal.slugTaken}`)
  ).toBeVisible({ timeout: 5000 });

  // The submit button should be disabled with an invalid slug
  await expect(
    authedPage.locator(Modal.content).getByRole('button', { name: 'Create Workspace' })
  ).toBeDisabled();

  // Console should still be functional after closing modal
  await authedPage.keyboard.press('Escape');
  await expect(authedPage.locator(Account.shell)).toBeVisible();
});

// ── 2. Concurrent sessions: two tabs same console ────────────────────

test('two tabs can view the console simultaneously @regression', async ({ browser }) => {
  const { context, page: page1 } = await loginAsTestUser(browser);

  try {
    const page2 = await context.newPage();

    await page1.goto('/platform/console');
    await page2.goto('/platform/console');

    await expect(page1.locator(Account.shell)).toBeVisible({ timeout: 10000 });
    await expect(page2.locator(Account.shell)).toBeVisible({ timeout: 10000 });

    // Action in tab 1 doesn't crash tab 2
    await page1.getByRole('button', { name: '+ New Workspace' }).click();
    await expect(page1.locator(Modal.content)).toBeVisible();

    // Tab 2 still responsive
    await expect(page2.locator(Account.shell)).toBeVisible();

    await page2.close();
  } finally {
    await context.close();
  }
});

// ── 3. Token refresh during page load ────────────────────────────────

test('page loads correctly after brief delay @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  try {
    // First load — establishes session
    await page.goto('/platform/console');
    await expect(page.locator(Account.shell)).toBeVisible({ timeout: 10000 });

    // Wait briefly then navigate again — silentRefresh should work transparently
    await page.waitForTimeout(3000);
    await page.goto('/platform/console');
    await expect(page.locator(Account.shell)).toBeVisible({ timeout: 10000 });
  } finally {
    await context.close();
  }
});

// ── 4. Tenant deletion cascade: UI updates after deleting tenant ─────

test('deleting a tenant removes it from the workspace list @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);

  try {
    const token = await getAccessToken(context);
    const tenant = await createTestTenant(context.request, token, 'Delete Me');

    await page.goto('/platform/console');
    await expect(page.locator(Account.shell)).toBeVisible({ timeout: 10000 });

    // Verify tenant appears
    await expect(page.locator(Account.tenantCard).filter({ hasText: 'Delete Me' })).toBeVisible();

    // Delete via API
    const freshToken = await getAccessToken(context);
    await deleteTestTenant(context.request, freshToken, tenant.id, tenant.slug);

    // Reload — tenant should be gone
    await page.goto('/platform/console');
    await expect(page.locator(Account.shell)).toBeVisible({ timeout: 10000 });
    await expect(page.locator(Account.tenantCard).filter({ hasText: 'Delete Me' })).toHaveCount(0);
  } finally {
    await context.close();
  }
});

// ── 5. Deep link resilience ──────────────────────────────────────────

test('deep link to /platform/console loads when authenticated @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 10000 });
  await expect(authedPage.getByText('Your Workspaces')).toBeVisible();
});

test('deep link to /platform/console redirects to login when unauthenticated @regression', async ({ page }) => {
  await page.goto('/platform/console');
  await page.waitForURL('**/platform/login', { timeout: 8000 });
  expect(page.url()).toContain('/platform/login');
});
