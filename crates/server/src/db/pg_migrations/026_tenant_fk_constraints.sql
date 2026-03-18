-- Add missing FK constraints on tenant_id columns.
-- Uses DO blocks to check for existing constraints (idempotent).

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_refresh_tokens_tenant' AND table_name = 'refresh_tokens'
  ) THEN
    ALTER TABLE refresh_tokens
      ADD CONSTRAINT fk_refresh_tokens_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_oauth_clients_tenant' AND table_name = 'oauth_clients'
  ) THEN
    ALTER TABLE oauth_clients
      ADD CONSTRAINT fk_oauth_clients_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_oauth_states_tenant' AND table_name = 'oauth_states'
  ) THEN
    ALTER TABLE oauth_states
      ADD CONSTRAINT fk_oauth_states_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_auth_codes_tenant' AND table_name = 'auth_codes'
  ) THEN
    ALTER TABLE auth_codes
      ADD CONSTRAINT fk_auth_codes_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_login_attempts_tenant' AND table_name = 'login_attempts'
  ) THEN
    ALTER TABLE login_attempts
      ADD CONSTRAINT fk_login_attempts_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_user_roles_tenant' AND table_name = 'user_roles'
  ) THEN
    ALTER TABLE user_roles
      ADD CONSTRAINT fk_user_roles_tenant
      FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;
  END IF;
END $$;
