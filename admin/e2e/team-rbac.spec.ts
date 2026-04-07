import { test, expect, APIRequestContext } from '@playwright/test';
import { BASE } from './helpers';

const TEST_PASSWORD = 'E2eTestPassword123!';

// ── Helpers ──────────────────────────────────────────────────────────────────

async function postWithRetry(
  request: APIRequestContext,
  url: string,
  options: any,
  attempts = 4,
) {
  let response: any;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    response = await request.post(url, options);
    if (response.status() !== 429) {
      return response;
    }
    const retryAfter = Number(response.headers()['retry-after'] || '1');
    await new Promise((resolve) =>
      setTimeout(resolve, Math.max(500, retryAfter * 1000)),
    );
  }
  return response;
}

async function createTestAccount(
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

async function createTestTenant(
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

async function inviteMember(
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
  const inviteId: string = data.invite?.id;
  const inviteToken: string = data.invite_token;
  if (!inviteId || !inviteToken) {
    throw new Error(`inviteMember(${email}): unexpected response ${JSON.stringify(data)}`);
  }

  return { inviteId, inviteToken };
}

async function acceptInvite(
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

async function listTeam(request: APIRequestContext, token: string): Promise<any> {
  const res = await request.get(BASE + '/platform/admin/team', {
    headers: { Authorization: 'Bearer ' + token },
  });

  if (!res.ok()) {
    throw new Error(`listTeam failed: ${await res.text()}`);
  }

  return res.json();
}

async function listTenants(
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

// ── Block 1: Invite Lifecycle ─────────────────────────────────────────────────

test.describe.serial('Invite Lifecycle', () => {
  let ownerAToken: string;
  let tenantA1: { id: string; slug: string };
  let memberCToken: string;
  let memberCEmail: string;

  // Invite state shared across serial tests
  let createdInviteId: string;
  let createdInviteToken: string;

  test.beforeAll(async ({ request }) => {
    const ownerA = await createTestAccount(request, 'owner-a-inv');
    ownerAToken = ownerA.token;

    tenantA1 = await createTestTenant(request, ownerAToken, 'Invite Tenant A1');

    const memberC = await createTestAccount(request, 'member-c-inv');
    memberCToken = memberC.token;
    memberCEmail = memberC.email;
  });

  test.afterAll(async ({ request }) => {
    if (tenantA1?.id) {
      await request.delete(
        BASE + '/platform/self-serve/tenants/' + tenantA1.id,
        {
          headers: {
            Authorization: 'Bearer ' + ownerAToken,
            'Content-Type': 'application/json',
          },
          data: { confirm_slug: tenantA1.slug },
        },
      );
    }
  });

  test('owner can create invite with email and tenant assignments', async ({ request }) => {
    const res = await postWithRetry(request, BASE + '/platform/admin/team/invites', {
      headers: {
        Authorization: 'Bearer ' + ownerAToken,
        'Content-Type': 'application/json',
      },
      data: { email: memberCEmail, role: 'viewer', tenant_ids: [tenantA1.id] },
    });

    expect(res.status()).toBeLessThan(300);
    const data = await res.json();

    expect(data.invite).toBeDefined();
    expect(data.invite.id).toBeTruthy();
    expect(data.invite.status).toBe('pending');
    expect(data.invite_token).toBeTruthy();
    expect(data.invite_url).toBeTruthy();

    // Persist for subsequent serial tests
    createdInviteId = data.invite.id;
    createdInviteToken = data.invite_token;
  });

  test('pending invite appears in team list', async ({ request }) => {
    // The team invites list is at /platform/admin/team/invites
    const res = await request.get(BASE + '/platform/admin/team/invites', {
      headers: { Authorization: 'Bearer ' + ownerAToken },
    });

    expect(res.ok()).toBeTruthy();
    const data = await res.json();
    const invites: any[] = data.invites || [];

    const found = invites.find(
      (inv: any) =>
        inv.email?.toLowerCase() === memberCEmail.toLowerCase() &&
        inv.status === 'pending',
    );
    expect(found).toBeDefined();
  });

  test('invited member can accept invite', async ({ request }) => {
    const data = await acceptInvite(request, memberCToken, createdInviteToken);

    expect(data.accepted).toBe(true);
  });

  test('accepted member appears in team member list', async ({ request }) => {
    const data = await listTeam(request, ownerAToken);
    const members: any[] = data.members || [];

    const found = members.find(
      (m: any) =>
        m.email?.toLowerCase() === memberCEmail.toLowerCase() &&
        m.status === 'active',
    );
    expect(found).toBeDefined();
  });

  test('owner can revoke a pending invite', async ({ request }) => {
    // Create a fresh invite for a different address
    const random = Math.random().toString(36).slice(2, 8);
    const revokeTargetEmail = `e2e-revoke-${Date.now()}-${random}@test.local`;

    const createRes = await postWithRetry(
      request,
      BASE + '/platform/admin/team/invites',
      {
        headers: {
          Authorization: 'Bearer ' + ownerAToken,
          'Content-Type': 'application/json',
        },
        data: {
          email: revokeTargetEmail,
          role: 'viewer',
          tenant_ids: [tenantA1.id],
        },
      },
    );
    expect(createRes.status()).toBeLessThan(300);
    const created = await createRes.json();
    const revokeInviteId: string = created.invite.id;

    // Revoke it
    const revokeRes = await postWithRetry(
      request,
      BASE + '/platform/admin/team/invites/' + revokeInviteId + '/revoke',
      {
        headers: {
          Authorization: 'Bearer ' + ownerAToken,
          'Content-Type': 'application/json',
        },
        data: {},
      },
    );
    expect(revokeRes.ok()).toBeTruthy();
    const revokeData = await revokeRes.json();

    expect(revokeData.status).toBe('revoked');
  });
});

// ── Block 2: Role Enforcement ─────────────────────────────────────────────────

test.describe.serial('Role Enforcement', () => {
  let ownerAToken: string;
  let ownerAEmail: string;
  let tenantA1: { id: string; slug: string };
  let tenantA2: { id: string; slug: string };
  let memberCToken: string;
  let memberCEmail: string;
  let memberDToken: string;
  let memberDEmail: string;

  test.beforeAll(async ({ request }) => {
    const ownerA = await createTestAccount(request, 'owner-a-role');
    ownerAToken = ownerA.token;
    ownerAEmail = ownerA.email;

    tenantA1 = await createTestTenant(request, ownerAToken, 'Role Tenant A1');
    tenantA2 = await createTestTenant(request, ownerAToken, 'Role Tenant A2');

    // Member C: viewer on A1 only
    const memberC = await createTestAccount(request, 'member-c-role');
    memberCToken = memberC.token;
    memberCEmail = memberC.email;
    const inviteC = await inviteMember(request, ownerAToken, memberCEmail, 'viewer', [tenantA1.id]);
    await acceptInvite(request, memberCToken, inviteC.inviteToken);

    // Member D: admin on A1 + A2
    const memberD = await createTestAccount(request, 'member-d-role');
    memberDToken = memberD.token;
    memberDEmail = memberD.email;
    const inviteD = await inviteMember(request, ownerAToken, memberDEmail, 'admin', [
      tenantA1.id,
      tenantA2.id,
    ]);
    await acceptInvite(request, memberDToken, inviteD.inviteToken);
  });

  test.afterAll(async ({ request }) => {
    for (const tenant of [tenantA1, tenantA2]) {
      if (tenant?.id) {
        await request.delete(
          BASE + '/platform/self-serve/tenants/' + tenant.id,
          {
            headers: {
              Authorization: 'Bearer ' + ownerAToken,
              'Content-Type': 'application/json',
            },
            data: { confirm_slug: tenant.slug },
          },
        );
      }
    }
  });

  test('viewer can read tenant data', async ({ request }) => {
    const tenants = await listTenants(request, memberCToken);
    const a1 = tenants.find((t: any) => t.id === tenantA1.id);

    expect(a1).toBeDefined();
    expect(a1.access).toBe('team');
    expect(a1.team_role).toBe('viewer');
  });

  test('viewer cannot delete a tenant', async ({ request }) => {
    const res = await request.delete(
      BASE + '/platform/self-serve/tenants/' + tenantA1.id,
      {
        headers: {
          Authorization: 'Bearer ' + memberCToken,
          'Content-Type': 'application/json',
        },
        data: { confirm_slug: tenantA1.slug },
      },
    );

    expect(res.status()).toBe(403);
  });

  test('viewer cannot rotate tenant API key', async ({ request }) => {
    const res = await postWithRetry(
      request,
      BASE + '/platform/self-serve/tenants/' + tenantA1.id + '/rotate-key',
      {
        headers: {
          Authorization: 'Bearer ' + memberCToken,
          'Content-Type': 'application/json',
        },
        data: {},
      },
    );

    expect(res.status()).toBe(403);
  });

  test('admin can access mutation endpoints', async ({ request }) => {
    // PATCH A1 name as admin member D
    const res = await request.patch(
      BASE + '/platform/self-serve/tenants/' + tenantA1.id,
      {
        headers: {
          Authorization: 'Bearer ' + memberDToken,
          'Content-Type': 'application/json',
        },
        data: { name: 'Role Tenant A1 (patched by admin)' },
      },
    );

    expect(res.status()).toBe(200);
  });

  test('member only sees assigned tenants', async ({ request }) => {
    const cTenants = await listTenants(request, memberCToken);
    const dTenants = await listTenants(request, memberDToken);

    const cIds = cTenants.map((t: any) => t.id);
    const dIds = dTenants.map((t: any) => t.id);

    // C sees A1 but not A2
    expect(cIds).toContain(tenantA1.id);
    expect(cIds).not.toContain(tenantA2.id);

    // D sees both
    expect(dIds).toContain(tenantA1.id);
    expect(dIds).toContain(tenantA2.id);
  });

  test('account owner cannot remove themselves', async ({ request }) => {
    // Find owner A's member ID from their team list
    const teamData = await listTeam(request, ownerAToken);
    const members: any[] = teamData.members || [];
    const ownerEntry = members.find(
      (m: any) => m.email?.toLowerCase() === ownerAEmail.toLowerCase(),
    );
    expect(ownerEntry).toBeDefined();

    const res = await request.delete(
      BASE + '/platform/admin/team/' + ownerEntry.id,
      {
        headers: {
          Authorization: 'Bearer ' + ownerAToken,
          'Content-Type': 'application/json',
        },
      },
    );

    expect(res.ok()).toBeFalsy();
    const data = await res.json();
    // The error code is cannot_remove_self
    const errCode = data.error || data.code || data.message || '';
    expect(errCode).toMatch(/cannot_remove_self/i);
  });

  test('member cannot remove other members', async ({ request }) => {
    // Get D's member ID from owner's team list
    const teamData = await listTeam(request, ownerAToken);
    const members: any[] = teamData.members || [];
    const dEntry = members.find(
      (m: any) => m.email?.toLowerCase() === memberDEmail.toLowerCase(),
    );
    expect(dEntry).toBeDefined();

    // C (non-owner) tries to delete D — the team route requires account owner
    const res = await request.delete(
      BASE + '/platform/admin/team/' + dEntry.id,
      {
        headers: {
          Authorization: 'Bearer ' + memberCToken,
          'Content-Type': 'application/json',
        },
      },
    );

    expect(res.status()).toBeGreaterThanOrEqual(400);
  });

  test('owner can update member role', async ({ request }) => {
    // Find C's member ID
    const teamData = await listTeam(request, ownerAToken);
    const members: any[] = teamData.members || [];
    const cEntry = members.find(
      (m: any) => m.email?.toLowerCase() === memberCEmail.toLowerCase(),
    );
    expect(cEntry).toBeDefined();

    // Update C to editor on A1
    const patchRes = await request.patch(
      BASE + '/platform/admin/team/' + cEntry.id,
      {
        headers: {
          Authorization: 'Bearer ' + ownerAToken,
          'Content-Type': 'application/json',
        },
        data: {
          tenant_assignments: [{ tenant_id: tenantA1.id, role: 'editor' }],
        },
      },
    );
    expect(patchRes.ok()).toBeTruthy();

    // Verify C now sees team_role: 'editor' on A1
    const tenants = await listTenants(request, memberCToken);
    const a1 = tenants.find((t: any) => t.id === tenantA1.id);
    expect(a1).toBeDefined();
    expect(a1.team_role).toBe('editor');
  });

  test('owner can remove a team member', async ({ request }) => {
    // Find C's member ID again (after possible role update)
    const teamData = await listTeam(request, ownerAToken);
    const members: any[] = teamData.members || [];
    const cEntry = members.find(
      (m: any) => m.email?.toLowerCase() === memberCEmail.toLowerCase(),
    );
    expect(cEntry).toBeDefined();

    // Owner removes C
    const delRes = await request.delete(
      BASE + '/platform/admin/team/' + cEntry.id,
      {
        headers: {
          Authorization: 'Bearer ' + ownerAToken,
          'Content-Type': 'application/json',
        },
      },
    );
    expect(delRes.ok()).toBeTruthy();

    // C no longer sees A1
    const tenants = await listTenants(request, memberCToken);
    const a1 = tenants.find((t: any) => t.id === tenantA1.id);
    expect(a1).toBeUndefined();
  });
});

// ── Block 3: Cross-Account Isolation ─────────────────────────────────────────

test.describe('Cross-Account Isolation', () => {
  let ownerAToken: string;
  let ownerAEmail: string;
  let ownerBToken: string;
  let ownerBEmail: string;
  let tenantA1: { id: string; slug: string };
  let tenantB1: { id: string; slug: string };
  let memberDToken: string;
  let memberDEmail: string;

  test.beforeAll(async ({ request }) => {
    const ownerA = await createTestAccount(request, 'owner-a-iso');
    ownerAToken = ownerA.token;
    ownerAEmail = ownerA.email;

    const ownerB = await createTestAccount(request, 'owner-b-iso');
    ownerBToken = ownerB.token;
    ownerBEmail = ownerB.email;

    tenantA1 = await createTestTenant(request, ownerAToken, 'Iso Tenant A1');
    tenantB1 = await createTestTenant(request, ownerBToken, 'Iso Tenant B1');

    // Member D belongs to owner A's team
    const memberD = await createTestAccount(request, 'member-d-iso');
    memberDToken = memberD.token;
    memberDEmail = memberD.email;
    const inviteD = await inviteMember(request, ownerAToken, memberDEmail, 'viewer', [
      tenantA1.id,
    ]);
    await acceptInvite(request, memberDToken, inviteD.inviteToken);
  });

  test.afterAll(async ({ request }) => {
    if (tenantA1?.id) {
      await request.delete(
        BASE + '/platform/self-serve/tenants/' + tenantA1.id,
        {
          headers: {
            Authorization: 'Bearer ' + ownerAToken,
            'Content-Type': 'application/json',
          },
          data: { confirm_slug: tenantA1.slug },
        },
      );
    }
    if (tenantB1?.id) {
      await request.delete(
        BASE + '/platform/self-serve/tenants/' + tenantB1.id,
        {
          headers: {
            Authorization: 'Bearer ' + ownerBToken,
            'Content-Type': 'application/json',
          },
          data: { confirm_slug: tenantB1.slug },
        },
      );
    }
  });

  test('owner A cannot see owner B team members', async ({ request }) => {
    const teamData = await listTeam(request, ownerAToken);
    const members: any[] = teamData.members || [];
    const emails = members.map((m: any) => m.email?.toLowerCase());

    expect(emails).not.toContain(ownerBEmail.toLowerCase());
  });

  test('owner A cannot see owner B tenants', async ({ request }) => {
    const tenants = await listTenants(request, ownerAToken);
    const ids = tenants.map((t: any) => t.id);

    expect(ids).not.toContain(tenantB1.id);
  });

  test('member of account A cannot access account B tenant', async ({ request }) => {
    // D is on A's team; tries to GET B's tenant detail
    const res = await request.get(
      BASE + '/platform/self-serve/tenants/' + tenantB1.id,
      {
        headers: { Authorization: 'Bearer ' + memberDToken },
      },
    );

    expect(res.status()).toBeGreaterThanOrEqual(400);
  });

  test('invite from owner A cannot grant access to owner B tenant', async ({ request }) => {
    // Owner A tries to invite someone with B's tenant_id — should fail validation
    const random = Math.random().toString(36).slice(2, 8);
    const targetEmail = `e2e-cross-${Date.now()}-${random}@test.local`;

    const res = await postWithRetry(request, BASE + '/platform/admin/team/invites', {
      headers: {
        Authorization: 'Bearer ' + ownerAToken,
        'Content-Type': 'application/json',
      },
      data: { email: targetEmail, role: 'viewer', tenant_ids: [tenantB1.id] },
    });

    // Must be rejected — B1 is not owned by A
    expect(res.status()).toBeGreaterThanOrEqual(400);
  });

  test("accepting A invite does not grant access to B data", async ({ request }) => {
    // D is already on A's team. Verify D's tenants don't include B1.
    const tenants = await listTenants(request, memberDToken);
    const ids = tenants.map((t: any) => t.id);
    expect(ids).not.toContain(tenantB1.id);

    // D's team list (as viewed through /platform/admin/team) should not include B.
    // Note: D is not an account owner, so this endpoint will reject — that itself
    // confirms isolation. If it somehow succeeded, check for B's email.
    const teamRes = await request.get(BASE + '/platform/admin/team', {
      headers: { Authorization: 'Bearer ' + memberDToken },
    });

    if (teamRes.ok()) {
      const teamData = await teamRes.json();
      const emails = (teamData.members || []).map((m: any) => m.email?.toLowerCase());
      expect(emails).not.toContain(ownerBEmail.toLowerCase());
    } else {
      // If the endpoint rejects non-owners (expected), the test still passes —
      // the important assertion is that D can't see B1 in tenants (asserted above).
      expect(teamRes.status()).toBeGreaterThanOrEqual(400);
    }
  });
});
