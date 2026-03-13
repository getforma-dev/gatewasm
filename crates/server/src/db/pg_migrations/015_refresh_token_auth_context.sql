-- Migration 015: Refresh token auth context
-- Persists how the session was authenticated so owner-plane middleware can
-- enforce SSO + MFA on access-token refresh.

ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS auth_method TEXT DEFAULT 'password';
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS auth_provider TEXT;

UPDATE refresh_tokens
SET auth_method = 'password'
WHERE auth_method IS NULL;

ALTER TABLE refresh_tokens ALTER COLUMN auth_method SET DEFAULT 'password';
ALTER TABLE refresh_tokens ALTER COLUMN auth_method SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_auth_context
    ON refresh_tokens (user_id, auth_method, mfa_verified);
