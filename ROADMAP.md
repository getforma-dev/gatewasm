# GateWASM — Future Implementation Roadmap

> This document maps every login path, user type, and feature needed to make GateWASM a production-grade auth platform. It serves as the implementation guide for future work. Nothing here is built yet — this is the plan.

## Identity Model

GateWASM is a **multi-tenant auth-as-a-service platform**. We are the identity provider. We own the user database, JWT signing keys, OAuth flows, and session management. We do NOT use Cognito, IAM, Azure AD, or any other identity provider under the hood.

**How we compare:**

| Feature | Cognito | Auth0 | Clerk | GateWASM |
|---------|---------|-------|-------|----------|
| User store | AWS-managed | MongoDB | Postgres | PostgreSQL (we own) |
| JWT signing | RSA | RS256 | RS256 | EdDSA (Ed25519) |
| Hosted UI | Basic | Universal Login | Components | Forma.js pages |
| Multi-tenancy | User pools | Organizations | Organizations | Tenants + Orgs |
| Pricing | Per-MAU | Per-MAU | Per-MAU | Self-hosted or Per-MAU |
| Open source | No | No | No | Yes |

---

## Login Paths — Complete Breakdown

### Path 1: Individual Developer (localhost)

**Persona:** Solo dev building a side project, testing GateWASM locally.

**Flow:**
1. Dev signs up at `auth.getforma.dev/t/forma/login` → gets a Forma account
2. Dev creates a tenant via Platform API or self-service dashboard (future)
3. Dev gets a tenant slug (e.g., `myapp`) and management API key
4. Dev adds `<script src="https://cdn.getforma.dev/bridge/v1/gate-bridge.min.js">` to their app
5. Dev initializes bridge with `serverUrl: 'https://auth.getforma.dev'`, `clientId: 'myapp-hosted'`
6. Users of dev's app sign up/login at `auth.getforma.dev/t/myapp/login`

**What's needed (mostly done):**
- [x] Tenant creation via Platform API
- [x] Hosted login page with tenant branding
- [x] Bridge.js CDN distribution
- [x] OAuth (Google, GitHub) configured per-tenant
- [ ] Self-service tenant creation (sign up → get your own tenant without API calls)
- [ ] Localhost callback URL allowlist (allow `http://localhost:*` by default in dev mode)

**Status:** Working today. Dev can test full auth flow from localhost via CORS `*`.

---

### Path 2: Production App (the median mature dev)

**Persona:** Dev with a real app in production, needs auth for their users.

**Flow:**
1. Dev already has a tenant from Path 1
2. Dev configures `login_redirect_url` to their production app's callback
3. Dev optionally sets up a custom domain (`auth.myapp.com` → GateWASM)
4. Dev's users sign up and log in through the hosted login page or embedded bridge.js
5. Dev uses Management API (`mgmt_` key) for user administration from their backend
6. Dev's frontend validates JWTs using the JWKS endpoint (`/auth/jwks`)

**What's needed:**
- [x] Management API (user listing, blocking, stats, audit log)
- [x] Tenant config (providers, origins, branding)
- [ ] Custom domains per tenant (CNAME → App Runner, TLS cert per-tenant)
- [ ] Webhook notifications (user.created, user.deleted, login.success, login.failed)
- [ ] Email verification flow (send verification email, not just endpoint)
- [ ] Email templates (welcome, verification, password reset — configurable per-tenant)

**Status:** Core flow works. Custom domains and webhooks are the gaps.

---

### Path 3: B2B / Enterprise (organizations within a tenant)

**Persona:** SaaS company where their customers are organizations, not individual users.

**Example:** Slack (tenant) → Workspaces (organizations) → Users belong to workspaces.

**Flow:**
1. SaaS company creates a GateWASM tenant
2. SaaS company's customers create organizations within the tenant
3. Organization admin invites team members via email
4. Invited users sign up and are automatically added to the organization
5. Organization-level roles (admin/member/viewer) control access
6. Users can belong to multiple organizations within the same tenant

**What's needed (NOT built yet):**

#### Organizations Table
```sql
CREATE TABLE organizations (
    id TEXT PRIMARY KEY,           -- 'org_' + uuid
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,            -- URL-friendly, unique within tenant
    logo_url TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tenant_id, slug)
);
```

#### Organization Memberships
```sql
CREATE TABLE org_memberships (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL DEFAULT 'member',  -- 'owner', 'admin', 'member', 'viewer'
    invited_by TEXT REFERENCES users(id),
    joined_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(org_id, user_id)
);
```

#### Invitations
```sql
CREATE TABLE invitations (
    id TEXT PRIMARY KEY,           -- 'inv_' + uuid
    org_id TEXT NOT NULL REFERENCES organizations(id),
    email TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'member',
    token TEXT NOT NULL UNIQUE,    -- Secure random token
    invited_by TEXT REFERENCES users(id),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

#### API Endpoints
```
POST   /org                          Create organization
GET    /org                          List user's organizations
GET    /org/{slug}                   Get organization details
PATCH  /org/{slug}                   Update organization
DELETE /org/{slug}                   Delete organization (owner only)

GET    /org/{slug}/members           List organization members
POST   /org/{slug}/invite            Invite user by email
DELETE /org/{slug}/members/{id}      Remove member
PATCH  /org/{slug}/members/{id}      Update member role

POST   /invite/accept                Accept invitation (with token)
GET    /invite/{token}               Get invitation details
```

#### JWT Claims Extension
```json
{
  "sub": "usr_xxx",
  "tenant_id": "ten_xxx",
  "roles": ["user"],
  "orgs": [
    { "id": "org_xxx", "slug": "acme", "role": "admin" },
    { "id": "org_yyy", "slug": "beta", "role": "member" }
  ]
}
```

**Status:** Not built. This is the next major feature.

---

### Path 4: Forma Dogfooding (us managing our own users)

**Persona:** Forma (the company) uses GateWASM to manage GateWASM's users.

**Flow:**
1. Forma team members log in at `auth.getforma.dev/t/forma/login`
2. Admin users see the dashboard with user management, stats, audit logs
3. Admin users can block/unblock users, view login history, etc.
4. This IS the demo — customers see Forma using its own product

**What's needed:**
- [x] Forma tenant created (`slug: forma`)
- [x] Dashboard at `/t/forma/dashboard`
- [x] Admin API (JWT-protected `/admin/*` endpoints)
- [x] Login redirects to dashboard after auth
- [ ] Admin role assignment UI (currently done via SQL)
- [ ] Self-service tenant creation from dashboard (customer signs up → gets own tenant)

**Status:** Working today. Forma team can log in and manage users.

---

### Path 5: Enterprise SSO (SAML/OIDC federation)

**Persona:** Large company that uses their own identity provider (Okta, Azure AD, OneLogin).

**Flow:**
1. Enterprise customer creates a GateWASM tenant
2. Enterprise configures SSO: adds their IdP's SAML metadata or OIDC discovery URL
3. Users from the enterprise sign in via their company's SSO
4. GateWASM creates/updates user records from SAML assertions
5. Enterprise controls which users can access the app via their IdP

**What's needed (NOT built yet):**
- [ ] SAML 2.0 Service Provider (SP-initiated SSO)
- [ ] OIDC federation (external IdP as identity source)
- [ ] IdP metadata import (XML or discovery URL)
- [ ] Attribute mapping (SAML attributes → GateWASM user fields)
- [ ] JIT (Just-In-Time) user provisioning from SAML assertions
- [ ] Per-tenant SSO configuration
- [ ] Force-SSO mode (disable password login for a tenant)

**Status:** Not built. Lower priority than Organizations.

---

## Feature Roadmap

### Phase 1: Polish (Current)
- [x] CloudFront CDN for bridge.js
- [x] Dashboard with admin panel
- [x] OAuth auto-linking (same email)
- [x] Distribution page (/get-started)
- [x] Swagger API docs
- [ ] Fix admin role assignment (UI or management API)
- [ ] Email sending (verification, password reset — currently endpoint-only)

### Phase 2: Organizations & Invitations
- [ ] `organizations` table + CRUD API
- [ ] `org_memberships` table + member management
- [ ] `invitations` table + invite flow
- [ ] Organization-scoped JWT claims (`orgs` array)
- [ ] Dashboard org selector (switch between organizations)
- [ ] Invite acceptance page

### Phase 3: Self-Service & Developer Experience
- [ ] Self-service tenant creation (sign up → get tenant, no API calls)
- [ ] Developer dashboard (API keys, OAuth config, webhook setup)
- [ ] npm package (`@gatewasm/bridge`)
- [ ] React/Vue/Svelte SDK wrappers
- [ ] Documentation site (separate from Swagger)

### Phase 4: Enterprise
- [ ] Custom domains per tenant (CNAME → TLS cert per-tenant)
- [ ] SAML 2.0 SP support
- [ ] OIDC federation
- [ ] Webhook notifications
- [ ] Audit log export (CSV/JSON)
- [ ] SOC2 compliance mode

### Phase 5: Scale
- [ ] Multi-region deployment
- [ ] Read replicas for PostgreSQL
- [ ] Redis session cache
- [ ] Usage-based billing (Stripe integration)
- [ ] Rate limiting per-tenant (not just per-IP)

---

## Architecture Decisions

### Why Not Cognito?

We are NOT using Cognito because:
1. **Vendor lock-in** — Cognito users are stuck in AWS. Our users own their data in PostgreSQL.
2. **Limited customization** — Cognito's hosted UI is minimal. We have full control with Forma.js.
3. **Multi-tenancy** — Cognito uses one user pool per tenant. We use one database with tenant isolation.
4. **Pricing** — Cognito charges per-MAU. Self-hosted GateWASM is free.
5. **We ARE the product** — We're building a Cognito competitor, not using one.

### Why Not Azure AD / IAM?

- **Azure AD** is for enterprise identity (SSO, directory). We'll integrate WITH it (Path 5), not use it as our backend.
- **IAM** is for AWS resource access control, not user authentication. Different layer entirely.

### What About Session Tracking?

We use **JWTs + refresh tokens in PostgreSQL**. No Cognito-style session service. The `refresh_tokens` table tracks active sessions, and the `login_attempts` table tracks login history. The dashboard shows this data.

For future server-side session management (if needed), the `sessions` table is already created but reserved.

---

## Implementation Notes

### Adding an Organization (Phase 2 sketch)

```rust
// New middleware: extract_org (after extract_tenant, before require_auth)
// Reads org slug from path or header, validates membership

// JWT claims extended:
pub struct Claims {
    pub sub: String,
    pub tenant_id: String,
    pub roles: Vec<String>,
    pub orgs: Vec<OrgClaim>,  // NEW
}

pub struct OrgClaim {
    pub id: String,
    pub slug: String,
    pub role: String,  // "owner", "admin", "member", "viewer"
}
```

### Invitation Flow

1. Admin calls `POST /org/{slug}/invite` with `{ email: "new@user.com", role: "member" }`
2. Server creates invitation record with secure token, sends email (future: just returns token for now)
3. Invited user clicks link: `auth.getforma.dev/invite/{token}`
4. If user has account → add to org. If not → redirect to signup, then add to org.
5. Invitation marked as accepted, membership created.

### Custom Domains (Phase 4 sketch)

Each tenant gets their own domain:
1. Tenant sets `custom_domain` via management API
2. CDK creates ACM certificate (DNS validation at tenant's registrar)
3. App Runner associated domain or CloudFront distribution with the custom domain
4. Tenant adds CNAME at their registrar

This is complex because App Runner has limited custom domain support. May need to put a CloudFront distribution in front of App Runner for per-tenant SSL.
