-- Migration 019: Owner dashboard query performance indexes
--
-- Speeds up provider-plane inventory and analytics queries that aggregate by
-- owner_user_id, tenant_id, status windows, and billing dimensions.

CREATE INDEX IF NOT EXISTS idx_tenants_owner_active_inventory
    ON tenants (owner_user_id, created_at DESC)
    WHERE id != 'ten_platform' AND status != 'deleted';

CREATE INDEX IF NOT EXISTS idx_tenants_owner_plan_status
    ON tenants (owner_user_id, COALESCE(plan, 'trial'), status, trial_ends_at);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_tenant_active_window
    ON refresh_tokens (tenant_id, revoked, expires_at DESC);

CREATE INDEX IF NOT EXISTS idx_login_attempts_tenant_success_window
    ON login_attempts (tenant_id, success, created_at DESC, user_id);

CREATE INDEX IF NOT EXISTS idx_provider_billing_customer_status_window
    ON provider_billing_events (customer_user_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_provider_billing_tenant_status_window
    ON provider_billing_events (tenant_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_platform_accounts_status_created
    ON platform_accounts (status, created_at DESC);
