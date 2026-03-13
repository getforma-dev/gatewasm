# GateWASM Auth — Provider/Customer/End-User Runbook

This runbook explains exactly how to operate GateWASM as an auth SaaS provider.

## 1. Personas and Planes

1. Provider plane (you, Victor): manage customers globally.
2. Customer tenant plane (John, Mike): each customer manages their own tenant config and users.
3. End-user plane (students, API buyers): users sign in to each customer tenant.

## 2. Route Map (Who Uses What and Why)

| Route | Who uses it | Purpose |
|---|---|---|
| `https://auth.getforma.dev/platform/admin/login` | Victor + invited Forma operators | Provider control plane login (internal) |
| `https://auth.getforma.dev/platform/admin/dashboard` | Victor + invited Forma operators | Global customer/tenant oversight |
| `https://auth.getforma.dev/platform/onboarding` | New customer admins (John, Mike) | Signup + first tenant creation |
| `https://auth.getforma.dev/platform/login` | Returning customer admins | Login to manage their own tenants |
| `https://auth.getforma.dev/platform/console` | Customer admins | Customer-level tenant list and management |
| `https://auth.getforma.dev/t/{slug}/admin` | Customer tenant admins | Tenant-level operations (users, roles, webhooks, audit) |
| `https://auth.getforma.dev/t/{slug}/login` | End users of a customer app | Hosted login/signup page for that tenant |

Use this quick rule:

1. Provider-level/global actions -> `/platform/admin/*`
2. Customer account actions -> `/platform/*`
3. End-user and tenant-operator actions -> `/t/{slug}/*`

## 3. One-Time Setup for Victor (Provider Owner)

1. Ensure your owner email is in `PLATFORM_OWNER_EMAILS`.
Example:

```bash
PLATFORM_OWNER_EMAILS=victor@getforma.dev
```

2. Sign in at `https://auth.getforma.dev/platform/admin/login`.
  - Owner plane requires Google/GitHub SSO and MFA verification.
3. Verify you can open `https://auth.getforma.dev/platform/admin/dashboard`.
4. Verify public and internal docs split:
- Public: `https://auth.getforma.dev/docs`
- Internal: `https://auth.getforma.dev/internal/docs` (requires `plat_...`)
5. To add CTO/ops teammates (invite-only owner access):
- Open `https://auth.getforma.dev/platform/admin/dashboard`
- Use **Invite Provider Operators** with their company email.
- Copy/share the generated invite link once (shown at creation).
- Invitee signs in at `/platform/login` via that link using SSO, completes MFA, then gets `platform_owner`.

## 4. John Onboarding Flow (Study App)

1. John opens `https://auth.getforma.dev/platform/onboarding`.
2. John creates account and tenant:
- Tenant name: `John Study`
- Tenant slug: `john-study`
3. John gets a management key once:
- `mgmt_...` (shown once, not recoverable)
4. John stores key in backend only (never frontend):

```bash
GATEWASM_BASE_URL=https://auth.getforma.dev
GATEWASM_MGMT_API_KEY=mgmt_xxx
```

5. John configures tenant login redirect to his app callback:
- Good: `https://app.johnstudy.com/auth/callback`
- Bad: `https://auth.getforma.dev/t/john-study/login`
- Bad in prod: `http://localhost:3000/callback`

6. John’s students use:
- `https://auth.getforma.dev/t/john-study/login`

7. John manages tenant users/settings at:
- `https://auth.getforma.dev/t/john-study/admin`

## 5. Mike Onboarding Flow (Weather API App)

1. Mike opens `https://auth.getforma.dev/platform/onboarding`.
2. Mike creates tenant:
- Name: `Mike Weather`
- Slug: `mike-weather`
3. Mike saves backend key:

```bash
GATEWASM_BASE_URL=https://auth.getforma.dev
GATEWASM_MGMT_API_KEY=mgmt_xxx
```

4. Mike sets redirect URL to his app callback:
- Example: `https://weather.mikeapp.com/auth/callback`

5. Mike’s buyers log in at:
- `https://auth.getforma.dev/t/mike-weather/login`

6. Mike uses backend automation with `mgmt_` key to track users before issuing his API keys.

## 6. What Victor Sees and Tracks

From `https://auth.getforma.dev/platform/admin/dashboard`, Victor can track:

1. Total customers using your auth service.
2. Active customers.
3. Total active tenants.
4. Growth and status trends.

For API-level visibility (provider plane):

1. `GET /platform/admin/stats`
2. `GET /platform/admin/customers`
3. `GET /platform/admin/customers/{id}`
4. `GET /platform/admin/customers/{id}/tenants`

Victor tracks John and Mike as customers, not as their individual end-users by default. End-user management stays inside each tenant plane.

## 7. Customer Backend Usage Pattern (`mgmt_` Key)

Use `mgmt_` key server-to-server only:

```bash
curl -s https://auth.getforma.dev/manage/users \
  -H "Authorization: Bearer $GATEWASM_MGMT_API_KEY"
```

Do not put `mgmt_` keys in:

1. Browser JavaScript
2. Mobile apps
3. Public repos

## 8. Daily Operating Checklist (Victor)

1. Check provider dashboard for customer growth/health.
2. Review suspicious tenants or error spikes.
3. Confirm docs boundaries:
- `/docs` stays public-only
- `/internal/docs` stays protected
4. Spot-check customer onboarding completion:
- customer account exists
- at least one tenant exists
- redirect URL is non-localhost for production tenants

## 9. Common Mistakes and Fixes

1. Redirect points to localhost in production:
- Fix tenant `login_redirect_url` to real HTTPS callback.

2. Customer asks “where do I put `mgmt_` key?”:
- Answer: backend env vars only.

3. Customer sends `mgmt_` from browser:
- Move calls to customer backend API and proxy server-side.

4. Customer can log in but callback fails:
- Validate exact redirect URI in tenant config and OAuth client setup.

## 10. Production Guardrails

1. Provider admins use `/platform/admin/*`.
2. Customer admins use `/platform/*` and `/t/{slug}/admin`.
3. End users use `/t/{slug}/login`.
4. `/manage/*` accepts `mgmt_` key only.
5. Owner/global operations stay in provider plane, not tenant plane.

## 11. End-to-End Story (Victor -> John -> Mike)

1. Victor setup (one time):
- Set `PLATFORM_OWNER_EMAILS=victor@getforma.dev`.
- Log in at `/platform/admin/login`.
- Confirm dashboard access at `/platform/admin/dashboard`.

2. John becomes a customer:
- John signs up at `/platform/onboarding`.
- John creates tenant `john-study`.
- John stores `mgmt_...` in his backend env only.
- John sets `login_redirect_url` to his React app callback.
- John's students log in via `/t/john-study/login`.
- John manages students/operators in `/t/john-study/admin`.

3. Mike becomes a customer:
- Mike signs up at `/platform/onboarding`.
- Mike creates tenant `mike-weather`.
- Mike stores `mgmt_...` in backend env only.
- Mike sets callback URL for his weather app.
- Mike's buyers log in via `/t/mike-weather/login`.
- Mike manages user lifecycle in `/t/mike-weather/admin` and backend `/manage/*` calls.

4. Victor monitors business usage:
- Open `/platform/admin/dashboard` for totals and trends.
- Use `/platform/admin/customers` and `/platform/admin/customers/{id}/tenants` for exact account mapping.
- This gives Victor customer-level visibility (John and Mike), while each customer handles their own end users.

5. Security boundary for everyone:
- `mgmt_` keys: backend automation only.
- Browser UI auth: JWT session, tenant-scoped role checks.
- Internal docs: `/internal/docs` only; public docs stay at `/docs`.
