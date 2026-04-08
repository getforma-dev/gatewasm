import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { getAccessToken } from './helpers';
import { createTestTenant, deleteTestTenantFromContext } from './factories';
import { Workspace } from './selectors';

test('quick switcher opens from keyboard and switches tenants @regression', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const createdTenants: { id: string; slug: string }[] = [];

  try {
    const accessToken = await getAccessToken(context);

    const tenantA = await createTestTenant(context.request, accessToken, 'Palette Alpha');
    createdTenants.push(tenantA);
    const tenantB = await createTestTenant(context.request, accessToken, 'Palette Beta');
    createdTenants.push(tenantB);

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Palette Alpha' }).first().click();
    await expect(page.getByRole('heading', { name: 'Palette Alpha' })).toBeVisible();

    await page.keyboard.press('Control+K');
    await expect(page.locator('.command-palette-input')).toBeVisible();
    await page.locator('.command-palette-input').fill('members');
    await page.keyboard.press('Enter');

    await expect(page.getByRole('heading', { name: 'Members' })).toBeVisible();

    await page.keyboard.press('Control+K');
    await expect(page.locator('.command-palette-input')).toBeVisible();
    await page.locator('.command-palette-input').fill(tenantB.slug);
    await page.locator('.command-palette-item').filter({ hasText: 'Palette Beta' }).first().click();

    await expect(page.getByRole('heading', { name: 'Palette Beta' })).toBeVisible();
    await expect(page.locator(Workspace.tenantName)).toContainText('Palette Beta');
  } finally {
    for (const tenant of createdTenants) {
      await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
    }

    await context.close();
  }
});
