# Auth Step Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the onboarding wizard's auth step into a three-layer model (Access / Credentials / Social / Pro), wire `signup_mode` to a new segmented control, and seed slots for Email OTP, Passkey, SMS OTP, SAML/SSO, and Custom OIDC as visual placeholders.

**Architecture:** All changes are inside `StepAuthMethods` in `admin/src/platform/console/OnboardingWizard.ts:538` plus a small CSS addition for the segmented control in `admin/src/core/theme/components.css`. No backend changes — `signup_mode` already round-trips through `updateTenant`. Tests are Playwright e2e in a new `admin/e2e/auth-step-redesign.spec.ts` file. Each task adds one new test that exercises one slice of the redesign before implementing it.

**Tech Stack:** TypeScript, custom `h(...)` virtual-DOM helper (FormaJS), SolidJS-style signals, Playwright for e2e.

**Spec:** `docs/superpowers/specs/2026-04-09-auth-step-redesign-design.md`

---

## Background reading before starting

These are the files the engineer should read once before Task 1 to build the mental model:

1. **`docs/superpowers/specs/2026-04-09-auth-step-redesign-design.md`** — the spec this plan implements. Read it end-to-end.
2. **`admin/src/platform/console/OnboardingWizard.ts:538-685`** — the current `StepAuthMethods` function. This is the *only* function being modified.
3. **`admin/src/platform/console/OnboardingWizard.ts:1002-1056`** — the existing Pro section pattern in `StepBranding`. Task 3 below mirrors this pattern.
4. **`admin/src/core/theme/components.css:3705-3762`** — existing wizard provider toggle CSS classes (already cover all "Coming soon" needs).
5. **`admin/src/core/theme/components.css:4395-4433`** — existing wizard pro section CSS classes (already cover Pro section needs).
6. **`admin/e2e/wizard-flow.spec.ts`** — the existing tenant-lifecycle Playwright test. The test scaffold in Task 1 follows this file's setup pattern (register platform user → create tenant via API → drive browser).
7. **`admin/e2e/helpers.ts:1-52`** — `BASE`, `postWithRetry`, `getAccessToken` helpers used by all e2e tests.

### Verified facts (do not re-verify, do not change)

- `signup_mode` is already in the `Tenant` TypeScript interface (`admin/src/platform/console/store.ts:42`).
- `signup_mode` is already in `updateTenant`'s `Pick<Tenant, ...>` allowlist (`admin/src/platform/console/store.ts:812`).
- `UpdateTenantRequest` on the server already accepts `signup_mode` (`crates/server/src/handlers/management.rs:71`).
- The server validates `signup_mode` against `["open", "invite_only"]` (`crates/server/src/handlers/management.rs:81-92`).
- The `PATCH /platform/self-serve/tenants/{id}` handler persists `signup_mode` to the database (`crates/server/src/handlers/management.rs:475-481`).
- `StepAuthMethods` is the wizard step rendered when `tenant.onboarding_step === 3` (which becomes `wizardStep() === 2` after the off-by-one adjustment at `OnboardingWizard.ts:1637`).
- The current "always at least email" guard at `OnboardingWizard.ts:551` (`if (p === "email" && list.length === 1) return;`) must carry forward unchanged.

### Conventions to follow

- The codebase uses a custom `h(tag, props, ...children)` helper (FormaJS-style virtual DOM), **not JSX**. All new UI is built by calling `h(...)`. See lines 627-684 for the existing `StepAuthMethods` body as the reference shape.
- Signals use `createSignal<T>(initial)` and are read by calling them (`providers()`) and written by their setter (`setProviders([...])`). See line 546.
- Provider rows use the `ProviderToggle` helper at line 588. It already handles `available: false` by rendering a "Coming soon" badge and disabling the checkbox — do not duplicate that logic.
- CSS classes are kebab-case and live in `admin/src/core/theme/components.css`. Wizard-specific classes are prefixed `wizard-`.
- Test files in `admin/e2e/` use Playwright with the helpers in `admin/e2e/helpers.ts`. Run them via `npm run test` (which runs `playwright test` per `admin/package.json:8`).

---

## File Structure

| File | What changes |
|---|---|
| `admin/src/platform/console/OnboardingWizard.ts` | `StepAuthMethods` function only (lines 538-685). Add `signupMode` signal, append two entries to `CREDENTIAL_PROVIDERS`, render new "Access" provider-group as the first group inside the left pane, render new Pro section after the Social group, add `signup_mode` to the `updateTenant` call in `handleContinue`. |
| `admin/src/core/theme/components.css` | Add ~30 lines of new CSS for `.wizard-access-segmented`, `.wizard-access-segmented-option`, and `.wizard-access-segmented-option.active`. Insert immediately after the existing wizard-provider-* block (after line 3762). |
| `admin/e2e/auth-step-redesign.spec.ts` | New file. Playwright test suite that creates a platform user + tenant via API, drives the browser into the wizard at step 3, and asserts the new structure piece by piece across tasks. |

No new files outside these three. No migrations. No new types.

---

## Task 1: Test scaffold + add Email OTP and Passkey to Credentials

**Files:**
- Create: `admin/e2e/auth-step-redesign.spec.ts`
- Modify: `admin/src/platform/console/OnboardingWizard.ts:572-579` (the `CREDENTIAL_PROVIDERS` array literal)

This task creates the test harness that all subsequent tasks will use, then makes its first assertion against an actual change (Email OTP and Passkey rows in the Credentials group). The two changes are bundled because they share the test scaffold cost — separating them would mean writing two near-identical scaffolds.

- [ ] **Step 1: Create the test scaffold file**

Create `admin/e2e/auth-step-redesign.spec.ts` with the following content. The `beforeAll` registers a platform user, creates a tenant, and PATCHes it into the auth step (`onboarding_step: 3`) with intent `internal` and `signup_mode: invite_only` so later tasks can verify the pre-fill behavior. The `afterAll` cleans up the tenant.

```typescript
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
```

- [ ] **Step 2: Run the new test to verify it fails**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Email OTP and Passkey" --reporter=line
```

Expected: **FAIL.** The error should mention either `Email OTP` or `Passkey` not being visible (the rows don't exist yet). The test scaffold itself (login, tenant creation, wizard navigation) should succeed — if the failure is in the scaffold (e.g., 401 on signup, can't find tenant card), debug the scaffold before continuing.

If the scaffold fails because the wizard does not auto-open on tenant card click for an incomplete tenant: the wizard mount logic at `admin/src/platform/console/ConsoleApp.ts:497` and `admin/src/platform/console/OnboardingWizard.ts:1631-1639` is the source of truth. Read those, then adjust the navigation in `openWizardAtAuthStep` accordingly (you may need an additional click or a `waitFor` on the wizard headline). Do not change the wizard logic itself.

- [ ] **Step 3: Append Email OTP and Passkey to `CREDENTIAL_PROVIDERS`**

Edit `admin/src/platform/console/OnboardingWizard.ts:572-579`. Change this:

```typescript
  const CREDENTIAL_PROVIDERS = [
    { key: "email", label: "Email / Password" },
    {
      key: "magic_link",
      label: "Magic Link",
      subtitle: "Passwordless sign-in via email",
    },
  ];
```

to this:

```typescript
  const CREDENTIAL_PROVIDERS = [
    { key: "email", label: "Email / Password" },
    {
      key: "magic_link",
      label: "Magic Link",
      subtitle: "Passwordless sign-in via email",
    },
    {
      key: "email_otp",
      label: "Email OTP",
      subtitle: "6-digit code sent to email",
      available: false,
    },
    {
      key: "passkey",
      label: "Passkey",
      subtitle: "Sign in with Face ID, fingerprint, or device PIN",
      available: false,
    },
  ];
```

The existing `ProviderToggle` helper (line 588) already handles `available: false` by rendering a "Coming soon" badge and disabling the checkbox — no other code change is needed.

- [ ] **Step 4: Run the test again to verify it passes**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Email OTP and Passkey" --reporter=line
```

Expected: **PASS** (1 test, 0 failures).

- [ ] **Step 5: Typecheck**

```bash
cd admin && npm run typecheck
```

Expected: no errors. The new type shape `{ key, label, subtitle?, available? }` matches the existing `ProviderToggle` opt parameter (line 588-593) so this should pass cleanly.

- [ ] **Step 6: Commit**

```bash
git add admin/e2e/auth-step-redesign.spec.ts admin/src/platform/console/OnboardingWizard.ts
git commit -m "$(cat <<'EOF'
feat(wizard): add Email OTP and Passkey to credentials group as Coming soon

Adds two new entries to CREDENTIAL_PROVIDERS in the auth step. Both render
through the existing ProviderToggle path with available: false, which already
shows a "Coming soon" badge and disables the checkbox.

Also introduces admin/e2e/auth-step-redesign.spec.ts as the test harness for
the rest of the auth step redesign work.
EOF
)"
```

---

## Task 2: Add the Pro section

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts:647-653` (insert a new section after the existing Social Providers group block)
- Modify: `admin/e2e/auth-step-redesign.spec.ts` (add one new test)

Mirrors the existing Pro section pattern from `StepBranding` (`OnboardingWizard.ts:1002-1056`) — same `wizard-pro-section` shell, same `wizard-pro-header` with "Pro" badge + "Upgrade to unlock" label, same `wizard-pro-item` rows with disabled checkboxes. No new CSS.

- [ ] **Step 1: Add the failing test**

In `admin/e2e/auth-step-redesign.spec.ts`, add this new `test(...)` block inside the `test.describe(...)` block, after the Email OTP test:

```typescript
  test('Pro section renders SMS OTP, SAML/SSO, and Custom OIDC as upgrade-locked items @regression', async ({ browser }) => {
    const { page, close } = await openWizardAtAuthStep(browser);

    const proSection = page.locator('.wizard-pro-section');
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

    await close();
  });
```

- [ ] **Step 2: Run the new test to verify it fails**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Pro section" --reporter=line
```

Expected: **FAIL.** The error should be that `.wizard-pro-section` is not visible — the section doesn't exist in `StepAuthMethods` yet.

- [ ] **Step 3: Add the Pro section to `StepAuthMethods`**

Edit `admin/src/platform/console/OnboardingWizard.ts`. Find this block (currently at lines 647-653, the Social Providers group):

```typescript
      h(
        "div",
        { class: "wizard-provider-group" },
        h("div", { class: "wizard-provider-group-label" }, "Social Providers"),
        ...SOCIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),
```

Immediately after that closing `),` (and before the `wizard-actions` block at line 654), insert:

```typescript
      h(
        "div",
        { class: "wizard-pro-section" },
        h(
          "div",
          { class: "wizard-pro-header" },
          h("span", { class: "wizard-provider-badge" }, "Pro"),
          h("span", { class: "wizard-pro-label" }, "Upgrade to unlock"),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "SMS OTP"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "SAML / SSO"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Custom OIDC"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
      ),
```

This is the exact shape used by the branding step's Pro section (`OnboardingWizard.ts:1002-1056`). The three rows are inlined rather than mapped over an array — three rows is below the threshold for a helper, and inlining keeps the change reviewable in one place.

- [ ] **Step 4: Run the test again to verify it passes**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Pro section" --reporter=line
```

Expected: **PASS.**

Also re-run the previous test to verify nothing regressed:

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --reporter=line
```

Expected: **2 passed.**

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts admin/e2e/auth-step-redesign.spec.ts
git commit -m "$(cat <<'EOF'
feat(wizard): add Pro section to auth step with SMS OTP, SAML/SSO, OIDC

Renders a wizard-pro-section after the Social Providers group with three
upgrade-locked rows: SMS OTP, SAML / SSO, Custom OIDC. Mirrors the existing
Pro section pattern from StepBranding — same classes, same disabled-toggle
shape, no new CSS.
EOF
)"
```

---

## Task 3: Add the Access provider-group with segmented control

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (`StepAuthMethods` function — add `signupMode` signal, render new provider-group as the first group inside the left pane)
- Modify: `admin/src/core/theme/components.css` (add ~30 lines after line 3762 for the segmented control classes)
- Modify: `admin/e2e/auth-step-redesign.spec.ts` (add one new test)

This is the largest task in the plan because it introduces new state, new visible UI, and new CSS together. The test asserts the visible end-state: the Access group exists, is rendered first, has both options with sub-copy, and has the "Invite-only" option active (because the test tenant was created with `signup_mode: invite_only` in the scaffold).

- [ ] **Step 1: Add the failing test**

In `admin/e2e/auth-step-redesign.spec.ts`, add this new `test(...)` block after the Pro section test:

```typescript
  test('Access provider-group renders first with segmented control pre-filled from signup_mode @regression', async ({ browser }) => {
    const { page, close } = await openWizardAtAuthStep(browser);

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

    await close();
  });
```

- [ ] **Step 2: Run the new test to verify it fails**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Access provider-group" --reporter=line
```

Expected: **FAIL.** The error should be that the first provider-group's label is "Credentials" (not "Access") or that `.wizard-access-segmented-option` is not found.

- [ ] **Step 3: Add CSS for the segmented control**

Edit `admin/src/core/theme/components.css`. Find the existing `.wizard-provider-badge` block at line 3755-3762:

```css
.wizard-provider-badge {
  font-size: 0.65rem;
  color: var(--text-muted);
  background: var(--surface-overlay);
  padding: 2px 6px;
  border-radius: 4px;
  width: fit-content;
}
```

Immediately after the closing `}` of that block (and before the `/* Mode selection (Step 3) */` comment at line 3764), insert:

```css
/* Access mode segmented control (Step 2 / Auth) */

.wizard-access-segmented {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}

.wizard-access-segmented-option {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}

.wizard-access-segmented-option:hover {
  border-color: var(--border-strong);
}

.wizard-access-segmented-option.active {
  border-color: var(--accent);
  background: var(--surface-raised);
}

.wizard-access-segmented-option-label {
  font-size: 0.85rem;
  font-weight: 500;
}

.wizard-access-segmented-option-subtitle {
  font-size: 0.75rem;
  color: var(--text-muted);
}
```

The CSS variables (`--border`, `--border-strong`, `--accent`, `--surface-raised`, `--text`, `--text-muted`) are the same ones used by the existing wizard-provider-* classes — they are defined elsewhere in the theme and do not need to be created.

- [ ] **Step 4: Add the `signupMode` signal and render the Access group**

Edit `admin/src/platform/console/OnboardingWizard.ts`. Two changes inside `StepAuthMethods`:

**4a. Add the signal.** Find this block at lines 543-546:

```typescript
  const current = (tenant.allowed_providers as string[] | undefined) || [
    "email",
  ];
  const [providers, setProviders] = createSignal<string[]>([...current]);
```

Immediately after that, insert:

```typescript
  const currentSignupMode = (tenant.signup_mode as string | undefined) || "open";
  const [signupMode, setSignupMode] = createSignal<string>(currentSignupMode);
```

**4b. Render the Access group as the first group in the left pane.** Find the existing Credentials group block (currently at lines 640-645):

```typescript
      h(
        "div",
        { class: "wizard-provider-group" },
        h("div", { class: "wizard-provider-group-label" }, "Credentials"),
        ...CREDENTIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),
```

Immediately *before* that block (and after the `wizard-subtitle` `<p>` at line 638), insert the new Access group:

```typescript
      h(
        "div",
        { class: "wizard-provider-group" },
        h("div", { class: "wizard-provider-group-label" }, "Access"),
        h(
          "div",
          { class: "wizard-access-segmented" },
          h(
            "button",
            {
              type: "button",
              class: () =>
                "wizard-access-segmented-option" +
                (signupMode() === "open" ? " active" : ""),
              onClick: () => setSignupMode("open"),
            },
            h("span", { class: "wizard-access-segmented-option-label" }, "Open"),
            h(
              "span",
              { class: "wizard-access-segmented-option-subtitle" },
              "Anyone can create an account.",
            ),
          ),
          h(
            "button",
            {
              type: "button",
              class: () =>
                "wizard-access-segmented-option" +
                (signupMode() === "invite_only" ? " active" : ""),
              onClick: () => setSignupMode("invite_only"),
            },
            h(
              "span",
              { class: "wizard-access-segmented-option-label" },
              "Invite-only",
            ),
            h(
              "span",
              { class: "wizard-access-segmented-option-subtitle" },
              "Only people you invite can sign up. You'll manage invites from the dashboard.",
            ),
          ),
        ),
      ),
```

The reactive `class: () => ...` pattern matches how other classes in this file react to signal changes (e.g., line 598-600 in the existing `ProviderToggle`).

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "Access provider-group" --reporter=line
```

Expected: **PASS.**

If the `active` class assertion fails because the reactive class binding doesn't update on click: confirm the `class: () => ...` function form is being rendered correctly. The same pattern is used at `OnboardingWizard.ts:598-600` for `wizard-provider-toggle` — if that one works, this one should too. If the reactive form is being toString'd instead of called, the issue is in the FormaJS `h` runtime, not in this code.

Run the full file too:

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --reporter=line
```

Expected: **3 passed.**

- [ ] **Step 6: Typecheck**

```bash
cd admin && npm run typecheck
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts admin/src/core/theme/components.css admin/e2e/auth-step-redesign.spec.ts
git commit -m "$(cat <<'EOF'
feat(wizard): add Access provider-group with segmented signup_mode control

Adds an "Access" provider-group as the first group inside the auth step's
left pane, with a two-button segmented control bound to a new signupMode
signal. The signal pre-fills from tenant.signup_mode (which intent step
already populates), so a tenant set up as "internal" lands on Invite-only
without re-asking.

Adds wizard-access-segmented* CSS for the new control. Persistence wire-up
to handleContinue lands in the next commit.
EOF
)"
```

---

## Task 4: Wire `signupMode` through `handleContinue`

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts:558-563` (`handleContinue` inside `StepAuthMethods`)
- Modify: `admin/e2e/auth-step-redesign.spec.ts` (add one new test)

The segmented control already updates the in-memory signal when clicked. This task makes the value persist when the user clicks Continue.

- [ ] **Step 1: Add the failing test**

In `admin/e2e/auth-step-redesign.spec.ts`, add this new `test(...)` block after the Access group test:

```typescript
  test('clicking Open then Continue persists signup_mode=open to the tenant @regression', async ({ browser, request }) => {
    const { page, close } = await openWizardAtAuthStep(browser);

    // Tenant starts at signup_mode=invite_only (set in beforeAll). Switch to Open.
    const firstGroup = page.locator('.wizard-pane-left .wizard-provider-group').first();
    await firstGroup.locator('.wizard-access-segmented-option').filter({ hasText: 'Open' }).click();

    // Click the step's Continue button
    await page.locator('.wizard-pane-left .btn.btn-accent').filter({ hasText: /Continue/ }).click();

    // Verify the persisted value via the management API
    await expect.poll(async () => {
      const res = await request.get(BASE + '/platform/self-serve/tenants/' + tenantId, {
        headers: { 'Authorization': 'Bearer ' + platformToken },
      });
      if (!res.ok()) return null;
      const body = await res.json();
      return body.signup_mode;
    }, { timeout: 5000 }).toBe('open');

    await close();
  });

  test.afterEach(async ({ request }) => {
    // Reset signup_mode + onboarding_step so each test starts from the same state
    if (!tenantId) return;
    await request.patch(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { 'Authorization': 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { signup_mode: 'invite_only', onboarding_step: 3 },
    });
  });
```

The `afterEach` block resets the tenant state between tests so the order of tests does not matter. Add it as a sibling of the existing tests inside the `test.describe` — Playwright's `test.afterEach` only applies to tests in the same describe block.

- [ ] **Step 2: Run the new test to verify it fails**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "persists signup_mode" --reporter=line
```

Expected: **FAIL.** The poll should time out with the API returning `signup_mode: 'invite_only'` (the original value) instead of `'open'`, because `handleContinue` is not yet sending the new value.

- [ ] **Step 3: Add `signup_mode` to the `updateTenant` call**

Edit `admin/src/platform/console/OnboardingWizard.ts`. Find the existing `handleContinue` at lines 558-563:

```typescript
  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { allowed_providers: providers() });
    setSaving(false);
    advanceStep();
  }
```

Replace the `updateTenant` line so the call passes `signup_mode` alongside `allowed_providers`:

```typescript
  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      allowed_providers: providers(),
      signup_mode: signupMode(),
    });
    setSaving(false);
    advanceStep();
  }
```

That is the entire change. `signup_mode` is already in `updateTenant`'s allowlist (`store.ts:812`) so no other code is touched.

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --grep "persists signup_mode" --reporter=line
```

Expected: **PASS.**

Run the full file:

```bash
cd admin && npx playwright test e2e/auth-step-redesign.spec.ts --reporter=line
```

Expected: **4 passed.**

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts admin/e2e/auth-step-redesign.spec.ts
git commit -m "$(cat <<'EOF'
feat(wizard): persist signup_mode from auth step Continue button

handleContinue now sends signup_mode alongside allowed_providers in the
updateTenant call, so the new Access segmented control's value actually
sticks when the operator advances past the auth step.
EOF
)"
```

---

## Task 5: Final verification

**Files:** none modified.

This task runs the full test suite, the typecheck, and a manual visual check, then commits nothing if all pass. It is the safety net that catches any regression in adjacent tests caused by the StepAuthMethods changes.

- [ ] **Step 1: Run the full Playwright suite**

```bash
cd admin && npm run test
```

Expected: all tests pass. If any unrelated test fails, investigate before continuing — the auth step changes are tightly scoped, so a failure in `wizard-flow.spec.ts`, `console.spec.ts`, or `visual.spec.ts` is more likely a real regression than a flake. If a visual snapshot test fails (e.g., `visual.spec.ts-snapshots/onboarding-darwin.png`), the snapshot is stale because the wizard step now has more content — re-baseline only after manual visual verification (Step 3).

- [ ] **Step 2: Typecheck**

```bash
cd admin && npm run typecheck
```

Expected: no errors.

- [ ] **Step 3: Manual visual check**

Start the dev server and walk through the auth step in a browser to confirm the redesigned step looks right at each interaction:

```bash
cd admin && npm run dev
```

Open `http://127.0.0.1:3102/platform/console`, log in as a platform user, click into a tenant whose onboarding is incomplete (or create a new one and walk through steps 1-2), and verify on the auth step:

1. The four `wizard-provider-group` sections appear in this order, top to bottom: Access, Credentials, Social Providers, Pro upgrade.
2. The Access segmented control has both options visible with sub-copy.
3. Clicking between Open and Invite-only updates the active class instantly.
4. The Credentials group shows: Email/Password (working), Magic Link (working), Email OTP (Coming soon, disabled), Passkey (Coming soon, disabled).
5. The Social Providers group is unchanged: Google (working), GitHub (working), Microsoft (Coming soon), Apple (Coming soon).
6. The Pro section appears below Social with a "Pro" badge in the header and three locked rows: SMS OTP, SAML / SSO, Custom OIDC.
7. The right-pane live preview is unchanged.
8. Clicking Continue advances to the Branding step (no errors in console).

If anything looks off — especially the segmented control's active state, or the Pro section's spacing relative to Social — fix inline and amend the relevant task's commit. Do not proceed to the next plan until this manual check passes.

- [ ] **Step 4: No commit**

If everything passes, this task creates no new commit. The work is done.

If a visual snapshot did need re-baselining in Step 1, commit that separately:

```bash
git add admin/e2e/visual.spec.ts-snapshots/
git commit -m "test: re-baseline onboarding visual snapshot for auth step redesign"
```

---

## Summary of commits

By the end of this plan, the branch should have the following new commits:

1. `feat(wizard): add Email OTP and Passkey to credentials group as Coming soon`
2. `feat(wizard): add Pro section to auth step with SMS OTP, SAML/SSO, OIDC`
3. `feat(wizard): add Access provider-group with segmented signup_mode control`
4. `feat(wizard): persist signup_mode from auth step Continue button`
5. (optional) `test: re-baseline onboarding visual snapshot for auth step redesign`

Each commit is independently shippable and reviewable.
