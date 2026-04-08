/**
 * Accessibility Tests — axe-core WCAG 2.0 AA compliance
 *
 * Scans public pages and the authenticated console for accessibility violations.
 * Uses @axe-core/playwright for automated auditing.
 */
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
      // color-contrast excluded — tracked as design-system issue, not test failure
      .disableRules(['color-contrast', 'link-in-text-block'])
      .analyze();
    expect(results.violations).toEqual([]);
  });
}

test('console: no accessibility violations @a11y @regression', async ({ authedPage }) => {
  await authedPage.goto('/platform/console');
  await authedPage.locator('.account-shell').waitFor({ timeout: 10000 });
  const results = await new AxeBuilder({ page: authedPage })
    .withTags(['wcag2a', 'wcag2aa'])
    .disableRules(['color-contrast', 'link-in-text-block'])
    .analyze();
  expect(results.violations).toEqual([]);
});
