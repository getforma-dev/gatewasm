# E2E Test Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix broken e2e tests (URL mismatches), add WebSocket infrastructure tests, and add comprehensive team RBAC tests including cross-account isolation.

**Architecture:** Three parallel tracks: (1) centralize BASE_URL in helpers.ts, update 6 importers; (2) new `websocket.spec.ts` with ticket auth, event delivery via management API triggers, and reconnection tests; (3) new `team-rbac.spec.ts` with invite lifecycle, role enforcement, and cross-account isolation — all API-driven for speed.

**Tech Stack:** Playwright, existing auth-helper.ts and helpers.ts patterns, management API with MASTER_MGMT_KEY for server-side event triggers.

**Spec:** `docs/superpowers/specs/2026-04-07-e2e-test-audit-design.md`

---

## File Map

### New Files

| File | Responsibility |
|------|---------------|
| `admin/e2e/websocket.spec.ts` | WS ticket auth, event delivery, reconnection tests |
| `admin/e2e/team-rbac.spec.ts` | Invite lifecycle, role enforcement, cross-account isolation |

### Modified Files

| File | Change |
|------|--------|
| `admin/e2e/helpers.ts` | Add `export const BASE` constant |
| `admin/e2e/command-palette.spec.ts` | Import BASE from helpers, remove local const |
| `admin/e2e/wizard-flow.spec.ts` | Import BASE from helpers, remove local const |
| `admin/e2e/persona-workflows.spec.ts` | Import BASE from helpers, remove local const |
| `admin/e2e/session-inspector.spec.ts` | Import BASE from helpers, remove local const |
| `admin/e2e/fixes-verify.spec.ts` | Import BASE from helpers, remove local const |
| `admin/e2e/overview-analytics.spec.ts` | Import BASE from helpers, remove local const |

---

## Task 1: Centralize BASE_URL in helpers.ts

**Files:**
- Modify: `admin/e2e/helpers.ts:1`
- Modify: `admin/e2e/command-palette.spec.ts:4`
- Modify: `admin/e2e/wizard-flow.spec.ts:3`
- Modify: `admin/e2e/persona-workflows.spec.ts:14`
- Modify: `admin/e2e/session-inspector.spec.ts:4`
- Modify: `admin/e2e/fixes-verify.spec.ts:3`
- Modify: `admin/e2e/overview-analytics.spec.ts:4`

- [ ] **Step 1: Add BASE export to helpers.ts**

At the top of `admin/e2e/helpers.ts`, before the first import, add:

```typescript
export const BASE = process.env.BASE_URL || 'http://127.0.0.1:3102';
```

- [ ] **Step 2: Update command-palette.spec.ts**

Replace line 4:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With import (add to existing imports):
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 3: Update wizard-flow.spec.ts**

Replace line 3:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With:
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 4: Update persona-workflows.spec.ts**

Replace line 14:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With (add to existing imports near line 11):
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 5: Update session-inspector.spec.ts**

Replace line 4:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With:
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 6: Update fixes-verify.spec.ts**

Replace line 3:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With:
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 7: Update overview-analytics.spec.ts**

Replace line 4:
```typescript
const BASE = process.env.BASE_URL || 'http://localhost:3001';
```
With:
```typescript
import { BASE } from './helpers';
```

- [ ] **Step 8: Verify TypeScript compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx tsc --noEmit 2>&1 | grep -i "e2e\|helpers\|BASE"`

Expected: No errors related to BASE or helpers imports.

- [ ] **Step 9: Commit**

```bash
git add admin/e2e/helpers.ts admin/e2e/command-palette.spec.ts admin/e2e/wizard-flow.spec.ts admin/e2e/persona-workflows.spec.ts admin/e2e/session-inspector.spec.ts admin/e2e/fixes-verify.spec.ts admin/e2e/overview-analytics.spec.ts
git commit -m "fix(e2e): centralize BASE_URL in helpers.ts — fixes 6 tests hitting wrong port"
```

---

## Task 2: WebSocket E2E Tests — Ticket Auth & Connection Lifecycle

**Files:**
- Create: `admin/e2e/websocket.spec.ts`

- [ ] **Step 1: Create websocket.spec.ts with helpers and Block 1 tests**

Create `admin/e2e/websocket.spec.ts`:

```typescript
import { test, expect, Page, BrowserContext } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE } from './helpers';

// ── Helpers ──────────────────────────────────────────────────────────────

async function getAccessToken(context: BrowserContext): Promise<string> {
  const res = await context.request.post(BASE + '/t/platform/auth/token', {
    data: { grant_type: 'refresh_token' },
  });
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  return data.access_token as string;
}

async function getWsTicket(context: BrowserContext, token: string): Promise<string> {
  const res = await context.request.post(BASE + '/ws/ticket', {
    headers: { Authorization: 'Bearer ' + token },
  });
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  return data.ticket as string;
}

/**
 * Open a WebSocket inside the browser page context.
 * Returns a handle for sending messages and waiting for received messages.
 */
async function connectWebSocket(page: Page, ticket: string) {
  const wsUrl = BASE.replace(/^http/, 'ws') + '/ws?ticket=' + ticket;

  await page.evaluate((url) => {
    (window as any).__ws_messages = [];
    (window as any).__ws_open = false;
    (window as any).__ws_closed = false;
    const ws = new WebSocket(url);
    (window as any).__ws = ws;
    ws.onopen = () => { (window as any).__ws_open = true; };
    ws.onmessage = (ev) => {
      try {
        (window as any).__ws_messages.push(JSON.parse(ev.data));
      } catch {
        (window as any).__ws_messages.push(ev.data);
      }
    };
    ws.onclose = () => { (window as any).__ws_closed = true; };
  }, wsUrl);

  return {
    waitForOpen: async (timeoutMs = 5000) => {
      await page.waitForFunction(() => (window as any).__ws_open, {}, { timeout: timeoutMs });
    },
    waitForClose: async (timeoutMs = 5000) => {
      await page.waitForFunction(() => (window as any).__ws_closed, {}, { timeout: timeoutMs });
    },
    send: async (msg: object) => {
      await page.evaluate((m) => (window as any).__ws.send(JSON.stringify(m)), msg);
    },
    getMessages: async (): Promise<any[]> => {
      return page.evaluate(() => (window as any).__ws_messages);
    },
    waitForMessage: async (predicate: string, timeoutMs = 5000): Promise<any> => {
      // predicate is a stringified function body to run in browser context
      return page.waitForFunction(
        (pred) => {
          const fn = new Function('msg', 'return ' + pred);
          return (window as any).__ws_messages.find((m: any) => fn(m));
        },
        predicate,
        { timeout: timeoutMs },
      ).then(handle => handle.jsonValue());
    },
    close: async () => {
      await page.evaluate(() => (window as any).__ws?.close());
    },
  };
}

// ── Block 1: Ticket Auth & Connection Lifecycle ──────────────────────────

test.describe('WS Ticket Auth & Connection Lifecycle', () => {
  let context: BrowserContext;
  let page: Page;
  let token: string;

  test.beforeAll(async ({ browser }) => {
    const auth = await loginAsTestUser(browser);
    context = auth.context;
    page = auth.page;
    token = await getAccessToken(context);
  });

  test.afterAll(async () => {
    await context?.close();
  });

  test('POST /ws/ticket returns a ticket when authenticated', async () => {
    const res = await context.request.post(BASE + '/ws/ticket', {
      headers: { Authorization: 'Bearer ' + token },
    });
    expect(res.status()).toBe(200);
    const data = await res.json();
    expect(data.ticket).toBeTruthy();
    expect(typeof data.ticket).toBe('string');
    expect(data.ticket.length).toBe(64); // 32 bytes hex-encoded
  });

  test('POST /ws/ticket rejects unauthenticated requests', async () => {
    const res = await context.request.post(BASE + '/ws/ticket');
    expect(res.status()).toBe(401);
  });

  test('WS upgrade succeeds with valid ticket', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();

    // Send ping, expect pong
    await ws.send({ type: 'ping' });
    await ws.waitForMessage('msg.type === "pong"');
    const messages = await ws.getMessages();
    expect(messages.some((m: any) => m.type === 'pong')).toBe(true);

    await ws.close();
  });

  test('WS upgrade rejects invalid ticket', async () => {
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(500);

    const ws = await connectWebSocket(page, 'bogus_invalid_ticket_value');
    await ws.waitForClose(5000);

    const isOpen = await page.evaluate(() => (window as any).__ws_open);
    // Connection should have been rejected (never opened, or opened then immediately closed)
    const isClosed = await page.evaluate(() => (window as any).__ws_closed);
    expect(isClosed).toBe(true);
  });

  test('WS upgrade rejects reused ticket', async () => {
    const ticket = await getWsTicket(context, token);

    // First connection burns the ticket
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(500);
    const ws1 = await connectWebSocket(page, ticket);
    await ws1.waitForOpen();
    await ws1.close();
    await ws1.waitForClose();

    // Second connection with same ticket should fail
    await page.waitForTimeout(300);
    const ws2 = await connectWebSocket(page, ticket);
    await ws2.waitForClose(5000);
    const isClosed = await page.evaluate(() => (window as any).__ws_closed);
    expect(isClosed).toBe(true);
  });

  test('WS upgrade rejects expired ticket (30s TTL)', async () => {
    test.setTimeout(45000); // Extended timeout for this test
    const ticket = await getWsTicket(context, token);

    // Wait for ticket to expire
    await page.waitForTimeout(31000);

    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(500);
    const ws = await connectWebSocket(page, ticket);
    await ws.waitForClose(5000);
    const isClosed = await page.evaluate(() => (window as any).__ws_closed);
    expect(isClosed).toBe(true);
  });

  test('Server sends pong in response to ping', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(500);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    await ws.send({ type: 'ping' });

    await ws.waitForMessage('msg.type === "pong"');
    const messages = await ws.getMessages();
    expect(messages.find((m: any) => m.type === 'pong')).toBeTruthy();

    await ws.close();
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/websocket.spec.ts
git commit -m "test(ws): ticket auth and connection lifecycle e2e tests"
```

---

## Task 3: WebSocket E2E Tests — Event Delivery

**Files:**
- Modify: `admin/e2e/websocket.spec.ts`

- [ ] **Step 1: Add Block 2 (event delivery) tests**

Append to `admin/e2e/websocket.spec.ts`, after the Block 1 describe block:

```typescript
// ── Block 2: Topic Subscription & Event Delivery ─────────────────────────

test.describe('WS Event Delivery', () => {
  let context: BrowserContext;
  let page: Page;
  let token: string;
  let tenantId: string;
  const tenantSlug = 'e2e-ws-' + Date.now().toString(36);

  test.beforeAll(async ({ browser }) => {
    const auth = await loginAsTestUser(browser);
    context = auth.context;
    page = auth.page;
    token = await getAccessToken(context);

    // Create a test tenant for activity events
    const res = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { name: 'WS Test Tenant', slug: tenantSlug },
    });
    expect(res.ok()).toBeTruthy();
    const data = await res.json();
    tenantId = data.id || data.tenant?.id;
    expect(tenantId).toBeTruthy();
  });

  test.afterAll(async () => {
    // Cleanup test tenant
    if (tenantId) {
      await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
        headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
        data: { confirm_slug: tenantSlug },
      });
    }
    await context?.close();
  });

  test('receives team:changed when invite is created', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    await ws.send({ type: 'subscribe', topics: ['team:changed'] });
    await page.waitForTimeout(300);

    // Create an invite via API
    const inviteEmail = 'ws-invite-' + Date.now() + '@test.local';
    const inviteRes = await context.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { email: inviteEmail, role: 'viewer', tenant_ids: [tenantId] },
    });
    expect(inviteRes.ok()).toBeTruthy();

    // Wait for event
    await ws.waitForMessage('msg.type === "event" && msg.topic === "team:changed" && msg.hint === "invite_created"', 5000);
    const messages = await ws.getMessages();
    const event = messages.find((m: any) => m.type === 'event' && m.topic === 'team:changed');
    expect(event).toBeTruthy();
    expect(event.hint).toBe('invite_created');

    await ws.close();
  });

  test('receives tenant:activity on end user signup', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    await ws.send({ type: 'subscribe', topics: ['tenant:activity'] });
    await page.waitForTimeout(300);

    // Register an end user on the test tenant
    const endUserEmail = 'ws-enduser-' + Date.now() + '@test.local';
    const regRes = await context.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: { email: endUserEmail, password: 'TestPass123!', name: 'WS Test End User' },
    });
    expect(regRes.ok()).toBeTruthy();

    await ws.waitForMessage('msg.type === "event" && msg.topic === "tenant:activity" && msg.hint === "signup"', 5000);
    const messages = await ws.getMessages();
    const event = messages.find((m: any) => m.type === 'event' && m.topic === 'tenant:activity');
    expect(event).toBeTruthy();
    expect(event.hint).toBe('signup');

    await ws.close();
  });

  test('receives metrics:updated alongside tenant:activity', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    await ws.send({ type: 'subscribe', topics: ['tenant:activity', 'metrics:updated'] });
    await page.waitForTimeout(300);

    // Trigger activity
    const email = 'ws-dual-' + Date.now() + '@test.local';
    await context.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: { email, password: 'TestPass123!', name: 'Dual Emit Test' },
    });

    // Both events should arrive
    await ws.waitForMessage('msg.type === "event" && msg.topic === "tenant:activity"', 5000);
    await ws.waitForMessage('msg.type === "event" && msg.topic === "metrics:updated"', 5000);
    const messages = await ws.getMessages();
    expect(messages.some((m: any) => m.topic === 'tenant:activity')).toBe(true);
    expect(messages.some((m: any) => m.topic === 'metrics:updated')).toBe(true);

    await ws.close();
  });

  test('does not receive events for unsubscribed topics', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    // Subscribe only to team:changed, NOT tenant:activity
    await ws.send({ type: 'subscribe', topics: ['team:changed'] });
    await page.waitForTimeout(300);

    // Trigger tenant:activity event
    const email = 'ws-unsub-' + Date.now() + '@test.local';
    await context.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug, 'Content-Type': 'application/json' },
      data: { email, password: 'TestPass123!', name: 'Unsub Test' },
    });

    // Wait 2s, verify no tenant:activity event arrived
    await page.waitForTimeout(2000);
    const messages = await ws.getMessages();
    expect(messages.filter((m: any) => m.topic === 'tenant:activity').length).toBe(0);

    await ws.close();
  });

  test('does not receive events for other tenants', async () => {
    // Create a second tenant
    const otherSlug = 'e2e-ws-other-' + Date.now().toString(36);
    const otherRes = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { name: 'WS Other Tenant', slug: otherSlug },
    });
    expect(otherRes.ok()).toBeTruthy();
    const otherData = await otherRes.json();
    const otherTenantId = otherData.id || otherData.tenant?.id;

    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();
    await ws.send({ type: 'subscribe', topics: ['tenant:activity'] });
    await page.waitForTimeout(300);

    // Trigger activity on OTHER tenant
    const email = 'ws-other-' + Date.now() + '@test.local';
    await context.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': otherSlug, 'Content-Type': 'application/json' },
      data: { email, password: 'TestPass123!', name: 'Other Tenant User' },
    });

    // Wait 2s, verify no event for other tenant
    await page.waitForTimeout(2000);
    const messages = await ws.getMessages();
    // The WS connection is scoped to the platform tenant, so tenant:activity
    // for otherSlug should not arrive (different tenant_id)
    expect(messages.filter((m: any) => m.topic === 'tenant:activity').length).toBe(0);

    // Cleanup
    await context.request.delete(BASE + '/platform/self-serve/tenants/' + otherTenantId, {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { confirm_slug: otherSlug },
    });

    await ws.close();
  });

  test('subscribe/unsubscribe dynamically updates filtering', async () => {
    const ticket = await getWsTicket(context, token);
    await page.goto(BASE + '/platform/console');
    await page.waitForTimeout(1000);

    const ws = await connectWebSocket(page, ticket);
    await ws.waitForOpen();

    // Subscribe to team:changed
    await ws.send({ type: 'subscribe', topics: ['team:changed'] });
    await page.waitForTimeout(300);

    // Trigger team event — should receive it
    const email1 = 'ws-dynsub-' + Date.now() + '@test.local';
    await context.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { email: email1, role: 'viewer', tenant_ids: [tenantId] },
    });
    await ws.waitForMessage('msg.type === "event" && msg.topic === "team:changed"', 5000);

    // Now unsubscribe
    await ws.send({ type: 'unsubscribe', topics: ['team:changed'] });
    await page.waitForTimeout(300);

    // Clear message buffer
    await page.evaluate(() => { (window as any).__ws_messages = []; });

    // Trigger another team event — should NOT receive it
    const email2 = 'ws-dynsub2-' + Date.now() + '@test.local';
    await context.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
      data: { email: email2, role: 'viewer', tenant_ids: [tenantId] },
    });
    await page.waitForTimeout(2000);
    const messages = await ws.getMessages();
    expect(messages.filter((m: any) => m.topic === 'team:changed').length).toBe(0);

    await ws.close();
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/websocket.spec.ts
git commit -m "test(ws): event delivery e2e tests — team:changed, tenant:activity, isolation"
```

---

## Task 4: WebSocket E2E Tests — Health Endpoint

**Files:**
- Modify: `admin/e2e/websocket.spec.ts`

- [ ] **Step 1: Add Block 3 (health/connection count) test**

Append to `admin/e2e/websocket.spec.ts`:

```typescript
// ── Block 3: Connection Count & Health ───────────────────────────────────

test.describe('WS Health & Connection Count', () => {
  test('health endpoint reports ws_connections count', async ({ browser }) => {
    const auth = await loginAsTestUser(browser);
    const token = await getAccessToken(auth.context);

    // Check baseline
    const health0 = await auth.context.request.get(BASE + '/health');
    const h0 = await health0.json();
    const baseline = h0.ws_connections ?? 0;

    // Open first WS connection
    const ticket1 = await getWsTicket(auth.context, token);
    await auth.page.goto(BASE + '/platform/console');
    await auth.page.waitForTimeout(500);
    const ws1 = await connectWebSocket(auth.page, ticket1);
    await ws1.waitForOpen();
    await auth.page.waitForTimeout(500);

    const health1 = await auth.context.request.get(BASE + '/health');
    const h1 = await health1.json();
    expect(h1.ws_connections).toBe(baseline + 1);

    // Open second WS connection in a new page
    const page2 = await auth.context.newPage();
    const ticket2 = await getWsTicket(auth.context, token);
    await page2.goto(BASE + '/platform/console');
    await page2.waitForTimeout(500);
    const ws2 = await connectWebSocket(page2, ticket2);
    await ws2.waitForOpen();
    await page2.waitForTimeout(500);

    const health2 = await auth.context.request.get(BASE + '/health');
    const h2 = await health2.json();
    expect(h2.ws_connections).toBe(baseline + 2);

    // Close first connection
    await ws1.close();
    await auth.page.waitForTimeout(1000);

    const health3 = await auth.context.request.get(BASE + '/health');
    const h3 = await health3.json();
    expect(h3.ws_connections).toBe(baseline + 1);

    // Cleanup
    await ws2.close();
    await page2.close();
    await auth.context.close();
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/websocket.spec.ts
git commit -m "test(ws): health endpoint ws_connections count e2e test"
```

---

## Task 5: Team RBAC E2E Tests — Helpers & Invite Lifecycle

**Files:**
- Create: `admin/e2e/team-rbac.spec.ts`

- [ ] **Step 1: Create team-rbac.spec.ts with helpers and Block 1**

Create `admin/e2e/team-rbac.spec.ts`:

```typescript
import { test, expect, BrowserContext, APIRequestContext } from '@playwright/test';
import { BASE } from './helpers';

const TEST_PASSWORD = 'E2eTestPassword123!';

// ── Helpers ──────────────────────────────────────────────────────────────

async function postWithRetry(
  request: APIRequestContext,
  url: string,
  options: { headers?: Record<string, string>; data?: unknown },
  attempts = 4,
) {
  let lastResponse: any;
  for (let i = 0; i < attempts; i++) {
    lastResponse = await request.post(url, options);
    if (lastResponse.status() !== 429) return lastResponse;
    const retryAfter = Number(lastResponse.headers()['retry-after'] || '1');
    await new Promise(r => setTimeout(r, Math.max(500, retryAfter * 1000)));
  }
  return lastResponse;
}

async function createTestAccount(request: APIRequestContext, label: string): Promise<{
  token: string;
  userId: string;
  email: string;
}> {
  const email = `e2e-${label}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}@test.local`;
  const res = await postWithRetry(request, BASE + '/auth/register', {
    headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
    data: { email, password: TEST_PASSWORD, name: 'E2E ' + label },
  });
  expect(res.ok(), `createTestAccount(${label}) failed: ${res.status()}`).toBeTruthy();
  const data = await res.json();
  return { token: data.access_token, userId: data.user?.id, email };
}

async function createTestTenant(
  request: APIRequestContext,
  token: string,
  name: string,
): Promise<{ id: string; slug: string }> {
  const slug = 'e2e-' + name.toLowerCase().replace(/\s/g, '-') + '-' + Date.now().toString(36);
  const res = await request.post(BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' },
    data: { name, slug },
  });
  expect(res.ok(), `createTestTenant(${name}) failed: ${res.status()}`).toBeTruthy();
  const data = await res.json();
  return { id: data.id || data.tenant?.id, slug };
}

async function inviteMember(
  request: APIRequestContext,
  ownerToken: string,
  email: string,
  role: string,
  tenantIds: string[],
): Promise<{ inviteId: string; inviteToken: string }> {
  const res = await request.post(BASE + '/platform/admin/team/invites', {
    headers: { Authorization: 'Bearer ' + ownerToken, 'Content-Type': 'application/json' },
    data: { email, role, tenant_ids: tenantIds },
  });
  expect(res.ok(), `inviteMember(${email}) failed: ${res.status()}`).toBeTruthy();
  const data = await res.json();
  return { inviteId: data.invite?.id, inviteToken: data.invite_token };
}

async function acceptInvite(request: APIRequestContext, memberToken: string, inviteToken: string) {
  const res = await request.post(BASE + '/platform/admin/invites/accept', {
    headers: { Authorization: 'Bearer ' + memberToken, 'Content-Type': 'application/json' },
    data: { token: inviteToken },
  });
  expect(res.ok(), `acceptInvite failed: ${res.status()}`).toBeTruthy();
  return res.json();
}

async function listTeam(request: APIRequestContext, token: string): Promise<any> {
  const res = await request.get(BASE + '/platform/admin/team', {
    headers: { Authorization: 'Bearer ' + token },
  });
  expect(res.ok()).toBeTruthy();
  return res.json();
}

async function listTenants(request: APIRequestContext, token: string): Promise<any[]> {
  const res = await request.get(BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token },
  });
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  return data.tenants || [];
}

// ── Block 1: Invite Lifecycle ────────────────────────────────────────────

test.describe('Invite Lifecycle', () => {
  let ownerA: { token: string; userId: string; email: string };
  let tenantA1: { id: string; slug: string };
  let memberC: { token: string; userId: string; email: string };
  let inviteC: { inviteId: string; inviteToken: string };

  test.beforeAll(async ({ request }) => {
    ownerA = await createTestAccount(request, 'owner-inv');
    tenantA1 = await createTestTenant(request, ownerA.token, 'Invite Test');
    memberC = await createTestAccount(request, 'member-inv');
  });

  test.afterAll(async ({ request }) => {
    if (tenantA1?.id) {
      await request.delete(BASE + '/platform/self-serve/tenants/' + tenantA1.id, {
        headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
        data: { confirm_slug: tenantA1.slug },
      });
    }
  });

  test('owner can create invite with email and tenant assignments', async ({ request }) => {
    const res = await request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
      data: { email: memberC.email, role: 'viewer', tenant_ids: [tenantA1.id] },
    });
    expect(res.ok()).toBeTruthy();
    const data = await res.json();
    expect(data.invite?.id).toBeTruthy();
    expect(data.invite?.status).toBe('pending');
    expect(data.invite_token).toBeTruthy();
    expect(data.invite_url).toBeTruthy();
    inviteC = { inviteId: data.invite.id, inviteToken: data.invite_token };
  });

  test('pending invite appears in team list', async ({ request }) => {
    const team = await listTeam(request, ownerA.token);
    const pending = team.pending_invites || team.invites || [];
    const match = pending.find((inv: any) => inv.email === memberC.email);
    expect(match).toBeTruthy();
    expect(match.status).toBe('pending');
  });

  test('invited member can accept invite', async ({ request }) => {
    const data = await acceptInvite(request, memberC.token, inviteC.inviteToken);
    expect(data.accepted).toBe(true);
  });

  test('accepted member appears in team member list', async ({ request }) => {
    const team = await listTeam(request, ownerA.token);
    const members = team.members || [];
    const match = members.find((m: any) => m.email === memberC.email);
    expect(match).toBeTruthy();
    expect(match.status).toBe('active');
  });

  test('owner can revoke a pending invite', async ({ request }) => {
    // Create a new invite to revoke
    const newEmail = 'e2e-revoke-' + Date.now() + '@test.local';
    const inv = await inviteMember(request, ownerA.token, newEmail, 'viewer', [tenantA1.id]);

    // Revoke it
    const res = await request.post(BASE + '/platform/admin/team/invites/' + inv.inviteId + '/revoke', {
      headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
    });
    expect(res.ok()).toBeTruthy();
    const data = await res.json();
    expect(data.status).toBe('revoked');
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/team-rbac.spec.ts
git commit -m "test(rbac): invite lifecycle e2e tests — create, accept, revoke"
```

---

## Task 6: Team RBAC E2E Tests — Role Enforcement

**Files:**
- Modify: `admin/e2e/team-rbac.spec.ts`

- [ ] **Step 1: Add Block 2 (role enforcement) tests**

Append to `admin/e2e/team-rbac.spec.ts`:

```typescript
// ── Block 2: Role Enforcement ────────────────────────────────────────────

test.describe('Role Enforcement', () => {
  let ownerA: { token: string; userId: string; email: string };
  let tenantA1: { id: string; slug: string };
  let tenantA2: { id: string; slug: string };
  let memberC: { token: string; userId: string; email: string }; // viewer on A1 only
  let memberD: { token: string; userId: string; email: string }; // admin on A1 + A2
  let memberCId: string;

  test.beforeAll(async ({ request }) => {
    ownerA = await createTestAccount(request, 'owner-role');
    tenantA1 = await createTestTenant(request, ownerA.token, 'Role Test 1');
    tenantA2 = await createTestTenant(request, ownerA.token, 'Role Test 2');
    memberC = await createTestAccount(request, 'viewer-role');
    memberD = await createTestAccount(request, 'admin-role');

    // Invite C as viewer on A1 only
    const invC = await inviteMember(request, ownerA.token, memberC.email, 'viewer', [tenantA1.id]);
    await acceptInvite(request, memberC.token, invC.inviteToken);

    // Invite D as admin on A1 + A2
    const invD = await inviteMember(request, ownerA.token, memberD.email, 'admin', [tenantA1.id, tenantA2.id]);
    await acceptInvite(request, memberD.token, invD.inviteToken);

    // Get memberC's member ID for later tests
    const team = await listTeam(request, ownerA.token);
    const cMember = (team.members || []).find((m: any) => m.email === memberC.email);
    memberCId = cMember?.id;
  });

  test.afterAll(async ({ request }) => {
    for (const t of [tenantA1, tenantA2]) {
      if (t?.id) {
        await request.delete(BASE + '/platform/self-serve/tenants/' + t.id, {
          headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
          data: { confirm_slug: t.slug },
        });
      }
    }
  });

  test('viewer can read tenant data', async ({ request }) => {
    const tenants = await listTenants(request, memberC.token);
    const a1 = tenants.find((t: any) => t.id === tenantA1.id);
    expect(a1).toBeTruthy();
    expect(a1.access).toBe('team');
    expect(a1.team_role).toBe('viewer');
  });

  test('viewer cannot delete a tenant', async ({ request }) => {
    const res = await request.delete(BASE + '/platform/self-serve/tenants/' + tenantA1.id, {
      headers: { Authorization: 'Bearer ' + memberC.token, 'Content-Type': 'application/json' },
      data: { confirm_slug: tenantA1.slug },
    });
    expect(res.status()).toBeGreaterThanOrEqual(403);
  });

  test('viewer cannot rotate tenant API key', async ({ request }) => {
    const res = await request.post(BASE + '/platform/self-serve/tenants/' + tenantA1.id + '/rotate-key', {
      headers: { Authorization: 'Bearer ' + memberC.token, 'Content-Type': 'application/json' },
    });
    expect(res.status()).toBeGreaterThanOrEqual(403);
  });

  test('admin can access mutation endpoints', async ({ request }) => {
    const res = await request.patch(BASE + '/platform/self-serve/tenants/' + tenantA1.id, {
      headers: { Authorization: 'Bearer ' + memberD.token, 'Content-Type': 'application/json' },
      data: { name: 'Role Test 1 Updated' },
    });
    expect(res.ok()).toBeTruthy();
  });

  test('member only sees assigned tenants', async ({ request }) => {
    // C should see A1 but NOT A2
    const tenantsC = await listTenants(request, memberC.token);
    expect(tenantsC.some((t: any) => t.id === tenantA1.id)).toBe(true);
    expect(tenantsC.some((t: any) => t.id === tenantA2.id)).toBe(false);

    // D should see both A1 and A2
    const tenantsD = await listTenants(request, memberD.token);
    expect(tenantsD.some((t: any) => t.id === tenantA1.id)).toBe(true);
    expect(tenantsD.some((t: any) => t.id === tenantA2.id)).toBe(true);
  });

  test('account owner cannot remove themselves', async ({ request }) => {
    // Get owner's member ID
    const team = await listTeam(request, ownerA.token);
    const ownerMember = (team.members || []).find((m: any) => m.email === ownerA.email);
    expect(ownerMember).toBeTruthy();

    const res = await request.delete(BASE + '/platform/admin/team/' + ownerMember.id, {
      headers: { Authorization: 'Bearer ' + ownerA.token },
    });
    expect(res.ok()).toBe(false);
    const data = await res.json();
    expect(data.error).toBe('cannot_remove_self');
  });

  test('member cannot remove other members', async ({ request }) => {
    // Get D's member ID
    const team = await listTeam(request, ownerA.token);
    const dMember = (team.members || []).find((m: any) => m.email === memberD.email);

    // C (viewer) tries to remove D — should fail (only owner can manage team)
    const res = await request.delete(BASE + '/platform/admin/team/' + dMember.id, {
      headers: { Authorization: 'Bearer ' + memberC.token },
    });
    expect(res.ok()).toBe(false);
  });

  test('owner can update member role', async ({ request }) => {
    const res = await request.patch(BASE + '/platform/admin/team/' + memberCId, {
      headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
      data: { tenant_assignments: [{ tenant_id: tenantA1.id, role: 'editor' }] },
    });
    expect(res.ok()).toBeTruthy();

    // Verify updated role
    const tenants = await listTenants(request, memberC.token);
    const a1 = tenants.find((t: any) => t.id === tenantA1.id);
    expect(a1?.team_role).toBe('editor');
  });

  test('owner can remove a team member', async ({ request }) => {
    const res = await request.delete(BASE + '/platform/admin/team/' + memberCId, {
      headers: { Authorization: 'Bearer ' + ownerA.token },
    });
    expect(res.ok()).toBeTruthy();

    // C should no longer see A1
    const tenants = await listTenants(request, memberC.token);
    expect(tenants.some((t: any) => t.id === tenantA1.id)).toBe(false);
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/team-rbac.spec.ts
git commit -m "test(rbac): role enforcement e2e tests — viewer/admin/owner permissions"
```

---

## Task 7: Team RBAC E2E Tests — Cross-Account Isolation

**Files:**
- Modify: `admin/e2e/team-rbac.spec.ts`

- [ ] **Step 1: Add Block 3 (cross-account isolation) tests**

Append to `admin/e2e/team-rbac.spec.ts`:

```typescript
// ── Block 3: Cross-Account Isolation ─────────────────────────────────────

test.describe('Cross-Account Isolation', () => {
  let ownerA: { token: string; userId: string; email: string };
  let ownerB: { token: string; userId: string; email: string };
  let tenantA1: { id: string; slug: string };
  let tenantB1: { id: string; slug: string };
  let memberD: { token: string; userId: string; email: string }; // on A's team

  test.beforeAll(async ({ request }) => {
    ownerA = await createTestAccount(request, 'owner-iso-a');
    ownerB = await createTestAccount(request, 'owner-iso-b');
    tenantA1 = await createTestTenant(request, ownerA.token, 'Iso A Tenant');
    tenantB1 = await createTestTenant(request, ownerB.token, 'Iso B Tenant');
    memberD = await createTestAccount(request, 'member-iso-d');

    // Invite D to Owner A's team
    const inv = await inviteMember(request, ownerA.token, memberD.email, 'admin', [tenantA1.id]);
    await acceptInvite(request, memberD.token, inv.inviteToken);
  });

  test.afterAll(async ({ request }) => {
    if (tenantA1?.id) {
      await request.delete(BASE + '/platform/self-serve/tenants/' + tenantA1.id, {
        headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
        data: { confirm_slug: tenantA1.slug },
      });
    }
    if (tenantB1?.id) {
      await request.delete(BASE + '/platform/self-serve/tenants/' + tenantB1.id, {
        headers: { Authorization: 'Bearer ' + ownerB.token, 'Content-Type': 'application/json' },
        data: { confirm_slug: tenantB1.slug },
      });
    }
  });

  test('owner A cannot see owner B team members', async ({ request }) => {
    const teamA = await listTeam(request, ownerA.token);
    const emails = (teamA.members || []).map((m: any) => m.email);
    expect(emails).not.toContain(ownerB.email);
  });

  test('owner A cannot see owner B tenants', async ({ request }) => {
    const tenants = await listTenants(request, ownerA.token);
    expect(tenants.some((t: any) => t.id === tenantB1.id)).toBe(false);
  });

  test('member of account A cannot access account B tenant', async ({ request }) => {
    // D is on A's team — try to access B's tenant stats
    const res = await request.get(
      BASE + '/platform/self-serve/tenants/' + tenantB1.id + '/stats',
      { headers: { Authorization: 'Bearer ' + memberD.token } },
    );
    // Should be 404 (not found in D's accessible tenants) or 403
    expect(res.status()).toBeGreaterThanOrEqual(400);
  });

  test('invite from owner A cannot grant access to owner B tenant', async ({ request }) => {
    const newEmail = 'e2e-cross-inv-' + Date.now() + '@test.local';
    const res = await request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + ownerA.token, 'Content-Type': 'application/json' },
      data: { email: newEmail, role: 'viewer', tenant_ids: [tenantB1.id] },
    });
    // Server should reject — B1 is not owned by A
    expect(res.ok()).toBe(false);
  });

  test('accepting A invite does not grant access to B data', async ({ request }) => {
    // D already accepted A's invite — verify D has zero visibility into B
    const tenantsD = await listTenants(request, memberD.token);
    expect(tenantsD.some((t: any) => t.id === tenantB1.id)).toBe(false);

    // D should not see B's team
    // D can only see their own team (A's team), not B's
    const teamD = await listTeam(request, memberD.token);
    const teamEmails = (teamD.members || []).map((m: any) => m.email);
    expect(teamEmails).not.toContain(ownerB.email);
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/team-rbac.spec.ts
git commit -m "test(rbac): cross-account isolation e2e tests — tenant/team boundary enforcement"
```

---

## Summary

| Task | Track | Tests Added | Files |
|------|-------|-------------|-------|
| 1 | Fix broken | 0 (plumbing fix) | 7 modified |
| 2 | WebSocket | 7 (ticket auth, lifecycle) | 1 created |
| 3 | WebSocket | 6 (event delivery) | 1 modified |
| 4 | WebSocket | 1 (health endpoint) | 1 modified |
| 5 | RBAC | 5 (invite lifecycle) | 1 created |
| 6 | RBAC | 9 (role enforcement) | 1 modified |
| 7 | RBAC | 5 (cross-account isolation) | 1 modified |
| **Total** | | **33 new tests** | **2 new, 7 modified** |
