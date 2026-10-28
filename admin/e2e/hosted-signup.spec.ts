import { test as base, expect, type APIRequestContext, type Page } from '@playwright/test';
import { createHash, createPublicKey, randomBytes, verify } from 'node:crypto';

// Synthetic issuer acceptance only. APIRequestContext is the backend; browser
// traffic must never receive its invitation secret or exchanged tokens.
const callback = 'https://genui.example.test/auth/callback';
const password = 'HostedInvite123!';
type Tenant = { id: string; slug: string; key: string; client: string; origin: string };
const platformHeaders = { Authorization: `Bearer ${process.env.PLATFORM_ADMIN_KEY || process.env.MASTER_PLATFORM_KEY || 'plat_local_dev_key'}` };
const test = base.extend<{ tenant: Tenant }>({
  tenant: async ({ request, baseURL }, use) => {
    if (!baseURL || !['127.0.0.1', 'localhost', '[::1]'].includes(new URL(baseURL).hostname)) {
      throw new Error('Hosted signup fixtures require an explicitly selected local synthetic server');
    }
    const slug = `hosted-invite-${randomBytes(6).toString('hex')}`;
    const created = await request.post(`${baseURL}/platform/tenants`, {
      headers: platformHeaders,
      data: { name: 'Invited account test', slug, owner_email: 'owner@example.test', allowed_providers: ['email', 'google', 'github'], signup_mode: 'invite_only' },
    });
    expect(created.status()).toBe(201);
    const data = await created.json();
    const tenant = { id: data.tenant.id, slug, key: data.api_key, client: `${slug}-app`, origin: baseURL };
    try {
      const registered = await request.post(`${baseURL}/manage/oauth-clients`, {
        headers: { Authorization: `Bearer ${tenant.key}` },
        data: { client_id: tenant.client, name: 'Synthetic app', redirect_uri: callback },
      });
      expect(registered.status()).toBe(201);
      await use(tenant);
    } finally {
      const removed = await request.delete(`${baseURL}/platform/tenants/${tenant.id}`, { headers: platformHeaders });
      expect(removed.status()).toBe(204);
    }
  },
});

function authorization(tenant: Tenant) {
  const verifier = randomBytes(32).toString('base64url');
  const state = randomBytes(32).toString('base64url');
  const parameters = {
    client_id: tenant.client, redirect_uri: callback, response_type: 'code', state,
    code_challenge: createHash('sha256').update(verifier).digest('base64url'),
    code_challenge_method: 'S256', provider: 'email',
  };
  return { verifier, state, parameters };
}

async function invite(request: APIRequestContext, tenant: Tenant, email: string, legacy = false) {
  const response = await request.post(`${tenant.origin}/manage/invites`, {
    headers: { Authorization: `Bearer ${tenant.key}` },
    data: { email, role: 'user', ...(legacy ? {} : { delivery_mode: 'server_only' }) },
  });
  expect(response.status()).toBe(201);
  const data = await response.json();
  if (!legacy) {
    expect(data.email_delivery).toBe('suppressed');
    expect(data.invite_url).toBeUndefined();
  }
  return data.invite_token as string;
}

async function watchHostedResponse(page: Page, path: string) {
  let received!: (value: { status: number; keys: string[]; hasCookie: boolean }) => void;
  const observed = new Promise<{ status: number; keys: string[]; hasCookie: boolean }>(resolve => { received = resolve; });
  await page.route(`**${path}`, async route => {
    const response = await route.fetch();
    const body = await response.json();
    // An opt-in mutation proves the browser-token assertions detect a leak.
    if (process.env.INJECT_HOSTED_SIGNUP_TOKEN_CONTROL === '1') body.access_token = 'synthetic-leak-control';
    await route.fulfill({ response, json: body });
    // Capture before Chromium discards the old document's response on navigation.
    // Retain only shape/status, never invitation, code or token values.
    received({ status: response.status(), keys: Object.keys(body), hasCookie: response.headers()['set-cookie'] !== undefined });
  });
  await page.route(`${callback}*`, route => route.fulfill({
    contentType: 'text/html', body: '<h1>Backend callback received</h1>',
  }));
  return { observed };
}

async function exchangeOnBackend(request: APIRequestContext, tenant: Tenant, page: Page, auth: ReturnType<typeof authorization>, expectedEmail: string, expectedVerified?: boolean) {
  await expect(page.getByRole('heading', { name: 'Backend callback received' })).toBeVisible();
  const redirected = new URL(page.url());
  expect(redirected.origin + redirected.pathname).toBe(callback);
  expect(redirected.searchParams.get('state')).toBe(auth.state);
  expect([...redirected.searchParams.keys()].sort()).toEqual(['code', 'state']);
  const response = await request.post(`${tenant.origin}/t/${tenant.slug}/auth/token`, {
    data: { grant_type: 'authorization_code', code: redirected.searchParams.get('code'), code_verifier: auth.verifier, client_id: tenant.client, redirect_uri: callback },
  });
  expect(response.status()).toBe(200);
  const token = await response.json();
  if (expectedVerified !== undefined) expect(token.user.email_verified).toBe(expectedVerified);
  const [header, payload, signature] = token.access_token.split('.');
  const decodedHeader = JSON.parse(Buffer.from(header, 'base64url').toString());
  const claims = JSON.parse(Buffer.from(payload, 'base64url').toString());
  const jwksResponse = await request.get(`${tenant.origin}/auth/jwks`);
  const jwks = await jwksResponse.json();
  const jwk = jwks.keys.find((key: { kid: string }) => key.kid === decodedHeader.kid);
  expect(decodedHeader.alg).toBe('EdDSA');
  expect(decodedHeader.typ).toBe('JWT');
  expect(jwk).toBeTruthy();
  expect(verify(null, Buffer.from(`${header}.${payload}`), createPublicKey({ key: jwk, format: 'jwk' }), Buffer.from(signature, 'base64url'))).toBe(true);
  expect(claims.iss).toBe(process.env.EXPECTED_ISSUER || 'gatewasm');
  expect(claims.aud).toBe(process.env.EXPECTED_AUDIENCE || 'gatewasm-app');
  expect(claims.tenant_id).toBe(tenant.id);
  expect(claims.sub).toBe(token.user.id);
  expect(claims.email).toBe(expectedEmail);
  expect(claims.roles).toEqual(['user']);
  expect(claims.exp).toBeGreaterThan(Date.now() / 1000);
  const logout = await request.post(`${tenant.origin}/t/${tenant.slug}/auth/logout`, {
    headers: { Authorization: `Bearer ${token.access_token}` }, data: { refresh_token: token.refresh_token },
  });
  expect(logout.status()).toBe(204);
}

test('hosted invitation creates an account through backend PKCE without browser tokens @smoke @regression', async ({ page, request, tenant }) => {
  const email = `invite-${randomBytes(6).toString('hex')}@example.test`;
  const secret = await invite(request, tenant, email);
  const auth = authorization(tenant);
  const pushed = await request.post(`${tenant.origin}/t/${tenant.slug}/oauth/par`, {
    form: { ...auth.parameters, invite_token: secret },
  });
  expect(pushed.status()).toBe(201);
  const { request_uri } = await pushed.json();
  const browserRequests: string[] = [];
  const errors: string[] = [];
  page.on('request', req => browserRequests.push(req.url() + (req.postData() || '')));
  page.on('pageerror', error => errors.push(error.message));
  const registerPath = `/t/${tenant.slug}/auth/register`;
  const { observed } = await watchHostedResponse(page, registerPath);
  // A previously saved legacy invitation must not override the server binding.
  await page.addInitScript(({ slug }) => sessionStorage.setItem(`tenant_invite_token_${slug}`, 'untrusted-legacy-invite'), { slug: tenant.slug });
  await page.goto(`${tenant.origin}/t/${tenant.slug}/auth/authorize?${new URLSearchParams({ client_id: tenant.client, request_uri })}`);
  await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();
  const injected = await page.evaluate(() => (window as any).__TENANT__);
  expect(injected.hosted_registration_allowed).toBe(true);
  expect(JSON.stringify(injected).includes(secret)).toBe(false);
  expect((await page.content()).includes(secret)).toBe(false);
  expect((await page.evaluate(() => JSON.stringify({ ...sessionStorage, ...localStorage }))).includes(secret)).toBe(false);
  await page.getByText('Sign up', { exact: true }).click();
  await page.getByLabel('Name', { exact: true }).fill('Invited person');
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  await expect(page.locator('select, input[name="role"], input[name="roles"]')).toHaveCount(0);
  const outgoing = page.waitForRequest(req => req.url().endsWith(registerPath));
  await page.getByRole('button', { name: 'Create account', exact: true }).click();
  const submitted = (await outgoing).postDataJSON();
  expect(Object.keys(submitted).sort()).toEqual(['authorization_request', 'email', 'name', 'password']);
  expect(submitted.authorization_request).toBe(injected.authorization_request);
  const registered = await observed;
  expect(registered.status).toBe(201);
  expect(registered.keys).toEqual(['redirect_uri']);
  expect(registered.hasCookie).toBe(false);
  await exchangeOnBackend(request, tenant, page, auth, email, false);
  expect(browserRequests.every(value => !value.includes(secret))).toBe(true);
  expect(browserRequests.some(value => value.includes('/auth/token'))).toBe(false);
  expect((await page.context().cookies(tenant.origin)).some(cookie => /refresh_token|gw_rt_/.test(cookie.name))).toBe(false);
  expect(errors).toEqual([]);
});

test('hosted login ignores query signup eligibility and keeps existing login working @smoke @regression', async ({ page, request, tenant }) => {
  const email = `existing-${randomBytes(6).toString('hex')}@example.test`;
  const created = await request.post(`${tenant.origin}/manage/users`, {
    headers: { Authorization: `Bearer ${tenant.key}` }, data: { email, password, name: 'Existing person', role: 'user' },
  });
  expect(created.status()).toBe(201);
  const providers = await request.patch(`${tenant.origin}/manage/config`, {
    headers: { Authorization: `Bearer ${tenant.key}` },
    data: { google_client_id: 'synthetic-google', google_client_secret: 'synthetic-only', github_client_id: 'synthetic-github', github_client_secret: 'synthetic-only' },
  });
  expect(providers.status()).toBe(200);
  await page.goto(`${tenant.origin}/t/${tenant.slug}/login`);
  await expect(page.getByRole('button', { name: /Continue with (Google|GitHub)/ })).toHaveCount(2);
  const auth = authorization(tenant);
  const loginPath = `/t/${tenant.slug}/auth/login`;
  const { observed } = await watchHostedResponse(page, loginPath);
  await page.goto(`${tenant.origin}/t/${tenant.slug}/auth/authorize?${new URLSearchParams(auth.parameters)}`);
  const spoofed = new URL(page.url());
  spoofed.searchParams.set('mode', 'register');
  spoofed.searchParams.set('hosted_registration_allowed', 'true');
  spoofed.searchParams.set('invite_token', 'untrusted-query-invite');
  await page.goto(spoofed.toString());
  expect((await page.evaluate(() => (window as any).__TENANT__)).hosted_registration_allowed).toBe(false);
  await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();
  await expect(page.getByText('Sign up', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Continue with (Google|GitHub)/ })).toHaveCount(0);
  await expect(page.getByLabel('Name', { exact: true })).toHaveCount(0);
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  const outgoing = page.waitForRequest(req => req.url().endsWith(loginPath));
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  expect(Object.keys((await outgoing).postDataJSON()).sort()).toEqual(['authorization_request', 'email', 'password']);
  const loggedIn = await observed;
  expect(loggedIn.status).toBe(200);
  expect(loggedIn.keys).toEqual(['redirect_uri']);
  expect(loggedIn.hasCookie).toBe(false);
  await exchangeOnBackend(request, tenant, page, auth, email);
});

test('legacy direct invitation still exposes its existing signup form @regression', async ({ page, request, tenant }) => {
  const secret = await invite(request, tenant, 'legacy@example.test', true);
  await page.goto(`${tenant.origin}/t/${tenant.slug}/login?${new URLSearchParams({ invite_token: secret, mode: 'register' })}`);
  await expect(page.getByRole('button', { name: 'Create account', exact: true })).toBeVisible();
  await expect(page.getByLabel('Name', { exact: true })).toBeVisible();
  await expect(page.getByText('Sign in', { exact: true })).toBeVisible();
});

test('empty hosted continuation cannot downgrade to legacy token login @smoke @regression', async ({ page, request, tenant }) => {
  const email = 'empty-request@example.test';
  const created = await request.post(`${tenant.origin}/manage/users`, {
    headers: { Authorization: `Bearer ${tenant.key}` }, data: { email, password, name: 'Existing person', role: 'user' },
  });
  expect(created.status()).toBe(201);
  await page.goto(`${tenant.origin}/t/${tenant.slug}/login?authorization_request=&mode=register&invite_token=untrusted`);
  await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeVisible();
  await expect(page.getByText('Sign up', { exact: true })).toHaveCount(0);
  await page.getByLabel('Email', { exact: true }).fill(email);
  await page.getByLabel('Password', { exact: true }).fill(password);
  const outgoing = page.waitForRequest(req => req.url().endsWith('/auth/login'));
  const incoming = page.waitForResponse(res => res.url().endsWith('/auth/login'));
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  expect((await outgoing).postDataJSON()).toEqual({ email, password, authorization_request: '' });
  const refused = await incoming;
  expect(refused.status()).toBe(400);
  const body = await refused.json();
  expect(body.access_token).toBeUndefined();
  expect(body.refresh_token).toBeUndefined();
  expect(refused.headers()['set-cookie']).toBeUndefined();
  expect((await page.context().cookies(tenant.origin)).some(cookie => /refresh_token|gw_rt_/.test(cookie.name))).toBe(false);
});
