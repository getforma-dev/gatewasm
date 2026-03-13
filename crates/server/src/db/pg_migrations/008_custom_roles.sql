-- Migration 008: Tenant-scoped custom roles with permissions

-- Add new columns to roles table
ALTER TABLE roles ADD COLUMN IF NOT EXISTS tenant_id TEXT;
ALTER TABLE roles ADD COLUMN IF NOT EXISTS is_system BOOLEAN DEFAULT FALSE;
ALTER TABLE roles ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE roles ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ DEFAULT NOW();

-- Make original permissions TEXT column nullable / defaulted (new rows use permissions_jsonb)
ALTER TABLE roles ALTER COLUMN permissions SET DEFAULT '[]';

-- Add JSONB permissions column (existing 'permissions' TEXT column has JSON-like data: '["read"]')
ALTER TABLE roles ADD COLUMN IF NOT EXISTS permissions_jsonb JSONB DEFAULT '[]'::jsonb;

-- Migrate existing TEXT permissions to JSONB
UPDATE roles SET permissions_jsonb = permissions::jsonb
    WHERE permissions IS NOT NULL AND permissions != '' AND permissions_jsonb = '[]'::jsonb;

-- Mark seed roles as system roles
UPDATE roles SET is_system = TRUE WHERE id IN ('role_user', 'role_admin');

-- Drop the global UNIQUE constraint on name (must become tenant-scoped)
ALTER TABLE roles DROP CONSTRAINT IF EXISTS roles_name_key;

-- Tenant-scoped unique: same role name allowed in different tenants.
-- System roles (tenant_id IS NULL) use sentinel '__system__' to stay globally unique.
CREATE UNIQUE INDEX IF NOT EXISTS idx_roles_tenant_name
    ON roles(COALESCE(tenant_id, '__system__'), name);

-- Fast lookup by tenant
CREATE INDEX IF NOT EXISTS idx_roles_tenant ON roles(tenant_id);

-- Add tenant_id to user_roles for scoped queries
ALTER TABLE user_roles ADD COLUMN IF NOT EXISTS tenant_id TEXT;
CREATE INDEX IF NOT EXISTS idx_user_roles_tenant ON user_roles(tenant_id);
