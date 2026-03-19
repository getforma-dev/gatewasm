-- Add missing FK constraints on tenant_id columns.
-- The migration runner suppresses "already exists" errors, making this idempotent.
-- Orphan cleanup runs first to prevent FK violation on existing data.

UPDATE refresh_tokens SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);
UPDATE oauth_clients SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);
UPDATE oauth_states SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);
UPDATE auth_codes SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);
UPDATE login_attempts SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);
UPDATE user_roles SET tenant_id = NULL WHERE tenant_id IS NOT NULL AND tenant_id NOT IN (SELECT id FROM tenants);

ALTER TABLE refresh_tokens
  ADD CONSTRAINT fk_refresh_tokens_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE oauth_clients
  ADD CONSTRAINT fk_oauth_clients_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE oauth_states
  ADD CONSTRAINT fk_oauth_states_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE auth_codes
  ADD CONSTRAINT fk_auth_codes_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE login_attempts
  ADD CONSTRAINT fk_login_attempts_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE user_roles
  ADD CONSTRAINT fk_user_roles_tenant
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
