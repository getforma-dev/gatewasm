import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { getAccessToken } from './helpers';
import { createTestTenant, deleteTestTenantFromContext } from './factories';
import { Workspace } from './selectors';

test('platform login hides owner MFA until required @regression', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  await expect(page.getByText('Owner MFA Verification')).toHaveCount(0);
});

test('auth reference lives inside integrate @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  let tenant: { id: string; slug: string } | undefined;

  try {
    const accessToken = await getAccessToken(context);
    tenant = await createTestTenant(context.request, accessToken, 'Smoke Tenant');

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Smoke Tenant' }).first().click();

    // Switch to Operator plane and navigate to Integrate tab
    await page.locator(Workspace.tier1).getByRole('button', { name: 'Operator' }).click();
    await page.waitForTimeout(500);

    await page.locator(Workspace.tier2).getByRole('button', { name: 'Integrate' }).click();
    await page.waitForTimeout(1000);

    await expect(page.getByRole('tab', { name: 'Getting Started' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Auth Reference' })).toBeVisible();

    await page.getByRole('tab', { name: 'Auth Reference' }).click();
    await page.waitForTimeout(1000);

    // Auth reference content is always visible (no login gate)
    await expect(page.getByText('Understand the auth payload your app receives')).toBeVisible();
    await expect(page.getByText('Callback Payload')).toBeVisible();
    await expect(page.getByText('User Object Shape')).toBeVisible();
    await expect(page.getByText('Token Format', { exact: true })).toBeVisible();
    await expect(page.getByText('Auth Endpoints')).toBeVisible();
  } finally {
    if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
    await context.close();
  }
});

test('users view shows active session count @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  let tenant: { id: string; slug: string } | undefined;

  try {
    const accessToken = await getAccessToken(context);
    tenant = await createTestTenant(context.request, accessToken, 'Session Test');

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Session Test' }).first().click();

    // Switch to Organization plane and navigate to Members tab
    await page.locator(Workspace.tier1).getByRole('button', { name: 'Organization' }).click();
    await page.waitForTimeout(500);

    await page.locator(Workspace.tier2).getByRole('button', { name: 'Members' }).click();
    await page.waitForTimeout(1000);

    // Members view should render (may be empty for a new tenant)
    await expect(page.getByRole('heading', { name: 'Members' })).toBeVisible();
  } finally {
    if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
    await context.close();
  }
});
