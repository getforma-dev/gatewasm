-- 024_enterprise_security.sql

-- PAR (Pushed Authorization Requests)
CREATE TABLE IF NOT EXISTS par_requests (
    request_uri TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    parameters JSONB NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_par_expiry ON par_requests(expires_at) WHERE consumed = FALSE;

-- Client certificates (mTLS)
CREATE TABLE IF NOT EXISTS client_certificates (
    id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    client_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL DEFAULT 'Certificate',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    UNIQUE(tenant_id, fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_client_certs_lookup ON client_certificates(tenant_id, client_id)
  WHERE revoked_at IS NULL;

-- DPoP jti replay cache (in-memory primary, DB as fallback for multi-instance)
CREATE TABLE IF NOT EXISTS dpop_jti_cache (
    jti TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dpop_jti_expiry ON dpop_jti_cache(expires_at);
