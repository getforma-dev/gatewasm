// admin/e2e/factories.ts
import { expect, APIRequestContext, BrowserContext } from '@playwright/test';
import { BASE, postWithRetry, getAccessToken } from './helpers';

const TEST_PASSWORD = 'E2eTestPassword123!';

export async function createTestAccount(
  request: APIRequestContext,
  label: string,
): Promise<{ token: string; userId: string; email: string }> {
  const random = Math.random().toString(36).slice(2, 8);
  const email = `e2e-${label}-${Date.now()}-${random}@test.local`;

  const res = await postWithRetry(request, BASE + '/auth/register', {
    headers: {
      'X-Tenant-Slug': 'platform',
      'Content-Type': 'application/json',
    },
    data: { email, password: TEST_PASSWORD, name: `E2E ${label}` },
  });

  if (!res.ok()) {
    throw new Error(`createTestAccount(${label}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  const token: string = data.access_token;
  const userId: string = data.user_id || data.id;
  if (!token) {
    throw new Error(`createTestAccount(${label}): no access_token in ${JSON.stringify(data)}`);
  }

  return { token, userId, email };
}

export async function createTestTenant(
  request: APIRequestContext,
  token: string,
  name: string,
): Promise<{ id: string; slug: string }> {
  const slug = `e2e-${name.toLowerCase().replace(/\s+/g, '-')}-${Date.now().toString(36)}`;

  const res = await postWithRetry(request, BASE + '/platform/self-serve/tenants', {
    headers: {
      Authorization: 'Bearer ' + token,
      'Content-Type': 'application/json',
    },
    data: { name, slug },
  });

  if (!res.ok()) {
    throw new Error(`createTestTenant(${name}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  const id: string = data.id || data.tenant?.id;
  if (!id) {
    throw new Error(`createTestTenant(${name}): no id in ${JSON.stringify(data)}`);
  }

  return { id, slug };
}

export async function deleteTestTenant(
  request: APIRequestContext,
  token: string,
  tenantId: string,
  slug: string,
): Promise<void> {
  await request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
    headers: {
      Authorization: 'Bearer ' + token,
      'Content-Type': 'application/json',
    },
    data: { confirm_slug: slug },
  });
}

export async function deleteTestTenantFromContext(
  context: BrowserContext,
  tenantId: string,
  slug: string,
): Promise<void> {
  try {
    const token = await getAccessToken(context);
    await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
      headers: { Authorization: 'Bearer ' + token },
      data: { confirm_slug: slug },
    });
  } catch (e) {
    console.warn('Tenant cleanup failed:', e);
  }
}

export async function inviteMember(
  request: APIRequestContext,
  ownerToken: string,
  email: string,
  role: string,
  tenantIds: string[],
): Promise<{ inviteId: string; inviteToken: string }> {
  const res = await postWithRetry(request, BASE + '/platform/admin/team/invites', {
    headers: {
      Authorization: 'Bearer ' + ownerToken,
      'Content-Type': 'application/json',
    },
    data: { email, role, tenant_ids: tenantIds },
  });

  if (!res.ok()) {
    throw new Error(`inviteMember(${email}) failed: ${await res.text()}`);
  }

  const data = await res.json();
  return { inviteId: data.invite?.id, inviteToken: data.invite_token };
}

export async function acceptInvite(
  request: APIRequestContext,
  memberToken: string,
  inviteToken: string,
): Promise<any> {
  const res = await postWithRetry(request, BASE + '/platform/admin/invites/accept', {
    headers: {
      Authorization: 'Bearer ' + memberToken,
      'Content-Type': 'application/json',
    },
    data: { token: inviteToken },
  });

  if (!res.ok()) {
    throw new Error(`acceptInvite failed: ${await res.text()}`);
  }

  return res.json();
}

export async function listTeam(request: APIRequestContext, token: string): Promise<any> {
  const res = await request.get(BASE + '/platform/admin/team', {
    headers: { Authorization: 'Bearer ' + token },
  });

  if (!res.ok()) {
    throw new Error(`listTeam failed: ${await res.text()}`);
  }

  return res.json();
}

export async function listTenants(
  request: APIRequestContext,
  token: string,
): Promise<any[]> {
  const res = await request.get(BASE + '/platform/self-serve/tenants', {
    headers: { Authorization: 'Bearer ' + token },
  });

  if (!res.ok()) {
    throw new Error(`listTenants failed: ${await res.text()}`);
  }

  const data = await res.json();
  return data.tenants || [];
}

/** Re-login to get a fresh access token (avoids stale token in afterAll). */
export async function refreshAccountToken(
  request: APIRequestContext,
  email: string,
): Promise<string> {
  const res = await postWithRetry(request, BASE + '/auth/login', {
    headers: { 'X-Tenant-Slug': 'platform', 'Content-Type': 'application/json' },
    data: { email, password: TEST_PASSWORD },
  });
  if (!res.ok()) throw new Error(`refreshAccountToken(${email}) failed: ${await res.text()}`);
  const data = await res.json();
  return data.access_token as string;
}
