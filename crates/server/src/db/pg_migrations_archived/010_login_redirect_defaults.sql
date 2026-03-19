-- Migration 010: remove legacy localhost callback defaults
-- Older tenants may still carry this placeholder value.
-- We now compute a safe runtime fallback: {SERVER_URL}/t/{slug}/dashboard.

UPDATE tenants
SET login_redirect_url = NULL
WHERE login_redirect_url = 'http://localhost:3000/callback';

ALTER TABLE tenants
ALTER COLUMN login_redirect_url DROP DEFAULT;
