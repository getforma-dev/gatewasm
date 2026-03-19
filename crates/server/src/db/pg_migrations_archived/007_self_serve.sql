-- Migration 007: Self-serve tenant onboarding
-- Adds plan/trial columns and seeds the reserved "platform" tenant.

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS plan TEXT DEFAULT 'trial';
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS trial_ends_at TIMESTAMPTZ;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS owner_user_id TEXT;

-- Seed the reserved "platform" tenant (idempotent).
-- Tenant owners register as users on this tenant, then create their own tenants.
INSERT INTO tenants (id, name, slug, owner_email, status, max_users, plan, login_redirect_url)
VALUES (
  'ten_platform',
  'GateWASM Platform',
  'platform',
  'admin@getforma.dev',
  'active',
  100000,
  'enterprise',
  '/platform/dashboard'
)
ON CONFLICT (slug) DO NOTHING;
