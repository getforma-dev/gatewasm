-- GateWASM schema (squashed from 26 migrations)
-- Generated: 2026-03-19
-- After production launch, new migrations start at version 2.

CREATE TABLE public.api_keys (
    id text NOT NULL,
    user_id text NOT NULL,
    key_hash text NOT NULL,
    key_prefix text NOT NULL,
    name text NOT NULL,
    scopes text,
    last_used_at timestamp with time zone,
    expires_at timestamp with time zone,
    revoked boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE public.audit_log (
    id integer NOT NULL,
    event_id text NOT NULL,
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

CREATE SEQUENCE public.audit_log_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.audit_log_id_seq OWNED BY public.audit_log.id;

CREATE TABLE public.auth_codes (
    code text NOT NULL,
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

CREATE TABLE public.client_certificates (
    id text NOT NULL,
    fingerprint text NOT NULL,
    client_id text NOT NULL,
    tenant_id text NOT NULL,
    name text DEFAULT 'Certificate'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone
);

CREATE TABLE public.credentials (
    id text NOT NULL,
    user_id text NOT NULL,
    provider text NOT NULL,
    provider_user_id text,
    hashed_password text,
    metadata text,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE public.dpop_jti_cache (
    jti text NOT NULL,
    expires_at timestamp with time zone NOT NULL
);

CREATE TABLE public.fedcm_approved_clients (
    id text NOT NULL,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    client_id text NOT NULL,
    approved_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE public.login_attempts (
    id text NOT NULL,
    email text NOT NULL,
    user_id text,
    success boolean DEFAULT false NOT NULL,
    ip_address text,
    user_agent text,
    failure_reason text,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE public.oauth_clients (
    id text NOT NULL,
    redirect_uri text NOT NULL,
    name text NOT NULL,
    tenant_id text
);

CREATE TABLE public.oauth_states (
    state text NOT NULL,
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

CREATE TABLE public.organization_members (
    organization_id text NOT NULL,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now()
);

CREATE TABLE public.organizations (
    id text NOT NULL,
    tenant_id text NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);

CREATE TABLE public.owner_invites (
    id text NOT NULL,
    email text NOT NULL,
    token_hash text NOT NULL,
    invited_by_user_id text NOT NULL,
    accepted_by_user_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    accepted_at timestamp with time zone,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT owner_invites_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'accepted'::text, 'revoked'::text, 'expired'::text])))
);

CREATE TABLE public.owner_timeline_events (
    id text NOT NULL,
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

CREATE TABLE public.par_requests (
    request_uri text NOT NULL,
    client_id text NOT NULL,
    parameters jsonb NOT NULL,
    tenant_id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed boolean DEFAULT false NOT NULL
);

CREATE TABLE public.platform_accounts (
    user_id text NOT NULL,
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

CREATE TABLE public.provider_billing_events (
    id text NOT NULL,
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

CREATE TABLE public.refresh_tokens (
    id text NOT NULL,
    user_id text NOT NULL,
    token_hash text NOT NULL,
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

CREATE TABLE public.roles (
    id text NOT NULL,
    name text NOT NULL,
    permissions text DEFAULT '[]'::text NOT NULL,
    tenant_id text,
    is_system boolean DEFAULT false,
    description text,
    created_at timestamp with time zone DEFAULT now(),
    permissions_jsonb jsonb DEFAULT '[]'::jsonb
);

CREATE TABLE public.sessions (
    id text NOT NULL,
    user_id text NOT NULL,
    token_hash text NOT NULL,
    ip_address text,
    user_agent text,
    expires_at timestamp with time zone NOT NULL,
    revoked boolean DEFAULT false,
    created_at timestamp with time zone DEFAULT now(),
    tenant_id text
);

CREATE TABLE public.ssf_outbox (
    id bigint NOT NULL,
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

CREATE SEQUENCE public.ssf_outbox_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

ALTER SEQUENCE public.ssf_outbox_id_seq OWNED BY public.ssf_outbox.id;

CREATE SEQUENCE public.ssf_sequence_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

CREATE TABLE public.ssf_streams (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
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

CREATE TABLE public.tenant_entitlements (
    tenant_id text NOT NULL,
    plan text DEFAULT 'trial'::text NOT NULL,
    max_users integer DEFAULT 100 NOT NULL,
    max_organizations integer DEFAULT 3 NOT NULL,
    allow_custom_roles boolean DEFAULT true NOT NULL,
    allow_webhooks boolean DEFAULT true NOT NULL,
    allow_organizations boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);

CREATE TABLE public.tenant_user_invites (
    id text NOT NULL,
    tenant_id text NOT NULL,
    email text NOT NULL,
    role_id text NOT NULL,
    token_hash text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    invited_by_user_id text,
    accepted_by_user_id text,
    expires_at timestamp with time zone NOT NULL,
    accepted_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE TABLE public.tenants (
    id text NOT NULL,
    name text NOT NULL,
    slug text NOT NULL,
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
    security_config jsonb DEFAULT '{}'::jsonb NOT NULL
);

CREATE TABLE public.user_roles (
    user_id text NOT NULL,
    role_id text NOT NULL,
    tenant_id text
);

CREATE TABLE public.users (
    id text NOT NULL,
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

CREATE TABLE public.webauthn_challenges (
    id text NOT NULL,
    user_id text NOT NULL,
    tenant_id text NOT NULL,
    challenge_data jsonb NOT NULL,
    ceremony_type text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:05:00'::interval) NOT NULL
);

CREATE TABLE public.webauthn_credentials (
    id text NOT NULL,
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
    last_used_at timestamp with time zone
);

CREATE TABLE public.webhook_deliveries (
    id text NOT NULL,
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

CREATE TABLE public.webhooks (
    id text NOT NULL,
    tenant_id text NOT NULL,
    url text NOT NULL,
    secret text NOT NULL,
    events jsonb DEFAULT '[]'::jsonb NOT NULL,
    active boolean DEFAULT true,
    description text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);

ALTER TABLE ONLY public.audit_log ALTER COLUMN id SET DEFAULT nextval('public.audit_log_id_seq'::regclass);

ALTER TABLE ONLY public.ssf_outbox ALTER COLUMN id SET DEFAULT nextval('public.ssf_outbox_id_seq'::regclass);

ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_key_hash_key UNIQUE (key_hash);

ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.audit_log
    ADD CONSTRAINT audit_log_event_id_key UNIQUE (event_id);

ALTER TABLE ONLY public.audit_log
    ADD CONSTRAINT audit_log_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.auth_codes
    ADD CONSTRAINT auth_codes_pkey PRIMARY KEY (code);

ALTER TABLE ONLY public.client_certificates
    ADD CONSTRAINT client_certificates_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.client_certificates
    ADD CONSTRAINT client_certificates_tenant_id_fingerprint_key UNIQUE (tenant_id, fingerprint);

ALTER TABLE ONLY public.credentials
    ADD CONSTRAINT credentials_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.dpop_jti_cache
    ADD CONSTRAINT dpop_jti_cache_pkey PRIMARY KEY (jti);

ALTER TABLE ONLY public.fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_user_id_tenant_id_client_id_key UNIQUE (user_id, tenant_id, client_id);

ALTER TABLE ONLY public.login_attempts
    ADD CONSTRAINT login_attempts_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.oauth_clients
    ADD CONSTRAINT oauth_clients_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.oauth_states
    ADD CONSTRAINT oauth_states_pkey PRIMARY KEY (state);

ALTER TABLE ONLY public.organization_members
    ADD CONSTRAINT organization_members_pkey PRIMARY KEY (organization_id, user_id);

ALTER TABLE ONLY public.organizations
    ADD CONSTRAINT organizations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.organizations
    ADD CONSTRAINT organizations_tenant_id_slug_key UNIQUE (tenant_id, slug);

ALTER TABLE ONLY public.owner_invites
    ADD CONSTRAINT owner_invites_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.owner_invites
    ADD CONSTRAINT owner_invites_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY public.owner_timeline_events
    ADD CONSTRAINT owner_timeline_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.par_requests
    ADD CONSTRAINT par_requests_pkey PRIMARY KEY (request_uri);

ALTER TABLE ONLY public.platform_accounts
    ADD CONSTRAINT platform_accounts_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY public.provider_billing_events
    ADD CONSTRAINT provider_billing_events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.ssf_outbox
    ADD CONSTRAINT ssf_outbox_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.ssf_streams
    ADD CONSTRAINT ssf_streams_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.tenant_entitlements
    ADD CONSTRAINT tenant_entitlements_pkey PRIMARY KEY (tenant_id);

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT tenants_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.tenants
    ADD CONSTRAINT tenants_slug_key UNIQUE (slug);

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_pkey PRIMARY KEY (user_id, role_id);

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.webauthn_challenges
    ADD CONSTRAINT webauthn_challenges_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_tenant_id_credential_id_key UNIQUE (tenant_id, credential_id);

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_pkey PRIMARY KEY (id);

CREATE INDEX idx_api_keys_user_id ON public.api_keys USING btree (user_id);

CREATE INDEX idx_audit_action ON public.audit_log USING btree (tenant_id, action);

CREATE INDEX idx_audit_result ON public.audit_log USING btree (tenant_id, result);

CREATE INDEX idx_audit_tenant ON public.audit_log USING btree (tenant_id);

CREATE INDEX idx_audit_timestamp ON public.audit_log USING btree (tenant_id, "timestamp");

CREATE INDEX idx_audit_user ON public.audit_log USING btree (tenant_id, user_id);

CREATE INDEX idx_auth_codes_expires ON public.auth_codes USING btree (expires_at);

CREATE INDEX idx_auth_codes_tenant ON public.auth_codes USING btree (tenant_id);

CREATE INDEX idx_client_certs_lookup ON public.client_certificates USING btree (tenant_id, client_id) WHERE (revoked_at IS NULL);

CREATE INDEX idx_credentials_tenant ON public.credentials USING btree (tenant_id, user_id);

CREATE UNIQUE INDEX idx_credentials_tenant_provider_unique ON public.credentials USING btree (tenant_id, provider, provider_user_id);

CREATE INDEX idx_credentials_user_id ON public.credentials USING btree (user_id);

CREATE INDEX idx_dpop_jti_expiry ON public.dpop_jti_cache USING btree (expires_at);

CREATE INDEX idx_entitlements_plan ON public.tenant_entitlements USING btree (plan);

CREATE INDEX idx_login_attempts_created ON public.login_attempts USING btree (created_at);

CREATE INDEX idx_login_attempts_email ON public.login_attempts USING btree (email);

CREATE INDEX idx_login_attempts_tenant ON public.login_attempts USING btree (tenant_id, email);

CREATE INDEX idx_login_attempts_tenant_success_window ON public.login_attempts USING btree (tenant_id, success, created_at DESC, user_id);

CREATE INDEX idx_oauth_states_expires ON public.oauth_states USING btree (expires_at);

CREATE INDEX idx_oauth_states_tenant ON public.oauth_states USING btree (tenant_id);

CREATE INDEX idx_org_members_tenant ON public.organization_members USING btree (tenant_id);

CREATE INDEX idx_org_members_user ON public.organization_members USING btree (user_id);

CREATE INDEX idx_orgs_tenant ON public.organizations USING btree (tenant_id);

CREATE INDEX idx_owner_invites_email ON public.owner_invites USING btree (email);

CREATE INDEX idx_owner_invites_invited_by ON public.owner_invites USING btree (invited_by_user_id);

CREATE INDEX idx_owner_invites_status_expires ON public.owner_invites USING btree (status, expires_at);

CREATE INDEX idx_owner_timeline_action ON public.owner_timeline_events USING btree (action, created_at DESC);

CREATE INDEX idx_owner_timeline_actor ON public.owner_timeline_events USING btree (actor_user_id, created_at DESC);

CREATE INDEX idx_owner_timeline_customer ON public.owner_timeline_events USING btree (customer_user_id, created_at DESC);

CREATE INDEX idx_owner_timeline_request ON public.owner_timeline_events USING btree (request_id);

CREATE INDEX idx_owner_timeline_tenant ON public.owner_timeline_events USING btree (tenant_id, created_at DESC);

CREATE INDEX idx_par_expiry ON public.par_requests USING btree (expires_at) WHERE (consumed = false);

CREATE INDEX idx_platform_accounts_category ON public.platform_accounts USING btree (category);

CREATE INDEX idx_platform_accounts_lifecycle_stage ON public.platform_accounts USING btree (lifecycle_stage);

CREATE INDEX idx_platform_accounts_status ON public.platform_accounts USING btree (status);

CREATE INDEX idx_platform_accounts_status_created ON public.platform_accounts USING btree (status, created_at DESC);

CREATE INDEX idx_provider_billing_customer ON public.provider_billing_events USING btree (customer_user_id, created_at DESC);

CREATE INDEX idx_provider_billing_customer_status_window ON public.provider_billing_events USING btree (customer_user_id, status, created_at DESC);

CREATE INDEX idx_provider_billing_status ON public.provider_billing_events USING btree (status, created_at DESC);

CREATE INDEX idx_provider_billing_tenant_status_window ON public.provider_billing_events USING btree (tenant_id, status, created_at DESC);

CREATE INDEX idx_refresh_tokens_auth_context ON public.refresh_tokens USING btree (user_id, auth_method, mfa_verified);

CREATE INDEX idx_refresh_tokens_hash ON public.refresh_tokens USING btree (token_hash);

CREATE INDEX idx_refresh_tokens_last_activity ON public.refresh_tokens USING btree (user_id, last_activity_at) WHERE (revoked = false);

CREATE INDEX idx_refresh_tokens_tenant ON public.refresh_tokens USING btree (tenant_id, user_id);

CREATE INDEX idx_refresh_tokens_tenant_active_window ON public.refresh_tokens USING btree (tenant_id, revoked, expires_at DESC);

CREATE INDEX idx_refresh_tokens_user_id ON public.refresh_tokens USING btree (user_id);

CREATE INDEX idx_roles_tenant ON public.roles USING btree (tenant_id);

CREATE UNIQUE INDEX idx_roles_tenant_name ON public.roles USING btree (COALESCE(tenant_id, '__system__'::text), name);

CREATE INDEX idx_sessions_expires ON public.sessions USING btree (expires_at);

CREATE INDEX idx_sessions_user_id ON public.sessions USING btree (user_id);

CREATE INDEX idx_ssf_outbox_pending ON public.ssf_outbox USING btree (next_attempt_at) WHERE (status = 'pending'::text);

CREATE INDEX idx_ssf_outbox_stream_seq ON public.ssf_outbox USING btree (stream_id, sequence_number);

CREATE INDEX idx_ssf_streams_tenant ON public.ssf_streams USING btree (tenant_id) WHERE (status = 'enabled'::text);

CREATE INDEX idx_tenant_user_invites_tenant_email ON public.tenant_user_invites USING btree (tenant_id, lower(email));

CREATE INDEX idx_tenant_user_invites_tenant_status ON public.tenant_user_invites USING btree (tenant_id, status, expires_at DESC);

CREATE INDEX idx_tenant_user_invites_token_hash ON public.tenant_user_invites USING btree (token_hash);

CREATE INDEX idx_tenants_api_key_prefix ON public.tenants USING btree (api_key_prefix);

CREATE INDEX idx_tenants_owner_active_inventory ON public.tenants USING btree (owner_user_id, created_at DESC) WHERE ((id <> 'ten_platform'::text) AND (status <> 'deleted'::text));

CREATE INDEX idx_tenants_owner_plan_status ON public.tenants USING btree (owner_user_id, COALESCE(plan, 'trial'::text), status, trial_ends_at);

CREATE INDEX idx_tenants_slug ON public.tenants USING btree (slug);

CREATE INDEX idx_user_roles_tenant ON public.user_roles USING btree (tenant_id);

CREATE INDEX idx_users_mfa_locked_until ON public.users USING btree (mfa_locked_until);

CREATE INDEX idx_users_tenant ON public.users USING btree (tenant_id);

CREATE INDEX idx_users_tenant_email ON public.users USING btree (tenant_id, email);

CREATE UNIQUE INDEX idx_users_tenant_email_unique ON public.users USING btree (tenant_id, email);

CREATE INDEX idx_webauthn_challenges_expiry ON public.webauthn_challenges USING btree (expires_at);

CREATE INDEX idx_webauthn_user ON public.webauthn_credentials USING btree (user_id, tenant_id);

CREATE INDEX idx_webhooks_tenant ON public.webhooks USING btree (tenant_id);

CREATE INDEX idx_webhooks_tenant_active ON public.webhooks USING btree (tenant_id, active);

CREATE INDEX idx_whd_status ON public.webhook_deliveries USING btree (status, next_retry_at);

CREATE INDEX idx_whd_webhook ON public.webhook_deliveries USING btree (webhook_id);

CREATE INDEX idx_whd_webhook_created ON public.webhook_deliveries USING btree (webhook_id, created_at DESC);

CREATE UNIQUE INDEX uq_owner_invites_pending_email ON public.owner_invites USING btree (lower(email)) WHERE (status = 'pending'::text);

ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.auth_codes
    ADD CONSTRAINT auth_codes_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.client_certificates
    ADD CONSTRAINT client_certificates_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.credentials
    ADD CONSTRAINT credentials_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.fedcm_approved_clients
    ADD CONSTRAINT fedcm_approved_clients_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.auth_codes
    ADD CONSTRAINT fk_auth_codes_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.login_attempts
    ADD CONSTRAINT fk_login_attempts_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.oauth_clients
    ADD CONSTRAINT fk_oauth_clients_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.oauth_states
    ADD CONSTRAINT fk_oauth_states_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT fk_refresh_tokens_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT fk_user_roles_tenant FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.organization_members
    ADD CONSTRAINT organization_members_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES public.organizations(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.organization_members
    ADD CONSTRAINT organization_members_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.organization_members
    ADD CONSTRAINT organization_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.organizations
    ADD CONSTRAINT organizations_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.owner_invites
    ADD CONSTRAINT owner_invites_accepted_by_user_id_fkey FOREIGN KEY (accepted_by_user_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.owner_invites
    ADD CONSTRAINT owner_invites_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.par_requests
    ADD CONSTRAINT par_requests_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.platform_accounts
    ADD CONSTRAINT platform_accounts_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.provider_billing_events
    ADD CONSTRAINT provider_billing_events_customer_user_id_fkey FOREIGN KEY (customer_user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.provider_billing_events
    ADD CONSTRAINT provider_billing_events_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.sessions(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.refresh_tokens
    ADD CONSTRAINT refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.ssf_outbox
    ADD CONSTRAINT ssf_outbox_stream_id_fkey FOREIGN KEY (stream_id) REFERENCES public.ssf_streams(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.ssf_streams
    ADD CONSTRAINT ssf_streams_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.tenant_entitlements
    ADD CONSTRAINT tenant_entitlements_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_accepted_by_user_id_fkey FOREIGN KEY (accepted_by_user_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_role_id_fkey FOREIGN KEY (role_id) REFERENCES public.roles(id);

ALTER TABLE ONLY public.tenant_user_invites
    ADD CONSTRAINT tenant_user_invites_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_role_id_fkey FOREIGN KEY (role_id) REFERENCES public.roles(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_webhook_id_fkey FOREIGN KEY (webhook_id) REFERENCES public.webhooks(id) ON DELETE CASCADE;

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_tenant_id_fkey FOREIGN KEY (tenant_id) REFERENCES public.tenants(id);

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
  'GateWASM Platform',
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
