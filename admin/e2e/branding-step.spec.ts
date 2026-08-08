import { test, expect } from '@playwright/test';
import { BASE, patchWithRetry, postWithRetry } from './helpers';

test.describe('Branding step @regression', () => {
  let platformToken: string;
  let platformEmail: string;
  const platformPassword = 'Test1234!';
  let tenantId: string;
  const tenantSlug = 'brand-' + Date.now().toString(36);

  test.beforeAll(async ({ request }) => {
    platformEmail = 'brand-' + Date.now() + '@test.com';
    const signup = await postWithRetry(request, BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword, name: 'Brand E2E' },
    });
    if (!signup.ok()) throw new Error('signup failed: ' + await signup.text());
    platformToken = (await signup.json()).access_token;

    const create = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { name: 'Brand App', slug: tenantSlug },
    });
    if (!create.ok()) throw new Error('tenant create failed: ' + await create.text());
    const body = await create.json();
    tenantId = body.id || body.tenant?.id;

    // Advance to branding step (step index 3, onboarding_step 4)
    await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { tenant_intent: 'saas', signup_mode: 'open', onboarding_step: 4 },
    });
  });

  test.afterAll(async ({ request }) => {
    if (!tenantId) return;
    await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { confirm_slug: tenantSlug },
    });
  });

  async function openBrandingStep(browser: import('@playwright/test').Browser) {
    const context = await browser.newContext();
    const page = await context.newPage();
    const login = await context.request.post(BASE + '/auth/login', {
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      data: { email: platformEmail, password: platformPassword },
    });
    if (!login.ok()) throw new Error('login failed: ' + await login.text());

    await page.goto(BASE + '/platform/console');
    await page.locator('.tenant-card').filter({ hasText: 'Brand App' }).click();
    await expect(page.locator('h1.wizard-headline', { hasText: 'Style your sign-in card' })).toBeVisible({ timeout: 8000 });
    return { page, close: () => context.close() };
  }

  test('font family changes apply to the live preview card', async ({ browser }) => {
    const { page, close } = await openBrandingStep(browser);
    try {
      const card = page.locator('.wizard-pane-right .lp-card');
      const fontSelect = page.locator('.wizard-pane-left select.input');

      // Default: no explicit font-family on the card (System default)
      const initialFont = await card.evaluate((el) => el.style.fontFamily);
      expect(initialFont).toBe('');

      // Select "Inter"
      await fontSelect.selectOption('Inter');
      const interFont = await card.evaluate((el) => el.style.fontFamily);
      expect(interFont).toContain('Inter');

      // Switch to "Poppins"
      await fontSelect.selectOption('Poppins');
      const poppinsFont = await card.evaluate((el) => el.style.fontFamily);
      expect(poppinsFont).toContain('Poppins');

      // Switch back to System default — font-family should be cleared
      await fontSelect.selectOption('System default');
      const resetFont = await card.evaluate((el) => el.style.fontFamily);
      expect(resetFont).toBe('');
    } finally {
      await close();
    }
  });

  test('text size changes apply to the live preview card', async ({ browser }) => {
    const { page, close } = await openBrandingStep(browser);
    try {
      const card = page.locator('.wizard-pane-right .lp-card');
      const sizeButtons = page.locator('.wizard-size-option');

      // Default: Medium (15px)
      await expect(sizeButtons.filter({ hasText: 'Medium' })).toHaveClass(/active/);
      const defaultSize = await card.evaluate((el) => el.style.fontSize);
      expect(defaultSize).toBe('15px');

      // Click Small
      await sizeButtons.filter({ hasText: 'Small' }).click();
      await expect(sizeButtons.filter({ hasText: 'Small' })).toHaveClass(/active/);
      const smallSize = await card.evaluate((el) => el.style.fontSize);
      expect(smallSize).toBe('13px');

      // Click Large
      await sizeButtons.filter({ hasText: 'Large' }).click();
      await expect(sizeButtons.filter({ hasText: 'Large' })).toHaveClass(/active/);
      const largeSize = await card.evaluate((el) => el.style.fontSize);
      expect(largeSize).toBe('17px');

      // Back to Medium
      await sizeButtons.filter({ hasText: 'Medium' }).click();
      const medSize = await card.evaluate((el) => el.style.fontSize);
      expect(medSize).toBe('15px');
    } finally {
      await close();
    }
  });

  test('text color changes apply to the live preview CSS variables', async ({ browser }) => {
    const { page, close } = await openBrandingStep(browser);
    try {
      const browser_ = page.locator('.wizard-pane-right .lp-browser');
      const textColorInput = page.locator('.wizard-pane-left .wizard-color-row').last().locator('input[type="text"]');

      // Default: theme token for --lp-text (dark mode: #e8e6e3)
      const defaultText = await browser_.evaluate((el) =>
        getComputedStyle(el).getPropertyValue('--lp-text').trim(),
      );
      expect(defaultText).toBe('#e8e6e3');

      // Type a custom color
      await textColorInput.fill('#ff0000');
      await page.waitForTimeout(100);
      const redText = await browser_.evaluate((el) =>
        getComputedStyle(el).getPropertyValue('--lp-text').trim(),
      );
      expect(redText).toBe('#ff0000');

      // Reset button should appear and clear the color
      const resetBtn = page.locator('.wizard-pane-left .wizard-color-row').last().getByRole('button', { name: 'Reset' });
      await expect(resetBtn).toBeVisible();
      await resetBtn.click();
      const resetText = await browser_.evaluate((el) =>
        getComputedStyle(el).getPropertyValue('--lp-text').trim(),
      );
      expect(resetText).toBe('#e8e6e3');
    } finally {
      await close();
    }
  });

  test('button color reset returns to default accent', async ({ browser }) => {
    const { page, close } = await openBrandingStep(browser);
    try {
      // Button color is the first wizard-color-row
      const colorRow = page.locator('.wizard-pane-left .wizard-color-row').first();
      const colorTextInput = colorRow.locator('input[type="text"]');

      // Default: #c4a265 — no Reset button visible
      await expect(colorTextInput).toHaveValue('#c4a265');
      await expect(colorRow.getByRole('button', { name: 'Reset' })).toHaveCount(0);

      // Change color
      await colorTextInput.fill('#ff5500');
      await page.waitForTimeout(100);
      await expect(colorRow.getByRole('button', { name: 'Reset' })).toBeVisible();

      // Click Reset
      await colorRow.getByRole('button', { name: 'Reset' }).click();
      await expect(colorTextInput).toHaveValue('#c4a265');
      await expect(colorRow.getByRole('button', { name: 'Reset' })).toHaveCount(0);
    } finally {
      await close();
    }
  });

  test('no CSP violations during branding interactions', async ({ browser }) => {
    const { page, close } = await openBrandingStep(browser);
    const cspErrors: string[] = [];
    page.on('console', (msg) => {
      if (msg.text().includes('Content Security Policy')) {
        cspErrors.push(msg.text());
      }
    });

    try {
      const fontSelect = page.locator('.wizard-pane-left select.input');
      const sizeButtons = page.locator('.wizard-size-option');

      // Exercise all controls
      await fontSelect.selectOption('DM Sans');
      await sizeButtons.filter({ hasText: 'Large' }).click();

      const textColorInput = page.locator('.wizard-pane-left .wizard-color-row').last().locator('input[type="text"]');
      await textColorInput.fill('#00ff00');

      const buttonColorInput = page.locator('.wizard-pane-left .wizard-color-row').first().locator('input[type="text"]');
      await buttonColorInput.fill('#aa0000');

      // Wait for any async CSP error logs
      await page.waitForTimeout(500);

      expect(
        cspErrors,
        'CSP violations during branding interactions:\n' + cspErrors.join('\n'),
      ).toEqual([]);
    } finally {
      await close();
    }
  });

  test.afterEach(async ({ request }) => {
    // Reset branding state so tests don't interfere with each other
    if (!tenantId) return;
    await patchWithRetry(request, BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + platformToken, 'Content-Type': 'application/json' },
      data: { onboarding_step: 4, branding_primary_color: '#c4a265' },
    });
  });
});
