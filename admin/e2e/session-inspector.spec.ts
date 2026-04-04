import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';

const BASE = process.env.BASE_URL || 'http://localhost:3001';

test('platform login hides owner MFA until required', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  await expect(page.getByText('Owner MFA Verification')).toHaveCount(0);
});

test('session inspector lives inside integrate', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const slug = 'smoke-' + Date.now().toString(36);
  let tenantId = '';

  try {
    const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
      data: { grant_type: 'refresh_token' },
    });
    expect(refresh.ok()).toBeTruthy();
    const refreshJson = await refresh.json();
    const accessToken = refreshJson.access_token as string;

    const createTenant = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
      data: { name: 'Smoke Tenant', slug },
    });
    expect(createTenant.ok()).toBeTruthy();
    const createJson = await createTenant.json();
    tenantId = String(createJson.tenant.id);

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Smoke Tenant' }).first().click();
    await expect(page.locator('.sidebar')).toBeVisible();
    await expect(page.locator('.sidebar').getByText('Auth Sandbox')).toHaveCount(0);

    await page.locator('.sidebar').getByText('Integrate').click();
    await page.waitForTimeout(1000);

    await expect(page.getByRole('tab', { name: 'Getting Started' })).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Session Inspector' })).toBeVisible();

    await page.getByRole('tab', { name: 'Session Inspector' }).click();
    await page.waitForTimeout(1000);

    await expect(page.getByText('Inspect the live auth payload')).toBeVisible();
    await expect(page.getByText('Start tenant sign-in for Smoke Tenant')).toBeVisible();
  } finally {
    if (tenantId) {
      const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
        data: { grant_type: 'refresh_token' },
      });
      if (refresh.ok()) {
        const refreshJson = await refresh.json();
        const accessToken = refreshJson.access_token as string;
        await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { Authorization: 'Bearer ' + accessToken },
          data: { confirm_slug: slug },
        });
      }
    }

    await context.close();
  }
});
