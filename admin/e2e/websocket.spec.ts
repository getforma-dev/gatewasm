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

/** Make an authenticated JSON API call using Node's native fetch.
 *  Playwright's BrowserContext.request doesn't reliably send JSON bodies,
 *  so we use native fetch for triggering server-side events in WS tests. */
async function apiPost(path: string, token: string, body: unknown): Promise<{ ok: boolean; status: number; data: any }> {
  const res = await fetch(BASE + path, {
    method: 'POST',
    headers: { 'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await res.text();
  let data: any;
  try { data = JSON.parse(text); } catch { data = text; }
  return { ok: res.ok, status: res.status, data };
}

async function apiDelete(path: string, token: string, body?: unknown): Promise<{ ok: boolean; status: number }> {
  const res = await fetch(BASE + path, {
    method: 'DELETE',
    headers: { 'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  return { ok: res.ok, status: res.status };
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
     * Polls messages from the browser context on the Node side to avoid
     * `new Function()` / CSP issues inside the page.
     */
    async waitForMessage(predicate: (parsed: any) => boolean, timeoutMs = 5000): Promise<any> {
      const start = Date.now();
      while (Date.now() - start < timeoutMs) {
        const raw: string[] = await page.evaluate(() => (window as any).__ws_messages as string[]);
        for (const m of raw) {
          try {
            const parsed = JSON.parse(m);
            if (predicate(parsed)) return parsed;
          } catch { /* skip non-JSON */ }
        }
        await page.waitForTimeout(100);
      }
      throw new Error('waitForMessage timed out after ' + timeoutMs + 'ms');
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
    await ws.waitForMessage((msg) => msg.type === 'pong');

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

    await ws.waitForMessage((msg) => msg.type === 'pong', 5000);

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

    const result = await apiPost('/platform/self-serve/tenants', accessToken, { name: 'WS Test Tenant', slug: tenantSlug });
    expect(result.ok).toBe(true);
    tenantId = (result.data.tenant?.id || result.data.id) as string;
  });

  test.afterAll(async () => {
    if (tenantId) {
      const token = await getAccessToken(evtContext);
      await apiDelete('/platform/self-serve/tenants/' + tenantId, token, { confirm_slug: tenantSlug });
    }
    await evtContext.close();
  });

  // -------------------------------------------------------------------------
  // Test 1 — receives team:changed when invite is created
  // -------------------------------------------------------------------------
  test('receives team:changed when invite is created', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForLoadState('networkidle');

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));
    const inviteEmail = 'invite-' + Date.now() + '@test.local';

    const result = await apiPost('/platform/admin/team/invites', token, { email: inviteEmail, role: 'viewer', tenant_ids: [tenantId] });
    expect(result.ok, `invite via apiPost failed: ${result.status} ${JSON.stringify(result.data)}`).toBe(true);

    await ws.waitForMessage(
      (msg) => msg.type === 'event' && msg.topic === 'team:changed' && msg.hint === 'invite_created',
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
  // Test 2 — receives tenant:activity on platform login
  // -------------------------------------------------------------------------
  // NOTE: The WS connection is scoped to the platform tenant (ten_platform).
  // tenant:activity events from child tenants have a different tenant_id and
  // won't match. To test tenant:activity delivery, we trigger a platform login
  // which emits with tenant_id "ten_platform".
  test('receives tenant:activity on platform login', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForLoadState('networkidle');

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity'] }));
    // Trigger a platform login (emits tenant:activity with tenant_id "ten_platform")
    const loginEmail = 'ws-login-' + Date.now() + '@test.local';
    // Register first (X-Tenant-Slug header — cannot use apiPost Bearer auth helper)
    await fetch(BASE + '/auth/register', {
      method: 'POST',
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: loginEmail, password: 'E2eTestPassword123!', name: 'WS Login Test' }),
    });

    await ws.waitForMessage(
      (msg) => msg.type === 'event' && msg.topic === 'tenant:activity' && msg.hint === 'signup',
      8000,
    );

    const messages = await ws.getMessages();
    const found = messages.some((m) => {
      try {
        const p = JSON.parse(m);
        return p.type === 'event' && p.topic === 'tenant:activity';
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
    await evtPage.waitForLoadState('networkidle');

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity', 'metrics:updated'] }));
    // Platform signup emits both tenant:activity and metrics:updated with "ten_platform"
    const email = 'ws-dual-' + Date.now() + '@test.local';
    await fetch(BASE + '/auth/register', {
      method: 'POST',
      headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password: 'E2eTestPassword123!', name: 'WS Dual Emit Test' }),
    });

    await ws.waitForMessage(
      (msg) => msg.type === 'event' && msg.topic === 'tenant:activity',
      8000,
    );
    await ws.waitForMessage(
      (msg) => msg.type === 'event' && msg.topic === 'metrics:updated',
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
    await evtPage.waitForLoadState('networkidle');

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    // Subscribe only to metrics:updated — NOT team:changed
    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['metrics:updated'] }));
    // Trigger team:changed event (invite creation emits with ten_platform)
    const inviteEmail = 'ws-unsub-test-' + Date.now() + '@test.local';
    await apiPost('/platform/admin/team/invites', token, { email: inviteEmail, role: 'viewer', tenant_ids: [tenantId] });

    await evtPage.waitForTimeout(2000);

    const messages = await ws.getMessages();
    const hasTeamEvent = messages.some((m) => {
      try { const p = JSON.parse(m); return p.type === 'event' && p.topic === 'team:changed'; } catch { return false; }
    });
    expect(hasTeamEvent).toBe(false);

    await ws.close();
  });

  // -------------------------------------------------------------------------
  // Test 5 — child tenant activity does not leak to platform-scoped WS
  // -------------------------------------------------------------------------
  test('child tenant activity does not leak to platform-scoped WS', async () => {
    // Create a second tenant
    const token = await getAccessToken(evtContext);
    const otherSlug = 'e2e-ws-other-' + Date.now().toString(36);
    const createRes = await apiPost('/platform/self-serve/tenants', token, { name: 'WS Other Tenant', slug: otherSlug });
    expect(createRes.ok).toBe(true);
    const otherTenantId = (createRes.data.tenant?.id || createRes.data.id) as string;

    try {
      await evtPage.goto(BASE + '/platform/console');
      await evtPage.waitForTimeout(1000);

      const ticket = await getWsTicket(evtContext, token);
      const ws = await connectWebSocket(evtPage, ticket);
      await ws.waitForOpen();

      await ws.send(JSON.stringify({ type: 'subscribe', topics: ['tenant:activity'] }));
      // Register end user on the OTHER tenant — should not appear in this WS session
      const endUserEmail = 'end-user-other-' + Date.now() + '@test.local';
      await fetch(BASE + '/auth/register', {
        method: 'POST',
        headers: { 'X-Tenant-Slug': otherSlug, 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: endUserEmail, password: 'E2eTestPassword123!', name: 'WS Other User' }),
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
      await apiDelete('/platform/self-serve/tenants/' + otherTenantId, cleanToken, { confirm_slug: otherSlug });
    }
  });

  // -------------------------------------------------------------------------
  // Test 6 — subscribe/unsubscribe dynamically updates filtering
  // -------------------------------------------------------------------------
  test('subscribe/unsubscribe dynamically updates filtering', async () => {
    await evtPage.goto(BASE + '/platform/console');
    await evtPage.waitForLoadState('networkidle');

    const token = await getAccessToken(evtContext);
    const ticket = await getWsTicket(evtContext, token);
    const ws = await connectWebSocket(evtPage, ticket);
    await ws.waitForOpen();

    // Subscribe to team:changed
    await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));
    // Create first invite — should receive event
    const email1 = 'dyn-invite-1-' + Date.now() + '@test.local';
    const invite1Res = await apiPost('/platform/admin/team/invites', token, { email: email1, role: 'viewer', tenant_ids: [tenantId] });
    expect(invite1Res.ok, `invite1 failed: ${invite1Res.status}`).toBe(true);

    await ws.waitForMessage(
      (msg) => msg.type === 'event' && msg.topic === 'team:changed' && msg.hint === 'invite_created',
      8000,
    );

    // Unsubscribe from team:changed
    await ws.send(JSON.stringify({ type: 'unsubscribe', topics: ['team:changed'] }));
    // Clear message buffer
    await evtPage.evaluate(() => { (window as any).__ws_messages = []; });

    // Create second invite — should NOT receive event after unsubscribe
    const email2 = 'dyn-invite-2-' + Date.now() + '@test.local';
    const invite2Res = await apiPost('/platform/admin/team/invites', token, { email: email2, role: 'viewer', tenant_ids: [tenantId] });
    expect(invite2Res.ok, `invite2 failed: ${invite2Res.status}`).toBe(true);

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
      await page.waitForLoadState('networkidle');

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
      await page2.waitForLoadState('networkidle');

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
      await page.waitForTimeout(500);

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

// ---------------------------------------------------------------------------
// Block 4 — Reconnection & Server-Initiated Close
// ---------------------------------------------------------------------------

test.describe('WS Reconnection & Server-Initiated Close', () => {
  // -------------------------------------------------------------------------
  // Test 1 — Server closes connection after 5 invalid messages
  // -------------------------------------------------------------------------
  test('server closes connection after 5 invalid messages', async ({ browser }) => {
    const { context, page } = await loginAsTestUser(browser);

    try {
      const token = await getAccessToken(context);
      const ticket = await getWsTicket(context, token);
      await page.goto(BASE + '/platform/console');
      await page.waitForLoadState('networkidle');

      const ws = await connectWebSocket(page, ticket);
      await ws.waitForOpen();

      // Send 5 invalid messages (not valid JSON or wrong type)
      for (let i = 0; i < 5; i++) {
        await ws.send('this is not valid json ' + i);
      }

      // Server should close the connection after 5 invalid messages
      await ws.waitForClose(5000);
    } finally {
      await context.close();
    }
  });

  // -------------------------------------------------------------------------
  // Test 2 — Server sends auth_expired when JWT expires
  // -------------------------------------------------------------------------
  // The test server is configured with JWT_ACCESS_TOKEN_EXPIRY=15 (15 seconds).
  // The WS handler sets a timer based on the JWT's exp claim.
  test('server sends auth_expired when JWT expires', async ({ browser }) => {
    test.setTimeout(30000); // Extended timeout — waiting for 15s token expiry

    const { context, page } = await loginAsTestUser(browser);

    try {
      const token = await getAccessToken(context);
      const ticket = await getWsTicket(context, token);
      await page.goto(BASE + '/platform/console');
      await page.waitForLoadState('networkidle');

      const ws = await connectWebSocket(page, ticket);
      await ws.waitForOpen();

      // Subscribe so we can distinguish auth_expired from other messages
      await ws.send(JSON.stringify({ type: 'subscribe', topics: ['team:changed'] }));

      // Wait for the JWT to expire (15s configured in test env + buffer)
      await ws.waitForMessage(
        (msg) => msg.type === 'auth_expired',
        20000,
      );

      const messages = await ws.getMessages();
      const hasAuthExpired = messages.some((m) => {
        try { return JSON.parse(m).type === 'auth_expired'; } catch { return false; }
      });
      expect(hasAuthExpired).toBe(true);

      // Connection should be closed after auth_expired
      await ws.waitForClose(3000);
    } finally {
      await context.close();
    }
  });
});
