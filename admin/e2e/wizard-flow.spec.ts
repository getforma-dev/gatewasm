import { test, expect } from '@playwright/test';

const BASE = process.env.BASE_URL || 'http://localhost:3001';

async function postJsonWithRetry(request: any, url: string, options: any, attempts = 4) {
  let response: any;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    response = await request.post(url, options);
    if (response.status() !== 429) {
      return response;
    }
    const retryAfter = Number(response.headers()['retry-after'] || '1');
    await new Promise((resolve) => setTimeout(resolve, Math.max(500, retryAfter * 1000)));
  }
  return response;
}

test.describe('Tenant workspace lifecycle', () => {

  let platformToken: string;
  let platformEmail: string;
  const platformPassword = 'Test1234!';
  let tenantId: string;
  let tenantUserEmail: string;
  const tenantSlug = 'e2e-workspace-' + Date.now().toString(36);

  test.beforeAll(async ({ request }) => {
    platformEmail = `e2e-${Date.now()}@test.com`;

    // Register platform account via API
    const signup = await postJsonWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword, name: 'E2E User' },
    });
    if (!signup.ok()) {
      throw new Error('Platform signup failed: ' + await signup.text());
    }
    const signupData = await signup.json();
    platformToken = signupData.access_token;
    if (!platformToken) throw new Error('Platform signup failed: ' + JSON.stringify(signupData));

    // Create tenant via API
    const create = await postJsonWithRetry(request, BASE + '/platform/self-serve/tenants', {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { name: 'E2E Workspace App', slug: tenantSlug },
    });
    if (!create.ok()) {
      throw new Error('Tenant creation failed: ' + await create.text());
    }
    const tenantData = await create.json();
    tenantId = tenantData.id || tenantData.tenant?.id;
    if (!tenantId) throw new Error('Tenant creation failed: ' + JSON.stringify(tenantData));
  });

  test.afterAll(async ({ request }) => {
    if (!tenantId) {
      return;
    }

    await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { confirm_slug: tenantSlug },
    });
  });

  test('platform login now lands in the console workspace instead of the removed wizard', async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();

    await context.request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword },
    });

    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(2000);

    await expect(page.getByText('Make it yours')).toHaveCount(0);
    await expect(page.getByText('Your Tenants')).toBeVisible();
    await expect(page.locator('.tenant-card').filter({ hasText: 'E2E Workspace App' })).toBeVisible();

    await context.close();
  });

  test('registering a tenant user updates workspace stats', async ({ request }) => {
    // Register a user on the tenant via API
    tenantUserEmail = 'testuser-' + Date.now().toString(36) + '@e2e.com';
    const reg = await postJsonWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: {
        email: tenantUserEmail,
        password: 'Test1234!',
        name: 'Test User',
      },
    });
    expect(reg.status()).toBeLessThan(300);
    const regData = await reg.json();
    console.log('User registered:', regData.user_id ? 'yes' : 'check response');

    // Verify stats show the user
    const stats = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId + '/stats', {
      headers: { 'Authorization': 'Bearer ' + platformToken },
    });
    const statsData = await stats.json();
    console.log('Stats:', { total_users: statsData.total_users, latest_user: statsData.latest_user });
    expect(statsData.total_users).toBeGreaterThanOrEqual(1);
    expect(statsData.latest_user).toBeTruthy();
    expect(statsData.latest_user.email).toBe(tenantUserEmail);
  });

  test('removed tenant sandbox route now returns 404', async ({ request }) => {
    const resp = await request.get(BASE + '/t/' + tenantSlug + '/sandbox', {
      maxRedirects: 0,
    });
    expect(resp.status()).toBe(404);
  });
});
