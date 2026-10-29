-- Email verification challenges are stored only as SHA-256 hashes, bound to the
-- address they were issued for, with the last delivery outcome recorded.
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_email TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_delivery TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_delivery_at TIMESTAMPTZ;
-- Withdraw plaintext challenges issued before hashing (affected users resend).
UPDATE users SET verification_token = NULL, verification_token_expires = NULL
WHERE verification_token IS NOT NULL;
CREATE INDEX IF NOT EXISTS users_verification_token ON users(verification_token)
WHERE verification_token IS NOT NULL;
