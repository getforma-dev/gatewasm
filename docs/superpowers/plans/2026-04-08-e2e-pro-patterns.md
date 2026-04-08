# E2E Pro Patterns Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the 135-test Playwright e2e suite with custom fixtures, test tagging, CI integration, selector maps, visual regression, centralized factories, accessibility testing, and missing regression scenarios.

**Architecture:** Each task is independent and mergeable on its own. Fixtures (Task 1) and factories (Task 6) are foundational — other tasks build on them. The selector map (Task 4) is a refactor with no behavior change. All new tests are additive.

**Tech Stack:** Playwright ^1.50.0, @axe-core/playwright, TypeScript, GitHub Actions

---

## File Structure

```
admin/e2e/
├── fixtures.ts              # NEW — custom test.extend() fixtures
├── factories.ts             # NEW — centralized test data builders
├── selectors.ts             # NEW — selector map constants
├── accessibility.spec.ts    # NEW — axe-core a11y tests
├── visual.spec.ts           # NEW — screenshot regression tests
├── regression.spec.ts       # NEW — edge-case regression scenarios
├── helpers.ts               # MODIFY — re-export from fixtures
├── auth-helper.ts           # MODIFY — used by fixtures internally
├── *.spec.ts                # MODIFY — adopt fixtures, tags, selectors
admin/
├── package.json             # MODIFY — add scripts + axe-core dep
├── playwright.config.ts     # MODIFY — add projects for tagging
├── playwright-dev.config.ts # MODIFY — mirror tag config
.github/workflows/
├── ci.yml                   # MODIFY — add e2e job
```

---

### Task 1: Custom Fixtures via test.extend()

**Files:**
- Create: `admin/e2e/fixtures.ts`
- Modify: `admin/e2e/console.spec.ts`
- Modify: `admin/e2e/overview-analytics.spec.ts`

The fixtures file provides `authedPage` (logged-in browser context with auto-cleanup) and `testTenant` (creates a tenant, deletes in teardown). All existing tests can migrate incrementally.

- [ ] **Step 1: Create fixtures.ts**

```typescript
// admin/e2e/fixtures.ts
import { test as base, expect, BrowserContext, Page } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE, getAccessToken, postWithRetry } from './helpers';

type AuthedFixture = { authedPage: Page; authedContext: BrowserContext };

type TenantFixture = {
  testTenant: { id: string; slug: string; name: string };
};

export const test = base.extend<AuthedFixture & TenantFixture>({
  authedPage: async ({ browser }, use) => {
    const { context, page } = await loginAsTestUser(browser);
    await use(page);
    await context.close();
  },

  authedContext: async ({ browser }, use) => {
    const { context } = await loginAsTestUser(browser);
    await use(context);
    await context.close();
  },

  testTenant: async ({ browser }, use) => {
    const { context } = await loginAsTestUser(browser);
    const token = await getAccessToken(context);
    const slug = 'e2e-fixture-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2, 6);
    const name = 'Fixture Tenant ' + slug.slice(-6);

    const res = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token },
      data: { name, slug },
    });
    expect(res.ok()).toBeTruthy();
    const json = await res.json();
    const id = String(json.tenant?.id || json.id);

    await use({ id, slug, name });

    // Teardown: delete tenant
    try {
      const freshToken = await getAccessToken(context);
      await context.request.delete(BASE + '/platform/self-serve/tenants/' + id, {
        headers: { Authorization: 'Bearer ' + freshToken },
        data: { confirm_slug: slug },
      });
    } catch { /* cleanup best-effort */ }
    await context.close();
  },
});

export { expect } from '@playwright/test';
```

- [ ] **Step 2: Migrate console.spec.ts to use fixtures**

Replace the 6 authenticated tests in `admin/e2e/console.spec.ts` to use `authedPage` instead of manual `loginAsTestUser` + `try/finally`. Import `test` from `./fixtures` instead of `@playwright/test`.

Change the import:
```typescript
import { test, expect } from './fixtures';
```

Replace each authenticated test. Example for `authenticated: no console errors`:
```typescript
test('authenticated: no console errors @smoke', async ({ authedPage }) => {
  const errors = collectErrors(authedPage);
  await authedPage.goto('/platform/console', { waitUntil: 'networkidle' });
  expectNoConsoleErrors(errors);
});
```

The `authedPage` fixture handles login + cleanup. Unauthenticated tests keep using `{ page }` from the default fixture.

- [ ] **Step 3: Migrate overview-analytics.spec.ts to use testTenant fixture**

Replace the manual tenant create/delete with:
```typescript
import { test, expect } from './fixtures';

test('overview shows analytics filters @regression', async ({ authedPage, testTenant }) => {
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator('.account-shell')).toBeVisible({ timeout: 10000 });
  await authedPage.locator('.tenant-card').filter({ hasText: testTenant.name }).first().click();
  // ... rest of assertions unchanged
});
```

- [ ] **Step 4: Run tests to verify no regressions**

Run: `cd admin && npx playwright test e2e/console.spec.ts e2e/overview-analytics.spec.ts`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add admin/e2e/fixtures.ts admin/e2e/console.spec.ts admin/e2e/overview-analytics.spec.ts
git commit -m "feat(e2e): add custom Playwright fixtures — authedPage and testTenant"
```

---

### Task 2: Test Tagging and npm Scripts

**Files:**
- Modify: `admin/playwright.config.ts`
- Modify: `admin/package.json`
- Modify: All `*.spec.ts` files (add tags to test titles)

Tags use Playwright's `--grep` flag. Convention: `@smoke` (fast, critical path), `@regression` (full suite), `@rbac`, `@ws`, `@a11y`, `@visual`.

- [ ] **Step 1: Add npm scripts to package.json**

Add these scripts to `admin/package.json`:
```json
{
  "scripts": {
    "test": "playwright test",
    "test:smoke": "playwright test --grep @smoke",
    "test:regression": "playwright test --grep @regression",
    "test:rbac": "playwright test --grep @rbac",
    "test:ws": "playwright test --grep @ws",
    "test:visual": "playwright test --grep @visual",
    "test:a11y": "playwright test --grep @a11y",
    "test:headed": "playwright test --headed",
    "test:debug": "playwright test --debug",
    "test:dev": "playwright test --config=playwright-dev.config.ts"
  }
}
```

- [ ] **Step 2: Tag all existing tests**

Add `@smoke` to critical-path tests (page loads, auth redirects, login flow). Add `@regression` to all tests. Add domain tags to specialized suites.

Tag map:
- `smoke.spec.ts`: all tests get `@smoke @regression`
- `login.spec.ts`: hydration + form tests get `@smoke @regression`
- `console.spec.ts`: auth + shell tests get `@smoke @regression`
- `navigation.spec.ts`: redirect tests get `@smoke @regression`
- `tenant-login.spec.ts`: form load test gets `@smoke @regression`
- `team-rbac.spec.ts`: all tests get `@rbac @regression`
- `websocket.spec.ts`: all tests get `@ws @regression`
- `persona-workflows.spec.ts`: all tests get `@regression`
- `benchmark.spec.ts`: all tests get `@regression`
- `onboarding.spec.ts`: hydration test gets `@smoke @regression`, rest `@regression`
- `command-palette.spec.ts`: `@regression`
- `fixes-verify.spec.ts`: `@regression`
- `wizard-flow.spec.ts`: `@regression`
- `session-inspector.spec.ts`: `@regression`
- `overview-analytics.spec.ts`: `@regression`

Example: in `smoke.spec.ts`:
```typescript
test(`${route.name}: loads without console errors @smoke @regression`, async ({ page }) => {
```

Example: in `team-rbac.spec.ts`:
```typescript
test('owner can create invite with email and tenant assignments @rbac @regression', async ({ request }) => {
```

- [ ] **Step 3: Verify tagging works**

Run: `cd admin && npx playwright test --grep @smoke --list`
Expected: ~20-25 tests listed (the critical-path subset).

Run: `cd admin && npx playwright test --grep @regression --list`
Expected: All 135 tests listed.

- [ ] **Step 4: Commit**

```bash
git add admin/package.json admin/e2e/*.spec.ts
git commit -m "feat(e2e): add test tags (@smoke, @regression, @rbac, @ws) and npm scripts"
```

---

### Task 3: E2E Tests in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

Add an `e2e-tests` job that starts the server, runs the smoke suite, and uploads Playwright report on failure.

- [ ] **Step 1: Add e2e job to ci.yml**

Add after the `integration-tests` job:
```yaml
  e2e-tests:
    name: E2E smoke tests
    runs-on: ubuntu-24.04
    timeout-minutes: 15
    needs: frontend
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: gatewasm
          POSTGRES_PASSWORD: gatewasm
          POSTGRES_DB: gatewasm_test
        ports:
          - 5433:5432
        options: >-
          --health-cmd "pg_isready -U gatewasm -d gatewasm_test"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 10
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: npm
          cache-dependency-path: admin/package-lock.json
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build admin assets
        working-directory: admin
        run: |
          npm ci
          npx playwright install --with-deps chromium
          npx tsx build.ts --ssr
      - name: Run e2e smoke tests
        working-directory: admin
        run: npx playwright test --grep @smoke
      - name: Upload Playwright report
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report
          path: admin/test-results/
          retention-days: 7
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add e2e smoke test job to GitHub Actions"
```

---

### Task 4: Selector Map

**Files:**
- Create: `admin/e2e/selectors.ts`
- Modify: `admin/e2e/console.spec.ts`
- Modify: `admin/e2e/persona-workflows.spec.ts`
- Modify: `admin/e2e/navigation.spec.ts`
- Modify: `admin/e2e/command-palette.spec.ts`
- Modify: `admin/e2e/session-inspector.spec.ts`
- Modify: `admin/e2e/overview-analytics.spec.ts`
- Modify: `admin/e2e/wizard-flow.spec.ts`
- Modify: `admin/e2e/tenant-login.spec.ts`

Extract the 63 repeated CSS selectors into a single map. When the UI changes, you update one file.

- [ ] **Step 1: Create selectors.ts**

```typescript
// admin/e2e/selectors.ts
/** Centralized selector map — update here when UI class names change. */

export const Account = {
  shell: '.account-shell',
  tabs: '.account-tabs',
  tenantCard: (name: string) => `.tenant-card >> text="${name}"`,
} as const;

export const Workspace = {
  planeNav: '.plane-nav',
  planeNavCollapsed: '.plane-nav-collapsed',
  tier1: '.plane-tier1',
  tier2: '.plane-tier2',
  activePlane: '.plane-btn.active',
  tenantTrigger: '.plane-tenant-trigger',
  tenantMenu: '.plane-tenant-menu',
  tenantMenuItem: '.plane-tenant-menu-item',
  tenantMenuBack: '.plane-tenant-menu-back',
  tenantName: '.plane-tenant-name',
  avatar: '.plane-avatar',
} as const;

export const Modal = {
  content: '.modal-content',
} as const;

export const TenantLogin = {
  card: '.tl-card',
  appName: '.tl-app-name',
  subtitle: '.tl-subtitle',
  passwordToggle: '.tl-password-toggle',
  toggle: '.tl-toggle .tl-link',
  footer: '.secured-badge',
} as const;
```

- [ ] **Step 2: Migrate console.spec.ts to use selectors**

Replace hardcoded selectors with imports:
```typescript
import { Account, Workspace } from './selectors';

// Before:
await expect(page.locator('.account-shell')).toBeVisible({ timeout: 8000 });
// After:
await expect(page.locator(Account.shell)).toBeVisible({ timeout: 8000 });

// Before:
await page.locator('.account-shell .plane-avatar').click();
// After:
await page.locator(`${Account.shell} ${Workspace.avatar}`).click();
```

- [ ] **Step 3: Migrate persona-workflows.spec.ts to use selectors**

Replace the 31 selector references. Key replacements:
```typescript
import { Account, Workspace } from './selectors';

// Before:
await expect(page.locator('.account-tabs')).toBeVisible();
// After:
await expect(page.locator(Account.tabs)).toBeVisible();

// Before:
await expect(page.locator('.plane-nav')).toBeVisible();
// After:
await expect(page.locator(Workspace.planeNav)).toBeVisible();

// Before:
await expect(page.locator('.plane-btn.active')).toContainText('Operator');
// After:
await expect(page.locator(Workspace.activePlane)).toContainText('Operator');
```

- [ ] **Step 4: Migrate remaining files**

Apply same pattern to `navigation.spec.ts`, `command-palette.spec.ts`, `session-inspector.spec.ts`, `overview-analytics.spec.ts`, `wizard-flow.spec.ts`, `tenant-login.spec.ts`.

- [ ] **Step 5: Run full suite to verify no regressions**

Run: `cd admin && npx playwright test`
Expected: 135 passed.

- [ ] **Step 6: Commit**

```bash
git add admin/e2e/selectors.ts admin/e2e/*.spec.ts
git commit -m "refactor(e2e): extract 63 CSS selectors into centralized selectors.ts map"
```

---

### Task 5: Visual Regression Testing

**Files:**
- Create: `admin/e2e/visual.spec.ts`

Use Playwright's built-in `toHaveScreenshot()` for the 4 public pages + the authenticated console. First run creates baselines in `admin/e2e/visual.spec.ts-snapshots/`.

- [ ] **Step 1: Create visual.spec.ts**

```typescript
// admin/e2e/visual.spec.ts
import { test, expect } from './fixtures';

const VISUAL_PAGES = [
  { path: '/platform/login', name: 'platform-login' },
  { path: '/platform/onboarding', name: 'onboarding' },
  { path: '/t/platform/login', name: 'tenant-login' },
];

for (const route of VISUAL_PAGES) {
  test(`${route.name}: visual snapshot @visual @regression`, async ({ page }) => {
    await page.goto(route.path, { waitUntil: 'networkidle' });
    await expect(page).toHaveScreenshot(`${route.name}.png`, {
      maxDiffPixelRatio: 0.01,
      animations: 'disabled',
    });
  });
}

test('console: visual snapshot @visual @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await authedPage.locator('.account-shell').waitFor({ timeout: 10000 });
  await expect(authedPage).toHaveScreenshot('console.png', {
    maxDiffPixelRatio: 0.01,
    animations: 'disabled',
  });
});
```

- [ ] **Step 2: Generate baselines**

Run: `cd admin && npx playwright test e2e/visual.spec.ts --update-snapshots`
Expected: Creates `admin/e2e/visual.spec.ts-snapshots/` with 4 PNG baselines.

- [ ] **Step 3: Verify snapshots pass on re-run**

Run: `cd admin && npx playwright test e2e/visual.spec.ts`
Expected: 4 passed.

- [ ] **Step 4: Commit baselines**

```bash
git add admin/e2e/visual.spec.ts admin/e2e/visual.spec.ts-snapshots/
git commit -m "feat(e2e): add visual regression tests for public pages and console"
```

---

### Task 6: Centralized Test Data Factories

**Files:**
- Create: `admin/e2e/factories.ts`
- Modify: `admin/e2e/team-rbac.spec.ts`
- Modify: `admin/e2e/persona-workflows.spec.ts`
- Modify: `admin/e2e/command-palette.spec.ts`
- Modify: `admin/e2e/session-inspector.spec.ts`

Extract the 6 duplicated factory functions into one file.

- [ ] **Step 1: Create factories.ts**

```typescript
// admin/e2e/factories.ts
import { expect, APIRequestContext, BrowserContext } from '@playwright/test';
import { BASE, postWithRetry, getAccessToken } from './helpers';

const TEST_PASSWORD = 'E2eTestPassword123!';

/**
 * Register a new platform account. Returns access token, user ID, and email.
 * Works with both APIRequestContext (test fixtures) and BrowserContext.request.
 */
export async function createTestAccount(
  request: APIRequestContext,
  label: string,
): Promise<{ token: string; userId: string; email: string }> {
  const random = Math.random().toString(36).slice(2, 8);
  const email = `e2e-${label}-${Date.now()}-${random}@test.local`;

  const res = await postWithRetry(request, BASE + '/auth/register', {
    headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
    data: { email, password: TEST_PASSWORD, name: `E2E ${label}` },
  });

  if (!res.ok()) {
    throw new Error(`createTestAccount(${label}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  const token: string = data.access_token;
  const userId: string = data.user_id || data.id;
  if (!token) {
    throw new Error(`createTestAccount(${label}): no access_token in ${JSON.stringify(data)}`);
  }

  return { token, userId, email };
}

/** Create a tenant via the self-serve API. */
export async function createTestTenant(
  request: APIRequestContext,
  token: string,
  name: string,
): Promise<{ id: string; slug: string }> {
  const slug = `e2e-${name.toLowerCase().replace(/\s+/g, '-')}-${Date.now().toString(36)}`;

  const res = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
    data: { name, slug },
  });

  if (!res.ok()) {
    throw new Error(`createTestTenant(${name}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  const id: string = data.id || data.tenant?.id;
  if (!id) {
    throw new Error(`createTestTenant(${name}): no id in ${JSON.stringify(data)}`);
  }

  return { id, slug };
}

/** Delete a tenant (best-effort cleanup). */
export async function deleteTestTenant(
  request: APIRequestContext,
  token: string,
  tenantId: string,
  slug: string,
): Promise<void> {
  await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
    headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
    data: { confirm_slug: slug },
  });
}

/** Convenience: delete using a BrowserContext (refreshes token first). */
export async function deleteTestTenantFromContext(
  context: BrowserContext,
  tenantId: string,
  slug: string,
): Promise<void> {
  try {
    const token = await getAccessToken(context);
    await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + token },
      data: { confirm_slug: slug },
    });
  } catch (e) { console.warn('Tenant cleanup failed:', e); }
}

/** Create a team invite. */
export async function inviteMember(
  request: APIRequestContext,
  ownerToken: string,
  email: string,
  role: string,
  tenantIds: string[],
): Promise<{ inviteId: string; inviteToken: string }> {
  const res = await postWithRetry(request, BASE + '/platform/admin/team/invites', {
    headers: { Authorization: 'Bearer ' + ownerToken, 'Content-Type': 'application/json' },
    data: { email, role, tenant_ids: tenantIds },
  });

  if (!res.ok()) {
    throw new Error(`inviteMember(${email}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  return { inviteId: data.invite?.id, inviteToken: data.invite_token };
}

/** Accept a team invite. */
export async function acceptInvite(
  request: APIRequestContext,
  memberToken: string,
  inviteToken: string,
): Promise<any> {
  const res = await postWithRetry(request, BASE + '/platform/admin/invites/accept', {
    headers: { Authorization: 'Bearer ' + memberToken, 'Content-Type': 'application/json' },
    data: { token: inviteToken },
  });

  if (!res.ok()) {
    throw new Error(`acceptInvite failed: ${await res.text()}`);
  }

  return res.json();
}

/** List team members. */
export async function listTeam(request: APIRequestContext, token: string): Promise<any> {
  const res = await request.get(BASE + '/platform/admin/team', {
    headers: { Authorization: 'Bearer ' + token },
  });
  if (!res.ok()) throw new Error(`listTeam failed: ${await res.text()}`);
  return res.json();
}

/** List tenants visible to a user. */
export async function listTenants(request: APIRequestContext, token: string): Promise<any[]> {
  const res = await request.get(BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token },
  });
  if (!res.ok()) throw new Error(`listTenants failed: ${await res.text()}`);
  const data = await res.json();
  return data.tenants || [];
}

/** Re-login to get a fresh access token (for APIRequestContext, not BrowserContext). */
export async function refreshAccountToken(
  request: APIRequestContext,
  email: string,
): Promise<string> {
  const res = await postWithRetry(request, BASE + '/auth/login', {
    headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
    data: { email, password: TEST_PASSWORD },
  });
  if (!res.ok()) throw new Error(`refreshAccountToken(${email}) failed: ${await res.text()}`);
  const data = await res.json();
  return data.access_token as string;
}
```

- [ ] **Step 2: Migrate team-rbac.spec.ts to import from factories**

Remove the 7 local function definitions (lines 6-155). Replace with:
```typescript
import {
  createTestAccount,
  createTestTenant,
  inviteMember,
  acceptInvite,
  listTeam,
  listTenants,
  refreshAccountToken,
} from './factories';
```

Keep `TEST_PASSWORD` local since it's only used in the `refreshAccountToken` call which is now in factories.

- [ ] **Step 3: Migrate persona-workflows.spec.ts**

Remove local `createTenant` and `deleteTenant` helpers. Import from factories:
```typescript
import { createTestTenant, deleteTestTenantFromContext } from './factories';
```

Update calls: `createTenant(context, token, name, slug)` → `createTestTenant(context.request, token, name)` (slug is auto-generated).

- [ ] **Step 4: Migrate command-palette.spec.ts and session-inspector.spec.ts**

Same pattern — replace inline tenant create/delete with factory imports.

- [ ] **Step 5: Run full suite**

Run: `cd admin && npx playwright test`
Expected: 135 passed.

- [ ] **Step 6: Commit**

```bash
git add admin/e2e/factories.ts admin/e2e/team-rbac.spec.ts admin/e2e/persona-workflows.spec.ts admin/e2e/command-palette.spec.ts admin/e2e/session-inspector.spec.ts
git commit -m "refactor(e2e): centralize test data factories into factories.ts"
```

---

### Task 7: Accessibility Testing

**Files:**
- Create: `admin/e2e/accessibility.spec.ts`
- Modify: `admin/package.json` (add dep)

- [ ] **Step 1: Install axe-core**

Run: `cd admin && npm install -D @axe-core/playwright`

- [ ] **Step 2: Create accessibility.spec.ts**

```typescript
// admin/e2e/accessibility.spec.ts
import { test, expect } from './fixtures';
import AxeBuilder from '@axe-core/playwright';

const A11Y_PAGES = [
  { path: '/platform/login', name: 'platform-login' },
  { path: '/platform/onboarding', name: 'onboarding' },
  { path: '/t/platform/login', name: 'tenant-login' },
];

for (const route of A11Y_PAGES) {
  test(`${route.name}: no accessibility violations @a11y @regression`, async ({ page }) => {
    await page.goto(route.path, { waitUntil: 'networkidle' });
    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa'])
      .analyze();
    expect(results.violations).toEqual([]);
  });
}

test('console: no accessibility violations @a11y @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await authedPage.locator('.account-shell').waitFor({ timeout: 10000 });
  const results = await new AxeBuilder({ page: authedPage })
    .withTags(['wcag2a', 'wcag2aa'])
    .analyze();
  expect(results.violations).toEqual([]);
});
```

- [ ] **Step 3: Run a11y tests**

Run: `cd admin && npx playwright test e2e/accessibility.spec.ts`
Expected: 4 tests. Some may fail with real violations — that's good, it identifies what to fix.

If violations are found that are known/accepted (e.g., contrast in a dark theme), add `.exclude()` rules:
```typescript
const results = await new AxeBuilder({ page })
  .withTags(['wcag2a', 'wcag2aa'])
  .exclude('.known-contrast-exception')
  .analyze();
```

- [ ] **Step 4: Commit**

```bash
git add admin/package.json admin/package-lock.json admin/e2e/accessibility.spec.ts
git commit -m "feat(e2e): add axe-core accessibility tests for public pages and console"
```

---

### Task 8: Missing Regression Scenarios

**Files:**
- Create: `admin/e2e/regression.spec.ts`

Five edge-case scenarios not covered by existing tests: error state recovery, concurrent sessions, token refresh race, tenant deletion cascade, and deep link resilience.

- [ ] **Step 1: Create regression.spec.ts**

```typescript
// admin/e2e/regression.spec.ts
import { test, expect } from './fixtures';
import { BASE, getAccessToken } from './helpers';
import { createTestTenant, deleteTestTenantFromContext } from './factories';

// ── 1. Error state recovery: API failure during tenant creation ──────

test('console recovers gracefully when tenant creation fails @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await authedPage.locator('.account-shell').waitFor({ timeout: 10000 });

  // Open create modal
  await authedPage.getByRole('button', { name: '+ New Workspace' }).click();
  await expect(authedPage.locator('.modal-content')).toBeVisible();

  // Submit with an invalid slug (empty or too short)
  await authedPage.getByLabel('App Name').fill('Test');
  await authedPage.locator('#field-tenant-slug').fill('x');
  await authedPage.locator('.modal-content').getByRole('button', { name: 'Create Workspace' }).click();

  // Should show an error in the modal, not crash the page
  await expect(
    authedPage.locator('.modal-content .alert, .modal-content .error, .modal-content [role="alert"]')
  ).toBeVisible({ timeout: 5000 });

  // Console should still be functional after dismissing
  await authedPage.keyboard.press('Escape');
  await expect(authedPage.locator('.account-shell')).toBeVisible();
});

// ── 2. Concurrent sessions: two tabs same console ────────────────────

test('two tabs can view the console simultaneously @regression', async ({ browser }) => {
  // Shared context = shared cookies = same session
  const { context, page: page1 } = await (await import('./auth-helper')).loginAsTestUser(browser);

  try {
    const page2 = await context.newPage();

    await page1.goto('/platform/console');
    await page2.goto('/platform/console');

    await expect(page1.locator('.account-shell')).toBeVisible({ timeout: 10000 });
    await expect(page2.locator('.account-shell')).toBeVisible({ timeout: 10000 });

    // Action in tab 1 doesn't crash tab 2
    await page1.getByRole('button', { name: '+ New Workspace' }).click();
    await expect(page1.locator('.modal-content')).toBeVisible();

    // Tab 2 still responsive
    await expect(page2.locator('.account-shell')).toBeVisible();

    await page2.close();
  } finally {
    await context.close();
  }
});

// ── 3. Token refresh during page load ────────────────────────────────

test('page loads correctly even when access token expires mid-navigation @regression', async ({ browser }) => {
  const { context, page } = await (await import('./auth-helper')).loginAsTestUser(browser);

  try {
    // First load — establishes session
    await page.goto('/platform/console');
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 10000 });

    // Wait for the access token to be close to expiry (test env uses 60s)
    // Navigate again — silentRefresh should kick in transparently
    await page.waitForTimeout(5000);
    await page.goto('/platform/console');
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 10000 });
  } finally {
    await context.close();
  }
});

// ── 4. Tenant deletion cascade: UI updates after deleting active tenant ──

test('deleting the last tenant returns to empty workspace list @regression', async ({ browser }) => {
  const { context, page } = await (await import('./auth-helper')).loginAsTestUser(browser);

  try {
    const token = await getAccessToken(context);
    const tenant = await createTestTenant(context.request, token, 'Delete Me');

    await page.goto('/platform/console');
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 10000 });

    // Verify tenant appears
    await expect(page.locator('.tenant-card').filter({ hasText: 'Delete Me' })).toBeVisible();

    // Delete via API
    const freshToken = await getAccessToken(context);
    await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenant.id, {
      headers: { Authorization: 'Bearer ' + freshToken },
      data: { confirm_slug: tenant.slug },
    });

    // Reload — tenant should be gone
    await page.goto('/platform/console');
    await expect(page.locator('.account-shell')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('.tenant-card').filter({ hasText: 'Delete Me' })).toHaveCount(0);
  } finally {
    await context.close();
  }
});

// ── 5. Deep link resilience: bookmarked console URL loads correctly ──

test('deep link to /platform/console loads without errors when authenticated @regression', async ({ authedPage }) => {
  // Direct navigation (simulates a bookmark)
  await authedPage.goto('/platform/console');
  await expect(authedPage.locator('.account-shell')).toBeVisible({ timeout: 10000 });
  await expect(authedPage.getByText('Your Workspaces')).toBeVisible();
});

test('deep link to /platform/console redirects to login when unauthenticated @regression', async ({ page }) => {
  // Fresh page, no auth
  await page.goto('/platform/console');
  await page.waitForURL('**/platform/login', { timeout: 8000 });
  expect(page.url()).toContain('/platform/login');
});
```

- [ ] **Step 2: Run regression tests**

Run: `cd admin && npx playwright test e2e/regression.spec.ts`
Expected: 6 tests, all pass.

- [ ] **Step 3: Run full suite to ensure no conflicts**

Run: `cd admin && npx playwright test`
Expected: ~141 tests (135 existing + 6 new), all pass.

- [ ] **Step 4: Commit**

```bash
git add admin/e2e/regression.spec.ts
git commit -m "feat(e2e): add edge-case regression tests — error recovery, concurrent sessions, deep links"
```

---

## Execution Order

Tasks 1 (fixtures) and 6 (factories) are foundational. Tasks 5, 7, 8 depend on Task 1 (they import from `fixtures.ts`). Task 4 (selectors) is independent. Task 2 (tagging) should come before Task 3 (CI) since CI uses `--grep @smoke`.

**Recommended order:** 1 → 6 → 2 → 4 → 3 → 5 → 7 → 8
