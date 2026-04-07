/**
 * WebSocket E2E Tests — Ticket Auth & Connection Lifecycle
 *
 * Validates the ticket-based WebSocket auth flow:
 *   POST /ws/ticket (JWT → single-use 30s ticket)
 *   ws://host/ws?ticket=<ticket>
 *
 * Block 1: Ticket issuance and connection lifecycle
 */
import { test, expect, Browser, BrowserContext, Page } from '@playwright/test';
import { loginAsTestUser } from './auth-helper';
import { BASE } from './helpers';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Exchange the refresh cookie for a short-lived access token. */
async function getAccessToken(context: BrowserContext): Promise<string> {
  const res = await context.request.post(BASE + '/t/platform/auth/token', {
    data: { grant_type: 'refresh_token' },
  });
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  return data.access_token as string;
}

/** Exchange an access token for a single-use WS ticket. */
async function getWsTicket(context: BrowserContext, token: string): Promise<string> {
  const res = await context.request.post(BASE + '/ws/ticket', {
    headers: { Authorization: 'Bearer ' + token },
  });
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  return data.ticket as string;
}

/**
 * Open a WebSocket inside the browser page via page.evaluate().
 *
 * Messages are stored in window.__ws_messages.
 * Returns a handle with helpers for interacting with the connection.
 */
async function connectWebSocket(page: Page, ticket: string) {
  const wsUrl = BASE.replace(/^http/, 'ws') + '/ws?ticket=' + ticket;

  await page.evaluate((url: string) => {
    (window as any).__ws_messages = [];
    (window as any).__ws_open = false;
    (window as any).__ws_closed = false;

    const ws = new WebSocket(url);

    ws.addEventListener('open', () => {
      (window as any).__ws_open = true;
    });
    ws.addEventListener('message', (evt: MessageEvent) => {
      (window as any).__ws_messages.push(evt.data);
    });
    ws.addEventListener('close', () => {
      (window as any).__ws_closed = true;
    });

    (window as any).__ws = ws;
  }, wsUrl);

  return {
    async waitForOpen(timeoutMs = 5000): Promise<void> {
      await page.waitForFunction(() => (window as any).__ws_open === true, { timeout: timeoutMs });
    },

    async waitForClose(timeoutMs = 10000): Promise<void> {
      await page.waitForFunction(() => (window as any).__ws_closed === true, { timeout: timeoutMs });
    },

    async send(msg: string): Promise<void> {
      await page.evaluate((m: string) => (window as any).__ws.send(m), msg);
    },

    async getMessages(): Promise<string[]> {
      return page.evaluate(() => (window as any).__ws_messages as string[]);
    },

    /**
     * Wait until a received message satisfies the predicate.
     * predicateString must be a JS arrow function source that accepts a
     * single message string, e.g. "(msg) => JSON.parse(msg).type === 'pong'"
     */
    async waitForMessage(predicateString: string, timeoutMs = 5000): Promise<void> {
      await page.waitForFunction(
        ([pred]: [string]) => {
          const messages: string[] = (window as any).__ws_messages;
          const fn = new Function('msg', 'return (' + pred + ')(msg)') as (m: string) => boolean;
          return messages.some((m) => { try { return fn(m); } catch { return false; } });
        },
        [predicateString],
        { timeout: timeoutMs },
      );
    },

    async close(): Promise<void> {
      await page.evaluate(() => (window as any).__ws?.close());
    },
  };
}

// ---------------------------------------------------------------------------
// Suite setup
// ---------------------------------------------------------------------------

let sharedBrowser: Browser;
let sharedContext: BrowserContext;
let sharedPage: Page;

test.describe('WS Ticket Auth & Connection Lifecycle', () => {
  test.beforeAll(async ({ browser }) => {
    sharedBrowser = browser;
    const { context, page } = await loginAsTestUser(browser);
    sharedContext = context;
    sharedPage = page;
  });

  test.afterAll(async () => {
    await sharedContext.close();
  });

  // -------------------------------------------------------------------------
  // Test 1 — POST /ws/ticket returns a ticket when authenticated
  // -------------------------------------------------------------------------
  test('POST /ws/ticket returns a ticket when authenticated', async () => {
    const token = await getAccessToken(sharedContext);
    const res = await sharedContext.request.post(BASE + '/ws/ticket', {
      headers: { Authorization: 'Bearer ' + token },
    });
    expect(res.status()).toBe(200);
    const data = await res.json();
    expect(typeof data.ticket).toBe('string');
    // Ticket must be a 64-character hex string
    expect(data.ticket).toMatch(/^[0-9a-f]{64}$/);
  });

  // -------------------------------------------------------------------------
  // Test 2 — POST /ws/ticket rejects unauthenticated requests
  // -------------------------------------------------------------------------
  test('POST /ws/ticket rejects unauthenticated requests', async () => {
    const res = await sharedContext.request.post(BASE + '/ws/ticket');
    expect(res.status()).toBe(401);
  });

  // -------------------------------------------------------------------------
  // Test 3 — WS upgrade succeeds with valid ticket
  // -------------------------------------------------------------------------
  test('WS upgrade succeeds with valid ticket', async () => {
    await sharedPage.goto(BASE + '/platform/console');
    const token = await getAccessToken(sharedContext);
    const ticket = await getWsTicket(sharedContext, token);
    const ws = await connectWebSocket(sharedPage, ticket);

    await ws.waitForOpen();

    // Send ping and verify pong
    await ws.send(JSON.stringify({ type: 'ping' }));
    await ws.waitForMessage('(msg) => { try { return JSON.parse(msg).type === "pong"; } catch(e) { return false; } }');

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 4 — WS upgrade rejects invalid ticket
  // -------------------------------------------------------------------------
  test('WS upgrade rejects invalid ticket', async () => {
    await sharedPage.goto(BASE + '/platform/console');
    const bogusTicket = 'a'.repeat(64);
    const ws = await connectWebSocket(sharedPage, bogusTicket);
    await ws.waitForClose();
  });

  // -------------------------------------------------------------------------
  // Test 5 — WS upgrade rejects reused ticket
  // -------------------------------------------------------------------------
  test('WS upgrade rejects reused ticket', async () => {
    await sharedPage.goto(BASE + '/platform/console');
    const token = await getAccessToken(sharedContext);
    const ticket = await getWsTicket(sharedContext, token);

    // First connection burns the ticket
    const ws1 = await connectWebSocket(sharedPage, ticket);
    await ws1.waitForOpen();
    await ws1.close();
    await ws1.waitForClose();

    // Navigate to a fresh page so the old WS state is gone
    await sharedPage.goto(BASE + '/platform/console');

    // Second connection with the same (now consumed) ticket must be rejected
    const ws2 = await connectWebSocket(sharedPage, ticket);
    await ws2.waitForClose();
  });

  // -------------------------------------------------------------------------
  // Test 6 — WS upgrade rejects expired ticket (30 s TTL)
  // -------------------------------------------------------------------------
  test('WS upgrade rejects expired ticket (30s TTL)', async () => {
    test.setTimeout(45000);

    await sharedPage.goto(BASE + '/platform/console');
    const token = await getAccessToken(sharedContext);
    const ticket = await getWsTicket(sharedContext, token);

    // Wait for the ticket to expire
    await sharedPage.waitForTimeout(31000);

    const ws = await connectWebSocket(sharedPage, ticket);
    await ws.waitForClose();
  });

  // -------------------------------------------------------------------------
  // Test 7 — Server sends pong in response to ping
  // -------------------------------------------------------------------------
  test('Server sends pong in response to ping', async () => {
    await sharedPage.goto(BASE + '/platform/console');
    const token = await getAccessToken(sharedContext);
    const ticket = await getWsTicket(sharedContext, token);
    const ws = await connectWebSocket(sharedPage, ticket);

    await ws.waitForOpen();
    await ws.send(JSON.stringify({ type: 'ping' }));

    await ws.waitForMessage(
      '(msg) => { try { return JSON.parse(msg).type === "pong"; } catch(e) { return false; } }',
      5000,
    );

    const messages = await ws.getMessages();
    const hasPong = messages.some((m) => {
      try { return JSON.parse(m).type === 'pong'; } catch { return false; }
    });
    expect(hasPong).toBe(true);

    await ws.close();
  });
});

// ---------------------------------------------------------------------------
// Block 2 — WS Event Delivery
// ---------------------------------------------------------------------------

test.describe('WS Event Delivery', () => {
  let evtContext: BrowserContext;
  let evtPage: Page;
  let accessToken: string;
  let tenantId: string;
  let tenantSlug: string;

  test.beforeAll(async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);
    evtContext = context;
    evtPage = page;

    accessToken = await getAccessToken(evtContext);

    tenantSlug = 'e2e-ws-' + Date.now().toString(36);

    const res = await evtContext.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
      data: { name: 'WS Test Tenant', slug: tenantSlug },
    });
    expect(res.ok()).toBeTruthy();
    const tenant = await res.json();
    tenantId = tenant.id as string;
  });

  test.afterAll(async () => {
    if (tenantId) {
      const token = await getAccessToken(evtContext);
      await evtContext.request.delete(BASE + '/platform/admin/tenants/' + tenantId, {
        headers: { Authorization: 'Bearer ' + token },
      });
    }
    await evtContext.close();
  });

  // -------------------------------------------------------------------------
  // Test 1 — receives team:changed when invite is created
  // -------------------------------------------------------------------------
  test('receives team:changed when invite is created', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForTimeout(1000);

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));
    await evtPage.waitForTimeout(300);

    const inviteEmail = 'invite-' + Date.now() + '@test.local';
    const inviteRes = await evtContext.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token },
      data: { email: inviteEmail, role: 'viewer', tenant_ids: [tenantId] },
    });
    expect(inviteRes.ok()).toBeTruthy();

    await ws.waitForMessage(
      '(msg) => { try { const p = JSON.parse(msg); return p.type === "event" && p.topic === "team:changed" && p.hint === "invite_created"; } catch(e) { return false; } }',
      8000,
    );

    const messages = await ws.getMessages();
    const found = messages.some((m) => {
      try {
        const p = JSON.parse(m);
        return p.type === 'event' && p.topic === 'team:changed' && p.hint === 'invite_created';
      } catch { return false; }
    });
    expect(found).toBe(true);

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 2 — receives tenant:activity on end user signup
  // -------------------------------------------------------------------------
  test('receives tenant:activity on end user signup', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForTimeout(1000);

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity'] }));
    await evtPage.waitForTimeout(300);

    const endUserEmail = 'end-user-' + Date.now() + '@test.local';
    const signupRes = await evtContext.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug },
      data: { email: endUserEmail, password: 'E2eTestPassword123!', name: 'WS End User' },
    });
    expect(signupRes.ok()).toBeTruthy();

    await ws.waitForMessage(
      '(msg) => { try { const p = JSON.parse(msg); return p.type === "event" && p.topic === "tenant:activity" && p.hint === "signup"; } catch(e) { return false; } }',
      8000,
    );

    const messages = await ws.getMessages();
    const found = messages.some((m) => {
      try {
        const p = JSON.parse(m);
        return p.type === 'event' && p.topic === 'tenant:activity' && p.hint === 'signup';
      } catch { return false; }
    });
    expect(found).toBe(true);

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 3 — receives metrics:updated alongside tenant:activity
  // -------------------------------------------------------------------------
  test('receives metrics:updated alongside tenant:activity', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForTimeout(1000);

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity', 'metrics:updated'] }));
    await evtPage.waitForTimeout(300);

    const endUserEmail = 'end-user-metrics-' + Date.now() + '@test.local';
    const signupRes = await evtContext.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug },
      data: { email: endUserEmail, password: 'E2eTestPassword123!', name: 'WS Metrics User' },
    });
    expect(signupRes.ok()).toBeTruthy();

    await ws.waitForMessage(
      '(msg) => { try { const p = JSON.parse(msg); return p.type === "event" && p.topic === "tenant:activity"; } catch(e) { return false; } }',
      8000,
    );
    await ws.waitForMessage(
      '(msg) => { try { const p = JSON.parse(msg); return p.type === "event" && p.topic === "metrics:updated"; } catch(e) { return false; } }',
      8000,
    );

    const messages = await ws.getMessages();
    const hasActivity = messages.some((m) => {
      try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'tenant:activity'; } catch { return false; }
    });
    const hasMetrics = messages.some((m) => {
      try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'metrics:updated'; } catch { return false; }
    });
    expect(hasActivity).toBe(true);
    expect(hasMetrics).toBe(true);

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 4 — does not receive events for unsubscribed topics
  // -------------------------------------------------------------------------
  test('does not receive events for unsubscribed topics', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForTimeout(1000);

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    // Subscribe only to team:changed — NOT tenant:activity
    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));
    await evtPage.waitForTimeout(300);

    const endUserEmail = 'end-user-unsub-' + Date.now() + '@test.local';
    await evtContext.request.post(BASE + '/auth/register', {
      headers: { 'X-Tenant-Slug': tenantSlug },
      data: { email: endUserEmail, password: 'E2eTestPassword123!', name: 'WS Unsub User' },
    });

    await evtPage.waitForTimeout(2000);

    const messages = await ws.getMessages();
    const hasActivity = messages.some((m) => {
      try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'tenant:activity'; } catch { return false; }
    });
    expect(hasActivity).toBe(false);

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 5 — does not receive events for other tenants
  // -------------------------------------------------------------------------
  test('does not receive events for other tenants', async () => {
    // Create a second tenant
    const token = await getAccessToken(evtContext);
    const otherSlug = 'e2e-ws-other-' + Date.now().toString(36);
    const createRes = await evtContext.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + token },
      data: { name: 'WS Other Tenant', slug: otherSlug },
    });
    expect(createRes.ok()).toBeTruthy();
    const otherTenant = await createRes.json();
    const otherTenantId = otherTenant.id as string;

    try {
      await evtPage.goto(BASE + '/platform/console');
      await evtPage.waitForTimeout(1000);

      const ticket = await getWsTicket(evtContext, token);
      const ws = await connectWebSocket(evtPage, ticket);
      await ws.waitForOpen();

      await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity'] }));
      await evtPage.waitForTimeout(300);

      // Register end user on the OTHER tenant — should not appear in this WS session
      const endUserEmail = 'end-user-other-' + Date.now() + '@test.local';
      await evtContext.request.post(BASE + '/auth/register', {
        headers: { 'X-Tenant-Slug': otherSlug },
        data: { email: endUserEmail, password: 'E2eTestPassword123!', name: 'WS Other User' },
      });

      await evtPage.waitForTimeout(2000);

      const messages = await ws.getMessages();
      const hasActivity = messages.some((m) => {
        try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'tenant:activity'; } catch { return false; }
      });
      expect(hasActivity).toBe(false);

      await ws.close();
    } finally {
      // Clean up second tenant
      const cleanToken = await getAccessToken(evtContext);
      await evtContext.request.delete(BASE + '/platform/admin/tenants/' + otherTenantId, {
        headers: { Authorization: 'Bearer ' + cleanToken },
      });
    }
  });

  // -------------------------------------------------------------------------
  // Test 6 — subscribe/unsubscribe dynamically updates filtering
  // -------------------------------------------------------------------------
  test('subscribe/unsubscribe dynamically updates filtering', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForTimeout(1000);

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    // Subscribe to team:changed
    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));
    await evtPage.waitForTimeout(300);

    // Create first invite — should receive event
    const email1 = 'dyn-invite-1-' + Date.now() + '@test.local';
    const invite1Res = await evtContext.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token },
      data: { email: email1, role: 'viewer', tenant_ids: [tenantId] },
    });
    expect(invite1Res.ok()).toBeTruthy();

    await ws.waitForMessage(
      '(msg) => { try { const p = JSON.parse(msg); return p.type === "event" && p.topic === "team:changed" && p.hint === "invite_created"; } catch(e) { return false; } }',
      8000,
    );

    // Unsubscribe from team:changed
    await ws.send(JSON.stringify({ type: 'unsubscribe', topics: ['team:changed'] }));
    await evtPage.waitForTimeout(300);

    // Clear message buffer
    await evtPage.evaluate(() => { (window as any).__ws_messages = []; });

    // Create second invite — should NOT receive event after unsubscribe
    const email2 = 'dyn-invite-2-' + Date.now() + '@test.local';
    const invite2Res = await evtContext.request.post(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + token },
      data: { email: email2, role: 'viewer', tenant_ids: [tenantId] },
    });
    expect(invite2Res.ok()).toBeTruthy();

    await evtPage.waitForTimeout(2000);

    const messages = await ws.getMessages();
    const hasEvent = messages.some((m) => {
      try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'team:changed'; } catch { return false; }
    });
    expect(hasEvent).toBe(false);

    await ws.close();
  });
});

// ---------------------------------------------------------------------------
// Block 3 — WS Health & Connection Count
// ---------------------------------------------------------------------------

test.describe('WS Health & Connection Count', () => {
  // -------------------------------------------------------------------------
  // Test 1 — health endpoint reports ws_connections count
  // -------------------------------------------------------------------------
  test('health endpoint reports ws_connections count', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);

    try {
      // Get baseline ws_connections from /health
      const healthRes = await context.request.get(BASE + '/health');
      expect(healthRes.ok()).toBeTruthy();
      const healthData = await healthRes.json();
      const baseline: number = healthData.ws_connections ?? 0;

      // Open first WS connection
      await page.goto(BASE + '/platform/console');
      await page.waitForTimeout(1000);

      const token1 = await getAccessToken(context);
      const ticket1 = await getWsTicket(context, token1);
      const ws1 = await connectWebSocket(page, ticket1);
      await ws1.waitForOpen();

      // Verify count = baseline + 1
      const health1 = await context.request.get(BASE + '/health');
      const data1 = await health1.json();
      expect(data1.ws_connections).toBe(baseline + 1);

      // Open a second connection on a new page in the same context
      const page2 = await context.newPage();
      await page2.goto(BASE + '/platform/console');
      await page2.waitForTimeout(1000);

      const token2 = await getAccessToken(context);
      const ticket2 = await getWsTicket(context, token2);
      const ws2 = await connectWebSocket(page2, ticket2);
      await ws2.waitForOpen();

      // Verify count = baseline + 2
      const health2 = await context.request.get(BASE + '/health');
      const data2 = await health2.json();
      expect(data2.ws_connections).toBe(baseline + 2);

      // Close first connection and wait for server to register the close
      await ws1.close();
      await page.waitForTimeout(1000);

      // Verify count = baseline + 1
      const health3 = await context.request.get(BASE + '/health');
      const data3 = await health3.json();
      expect(data3.ws_connections).toBe(baseline + 1);

      // Clean up
      await ws2.close();
      await page2.close();
    } finally {
      await context.close();
    }
  });
});
