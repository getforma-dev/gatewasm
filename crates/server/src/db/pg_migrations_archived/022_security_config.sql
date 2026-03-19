-- Migration 022: Add security_config JSONB column to tenants

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS security_config JSONB DEFAULT '{}'::jsonb;
