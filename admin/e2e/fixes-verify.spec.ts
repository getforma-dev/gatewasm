import { test, expect } from '@playwright/test';
import { BASE, postWithRetry } from './helpers';
import { TenantLogin } from './selectors';

test.describe('Route and cache fixes', () => {

  test('/dev/clear-cache is gone @regression', async ({ page }) => {
    // The route was unauthenticated, shipped in the release binary, and ran
    // `SELECT slug FROM tenants` — handing the full tenant list to any
    // anonymous caller via Set-Cookie headers and expiring every session.
    // Removed; `npm run dev:clean` and `dev:reset` cover the dev workflow.
    const res = await page.goto(BASE + '/dev/clear-cache');
    expect(res?.status()).toBe(404);
  });

  test('dashboard does NOT doom-loop @regression', async ({ page }) => {
    await page.goto(BASE + '/t/platform/dashboard');
    // Wait for the page to settle — should redirect to login, not loop
    await page.waitForURL('**/login', { timeout: 15000 });
    expect(page.url()).toContain('/login');
  });

  test('login page respects mode=register param @regression', async ({ page }) => {
    await page.goto(BASE + '/t/platform/login?mode=register');
    await page.waitForSelector(TenantLogin.card, { timeout: 5000 });
    const body = await page.textContent('body');
    const hasRegister = body?.includes('Create') || body?.includes('Sign up');
    expect(hasRegister).toBeTruthy();
  });

  test('redirect param works for tenant-owner login preview @regression', async ({ page, request }) => {
    const slug = 'redir-test-' + Date.now().toString(36);
    const email = `owner-${Date.now()}@test.com`;

    const signup = await postWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email, password: 'Test1234!', name: 'Owner Test' },
    });
    if (!signup.ok()) {
      throw new Error('Platform signup failed: ' + await signup.text());
    }
    const { access_token } = await signup.json();
    expect(access_token).toBeTruthy();

    const create = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
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
    await page.waitForSelector(TenantLogin.card, { timeout: 5000 });

    const body = await page.textContent('body');
    const hasRegister = body?.includes('Create') || body?.includes('Sign up');
    expect(hasRegister).toBeTruthy();

    await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + access_token, 'Content-Type': 'application/json' },
      data: { confirm_slug: slug },
    });
  });

  test('removed sandbox route now returns 404 @regression', async ({ request }) => {
    const sandbox = await request.get(BASE + '/t/platform/sandbox', { maxRedirects: 0 });
    expect(sandbox.status()).toBe(404);
  });
});
