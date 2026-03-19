-- MFA hardening:
-- 1) Track failed MFA verification attempts with lockout window.
-- 2) Keep schema ready for encrypted MFA secret storage.

ALTER TABLE users ADD COLUMN IF NOT EXISTS mfa_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN IF NOT EXISTS mfa_locked_until TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_users_mfa_locked_until ON users(mfa_locked_until);
