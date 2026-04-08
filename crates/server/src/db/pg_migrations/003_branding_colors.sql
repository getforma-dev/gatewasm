-- 003_branding_colors.sql
-- Adds background and card color columns for login page branding

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_bg_color text;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_card_color text;
