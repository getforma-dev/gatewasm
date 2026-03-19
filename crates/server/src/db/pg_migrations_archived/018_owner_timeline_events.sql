-- Owner timeline events used by provider control-plane Customer 360 and timeline tab.
CREATE TABLE IF NOT EXISTS owner_timeline_events (
    id TEXT PRIMARY KEY,
    customer_user_id TEXT,
    tenant_id TEXT,
    actor_user_id TEXT,
    actor_email TEXT,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    request_id TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_owner_timeline_customer
    ON owner_timeline_events (customer_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_owner_timeline_tenant
    ON owner_timeline_events (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_owner_timeline_actor
    ON owner_timeline_events (actor_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_owner_timeline_action
    ON owner_timeline_events (action, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_owner_timeline_request
    ON owner_timeline_events (request_id);
