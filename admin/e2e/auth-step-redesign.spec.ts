import { test, expect, Page } from '@playwright/test';
import { BASE, postWithRetry } from './helpers';

test.describe('Auth step redesign', () => {
  let platformToken: string;
  let platformEmail: string;
  const platformPassword = 'Test1234!';
  let tenantId: string;
  const tenantSlug = 'auth-step-' + Date.now().toString(36);

  test.beforeAll(async ({ request }) => {
    platformEmail = 'auth-step-' + Date.now() + '@test.com';

    // 1. Register platform user
    const signup = await postWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword, name: 'Auth Step E2E' },
    });
    if (!signup.ok()) throw new Error('Platform signup failed: ' + await signup.text());
    platformToken = (await signup.json()).access_token;
    if (!platformToken) throw new Error('No access token from signup');

    // 2. Create tenant
    const create = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { name: 'Auth Step App', slug: tenantSlug },
    });
    if (!create.ok()) throw new Error('Tenant creation failed: ' + await create.text());
    const created = await create.json();
    tenantId = created.id || created.tenant?.id;
    if (!tenantId) throw new Error('No tenant id from create');

    // 3. PATCH tenant into the auth step with intent=internal + signup_mode=invite_only
    const patch = await request.patch(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: {
        tenant_intent: 'internal',
        signup_mode: 'invite_only',
        onboarding_step: 3,
      },
    });
    if (!patch.ok()) throw new Error('Tenant PATCH failed: ' + await patch.text());
  });

  test.afterAll(async ({ request }) => {
    if (!tenantId) return;
    await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { confirm_slug: tenantSlug },
    });
  });

  /**
   * Logs the platform user into a fresh browser context, navigates to the
   * console, opens the test tenant's wizard, and returns the page positioned
   * at the auth step (step 3).
   */
  async function openWizardAtAuthStep(browser: any): Promise<{ page: Page; close: () => Promise<void> }> {
    const context = await browser.newContext();
    const page = await context.newPage();

    // Browser login (sets refresh cookie)
    await context.request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword },
    });

    await page.goto(BASE + '/platform/console');
    // Click into the test tenant card
    await page.locator('.tenant-card').filter({ hasText: 'Auth Step App' }).click();

    // The wizard should mount because onboarding_completed is false.
    // Verify we're on the auth step.
    await expect(page.locator('h1.wizard-headline').filter({ hasText: 'How will users sign in?' })).toBeVisible();

    return {
      page,
      close: async () => { await context.close(); },
    };
  }

  test('credentials group renders Email OTP and Passkey rows with Coming soon badges @regression', async ({ browser }) => {
    const { page, close } = await openWizardAtAuthStep(browser);

    const credentialsGroup = page.locator('.wizard-provider-group').filter({ hasText: 'Credentials' });

    // Email OTP row
    const emailOtpRow = credentialsGroup.locator('.wizard-provider-toggle').filter({ hasText: 'Email OTP' });
    await expect(emailOtpRow).toBeVisible();
    await expect(emailOtpRow.locator('.wizard-provider-subtitle')).toHaveText('6-digit code sent to email');
    await expect(emailOtpRow.locator('.wizard-provider-badge')).toHaveText('Coming soon');
    await expect(emailOtpRow.locator('input[type="checkbox"]')).toBeDisabled();

    // Passkey row
    const passkeyRow = credentialsGroup.locator('.wizard-provider-toggle').filter({ hasText: 'Passkey' });
    await expect(passkeyRow).toBeVisible();
    await expect(passkeyRow.locator('.wizard-provider-subtitle')).toHaveText('Sign in with Face ID, fingerprint, or device PIN');
    await expect(passkeyRow.locator('.wizard-provider-badge')).toHaveText('Coming soon');
    await expect(passkeyRow.locator('input[type="checkbox"]')).toBeDisabled();

    await close();
  });
});
