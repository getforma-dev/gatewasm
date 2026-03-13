# GateWASM HIPAA Compliance Specification

## Overview

This document maps HIPAA Security Rule Technical Safeguards (45 CFR 164.312) to GateWASM enforcement mechanisms. When `COMPLIANCE_MODE=hipaa` is set, all required controls are automatically enforced. The server refuses to start if any required control cannot be satisfied.

This document serves three audiences:
1. **AI agents** — can read this and correctly configure a HIPAA-compliant deployment
2. **Developers** — can understand what each control means and how to configure it
3. **Auditors** — can trace every HIPAA requirement to its enforcement mechanism and test

## How to Enable

Set `COMPLIANCE_MODE=hipaa` in your tenant settings (dashboard) or environment. The server will:
1. Override any config values that are weaker than HIPAA minimums
2. Log every override it makes
3. Validate all required controls
4. Refuse to start if any required control fails

## Automatic Overrides

When HIPAA mode is active, the server enforces these minimums:

| Setting | Override | Regulation |
|---------|----------|-----------|
| SESSION_TIMEOUT_MINUTES | <= 15 | 164.312(a)(2)(iii) |
| MFA_REQUIRED | true | 164.312(d) |
| JWT_ALGORITHM | != HS256 | 164.312(a)(2)(iv) |
| MAX_FAILED_LOGINS | <= 5 | Best practice |
| AUDIT_LOG_ENABLED | true | 164.312(b) |

---

## Controls

### HIPAA-AC-1: Access Control
**Regulation:** 45 CFR 164.312(a)(1)
**Requirement:** Allow access only to authorized persons
**Classification:** Required

**How GateWASM enforces this:**
- Role-based access control (RBAC) is enabled by default
- Every authenticated request checks the user's roles via JWT claims
- Unauthenticated requests to protected endpoints are denied with 401
- Default deny policy: all protected endpoints require valid JWT

**Configuration:** No action needed. RBAC is always active.

**Verification:**
- Test: `hipaa_ac1_unauthenticated_request_denied`
- Test: `hipaa_ac1_correct_role_allowed`

**Evidence for auditors:** Run compliance report to see RBAC status and test results.

---

### HIPAA-AC-2: Unique User Identification
**Regulation:** 45 CFR 164.312(a)(2)(i)
**Requirement:** Assign a unique name/number for identifying and tracking user identity
**Classification:** Required

**How GateWASM enforces this:**
- Every user gets a UUID on registration (`usr_` prefix + UUIDv4)
- All audit log entries include user_id for tracking
- JWTs include user ID in the `sub` claim

**Configuration:** No action needed. UUIDs are generated automatically.

**Verification:**
- Test: `hipaa_ac2_users_have_unique_ids`
- Test: `hipaa_ac2_audit_entries_include_user_id`

---

### HIPAA-AC-3: Emergency Access Procedure
**Regulation:** 45 CFR 164.312(a)(2)(ii)
**Requirement:** Procedures for obtaining necessary ePHI during an emergency
**Classification:** Required

**How GateWASM enforces this:**
- Management API provides admin access to user data
- Platform admin key (`plat_`) can access any tenant's data
- All emergency access is logged to the audit trail

**Configuration:** Ensure `PLATFORM_ADMIN_KEY` is set and securely stored.

**Verification:**
- Test: `hipaa_ac3_emergency_access_endpoint_exists`

---

### HIPAA-AC-4: Automatic Logoff
**Regulation:** 45 CFR 164.312(a)(2)(iii)
**Requirement:** Terminate session after predetermined time of inactivity
**Classification:** Addressable

**How GateWASM enforces this:**
- Session timeout enforced via `last_activity` tracking on refresh tokens
- HIPAA mode caps timeout at 15 minutes
- Expired sessions return 401 with `session_timeout` error
- Timeout events are logged to the audit trail

**Configuration:** `SESSION_TIMEOUT_MINUTES=15` (auto-set in HIPAA mode)

**Verification:**
- Test: `hipaa_ac4_timeout_cannot_exceed_15_minutes`

---

### HIPAA-AC-5: Encryption and Decryption
**Regulation:** 45 CFR 164.312(a)(2)(iv)
**Requirement:** Mechanism to encrypt and decrypt ePHI
**Classification:** Addressable

**How GateWASM enforces this:**
- JWTs signed with EdDSA (Ed25519) — asymmetric cryptography
- HS256 (symmetric) is rejected in HIPAA mode
- Passwords hashed with Argon2id (memory-hard)
- TLS 1.2+ enforced at the reverse proxy level

**Configuration:**
- `JWT_ALGORITHM=EdDSA` (default, strongest option)
- `TLS_MIN_VERSION=1.2` (configure at reverse proxy)

**Verification:**
- Test: `hipaa_ac5_jwt_not_hs256`
- Test: `hipaa_ac5_passwords_hashed_argon2id`

---

### HIPAA-AUDIT: Audit Controls
**Regulation:** 45 CFR 164.312(b)
**Requirement:** Record and examine activity in information systems
**Classification:** Required

**How GateWASM enforces this:**
- Structured audit log table with immutable (INSERT-only) enforcement
- Every auth event logged: register, login, logout, token refresh, access denied
- Each entry includes: timestamp, event_id, user_id, action, resource, ip_address, result
- Audit log retained for compliance reporting
- No UPDATE or DELETE operations allowed on audit_log table

**Configuration:** `AUDIT_LOG_ENABLED=true` (auto-set in HIPAA mode)

**Verification:**
- Test: `hipaa_audit_all_auth_events_logged`
- Test: `hipaa_audit_entries_have_required_fields`
- Test: `hipaa_audit_login_failure_logged`

**Evidence for auditors:** Query `GET /api/v1/compliance/report` for full audit log summary.

---

### HIPAA-INT: Integrity
**Regulation:** 45 CFR 164.312(c)(1)
**Requirement:** Protect ePHI from improper alteration or destruction
**Classification:** Required

**How GateWASM enforces this:**
- JWT signatures verified on every request (EdDSA)
- Tampered JWTs are rejected with 401
- Refresh tokens rotated on every use (one-time use)
- Token replay detection: reuse of revoked token triggers family-wide revocation
- All token operations use database transactions for atomicity

**Configuration:** No action needed. All integrity mechanisms are built-in.

**Verification:**
- Test: `hipaa_int_tampered_jwt_rejected`
- Test: `hipaa_int_refresh_token_rotated`
- Test: `hipaa_int_replay_detected_family_revoked`

---

### HIPAA-AUTH: Person or Entity Authentication
**Regulation:** 45 CFR 164.312(d)
**Requirement:** Verify that a person seeking access is who they claim to be
**Classification:** Required

**How GateWASM enforces this:**
- Multi-factor authentication required for all users in HIPAA mode
- Password minimum length: 8 characters
- Password complexity validation (uppercase, lowercase, digit, no common passwords)
- Account lockout after 5 failed login attempts (15-minute cooldown)

**Configuration:** `MFA_REQUIRED=true` (auto-set in HIPAA mode)

**Verification:**
- Test: `hipaa_auth_weak_password_rejected`
- Test: `hipaa_auth_identity_verified_before_access`

---

### HIPAA-TX: Transmission Security
**Regulation:** 45 CFR 164.312(e)(1)
**Requirement:** Guard against unauthorized access during transmission
**Classification:** Required

**How GateWASM enforces this:**
- TLS 1.2+ required for all connections (enforced at reverse proxy)
- Cookies set with `Secure` flag (HTTPS only)
- Cookies set with `HttpOnly` flag (no JavaScript access)
- Cookies set with `SameSite=Strict` (no cross-site)

**Configuration:** Configure TLS at your reverse proxy (ALB, nginx, etc.)

**Verification:**
- Test: `hipaa_tx_secure_cookies_set`

---

## For AI Agents

Read `requirements.json` alongside this document. The JSON provides machine-parseable enforcement rules:
- Each control's `enforcement` array tells you exactly what config values must be set
- Each control's `tests` array tells you how to verify
- The `operator` field supports: `eq`, `neq`, `gte`, `lte`, `in`
- The `check` field maps to config keys via `get_config_value()` in `validator.rs`

## Running Compliance Validation

- **Dashboard:** Settings > Compliance > View Status
- **API:** `GET /api/v1/compliance/status` (requires management API key)
- **Report:** `GET /api/v1/compliance/report` (markdown) or `?format=json`
- **Startup:** Server validates automatically and refuses to start if failing
- **Tests:** `cargo test` includes compliance tests

## Generating a Compliance Report

The compliance report includes:
- Executive summary with pass/fail counts
- Per-control details with evidence
- Audit log summary (last 30 days)
- Configuration summary
- Test results

Provide this report to your auditor as evidence of technical safeguard implementation.
