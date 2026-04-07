# E2E Test Audit & New Test Coverage

**Date:** 2026-04-07
**Branch:** `feat/platform-team-rbac`
**Status:** Approved — ready for implementation planning

## Overview

Audit and strengthen the e2e test suite across three parallel tracks: fix existing broken tests, add WebSocket infrastructure tests, and add comprehensive team RBAC tests including cross-account isolation. All tests are Playwright-based, API-driven for speed, and run against the test server on port 3102.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| WS event triggers | Management API + direct auth API | Full pipeline validation without multi-browser orchestration |
| Test file organization | One file per feature area | Matches existing pattern (`console.spec.ts`, `login.spec.ts`). Two new files. |
| RBAC test setup | API-driven, not UI-driven | Fast (~2-3s/test), deterministic, no selector fragility |
| BASE_URL fix | Centralize in `helpers.ts` | Prevents future drift across 5+ files |
| `fixes-verify.spec.ts` | Keep all tests, fix URL only | `/dev/clear-cache` is a real dev workflow route |
| 30s ticket expiry test | Include with extended timeout | Ticket expiry is a security guarantee on an auth platform |

---

## Track 1: Fix Existing Broken Tests

### BASE_URL Centralization

Extract to `e2e/helpers.ts`:
```typescript
export const BASE = process.env.BASE_URL || 'http://127.0.0.1:3102';
```

Update 5 files to import from helpers instead of defining locally:
- `command-palette.spec.ts` (line 4)
- `wizard-flow.spec.ts` (line 3)
- `persona-workflows.spec.ts` (line 14)
- `session-inspector.spec.ts` (line 4)
- `fixes-verify.spec.ts` (line 3)

### Stale Content Cleanup

- Review and update Phase 6 comments in `smoke.spec.ts` if client-side redirects are now implemented
- Verify selectors in `persona-workflows.spec.ts` still match current component class names after refactoring (`.plane-nav`, `.plane-btn`, `.account-tabs`, etc.)

No test deletions. No behavior changes.

---

## Track 2: WebSocket E2E Tests

**File:** `admin/e2e/websocket.spec.ts`

### Test Helper: `connectWebSocket`

Runs in browser context via `page.evaluate()` so it exercises the real `WebSocket` API:

```typescript
async function connectWebSocket(page: Page, auth: { token: string }): Promise<{
  messages: any[];
  send: (msg: object) => void;
  close: () => void;
  waitForMessage: (predicate: (msg: any) => boolean, timeoutMs?: number) => Promise<any>;
}>
```

Flow: POST `/ws/ticket` with JWT → open `ws://host/ws?ticket=` in browser → collect messages into array → return control object with `waitForMessage` that resolves on predicate match or rejects on timeout.

### Block 1: Ticket Auth & Connection Lifecycle

| Test | Validates |
|------|-----------|
| `POST /ws/ticket returns ticket when authenticated` | Ticket endpoint works behind auth middleware. Response: `{ ticket: "64-char hex" }` |
| `POST /ws/ticket rejects unauthenticated requests` | Returns 401 without JWT |
| `WS upgrade succeeds with valid ticket` | Open WebSocket with ticket, verify onopen fires |
| `WS upgrade rejects invalid ticket` | Connect with `?ticket=bogus`, verify immediate close |
| `WS upgrade rejects expired ticket (30s)` | Get ticket, wait 31s, try connect. Extended timeout on test. |
| `WS upgrade rejects reused ticket` | Get ticket, connect (burns it), connect again. Verify rejection. |
| `Server sends pong in response to ping` | Send `{ type: "ping" }`, verify `{ type: "pong" }` received |

### Block 2: Topic Subscription & Event Delivery

Uses management API (`MASTER_MGMT_KEY`) and direct auth endpoints to trigger server-side events.

| Test | Validates |
|------|-----------|
| `Receives team:changed when invite is created` | Owner opens WS, subscribes to `team:changed`. Create invite via API. Verify event with `hint: "invite_created"`. |
| `Receives tenant:activity on end user signup` | Owner opens WS on tenant, subscribes to `tenant:activity`. Register end user on that tenant. Verify event with `hint: "signup"`. |
| `Receives metrics:updated alongside tenant:activity` | Same as above. Verify both `tenant:activity` and `metrics:updated` arrive (dual-emit from `emit_activity()`). |
| `Does not receive events for unsubscribed topics` | Subscribe only to `team:changed`. Trigger login. Verify no message within 2s. |
| `Does not receive events for other tenants` | Open WS for Tenant A. Trigger activity on Tenant B. Verify no message (tenant isolation). |
| `Subscribe/unsubscribe dynamically updates filtering` | Subscribe, receive events. Unsubscribe. Trigger. Verify silence. |

### Block 3: Reconnection & Auth Expiry

| Test | Validates |
|------|-----------|
| `Reconnects after server-initiated close` | Open WS. Force server close. Verify client opens new connection. |
| `Receives auth_expired when token expires` | Use short token expiry. Verify `auth_expired` message. Verify client reconnects with fresh ticket. |
| `Health endpoint reports ws_connections count` | Open 2 connections → `/health` shows 2. Close 1 → shows 1. |

---

## Track 3: Team RBAC E2E Tests

**File:** `admin/e2e/team-rbac.spec.ts`

### Test Setup (beforeAll)

All API-driven, no UI:

- **Account Owner A** — register, create Tenant A1 and Tenant A2
- **Account Owner B** — register, create Tenant B1
- **Member C** — register on platform, invited to Owner A's team as `viewer` on Tenant A1 only
- **Member D** — register on platform, invited to Owner A's team as `admin` on Tenant A1 and A2

### Test Helpers

```typescript
async function createTestAccount(request): Promise<{ token, userId, email }>
async function createTestTenant(request, token, name): Promise<{ id, slug }>
async function inviteMember(request, ownerToken, email, role, tenantIds): Promise<{ inviteId, inviteToken }>
```

### Block 1: Invite Lifecycle

| Test | Validates |
|------|-----------|
| `Owner can create invite with email and tenant assignments` | API returns `invite_id`, `invite_url`, `status: "pending"` |
| `Pending invite appears in team list` | `GET /platform/admin/team` includes pending invite with correct email/role/tenants |
| `Invited member can accept invite` | `POST /platform/admin/invites/accept` with token returns `accepted: true` |
| `Accepted member appears in team member list` | Team list shows Member C as active with `viewer` role on A1 |
| `Owner can revoke a pending invite` | Create + revoke. Verify `status: "revoked"`. Token no longer accepted. |
| `Duplicate invite to existing member is rejected` | Re-invite Member C (already a member). Verify error. |

### Block 2: Role Enforcement

| Test | Validates |
|------|-----------|
| `Viewer can read tenant data` | Member C fetches tenants. A1 appears with `access: "team"`, `team_role: "viewer"`. |
| `Viewer cannot delete a tenant` | Member C: `DELETE /platform/self-serve/tenants/{A1}` → 403 |
| `Viewer cannot rotate tenant API key` | Member C: `POST .../rotate-key` → 403 |
| `Admin can access mutation endpoints` | Member D: `PATCH /platform/self-serve/tenants/{A1}` → 200 |
| `Member only sees assigned tenants` | C sees A1 only. D sees A1 and A2. Neither sees A2 (for C) or unassigned tenants. |
| `Account owner cannot be removed` | Owner A removes self → error `cannot_remove_self` |
| `Member cannot remove other members` | Member C removes Member D → 403 |
| `Owner can update member role` | Owner A changes C from viewer to editor. Subsequent requests reflect new role. |
| `Owner can remove a team member` | Owner A removes C. C no longer sees A1. |

### Block 3: Cross-Account Isolation

| Test | Validates |
|------|-----------|
| `Owner A cannot see Owner B's team members` | A's team list contains only A's members |
| `Owner A cannot see Owner B's tenants` | A's tenant list does not include B1 |
| `Member of Account A cannot access Account B's tenant` | Member D accesses B1 → 404 or 403 |
| `Invite from Owner A cannot grant access to Owner B's tenant` | Owner A creates invite with B1's tenant_id → server rejects |
| `Accepting A's invite does not grant access to B's data` | After C accepts A's invite, C has zero visibility into B's team/tenants |

---

## Test Infrastructure Notes

- **Server:** Playwright global setup starts `cargo run -p gatewasm-server` on port 3102 with test database
- **Database:** `postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test`
- **Auth:** `auth-helper.ts` handles user registration and login with retry for rate limits
- **Management API:** `MASTER_MGMT_KEY=mgmt_test_key` for triggering server-side events in WS tests
- **Cleanup:** `afterAll` blocks delete created tenants/users to avoid test pollution

## Dependencies

No new dependencies. Uses existing Playwright setup, `auth-helper.ts`, and `helpers.ts`.
