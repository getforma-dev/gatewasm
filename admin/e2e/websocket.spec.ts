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
