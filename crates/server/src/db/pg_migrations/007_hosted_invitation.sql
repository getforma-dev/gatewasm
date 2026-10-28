ALTER TABLE hosted_authorization_requests ADD COLUMN IF NOT EXISTS invite_token_hash TEXT;
CREATE INDEX IF NOT EXISTS par_tenant_pending ON par_requests(tenant_id, expires_at) WHERE consumed=FALSE;
