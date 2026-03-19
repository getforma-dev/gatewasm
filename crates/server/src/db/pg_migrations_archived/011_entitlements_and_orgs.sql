-- Migration 011: Entitlements + Organizations model

CREATE TABLE IF NOT EXISTS tenant_entitlements (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    plan TEXT NOT NULL DEFAULT 'trial',
    max_users INTEGER NOT NULL DEFAULT 100,
    max_organizations INTEGER NOT NULL DEFAULT 3,
    allow_custom_roles BOOLEAN NOT NULL DEFAULT TRUE,
    allow_webhooks BOOLEAN NOT NULL DEFAULT TRUE,
    allow_organizations BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entitlements_plan ON tenant_entitlements(plan);

INSERT INTO tenant_entitlements (
    tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations
)
SELECT
    t.id,
    COALESCE(t.plan, 'trial'),
    COALESCE(t.max_users, 1000),
    CASE WHEN COALESCE(t.plan, 'trial') IN ('enterprise', 'pro') THEN 50 ELSE 3 END,
    TRUE,
    TRUE,
    TRUE
FROM tenants t
ON CONFLICT (tenant_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (tenant_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_orgs_tenant ON organizations(tenant_id);

CREATE TABLE IF NOT EXISTS organization_members (
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (organization_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_org_members_tenant ON organization_members(tenant_id);
CREATE INDEX IF NOT EXISTS idx_org_members_user ON organization_members(user_id);
