CREATE TABLE IF NOT EXISTS hosted_authorization_requests (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    client_state TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    browser_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS hosted_authorization_expiry ON hosted_authorization_requests(expires_at);
