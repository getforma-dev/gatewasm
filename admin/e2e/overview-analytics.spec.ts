import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE, getAccessToken } from './helpers';

test('overview shows analytics filters and resilient empty states', async ({ browser }) => {
  test.setTimeout(45000);
  const { context, page } = await loginAsTestUser(browser);
  const slug = 'overview-' + Date.now().toString(36);
  let tenantId = '';

  try {
    const accessToken = await getAccessToken(context);
    const createTenant = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
      data: { name: 'Overview Tenant', slug },
    });
    expect(createTenant.ok()).toBeTruthy();
    const createJson = await createTenant.json();
    tenantId = String(createJson.tenant.id);

    // Navigate to console — silentRefresh may be rate-limited during full suite runs
    await page.goto('/platform/console');
    // If the client-side silentRefresh was rate-limited, retry with a pause
    for (let attempt = 0; attempt < 3; attempt++) {
      await page.waitForLoadState('networkidle');
      if (!page.url().includes('/login')) break;
      await page.waitForTimeout(1500);
      await page.goto('/platform/console');
    }
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 10000 });

    await page.locator('.tenant-card').filter({ hasText: 'Overview Tenant' }).first().click();
    await expect(page.locator('.plane-nav')).toBeVisible({ timeout: 8000 });

    await expect(page.getByRole('tab', { name: '7d' })).toBeVisible();
    await expect(page.getByText('No auth traffic yet')).toBeVisible();
    // Activity feed card renders (empty state or data) — wait up to 10s for stats to load
    await expect(page.getByText('What changed most recently')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('Failed Sign-Ins (7d)')).toBeVisible();

    await page.getByRole('tab', { name: '30d' }).click();
    await expect(page.getByText('Failed Sign-Ins (30d)')).toBeVisible();

    await page.getByRole('tab', { name: '24h' }).click();
    await expect(page.getByText('Failed Sign-Ins (24h)')).toBeVisible();
  } finally {
    if (tenantId) {
      try {
        const cleanupToken = await getAccessToken(context);
        await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { Authorization: 'Bearer ' + cleanupToken },
          data: { confirm_slug: slug },
        });
      } catch { /* cleanup best-effort — context may be destroyed on timeout */ }
    }

    await context.close();
  }
});
