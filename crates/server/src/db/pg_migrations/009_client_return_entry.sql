-- The client whose validated hosted request created the account, and the
-- per-client entry route the verified page may offer (a fresh sign-in).
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_client_id TEXT;
ALTER TABLE oauth_clients ADD COLUMN IF NOT EXISTS return_entry_url TEXT;
