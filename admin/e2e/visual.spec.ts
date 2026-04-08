/**
 * Visual Regression Tests
 *
 * Screenshot comparisons for critical pages. First run creates baselines
 * in visual.spec.ts-snapshots/. Update baselines with --update-snapshots.
 */
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
