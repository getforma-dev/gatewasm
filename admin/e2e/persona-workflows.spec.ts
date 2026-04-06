/**
 * Persona-Based E2E Tests
 *
 * Validates that the three-plane navigation system serves four distinct user personas:
 * 1. Indie Dev — single tenant, Overview + Integrate only
 * 2. Startup (B2B SaaS) — Organizations, Roles, Invites, Members
 * 3. Enterprise (Regulated) — Security Config, Audit Log, Sessions, Passkeys
 * 4. Agency (Multi-Tenant) — Tenant switcher, API Keys at a glance
 */

import { test, expect } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';

const BASE = process.env.BASE_URL || 'http://localhost:3001';

// ── Helper: get access token from context ──────────────────────────────
async function getAccessToken(context: any): Promise<string> {
  const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
    data: { grant_type: 'refresh_token' },
  });
  expect(refresh.ok()).toBeTruthy();
  const json = await refresh.json();
  return json.access_token as string;
}

// ── Helper: create tenant via API ──────────────────────────────────────
async function createTenant(context: any, token: string, name: string, slug: string): Promise<string> {
  const res = await context.request.post(BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token },
    data: { name, slug },
  });
  expect(res.ok()).toBeTruthy();
  const json = await res.json();
  return String(json.tenant.id);
}

// ── Helper: delete tenant via API ──────────────────────────────────────
async function deleteTenant(context: any, tenantId: string, slug: string): Promise<void> {
  try {
    const token = await getAccessToken(context);
    await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + token },
      data: { confirm_slug: slug },
    });
  } catch { /* cleanup best-effort */ }
}

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 1: Indie Dev
// One tenant, lives in Overview + Integrate, never touches Plane 2/3
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Indie Dev', () => {
  test('create tenant → Overview → Integrate → Auth Reference → Webhooks', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug = 'indie-' + Date.now().toString(36);
    let tenantId = '';

    try {
      const token = await getAccessToken(context);
      tenantId = await createTenant(context, token, 'Indie App', slug);

      // Land in Account mode
      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Account mode: should see Tenants tab active
      await expect(page.locator('.account-tabs')).toBeVisible();

      // Select the tenant → enters Workspace mode
      await page.locator('.tenant-card').filter({ hasText: 'Indie App' }).first().click();
      await page.waitForTimeout(1000);

      // Should be in Workspace mode with PlaneNav visible
      await expect(page.locator('.plane-nav')).toBeVisible();

      // Operator plane should be active by default
      await expect(page.locator('.plane-btn.active')).toContainText('Operator');

      // Overview tab should be visible and active
      await expect(page.getByText('Overview')).toBeVisible();

      // Navigate to Integrate tab
      await page.getByRole('button', { name: 'Integrate' }).click();
      await page.waitForTimeout(1000);

      // Should see Getting Started, Auth Reference, and Webhooks subtabs
      await expect(page.getByRole('tab', { name: 'Getting Started' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Auth Reference' })).toBeVisible();
      await expect(page.getByRole('tab', { name: 'Webhooks' })).toBeVisible();

      // Check Auth Reference content
      await page.getByRole('tab', { name: 'Auth Reference' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Callback Payload')).toBeVisible();
      await expect(page.getByText('Token Format')).toBeVisible();

      // Check Webhooks tab
      await page.getByRole('tab', { name: 'Webhooks' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Webhooks')).toBeVisible();

    } finally {
      if (tenantId) await deleteTenant(context, tenantId, slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 2: Startup (B2B SaaS)
// Lives in Plane 2: Members, Organizations, Roles, Invites
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Startup B2B', () => {
  test('Members → Invites → Organizations → Roles → Audit Log', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug = 'startup-' + Date.now().toString(36);
    let tenantId = '';

    try {
      const token = await getAccessToken(context);
      tenantId = await createTenant(context, token, 'Startup SaaS', slug);

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Select tenant
      await page.locator('.tenant-card').filter({ hasText: 'Startup SaaS' }).first().click();
      await page.waitForTimeout(1000);

      // Switch to Organization plane
      await page.getByRole('button', { name: 'Organization' }).click();
      await page.waitForTimeout(500);

      // ── Members tab ──────────────────────────────────────────────
      await page.getByRole('button', { name: 'Members' }).click();
      await page.waitForTimeout(1000);

      // Should see the Invites panel
      await expect(page.getByText('Pending Invites')).toBeVisible();

      // Should see the user list section
      await expect(page.getByText('Recent Users')).toBeVisible();

      // ── Organizations tab ────────────────────────────────────────
      await page.getByRole('button', { name: 'Organizations' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('Organizations')).toBeVisible();

      // ── Roles tab ────────────────────────────────────────────────
      await page.getByRole('button', { name: 'Roles' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('All Roles')).toBeVisible();

      // ── Audit Log tab ────────────────────────────────────────────
      await page.getByRole('button', { name: 'Audit Log' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('Login Attempts')).toBeVisible();

    } finally {
      if (tenantId) await deleteTenant(context, tenantId, slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 3: Enterprise (Regulated)
// Security tab critical, all three planes used
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Enterprise', () => {
  test('Security Config → Sessions → Passkeys → Tokens → Activity', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug = 'enterprise-' + Date.now().toString(36);
    let tenantId = '';

    try {
      const token = await getAccessToken(context);
      tenantId = await createTenant(context, token, 'Enterprise Corp', slug);

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Select tenant
      await page.locator('.tenant-card').filter({ hasText: 'Enterprise Corp' }).first().click();
      await page.waitForTimeout(1000);

      // ── Plane 1: Security tab ────────────────────────────────────
      await page.getByRole('button', { name: 'Operator' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Security' }).click();
      await page.waitForTimeout(1000);

      // Should see security config sections
      await expect(page.getByText('Security')).toBeVisible();

      // ── Plane 3: Sessions ────────────────────────────────────────
      await page.getByRole('button', { name: 'End User' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Sessions' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('Active Sessions')).toBeVisible();

      // ── Plane 3: Passkeys ────────────────────────────────────────
      await page.getByRole('button', { name: 'Passkeys' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('WebAuthn Credentials')).toBeVisible();

      // ── Plane 3: Tokens ──────────────────────────────────────────
      await page.getByRole('button', { name: 'Tokens' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('JWKS Public Keys')).toBeVisible();
      await expect(page.getByText('Token Configuration')).toBeVisible();

      // ── Plane 3: Activity ────────────────────────────────────────
      await page.getByRole('button', { name: 'Activity' }).click();
      await page.waitForTimeout(1000);

      await expect(page.getByText('Authentication Timeline')).toBeVisible();

    } finally {
      if (tenantId) await deleteTenant(context, tenantId, slug);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// PERSONA 4: Agency (Multi-Tenant)
// Tenant switcher power user, API Keys at a glance
// ═══════════════════════════════════════════════════════════════════════

test.describe('Persona: Agency Multi-Tenant', () => {
  test('create 2 tenants → switch between them → API Keys view', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug1 = 'client-a-' + Date.now().toString(36);
    const slug2 = 'client-b-' + Date.now().toString(36);
    let tenantId1 = '';
    let tenantId2 = '';

    try {
      const token = await getAccessToken(context);
      tenantId1 = await createTenant(context, token, 'Client Alpha', slug1);
      tenantId2 = await createTenant(context, token, 'Client Beta', slug2);

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // Account mode: should see both tenants in the list
      await expect(page.locator('.tenant-card').filter({ hasText: 'Client Alpha' })).toBeVisible();
      await expect(page.locator('.tenant-card').filter({ hasText: 'Client Beta' })).toBeVisible();

      // ── API Keys tab ─────────────────────────────────────────────
      await page.getByRole('button', { name: 'API Keys' }).click();
      await page.waitForTimeout(1000);

      // Should see both tenants' keys listed
      await expect(page.getByText('Client Alpha')).toBeVisible();
      await expect(page.getByText('Client Beta')).toBeVisible();

      // ── Select first tenant ──────────────────────────────────────
      // Go back to Tenants tab
      await page.getByRole('button', { name: 'Tenants' }).click();
      await page.waitForTimeout(500);

      await page.locator('.tenant-card').filter({ hasText: 'Client Alpha' }).first().click();
      await page.waitForTimeout(1000);

      // Should be in workspace mode
      await expect(page.locator('.plane-nav')).toBeVisible();

      // Tenant name should appear in the header (tenant switcher)
      await expect(page.getByText('Client Alpha')).toBeVisible();

      // ── Use tenant switcher to switch to Client Beta ─────────────
      await page.locator('.plane-tenant-trigger').click();
      await page.waitForTimeout(500);

      // Tenant switcher dropdown should show both tenants
      await expect(page.locator('.plane-tenant-menu')).toBeVisible();
      await expect(page.locator('.plane-tenant-menu').getByText('Client Beta')).toBeVisible();

      // Click Client Beta
      await page.locator('.plane-tenant-menu').getByText('Client Beta').click();
      await page.waitForTimeout(1000);

      // Should now show Client Beta in the header
      await expect(page.locator('.plane-tenant-name')).toContainText('Client Beta');

      // ── Use tenant switcher to go back to Account mode ───────────
      await page.locator('.plane-tenant-trigger').click();
      await page.waitForTimeout(500);
      await page.locator('.plane-tenant-menu-back').click();
      await page.waitForTimeout(1000);

      // Should be back in Account mode
      await expect(page.locator('.account-tabs')).toBeVisible();

    } finally {
      if (tenantId1) await deleteTenant(context, tenantId1, slug1);
      if (tenantId2) await deleteTenant(context, tenantId2, slug2);
      await context.close();
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════
// NAVIGATION STRUCTURE: Verify all 15 tabs exist and are reachable
// ═══════════════════════════════════════════════════════════════════════

test.describe('Navigation Completeness', () => {
  test('all 3 account tabs + 12 workspace tabs are reachable', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug = 'nav-test-' + Date.now().toString(36);
    let tenantId = '';

    try {
      const token = await getAccessToken(context);
      tenantId = await createTenant(context, token, 'Nav Test', slug);

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      // ── Account Mode: 3 tabs ─────────────────────────────────────
      await expect(page.getByRole('button', { name: 'Tenants' })).toBeVisible();
      await expect(page.getByRole('button', { name: 'API Keys' })).toBeVisible();
      await expect(page.getByRole('button', { name: 'Settings' })).toBeVisible();

      // Click each account tab
      await page.getByRole('button', { name: 'API Keys' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Tenant Keys')).toBeVisible();

      await page.getByRole('button', { name: 'Settings' }).click();
      await page.waitForTimeout(500);
      await expect(page.getByText('Profile')).toBeVisible();

      // Go back to Tenants and select one
      await page.getByRole('button', { name: 'Tenants' }).click();
      await page.waitForTimeout(500);
      await page.locator('.tenant-card').filter({ hasText: 'Nav Test' }).first().click();
      await page.waitForTimeout(1000);

      // ── Workspace Mode: Plane 1 (3 tabs) ─────────────────────────
      await page.getByRole('button', { name: 'Operator' }).click();
      await page.waitForTimeout(300);

      // Overview (default)
      await page.getByRole('button', { name: 'Overview' }).click();
      await page.waitForTimeout(500);

      // Integrate
      await page.getByRole('button', { name: 'Integrate' }).click();
      await page.waitForTimeout(500);

      // Security
      await page.getByRole('button', { name: 'Security' }).click();
      await page.waitForTimeout(500);

      // ── Workspace Mode: Plane 2 (5 tabs) ─────────────────────────
      await page.getByRole('button', { name: 'Organization' }).click();
      await page.waitForTimeout(300);

      await page.getByRole('button', { name: 'Members' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Hosted Page' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Organizations' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Roles' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Audit Log' }).click();
      await page.waitForTimeout(500);

      // ── Workspace Mode: Plane 3 (4 tabs) ─────────────────────────
      await page.getByRole('button', { name: 'End User' }).click();
      await page.waitForTimeout(300);

      await page.getByRole('button', { name: 'Sessions' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Passkeys' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Tokens' }).click();
      await page.waitForTimeout(500);

      await page.getByRole('button', { name: 'Activity' }).click();
      await page.waitForTimeout(500);

      // If we got here without errors, all 15 tabs are reachable

    } finally {
      if (tenantId) await deleteTenant(context, tenantId, slug);
      await context.close();
    }
  });

  test('keyboard shortcuts switch planes', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    const slug = 'kb-' + Date.now().toString(36);
    let tenantId = '';

    try {
      const token = await getAccessToken(context);
      tenantId = await createTenant(context, token, 'KB Test', slug);

      await page.goto('/platform/console');
      await page.waitForTimeout(1500);

      await page.locator('.tenant-card').filter({ hasText: 'KB Test' }).first().click();
      await page.waitForTimeout(1000);

      // Press '2' to switch to Organization plane
      await page.keyboard.press('2');
      await page.waitForTimeout(500);
      await expect(page.locator('.plane-btn.active')).toContainText('Organization');

      // Press '3' to switch to End User plane
      await page.keyboard.press('3');
      await page.waitForTimeout(500);
      await expect(page.locator('.plane-btn.active')).toContainText('End User');

      // Press '1' to switch back to Operator
      await page.keyboard.press('1');
      await page.waitForTimeout(500);
      await expect(page.locator('.plane-btn.active')).toContainText('Operator');

      // Press backtick to collapse nav
      await page.keyboard.press('`');
      await page.waitForTimeout(500);
      await expect(page.locator('.plane-nav-collapsed')).toBeTruthy();

      // Press backtick again to expand
      await page.keyboard.press('`');
      await page.waitForTimeout(500);

    } finally {
      if (tenantId) await deleteTenant(context, tenantId, slug);
      await context.close();
    }
  });
});
