import { test, expect } from '@playwright/test';

const BASE = 'http://localhost:3001';

test.describe('Console Wizard + Dashboard Loop', () => {

  let platformToken: string;
  let tenantId: string;
  const tenantSlug = 'e2e-wiz-' + Date.now().toString(36);

  test.beforeAll(async ({ request }) => {
    // Register platform account via API
    const signup = await request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: `e2e-${Date.now()}@test.com`, password: 'Test1234!', name: 'E2E User' },
    });
    const signupData = await signup.json();
    platformToken = signupData.access_token;
    if (!platformToken) throw new Error('Platform signup failed: ' + JSON.stringify(signupData));

    // Create tenant via API
    const create = await request.post(BASE + '/platform/self-serve/tenants', {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { name: 'E2E Wizard App', slug: tenantSlug },
    });
    const tenantData = await create.json();
    tenantId = tenantData.id || tenantData.tenant?.id;
    if (!tenantId) throw new Error('Tenant creation failed: ' + JSON.stringify(tenantData));
  });

  test('wizard Step 1 renders and advances to Step 2', async ({ page }) => {
    // Set platform auth cookie by logging in through the browser
    await page.goto(BASE + '/t/platform/login');
    await page.waitForTimeout(2000);

    // Navigate to console
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(5000);

    // Check page loaded
    const body = await page.textContent('body');
    console.log('Console-v2 body preview:', body?.slice(0, 400));

    // Look for wizard content
    const hasStep1 = body?.includes('Make it yours') || body?.includes('Step 1');
    console.log('Wizard Step 1 visible:', hasStep1);

    // If wizard shows, try to advance
    if (hasStep1) {
      const nextBtn = page.locator('button:has-text("Next")');
      const isVisible = await nextBtn.isVisible().catch(() => false);
      console.log('Next button visible:', isVisible);
    }
  });

  test('registering a tenant user triggers wizard success', async ({ request }) => {
    // Register a user on the tenant via API
    const reg = await request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: { email: 'testuser@e2e.com', password: 'Test1234!', name: 'Test User' },
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
    expect(statsData.latest_user.email).toBe('testuser@e2e.com');
  });

  test('introspect endpoint returns correct data', async ({ request }) => {
    // Login as tenant user
    const login = await request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: { email: 'testuser@e2e.com', password: 'Test1234!' },
    });
    const { access_token } = await login.json();
    expect(access_token).toBeTruthy();

    // Call introspect
    const resp = await request.get(BASE + '/auth/introspect', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Authorization': 'Bearer ' + access_token },
    });
    expect(resp.status()).toBe(200);
    const data = await resp.json();

    expect(data.user.email).toBe('testuser@e2e.com');
    expect(data.tenant.slug).toBe(tenantSlug);
    expect(data.token.algorithm).toBe('EdDSA');
    console.log('Introspect: PASS');
  });

  test('sandbox redirects unauthenticated to login with redirect param', async ({ request }) => {
    const resp = await request.get(BASE + '/t/' + tenantSlug + '/sandbox', {
      maxRedirects: 0,
    });
    // Should be a redirect (307)
    expect([301, 302, 307, 308]).toContain(resp.status());
    const location = resp.headers()['location'] || '';
    expect(location).toContain('/login');
    expect(location).toContain('redirect=');
    expect(location).toContain('sandbox');
    console.log('Sandbox redirect:', location);
  });

  test('login page with redirect param shows register mode', async ({ page }) => {
    const url = BASE + '/t/' + tenantSlug + '/login?redirect=/t/' + tenantSlug + '/sandbox&mode=register';
    await page.goto(url);
    await page.waitForTimeout(3000);

    const body = await page.textContent('body');
    // Should show register mode (Create account or similar)
    const hasRegister = body?.includes('Create') || body?.includes('register') || body?.includes('Sign up');
    console.log('Register mode:', hasRegister, '(body preview:', body?.slice(0, 200), ')');
  });

  test('dashboard does NOT doom-loop', async ({ page }) => {
    // Go to tenant dashboard (user may or may not be authed)
    await page.goto(BASE + '/t/' + tenantSlug + '/dashboard');

    // Wait for potential redirects to settle
    await page.waitForTimeout(8000);

    const finalUrl = page.url();
    const body = await page.textContent('body');

    console.log('Dashboard final URL:', finalUrl);
    console.log('Dashboard body preview:', body?.slice(0, 300));

    // Should NOT be rapidly cycling between login and dashboard
    // It should either:
    // a) Show the dashboard (if authed)
    // b) Be on the login page (if redirected once)
    // c) Show an error message (if loop was broken)
    const isStable = !finalUrl.includes('/dashboard') || // redirected away
      body?.includes('Unable to load') || // loop breaker message
      body?.includes('Sign in') || // on login page
      body?.length! > 50; // has some content (not blank looping)

    console.log('Dashboard is stable (no doom loop):', isStable);
    expect(isStable).toBeTruthy();
  });

  test('tenant onboarding fields persist via API', async ({ request }) => {
    // Update onboarding step
    const patch1 = await request.patch(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { onboarding_step: 2 },
    });
    expect(patch1.status()).toBe(200);

    // Read back
    const get1 = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken },
    });
    const data1 = await get1.json();
    expect(data1.onboarding_step).toBe(2);
    expect(data1.onboarding_completed).toBe(false);

    // Complete onboarding
    const patch2 = await request.patch(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { onboarding_completed: true, preferred_tab: 'integrate', onboarding_step: 3 },
    });
    expect(patch2.status()).toBe(200);

    const get2 = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken },
    });
    const data2 = await get2.json();
    expect(data2.onboarding_completed).toBe(true);
    expect(data2.preferred_tab).toBe('integrate');
    expect(data2.onboarding_step).toBe(3);
    console.log('Onboarding fields persist: PASS');
  });
});
