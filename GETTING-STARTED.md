# GateWASM — Getting Started Guide

How users move through the system, from first visit to daily use.

---

## The Four User Types

| Role | What they do | How they get in |
|------|-------------|-----------------|
| **Platform Owner** | You. Manages all customers, tenants, billing, team | `/platform/login` |
| **Customer** | Signs up to use GateWASM for their app. Creates and owns tenants | `/platform/onboarding` |
| **Tenant Admin** | Manages users, roles, webhooks for a specific tenant | `/t/{slug}/admin` |
| **End User** | Signs into a customer's app via GateWASM auth | `/t/{slug}/login` |

---

## Flow 1: New Customer Signs Up (Self-Serve)

**Share this link:** `/platform/onboarding`

```
1. Customer visits /platform/onboarding
2. Creates account (email/password or OAuth)
3. Redirected to /platform/console
4. Creates their first tenant (name + slug)
5. Gets a management API key (mgmt_...)
6. Integrates into their app using bridge.js or the API
```

**After setup, the customer has:**
- A platform account (for managing their tenants)
- A tenant with a slug (e.g., `acme`)
- A management API key (for backend automation)
- A login page for their users at `/t/acme/login`

---

## Flow 2: End User Signs Into a Tenant

**Share this link:** `/t/{slug}/login` (e.g., `/t/acme/login`)

```
1. End user visits /t/acme/login
2. Registers (if open signup) or signs in
3. Redirected to the tenant's configured redirect URL
   (default: /t/acme/dashboard)
```

This is the page customers embed or link from their own app. End users never see the platform — they only interact with the tenant login.

**Signup modes:**
- **Open** — anyone can register
- **Invite-only** — requires an invite link from the tenant admin

---

## Flow 3: Invite Someone to Your Owner Team

**You do this from:** `/platform/admin/dashboard` → Invites tab

```
1. You go to /platform/admin/dashboard
2. Click Invites in the sidebar
3. Enter their email, create invite
4. Copy the invite URL (expires in 72 hours)
5. Share the link with them
6. They visit the link → /platform/onboarding?invite=...
7. They create an account
8. They get the platform_owner role automatically
9. They can now access /platform/admin/dashboard
```

**Requirements for owner access:** Platform account + owner role + MFA enabled.

---

## Flow 4: Tenant Admin Manages Users

**Access:** `/t/{slug}/admin` (requires admin role on that tenant)

```
1. Customer creates a tenant via /platform/console
2. Customer uses the Management API to create users and assign roles:

   # Create a user with admin role
   curl -X POST http://localhost:3001/manage/users \
     -H "Authorization: Bearer mgmt_..." \
     -H "Content-Type: application/json" \
     -d '{"email":"admin@acme.com","password":"...","name":"Admin","role":"admin"}'

3. The admin user signs in at /t/acme/login
4. After login, they can access /t/acme/admin
5. From the admin console they manage: users, roles, webhooks, audit log
```

---

## Quick Reference: All Routes

### Public (no auth required)

| Route | Purpose |
|-------|---------|
| `/platform/onboarding` | New customer signup |
| `/platform/login` | Returning platform user login |
| `/t/{slug}/login` | Tenant end-user login/register |
| `/platform/benchmark` | Public performance benchmark |

### Authenticated

| Route | Who | Purpose |
|-------|-----|---------|
| `/platform/console` | Platform users | Create/manage tenants, API keys |
| `/platform/admin/dashboard` | Platform owners | Customers, billing, timeline, invites |
| `/t/{slug}/dashboard` | Tenant end-users | Profile, integration, settings |
| `/t/{slug}/admin` | Tenant admins | Users, roles, webhooks, audit log |

### API

| Prefix | Auth | Purpose |
|--------|------|---------|
| `/auth/*` | Tenant-scoped | Login, register, token, MFA, OAuth |
| `/manage/*` | `mgmt_` API key | Backend user/role/webhook management |
| `/platform/*` | Platform JWT | Self-serve tenant CRUD |
| `/admin/*` | Admin JWT | Tenant admin operations |

---

## Local Dev Quick Start

```bash
# 1. Start database + server
npm run db && npm run dev

# 2. Seed test data (first time only)
npm run db:seed

# 3. Test the flows
```

| Flow | URL | Credentials |
|------|-----|-------------|
| Platform login | http://localhost:3001/platform/login | *create account at onboarding* |
| Platform onboarding | http://localhost:3001/platform/onboarding | *new signup* |
| Tenant login (forma) | http://localhost:3001/t/forma/login | `admin@local.dev` / `Test1234!` |
| Tenant login (acme) | http://localhost:3001/t/acme/login | `john@acme.test` / `Test1234!` |
| Owner dashboard | http://localhost:3001/platform/admin/dashboard | *login via platform first* |
| Tenant admin | http://localhost:3001/t/forma/admin | *login as admin@local.dev first* |

---

## OAuth Setup (Optional)

To enable Google/GitHub sign-in locally, add to `.env`:

```
GOOGLE_CLIENT_ID=your-google-client-id
GOOGLE_CLIENT_SECRET=your-google-client-secret
GITHUB_CLIENT_ID=your-github-client-id
GITHUB_CLIENT_SECRET=your-github-client-secret
```

OAuth callback URLs to configure in your provider:
- Google: `http://localhost:3001/auth/callback/google`
- GitHub: `http://localhost:3001/auth/callback/github`

Without these, all pages work with email/password auth only.
