import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE } from './helpers';

async function createAccessToken(context: Awaited<ReturnType<typeof loginAsTestUser>>['context']) {
  const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
    data: { grant_type: 'refresh_token' },
  });
  expect(refresh.ok()).toBeTruthy();
  const refreshJson = await refresh.json();
  return refreshJson.access_token as string;
}

test('overview shows analytics filters and resilient empty states', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const slug = 'overview-' + Date.now().toString(36);
  let tenantId = '';

  try {
    const accessToken = await createAccessToken(context);
    const createTenant = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
      data: { name: 'Overview Tenant', slug },
    });
    expect(createTenant.ok()).toBeTruthy();
    const createJson = await createTenant.json();
    tenantId = String(createJson.tenant.id);

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Overview Tenant' }).first().click();

    await expect(page.getByRole('tab', { name: '7d' })).toBeVisible();
    await expect(page.getByText('No auth traffic yet')).toBeVisible();
    await expect(page.getByText('No activity in this window')).toBeVisible();
    await expect(page.getByText('Failed Sign-Ins (7d)')).toBeVisible();

    await page.getByRole('tab', { name: '30d' }).click();
    await expect(page.getByText('Failed Sign-Ins (30d)')).toBeVisible();

    await page.getByRole('tab', { name: '24h' }).click();
    await expect(page.getByText('Failed Sign-Ins (24h)')).toBeVisible();
  } finally {
    if (tenantId) {
      const cleanupToken = await createAccessToken(context);
      await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
        headers: { Authorization: 'Bearer ' + cleanupToken },
        data: { confirm_slug: slug },
      });
    }

    await context.close();
  }
});
