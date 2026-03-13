-- Migration 006: Hosted login branding columns on tenants
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_logo_url TEXT;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_primary_color TEXT DEFAULT '#6366f1';
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_app_name TEXT;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_custom_css TEXT;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS login_redirect_url TEXT DEFAULT 'http://localhost:3000/callback';
