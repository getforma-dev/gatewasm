-- 004_platform_members.sql
-- Platform team management: members table, tenant assignments, invite role extension

-- ── Platform Members ────────────────────────────────────────────────────

CREATE TABLE public.platform_members (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL REFERENCES public.users(id),
    role text NOT NULL CHECK (role IN ('owner', 'admin', 'editor', 'viewer')),
    invited_by_user_id text REFERENCES public.users(id),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    last_login_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT platform_members_user_unique UNIQUE (user_id)
);

CREATE INDEX idx_platform_members_role ON public.platform_members (role);
CREATE INDEX idx_platform_members_status ON public.platform_members (status);

-- ── Platform Member Tenant Assignments ──────────────────────────────────

CREATE TABLE public.platform_member_tenants (
    id text NOT NULL PRIMARY KEY,
    member_id text NOT NULL REFERENCES public.platform_members(id) ON DELETE CASCADE,
    tenant_id text NOT NULL REFERENCES public.tenants(id) ON DELETE CASCADE,
    granted_by_user_id text NOT NULL REFERENCES public.users(id),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT platform_member_tenants_unique UNIQUE (member_id, tenant_id)
);

CREATE INDEX idx_platform_member_tenants_member ON public.platform_member_tenants (member_id);
CREATE INDEX idx_platform_member_tenants_tenant ON public.platform_member_tenants (tenant_id);

-- ── Extend owner_invites with role + tenant scope ───────────────────────

ALTER TABLE public.owner_invites ADD COLUMN IF NOT EXISTS role text NOT NULL DEFAULT 'owner';
ALTER TABLE public.owner_invites ADD COLUMN IF NOT EXISTS tenant_ids text[] NOT NULL DEFAULT '{}';
