-- Migration 014: Provider owner invite flow
-- Enables invite-only onboarding for additional provider operators.

CREATE TABLE IF NOT EXISTS owner_invites (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    invited_by_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    accepted_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'revoked', 'expired')),
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_owner_invites_email ON owner_invites (email);
CREATE INDEX IF NOT EXISTS idx_owner_invites_status_expires ON owner_invites (status, expires_at);
CREATE INDEX IF NOT EXISTS idx_owner_invites_invited_by ON owner_invites (invited_by_user_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_owner_invites_pending_email
    ON owner_invites (lower(email))
    WHERE status = 'pending';
