# Platform Team Management & RBAC

Multi-user platform team management with role-based access control for the fAuth console. Allows platform owners to invite team members with granular roles and scoped tenant permissions.

## Roles

Clean hierarchy: **Owner > Admin > Editor > Viewer**.

| Role | Scope | Description |
|------|-------|-------------|
| Owner | All tenants | Singleton. Full control. Only role that can manage admins, delete tenants, manage billing. Cannot be removed or downgraded. |
| Admin | All tenants | Manage all tenants, invite editors/viewers, manage non-admin members. Cannot manage other admins or delete tenants. |
| Editor | Assigned tenants | Edit settings, view stats, manage end users on assigned tenants only. Cannot create/delete tenants. |
| Viewer | Assigned tenants | Read-only access to assigned tenants. Cannot change anything. |

## Data Model

### New table: `platform_members`

```sql
CREATE TABLE public.platform_members (
    id text NOT NULL PRIMARY KEY,
    user_id text NOT NULL REFERENCES users(id),
    role text NOT NULL CHECK (role IN ('owner', 'admin', 'editor', 'viewer')),
    invited_by_user_id text REFERENCES users(id),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    last_login_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT platform_members_user_unique UNIQUE (user_id)
);
```

### New table: `platform_member_tenants`

Junction table for editor/viewer tenant assignments. Owner/admin see all tenants implicitly.

```sql
CREATE TABLE public.platform_member_tenants (
    id text NOT NULL PRIMARY KEY,
    member_id text NOT NULL REFERENCES platform_members(id) ON DELETE CASCADE,
    tenant_id text NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    granted_by_user_id text NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT platform_member_tenants_unique UNIQUE (member_id, tenant_id)
);
```

### Changes to existing `owner_invites` table

Add two columns:

```sql
ALTER TABLE owner_invites ADD COLUMN role text NOT NULL DEFAULT 'owner';
ALTER TABLE owner_invites ADD COLUMN tenant_ids text[] NOT NULL DEFAULT '{}';
```

`role` is the role being granted. `tenant_ids` holds tenant IDs for scoped roles (editor/viewer); empty for owner/admin.

### Bootstrap

On first platform login (`bootstrap_platform_plane`), insert a `platform_members` row with `role = 'owner'` for the configured owner email. This makes the existing owner a member of their own team. The existing `user_roles` entry for `role_platform_owner` is kept for backward compat with the existing `require_platform_owner` middleware; `platform_members` becomes the authoritative source for platform RBAC going forward.

## API Endpoints

### Team Management

All require Owner or Admin role (via middleware).

| Method | Path | Description | Who |
|--------|------|-------------|-----|
| `GET` | `/platform/admin/team` | List all members: id, name, email, role, status, last_login_at, tenant count | Owner, Admin |
| `GET` | `/platform/admin/team/:id` | Member detail: profile + assigned tenants | Owner, Admin |
| `PATCH` | `/platform/admin/team/:id` | Update role or status. Owner-only for changing admins. | Owner, Admin (scoped) |
| `DELETE` | `/platform/admin/team/:id` | Remove member. Owner-only for removing admins. Owner cannot be removed. | Owner, Admin (scoped) |
| `PUT` | `/platform/admin/team/:id/tenants` | Set tenant assignments for editor/viewer. Body: `{ "tenant_ids": [...] }`. Replaces all existing assignments. | Owner, Admin |

### Invite Changes

Extend existing endpoints:

| Method | Path | Changes |
|--------|------|---------|
| `POST` | `/platform/admin/invites` | Add `role` (required) and `tenant_ids` (required for editor/viewer) to request body. Send invite email via Resend. Return invite link. |
| `GET` | `/platform/admin/invites` | Include `role` and `tenant_ids` in response. |
| `POST` | `/platform/admin/invites/accept` | On accept: create `platform_members` row with invite's role, create `platform_member_tenants` rows for any assigned tenants, mark invite as accepted. |

Revoke endpoint unchanged.

### Self-Serve Changes

| Method | Path | Changes |
|--------|------|---------|
| `GET` | `/platform/self-serve/me` | Add `platform_role` field to response (owner/admin/editor/viewer). |
| `GET` | `/platform/self-serve/tenants` | Filter by tenant assignments for editor/viewer. Owner/admin continue to see all. |

### Middleware

New middleware reads from `platform_members` table (not `user_roles`). The existing `require_platform_owner` middleware and `role_platform_owner` in `user_roles` remain unchanged for backward compat — new middleware sits alongside it.

- `require_platform_team_admin`: requires `platform_members.role` of `owner` or `admin`. Used on `/platform/admin/team/*` and `/platform/admin/invites/*` routes.
- `require_platform_editor_or_above`: requires `platform_members.role` of `owner`, `admin`, or `editor`. Guards all self-serve mutation endpoints (reject viewers with 403).
- Tenant-level access extractor: for self-serve routes that take a tenant ID, verify the caller has access (owner/admin → all tenants; editor/viewer → check `platform_member_tenants`). Return 403 if not.
- Update `last_login_at` on `platform_members` during platform login flow.

## Authorization Matrix

| Action | Owner | Admin | Editor | Viewer |
|--------|-------|-------|--------|--------|
| See Team tab | yes | yes | no | no |
| Invite admin | yes | no | no | no |
| Invite editor/viewer | yes | yes | no | no |
| Change role to admin | yes | no | no | no |
| Change role to editor/viewer | yes | yes | no | no |
| Remove admin | yes | no | no | no |
| Remove editor/viewer | yes | yes | no | no |
| Edit tenant assignments | yes | yes | no | no |
| Create tenants | yes | yes | no | no |
| Delete tenants | yes | no | no | no |
| Edit tenant settings | yes | yes | assigned | no |
| View tenant stats/members | yes | yes | assigned | assigned |
| Manage API keys | yes | yes | no | no |
| Access account settings | yes | yes | own profile | own profile |

Key rules:
- Cannot change your own role.
- Can only manage roles below your own.
- Owner is singleton; cannot be removed or downgraded.
- Editor/Viewer never see Team tab or API Keys tab.

## Frontend

### Team Tab

New account-level tab at `/platform/console`, visible only to Owner and Admin. Sits alongside Tenants, API Keys, Settings.

**Team list view:**
- Header with member count and "Invite Member" primary CTA.
- Table: Name, Email, Role (badge), Status, Last Login, Actions (kebab menu).
- Filter chips: All, Owner, Admin, Editor, Viewer.
- Pending invites shown inline with "Pending" badge and "Revoke" action.

**Invite modal:**
- Email field.
- Role dropdown: Admin, Editor, Viewer (Owner not selectable).
- If Editor or Viewer: tenant multi-select with checkboxes.
- Sends email via Resend + shows copyable invite link on success.
- Validation: email required, role required, at least one tenant for editor/viewer.

**Member detail (expandable row):**
- Profile: name, email, role badge, joined date, last login.
- For Editor/Viewer: "Tenant Access" section with assigned tenants, add/remove.
- Login history: recent logins (timestamp, IP, user agent) using existing login history API.

### Console-Wide Changes

- `platformRole()` signal derived from `GET /platform/self-serve/me` response.
- Tenants list: filtered server-side for editor/viewer.
- Workspace views: viewer gets disabled action buttons via `createShow`/conditional rendering.
- PlaneNav tenant dropdown: only shows assigned tenants for editor/viewer.
- Account tabs: editor/viewer see only Tenants and Settings (own profile).
- Editor on unassigned tenant URL: API returns 403, frontend redirects to `/platform/console`.

## Invite Acceptance Flow

1. Owner/Admin creates invite → email sent via Resend with invite link.
2. Invitee clicks link → `/platform/login?owner_invite={token}`.
3. If no account: register via `/platform/onboarding`, `owner_invite` param preserved.
4. If existing account: sign in normally.
5. After auth completes, the platform console `init()` detects the `owner_invite` query param and calls `POST /platform/admin/invites/accept`.
6. Backend validates: token exists, not expired/revoked, email matches signed-in user.
7. Backend creates: `platform_members` row + `platform_member_tenants` rows.
8. Frontend redirects to `/platform/console`.

**Edge cases:**
- Expired invite: error message, suggest contacting inviter.
- Email mismatch: "This invite was sent to a different email address."
- Already accepted: no-op, redirect to console.
- Already a member: blocked at invite creation time ("This user is already a team member").

## Viewer Read-Only Enforcement

**API:** Single `require_platform_editor_or_above` middleware on all self-serve mutation endpoints. Viewers get 403.

**UI:** `platformRole()` signal drives conditional rendering. Same components, buttons disabled/hidden for viewer. No separate component tree.

**Tenant access:** Editor/viewer accessing unassigned tenant returns 403 from API. Frontend redirects to console home.
