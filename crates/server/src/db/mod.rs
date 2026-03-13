use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let max_conns: u32 = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let pool = PgPoolOptions::new()
        .max_connections(max_conns)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;

    Ok(pool)
}

/// Run migrations on an existing pool (public for test use).
pub async fn run_migrations_on_pool(pool: &PgPool) -> Result<(), sqlx::Error> {
    run_migrations(pool).await
}

async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Create migration tracking table
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INT PRIMARY KEY, applied_at TIMESTAMPTZ DEFAULT NOW())"
    ).execute(pool).await?;

    let migrations: &[(i32, &str, &str)] = &[
        (
            1,
            "initial schema",
            include_str!("pg_migrations/001_initial.sql"),
        ),
        (
            2,
            "hmac + email verification",
            include_str!("pg_migrations/002_add_hmac_and_verification.sql"),
        ),
        (
            3,
            "hardening",
            include_str!("pg_migrations/003_hardening.sql"),
        ),
        (
            4,
            "multi-tenancy",
            include_str!("pg_migrations/004_multi_tenancy.sql"),
        ),
        (
            5,
            "compliance engine",
            include_str!("pg_migrations/005_compliance.sql"),
        ),
        (
            6,
            "hosted login branding",
            include_str!("pg_migrations/006_hosted_login.sql"),
        ),
        (
            7,
            "self-serve onboarding",
            include_str!("pg_migrations/007_self_serve.sql"),
        ),
        (
            8,
            "custom roles",
            include_str!("pg_migrations/008_custom_roles.sql"),
        ),
        (
            9,
            "webhooks",
            include_str!("pg_migrations/009_webhooks.sql"),
        ),
        (
            10,
            "login redirect defaults",
            include_str!("pg_migrations/010_login_redirect_defaults.sql"),
        ),
        (
            11,
            "entitlements + org model",
            include_str!("pg_migrations/011_entitlements_and_orgs.sql"),
        ),
        (
            12,
            "owner plane",
            include_str!("pg_migrations/012_owner_plane.sql"),
        ),
        (
            13,
            "redirect normalization",
            include_str!("pg_migrations/013_redirect_normalization.sql"),
        ),
        (
            14,
            "owner invites",
            include_str!("pg_migrations/014_owner_invites.sql"),
        ),
        (
            15,
            "refresh token auth context",
            include_str!("pg_migrations/015_refresh_token_auth_context.sql"),
        ),
        (
            16,
            "owner dashboard analytics + billing",
            include_str!("pg_migrations/016_owner_dashboard_analytics_and_billing.sql"),
        ),
        (
            17,
            "mfa lockout + secret hardening",
            include_str!("pg_migrations/017_mfa_lockout_and_secret_hardening.sql"),
        ),
        (
            18,
            "owner timeline events",
            include_str!("pg_migrations/018_owner_timeline_events.sql"),
        ),
        (
            19,
            "owner query performance indexes",
            include_str!("pg_migrations/019_owner_query_performance_indexes.sql"),
        ),
        (
            20,
            "tenant signup mode + invites",
            include_str!("pg_migrations/020_tenant_signup_mode_and_invites.sql"),
        ),
        (
            21,
            "fix platform login redirect",
            include_str!("pg_migrations/021_fix_platform_login_redirect.sql"),
        ),
        (
            22,
            "security hardening",
            include_str!("pg_migrations/022_security_hardening.sql"),
        ),
        (
            23,
            "webauthn + fedcm",
            include_str!("pg_migrations/023_webauthn.sql"),
        ),
        (
            24,
            "enterprise security (PAR, mTLS certs, DPoP jti)",
            include_str!("pg_migrations/024_enterprise_security.sql"),
        ),
        (
            25,
            "SSF streams + outbox + geo columns",
            include_str!("pg_migrations/025_ssf.sql"),
        ),
    ];

    for &(version, label, sql) in migrations {
        let already_applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1)")
                .bind(version)
                .fetch_one(pool)
                .await?;

        if already_applied {
            continue;
        }

        tracing::info!(version, label, "Running migration");

        // Execute each statement individually for reliable error reporting
        for stmt in sql.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            if let Err(e) = sqlx::raw_sql(stmt).execute(pool).await {
                // Allow "already exists" errors for idempotent DDL (IF NOT EXISTS
                // handles most cases, but some ALTER TABLE statements may hit this)
                let msg = e.to_string();
                if msg.contains("already exists") || msg.contains("duplicate key") {
                    tracing::debug!(version, error = %e, "Skipping already-applied DDL statement");
                    continue;
                }
                tracing::error!(version, label, error = %e, "Migration failed");
                return Err(e);
            }
        }

        sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1)")
            .bind(version)
            .execute(pool)
            .await?;

        tracing::info!(version, label, "Migration applied successfully");
    }

    Ok(())
}

/// Clean up expired/stale records on startup.
pub async fn cleanup_expired(pool: &PgPool) {
    let now = chrono::Utc::now();
    let seven_days_ago = chrono::Utc::now() - chrono::Duration::days(7);

    match sqlx::query("DELETE FROM oauth_states WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(count = r.rows_affected(), "Cleaned expired oauth_states");
        }
        Err(e) => tracing::warn!(error = %e, "Failed to clean oauth_states"),
        _ => {}
    }

    match sqlx::query("DELETE FROM auth_codes WHERE expires_at < $1 OR used = TRUE")
        .bind(now)
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(count = r.rows_affected(), "Cleaned expired/used auth_codes");
        }
        Err(e) => tracing::warn!(error = %e, "Failed to clean auth_codes"),
        _ => {}
    }

    match sqlx::query("DELETE FROM refresh_tokens WHERE revoked = TRUE AND created_at < $1")
        .bind(seven_days_ago)
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(
                count = r.rows_affected(),
                "Cleaned old revoked refresh_tokens"
            );
        }
        Err(e) => tracing::warn!(error = %e, "Failed to clean refresh_tokens"),
        _ => {}
    }

    let ninety_days_ago = chrono::Utc::now() - chrono::Duration::days(90);
    match sqlx::query("DELETE FROM login_attempts WHERE created_at < $1")
        .bind(ninety_days_ago)
        .execute(pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(count = r.rows_affected(), "Cleaned old login_attempts");
        }
        Err(e) => tracing::warn!(error = %e, "Failed to clean login_attempts"),
        _ => {}
    }

    match sqlx::query(
        "UPDATE owner_invites
         SET status = 'expired', updated_at = NOW()
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(count = r.rows_affected(), "Expired stale owner invites");
        }
        Err(e) => tracing::warn!(error = %e, "Failed to expire owner_invites"),
        _ => {}
    }

    match sqlx::query(
        "UPDATE tenant_user_invites
         SET status = 'expired', updated_at = NOW()
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(
                count = r.rows_affected(),
                "Expired stale tenant_user_invites"
            );
        }
        Err(e) => tracing::warn!(error = %e, "Failed to expire tenant_user_invites"),
        _ => {}
    }
}

/// Log a login attempt (success or failure) for audit tracking.
pub async fn log_login_attempt(
    pool: &PgPool,
    email: &str,
    user_id: Option<&str>,
    success: bool,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    failure_reason: Option<&str>,
) {
    let id = format!("la_{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO login_attempts (id, email, user_id, success, ip_address, user_agent, failure_reason) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(email)
    .bind(user_id)
    .bind(success)
    .bind(ip_address)
    .bind(user_agent)
    .bind(failure_reason)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "Failed to log login attempt");
    }
}

/// Enforce a maximum number of active refresh tokens per user.
/// Revokes the oldest tokens if the limit is exceeded.
pub async fn enforce_refresh_token_limit(pool: &PgPool, user_id: &str, max_tokens: i64) {
    let result = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE id IN (
            SELECT id FROM refresh_tokens WHERE user_id = $1 AND revoked = FALSE
            ORDER BY created_at ASC
            LIMIT GREATEST(0, (SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1 AND revoked = FALSE) - $2)
        )",
    )
    .bind(user_id)
    .bind(max_tokens)
    .execute(pool)
    .await;

    if let Ok(r) = result {
        if r.rows_affected() > 0 {
            tracing::info!(
                user_id = %user_id,
                revoked = r.rows_affected(),
                "Enforced refresh token limit"
            );
        }
    }
}
