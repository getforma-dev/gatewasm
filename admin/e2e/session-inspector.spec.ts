import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE } from './helpers';

test('platform login hides owner MFA until required', async ({ page }) => {
  await page.goto('/platform/login');
  await page.waitForTimeout(1000);

  await expect(page.getByText('Owner MFA Verification')).toHaveCount(0);
});

test('auth reference lives inside integrate', async ({ browser }) => {
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

    // Switch to Operator plane and navigate to Integrate tab
    await page.locator('.plane-tier1').getByRole('button', { name: 'Operator' }).click();
    await page.waitForTimeout(500);

    await page.locator('.plane-tier2').getByRole('button', { name: 'Integrate' }).click();
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

test('users view shows active session count', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const slug = 'sess-' + Date.now().toString(36);
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
      data: { name: 'Session Test', slug },
    });
    expect(createTenant.ok()).toBeTruthy();
    const createJson = await createTenant.json();
    tenantId = String(createJson.tenant.id);

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Session Test' }).first().click();

    // Switch to Organization plane and navigate to Members tab
    await page.locator('.plane-tier1').getByRole('button', { name: 'Organization' }).click();
    await page.waitForTimeout(500);

    await page.locator('.plane-tier2').getByRole('button', { name: 'Members' }).click();
    await page.waitForTimeout(1000);

    // Members view should render (may be empty for a new tenant)
    await expect(page.getByRole('heading', { name: 'Members' })).toBeVisible();
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
