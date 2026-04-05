# Session Security Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden session lifecycle with periodic expired token cleanup, a configurable per-tenant session cap (replacing the hardcoded global 10), and session revocation on security-sensitive events (role changes).

**Architecture:** Three independent backend changes: (1) add a periodic background job to purge expired refresh tokens, (2) add `max_sessions_per_user` to `TenantSecurityConfig` and thread it through all token creation paths, (3) revoke sessions when user roles are modified. All changes are in the Rust/axum backend with no frontend changes needed.

**Tech Stack:** Rust, axum, sqlx, PostgreSQL, tokio

---

## Current State

- `db::enforce_refresh_token_limit()` already exists in `crates/server/src/db/mod.rs:245-267` — revokes oldest tokens when count exceeds a limit. Called from every token creation path.
- The limit is hardcoded: `MAX_REFRESH_TOKENS_PER_USER = 10` at `db/mod.rs:5`.
- `cleanup_expired()` in `db/mod.rs:99-187` runs once at startup. It only deletes **revoked** tokens older than 7 days. Expired but unrevoked tokens are never deleted.
- The background loop in `main.rs:185-198` runs every 30s but never touches `refresh_tokens`.
- Password reset already revokes all sessions (`password_reset.rs:166-171`).
- `block_user` already revokes all sessions (`management.rs:809-814`).
- Role assignment/removal in `management.rs` does NOT revoke sessions.

---

### Task 1: Add periodic cleanup of expired refresh tokens

**Files:**
- Modify: `crates/server/src/db/mod.rs:99-140` (add expired token cleanup to `cleanup_expired`)
- Modify: `crates/server/src/main.rs:192-196` (add periodic call in background loop)

This adds cleanup of expired-but-unrevoked tokens to prevent table bloat. Two insertion points: the startup cleanup function (immediate) and the background loop (periodic, every ~5 minutes).

- [ ] **Step 1: Add expired token deletion to `cleanup_expired` in db/mod.rs**

After the existing revoked-token cleanup block (after line 140), add a new block:

```rust
    // Clean expired (but never revoked) refresh tokens older than 24 hours past expiry
    let day_ago = chrono::Utc::now() - chrono::Duration::days(1);
    match sqlx::query("DELETE FROM refresh_tokens WHERE revoked = FALSE AND expires_at < $1")
        .bind(day_ago)
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(
                count = r.rows_affected(),
                "Cleaned expired (unrevoked) refresh_tokens"
            );
        }
        Err(e) => tracing::warn!(error = %e, "Failed to clean expired refresh_tokens"),
        _ => {}
    }
```

- [ ] **Step 2: Create a public periodic cleanup function in db/mod.rs**

Add after the `enforce_refresh_token_limit` function (after line 267):

```rust
/// Periodic cleanup of stale refresh tokens. Called from the background loop.
pub async fn cleanup_stale_refresh_tokens(pool: &PgPool) {
    let day_ago = chrono::Utc::now() - chrono::Duration::days(1);

    // Delete revoked tokens older than 7 days
    let revoked = sqlx::query(
        "DELETE FROM refresh_tokens WHERE revoked = TRUE AND created_at < $1",
    )
    .bind(chrono::Utc::now() - chrono::Duration::days(7))
    .execute(pool)
    .await;

    if let Ok(r) = &revoked {
        if r.rows_affected() > 0 {
            tracing::debug!(count = r.rows_affected(), "Cleaned old revoked refresh_tokens");
        }
    }

    // Delete expired (unrevoked) tokens past 24h grace period
    let expired = sqlx::query(
        "DELETE FROM refresh_tokens WHERE revoked = FALSE AND expires_at < $1",
    )
    .bind(day_ago)
    .execute(pool)
    .await;

    if let Ok(r) = &expired {
        if r.rows_affected() > 0 {
            tracing::debug!(count = r.rows_affected(), "Cleaned expired refresh_tokens");
        }
    }
}
```

- [ ] **Step 3: Add periodic call in the background loop in main.rs**

In `main.rs`, inside the `if tick % 10 == 0` block (line 192-196), add after `bg_tenant_cache.evict_expired();`:

```rust
                db::cleanup_stale_refresh_tokens(&retry_pool).await;
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/db/mod.rs crates/server/src/main.rs
git commit -m "feat: add periodic cleanup of expired refresh tokens

Expired-but-unrevoked tokens are now deleted after a 24h grace period,
both at startup and every ~5 minutes in the background loop. Revoked
tokens continue to be cleaned after 7 days."
```

---

### Task 2: Make session cap configurable per tenant

**Files:**
- Modify: `crates/server/src/security_config.rs` (add `max_sessions_per_user` field)
- Modify: `crates/server/src/handlers/auth.rs` (use tenant config instead of global constant)
- Modify: `crates/server/src/handlers/token.rs` (use tenant config instead of global constant)
- Modify: `crates/server/src/handlers/webauthn.rs` (use tenant config instead of global constant)

The `enforce_refresh_token_limit` function already accepts a `max_tokens` parameter. The change is to source that value from `TenantSecurityConfig` instead of the hardcoded `MAX_REFRESH_TOKENS_PER_USER = 10`.

- [ ] **Step 1: Add `max_sessions_per_user` to `TenantSecurityConfig`**

In `crates/server/src/security_config.rs`, add to the Foundation section (after line 15, the `client_fingerprint` field):

```rust
    #[serde(default = "default_max_sessions")]
    pub max_sessions_per_user: u32,
```

Add the default function (after `default_idle_timeout`, around line 48):

```rust
fn default_max_sessions() -> u32 {
    10
}
```

Update the `Default` impl (add after `client_fingerprint: true,` in the Default block):

```rust
            max_sessions_per_user: 10,
```

- [ ] **Step 2: Update `auth.rs` login handler to use tenant config**

In `crates/server/src/handlers/auth.rs`, the `login` function creates a refresh token and calls `enforce_refresh_token_limit`. Find line 1392:

```rust
    db::enforce_refresh_token_limit(&state.db, &user_id, db::MAX_REFRESH_TOKENS_PER_USER).await;
```

The tenant context is available in this function as `tenant` (a `TenantContext`). Replace that line with:

```rust
    db::enforce_refresh_token_limit(&state.db, &user_id, tenant.security_config.max_sessions_per_user as i64).await;
```

Also find the other call site in the same file (around line 786, in the `register` function). Replace the same pattern:

```rust
    db::enforce_refresh_token_limit(&state.db, &user_id, tenant.security_config.max_sessions_per_user as i64).await;
```

- [ ] **Step 3: Update `token.rs` to use tenant config**

In `crates/server/src/handlers/token.rs`, find the two calls to `enforce_refresh_token_limit`:

Line 286 (in `handle_auth_code`): The tenant context is available as `tenant`. Replace:
```rust
    db::enforce_refresh_token_limit(&state.db, &user_id, db::MAX_REFRESH_TOKENS_PER_USER).await;
```
With:
```rust
    db::enforce_refresh_token_limit(&state.db, &user_id, tenant.security_config.max_sessions_per_user as i64).await;
```

Line 727 (in `handle_refresh`): Same replacement. The `tenant` variable is available in this function.

- [ ] **Step 4: Update `webauthn.rs` to use tenant config**

In `crates/server/src/handlers/webauthn.rs`, find the call around line 1344:

```rust
    crate::db::enforce_refresh_token_limit(&state.db, &db_user_id, crate::db::MAX_REFRESH_TOKENS_PER_USER)
```

The tenant context is available in this function. Replace with:

```rust
    crate::db::enforce_refresh_token_limit(&state.db, &db_user_id, tenant.security_config.max_sessions_per_user as i64)
```

Note: Read the surrounding code to find the correct variable name for the tenant context — it may be `tenant` or `tenant_ctx`. Use whatever is in scope.

- [ ] **Step 5: Add unit test for the new config field**

In `crates/server/src/security_config.rs`, add to the existing test module:

```rust
    #[test]
    fn default_max_sessions_is_ten() {
        let config = TenantSecurityConfig::default();
        assert_eq!(config.max_sessions_per_user, 10);
    }

    #[test]
    fn from_json_custom_max_sessions() {
        let val = serde_json::json!({ "max_sessions_per_user": 5 });
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.max_sessions_per_user, 5);
    }

    #[test]
    fn from_json_missing_max_sessions_uses_default() {
        let val = serde_json::json!({});
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.max_sessions_per_user, 10);
    }
```

- [ ] **Step 6: Build and run tests**

Run: `cargo build 2>&1 | tail -5`
Run: `cargo test --lib security_config 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/security_config.rs crates/server/src/handlers/auth.rs crates/server/src/handlers/token.rs crates/server/src/handlers/webauthn.rs
git commit -m "feat: make session cap configurable per tenant via max_sessions_per_user

Replaces hardcoded MAX_REFRESH_TOKENS_PER_USER=10 with a per-tenant
setting in TenantSecurityConfig. Default remains 10. Operators can
lower it (e.g. 3 for high-security tenants) via the security config API."
```

---

### Task 3: Revoke sessions on role changes

**Files:**
- Modify: `crates/server/src/handlers/management.rs` (add session revocation to role assignment/removal endpoints)

When a user's roles change, their existing JWTs contain stale role claims. Revoking sessions forces re-authentication, which issues fresh tokens with correct roles. This follows the same pattern already used by `block_user` in the same file.

- [ ] **Step 1: Find the role assignment endpoint**

Search `crates/server/src/handlers/management.rs` for the function that assigns roles to users. Look for `INSERT INTO user_roles` or a function name like `assign_role` / `add_role` / `set_roles`.

Read the function to understand:
- What parameters it receives (user_id, tenant_id from TenantContext)
- Where the SQL INSERT/UPDATE happens
- Where the Ok(Json(...)) response is returned

- [ ] **Step 2: Add session revocation after role assignment**

After the role is assigned (after the INSERT query succeeds) and before the response is returned, add:

```rust
    // Revoke sessions so the user gets fresh tokens with updated role claims
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE")
        .bind(&user_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await
        .ok();
```

This is the exact same pattern used by `block_user` at lines 809-814 of the same file.

- [ ] **Step 3: Find the role removal endpoint**

Search for the function that removes roles from users. Look for `DELETE FROM user_roles` or function name like `remove_role` / `unassign_role`.

- [ ] **Step 4: Add session revocation after role removal**

Same pattern as Step 2 — add the revocation query after the DELETE succeeds:

```rust
    // Revoke sessions so the user gets fresh tokens with updated role claims
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE")
        .bind(&user_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await
        .ok();
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/handlers/management.rs
git commit -m "feat: revoke sessions on role assignment/removal

Forces re-authentication when roles change so users get fresh tokens
with correct role claims. Follows the same pattern as block_user."
```

---

### Task 4: Update existing tests for security config serialization roundtrip

**Files:**
- Modify: `crates/server/src/security_config.rs` (update `serialization_roundtrip` test)

The existing roundtrip test should verify the new field survives serialization.

- [ ] **Step 1: Update the serialization roundtrip test**

In the `serialization_roundtrip` test (around line 261), add the new field to the test config and assertion:

```rust
    #[test]
    fn serialization_roundtrip() {
        let config = TenantSecurityConfig {
            idle_timeout_minutes: 15,
            max_sessions_per_user: 5,
            webauthn_enabled: true,
            dpop_required: true,
            ..Default::default()
        };
        let json = serde_json::to_value(&config).unwrap();
        let parsed = TenantSecurityConfig::from_json(&json);
        assert_eq!(parsed.idle_timeout_minutes, 15);
        assert_eq!(parsed.max_sessions_per_user, 5);
        assert!(parsed.webauthn_enabled);
        assert!(parsed.dpop_required);
        assert!(parsed.host_prefix_cookies); // default preserved
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib security_config 2>&1 | tail -10`
Expected: All tests pass (including the 3 new ones from Task 2 + this update).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/security_config.rs
git commit -m "test: verify max_sessions_per_user in serialization roundtrip"
```
