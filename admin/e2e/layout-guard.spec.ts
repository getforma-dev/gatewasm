/**
 * Layout guards: no horizontal document overflow on covered routes, and the
 * rail-shell surfaces must render their content region with real geometry.
 *
 * Route coverage grows as layout fixes land; a route joins the list only
 * once it passes at all three widths.
 */
import { test, expect } from './fixtures';
import { BASE, postWithRetry, expectNoLayoutOverflow } from './helpers';
import { TEST_PASSWORD } from './factories';

const WIDTHS = [375, 768, 1280];

const COVERED_ROUTES = [
  '/platform/login',
  '/platform/onboarding',
  '/platform/benchmark',
  '/t/platform/login',
  '/t/platform/login?mode=register',
  '/t/platform/site',
  '/definitely-not-a-page',
];

for (const route of COVERED_ROUTES) {
  for (const width of WIDTHS) {
    test(`no horizontal overflow: ${route} @ ${width}px @regression`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(route, { waitUntil: 'networkidle' });
      await expectNoLayoutOverflow(page);
    });
  }
}

/**
 * The owner dashboard, tenant admin, and tenant dashboard all render
 * `.shell > aside.sidebar-rail + main.main`. In a column-flex shell the
 * 100vh rail leaves zero height for `.main`, so the page shows a sidebar
 * next to nothing. Guard: content has real height and sits BESIDE the
 * rail at desktop widths.
 */
test('rail shell: dashboard content renders beside the sidebar @regression', async ({ browser, testTenant }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = `layout-guard-${Date.now().toString(36)}@test.local`;

  const reg = await postWithRetry(context.request, BASE + '/auth/register', {
    headers: { 'X-Tenant-Slug': testTenant.slug, 'Content-Type': 'application/json' },
    data: { email, password: TEST_PASSWORD, name: 'Layout Guard' },
  });
  expect(reg.ok()).toBeTruthy();

  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(`/t/${testTenant.slug}/dashboard`, { waitUntil: 'networkidle' });
  const main = page.locator('.shell > .main');
  await main.waitFor({ timeout: 10000 });

  const box = await main.boundingBox();
  expect(box, 'main content region has a bounding box').toBeTruthy();
  expect(box!.height, 'content region must have real height').toBeGreaterThan(100);
  expect(box!.x, 'content must sit beside the 256px rail, not under it').toBeGreaterThan(200);

  // Stacked mobile layout: rail on top, content still visible below.
  await page.setViewportSize({ width: 375, height: 800 });
  await page.waitForTimeout(300);
  const mobileBox = await main.boundingBox();
  expect(mobileBox!.height, 'mobile content region must have real height').toBeGreaterThan(100);

  await context.close();
});
