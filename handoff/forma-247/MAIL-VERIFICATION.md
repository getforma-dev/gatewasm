# Email verification: provider-side slice

Implements steps 2 and 3 of the FORMA-247 mail-verification follow-on plan (`Forma/work/FORMA-247/MAIL-VERIFICATION-FOLLOW-ON-PLAN.md`): truthful delivery outcomes, and a hashed, tenant/user/address-bound, atomic one-use challenge. Step 4's "Continue to GenUI" destination waits for GenUI's reviewed sign-in/pending/resend route contract (APP-009/074). Nothing here sends real mail or configures a transport, and this source is not the selected rehearsal image (`dfba787`).

## Behavior

- **Delivery outcome.** `email::send_email` returns `suppressed`, `unconfigured` or `provider_accepted`, or an error, with a 15 s bound per provider call. `provider_accepted` means Resend or the SMTP relay took the message. It never means mailbox receipt. Signup still commits when delivery fails. The outcome of the challenge's send is recorded on the account (`users.verification_delivery`, `verification_delivery_at`), and a later resend's outcome is never overwritten by an earlier task. Legacy invite responses no longer report `sent` while `SUPPRESS_EMAIL` is set.
- **Challenge.** 32 random bytes (`vt_` + base64url). Only the SHA-256 hash is stored, bound to the issuance address (`verification_email`). It lasts 24 hours. Migration 8 withdraws pre-existing plaintext challenges, so affected users resend.
- **Opening the link** (`GET /t/{slug}/auth/verify-email?token=`) shows a confirmation page and changes nothing, so scanner prefetches cannot verify. The page's form POSTs the challenge in the body to a URL without a query string. Every verification page sends `Cache-Control: no-store`, `Referrer-Policy: no-referrer` and a nonce CSP with no scripts, `form-action 'self'`.
- **Consumption** (`POST /t/{slug}/auth/verify-email`, form `token`) locks the row and then consumes it if the hash, this tenant, the unchanged issuance address and wall-clock expiry (re-read after the lock) all match. Exactly one concurrent consumer wins and emits `user.email_verified`. Wrong tenant, expired, replayed, changed-address or hash-as-token attempts return HTML 400 and leave the account unverified. No session is created or resumed. Tenant users see "sign in again to continue", and the platform tenant keeps its `/platform/login?verified=true` link.

## Resend contract (for GenUI's backend-mediated resend)

`POST /t/{slug}/auth/resend-verification` with `Authorization: Bearer <fAuth access token>` for the account. The address is always the account's stored address, and the tenant must match the token's user. The existing per-IP limiter still applies, plus a 60 s per-account cooldown claimed atomically with challenge rotation.

| Status | Body `status` / `error` | Meaning |
|---|---|---|
| 202 | `status: provider_accepted` | New challenge issued and accepted by the provider. Not proof of receipt. The previous link is dead. |
| 200 | `status: already_verified` | Nothing sent. |
| 429 | `error: resend_cooldown` | Within 60 s of the last send for this account (or `too_many_requests` from the IP limiter). |
| 503 | `error: email_unavailable` | Mail suppressed or no transport configured. Nothing rotated or sent; the previous link still works. |
| 502 | `error: email_failed` | Provider refused or timed out. The previous link was replaced, and the new one may not arrive. Retry after the cooldown. |

GenUI should show the user a fixed sanitized message for each case. It must never forward the fAuth token, response JSON or any `Set-Cookie` to the browser.

## Evidence

Integration cases are in `crates/server/tests/cases/mail_verification.rs` and use an owned loopback SMTP capture with `.test` addresses. They cover the real message link (exact recipient, sender, branded subject), hash-only storage, a GET that changes nothing, wrong tenant, success then replay, expired, changed address, the hash presented as the token, 8 concurrent consumes (exactly 1 wins), expiry during a row-lock wait (this test caught the first single-UPDATE version, since PostgreSQL rechecks a WHERE after a lock wait only when the row changed), suppressed/unconfigured outcomes recorded and a 503 with nothing rotated, cooldown, 4 concurrent resends (exactly 1 accepted), rotation killing the old link, `already_verified`, and a provider refusal reported as 502/`failed`.

## Not done here

- Step 1 and step 4 depend on GenUI: the exact entry route, a client-association-derived "Continue to GenUI" destination validated from the hosted request (never mail- or browser-supplied), and the pending-state UI. The success page currently gives no link for tenant users.
- No controlled-inbox trial. External delivery needs its own agreed sender, transport, inbox allowlist, limits and cleanup.
- Provider error strings are logged as returned by `lettre`/`resend-rs`, without links or bodies. They were not audited exhaustively.
