# Dashboard Enhancements Implementation Plan (Tier 1 + Tier 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire 7 existing backend API surfaces into the dashboard — security config, tenant invites, organizations, enhanced user detail, compliance status, passkey management, and webhook improvements.

**Architecture:** Most features need a self-serve proxy endpoint in `onboarding.rs` (since the dashboard uses JWT auth, not management API keys) plus a frontend view/component. Organizations and invites are already registered on `/admin/*` routes but need self-serve proxies for the platform console's JWT-based auth. Each task is self-contained and shippable independently.

**Tech Stack:** Rust/axum/sqlx (backend proxies), TypeScript/FormaJS (frontend views)

**Sequencing:** Security Config first (highest value), then Invites (enhances Members tab), Organizations (new plane 2 feature), User Detail (Members enhancement), Compliance (new plane 1 feature), Passkey Management (Account Settings), Webhook improvements (last — mostly frontend polish).

---

### Task 1: Security Config — self-serve proxy + SecurityConfigView

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add `get_tenant_security_config` + `update_tenant_security_config`)
- Modify: `crates/server/src/handlers/mod.rs` (register routes)
- Modify: `admin/src/platform/console/store.ts` (add types, signals, load/save functions)
- Create: `admin/src/platform/console/views/SecurityConfigView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts` (add to workspace switch)
- Modify: `admin/src/platform/console/store.ts` (add 'security' to ConsoleView, add to PLANES)

This is the most impactful feature — lets operators configure password policies, MFA enforcement, session limits, and security features from the dashboard.

- [ ] **Step 1: Add self-serve proxy endpoints for security config**

Two endpoints in `onboarding.rs`:

`GET /platform/self-serve/tenants/{id}/security-config` — reads current security config:
```rust
pub async fn get_tenant_security_config(...) {
    // ownership verification (standard pattern)
    // SELECT COALESCE(security_config, '{}'), COALESCE(plan, 'trial') FROM tenants LEFT JOIN tenant_entitlements...
    // Return { security_config: TenantSecurityConfig, plan: String }
}
```

`PATCH /platform/self-serve/tenants/{id}/security-config` — updates security config:
```rust
pub async fn update_tenant_security_config(...) {
    // ownership verification
    // Load current config, merge patch, validate against plan
    // UPDATE tenants SET security_config = $1 WHERE id = $2
    // Return updated config
}
```

Follow the exact merge + validation pattern from `management.rs:1585-1690`.

Route registration:
```rust
.route(
    "/platform/self-serve/tenants/{id}/security-config",
    get(onboarding::get_tenant_security_config).patch(onboarding::update_tenant_security_config),
)
```

- [ ] **Step 2: Add store types and functions**

Add to `ConsoleView` type: `'security'`
Add to PLANES operator tabs: `{ id: 'security', label: 'Security', shortcut: '\u2325\u0033' }` (renumber integrate to ⌥3, security to ⌥2... actually put security after integrate)

Types:
```typescript
export interface SecurityConfig {
  idle_timeout_minutes: number;
  max_sessions_per_user: number;
  host_prefix_cookies: boolean;
  csp_enabled: boolean;
  client_fingerprint: boolean;
  webauthn_enabled: boolean;
  fedcm_enabled: boolean;
  dpop_required: boolean;
  par_required: boolean;
  mtls_enabled: boolean;
  introspection_enabled: boolean;
  caep_enabled: boolean;
  impossible_travel_enabled: boolean;
}
```

Signals: `securityConfig`, `securityConfigLoading`, `tenantPlan`
Functions: `loadSecurityConfig(tenantId)`, `saveSecurityConfig(tenantId, patch)`

- [ ] **Step 3: Create SecurityConfigView**

Form-based view organized by tier:

**Foundation (always available):**
- Idle Timeout: number input (minutes)
- Max Sessions Per User: number input (1-100)
- Host Prefix Cookies: toggle
- CSP Enabled: toggle
- Client Fingerprint: toggle

**Pro (show locked badge if not on pro plan):**
- WebAuthn Enabled: toggle
- FedCM Enabled: toggle

**Enterprise:**
- DPoP Required: toggle
- PAR Required: toggle
- mTLS Enabled: toggle
- Introspection Enabled: toggle

**Compliance:**
- CAEP Enabled: toggle
- Impossible Travel: toggle + speed threshold input

Each section shows the plan requirement. Save button at bottom calls `saveSecurityConfig`.

- [ ] **Step 4: Wire into ConsoleApp and hydrateTenantView**

Add `SecurityConfigView` import, add to createSwitch, add load to hydrateTenantView.

- [ ] **Step 5: Build and commit**

---

### Task 2: Tenant Invites — self-serve proxy + InvitesPanel in Members

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add invite proxy endpoints)
- Modify: `crates/server/src/handlers/mod.rs`
- Modify: `admin/src/platform/console/store.ts` (add invite types, signals, functions)
- Modify: `admin/src/platform/console/views/UsersView.ts` (add invites section)

Three self-serve proxy endpoints:
- `GET /platform/self-serve/tenants/{id}/invites` — list pending invites
- `POST /platform/self-serve/tenants/{id}/invites` — send invite (email + role)
- `POST /platform/self-serve/tenants/{id}/invites/{invite_id}/revoke` — revoke invite

Frontend: Add an "Invites" section at the top of the Members (UsersView) tab. Show pending invites with status badges, a "Send Invite" button with inline form (email + role dropdown), and revoke action.

Store additions: `TenantInvite` type, `invites`/`invitesLoading` signals, `loadInvites`, `sendInvite`, `revokeInvite` functions.

- [ ] **Step 1: Add self-serve invite proxy endpoints**
- [ ] **Step 2: Add store types and functions**
- [ ] **Step 3: Add InvitesPanel to UsersView**
- [ ] **Step 4: Build and commit**

---

### Task 3: Organizations — self-serve proxy + OrganizationsView

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add org proxy endpoints)
- Modify: `crates/server/src/handlers/mod.rs`
- Modify: `admin/src/platform/console/store.ts`
- Create: `admin/src/platform/console/views/OrganizationsView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts`

Eight self-serve proxy endpoints (mirror the admin routes):
- `GET/POST /platform/self-serve/tenants/{id}/organizations`
- `GET/PATCH/DELETE /platform/self-serve/tenants/{id}/organizations/{org_id}`
- `GET/POST /platform/self-serve/tenants/{id}/organizations/{org_id}/members`
- `DELETE /platform/self-serve/tenants/{id}/organizations/{org_id}/members/{user_id}`

Add 'organizations' to ConsoleView, add to Organization plane tabs.

Frontend: OrganizationsView with org list (name, slug, member count), create form, expandable detail with member list + add/remove member actions.

- [ ] **Step 1: Add self-serve org proxy endpoints (all 8)**
- [ ] **Step 2: Add store types and functions**
- [ ] **Step 3: Create OrganizationsView**
- [ ] **Step 4: Wire into ConsoleApp and PLANES**
- [ ] **Step 5: Build and commit**

---

### Task 4: User Detail Enhancement — enrich Members expand

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add user detail proxy)
- Modify: `crates/server/src/handlers/mod.rs`
- Modify: `admin/src/platform/console/store.ts` (add UserDetail type, load function)
- Modify: `admin/src/platform/console/views/UsersView.ts` (show enhanced detail on expand)

One self-serve proxy endpoint:
- `GET /platform/self-serve/tenants/{id}/users/{user_id}` — returns detailed user view

This mirrors `GET /manage/users/{id}` which returns `login_attempts`, `locked_until`, `providers`, `permissions`, `active_sessions` — richer data than the list endpoint.

Frontend: When a user row is expanded in Members, load the detail endpoint and show additional fields: login attempt count, lock status with timestamp, auth providers, permissions list.

- [ ] **Step 1: Add user detail proxy endpoint**
- [ ] **Step 2: Add store type and load function**
- [ ] **Step 3: Enhance UsersView expanded detail**
- [ ] **Step 4: Build and commit**

---

### Task 5: Compliance Status — self-serve proxy + view

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add compliance proxy)
- Modify: `crates/server/src/handlers/mod.rs`
- Modify: `admin/src/platform/console/store.ts`
- Create: `admin/src/platform/console/views/ComplianceView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts`

Two self-serve proxy endpoints:
- `GET /platform/self-serve/tenants/{id}/compliance/status`
- `GET /platform/self-serve/tenants/{id}/compliance/report`

These proxy to the existing compliance handlers using the tenant context.

Add 'compliance' to ConsoleView, add to Operator plane tabs.

Frontend: ComplianceView showing compliance posture — pass/fail checks, percentage score, downloadable report link.

- [ ] **Step 1: Add self-serve compliance proxy endpoints**
- [ ] **Step 2: Add store types and functions**
- [ ] **Step 3: Create ComplianceView**
- [ ] **Step 4: Wire into ConsoleApp and PLANES**
- [ ] **Step 5: Build and commit**

---

### Task 6: Passkey Management in Account Settings

**Files:**
- Modify: `admin/src/platform/console/store.ts` (add passkey management functions for own account)
- Modify: `admin/src/platform/console/views/AccountSettingsView.ts` (add passkey section)

No backend changes — the WebAuthn credential endpoints already exist at:
- `GET /auth/webauthn/credentials` — list own credentials
- `PATCH /auth/webauthn/credentials/{id}` — rename
- `DELETE /auth/webauthn/credentials/{id}` — delete

Note: These are tenant-scoped auth endpoints. The platform user needs to call them on the platform tenant (`/t/platform/auth/webauthn/credentials`). The store's `api` helper already prefixes with the platform tenant path.

Store additions: `myPasskeys` signal, `loadMyPasskeys`, `renameMyPasskey`, `deleteMyPasskey` functions.

Frontend: Add a "Passkeys" section to AccountSettingsView showing the user's registered credentials with rename and delete actions.

- [ ] **Step 1: Add store functions for own passkey management**
- [ ] **Step 2: Add passkeys section to AccountSettingsView**
- [ ] **Step 3: Build and commit**

---

### Task 7: Webhook Enhancements

**Files:**
- Modify: `admin/src/platform/console/store.ts` (add webhook types if missing)
- Create or modify: webhook management in an existing view

The webhook CRUD endpoints are already registered on `/admin/*` routes. The dashboard needs self-serve proxies OR we can add webhooks to an existing Operator tab.

Since webhooks are a developer-facing feature (event notifications for integrations), they fit naturally in the **Integrate** tab as a new section alongside the existing guide and auth reference.

Self-serve proxy endpoints needed:
- `GET/POST /platform/self-serve/tenants/{id}/webhooks`
- `GET/PATCH/DELETE /platform/self-serve/tenants/{id}/webhooks/{id}`
- `GET /platform/self-serve/tenants/{id}/webhooks/{id}/deliveries`
- `POST /platform/self-serve/tenants/{id}/webhooks/{id}/test`

Frontend: Add a "Webhooks" subtab to IntegrateView (alongside Guide and Auth Reference) showing webhook list with create/edit/delete, delivery history, and test webhook action.

- [ ] **Step 1: Add self-serve webhook proxy endpoints (7 endpoints)**
- [ ] **Step 2: Add store types and functions**
- [ ] **Step 3: Add Webhooks section to IntegrateView**
- [ ] **Step 4: Build and commit**
