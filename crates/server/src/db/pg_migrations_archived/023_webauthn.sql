-- 023_webauthn.sql

-- WebAuthn credentials (passkeys)
CREATE TABLE IF NOT EXISTS webauthn_credentials (
    id TEXT PRIMARY KEY,
    credential_id BYTEA NOT NULL,
    public_key BYTEA NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    is_synced BOOLEAN NOT NULL DEFAULT FALSE,
    transports TEXT[] NOT NULL DEFAULT '{}',
    name TEXT NOT NULL DEFAULT 'Passkey',
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    aaguid TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    UNIQUE(tenant_id, credential_id)
);

CREATE INDEX IF NOT EXISTS idx_webauthn_user ON webauthn_credentials(user_id, tenant_id);

-- WebAuthn challenge state (short-lived, for registration/login ceremonies)
CREATE TABLE IF NOT EXISTS webauthn_challenges (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    challenge_data JSONB NOT NULL,
    ceremony_type TEXT NOT NULL,  -- 'registration' or 'authentication'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '5 minutes'
);

CREATE INDEX IF NOT EXISTS idx_webauthn_challenges_expiry ON webauthn_challenges(expires_at);

-- FedCM approved clients
CREATE TABLE IF NOT EXISTS fedcm_approved_clients (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    client_id TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, tenant_id, client_id)
);
