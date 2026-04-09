import { test as base, expect, BrowserContext, Page } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE, getAccessToken } from './helpers';

type AuthedFixture = { authedPage: Page; authedContext: BrowserContext };
type TenantFixture = { testTenant: { id: string; slug: string; name: string } };

export const test = base.extend<AuthedFixture & TenantFixture>({
  // Single logged-in browser context shared by authedPage and testTenant.
  authedContext: async ({ browser }, use) => {
    const { context } = await loginAsTestUser(browser);
    await use(context);
    await context.close();
  },

  // A Page inside the shared authedContext.
  authedPage: async ({ authedContext }, use) => {
    const page = authedContext.pages()[0] || await authedContext.newPage();
    await use(page);
  },

  // Creates a tenant via API using the shared authedContext, deletes on teardown.
  testTenant: async ({ authedContext }, use) => {
    const token = await getAccessToken(authedContext);
    const slug = 'e2e-fixture-' + Date.now().toString(36) + '-' + Math.random().toString(36).slice(2, 6);
    const name = 'Fixture Tenant ' + slug.slice(-6);

    const res = await authedContext.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token },
      data: { name, slug },
    });
    expect(res.ok()).toBeTruthy();
    const json = await res.json();
    const id = String(json.tenant?.id || json.id);

    // Bypass onboarding so tests that click the tenant card land in the
    // workspace shell instead of the wizard. The console gates the workspace
    // on `onboarding_completed` (commit ae14675).
    const patchRes = await authedContext.request.patch(
      BASE + '/platform/self-serve/tenants/' + id,
      {
        headers: { Authorization: 'Bearer ' + token },
        data: { onboarding_completed: true, onboarding_step: 7 },
      },
    );
    expect(patchRes.ok()).toBeTruthy();

    await use({ id, slug, name });

    try {
      const freshToken = await getAccessToken(authedContext);
      await authedContext.request.delete(BASE + '/platform/self-serve/tenants/' + id, {
        headers: { Authorization: 'Bearer ' + freshToken },
        data: { confirm_slug: slug },
      });
    } catch { /* cleanup best-effort */ }
  },
});

export { expect } from '@playwright/test';
