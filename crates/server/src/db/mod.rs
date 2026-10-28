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

    // Single squashed migration — no public. qualifiers, works in any search_path.
    let migrations: &[(i32, &str, &str)] = &[
        (
            1,
            "full schema (squashed)",
            include_str!("pg_migrations/001_schema.sql"),
        ),
        (
            2,
            "onboarding wizard columns + mfa_policy",
            include_str!("pg_migrations/002_onboarding_wizard.sql"),
        ),
        (
            3,
            "branding bg_color + card_color",
            include_str!("pg_migrations/003_branding_colors.sql"),
        ),
        (
            4,
            "gold branding default for new tenants",
            include_str!("pg_migrations/004_gold_branding_default.sql"),
        ),
        (5, "hosted password authorization", include_str!("pg_migrations/005_hosted_authorization.sql")),
        (6, "KMS keyring versions", include_str!("pg_migrations/006_keyring_versions.sql")),
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
    tenant_id: Option<&str>,
) {
    let id = format!("la_{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO login_attempts (id, email, user_id, success, ip_address, user_agent, failure_reason, tenant_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(email)
    .bind(user_id)
    .bind(success)
    .bind(ip_address)
    .bind(user_agent)
    .bind(failure_reason)
    .bind(tenant_id)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "Failed to log login attempt");
    }
}

/// Fetch the role names for a user, falling back to `["user"]` on error.
/// When `tenant_id` is provided, only returns global roles and roles scoped to that tenant.
pub async fn fetch_user_roles(pool: &PgPool, user_id: &str, tenant_id: Option<&str>) -> Vec<String> {
    if let Some(tid) = tenant_id {
        sqlx::query_scalar(
            "SELECT r.name FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1 AND (r.tenant_id IS NULL OR r.tenant_id = $2)",
        )
        .bind(user_id)
        .bind(tid)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|_| vec!["user".to_string()])
    } else {
        sqlx::query_scalar(
            "SELECT r.name FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|_| vec!["user".to_string()])
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
