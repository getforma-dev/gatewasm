-- Add CSRF HMAC tag to oauth_states
ALTER TABLE oauth_states ADD COLUMN IF NOT EXISTS hmac_tag TEXT;

-- Add email verification columns to users
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_token TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_token_expires TIMESTAMPTZ;
