-- Migration 016: Provider owner dashboard analytics + billing model
--
-- Adds customer segmentation/lifecycle fields and a billing events table used by
-- the provider control-plane dashboard.

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS category TEXT NOT NULL DEFAULT 'standard';

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS notes TEXT;

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS risk_level TEXT NOT NULL DEFAULT 'normal';

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS lifecycle_stage TEXT NOT NULL DEFAULT 'active';

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS blocked_reason TEXT;

ALTER TABLE platform_accounts
    ADD COLUMN IF NOT EXISTS last_contacted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_platform_accounts_category
    ON platform_accounts (category);

CREATE INDEX IF NOT EXISTS idx_platform_accounts_lifecycle_stage
    ON platform_accounts (lifecycle_stage);

CREATE TABLE IF NOT EXISTS provider_billing_events (
    id TEXT PRIMARY KEY,
    customer_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    amount_cents BIGINT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'usd',
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'paid', 'failed', 'refunded')),
    external_invoice_id TEXT,
    receipt_url TEXT,
    period_start TIMESTAMPTZ,
    period_end TIMESTAMPTZ,
    notes TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by_user_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_provider_billing_customer
    ON provider_billing_events (customer_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_provider_billing_status
    ON provider_billing_events (status, created_at DESC);

