# Email, Verification & Invite UX Polish

Comprehensive UX improvements to the invite flow, email templates, email verification, and shared workspace visibility.

## 1. Invite Resolve Endpoint + Smart Login Page

### New Backend Endpoint

`GET /platform/admin/invites/resolve?token={invite_token}`

**No auth required** (public — the token itself is the secret).

Returns:
```json
{
  "invite_email": "someone@example.com",
  "inviter_email": "victor@getforma.dev",
  "role": "admin",
  "tenant_names": ["Test Workspace"],
  "user_exists": true,
  "status": "pending"
}
```

If token is invalid/expired/revoked, returns 404 with appropriate error.

Implementation: look up `owner_invites` by token hash, check `users` table for email existence. No side effects. Edge case: also verify the invite's `account_owner_user_id` still has an active account (hasn't been deleted/suspended) — don't resolve invites from dead accounts.

### Login Page Changes

When `/platform/login?owner_invite={token}` is loaded:

1. Call the resolve endpoint on page load
2. If valid invite, show a **branded invite banner** above the auth card:
   - "**victor@getforma.dev** invited you to join their team as **Admin** on **Test Workspace**"
   - Auto-fill the email field with the invite email (disabled — can't change it)
3. If `user_exists: true` → show "Sign in to accept invite" as the card title (instead of "Platform Login")
4. If `user_exists: false` → show "Create an account to accept invite" and switch to registration mode (show name field, confirm password, flip CTA to "Create Account")
5. If token invalid → show error alert, hide invite banner, show normal login

### Onboarding Page Changes

When `/platform/onboarding?owner_invite={token}` is loaded (user clicked "New here? Start onboarding"):

1. Preserve the `owner_invite` param in the URL
2. Auto-fill email from the resolved invite (call resolve endpoint)
3. After registration, redirect to `/platform/console?owner_invite={token}` (already implemented)

## 2. Branded Email Templates

Replace the indigo (#4f46e5) button color with the fAuth warm gold palette throughout all email templates.

### Color Updates

| Element | Old | New |
|---------|-----|-----|
| CTA button background | `#4f46e5` (indigo) | `#c4a265` (warm gold) |
| CTA button hover | — | `#d4b275` |
| CTA button text | `#ffffff` | `#0a0a0c` (near-black) |
| Link color | `#4f46e5` | `#c4a265` |
| Header text | `#6b7280` gray "Forma Auth" | `#c4a265` gold "fAuth" |
| Body background | `#f4f4f7` | `#0a0a0c` (dark, matching app) |
| Card background | `#ffffff` | `#111114` (surface dark) |
| Body text | `#374151` (dark gray) | `#e8e6e3` (off-white) |
| Muted text | `#6b7280` | `#6b6a72` |
| Footer border | `#e5e7eb` | `#1e1e24` |

### Template Structure (all emails)

```
┌─────────────────────────────┐
│  Dark background #0a0a0c    │
│  ┌───────────────────────┐  │
│  │ "F" fAuth (gold logo) │  │  ← Header with gold accent
│  ├───────────────────────┤  │
│  │                       │  │
│  │  Email body content   │  │  ← Dark card #111114
│  │  (off-white text)     │  │
│  │                       │  │
│  │  [ Gold CTA Button ]  │  │  ← #c4a265 bg, #0a0a0c text
│  │                       │  │
│  │  fallback link (gold) │  │
│  │                       │  │
│  ├───────────────────────┤  │
│  │  Footer (muted)       │  │
│  └───────────────────────┘  │
└─────────────────────────────┘
```

### Affected Templates

1. **Platform team invite** — "You're invited to fAuth"
2. **Email verification** — "Verify your email"
3. **Password reset** — "Reset your password"
4. **Tenant user invite** — "{app_name} invite"

All share the same `wrap_email_html()` function, so one change updates all.

**Email client compatibility:** Dark email templates require careful implementation — inline styles on every element (not just the wrapper), explicit background colors on `<td>` cells, and MSO conditional comments for Outlook. Commit to dark fully — no light fallback. Use inline styles aggressively to prevent client overrides.

## 3. Email Verification Flow

### Current behavior (broken)
- User registers → verification email sent → user clicks link → sees raw JSON `{"message":"Email verified successfully"}`
- No indication in the app that verification is pending
- No way to resend verification email

### New behavior

#### Verify Route (`/t/{slug}/auth/verify-email?token=...`)
- On success: redirect to `/platform/console?verified=true` (or `/platform/login?verified=true` if not logged in)
- On failure (expired/invalid): redirect to `/platform/console?verify_error=expired` (or login equivalent)
- Return HTML redirect page (not JSON), branded with fAuth styling, with a "Redirecting..." message and meta-refresh fallback

#### Console Verification Banner
- If `currentUser().email_verified === false`, show a persistent banner at the top of the console:
  - "Verify your email address to secure your account. Check your inbox for a verification link."
  - **Resend** button inline
  - **Dismiss** button (dismisses for this session only, comes back on next login)
- The `/auth/me` endpoint already returns `email_verified` — frontend just needs to surface it

#### Verified Toast
- When console loads with `?verified=true` query param, show success toast: "Email verified successfully"
- Clean the param from URL

#### Resend Verification Endpoint
`POST /t/platform/auth/resend-verification` (authenticated) — generates a new token, sends new verification email. Rate-limited to 1 per 60 seconds per user (separate from the existing 3-per-email-per-10-min rate limit on other email endpoints — don't overwrite those).

Note: An earlier session added code-based verification endpoints (`POST /t/{slug}/auth/verify-email` sends 6-digit code, `POST /t/{slug}/auth/verify-email/confirm` validates). Those are a different flow. We're using the **link-based** token flow (`GET /t/{slug}/auth/verify-email?token=...`) for signup verification emails — simpler UX. The code-based flow can coexist for future re-verification or sensitive actions.

## 4. Shared Workspace Role Badge in Nav

When a user enters a workspace that is `access === 'team'`, show the role badge in the tenant navigation bar.

### Where it appears
The workspace header/nav area that shows the tenant name. Add a small badge next to the tenant name:

```
Test Workspace [Admin]
```

Or in the sidebar/nav where the tenant name is displayed. Use the same `badge badge-subtle` styling from the member list.

### Data source
The `team_role` field is already returned by `list_my_tenants` for team tenants. Store it on the tenant object and read it in the workspace nav component.

## 5. Summary of Changes

### Backend (Rust)
1. New endpoint: `GET /platform/admin/invites/resolve?token=...` (public)
2. New endpoint: `POST /t/platform/auth/resend-verification` (authenticated, if not existing)
3. Update verify-email handler: return HTML redirect instead of JSON
4. Update `email.rs`: rebrand all templates with dark theme + gold palette

### Frontend (TypeScript)
5. Login page: detect invite, call resolve, show branded banner, auto-fill email, toggle login/signup
6. Onboarding page: preserve invite param, auto-fill email
7. Console: verification banner when `email_verified === false`
8. Console: `?verified=true` toast handling
9. Console: workspace nav role badge for team tenants
10. Store: add `team_role` to Tenant type, surface it in workspace context
