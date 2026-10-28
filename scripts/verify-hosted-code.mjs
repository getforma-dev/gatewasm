// Synthetic issuer-side rehearsal. This is not GenUI's integration acceptance.
import { createRequire } from 'node:module';
import { createHash, createPublicKey, randomBytes, verify } from 'node:crypto';
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
const require = createRequire(new URL('../admin/package.json', import.meta.url));
const { chromium, firefox, webkit } = require('playwright');
const config = JSON.parse(await readFile(process.argv[2], 'utf8'));
const receipt = JSON.parse(await readFile(process.argv[3], 'utf8'));
const results = [];
const tenant = config.tenants.find(t => t.slug === 'northstar');
const user = tenant.users.find(u => u.name === 'Maya');
const expected = receipt.tenants.find(t => t.slug === 'northstar');
for (const [name, engine] of Object.entries({ chromium, firefox, webkit })) {
  const browser = await engine.launch();
  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    const verifier = randomBytes(32).toString('base64url');
    const state = randomBytes(32).toString('base64url');
    let exchanged = false;
    const browserTokenResponses = [];
    await page.route('**/auth/login', async route => {
      const response = await route.fetch();
      const body = await response.json();
      if (process.env.INJECT_BROWSER_TOKEN_CONTROL === '1') body.access_token = 'synthetic-detection-control';
      if (body.access_token || body.refresh_token) browserTokenResponses.push('token in login response');
      await route.fulfill({ response, json: body });
    });
    await page.route(`${config.redirect_uri}*`, async route => {
      const callback = new URL(route.request().url());
      assert.equal(callback.searchParams.get('state'), state);
      const code = callback.searchParams.get('code');
      assert.ok(code);
      // Node-side fetch deliberately represents a backend exchange.
      const response = await fetch(`${config.auth_origin}/t/northstar/auth/token`, { method: 'POST',
        headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ grant_type: 'authorization_code', code, code_verifier: verifier, client_id: tenant.client_id, redirect_uri: config.redirect_uri }) });
      assert.equal(response.status, 200);
      const token = await response.json();
      const [header, claims, signature] = token.access_token.split('.');
      const h = JSON.parse(Buffer.from(header, 'base64url'));
      const c = JSON.parse(Buffer.from(claims, 'base64url'));
      const jwks = await (await fetch(`${config.auth_origin}/auth/jwks`)).json();
      const jwk = jwks.keys.find(k => k.kid === h.kid);
      assert.equal(h.alg, 'EdDSA'); assert.ok(jwk);
      assert.ok(verify(null, Buffer.from(`${header}.${claims}`), createPublicKey({ key: jwk, format: 'jwk' }), Buffer.from(signature, 'base64url')));
      assert.equal(c.iss, process.env.EXPECTED_ISSUER || 'gatewasm');
      assert.equal(c.aud, process.env.EXPECTED_AUDIENCE || 'gatewasm-app');
      assert.equal(c.tenant_id, expected.tenant_id);
      assert.equal(c.sub, expected.users.find(u => u.name === 'Maya').subject);
      assert.ok(c.roles.includes('admin')); assert.ok(c.exp > Date.now() / 1000);
      const logout = await fetch(`${config.auth_origin}/t/northstar/auth/logout`, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token.access_token}` }, body: JSON.stringify({ refresh_token: token.refresh_token }) });
      assert.equal(logout.status, 204);
      exchanged = true;
      await route.fulfill({ status: 200, contentType: 'text/html', body: '<main><h1>Backend session established</h1></main>' });
    });
    const authorize = new URL(`${config.auth_origin}/t/northstar/auth/authorize`);
    authorize.search = new URLSearchParams({ client_id: tenant.client_id, redirect_uri: config.redirect_uri,
      response_type: 'code', state, code_challenge: createHash('sha256').update(verifier).digest('base64url'), code_challenge_method: 'S256', provider: 'email' }).toString();
    await page.goto(authorize.toString());
    await page.getByLabel('Email', { exact: true }).fill(user.email);
    await page.getByLabel('Password', { exact: true }).fill(process.env[user.password_env]);
    assert.ok((await page.locator('body').innerText()).includes('Northstar'));
    assert.ok((await page.locator('body').innerText()).includes('Secured by fAuth'));
    assert.equal(await page.getByRole('button', { name: 'Continue with Google' }).count(), 0);
    assert.equal(await page.getByRole('button', { name: 'Continue with GitHub' }).count(), 0);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.getByRole('heading', { name: 'Backend session established' }).waitFor();
    assert.ok(exchanged); assert.deepEqual(browserTokenResponses, []);
    results.push({ browser: name, branded_login: 'pass', password_code_flow: 'pass', backend_exchange: 'pass', offline_jwks_claims: 'pass', browser_tokens: 'none' });
  } finally { await browser.close(); }
}
await writeFile(process.argv[4], JSON.stringify({ scope: 'synthetic issuer rehearsal; not GenUI consumer acceptance', results }, null, 2) + '\n');
console.log(`PASS: hosted code rehearsal in ${results.map(r => r.browser).join(', ')}`);
