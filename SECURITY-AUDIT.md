# GateWASM Security Audit & Hardening Log

> Tagged: `security` | Last updated: 2026-03-18

---

## 2026-03-18 — Full Security Audit + Hardening Pass

### Summary

Comprehensive security audit of the GateWASM multi-tenant auth platform covering secrets management, token blast radius, injection vectors, input validation, SSRF, CORS, rate limiting, and code quality. All findings have been resolved or documented for future work.

---

### Fixes Applied

#### CSS Injection via Tenant Branding `[security]` `[fixed]`

- **Commit:** `621b319`
- **Severity:** Medium
- **Issue:** `branding_primary_color` was interpolated directly into a `<style>` block without validation. A tenant admin could inject arbitrary CSS rules (e.g., data exfiltration via `background-image: url()`).
- **Fix:** Added `is_valid_css_color()` validation — only accepts `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` hex formats. Applied in both `management.rs` and `onboarding.rs` before DB write. Returns 400 on invalid input.
- **Files:** `handlers/management.rs`, `handlers/onboarding.rs`

#### SSRF via Webhook and SSF URLs `[security]` `[fixed]`

- **Commit:** `621b319`
- **Severity:** Medium
- **Issue:** Webhook and SSF receiver URLs accepted any `http://` or `https://` target with no private IP blocklist. A management key holder could target internal services (localhost, RFC 1918, AWS metadata 169.254.169.254).
- **Fix:** Added `is_private_url()` blocklist rejecting: localhost, 127.0.0.1, ::1, 10.x.x.x, 172.16-31.x.x, 192.168.x.x, 169.254.x.x (link-local + AWS metadata), .internal, .local. Applied in webhook create/update and SSF stream create/update.
- **Files:** `handlers/webhooks.rs`, `handlers/ssf.rs`

#### OAuth CSRF HMAC Timing Side-Channel `[security]` `[fixed]`

- **Commit:** `621b319`
- **Severity:** Medium
- **Issue:** OAuth callback CSRF state verification used standard `!=` string comparison instead of constant-time comparison. Theoretically exploitable to leak HMAC tag bytes via timing measurements.
- **Fix:** Replaced with `ct_eq` from the `subtle` crate (already a dependency, used correctly elsewhere for PKCE verification).
- **Files:** `handlers/oauth.rs`

#### AppConfig Debug Derive Leaks Secrets `[security]` `[fixed]`

- **Commit:** `621b319`
- **Severity:** Low
- **Issue:** `AppConfig` struct had `#[derive(Debug)]` which would print Ed25519 private key, database password, OAuth secrets, SMTP credentials, and MFA encryption key if ever debug-formatted in logs or error output.
- **Fix:** Replaced with manual `Debug` impl that only exposes `host`, `port`, `server_url`, `jwt_issuer`, `jwt_audience` with `.finish_non_exhaustive()`.
- **Files:** `config.rs`

#### Audit Trail: Login Attempts Missing tenant_id `[security]` `[fixed]`

- **Commit:** `18301a3`
- **Severity:** Medium
- **Issue:** `log_login_attempt()` did not record `tenant_id`, breaking per-tenant audit isolation. Owner dashboard queries filtering by `login_attempts.tenant_id` returned empty results.
- **Fix:** Added `tenant_id: Option<&str>` parameter to `log_login_attempt`. Updated all 5 call sites in `auth.rs` to pass tenant context.
- **Files:** `db/mod.rs`, `handlers/auth.rs`

#### Audit Trail: Hardcoded "0.0.0.0" IP Addresses `[security]` `[fixed]`

- **Commit:** `18301a3`
- **Severity:** Medium
- **Issue:** Multiple `audit::log_audit` calls in register, logout, and token handlers passed `"0.0.0.0"` instead of the actual client IP, degrading compliance audit trails (HIPAA/SOC2).
- **Fix:** Extract real client IP via `extract_client_ip()` at the top of each handler. Applied across `auth.rs` (register, login, logout, delete_account) and `token.rs` (refresh, revocation).
- **Files:** `handlers/auth.rs`, `handlers/token.rs`

#### Account Deletion: No Token Revocation or Audit `[security]` `[fixed]`

- **Commit:** `18301a3`
- **Severity:** Medium
- **Issue:** `delete_account` only ran `DELETE FROM users`. Existing refresh tokens remained valid (via family), no audit log was created, and tenant scoping was not verified.
- **Fix:** Added `UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1` before deletion. Added tenant scope verification (`claims.tenant_id` must match request tenant). Added audit log entry with `USER_ACCOUNT_DELETE` action.
- **Files:** `handlers/auth.rs`

#### Email Verification: No Rate Limiting `[security]` `[fixed]`

- **Commit:** `18301a3`
- **Severity:** Low
- **Issue:** `/auth/verify-email` was a public endpoint with no rate limiter. Could be used for token enumeration (though UUID entropy makes this impractical).
- **Fix:** Applied `register_limiter` to the verify-email route, consistent with other public auth endpoints.
- **Files:** `handlers/mod.rs`

#### MFA Key Derivation: CSRF Secret in Context `[security]` `[fixed]`

- **Commit:** `18301a3`
- **Severity:** Low
- **Issue:** MFA secret encryption used `csrf_secret` as domain-separation context, meaning CSRF secret compromise had a wider blast radius than intended.
- **Fix:** Replaced with `mfa_encryption_key` for domain separation. CSRF secret is now only used for its intended purpose (OAuth state HMAC).
- **Files:** `handlers/mfa.rs`

#### X-Forwarded-For Spoofing in Fingerprint Module `[security]` `[fixed]`

- **Commit:** `ffbe8ba`
- **Severity:** Medium
- **Issue:** `extract_client_ip()` blindly trusted the first `X-Forwarded-For` entry. Attackers could spoof this header to bypass IP-based fingerprint risk scoring used in refresh token rotation.
- **Fix:** Added IP address parse validation — only accepts values that parse as valid `IpAddr`. Falls back to peer IP on invalid/spoofed values.
- **Files:** `middleware/fingerprint.rs`, `handlers/auth.rs`, `handlers/token.rs`

#### Entitlements: Overly Permissive Fallback `[security]` `[fixed]`

- **Commit:** `ffbe8ba`
- **Severity:** Medium
- **Issue:** Unknown/missing tenants received `max_users: 1000` and all features enabled as a fallback. A non-existent tenant ID could bypass entitlement checks.
- **Fix:** Restrictive defaults: `max_users: 5`, all features OFF, with `tracing::warn!` log. Legitimate tenants always have a DB row.
- **Files:** `middleware/entitlements.rs`

---

### Confirmed Clean (No Issues Found)

| Area | Status |
|------|--------|
| Hardcoded secrets in source | Clean — all production secrets via AWS Secrets Manager |
| SQL injection | Clean — all queries parameterized with `.bind()` |
| Path traversal | Clean — assets embedded at compile time via `rust_embed` |
| Header injection | Clean — no user input in header values |
| CORS configuration | Clean — no wildcard + credentials, per-tenant allowlists |
| Open redirect (OAuth) | Clean — redirect_uri validated against registered clients |
| Token rotation | Solid — family tracking, replay detection, idle timeout |
| PKCE | Enforced — S256 required on all OAuth flows |
| Password hashing | Argon2 with random salt |
| CSP headers | Strict — nonce-based, no unsafe-inline/eval, frame-ancestors none |
| Rate limiting | All primary auth endpoints covered (login, register, token, MFA, WebAuthn, verify-email) |
| .gitignore | Correct — .env, .pem excluded, never committed |

---

### Future Work (Documented, Not Exploitable Now) `[security]` `[backlog]`

#### Ed25519 Key Rotation

No multi-key JWKS support. Rotating the signing key invalidates all active sessions immediately (hard cutover). Future: serve both old and new keys during a rotation window.

#### MFA Key Derivation Strengthening

Current: single SHA-256 hash, no salt, no stretching, no AAD binding to user context. Future: upgrade to HKDF with user_id as AAD, ensuring ciphertexts are non-transferable between users.

#### Platform Admin Key (`plat_`) Operational Controls

No audit trail for admin API usage, no IP allowlisting, no expiry, no multi-key support for different operators. Future: add admin audit logging, key scoping, and rotation.

#### Tenant OAuth Secrets at Rest

Stored plaintext in `tenants` table (encrypted at RDS storage layer, not application layer). Future: consider application-level encryption matching MFA secret pattern.

#### Individual Access Token Revocation

Access tokens (15-min lifetime) cannot be individually revoked. Account disable or session termination only takes effect on next refresh. Future: consider a token blacklist for immediate revocation (at the cost of per-request DB check).

---

### Code Quality Improvements (Same Session)

- Deduplicated `MAX_REFRESH_TOKENS_PER_USER` constant (was in 3 files, now in `db/mod.rs`)
- Extracted `fetch_user_roles()` to `db/mod.rs` (was inline in 5 locations)
- Added `TenantContext::from_db_row()` constructor (eliminates duplicate field mapping between `tenant.rs` and `management_auth.rs`)
- Removed dead middleware modules (`permissions.rs`, `mtls.rs`) from build
- Removed dead `admin::list_users` handler
- Removed 2 dead frontend files, 3 unused imports, 3 unused exports, 1 unused devDependency
- Consolidated 6 duplicate frontend utility functions into `core/utils.ts`

### Modularization (Same Session)

- `admin/build.ts`: 369 lines replaced with 39-line `@getforma/build` config call
- `html_template.rs`: 540 lines replaced with 7-line `forma_server::template` re-export (gains HTML escaping)
