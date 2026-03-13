-- Compliance engine: audit_log table + session tracking columns

-- COMPLIANCE: audit_log is append-only. Application code must NEVER issue UPDATE or DELETE.
CREATE TABLE IF NOT EXISTS audit_log (
    id SERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    ip_address TEXT NOT NULL DEFAULT '0.0.0.0',
    user_agent TEXT,
    session_id TEXT,
    result TEXT NOT NULL,
    failure_reason TEXT,
    compliance_mode TEXT NOT NULL DEFAULT 'standard',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_tenant ON audit_log(tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_log(tenant_id, user_id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(tenant_id, action);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(tenant_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_result ON audit_log(tenant_id, result);

-- Session activity tracking for compliance timeout enforcement
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS last_activity TIMESTAMPTZ DEFAULT NOW();

-- MFA tracking on sessions
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS mfa_verified BOOLEAN DEFAULT FALSE;

-- User MFA setup status
ALTER TABLE users ADD COLUMN IF NOT EXISTS mfa_secret TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS mfa_enabled BOOLEAN DEFAULT FALSE;
