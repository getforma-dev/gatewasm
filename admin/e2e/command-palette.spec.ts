import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE, getAccessToken } from './helpers';

test('quick switcher opens from keyboard and switches tenants', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const slugA = 'palette-a-' + Date.now().toString(36);
  const slugB = 'palette-b-' + Date.now().toString(36);
  const createdTenants: { id: string; slug: string }[] = [];

  try {
    const accessToken = await getAccessToken(context);

    for (const tenant of [
      { name: 'Palette Alpha', slug: slugA },
      { name: 'Palette Beta', slug: slugB },
    ]) {
      const createTenant = await context.request.post(BASE + '/platform/self-serve/tenants', {
        headers: { Authorization: 'Bearer ' + accessToken },
        data: tenant,
      });
      expect(createTenant.ok()).toBeTruthy();
      const createJson = await createTenant.json();
      createdTenants.push({ id: String(createJson.tenant.id), slug: tenant.slug });
    }

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
    await page.locator('.command-palette-input').fill(slugB);
    await page.locator('.command-palette-item').filter({ hasText: 'Palette Beta' }).first().click();

    await expect(page.getByRole('heading', { name: 'Palette Beta' })).toBeVisible();
    await expect(page.locator('.plane-tenant-name')).toContainText('Palette Beta');
  } finally {
    const cleanupToken = await getAccessToken(context);
    for (const tenant of createdTenants) {
      await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenant.id, {
        headers: { Authorization: 'Bearer ' + cleanupToken },
        data: { confirm_slug: tenant.slug },
      });
    }

    await context.close();
  }
});
