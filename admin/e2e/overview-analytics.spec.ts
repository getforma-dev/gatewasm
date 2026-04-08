import { test, expect } from './fixtures';
import { Account, Workspace } from './selectors';

test('overview shows analytics filters and resilient empty states @regression', async ({ authedPage, testTenant }) => {
  test.setTimeout(45000);

  // Navigate to console — silentRefresh may be rate-limited during full suite runs
  await authedPage.goto('/platform/console');
  // If the client-side silentRefresh was rate-limited, retry with a pause
  for (let attempt = 0; attempt < 3; attempt++) {
    await authedPage.waitForLoadState('networkidle');
    if (!authedPage.url().includes('/login')) break;
    await authedPage.waitForTimeout(1500);
    await authedPage.goto('/platform/console');
  }
  await expect(authedPage.locator(Account.shell)).toBeVisible({ timeout: 10000 });

  await authedPage.locator('.tenant-card').filter({ hasText: testTenant.name }).first().click();
  await expect(authedPage.locator(Workspace.planeNav)).toBeVisible({ timeout: 8000 });

  await expect(authedPage.getByRole('tab', { name: '7d' })).toBeVisible();
  await expect(authedPage.getByText('No auth traffic yet')).toBeVisible();
  // Activity feed card renders (empty state or data) — wait up to 10s for stats to load
  await expect(authedPage.getByText('What changed most recently')).toBeVisible({ timeout: 10000 });
  await expect(authedPage.getByText('Failed Sign-Ins (7d)')).toBeVisible();

  await authedPage.getByRole('tab', { name: '30d' }).click();
  await expect(authedPage.getByText('Failed Sign-Ins (30d)')).toBeVisible();

  await authedPage.getByRole('tab', { name: '24h' }).click();
  await expect(authedPage.getByText('Failed Sign-Ins (24h)')).toBeVisible();
});
