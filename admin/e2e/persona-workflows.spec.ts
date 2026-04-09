/**
 * Persona-Based E2E Tests
 *
 * Validates that the three-plane navigation system serves four distinct user personas:
 * 1. Indie Dev — single tenant, Overview + Integrate only
 * 2. Startup (B2B SaaS) — Organizations, Roles, Invites, Members
 * 3. Enterprise (Regulated) — Security Config, Audit Log, Sessions, Passkeys
 * 4. Agency (Multi-Tenant) — Tenant switcher, API Keys at a glance
 */

import { test, expect, Page } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { getAccessToken } from './helpers';
import { createTestTenant, deleteTestTenantFromContext } from './factories';
import { Account, Workspace } from './selectors';

// ── Helper: click a plane tab button (scoped to .plane-tier2) ──────────
async function clickTab(page: Page, name: string) {
  await page.locator(Workspace.tier2).getByRole('button', { name }).click();
  await page.waitForTimeout(800);
}

// ── Helper: click a plane button (scoped to .plane-tier1) ──────────────
async function clickPlane(page: Page, name: string) {
  await page.locator(Workspace.tier1).getByRole('button', { name }).click();
  await page.waitForTimeout(500);
}

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 1: Indie Dev
// One tenant, lives in Overview + Integrate, never touches Plane 2/3
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Indie Dev', () => {
  test('create tenant → Overview → Integrate → Auth Reference → Webhooks @regression', async ({ browser }) => {
    test.setTimeout(45000);
    const { context, page } = await loginAsTestUser(browser);
    let tenant: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant = await createTestTenant(context.request, token, 'Indie App');

      // Land in Account mode
      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Account mode: should see Tenants tab active
      await expect(page.locator(Account.tabs)).toBeVisible();

      // Select the tenant → enters Workspace mode
      await page.locator('.tenant-card').filter({ hasText: 'Indie App' }).first().click();
      await page.waitForTimeout(1000);

      // Should be in Workspace mode with PlaneNav visible
      await expect(page.locator(Workspace.planeNav)).toBeVisible();

      // Operator plane should be active by default
      await expect(page.locator(Workspace.activePlane)).toContainText('Operator');

      // Navigate to Integrate tab (scoped to tier2 to avoid matching overview buttons)
      await clickTab(page, 'Integrate');

      // Should see Getting Started, Auth Reference, and Webhooks subtabs
      await expect(page.getByRole('tab', { name: 'Getting Started' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Auth Reference' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Webhooks' })).toBeVisible();

      // Check Auth Reference content
      await page.getByRole('tab', { name: 'Auth Reference' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Callback Payload', { exact: true })).toBeVisible();
      await expect(page.getByText('Token Format', { exact: true })).toBeVisible();

      // Check Webhooks tab
      await page.getByRole('tab', { name: 'Webhooks' }).click();
      await page.waitForTimeout(500);
      await expect(page.locator('.section-title').filter({ hasText: 'Webhooks' })).toBeVisible();

    } finally {
      if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 2: Startup (B2B SaaS)
// Lives in Plane 2: Members, Organizations, Roles, Invites
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Startup B2B', () => {
  test('Members → Invites → Organizations → Roles → Audit Log @regression', async ({ browser }) => {
    test.setTimeout(45000);
    const { context, page } = await loginAsTestUser(browser);
    let tenant: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant = await createTestTenant(context.request, token, 'Startup SaaS');

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Select tenant
      await page.locator('.tenant-card').filter({ hasText: 'Startup SaaS' }).first().click();
      await page.waitForTimeout(1000);

      // Switch to Organization plane
      await clickPlane(page, 'Organization');

      // ── Members tab ──────────────────────────────────────────────
      await clickTab(page, 'Members');

      // Should see the Invites panel (use exact match)
      await expect(page.getByText('Pending Invites', { exact: true })).toBeVisible();

      // Should see the user list section
      await expect(page.getByText('Recent Members')).toBeVisible();

      // ── Organizations tab ────────────────────────────────────────
      await clickTab(page, 'Organizations');
      await expect(page.locator('.view-title').filter({ hasText: 'Organizations' })).toBeVisible();

      // ── Roles tab ────────────────────────────────────────────────
      await clickTab(page, 'Roles');
      await expect(page.getByText('All Roles')).toBeVisible();

      // ── Audit Log tab ────────────────────────────────────────────
      await clickTab(page, 'Audit Log');
      await expect(page.getByText('Login Attempts', { exact: true })).toBeVisible();

    } finally {
      if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 3: Enterprise (Regulated)
// Security tab critical, all three planes used
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Enterprise', () => {
  test('Security Config → Sessions → Passkeys → Tokens → Activity @regression', async ({ browser }) => {
    test.setTimeout(45000);
    const { context, page } = await loginAsTestUser(browser);
    let tenant: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant = await createTestTenant(context.request, token, 'Enterprise Corp');

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Select tenant
      await page.locator('.tenant-card').filter({ hasText: 'Enterprise Corp' }).first().click();
      await page.waitForTimeout(1000);

      // ── Plane 1: Security tab ────────────────────────────────────
      await clickPlane(page, 'Operator');
      await clickTab(page, 'Security');

      // Should see security config heading
      await expect(page.locator('.view-title').filter({ hasText: 'Security' })).toBeVisible();

      // ── Plane 3: Sessions ────────────────────────────────────────
      await clickPlane(page, 'End User');
      await clickTab(page, 'Sessions');
      await expect(page.getByText('Active Sessions', { exact: true })).toBeVisible();

      // ── Plane 3: Passkeys ────────────────────────────────────────
      await clickTab(page, 'Passkeys');
      await expect(page.getByText('WebAuthn Credentials', { exact: true })).toBeVisible();

      // ── Plane 3: Tokens ──────────────────────────────────────────
      await clickTab(page, 'Tokens');
      await expect(page.getByText('JWKS Public Keys', { exact: true })).toBeVisible();

      // ── Plane 3: Activity ────────────────────────────────────────
      await clickTab(page, 'Activity');
      await expect(page.getByText('Authentication Timeline')).toBeVisible();

    } finally {
      if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 4: Agency (Multi-Tenant)
// Tenant switcher power user, API Keys at a glance
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Agency Multi-Tenant', () => {
  test('create 2 tenants → switch between them → API Keys view @regression', async ({ browser }) => {
    test.setTimeout(45000);
    const { context, page } = await loginAsTestUser(browser);
    let tenant1: { id: string; slug: string } | undefined;
    let tenant2: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant1 = await createTestTenant(context.request, token, 'Client Alpha');
      tenant2 = await createTestTenant(context.request, token, 'Client Beta');

      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      // Clear workspace state to ensure account mode, then reload
      await page.evaluate(() => {
        try {
          for (const key of Object.keys(localStorage)) {
            if (key.startsWith('forma-console-workspace:')) localStorage.removeItem(key);
          }
        } catch {}
      });
      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      // Account mode: should see both tenants in the list
      await expect(page.locator('.tenant-card').filter({ hasText: 'Client Alpha' })).toBeVisible({ timeout: 10000 });
      await expect(page.locator('.tenant-card').filter({ hasText: 'Client Beta' })).toBeVisible();

      // ── API Keys tab ─────────────────────────────────────────────
      await page.locator(Account.tabs).getByRole('button', { name: 'API Keys' }).click();
      await page.waitForTimeout(1000);

      // Should see tenant keys section
      await expect(page.getByText('Your Applications')).toBeVisible();

      // ── Select first tenant ──────────────────────────────────────
      await page.locator(Account.tabs).getByRole('button', { name: 'Tenants' }).click();
      await page.waitForTimeout(500);

      await page.locator('.tenant-card').filter({ hasText: 'Client Alpha' }).first().click();
      await page.waitForTimeout(1000);

      // Should be in workspace mode
      await expect(page.locator(Workspace.planeNav)).toBeVisible();

      // Tenant name should appear in the tenant switcher
      await expect(page.locator(Workspace.tenantName)).toContainText('Client Alpha');

      // ── Use tenant switcher to switch to Client Beta ─────────────
      await page.locator(Workspace.tenantTrigger).click();
      await page.waitForTimeout(500);

      // Tenant switcher dropdown should show
      await expect(page.locator(Workspace.tenantMenu)).toBeVisible();

      // Click Client Beta in the dropdown
      await page.locator(Workspace.tenantMenuItem).filter({ hasText: 'Client Beta' }).click();
      await page.waitForTimeout(1000);

      // Should now show Client Beta in the header
      await expect(page.locator(Workspace.tenantName)).toContainText('Client Beta');

      // ── Use tenant switcher to go back to Account mode ───────────
      await page.locator(Workspace.tenantTrigger).click();
      await page.waitForTimeout(500);
      await page.locator(Workspace.tenantMenuBack).click();
      await page.waitForTimeout(1000);

      // Should be back in Account mode
      await expect(page.locator(Account.tabs)).toBeVisible();

    } finally {
      if (tenant1) await deleteTestTenantFromContext(context, tenant1.id, tenant1.slug);
      if (tenant2) await deleteTestTenantFromContext(context, tenant2.id, tenant2.slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// NAVIGATION STRUCTURE: Verify all 15 tabs exist and are reachable
// ═══════════════════════════════════════════════════════════════════════

test.describe('Navigation Completeness', () => {
  test('all 3 account tabs + 12 workspace tabs are reachable @regression', async ({ browser }) => {
    test.setTimeout(45000);
    const { context, page } = await loginAsTestUser(browser);
    let tenant: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant = await createTestTenant(context.request, token, 'Nav Test');

      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      // Clear workspace state to ensure account mode, then reload
      await page.evaluate(() => {
        try {
          for (const key of Object.keys(localStorage)) {
            if (key.startsWith('forma-console-workspace:')) localStorage.removeItem(key);
          }
        } catch {}
      });
      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      // ── Account Mode: 3 tabs ─────────────────────────────────────
      const accountTabs = page.locator(Account.tabs);
      await expect(accountTabs.getByRole('button', { name: 'Tenants' })).toBeVisible();
      await expect(accountTabs.getByRole('button', { name: 'API Keys' })).toBeVisible();
      await expect(accountTabs.getByRole('button', { name: 'Settings' })).toBeVisible();

      // Click API Keys tab
      await accountTabs.getByRole('button', { name: 'API Keys' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Your Applications')).toBeVisible();

      // Click Settings tab
      await accountTabs.getByRole('button', { name: 'Settings' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Profile', { exact: true })).toBeVisible();

      // Go back to Tenants and select one
      await accountTabs.getByRole('button', { name: 'Tenants' }).click();
      await page.waitForTimeout(500);
      await page.locator('.tenant-card').filter({ hasText: 'Nav Test' }).first().click();
      await page.waitForTimeout(1000);

      // ── Workspace Mode: Plane 1 (3 tabs) ─────────────────────────
      await clickPlane(page, 'Operator');
      await clickTab(page, 'Overview');
      await clickTab(page, 'Integrate');
      await clickTab(page, 'Security');

      // ── Workspace Mode: Plane 2 (5 tabs) ─────────────────────────
      await clickPlane(page, 'Organization');
      await clickTab(page, 'Members');
      await clickTab(page, 'Hosted Page');
      await clickTab(page, 'Organizations');
      await clickTab(page, 'Roles');
      await clickTab(page, 'Audit Log');

      // ── Workspace Mode: Plane 3 (4 tabs) ─────────────────────────
      await clickPlane(page, 'End User');
      await clickTab(page, 'Sessions');
      await clickTab(page, 'Passkeys');
      await clickTab(page, 'Tokens');
      await clickTab(page, 'Activity');

      // If we got here without errors, all 15 tabs are reachable

    } finally {
      if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
      await context.close();
    }
  });

  test('keyboard shortcuts switch planes @regression', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    let tenant: { id: string; slug: string } | undefined;

    try {
      const token = await getAccessToken(context);
      tenant = await createTestTenant(context.request, token, 'KB Test');

      // Navigate to console — the init() will load tenants fresh
      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      // Ensure we're in account mode
      await page.evaluate(() => {
        try {
          for (const key of Object.keys(localStorage)) {
            if (key.startsWith('forma-console-workspace:')) localStorage.removeItem(key);
          }
        } catch {}
      });
      await page.goto('/platform/console');
      await page.waitForTimeout(2000);

      await page.locator('.tenant-card').filter({ hasText: 'KB Test' }).first().click({ timeout: 10000 });
      await page.waitForTimeout(1000);

      // Press '2' to switch to Organization plane
      await page.keyboard.press('2');
      await page.waitForTimeout(500);
      await expect(page.locator(Workspace.activePlane)).toContainText('Organization');

      // Press '3' to switch to End User plane
      await page.keyboard.press('3');
      await page.waitForTimeout(500);
      await expect(page.locator(Workspace.activePlane)).toContainText('End User');

      // Press '1' to switch back to Operator
      await page.keyboard.press('1');
      await page.waitForTimeout(500);
      await expect(page.locator(Workspace.activePlane)).toContainText('Operator');

      // Press backtick to collapse nav
      await page.keyboard.press('`');
      await page.waitForTimeout(500);
      await expect(page.locator(Workspace.planeNav)).toHaveClass(/plane-nav-collapsed/);

      // Press backtick again to expand
      await page.keyboard.press('`');
      await page.waitForTimeout(500);

    } finally {
      if (tenant) await deleteTestTenantFromContext(context, tenant.id, tenant.slug);
      await context.close();
    }
  });
});
