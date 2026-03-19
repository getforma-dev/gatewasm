-- Migration 020: tenant signup mode + tenant user invites + OAuth invite passthrough

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS signup_mode TEXT NOT NULL DEFAULT 'open';
UPDATE tenants SET signup_mode = 'open' WHERE signup_mode IS NULL;

CREATE TABLE IF NOT EXISTS tenant_user_invites (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    role_id TEXT NOT NULL REFERENCES roles(id),
    token_hash TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'pending',
    invited_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    accepted_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenant_user_invites_tenant_status
    ON tenant_user_invites (tenant_id, status, expires_at DESC);

CREATE INDEX IF NOT EXISTS idx_tenant_user_invites_tenant_email
    ON tenant_user_invites (tenant_id, lower(email));

CREATE INDEX IF NOT EXISTS idx_tenant_user_invites_token_hash
    ON tenant_user_invites (token_hash);

ALTER TABLE oauth_states ADD COLUMN IF NOT EXISTS invite_token_hash TEXT;
