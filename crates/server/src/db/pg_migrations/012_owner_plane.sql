-- Migration 012: Provider owner plane
--
-- Adds owner metadata for platform accounts and seeds the platform_owner role.

CREATE TABLE IF NOT EXISTS platform_accounts (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'active',
    company_name TEXT,
    billing_ref TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_platform_accounts_status ON platform_accounts(status);

INSERT INTO roles (id, name, permissions, description, permissions_jsonb, tenant_id, is_system, created_at)
VALUES (
    'role_platform_owner',
    'platform_owner',
    '["platform:owner"]',
    'Provider control-plane owner role',
    '["platform:owner"]'::jsonb,
    NULL,
    TRUE,
    NOW()
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO platform_accounts (user_id, status, created_at, updated_at)
SELECT u.id, 'active', NOW(), NOW()
FROM users u
WHERE u.tenant_id = 'ten_platform'
ON CONFLICT (user_id) DO NOTHING;
