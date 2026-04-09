# Auth Step Redesign — Design Spec

**Date:** 2026-04-09
**Status:** Approved
**Scope:** Onboarding wizard, auth step (`StepAuthMethods` in `admin/src/platform/console/OnboardingWizard.ts:538`) — UI redesign + access-mode wire-up. No new backend providers.

---

## Problem

The wizard's auth step today only exposes one of the two real questions a tenant has to answer when setting up authentication.

It asks **how** users sign in (credentials + social providers, persisted to `allowed_providers`), but it does not ask **who** can sign in. Access mode (`signup_mode`: `open` vs `invite_only`) is silently set from intent defaults at step 2 and never surfaced again. An operator who picks "SaaS" and later realizes they want invite-only signups has no way to discover or change that decision in onboarding — they have to find it in the dashboard after launch.

A second, smaller problem: the credential and social groups are missing slots for methods that are on the near-term roadmap (Email OTP, Passkey, Microsoft, Apple) and methods that will be Pro-gated (SMS OTP, SAML/SSO, custom OIDC). Without those slots, the step under-sells the platform to operators evaluating it during onboarding, and every new method that ships needs a wizard-layout decision instead of just dropping into a pre-shaped slot.

## Solution

Restructure `StepAuthMethods` into a three-layer model that mirrors how identity platforms describe themselves:

1. **Access** — *who* can sign up. New segmented control wired to `signup_mode`, pre-filled from intent defaults.
2. **Credentials & Social** — *how* users prove identity. Existing groups, expanded with slots for Email OTP, Passkey, Microsoft, and Apple as "Coming soon."
3. **Pro** — paid tier slots: SMS OTP, SAML/SSO, Custom OIDC.

No schema changes. No backend provider work. The only new persistence is the existing-but-unsurfaced `signup_mode` field flowing from a new UI control through the existing `updateTenant` call.

The "anonymous session" / "auth-optional public site" idea (where end-user auth is fully disabled while the operator retains platform-console access) is a real future feature, but it lives as a Pro-gated dashboard toggle after launch, not in the onboarding wizard. It is explicitly **out of scope** for this spec.

---

## 1. Step Layout

Headline and subtitle stay as they are today:

> **How will users sign in?**
> Choose authentication methods for your app.

Below that, four `wizard-provider-group` sections (the existing visual primitive), top to bottom:

```
┌─ Access ─────────────────────────────────────────────────┐
│  [ Open ]  [ Invite-only ]      ← segmented control      │
│                                                          │
│  Open:        "Anyone can create an account."            │
│  Invite-only: "Only people you invite can sign up.       │
│                You'll manage invites from the dashboard."│
└──────────────────────────────────────────────────────────┘

┌─ Credentials ────────────────────────────────────────────┐
│  Email / Password                                [toggle]│
│  Magic Link                                      [toggle]│
│    "Passwordless sign-in via email"                      │
│  Email OTP                            [Coming soon] [—]  │
│    "6-digit code sent to email"                          │
│  Passkey                              [Coming soon] [—]  │
│    "Sign in with Face ID, fingerprint, or device PIN"    │
└──────────────────────────────────────────────────────────┘

┌─ Social ─────────────────────────────────────────────────┐
│  Google                                          [toggle]│
│  GitHub                                          [toggle]│
│  Microsoft                            [Coming soon] [—]  │
│  Apple                                [Coming soon] [—]  │
└──────────────────────────────────────────────────────────┘

┌─ [Pro]  Upgrade to unlock ───────────────────────────────┐
│  SMS OTP                                            [—]  │
│  SAML / SSO                                         [—]  │
│  Custom OIDC                                        [—]  │
└──────────────────────────────────────────────────────────┘

                                              [Continue →]
```

The right pane (live preview, derived from tenant branding) is unchanged.

---

## 2. Visual States

A method on this step is in one of three states. The state determines its visual treatment, not its semantic group.

| State | Visual | Toggle | When |
|---|---|---|---|
| **Available** | `wizard-provider-toggle`, no badge | Working | Backend supports it today |
| **Coming soon** | `wizard-provider-toggle` with "Coming soon" badge in `wizard-provider-info` | Disabled | On the free roadmap, backend not yet wired |
| **Pro** | `wizard-pro-item` row inside a `wizard-pro-section` with "Pro" / "Upgrade to unlock" header | Disabled | Gated to a paid tier |

This matches the patterns already in use elsewhere in the wizard:

- "Coming soon" matches Microsoft / Apple in today's `SOCIAL_PROVIDERS` list (`available: false` → "Coming soon" badge inside the toggle).
- "Pro" matches the Pro section already in the branding step (`wizard-pro-section`, `wizard-pro-header`, `wizard-pro-item` — see `OnboardingWizard.ts:1002-1056`).

No new CSS classes are introduced. The existing class set covers all three states.

---

## 3. Data Model

No schema changes. Two existing fields are involved:

| Field | Where it lives today | What changes |
|---|---|---|
| `tenants.signup_mode` (TEXT, default `'open'`) | Migration `020_tenant_signup_mode_and_invites.sql` (archived). Field exists in `Tenant` type and `INTENT_DEFAULTS`. | Now persisted from `StepAuthMethods` via `updateTenant` instead of only being set indirectly at step 2. |
| `tenants.allowed_providers` (TEXT[]) | Already wired through `StepAuthMethods` | Unchanged. |

`INTENT_DEFAULTS[intent].signup_mode` already provides the pre-fill values:

| Intent | Pre-filled signup_mode |
|---|---|
| `saas` | `open` |
| `internal` | `invite_only` |
| `community` | `open` |
| `client-portal` | `invite_only` |
| `marketplace` | `open` |

---

## 4. Component Changes

All changes are inside `StepAuthMethods` in `admin/src/platform/console/OnboardingWizard.ts`. No new files.

### 4.1 New `signupMode` signal

Mirror the existing `providers` signal pattern:

```ts
const currentSignupMode = (tenant.signup_mode as string) || "open";
const [signupMode, setSignupMode] = createSignal<string>(currentSignupMode);
```

### 4.2 Access group

A new `wizard-provider-group` rendered first in the step body, before Credentials. Two segmented buttons (`Open`, `Invite-only`), each with a sub-copy line. The selected option carries an "active" state distinguishable from the inactive option (existing button-style or new — see Open Question 1 below).

Each option's sub-copy:

- **Open:** "Anyone can create an account."
- **Invite-only:** "Only people you invite can sign up. You'll manage invites from the dashboard."

Clicking an option calls `setSignupMode(value)`. No save on click — the value persists at `handleContinue`, same as `providers`.

### 4.3 Credentials group additions

Append two entries to `CREDENTIAL_PROVIDERS`:

```ts
{
  key: "email_otp",
  label: "Email OTP",
  subtitle: "6-digit code sent to email",
  available: false,
},
{
  key: "passkey",
  label: "Passkey",
  subtitle: "Sign in with Face ID, fingerprint, or device PIN",
  available: false,
},
```

The existing `ProviderToggle` function already renders `available: false` as a disabled toggle with a "Coming soon" badge — no changes needed there.

### 4.4 Social group: no changes

`SOCIAL_PROVIDERS` already has Google, GitHub, Microsoft (`available: false`), Apple (`available: false`). Carries forward unchanged.

### 4.5 New Pro section

A `wizard-pro-section` rendered after Social, mirroring the structure in the branding step (`OnboardingWizard.ts:1002-1056`):

```ts
h("div", { class: "wizard-pro-section" },
  h("div", { class: "wizard-pro-header" },
    h("span", { class: "wizard-provider-badge" }, "Pro"),
    h("span", { class: "wizard-pro-label" }, "Upgrade to unlock"),
  ),
  ProItem("SMS OTP"),
  ProItem("SAML / SSO"),
  ProItem("Custom OIDC"),
)
```

`ProItem(label)` is a small inline helper (or just inlined three times — three rows is below the threshold for extracting a helper) that emits a `wizard-pro-item` div with a label span and a disabled checkbox toggle.

### 4.6 `handleContinue` wire-up

Add `signup_mode` to the existing `updateTenant` call:

```ts
async function handleContinue(): Promise<void> {
  setSaving(true);
  await updateTenant(tenant.id, {
    allowed_providers: providers(),
    signup_mode: signupMode(),
  });
  setSaving(false);
  advanceStep();
}
```

### 4.7 Existing guards carry forward

- The "always at least email" guard (`OnboardingWizard.ts:551`) is preserved unchanged.
- The right-pane live preview is unchanged.
- The branding-derived preview values (`previewAppName`, `previewLogoUrl`, etc.) are unchanged.

---

## 5. Backend

The spec was originally written assuming no backend changes. The WRITE path was verified during spec review:

- `signup_mode` is in `UpdateTenantRequest` (`crates/server/src/handlers/management.rs:71`) and the parallel `UpdateConfigRequest` (`management.rs:606`).
- Both update handlers persist it (`management.rs:477` and `management.rs:1121`) and validate it against `["open", "invite_only"]` via `normalize_signup_mode` (`management.rs:81`).
- It is already in the `Tenant` TypeScript type.

**Implementation note (added during execution):** the READ path the wizard frontend actually uses lives in `crates/server/src/handlers/onboarding.rs`, not `management.rs`. Four handlers in that file were missing `signup_mode` from their SQL SELECT and JSON response, which silently broke the round-trip:

- `list_my_tenants` (the personal-tenants SELECT and the team-shared-tenants SELECT)
- `get_my_tenant`
- `update_my_tenant` re-fetch (the response after a successful PATCH)
- `create_my_tenant` (the response after creating a tenant)

All four were fixed in commits `ae54830`, `7765500`, and (final polish) the merge commit. Each fix adds `COALESCE(signup_mode, 'open') AS signup_mode` to the SELECT and `"signup_mode": row.get::<String, _>("signup_mode")` to the JSON builder. No schema changes, no validation changes, no new types — just plugging the field into the existing query/serialization paths.

---

## 6. Out of Scope

Explicit non-goals for this spec, listed here so the implementation plan does not drift into them:

1. **Backend implementation of Email OTP, Passkey, Microsoft OAuth, Apple OAuth, SMS OTP, SAML/SSO, or Custom OIDC.** All of these ship as visual placeholders only. When the backend lands for any of them, lighting it up is a one-line change (`available: false` → `true` for Coming-soon items, or moving a row out of the Pro section for Pro items).
2. **Anonymous-session / auth-optional mode** for public sites where end-user auth is fully disabled. This is a real future feature but lives as a Pro-gated dashboard toggle, not in onboarding.
3. **A "No auth" access mode** in the access segmented control. A tenant that wants no end-user auth at all has nothing meaningful to do in the rest of the onboarding wizard (branding, test login, launch all become meaningless), so it does not belong here.
4. **Surfacing `signup_mode` outside the auth step.** It is set at step 2 (silently, via intent defaults), surfaced + editable at step 3 (this spec), and editable in the dashboard after launch (already exists). No new edit surfaces.
5. **Invite-management UI.** The "you'll manage invites from the dashboard" sub-copy refers to existing dashboard surface. This spec does not add or change that surface.

---

## 7. Open Questions for Implementation

1. **Segmented control visual style.** Does fAuth's design system already have a segmented-control component, or does the implementation introduce a small one inline? If it's inline, the simplest version is two `<button>` elements in a flex row with an "active" class on the selected one — no new abstraction needed for two options.

This is an implementation-time concern and does not affect the design.

---

## 8. Files Touched

- `admin/src/platform/console/OnboardingWizard.ts` — `StepAuthMethods` only. No changes outside this function except possibly a small CSS addition for the segmented control if no existing primitive applies.
- `admin/src/styles/...` — possibly one new class for the segmented control. Defer until implementation reads the existing CSS.

No backend changes, no migrations, no new types, no new files.
