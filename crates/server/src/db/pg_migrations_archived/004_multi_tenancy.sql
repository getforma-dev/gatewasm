-- Multi-tenancy: tenants table + tenant_id on all existing tables

CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    owner_email TEXT NOT NULL,
    api_key_hash TEXT,
    api_key_prefix TEXT,
    allowed_providers JSONB DEFAULT '["email"]'::jsonb,
    google_client_id TEXT,
    google_client_secret TEXT,
    github_client_id TEXT,
    github_client_secret TEXT,
    custom_jwt_issuer TEXT,
    allowed_origins JSONB DEFAULT '[]'::jsonb,
    max_users INTEGER DEFAULT 1000,
    status TEXT DEFAULT 'active',
    mau_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenants_slug ON tenants(slug);
CREATE INDEX IF NOT EXISTS idx_tenants_api_key_prefix ON tenants(api_key_prefix);

-- Add tenant_id to all existing tables
ALTER TABLE users ADD COLUMN IF NOT EXISTS tenant_id TEXT REFERENCES tenants(id);
ALTER TABLE credentials ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE oauth_clients ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE oauth_states ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE auth_codes ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE login_attempts ADD COLUMN IF NOT EXISTS tenant_id TEXT;

-- Composite indexes for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_users_tenant ON users(tenant_id);
CREATE INDEX IF NOT EXISTS idx_users_tenant_email ON users(tenant_id, email);
CREATE INDEX IF NOT EXISTS idx_credentials_tenant ON credentials(tenant_id, user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_tenant ON refresh_tokens(tenant_id, user_id);
CREATE INDEX IF NOT EXISTS idx_login_attempts_tenant ON login_attempts(tenant_id, email);
CREATE INDEX IF NOT EXISTS idx_oauth_states_tenant ON oauth_states(tenant_id);
CREATE INDEX IF NOT EXISTS idx_auth_codes_tenant ON auth_codes(tenant_id);

-- Drop global unique constraints that must be tenant-scoped
-- "users_email_key" from migration 001: email TEXT UNIQUE
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_email_key;
-- "credentials_provider_provider_user_id_key" from migration 001: UNIQUE(provider, provider_user_id)
ALTER TABLE credentials DROP CONSTRAINT IF EXISTS credentials_provider_provider_user_id_key;

-- Tenant-scoped unique constraints
-- (email must be unique per tenant, not globally)
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_tenant_email_unique ON users(tenant_id, email);
-- (provider+provider_user_id must be unique per tenant, not globally)
CREATE UNIQUE INDEX IF NOT EXISTS idx_credentials_tenant_provider_unique ON credentials(tenant_id, provider, provider_user_id);
