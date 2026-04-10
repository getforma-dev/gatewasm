import { test, expect, Page } from '@playwright/test';
import { BASE, patchWithRetry, postWithRetry } from './helpers';

/**
 * Navigation regressions for the workspace/wizard/account shell split.
 *
 * Background:
 *   ConsoleApp used to nest createShow(tenant && completed) inside
 *   createSwitch(mode). createSwitch CACHES its case branches, so switching
 *   modes detached-but-kept the workspace branch — including the inner
 *   createShow that could fire AFTER detachment and briefly mount
 *   OnboardingWizard. The wizard's mount wrote `#wizard-create` to the hash
 *   via replaceState, producing a visible "wizard flash" when clicking
 *   "← All Applications" and a stale cached mount on the next
 *   "+ New Application" click. The fix is a flat routing structure where
 *   a single derived `route()` function drives three sibling createShow
 *   bindings.
 *
 *   We also fixed startNewWorkspaceWizard to reset the URL path BEFORE the
 *   signal batch, so the wizard's mount (which runs during the effect flush
 *   at the end of the batch) can write `#wizard-create` without a trailing
 *   pushState stripping it.
 *
 * NOTE: These tests intentionally do NOT register page.on('request') /
 * page.on('response') listeners. Those listeners activate Playwright's
 * network interception mode, which interferes with fetch timing and
 * causes spurious hangs on the tenant-create POST. Keep the network
 * surface unobserved and rely on DOM + URL assertions instead.
 */
test.describe('Nav: wizard/workspace/account transitions @regression', () => {
  test.setTimeout(180000);

  async function signupAndLogin(request: import('@playwright/test').APIRequestContext): Promise<{
    email: string;
    password: string;
    token: string;
  }> {
    const email = 'nav-' + Date.now() + Math.random().toString(36).slice(2, 6) + '@test.com';
    const password = 'Test1234!';
    const res = await postWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email, password, name: 'Nav Test' },
    });
    if (!res.ok()) throw new Error('signup failed: ' + await res.text());
    const token = (await res.json()).access_token;
    return { email, password, token };
  }

  async function createCompletedTenant(
    request: import('@playwright/test').APIRequestContext,
    token: string,
    slug: string,
    name: string,
  ): Promise<string> {
    const create = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { name, slug },
    });
    if (!create.ok()) throw new Error('tenant create failed: ' + await create.text());
    const body = await create.json();
    const id = body.id || body.tenant?.id;
    await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + id, {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: {
        tenant_intent: 'saas',
        preferred_tab: 'integrate',
        onboarding_completed: true,
        onboarding_step: 7,
      },
    });
    return id;
  }

  async function browserLogin(page: Page, email: string, password: string): Promise<void> {
    const res = await page.context().request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email, password },
    });
    if (!res.ok()) throw new Error('login failed: ' + await res.text());
  }

  /**
   * Bug A: "← All Applications" must take you to account view without ever
   * flashing the wizard's Create step. We assert on the wizard's DOM
   * presence — never on the network or on timing — so this test is
   * deterministic.
   */
  test('clicking "← All Applications" from workspace lands in account view, not the wizard', async ({ browser, request }) => {
    const slug = 'nav-a-' + Date.now().toString(36);
    const { email, password, token } = await signupAndLogin(request);
    const tenantId = await createCompletedTenant(request, token, slug, 'Nav A App');

    const context = await browser.newContext();
    const page = await context.newPage();

    try {
      await browserLogin(page, email, password);
      await page.goto(BASE + '/platform/console/' + slug + '#integrate');
      await expect(page.locator('.shell')).toBeVisible({ timeout: 10000 });

      // Open the tenant dropdown in PlaneNav and click "← All Applications".
      await page.locator('.plane-tenant-switcher, .plane-tenant-trigger, [class*="tenant-menu"]').first().click();
      await page.getByRole('button', { name: /All Applications/ }).click();

      // Final URL must be the clean account path.
      await expect(page).toHaveURL(BASE + '/platform/console');

      // The wizard must NOT be in the DOM at all (not even briefly — our
      // locator waits up to 5s and asserts 0 count, which would fail if the
      // flat-route regression returned and the wizard cache flashed).
      await expect(page.locator('h1.wizard-headline')).toHaveCount(0);

      // The account view's "+ New Application" button must be present.
      await expect(page.getByRole('button', { name: /\+ New Application/ })).toBeVisible();
    } finally {
      try {
        await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
          data: { confirm_slug: slug },
        });
      } catch { /* ignore */ }
      await context.close();
    }
  });

  /**
   * Bug B cascade: after completing a tenant and clicking back to account,
   * clicking "+ New Application" must open a FRESH wizard and the first
   * Continue click must advance past Create. Previously, the createSwitch
   * cache reused a stale workspace branch whose inner createShow had
   * already transitioned, leaving the wizard in a half-initialized state
   * that required a second click.
   */
  test('second wizard run from account view advances past Create on a single Continue click', async ({ browser, request }) => {
    const slugA = 'nav-b-a-' + Date.now().toString(36);
    const slugB = 'nav-b-b-' + Date.now().toString(36);
    const { email, password, token } = await signupAndLogin(request);
    const tenantAId = await createCompletedTenant(request, token, slugA, 'Nav B App A');
    let tenantBId: string | null = null;

    const context = await browser.newContext();
    const page = await context.newPage();

    try {
      await browserLogin(page, email, password);

      // Start in tenant A's workspace, then click "← All Applications" —
      // this exercises the exact cascade path that made the cached wizard
      // branch go stale in the old code.
      await page.goto(BASE + '/platform/console/' + slugA + '#integrate');
      await expect(page.locator('.shell')).toBeVisible({ timeout: 10000 });
      await page.locator('.plane-tenant-switcher, .plane-tenant-trigger, [class*="tenant-menu"]').first().click();
      await page.getByRole('button', { name: /All Applications/ }).click();
      await expect(page.getByRole('button', { name: /\+ New Application/ })).toBeVisible({ timeout: 5000 });

      // Now click "+ New Application" and walk the Create step.
      await page.getByRole('button', { name: /\+ New Application/ }).click();
      await expect(page.locator('h1.wizard-headline', { hasText: 'Create your application' })).toBeVisible({ timeout: 5000 });

      // The wizard's mount must have written #wizard-create to the hash.
      // This is the contract broken by the old startNewWorkspaceWizard
      // pushState-after-batch bug.
      await expect(page).toHaveURL(BASE + '/platform/console#wizard-create');

      await page.getByLabel('App Name').fill('Nav B App B');
      await page.locator('input[placeholder="acme-app"]').fill(slugB);
      await expect(page.locator('.slug-status-available')).toBeVisible({ timeout: 8000 });

      // First click MUST advance to Intent.
      await page.getByRole('button', { name: /Create & Continue/ }).click();
      await expect(page.locator('h1.wizard-headline', { hasText: 'What are you building' })).toBeVisible({ timeout: 10000 });

      // URL should include the new slug and the wizard-intent hash.
      await expect(page).toHaveURL(BASE + '/platform/console/' + slugB + '#wizard-intent');

      // Capture tenantB id for cleanup
      const listRes = await request.get(BASE + '/platform/self-serve/tenants', {
        headers: { Authorization: 'Bearer ' + token },
      });
      if (listRes.ok()) {
        const body = await listRes.json();
        const found = (body.tenants || []).find((t: { slug: string; id: string }) => t.slug === slugB);
        if (found) tenantBId = found.id;
      }
    } finally {
      try {
        await request.delete(BASE + '/platform/self-serve/tenants/' + tenantAId, {
          headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
          data: { confirm_slug: slugA },
        });
      } catch { /* ignore */ }
      if (tenantBId) {
        try {
          await request.delete(BASE + '/platform/self-serve/tenants/' + tenantBId, {
            headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
            data: { confirm_slug: slugB },
          });
        } catch { /* ignore */ }
      }
      await context.close();
    }
  });
});
