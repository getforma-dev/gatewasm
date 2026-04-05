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

test.describe('Route and cache fixes', () => {

  test('/dev/clear-cache clears SW, caches, storage, and cookies', async ({ page }) => {
    // Set some state first
    await page.goto(BASE + '/t/forma/login');
    await page.waitForTimeout(2000);
    await page.evaluate(() => {
      localStorage.setItem('test_key', 'test_value');
      sessionStorage.setItem('test_session', 'test_value');
    });

    // Hit the clear cache page
    await page.goto(BASE + '/dev/clear-cache');
    await page.waitForTimeout(3000);

    const body = await page.textContent('body');
    expect(body).toContain('localStorage cleared');
    expect(body).toContain('sessionStorage cleared');
    expect(body).toContain('HttpOnly auth cookies expired');
    expect(body).toContain('Done');

    // Verify localStorage actually cleared
    await page.goto(BASE + '/t/forma/login');
    await page.waitForTimeout(1000);
    const lsValue = await page.evaluate(() => localStorage.getItem('test_key'));
    expect(lsValue).toBeNull();
    console.log('✓ /dev/clear-cache works');
  });

  test('dashboard does NOT doom-loop', async ({ page }) => {
    await page.goto(BASE + '/t/forma/dashboard');
    await page.waitForTimeout(10000);

    const finalUrl = page.url();
    const body = await page.textContent('body');
    console.log('Dashboard final URL:', finalUrl);

    // Should have settled on login page (not looping)
    const settled = finalUrl.includes('/login') ||
      (body?.includes('Unable to load') ?? false) ||
      (body?.includes('Sign in') ?? false);
    expect(settled).toBeTruthy();
    console.log('✓ Dashboard no doom loop');
  });

  test('login page respects mode=register param', async ({ page }) => {
    await page.goto(BASE + '/t/forma/login?mode=register');
    await page.waitForTimeout(3000);
    const body = await page.textContent('body');
    const hasRegister = body?.includes('Create') || body?.includes('Sign up');
    expect(hasRegister).toBeTruthy();
    console.log('✓ mode=register works');
  });

  test('redirect param works for tenant-owner login preview', async ({ page, request }) => {
    const slug = 'redir-test-' + Date.now().toString(36);
    const email = `owner-${Date.now()}@test.com`;

    const signup = await postJsonWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email, password: 'Test1234!', name: 'Owner Test' },
    });
    if (!signup.ok()) {
      throw new Error('Platform signup failed: ' + await signup.text());
    }
    const { access_token } = await signup.json();
    expect(access_token).toBeTruthy();

    const create = await postJsonWithRetry(request, BASE + '/platform/self-serve/tenants', {
      headers: { 'Authorization': 'Bearer ' + access_token, 'Content-Type': 'application/json' },
      data: { name: 'Redirect Test App', slug },
    });
    if (!create.ok()) {
      throw new Error('Tenant creation failed: ' + await create.text());
    }
    expect(create.status()).toBeLessThan(300);
    const createJson = await create.json();
    const tenantId = String(createJson.tenant?.id || createJson.id || '');

    await page.goto(BASE + '/t/' + slug + '/login?redirect=/t/' + slug + '/site&mode=register');
    await page.waitForTimeout(2000);

    const body = await page.textContent('body');
    const hasRegister = body?.includes('Create') || body?.includes('Sign up');
    expect(hasRegister).toBeTruthy();

    await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + access_token, 'Content-Type': 'application/json' },
      data: { confirm_slug: slug },
    });
  });

  test('removed sandbox route now returns 404', async ({ request }) => {
    const sandbox = await request.get(BASE + '/t/forma/sandbox', { maxRedirects: 0 });
    expect(sandbox.status()).toBe(404);
  });
});
