import { test, expect, Browser, Page } from '@playwright/test';
import { BASE, patchWithRetry, postWithRetry } from './helpers';

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
    const patch = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
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
  async function openWizardAtAuthStep(browser: Browser): Promise<{ page: Page; close: () => Promise<void> }> {
    const context = await browser.newContext();
    const page = await context.newPage();

    // Browser login (sets refresh cookie)
    const loginRes = await context.request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword },
    });
    if (!loginRes.ok()) {
      await context.close();
      throw new Error('Browser login failed: ' + await loginRes.text());
    }

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
    try {
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
    } finally {
      await close();
    }
  });

  test('Pro section renders SMS OTP, SAML/SSO, and Custom OIDC as upgrade-locked items @regression', async ({ browser }) => {
    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      // Scope to wizard-pane-left to stay robust if step rendering strategy changes.
      const proSection = page.locator('.wizard-pane-left .wizard-pro-section');
      await expect(proSection).toBeVisible();

      // Header
      await expect(proSection.locator('.wizard-pro-header .wizard-provider-badge')).toHaveText('Pro');
      await expect(proSection.locator('.wizard-pro-header .wizard-pro-label')).toHaveText('Upgrade to unlock');

      // Three rows in order: SMS OTP, SAML / SSO, Custom OIDC
      const items = proSection.locator('.wizard-pro-item');
      await expect(items).toHaveCount(3);
      await expect(items.nth(0)).toContainText('SMS OTP');
      await expect(items.nth(1)).toContainText('SAML / SSO');
      await expect(items.nth(2)).toContainText('Custom OIDC');

      // All checkboxes disabled
      const checkboxes = proSection.locator('input[type="checkbox"]');
      await expect(checkboxes).toHaveCount(3);
      for (let i = 0; i < 3; i++) {
        await expect(checkboxes.nth(i)).toBeDisabled();
      }
    } finally {
      await close();
    }
  });

  test('Access provider-group renders first with segmented control pre-filled from signup_mode @regression', async ({ browser }) => {
    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      // The Access group must be the FIRST wizard-provider-group inside the left pane
      const firstGroup = page.locator('.wizard-pane-left .wizard-provider-group').first();
      await expect(firstGroup.locator('.wizard-provider-group-label')).toHaveText('Access');

      // Both segmented options visible with their sub-copy
      const openOption = firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Open' });
      const inviteOption = firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Invite-only' });
      await expect(openOption).toBeVisible();
      await expect(inviteOption).toBeVisible();

      await expect(firstGroup).toContainText('Anyone can create an account.');
      await expect(firstGroup).toContainText("Only people you invite can sign up. You'll manage invites from the dashboard.");

      // Pre-fill: tenant was created with signup_mode=invite_only, so Invite-only is active
      await expect(inviteOption).toHaveClass(/active/);
      await expect(openOption).not.toHaveClass(/active/);

      // Click Open and verify the active class moves
      await openOption.click();
      await expect(openOption).toHaveClass(/active/);
      await expect(inviteOption).not.toHaveClass(/active/);
    } finally {
      await close();
    }
  });
});
