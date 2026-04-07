-- 004_platform_members.sql
-- Per-customer team management: members scoped by account_owner, per-tenant roles

-- ── Platform Members ────────────────────────────────────────────────────
-- Each row is scoped to a customer's team via account_owner_user_id.
-- Ownership is structural: account_owner_user_id == user_id means "I am the account owner".
-- Invited members have account_owner_user_id pointing to the inviter's account.

CREATE TABLE public.platform_members (
    id text NOT NULL PRIMARY KEY,
    account_owner_user_id text NOT NULL REFERENCES public.users(id),
    user_id text NOT NULL REFERENCES public.users(id),
    invited_by_user_id text REFERENCES public.users(id),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    last_login_at timestamp with time zone,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT platform_members_account_user_unique UNIQUE (account_owner_user_id, user_id)
);

CREATE INDEX idx_platform_members_account_owner ON public.platform_members (account_owner_user_id);
CREATE INDEX idx_platform_members_user ON public.platform_members (user_id);
CREATE INDEX idx_platform_members_status ON public.platform_members (status);

-- ── Platform Member Tenant Assignments ──────────────────────────────────
-- Each assignment carries a per-tenant role: admin, editor, or viewer.
-- Account owners do NOT need rows here — they have full access to their own tenants.

CREATE TABLE public.platform_member_tenants (
    id text NOT NULL PRIMARY KEY,
    member_id text NOT NULL REFERENCES public.platform_members(id) ON DELETE CASCADE,
    tenant_id text NOT NULL REFERENCES public.tenants(id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('admin', 'editor', 'viewer')),
    granted_by_user_id text NOT NULL REFERENCES public.users(id),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT platform_member_tenants_unique UNIQUE (member_id, tenant_id)
);

CREATE INDEX idx_platform_member_tenants_member ON public.platform_member_tenants (member_id);
CREATE INDEX idx_platform_member_tenants_tenant ON public.platform_member_tenants (tenant_id);

-- ── Extend owner_invites with role + tenant scope + account context ─────

ALTER TABLE public.owner_invites ADD COLUMN IF NOT EXISTS role text NOT NULL DEFAULT 'editor';
ALTER TABLE public.owner_invites ADD COLUMN IF NOT EXISTS tenant_ids text[] NOT NULL DEFAULT '{}';
ALTER TABLE public.owner_invites ADD COLUMN IF NOT EXISTS account_owner_user_id text REFERENCES public.users(id);

-- Fix: the global unique index only allowed one pending invite per email across ALL accounts.
-- With per-account scoping, two different account owners should be able to invite the same email.
DROP INDEX IF EXISTS uq_owner_invites_pending_email;
CREATE UNIQUE INDEX uq_owner_invites_pending_email_account
  ON public.owner_invites (lower(email), account_owner_user_id)
  WHERE (status = 'pending');
