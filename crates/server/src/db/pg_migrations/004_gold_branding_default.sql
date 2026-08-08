-- 004_gold_branding_default.sql
-- New tenants inherit the product's own accent instead of an indigo that
-- belongs to no palette here. Existing tenants keep whatever they chose:
-- only the column default moves, no rows are rewritten.

ALTER TABLE tenants ALTER COLUMN branding_primary_color SET DEFAULT '#c4a265'::text;
