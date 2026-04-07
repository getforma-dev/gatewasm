-- fAuth schema (squashed from 4 migrations: 001_initial + 002_onboarding_fields + 003_hosted_pages + 004_platform_members)
-- Generated: 2026-04-07
-- No public. qualifiers — works in both public schema (prod) and arbitrary schemas (integration tests via SET search_path)

-- ── Sequences ────────────────────────────────────────────────────────────

CREATE SEQUENCE audit_log_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

CREATE SEQUENCE ssf_outbox_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

CREATE SEQUENCE ssf_sequence_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- ── Core tables (no FK deps) ──────────────────────────────────────────────

CREATE TABLE tenants (
    id text NOT NULL PRIMARY KEY,
    name text NOT NULL,
    slug text NOT NULL UNIQUE,
    owner_email text NOT NULL,
    api_key_hash text,
    api_key_prefix text,
    allowed_providers jsonb DEFAULT '["email"]'::jsonb,
    google_client_id text,
    google_client_secret text,
    github_client_id text,
    github_client_secret text,
    custom_jwt_issuer text,
    allowed_origins jsonb DEFAULT '[]'::jsonb,
    max_users integer DEFAULT 1000,
    status text DEFAULT 'active'::text,
    mau_count integer DEFAULT 0,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    branding_logo_url text,
    branding_primary_color text DEFAULT '#6366f1'::text,
    branding_app_name text,
    branding_custom_css text,
    login_redirect_url text,
    plan text DEFAULT 'trial'::text,
    trial_ends_at timestamp with time zone,
    owner_user_id text,
    signup_mode text DEFAULT 'open'::text NOT NULL,
    security_config jsonb DEFAULT '{}'::jsonb NOT NULL,
    onboarding_completed boolean DEFAULT false NOT NULL,
    onboarding_step smallint DEFAULT 1 NOT NULL,
    preferred_tab text
);

CREATE TABLE users (
    id text NOT NULL PRIMARY KEY,
    email text NOT NULL,
    name text,
    avatar text,
    email_verified boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    verification_token text,
    verification_token_expires timestamp with time zone,
    login_attempts integer DEFAULT 0,
    locked_until timestamp with time zone,
    password_reset_token text,
    password_reset_expires timestamp with time zone,
    tenant_id text,
    mfa_secret text,
    mfa_enabled boolean DEFAULT false,
    mfa_attempts integer DEFAULT 0 NOT NULL,
    mfa_locked_until timestamp with time zone
);

CREATE TABLE roles (
    id text NOT NULL PRIMARY KEY,
    name text NOT NULL,
    permissions text DEFAULT '[]'::text NOT NULL,
    tenant_id text,
    is_system boolean DEFAULT false,
    description text,
    created_at timestamp with time zone DEFAULT now(),
    permissions_jsonb jsonb DEFAULT '[]'::jsonb
);

-- ── Tables dependent on tenants/users/roles ───────────────────────────────

CREATE TABLE api_keys (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    key_hash text NOT NULL UNIQUE,
    key_prefix text NOT NULL,
    name text NOT NULL,
    scopes text,
    last_used_at timestamp with time zone,
    expires_at timestamp with time zone,
    revoked boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE audit_log (
    id integer NOT NULL DEFAULT nextval('audit_log_id_seq') PRIMARY KEY,
    event_id text NOT NULL UNIQUE,
    "timestamp" timestamp with time zone DEFAULT now() NOT NULL,
    tenant_id text NOT NULL,
    user_id text,
    action text NOT NULL,
    resource text NOT NULL,
    ip_address text DEFAULT '0.0.0.0'::text NOT NULL,
    user_agent text,
    session_id text,
    result text NOT NULL,
    failure_reason text,
    compliance_mode text DEFAULT 'standard'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);

ALTER SEQUENCE audit_log_id_seq OWNED BY audit_log.id;

CREATE TABLE auth_codes (
    code text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    code_challenge text NOT NULL,
    redirect_uri text NOT NULL,
    client_id text NOT NULL,
    provider text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    used boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE client_certificates (
    id text NOT NULL PRIMARY KEY,
    fingerprint text NOT NULL,
    client_id text NOT NULL,
    tenant_id text NOT NULL,
    name text DEFAULT 'Certificate'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone,
    UNIQUE (tenant_id, fingerprint)
);

CREATE TABLE credentials (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    provider text NOT NULL,
    provider_user_id text,
    hashed_password text,
    metadata text,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE dpop_jti_cache (
    jti text NOT NULL PRIMARY KEY,
    expires_at timestamp with time zone NOT NULL
);

CREATE TABLE fedcm_approved_clients (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    client_id text NOT NULL,
    approved_at timestamp with time zone DEFAULT now() NOT NULL,
    UNIQUE (user_id, tenant_id, client_id)
);

CREATE TABLE login_attempts (
    id text NOT NULL PRIMARY KEY,
    email text NOT NULL,
    user_id text,
    success boolean DEFAULT false NOT NULL,
    ip_address text,
    user_agent text,
    failure_reason text,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE oauth_clients (
    id text NOT NULL PRIMARY KEY,
    redirect_uri text NOT NULL,
    name text NOT NULL,
    tenant_id text
);

CREATE TABLE oauth_states (
    state text NOT NULL PRIMARY KEY,
    code_challenge text NOT NULL,
    provider text NOT NULL,
    redirect_uri text NOT NULL,
    client_id text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    hmac_tag text,
    tenant_id text,
    invite_token_hash text
);

CREATE TABLE organizations (
    id text NOT NULL PRIMARY KEY,
    tenant_id text NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    UNIQUE (tenant_id, slug)
);

CREATE TABLE organization_members (
    organization_id text NOT NULL,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    PRIMARY KEY (organization_id, user_id)
);

CREATE TABLE owner_invites (
    id text NOT NULL PRIMARY KEY,
    email text NOT NULL,
    token_hash text NOT NULL UNIQUE,
    invited_by_user_id text NOT NULL,
    accepted_by_user_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    accepted_at timestamp with time zone,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    role text DEFAULT 'editor'::text NOT NULL,
    tenant_ids text[] DEFAULT '{}'::text[] NOT NULL,
    account_owner_user_id text,
    CONSTRAINT owner_invites_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'accepted'::text, 'revoked'::text, 'expired'::text])))
);

CREATE TABLE owner_timeline_events (
    id text NOT NULL PRIMARY KEY,
    customer_user_id text,
    tenant_id text,
    actor_user_id text,
    actor_email text,
    action text NOT NULL,
    resource text NOT NULL,
    request_id text,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE par_requests (
    request_uri text NOT NULL PRIMARY KEY,
    client_id text NOT NULL,
    parameters jsonb NOT NULL,
    tenant_id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed boolean DEFAULT false NOT NULL
);

CREATE TABLE platform_accounts (
    user_id text NOT NULL PRIMARY KEY,
    status text DEFAULT 'active'::text NOT NULL,
    company_name text,
    billing_ref text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    category text DEFAULT 'standard'::text NOT NULL,
    notes text,
    risk_level text DEFAULT 'normal'::text NOT NULL,
    lifecycle_stage text DEFAULT 'active'::text NOT NULL,
    blocked_reason text,
    last_contacted_at timestamp with time zone
);

CREATE TABLE platform_members (
    id text NOT NULL PRIMARY KEY,
    account_owner_user_id text NOT NULL,
    user_id text NOT NULL,
    invited_by_user_id text,
    status text DEFAULT 'active'::text NOT NULL,
    last_login_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT platform_members_status_check CHECK ((status = ANY (ARRAY['active'::text, 'suspended'::text]))),
    CONSTRAINT platform_members_account_user_unique UNIQUE (account_owner_user_id, user_id)
);

CREATE TABLE platform_member_tenants (
    id text NOT NULL PRIMARY KEY,
    member_id text NOT NULL,
    tenant_id text NOT NULL,
    role text NOT NULL,
    granted_by_user_id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT platform_member_tenants_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'editor'::text, 'viewer'::text]))),
    CONSTRAINT platform_member_tenants_unique UNIQUE (member_id, tenant_id)
);

CREATE TABLE provider_billing_events (
    id text NOT NULL PRIMARY KEY,
    customer_user_id text NOT NULL,
    tenant_id text,
    amount_cents bigint NOT NULL,
    currency text DEFAULT 'usd'::text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    external_invoice_id text,
    receipt_url text,
    period_start timestamp with time zone,
    period_end timestamp with time zone,
    notes text,
    metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_by_user_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT provider_billing_events_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'paid'::text, 'failed'::text, 'refunded'::text])))
);

CREATE TABLE refresh_tokens (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    token_hash text NOT NULL UNIQUE,
    session_id text,
    expires_at timestamp with time zone NOT NULL,
    revoked boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    family_id text,
    tenant_id text,
    last_activity timestamp with time zone DEFAULT now(),
    mfa_verified boolean DEFAULT false,
    auth_method text DEFAULT 'password'::text NOT NULL,
    auth_provider text,
    last_activity_at timestamp with time zone,
    client_fingerprint text,
    risk_score integer DEFAULT 0 NOT NULL,
    login_lat double precision,
    login_lon double precision,
    login_ip text,
    login_ip_type text
);

CREATE TABLE sessions (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    token_hash text NOT NULL,
    ip_address text,
    user_agent text,
    expires_at timestamp with time zone NOT NULL,
    revoked boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE ssf_streams (
    id uuid DEFAULT gen_random_uuid() NOT NULL PRIMARY KEY,
    tenant_id text NOT NULL,
    receiver_url text,
    delivery_method text DEFAULT 'push'::text NOT NULL,
    event_types text[] DEFAULT '{}'::text[] NOT NULL,
    shared_secret text NOT NULL,
    previous_shared_secret text,
    secret_rotated_at timestamp with time zone,
    secret_grace_hours integer DEFAULT 48 NOT NULL,
    status text DEFAULT 'enabled'::text NOT NULL,
    consecutive_failures integer DEFAULT 0 NOT NULL,
    last_error text,
    last_delivery_at timestamp with time zone,
    last_ack_sequence bigint DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE ssf_outbox (
    id bigint NOT NULL DEFAULT nextval('ssf_outbox_id_seq') PRIMARY KEY,
    tenant_id text NOT NULL,
    stream_id uuid NOT NULL,
    sequence_number bigint NOT NULL,
    event_type text NOT NULL,
    set_token text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    delivered_at timestamp with time zone,
    last_error text
);

ALTER SEQUENCE ssf_outbox_id_seq OWNED BY ssf_outbox.id;

CREATE TABLE tenant_entitlements (
    tenant_id text NOT NULL PRIMARY KEY,
    plan text DEFAULT 'trial'::text NOT NULL,
    max_users integer DEFAULT 100 NOT NULL,
    max_organizations integer DEFAULT 3 NOT NULL,
    allow_custom_roles boolean DEFAULT true NOT NULL,
    allow_webhooks boolean DEFAULT true NOT NULL,
    allow_organizations boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);

CREATE TABLE tenant_hosted_pages (
    tenant_id text NOT NULL PRIMARY KEY,
    headline text DEFAULT 'Welcome'::text NOT NULL,
    body text DEFAULT ''::text NOT NULL,
    cta_text text DEFAULT ''::text NOT NULL,
    cta_link text DEFAULT ''::text NOT NULL,
    custom_html text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE tenant_user_invites (
    id text NOT NULL PRIMARY KEY,
    tenant_id text NOT NULL,
    email text NOT NULL,
    role_id text NOT NULL,
    token_hash text NOT NULL UNIQUE,
    status text DEFAULT 'pending'::text NOT NULL,
    invited_by_user_id text,
    accepted_by_user_id text,
    expires_at timestamp with time zone NOT NULL,
    accepted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE user_roles (
    user_id text NOT NULL,
    role_id text NOT NULL,
    tenant_id text,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE webauthn_challenges (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    challenge_data jsonb NOT NULL,
    ceremony_type text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:05:00'::interval) NOT NULL
);

CREATE TABLE webauthn_credentials (
    id text NOT NULL PRIMARY KEY,
    credential_id bytea NOT NULL,
    public_key bytea NOT NULL,
    sign_count bigint DEFAULT 0 NOT NULL,
    is_synced boolean DEFAULT false NOT NULL,
    transports text[] DEFAULT '{}'::text[] NOT NULL,
    name text DEFAULT 'Passkey'::text NOT NULL,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    aaguid text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    UNIQUE (tenant_id, credential_id)
);

CREATE TABLE webhooks (
    id text NOT NULL PRIMARY KEY,
    tenant_id text NOT NULL,
    url text NOT NULL,
    secret text NOT NULL,
    events jsonb DEFAULT '[]'::jsonb NOT NULL,
    active boolean DEFAULT true,
    description text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);

CREATE TABLE webhook_deliveries (
    id text NOT NULL PRIMARY KEY,
    webhook_id text NOT NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL,
    response_status integer,
    response_body text,
    attempt integer DEFAULT 1,
    max_attempts integer DEFAULT 3,
    next_retry_at timestamp with time zone,
    status text DEFAULT 'pending'::text,
    delivered_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now()
);

-- ── Foreign keys (all tables exist before this block) ────────────────────

ALTER TABLE ONLY api_keys
    ADD CONSTRAINT api_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY auth_codes
    ADD CONSTRAINT auth_codes_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY auth_codes
    ADD CONSTRAINT fk_auth_codes_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY client_certificates
    ADD CONSTRAINT client_certificates_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY credentials
    ADD CONSTRAINT credentials_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY login_attempts
    ADD CONSTRAINT fk_login_attempts_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY oauth_clients
    ADD CONSTRAINT fk_oauth_clients_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY oauth_states
    ADD CONSTRAINT fk_oauth_states_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY organization_members
    ADD CONSTRAINT organization_members_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE;

ALTER TABLE ONLY organization_members
    ADD CONSTRAINT organization_members_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY organization_members
    ADD CONSTRAINT organization_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY organizations
    ADD CONSTRAINT organizations_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY owner_invites
    ADD CONSTRAINT owner_invites_accepted_by_user_id_fkey FOREIGN KEY (accepted_by_user_id) REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE ONLY owner_invites
    ADD CONSTRAINT owner_invites_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY owner_invites
    ADD CONSTRAINT owner_invites_account_owner_user_id_fkey FOREIGN KEY (account_owner_user_id) REFERENCES users(id);

ALTER TABLE ONLY par_requests
    ADD CONSTRAINT par_requests_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY platform_accounts
    ADD CONSTRAINT platform_accounts_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY platform_members
    ADD CONSTRAINT platform_members_account_owner_user_id_fkey FOREIGN KEY (account_owner_user_id) REFERENCES users(id);

ALTER TABLE ONLY platform_members
    ADD CONSTRAINT platform_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

ALTER TABLE ONLY platform_members
    ADD CONSTRAINT platform_members_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES users(id);

ALTER TABLE ONLY platform_member_tenants
    ADD CONSTRAINT platform_member_tenants_member_id_fkey FOREIGN KEY (member_id) REFERENCES platform_members(id) ON DELETE CASCADE;

ALTER TABLE ONLY platform_member_tenants
    ADD CONSTRAINT platform_member_tenants_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY platform_member_tenants
    ADD CONSTRAINT platform_member_tenants_granted_by_user_id_fkey FOREIGN KEY (granted_by_user_id) REFERENCES users(id);

ALTER TABLE ONLY provider_billing_events
    ADD CONSTRAINT provider_billing_events_customer_user_id_fkey FOREIGN KEY (customer_user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY provider_billing_events
    ADD CONSTRAINT provider_billing_events_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY refresh_tokens
    ADD CONSTRAINT fk_refresh_tokens_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY refresh_tokens
    ADD CONSTRAINT refresh_tokens_session_id_fkey FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY refresh_tokens
    ADD CONSTRAINT refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY ssf_outbox
    ADD CONSTRAINT ssf_outbox_stream_id_fkey FOREIGN KEY (stream_id) REFERENCES ssf_streams(id) ON DELETE CASCADE;

ALTER TABLE ONLY ssf_streams
    ADD CONSTRAINT ssf_streams_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY tenant_entitlements
    ADD CONSTRAINT tenant_entitlements_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY tenant_hosted_pages
    ADD CONSTRAINT tenant_hosted_pages_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_accepted_by_user_id_fkey FOREIGN KEY (accepted_by_user_id) REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE ONLY tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE ONLY tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_role_id_fkey FOREIGN KEY (role_id) REFERENCES roles(id);

ALTER TABLE ONLY tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY user_roles
    ADD CONSTRAINT fk_user_roles_tenant FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY user_roles
    ADD CONSTRAINT user_roles_role_id_fkey FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE;

ALTER TABLE ONLY user_roles
    ADD CONSTRAINT user_roles_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY users
    ADD CONSTRAINT users_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY webauthn_challenges
    ADD CONSTRAINT webauthn_challenges_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY webauthn_challenges
    ADD CONSTRAINT webauthn_challenges_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

ALTER TABLE ONLY webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE ONLY webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_webhook_id_fkey FOREIGN KEY (webhook_id) REFERENCES webhooks(id) ON DELETE CASCADE;

ALTER TABLE ONLY webhooks
    ADD CONSTRAINT webhooks_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES tenants(id);

-- ── Indexes ────────────────────────────────────────────────────────────────

CREATE INDEX idx_api_keys_user_id ON api_keys USING btree (user_id);

CREATE INDEX idx_audit_action ON audit_log USING btree (tenant_id, action);
CREATE INDEX idx_audit_result ON audit_log USING btree (tenant_id, result);
CREATE INDEX idx_audit_tenant ON audit_log USING btree (tenant_id);
CREATE INDEX idx_audit_timestamp ON audit_log USING btree (tenant_id, "timestamp");
CREATE INDEX idx_audit_user ON audit_log USING btree (tenant_id, user_id);

CREATE INDEX idx_auth_codes_expires ON auth_codes USING btree (expires_at);
CREATE INDEX idx_auth_codes_tenant ON auth_codes USING btree (tenant_id);

CREATE INDEX idx_client_certs_lookup ON client_certificates USING btree (tenant_id, client_id) WHERE (revoked_at IS NULL);

CREATE INDEX idx_credentials_tenant ON credentials USING btree (tenant_id, user_id);
CREATE UNIQUE INDEX idx_credentials_tenant_provider_unique ON credentials USING btree (tenant_id, provider, provider_user_id);
CREATE INDEX idx_credentials_user_id ON credentials USING btree (user_id);

CREATE INDEX idx_dpop_jti_expiry ON dpop_jti_cache USING btree (expires_at);

CREATE INDEX idx_entitlements_plan ON tenant_entitlements USING btree (plan);

CREATE INDEX idx_login_attempts_created ON login_attempts USING btree (created_at);
CREATE INDEX idx_login_attempts_email ON login_attempts USING btree (email);
CREATE INDEX idx_login_attempts_tenant ON login_attempts USING btree (tenant_id, email);
CREATE INDEX idx_login_attempts_tenant_success_window ON login_attempts USING btree (tenant_id, success, created_at DESC, user_id);

CREATE INDEX idx_oauth_states_expires ON oauth_states USING btree (expires_at);
CREATE INDEX idx_oauth_states_tenant ON oauth_states USING btree (tenant_id);

CREATE INDEX idx_org_members_tenant ON organization_members USING btree (tenant_id);
CREATE INDEX idx_org_members_user ON organization_members USING btree (user_id);

CREATE INDEX idx_orgs_tenant ON organizations USING btree (tenant_id);

CREATE INDEX idx_owner_invites_email ON owner_invites USING btree (email);
CREATE INDEX idx_owner_invites_invited_by ON owner_invites USING btree (invited_by_user_id);
CREATE INDEX idx_owner_invites_status_expires ON owner_invites USING btree (status, expires_at);
CREATE UNIQUE INDEX uq_owner_invites_pending_email_account ON owner_invites USING btree (lower(email), account_owner_user_id) WHERE (status = 'pending'::text);

CREATE INDEX idx_owner_timeline_action ON owner_timeline_events USING btree (action, created_at DESC);
CREATE INDEX idx_owner_timeline_actor ON owner_timeline_events USING btree (actor_user_id, created_at DESC);
CREATE INDEX idx_owner_timeline_customer ON owner_timeline_events USING btree (customer_user_id, created_at DESC);
CREATE INDEX idx_owner_timeline_request ON owner_timeline_events USING btree (request_id);
CREATE INDEX idx_owner_timeline_tenant ON owner_timeline_events USING btree (tenant_id, created_at DESC);

CREATE INDEX idx_par_expiry ON par_requests USING btree (expires_at) WHERE (consumed = false);

CREATE INDEX idx_platform_accounts_category ON platform_accounts USING btree (category);
CREATE INDEX idx_platform_accounts_lifecycle_stage ON platform_accounts USING btree (lifecycle_stage);
CREATE INDEX idx_platform_accounts_status ON platform_accounts USING btree (status);
CREATE INDEX idx_platform_accounts_status_created ON platform_accounts USING btree (status, created_at DESC);

CREATE INDEX idx_platform_members_account_owner ON platform_members USING btree (account_owner_user_id);
CREATE INDEX idx_platform_members_user ON platform_members USING btree (user_id);
CREATE INDEX idx_platform_members_status ON platform_members USING btree (status);

CREATE INDEX idx_platform_member_tenants_member ON platform_member_tenants USING btree (member_id);
CREATE INDEX idx_platform_member_tenants_tenant ON platform_member_tenants USING btree (tenant_id);

CREATE INDEX idx_provider_billing_customer ON provider_billing_events USING btree (customer_user_id, created_at DESC);
CREATE INDEX idx_provider_billing_customer_status_window ON provider_billing_events USING btree (customer_user_id, status, created_at DESC);
CREATE INDEX idx_provider_billing_status ON provider_billing_events USING btree (status, created_at DESC);
CREATE INDEX idx_provider_billing_tenant_status_window ON provider_billing_events USING btree (tenant_id, status, created_at DESC);

CREATE INDEX idx_refresh_tokens_auth_context ON refresh_tokens USING btree (user_id, auth_method, mfa_verified);
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens USING btree (token_hash);
CREATE INDEX idx_refresh_tokens_last_activity ON refresh_tokens USING btree (user_id, last_activity_at) WHERE (revoked = false);
CREATE INDEX idx_refresh_tokens_tenant ON refresh_tokens USING btree (tenant_id, user_id);
CREATE INDEX idx_refresh_tokens_tenant_active_window ON refresh_tokens USING btree (tenant_id, revoked, expires_at DESC);
CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens USING btree (user_id);

CREATE INDEX idx_roles_tenant ON roles USING btree (tenant_id);
CREATE UNIQUE INDEX idx_roles_tenant_name ON roles USING btree (COALESCE(tenant_id, '__system__'::text), name);

CREATE INDEX idx_sessions_expires ON sessions USING btree (expires_at);
CREATE INDEX idx_sessions_user_id ON sessions USING btree (user_id);

CREATE INDEX idx_ssf_outbox_pending ON ssf_outbox USING btree (next_attempt_at) WHERE (status = 'pending'::text);
CREATE INDEX idx_ssf_outbox_stream_seq ON ssf_outbox USING btree (stream_id, sequence_number);

CREATE INDEX idx_ssf_streams_tenant ON ssf_streams USING btree (tenant_id) WHERE (status = 'enabled'::text);

CREATE INDEX idx_tenant_user_invites_tenant_email ON tenant_user_invites USING btree (tenant_id, lower(email));
CREATE INDEX idx_tenant_user_invites_tenant_status ON tenant_user_invites USING btree (tenant_id, status, expires_at DESC);
CREATE INDEX idx_tenant_user_invites_token_hash ON tenant_user_invites USING btree (token_hash);

CREATE INDEX idx_tenants_api_key_prefix ON tenants USING btree (api_key_prefix);
CREATE INDEX idx_tenants_owner_active_inventory ON tenants USING btree (owner_user_id, created_at DESC) WHERE ((id <> 'ten_platform'::text) AND (status <> 'deleted'::text));
CREATE INDEX idx_tenants_owner_plan_status ON tenants USING btree (owner_user_id, COALESCE(plan, 'trial'::text), status, trial_ends_at);
CREATE INDEX idx_tenants_slug ON tenants USING btree (slug);

CREATE INDEX idx_user_roles_tenant ON user_roles USING btree (tenant_id);

CREATE INDEX idx_users_mfa_locked_until ON users USING btree (mfa_locked_until);
CREATE INDEX idx_users_tenant ON users USING btree (tenant_id);
CREATE INDEX idx_users_tenant_email ON users USING btree (tenant_id, email);
CREATE UNIQUE INDEX idx_users_tenant_email_unique ON users USING btree (tenant_id, email);

CREATE INDEX idx_webauthn_challenges_expiry ON webauthn_challenges USING btree (expires_at);
CREATE INDEX idx_webauthn_user ON webauthn_credentials USING btree (user_id, tenant_id);

CREATE INDEX idx_webhooks_tenant ON webhooks USING btree (tenant_id);
CREATE INDEX idx_webhooks_tenant_active ON webhooks USING btree (tenant_id, active);

CREATE INDEX idx_whd_status ON webhook_deliveries USING btree (status, next_retry_at);
CREATE INDEX idx_whd_webhook ON webhook_deliveries USING btree (webhook_id);
CREATE INDEX idx_whd_webhook_created ON webhook_deliveries USING btree (webhook_id, created_at DESC);

-- ── Seed data ──────────────────────────────────────────────────────────────

-- Seed: system roles
INSERT INTO roles (id, name, permissions, permissions_jsonb, is_system, description)
VALUES
  ('role_user', 'user', '["read"]', '["read"]', true, 'Default user role'),
  ('role_admin', 'admin', '["read","write","admin"]', '["read","write","admin"]', true, 'Tenant admin role'),
  ('role_platform_owner', 'platform_owner', '["platform:owner"]', '["platform:owner"]', true, 'Provider control-plane owner role')
ON CONFLICT DO NOTHING;

-- Seed: platform tenant
INSERT INTO tenants (id, name, slug, owner_email, status, max_users, plan, login_redirect_url)
VALUES (
  'ten_platform',
  'fAuth',
  'platform',
  'admin@getforma.dev',
  'active',
  10000,
  'enterprise',
  '/platform/console'
) ON CONFLICT DO NOTHING;

-- Seed: platform tenant entitlements
INSERT INTO tenant_entitlements (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations)
VALUES ('ten_platform', 'enterprise', 10000, 100, true, true, true)
ON CONFLICT DO NOTHING;
