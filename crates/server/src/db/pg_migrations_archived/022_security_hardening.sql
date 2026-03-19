-- 022_security_hardening.sql

-- Idle session timeout tracking
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ;
UPDATE refresh_tokens SET last_activity_at = created_at WHERE last_activity_at IS NULL;

-- Client fingerprint on access tokens (stored for risk scoring)
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS client_fingerprint TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS risk_score INT NOT NULL DEFAULT 0;

-- Tenant security config (per-tenant feature flags)
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS security_config JSONB NOT NULL DEFAULT '{}';

-- Index for idle timeout queries
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_last_activity
  ON refresh_tokens(user_id, last_activity_at)
  WHERE revoked = FALSE;
