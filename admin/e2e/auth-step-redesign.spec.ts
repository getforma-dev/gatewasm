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

  test('Continue button is disabled and warning shows when allowed_providers is empty @regression', async ({ browser, request }) => {
    // Force the tenant into the broken empty state, then open the wizard.
    const empty = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { allowed_providers: [] },
    });
    if (!empty.ok()) throw new Error('seed empty providers failed: ' + await empty.text());

    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      // Inline warning is in the DOM and NOT hidden when providers is empty.
      // Asserting explicit class state to make the visibility contract
      // (`.wizard-form-warning-hidden` toggles `display: none`) load-bearing,
      // so any future CSS regression on the hidden helper would surface here.
      const warning = page.locator('.wizard-pane-left .wizard-form-warning');
      await expect(warning).toHaveCount(1);
      await expect(warning).toContainText('at least one sign-in method');
      await expect(warning).not.toHaveClass(/wizard-form-warning-hidden/);

      // Continue button is disabled
      const continueBtn = page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ });
      await expect(continueBtn).toBeDisabled();

      // Toggle email on — warning hides, button enables
      const emailRow = page.locator('.wizard-pane-left .wizard-provider-toggle').filter({ hasText: 'Email / Password' });
      await emailRow.click();
      await expect(warning).toHaveClass(/wizard-form-warning-hidden/);
      await expect(continueBtn).toBeEnabled();
    } finally {
      await close();
    }
  });

  test('cannot remove the last sign-in method via the toggle (preventDefault guard) @regression', async ({ browser, request }) => {
    // Seed exactly one provider so the guard path is exercised on the next click.
    await setSignupMode(request, 'open');
    const seed = await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { allowed_providers: ['email'] },
    });
    if (!seed.ok()) throw new Error('seed failed: ' + await seed.text());

    const { page, close } = await openWizardAtAuthStep(browser);
    try {
      const emailRow = page.locator('.wizard-pane-left .wizard-provider-toggle').filter({ hasText: 'Email / Password' });
      // The input element is visually hidden — clicking it directly via
      // Playwright would time out waiting for visibility. Click the
      // .wizard-toggle label instead (the visible target the operator
      // actually interacts with), which fires the input's click event via
      // implicit form-control association.
      const emailLabel = emailRow.locator('label.wizard-toggle');
      const emailCheckbox = emailRow.locator('input[type="checkbox"]');

      // Email is the only provider — initial state checked.
      await expect(emailCheckbox).toBeChecked();

      // Click the toggle. The input's onClick handler should call
      // preventDefault when this would empty the providers list, the browser
      // should NOT flip the checkbox state, and the checkbox should remain
      // checked. Regression test for the toggle visual desync bug — without
      // preventDefault, the browser optimistically flips the visual while
      // the signal stays the same, leaving the toggle and the live preview
      // out of sync.
      await emailLabel.click();
      await expect(emailCheckbox).toBeChecked();

      // The continue button should remain enabled — providers is still ["email"].
      const continueBtn = page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ });
      await expect(continueBtn).toBeEnabled();

      // The warning should NOT be visible — providers is non-empty.
      const warning = page.locator('.wizard-pane-left .wizard-form-warning');
      await expect(warning).toHaveClass(/wizard-form-warning-hidden/);
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

  test('complete wizard walkthrough from + New Application to Launch Dashboard @regression', async ({ browser, request }) => {
    // Walks the entire onboarding wizard end-to-end as a real operator would,
    // verifying URL hash transitions, single-click Continue advancement, the
    // last-provider guard, the skip-button visibility per step, and the final
    // launch hard-navigation. Catches regressions in the integration of every
    // step's Continue handler with the hash-navigation race fix.

    const walkthroughSlug = 'walkthrough-' + Date.now().toString(36);
    const walkthroughName = 'Walkthrough App ' + Date.now();
    let walkthroughTenantId: string | null = null;

    const context = await browser.newContext();
    const page = await context.newPage();

    // Capture page errors so any thrown ReferenceError / TypeError surfaces in
    // the failure output. Without this, an exception thrown during a render
    // can bubble into a parent try/catch (e.g. inside updateTenant) and look
    // like a generic API failure with no clue what's actually wrong.
    const pageErrors: string[] = [];
    page.on('pageerror', (err) => {
      pageErrors.push('pageerror: ' + (err.stack || err.message));
    });
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        pageErrors.push('console.error: ' + msg.text());
      }
    });

    // Track all 4xx/5xx HTTP responses so we can scope the error filter to
    // specific endpoints rather than a broad "ignore all 400s" rule. The
    // CopyButton runtime error never produced a network failure, so this is
    // independent of pageErrors but helps tighten the assertion.
    const failedResponses: Array<{ url: string; status: number }> = [];
    page.on('response', (response) => {
      if (response.status() >= 400) {
        failedResponses.push({
          url: response.url().replace(BASE, ''),
          status: response.status(),
        });
      }
    });

    try {
      // Browser-context login (creates the refresh cookie).
      const loginRes = await context.request.post(BASE + '/auth/login', {
        headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
        data: { email: platformEmail, password: platformPassword },
      });
      if (!loginRes.ok()) throw new Error('Walkthrough login failed: ' + await loginRes.text());

      // Land on the console account view.
      await page.goto(BASE + '/platform/console');
      await expect(page.getByRole('button', { name: '+ New Application' })).toBeVisible({ timeout: 10000 });

      // ── Step 0: Create ────────────────────────────────────────────────
      await page.getByRole('button', { name: '+ New Application' }).click();
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'Create your application' })).toBeVisible();

      // Skip Setup button is HIDDEN on Create (per per-step visibility rule).
      await expect(page.locator('.wizard-skip')).toHaveCount(0);

      // Fill name → slug auto-derives → manually overwrite slug for predictability.
      await page.getByLabel('App Name').fill(walkthroughName);
      // The slug input doesn't have an accessible label; use the placeholder.
      const slugInput = page.locator('input[placeholder="acme-app"]');
      await slugInput.fill(walkthroughSlug);

      // Wait for slug availability check (debounced 500ms).
      await expect(page.locator('.slug-status-available')).toBeVisible({ timeout: 8000 });

      // Single-click advance.
      await page.getByRole('button', { name: /Create & Continue/ }).click();

      // Capture tenant id for cleanup. Wait for the wizard to advance first.
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'What are you building' })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-intent');
      expect(new URL(page.url()).pathname).toContain(walkthroughSlug);

      // Look up the tenant id by slug for cleanup.
      const listRes = await request.get(BASE + '/platform/self-serve/tenants', {
        headers: { 'Authorization': 'Bearer ' + platformToken },
      });
      const listBody = await listRes.json();
      const found = (listBody.tenants || []).find((t: { slug: string; id: string }) => t.slug === walkthroughSlug);
      if (found) walkthroughTenantId = found.id;

      // Skip Setup button is VISIBLE on Intent.
      await expect(page.locator('.wizard-skip')).toHaveCount(1);

      // ── Step 1: Intent ────────────────────────────────────────────────
      // Pick "Internal Tool" so the persisted preferred_tab is "integrate"
      // (the SaaS default also resolves to integrate, but Internal is the
      // most explicit "no public signup" intent).
      await page.locator('.intent-card').filter({ hasText: 'Internal Tool' }).click();
      await expect(page.locator('.intent-card.intent-card-selected').filter({ hasText: 'Internal Tool' })).toBeVisible();

      // Single-click Continue advances to Auth (regression for the hash race).
      await page.locator('.btn.btn-accent').filter({ hasText: /Continue/ }).first().click();
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'How will users sign in' })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-auth');

      // Skip Setup button is VISIBLE on Auth.
      await expect(page.locator('.wizard-skip')).toHaveCount(1);

      // ── Step 2: Auth ──────────────────────────────────────────────────
      // The Internal intent default is ["email", "google"], so Continue is
      // already enabled and the warning is hidden.
      const continueAuth = page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ });
      await expect(continueAuth).toBeEnabled();
      const warning = page.locator('.wizard-pane-left .wizard-form-warning');
      await expect(warning).toHaveClass(/wizard-form-warning-hidden/);

      // Toggle Google off — providers should drop to ["email"], Continue still enabled.
      const googleRow = page.locator('.wizard-pane-left .wizard-provider-toggle').filter({ hasText: 'Google' });
      await googleRow.locator('label.wizard-toggle').click();
      // Now ["email"] only — try to remove email, the preventDefault guard
      // should keep it checked.
      const emailRow = page.locator('.wizard-pane-left .wizard-provider-toggle').filter({ hasText: 'Email / Password' });
      const emailCheckbox = emailRow.locator('input[type="checkbox"]');
      await expect(emailCheckbox).toBeChecked();
      await emailRow.locator('label.wizard-toggle').click();
      await expect(emailCheckbox).toBeChecked();

      // Continue advances to Branding.
      await continueAuth.click();
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'Style your sign-in card' })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-branding');

      // Skip Setup is VISIBLE on Branding.
      await expect(page.locator('.wizard-skip')).toHaveCount(1);

      // ── Step 3: Branding ──────────────────────────────────────────────
      // Accept defaults — single-click Continue advances to Mode.
      await page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ }).click();
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'How do you want to use authentication' })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-mode');

      // Skip Setup is HIDDEN on Mode (per the per-step visibility rule).
      await expect(page.locator('.wizard-skip')).toHaveCount(0);

      // ── Step 4: Mode ──────────────────────────────────────────────────
      // Pick "Integrate into your app" → preferred_tab=integrate.
      await page.locator('.mode-card').filter({ hasText: 'Integrate into your app' }).click();
      await expect(page.locator('.mode-card.mode-card-selected').filter({ hasText: 'Integrate into your app' })).toBeVisible();

      await page.locator('.btn.btn-accent').filter({ hasText: /Continue/ }).first().click();
      // Test step has a different headline — "Try signing in as an end user".
      await expect(page.locator('h1.wizard-headline').filter({ hasText: 'Try signing in' })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-test');

      // Skip Setup (header) is HIDDEN on Test.
      await expect(page.locator('.wizard-skip')).toHaveCount(0);

      // ── Step 5: Test ──────────────────────────────────────────────────
      // Click the inline "Skip this step" button to advance.
      await page.getByRole('button', { name: 'Skip this step' }).click();
      await expect(page.locator('h1.wizard-headline').filter({ hasText: /Add authentication to your app|Your hosted page is live/ })).toBeVisible({ timeout: 8000 });
      expect(new URL(page.url()).hash).toBe('#wizard-launch');

      // Skip Setup (header) is HIDDEN on Launch.
      await expect(page.locator('.wizard-skip')).toHaveCount(0);

      // ── Step 6: Launch ────────────────────────────────────────────────
      // StepLaunch declares two Launch Dashboard buttons in mutually exclusive
      // branches (hosted-page vs integrate). We picked Integrate at the Mode
      // step, so exactly one Launch button should be in the DOM. Assert
      // explicitly so a future regression that causes both branches to render
      // is caught here instead of being papered over by .first().
      const launchBtn = page.getByRole('button', { name: /Launch Dashboard/ });
      await expect(launchBtn).toHaveCount(1);
      await expect(launchBtn).toBeVisible();
      await expect(launchBtn).toBeEnabled();
      await launchBtn.click();

      // Wait for the hard navigation to actually complete.
      await page.waitForURL(
        (url) => {
          const u = new URL(url.toString());
          return (
            u.pathname.includes(walkthroughSlug) && !u.hash.includes('wizard-')
          );
        },
        { timeout: 20000 },
      );

      // The destination URL hash should be a valid console view, not a wizard
      // step. Internal intent → integrate today, but accept any valid view so
      // this test doesn't break if the default routing rule changes.
      const finalHash = new URL(page.url()).hash;
      expect(['#overview', '#integrate', '#hosted-page']).toContain(finalHash);

      // Workspace shell should be visible (account-shell would be wrong).
      await expect(page.locator('.shell')).toBeVisible({ timeout: 15000 });
      // The wizard should NOT be in the DOM anymore.
      await expect(page.locator('.wizard-root')).toHaveCount(0);

      // The welcome toast should appear after the page reload (drained from
      // sessionStorage by init()).
      await expect(page.getByText('Your application is ready')).toBeVisible({ timeout: 10000 });

      // No JS errors should have leaked into the console during the entire
      // walkthrough. The CopyButton import bug (the root cause of the launch
      // empty-screen issue) would surface here as a ReferenceError because
      // OverviewView throws during the wizard → workspace shell transition.
      // Filter out generic resource-load errors that can't be attributed to
      // a specific code bug — failedResponses captures the actual HTTP status
      // and URL separately so we can assert on those specifically below.
      const seriousErrors = pageErrors.filter(
        (e) => !e.includes('Failed to load resource'),
      );
      expect(
        seriousErrors,
        'Walkthrough produced JS errors:\n' + seriousErrors.join('\n'),
      ).toEqual([]);

      // Tighten the HTTP failure filter to a known-benign allowlist:
      //   /stats             — brand-new tenant has no data, returns empty/400
      //   /webhooks          — empty list endpoint, can 4xx on first hydrate
      //   /t/{slug}/auth/token — tenant-scoped end-user auth refresh; the
      //                         platform operator is NOT a tenant end user, so
      //                         the silentRefresh after the hard navigation
      //                         legitimately fails with 400. Operator auth
      //                         continues to work via the platform cookie.
      // ANY OTHER 4xx/5xx during the walkthrough is a real regression.
      const unexpectedFailures = failedResponses.filter(
        (r) =>
          !r.url.includes('/stats') &&
          !r.url.includes('/webhooks') &&
          !r.url.match(/\/t\/[^/]+\/auth\/token/),
      );
      expect(
        unexpectedFailures,
        'Walkthrough produced unexpected HTTP failures:\n' +
          unexpectedFailures.map((r) => r.status + ' ' + r.url).join('\n'),
      ).toEqual([]);
    } finally {
      // Cleanup. Try the captured tenant id first (the happy path); if that
      // wasn't captured (e.g. test crashed before the wizard advanced past
      // Create), fall back to listing all tenants and deleting the one
      // matching the walkthrough slug. This prevents orphan tenants from
      // accumulating in the database when the test crashes early.
      try {
        const idForCleanup =
          walkthroughTenantId ??
          (await (async () => {
            try {
              const res = await request.get(
                BASE + '/platform/self-serve/tenants',
                {
                  headers: { Authorization: 'Bearer ' + platformToken },
                },
              );
              if (!res.ok()) return null;
              const body = await res.json();
              const found = (body.tenants || []).find(
                (t: { slug: string; id: string }) => t.slug === walkthroughSlug,
              );
              return found?.id ?? null;
            } catch {
              return null;
            }
          })());
        if (idForCleanup) {
          await request.delete(
            BASE + '/platform/self-serve/tenants/' + idForCleanup,
            {
              headers: {
                Authorization: 'Bearer ' + platformToken,
                'Content-Type': 'application/json',
              },
              data: { confirm_slug: walkthroughSlug },
            },
          );
        }
      } catch (err) {
        // Cleanup failure is non-fatal — log it but don't mask the original
        // test failure (if any).
        // eslint-disable-next-line no-console
        console.warn('Walkthrough cleanup failed:', err);
      }
      await context.close();
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
      data: { signup_mode: 'open', onboarding_step: 3, allowed_providers: ['email', 'google'] },
    });
    if (!reset.ok()) {
      throw new Error('afterEach reset failed (' + reset.status() + '): ' + await reset.text());
    }
  });
});
