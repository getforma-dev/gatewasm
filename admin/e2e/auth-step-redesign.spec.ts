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

    // 3. PATCH tenant into the auth step. Default signup_mode is 'open' so the
    //    Credentials/Social/Pro sections are visible to tests that need them;
    //    individual tests that exercise invite_only behavior PATCH to invite_only
    //    inside their own body before opening the wizard.
    const patch = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: {
        tenant_intent: 'saas',
        signup_mode: 'open',
        onboarding_step: 3,
      },
    });
    if (!patch.ok()) throw new Error('Tenant PATCH failed: ' + await patch.text());
  });

  /** Helper for tests that need to start in invite_only state. */
  async function setSignupMode(
    request: import('@playwright/test').APIRequestContext,
    mode: 'open' | 'invite_only',
  ): Promise<void> {
    const res = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { signup_mode: mode },
    });
    if (!res.ok()) throw new Error('setSignupMode(' + mode + ') failed: ' + await res.text());
  }

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

  test('Access provider-group renders first with segmented control pre-filled from signup_mode @regression', async ({ browser, request }) => {
    // Pre-fill assertion is meaningful only when the tenant starts in invite_only,
    // so each path is exercised: this test PATCHes to invite_only before opening.
    await setSignupMode(request, 'invite_only');

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

      // Pre-fill: tenant was just patched to signup_mode=invite_only, so Invite-only is active
      await expect(inviteOption).toHaveClass(/active/);
      await expect(openOption).not.toHaveClass(/active/);

      // Credentials/Social/Pro sections stay visible regardless of signup_mode —
      // invited users still need configurable auth methods to log in.
      const credentialsGroup = page.locator('.wizard-pane-left .wizard-provider-group').filter({ hasText: 'Credentials' });
      const socialGroup = page.locator('.wizard-pane-left .wizard-provider-group').filter({ hasText: 'Social Providers' });
      const proSection = page.locator('.wizard-pane-left .wizard-pro-section');
      await expect(credentialsGroup).toBeVisible();
      await expect(socialGroup).toBeVisible();
      await expect(proSection).toBeVisible();

      // The right-pane Live Preview reflects signup_mode by hiding the
      // "Don't have an account? Sign up" link in invite_only.
      const previewSignupLink = page.locator('.wizard-pane-right .lp-toggle');
      await expect(previewSignupLink).toHaveCount(0);

      // Click Open and verify the active class moves AND the preview signup link reappears.
      await openOption.click();
      await expect(openOption).toHaveClass(/active/);
      await expect(inviteOption).not.toHaveClass(/active/);
      await expect(previewSignupLink).toBeVisible();
      await expect(previewSignupLink).toContainText('Sign up');
    } finally {
      await close();
    }
  });

  test('clicking Open then Continue persists signup_mode=open to the tenant @regression', async ({ browser, request }) => {
    // Start in invite_only so the Continue click actually transitions the value.
    await setSignupMode(request, 'invite_only');

    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      const firstGroup = page.locator('.wizard-pane-left .wizard-provider-group').first();
      await firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Open' }).click();

      // Click the step's Continue button
      await page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ }).click();

      // Verify the persisted value via the management API.
      // Use a sentinel for missing/null so the assertion fails fast with a
      // readable diff instead of timing out silently if the field disappears
      // from the response shape.
      await expect.poll(async () => {
        const res = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { 'Authorization': 'Bearer ' + platformToken },
        });
        if (!res.ok()) return '__http_' + res.status() + '__';
        const body = await res.json();
        return body.signup_mode ?? '__missing__';
      }, { timeout: 5000, message: 'signup_mode should be "open" after Continue' }).toBe('open');
    } finally {
      await close();
    }
  });

  test('switching to invite_only and back to open preserves allowed_providers @regression', async ({ browser, request }) => {
    // Seed a non-default allowed_providers list so we can verify it round-trips intact.
    const seeded = ['email', 'magic_link', 'google', 'github'];
    const seedRes = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { allowed_providers: seeded, signup_mode: 'open' },
    });
    if (!seedRes.ok()) throw new Error('seed PATCH failed: ' + await seedRes.text());

    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      const firstGroup = page.locator('.wizard-pane-left .wizard-provider-group').first();

      // Toggle to invite_only then back to open. handleContinue persists on Continue.
      await firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Invite-only' }).click();
      await firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Open' }).click();
      await page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ }).click();

      // The seeded allowed_providers should be intact in the DB.
      await expect.poll(async () => {
        const res = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { 'Authorization': 'Bearer ' + platformToken },
        });
        if (!res.ok()) return null;
        const body = await res.json();
        // Normalize: API returns JSON value (could be array or string-encoded array).
        const providers = Array.isArray(body.allowed_providers)
          ? body.allowed_providers
          : (typeof body.allowed_providers === 'string' ? JSON.parse(body.allowed_providers) : []);
        return [...providers].sort();
      }, { timeout: 5000, message: 'allowed_providers should be preserved across the invite_only round-trip' }).toEqual([...seeded].sort());
    } finally {
      await close();
    }
  });

  test.afterEach(async ({ request }) => {
    // Reset signup_mode + onboarding_step so each test starts from the known
    // open + step-3 state set in beforeAll. Without this, the persistence test
    // (which advances onboarding_step) and tests that mutate signup_mode would
    // break openWizardAtAuthStep for any test that runs after them.
    if (!tenantId) return;
    const reset = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { signup_mode: 'open', onboarding_step: 3 },
    });
    if (!reset.ok()) {
      throw new Error('afterEach reset failed (' + reset.status() + '): ' + await reset.text());
    }
  });
});
