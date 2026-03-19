-- Migration 009: Webhooks + delivery tracking

CREATE TABLE IF NOT EXISTS webhooks (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    events JSONB NOT NULL DEFAULT '[]'::jsonb,
    active BOOLEAN DEFAULT TRUE,
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhooks_tenant ON webhooks(tenant_id);
CREATE INDEX IF NOT EXISTS idx_webhooks_tenant_active ON webhooks(tenant_id, active);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    response_status INTEGER,
    response_body TEXT,
    attempt INTEGER DEFAULT 1,
    max_attempts INTEGER DEFAULT 3,
    next_retry_at TIMESTAMPTZ,
    status TEXT DEFAULT 'pending',
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_whd_webhook ON webhook_deliveries(webhook_id);
CREATE INDEX IF NOT EXISTS idx_whd_status ON webhook_deliveries(status, next_retry_at);
CREATE INDEX IF NOT EXISTS idx_whd_webhook_created ON webhook_deliveries(webhook_id, created_at DESC);
