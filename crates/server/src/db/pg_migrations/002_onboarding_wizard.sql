-- 002_onboarding_wizard.sql
-- Adds wizard-specific columns to tenants table and mfa_policy default to security_config

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS tenant_intent text;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_dark_mode boolean NOT NULL DEFAULT true;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_button_text text NOT NULL DEFAULT 'Sign in';

-- Set mfa_policy default in security_config JSONB for existing tenants
UPDATE tenants
SET security_config = security_config || '{"mfa_policy": "off"}'
WHERE NOT (security_config ? 'mfa_policy');
