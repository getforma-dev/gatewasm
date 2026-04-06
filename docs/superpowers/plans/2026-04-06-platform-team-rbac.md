# Platform Team Management & RBAC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add multi-user platform team management with RBAC (Owner > Admin > Editor > Viewer) and scoped tenant permissions to the fAuth console.

**Architecture:** New `platform_members` and `platform_member_tenants` tables model the team. New middleware reads platform role from these tables. Existing owner invite system extended with role + tenant scope. Frontend gets a Team tab at the account level and role-based conditional rendering throughout.

**Tech Stack:** Rust/Axum backend, PostgreSQL, FormaJS (SolidJS-like) frontend, Resend for email.

**Spec:** `docs/superpowers/specs/2026-04-06-platform-team-rbac-design.md`

---

## File Map

### Backend — Create

| File | Purpose |
|------|---------|
| `crates/server/src/db/pg_migrations/004_platform_members.sql` | New migration: platform_members, platform_member_tenants tables, alter owner_invites |
| `crates/server/src/handlers/owner/team.rs` | Team CRUD handlers (list, get, update, delete, set tenants) |
| `crates/server/src/middleware/platform_team.rs` | New middleware: require_platform_team_admin, require_platform_editor, tenant access check |

### Backend — Modify

| File | Lines | Purpose |
|------|-------|---------|
| `crates/server/src/handlers/owner/mod.rs` | 1-5 | Add `pub mod team;` |
| `crates/server/src/handlers/owner/invites.rs` | 17-19, 102-177, 211-end | Add role/tenant_ids to create, accept creates platform_members |
| `crates/server/src/handlers/mod.rs` | 496-552 | Register team routes, apply new middleware |
| `crates/server/src/handlers/onboarding.rs` | self-serve tenants endpoint | Filter tenants by member access |
| `crates/server/src/main.rs` | 228-295 | Bootstrap: create platform_members row for owner |
| `crates/server/src/email.rs` | after line 362 | Add team invite email template |
| `crates/server/src/middleware/mod.rs` | module list | Add `pub mod platform_team;` |

### Frontend — Create

| File | Purpose |
|------|---------|
| `admin/src/platform/console/views/TeamView.ts` | Team list + invite modal + member detail |

### Frontend — Modify

| File | Purpose |
|------|---------|
| `admin/src/platform/console/store.ts` | AccountTab type, platformRole signal, team API functions |
| `admin/src/platform/console/ConsoleApp.ts` | Add Team tab, conditional tab visibility |

---

## Task 1: Database Migration

**Files:**
- Create: `crates/server/src/db/pg_migrations/004_platform_members.sql`

- [ ] **Step 1: Write the migration**

```sql
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
```

- [ ] **Step 2: Verify migration loads**

Run: `cargo build 2>&1 | tail -5`

The build system embeds migrations at compile time. A successful build confirms the SQL is syntactically valid.

- [ ] **Step 3: Apply migration locally**

Run the server to trigger migration, then verify tables exist:

```bash
cargo run &
sleep 3
kill %1
```

Then check with psql or the server logs for "applied migration 004".

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/db/pg_migrations/004_platform_members.sql
git commit -m "feat(db): add platform_members and platform_member_tenants tables"
```

---

## Task 2: Platform Team Middleware

**Files:**
- Create: `crates/server/src/middleware/platform_team.rs`
- Modify: `crates/server/src/middleware/mod.rs`

- [ ] **Step 1: Create the platform team middleware module**

Read `crates/server/src/middleware/platform_user.rs` for patterns. Then create the new middleware:

```rust
// crates/server/src/middleware/platform_team.rs

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sqlx::PgPool;

use crate::jwt::claims::Claims;

/// Extracted platform member info, inserted into request extensions.
#[derive(Debug, Clone)]
pub struct PlatformMember {
    pub member_id: String,
    pub user_id: String,
    pub email: String,
    pub role: String, // owner, admin, editor, viewer
}

impl PlatformMember {
    pub fn is_owner(&self) -> bool {
        self.role == "owner"
    }

    pub fn is_admin_or_above(&self) -> bool {
        self.role == "owner" || self.role == "admin"
    }

    pub fn is_editor_or_above(&self) -> bool {
        self.role == "owner" || self.role == "admin" || self.role == "editor"
    }
}

fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "forbidden",
            "message": message,
        })),
    )
        .into_response()
}

/// Middleware: requires the caller to be an active platform member.
/// Injects `PlatformMember` into request extensions.
/// Must run AFTER `require_auth` (needs Claims in extensions).
pub async fn require_platform_member(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let claims = req
        .extensions()
        .get::<Claims>()
        .cloned()
        .ok_or_else(|| forbidden("Authentication required"))?;

    if claims.tenant_id.as_deref() != Some("ten_platform") {
        return Err(forbidden("Platform authentication required"));
    }

    let db = req
        .extensions()
        .get::<PgPool>()
        .cloned()
        .ok_or_else(|| forbidden("Internal error"))?;

    let member = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT pm.id, pm.user_id, pm.role, u.email
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.user_id = $1 AND pm.status = 'active'",
    )
    .bind(&claims.sub)
    .fetch_optional(&db)
    .await
    .map_err(|_| forbidden("Internal error"))?;

    let (member_id, user_id, role, email) = member
        .ok_or_else(|| forbidden("You are not a platform team member"))?;

    let mut req = req;
    req.extensions_mut().insert(PlatformMember {
        member_id,
        user_id,
        email,
        role,
    });

    Ok(next.run(req).await)
}

/// Middleware: requires Owner or Admin role.
/// Must run AFTER `require_platform_member`.
pub async fn require_platform_team_admin(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let member = req
        .extensions()
        .get::<PlatformMember>()
        .cloned()
        .ok_or_else(|| forbidden("Platform membership required"))?;

    if !member.is_admin_or_above() {
        return Err(forbidden("Owner or Admin role required"));
    }

    Ok(next.run(req).await)
}

/// Middleware: requires Owner, Admin, or Editor role (rejects Viewer).
/// Must run AFTER `require_platform_member`.
pub async fn require_platform_editor_or_above(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let member = req
        .extensions()
        .get::<PlatformMember>()
        .cloned()
        .ok_or_else(|| forbidden("Platform membership required"))?;

    if !member.is_editor_or_above() {
        return Err(forbidden("Editor or above role required"));
    }

    Ok(next.run(req).await)
}
```

- [ ] **Step 2: Register the middleware module**

Read `crates/server/src/middleware/mod.rs` and add:

```rust
pub mod platform_team;
```

- [ ] **Step 3: Inject PgPool into request extensions**

Check how the AppState/PgPool is made available in middleware. Read the existing middleware to understand the pattern. The `PgPool` needs to be accessible. If it's not already in extensions, we'll need to add a layer in `handlers/mod.rs` that inserts `state.db.clone()` into extensions. Check if `State(state)` is available or if we need an Extension extractor.

If PgPool is not in extensions, add a middleware layer in `handlers/mod.rs` route setup that inserts it:

```rust
.layer(axum::Extension(state.db.clone()))
```

This goes on the router alongside the existing middleware layers.

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -10`

Expected: successful compilation.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/middleware/platform_team.rs crates/server/src/middleware/mod.rs
git commit -m "feat(middleware): add platform team RBAC middleware"
```

---

## Task 3: Bootstrap — Create Owner Platform Member

**Files:**
- Modify: `crates/server/src/main.rs` (lines 228-295, `bootstrap_platform_plane`)

- [ ] **Step 1: Read the current bootstrap function**

Read `crates/server/src/main.rs` lines 228-295 to understand the existing owner bootstrapping flow.

- [ ] **Step 2: Add platform_members upsert after role assignment**

After the existing loop that assigns `role_platform_owner` to configured owner emails (around line 285), add a block that ensures each owner also has a `platform_members` row:

```rust
// After the existing role assignment loop, add:
// ── Ensure platform_members row exists for each owner ────────────
for owner_email in &config.platform_owner_emails {
    let owner_user = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE lower(email) = lower($1) AND tenant_id = 'ten_platform'"
    )
    .bind(owner_email)
    .fetch_optional(&db)
    .await;

    if let Ok(Some((user_id,))) = owner_user {
        sqlx::query(
            "INSERT INTO platform_members (id, user_id, role, status, created_at, updated_at)
             VALUES ($1, $2, 'owner', 'active', NOW(), NOW())
             ON CONFLICT (user_id) DO UPDATE SET role = 'owner', updated_at = NOW()"
        )
        .bind(format!("pmem_{}", uuid::Uuid::new_v4()))
        .bind(&user_id)
        .execute(&db)
        .await
        .ok();
        tracing::info!(email = %owner_email, "platform_members: owner row ensured");
    }
}
```

- [ ] **Step 3: Build and test**

Run: `cargo build 2>&1 | tail -5`

Start the server and verify logs show "platform_members: owner row ensured".

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/main.rs
git commit -m "feat(bootstrap): create platform_members row for owner on startup"
```

---

## Task 4: Team Invite Email Template

**Files:**
- Modify: `crates/server/src/email.rs` (after line 362)

- [ ] **Step 1: Read existing email templates**

Read `crates/server/src/email.rs` lines 319-362 (`tenant_invite_email_content`) for the pattern.

- [ ] **Step 2: Add platform team invite email template**

Add after the existing `tenant_invite_email_content` function:

```rust
/// Build the subject / text / HTML for a platform team invite email.
pub fn platform_team_invite_email_content(
    invite_url: &str,
    role: &str,
    invited_by_email: &str,
    tenant_names: &[String],
) -> (String, String, String) {
    let subject = "You've been invited to join the fAuth platform team".to_string();

    let role_display = match role {
        "admin" => "Admin",
        "editor" => "Editor",
        "viewer" => "Viewer",
        _ => role,
    };

    let tenant_line = if tenant_names.is_empty() {
        "You'll have access to all workspaces.".to_string()
    } else {
        format!(
            "You'll have access to: {}.",
            tenant_names
                .iter()
                .map(|n| escape_html(n))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let text_body = format!(
        "{} has invited you to join the fAuth platform as {}.\n\n{}\n\nAccept the invite:\n{}\n\nThis link expires in 72 hours.",
        invited_by_email, role_display, tenant_line, invite_url
    );

    let html_body = email_html_wrapper(&format!(
        "<h2 style=\"margin:0 0 8px\">You're invited to fAuth</h2>\
         <p style=\"margin:0 0 16px;color:#a0a0a0\">{} has invited you to join as <strong>{}</strong>.</p>\
         <p style=\"margin:0 0 24px;color:#a0a0a0\">{}</p>\
         {}\
         <p style=\"margin:24px 0 0;color:#666;font-size:13px\">This link expires in 72 hours.</p>",
        escape_html(invited_by_email),
        escape_html(role_display),
        tenant_line,
        email_button(invite_url, "Accept Invite"),
    ));

    (subject, text_body, html_body)
}
```

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/email.rs
git commit -m "feat(email): add platform team invite email template"
```

---

## Task 5: Team API Handlers

**Files:**
- Create: `crates/server/src/handlers/owner/team.rs`
- Modify: `crates/server/src/handlers/owner/mod.rs` (line 1-5)

- [ ] **Step 1: Add team module to owner mod.rs**

Read `crates/server/src/handlers/owner/mod.rs` lines 1-5 and add:

```rust
pub mod team;
```

- [ ] **Step 2: Create team.rs with list handler**

```rust
// crates/server/src/handlers/owner/team.rs

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::error::AppError;
use crate::middleware::platform_team::PlatformMember;
use crate::AppState;

/// GET /platform/admin/team — List all platform team members.
pub async fn list_team_members(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)>(
        "SELECT pm.id, pm.user_id, pm.role, pm.status,
                pm.invited_by_user_id,
                u.email, COALESCE(u.name, '') as name,
                pm.last_login_at, pm.created_at
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         ORDER BY
           CASE pm.role
             WHEN 'owner' THEN 0
             WHEN 'admin' THEN 1
             WHEN 'editor' THEN 2
             WHEN 'viewer' THEN 3
           END,
           pm.created_at ASC"
    )
    .fetch_all(&state.db)
    .await?;

    let mut members = Vec::new();
    for (id, user_id, role, status, invited_by, email, name, last_login_at, created_at) in rows {
        let tenant_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM platform_member_tenants WHERE member_id = $1"
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,));

        members.push(serde_json::json!({
            "id": id,
            "user_id": user_id,
            "role": role,
            "status": status,
            "invited_by_user_id": invited_by,
            "email": email,
            "name": name,
            "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
            "created_at": created_at.to_rfc3339(),
            "tenant_count": tenant_count.0,
        }));
    }

    Ok(Json(serde_json::json!({ "members": members })))
}

/// GET /platform/admin/team/:id — Get member detail with assigned tenants.
pub async fn get_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let member = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)>(
        "SELECT pm.id, pm.user_id, pm.role, pm.status,
                pm.invited_by_user_id,
                u.email, COALESCE(u.name, '') as name,
                pm.last_login_at, pm.created_at
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.id = $1"
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Team member not found".into(),
    })?;

    let (id, user_id, role, status, invited_by, email, name, last_login_at, created_at) = member;

    let tenant_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT pmt.tenant_id, t.name, t.slug
         FROM platform_member_tenants pmt
         JOIN tenants t ON t.id = pmt.tenant_id
         WHERE pmt.member_id = $1
         ORDER BY t.name"
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let tenants: Vec<serde_json::Value> = tenant_rows
        .into_iter()
        .map(|(tid, tname, tslug)| serde_json::json!({
            "id": tid,
            "name": tname,
            "slug": tslug,
        }))
        .collect();

    Ok(Json(serde_json::json!({
        "member": {
            "id": id,
            "user_id": user_id,
            "role": role,
            "status": status,
            "invited_by_user_id": invited_by,
            "email": email,
            "name": name,
            "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
            "created_at": created_at.to_rfc3339(),
            "tenants": tenants,
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberRequest {
    pub role: Option<String>,
    pub status: Option<String>,
}

/// PATCH /platform/admin/team/:id — Update member role or status.
pub async fn update_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let caller = req.extensions().get::<PlatformMember>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    let body: UpdateMemberRequest = axum::Json::<UpdateMemberRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?.0;

    let target = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, user_id, role FROM platform_members WHERE id = $1"
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "member_not_found", message: "Team member not found".into() })?;

    let (_, target_user_id, target_role) = target;

    // Cannot modify yourself
    if target_user_id == caller.user_id {
        return Err(AppError::Forbidden { error: "self_modification", message: "Cannot modify your own role".into() });
    }

    // Cannot modify the owner
    if target_role == "owner" {
        return Err(AppError::Forbidden { error: "cannot_modify_owner", message: "Cannot modify the owner".into() });
    }

    // Admin cannot modify other admins
    if target_role == "admin" && !caller.is_owner() {
        return Err(AppError::Forbidden { error: "insufficient_role", message: "Only the owner can modify admins".into() });
    }

    // Validate new role if provided
    if let Some(ref new_role) = body.role {
        if !["admin", "editor", "viewer"].contains(&new_role.as_str()) {
            return Err(AppError::BadRequest { error: "invalid_role", message: "Role must be admin, editor, or viewer".into() });
        }
        // Only owner can promote to admin
        if new_role == "admin" && !caller.is_owner() {
            return Err(AppError::Forbidden { error: "insufficient_role", message: "Only the owner can promote to admin".into() });
        }
    }

    if let Some(ref new_status) = body.status {
        if !["active", "suspended"].contains(&new_status.as_str()) {
            return Err(AppError::BadRequest { error: "invalid_status", message: "Status must be active or suspended".into() });
        }
    }

    let mut updates = Vec::new();
    let mut bind_idx = 1;

    if body.role.is_some() {
        bind_idx += 1;
        updates.push(format!("role = ${}", bind_idx));
    }
    if body.status.is_some() {
        bind_idx += 1;
        updates.push(format!("status = ${}", bind_idx));
    }

    if updates.is_empty() {
        return Err(AppError::BadRequest { error: "no_changes", message: "No fields to update".into() });
    }

    updates.push("updated_at = NOW()".to_string());
    let sql = format!("UPDATE platform_members SET {} WHERE id = $1", updates.join(", "));

    let mut query = sqlx::query(&sql).bind(&member_id);
    if let Some(ref role) = body.role {
        query = query.bind(role);
    }
    if let Some(ref status) = body.status {
        query = query.bind(status);
    }

    query.execute(&state.db).await?;

    // If role changed to owner/admin, clear tenant assignments (they see all)
    if let Some(ref new_role) = body.role {
        if new_role == "admin" {
            sqlx::query("DELETE FROM platform_member_tenants WHERE member_id = $1")
                .bind(&member_id)
                .execute(&state.db)
                .await?;
        }
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// DELETE /platform/admin/team/:id — Remove a team member.
pub async fn remove_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let caller = req.extensions().get::<PlatformMember>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    let target = sqlx::query_as::<_, (String, String)>(
        "SELECT user_id, role FROM platform_members WHERE id = $1"
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "member_not_found", message: "Team member not found".into() })?;

    let (target_user_id, target_role) = target;

    if target_role == "owner" {
        return Err(AppError::Forbidden { error: "cannot_remove_owner", message: "Cannot remove the owner".into() });
    }

    if target_user_id == caller.user_id {
        return Err(AppError::Forbidden { error: "self_removal", message: "Cannot remove yourself".into() });
    }

    if target_role == "admin" && !caller.is_owner() {
        return Err(AppError::Forbidden { error: "insufficient_role", message: "Only the owner can remove admins".into() });
    }

    // CASCADE deletes platform_member_tenants rows
    sqlx::query("DELETE FROM platform_members WHERE id = $1")
        .bind(&member_id)
        .execute(&state.db)
        .await?;

    // Also remove the platform_owner role from user_roles if present
    sqlx::query(
        "DELETE FROM user_roles WHERE user_id = $1 AND tenant_id = 'ten_platform'"
    )
    .bind(&target_user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// PUT /platform/admin/team/:id/tenants — Set tenant assignments (full replace).
pub async fn set_member_tenants(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let caller = req.extensions().get::<PlatformMember>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    #[derive(Deserialize)]
    struct Body { tenant_ids: Vec<String> }

    let body: Body = axum::Json::<Body>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: tenant_ids".into(),
        })?.0;

    // Verify member exists and is editor/viewer
    let member_role = sqlx::query_as::<_, (String,)>(
        "SELECT role FROM platform_members WHERE id = $1 AND status = 'active'"
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "member_not_found", message: "Team member not found".into() })?;

    if member_role.0 == "owner" || member_role.0 == "admin" {
        return Err(AppError::BadRequest {
            error: "not_scoped_role",
            message: "Owner and Admin see all tenants. Tenant assignments are for Editor and Viewer only.".into(),
        });
    }

    let mut tx = state.db.begin().await?;

    // Clear existing assignments
    sqlx::query("DELETE FROM platform_member_tenants WHERE member_id = $1")
        .bind(&member_id)
        .execute(&mut *tx)
        .await?;

    // Insert new assignments
    for tenant_id in &body.tenant_ids {
        sqlx::query(
            "INSERT INTO platform_member_tenants (id, member_id, tenant_id, granted_by_user_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())"
        )
        .bind(format!("pmt_{}", uuid::Uuid::new_v4()))
        .bind(&member_id)
        .bind(tenant_id)
        .bind(&caller.user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({ "success": true, "tenant_count": body.tenant_ids.len() })))
}
```

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -10`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/owner/team.rs crates/server/src/handlers/owner/mod.rs
git commit -m "feat(api): add team CRUD handlers (list, get, update, remove, set tenants)"
```

---

## Task 6: Extend Invite Endpoints

**Files:**
- Modify: `crates/server/src/handlers/owner/invites.rs`

- [ ] **Step 1: Read the current invite handlers**

Read `crates/server/src/handlers/owner/invites.rs` fully to understand the existing flow.

- [ ] **Step 2: Update CreateOwnerInviteRequest struct**

Change the struct at lines 17-20:

```rust
#[derive(Debug, Deserialize)]
pub struct CreateOwnerInviteRequest {
    pub email: String,
    pub role: String,
    pub tenant_ids: Option<Vec<String>>,
    pub expires_in_hours: Option<i64>,
}
```

- [ ] **Step 3: Update create_owner_invite to validate role, store role/tenant_ids, and send email**

In the `create_owner_invite` function (line 102+), after email validation add role validation:

```rust
    // Validate role
    let role = body.role.as_str();
    if !["admin", "editor", "viewer"].contains(&role) {
        return Err(AppError::BadRequest {
            error: "invalid_role",
            message: "Role must be admin, editor, or viewer".into(),
        });
    }

    let tenant_ids = body.tenant_ids.unwrap_or_default();

    // Editor/viewer must have at least one tenant
    if (role == "editor" || role == "viewer") && tenant_ids.is_empty() {
        return Err(AppError::BadRequest {
            error: "tenants_required",
            message: "Editor and Viewer roles require at least one tenant assignment".into(),
        });
    }

    // Admin/owner should not have tenant assignments
    let tenant_ids_clean: Vec<String> = if role == "admin" {
        vec![]
    } else {
        tenant_ids
    };

    // Check if user is already a platform member
    let existing = sqlx::query_as::<_, (String,)>(
        "SELECT pm.id FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE lower(u.email) = lower($1) AND u.tenant_id = 'ten_platform'"
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest {
            error: "already_member",
            message: "This user is already a team member".into(),
        });
    }
```

Update the INSERT query to include role and tenant_ids:

```rust
    sqlx::query(
        "INSERT INTO owner_invites
         (id, email, token_hash, invited_by_user_id, role, tenant_ids, status, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&email)
    .bind(&token_hash)
    .bind(&claims.sub)
    .bind(&body.role)
    .bind(&tenant_ids_clean)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
```

After the transaction commits, send the invite email:

```rust
    tx.commit().await?;

    let invite_url = format!(
        "{}/platform/login?owner_invite={}",
        state.config.server_url, invite_token
    );

    // Resolve tenant names for the email
    let tenant_names: Vec<String> = if !tenant_ids_clean.is_empty() {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT name FROM tenants WHERE id = ANY($1) ORDER BY name"
        )
        .bind(&tenant_ids_clean)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        rows.into_iter().map(|(n,)| n).collect()
    } else {
        vec![]
    };

    // Send invite email
    let inviter_email = &claims.sub; // We'll use the claims email if available
    let inviter = sqlx::query_as::<_, (String,)>(
        "SELECT email FROM users WHERE id = $1"
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|(e,)| e)
    .unwrap_or_else(|| "A team member".to_string());

    let (subject, text_body, html_body) = crate::email::platform_team_invite_email_content(
        &invite_url,
        &body.role,
        &inviter,
        &tenant_names,
    );

    if let Err(e) = crate::email::send_email(&state.config, &email, &subject, &text_body, &html_body).await {
        tracing::warn!(error = %e, to = %email, "Failed to send team invite email");
    }
```

Update the response to include role and tenant_ids:

```rust
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": {
                "id": invite_id,
                "email": email,
                "role": body.role,
                "tenant_ids": tenant_ids_clean,
                "status": "pending",
                "expires_at": expires_at.to_rfc3339(),
                "invited_by_user_id": claims.sub,
            },
            "invite_token": invite_token,
            "invite_url": invite_url,
        })),
    ))
```

- [ ] **Step 4: Update list_owner_invites to include role and tenant_ids**

In `list_owner_invites`, update the SELECT to include the new columns and add them to the JSON response.

- [ ] **Step 5: Update accept_owner_invite to create platform_members**

At the end of the `accept_owner_invite` function, after marking the invite as accepted, add:

```rust
    // Create platform_members row
    let member_id = format!("pmem_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO platform_members (id, user_id, role, invited_by_user_id, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'active', NOW(), NOW())
         ON CONFLICT (user_id) DO NOTHING"
    )
    .bind(&member_id)
    .bind(&platform_user.user_id)
    .bind(&invite_role) // extracted from invite row
    .bind(&invite_invited_by) // extracted from invite row
    .execute(&mut *tx)
    .await?;

    // Create tenant assignments if present
    let invite_tenant_ids: Vec<String> = sqlx::query_as::<_, (Vec<String>,)>(
        "SELECT tenant_ids FROM owner_invites WHERE id = $1"
    )
    .bind(&invite_id)
    .fetch_one(&mut *tx)
    .await
    .map(|(ids,)| ids)
    .unwrap_or_default();

    // Get the actual member_id (might exist from ON CONFLICT)
    let actual_member = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM platform_members WHERE user_id = $1"
    )
    .bind(&platform_user.user_id)
    .fetch_one(&mut *tx)
    .await?;

    for tid in &invite_tenant_ids {
        sqlx::query(
            "INSERT INTO platform_member_tenants (id, member_id, tenant_id, granted_by_user_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())
             ON CONFLICT (member_id, tenant_id) DO NOTHING"
        )
        .bind(format!("pmt_{}", uuid::Uuid::new_v4()))
        .bind(&actual_member.0)
        .bind(tid)
        .bind(&invite_invited_by)
        .execute(&mut *tx)
        .await?;
    }
```

You'll need to extract `role` and `invited_by_user_id` from the invite SELECT query. Update the query at line 255 to also select these columns.

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1 | tail -10`

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/handlers/owner/invites.rs
git commit -m "feat(api): extend invites with role, tenant_ids, email sending, and platform_members creation"
```

---

## Task 7: Self-Serve Changes — /me and Tenant Filtering

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (the self-serve handler file)

- [ ] **Step 1: Read the /me endpoint handler**

Find the handler for `GET /platform/self-serve/me` or `GET /auth/me`. Read it to understand what it currently returns.

- [ ] **Step 2: Add platform_role to /me response**

After fetching the user, look up their platform member role:

```rust
    let platform_role = sqlx::query_as::<_, (String,)>(
        "SELECT role FROM platform_members WHERE user_id = $1 AND status = 'active'"
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|(r,)| r);
```

Add `"platform_role": platform_role` to the JSON response.

- [ ] **Step 3: Filter tenants endpoint for scoped roles**

Find the handler for `GET /platform/self-serve/tenants`. Currently it returns all tenants owned by the platform user. Modify it to filter by `platform_member_tenants` for editor/viewer:

```rust
    // Check platform member role
    let member = sqlx::query_as::<_, (String, String)>(
        "SELECT id, role FROM platform_members WHERE user_id = $1 AND status = 'active'"
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;

    let tenants = if let Some((member_id, role)) = member {
        if role == "editor" || role == "viewer" {
            // Scoped: only assigned tenants
            sqlx::query_as::<_, Tenant>(
                "SELECT t.* FROM tenants t
                 JOIN platform_member_tenants pmt ON pmt.tenant_id = t.id
                 WHERE pmt.member_id = $1 AND t.id != 'ten_platform'
                 ORDER BY t.name"
            )
            .bind(&member_id)
            .fetch_all(&state.db)
            .await?
        } else {
            // Owner/Admin: all tenants (existing query)
            sqlx::query_as::<_, Tenant>(/* existing query */)
                .fetch_all(&state.db)
                .await?
        }
    } else {
        // No platform_members row yet — return existing behavior
        sqlx::query_as::<_, Tenant>(/* existing query */)
            .fetch_all(&state.db)
            .await?
    };
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat(api): add platform_role to /me, filter tenants by member access"
```

---

## Task 8: Register Routes and Apply Middleware

**Files:**
- Modify: `crates/server/src/handlers/mod.rs` (lines 496-552)

- [ ] **Step 1: Read the current route registration**

Read `crates/server/src/handlers/mod.rs` lines 496-552 to understand the existing route structure.

- [ ] **Step 2: Add team routes under /platform/admin/team**

Add the team routes alongside the existing owner routes, with the new middleware:

```rust
    // Platform admin team routes (owner + admin)
    let team_routes = Router::new()
        .route("/", axum::routing::get(owner::team::list_team_members))
        .route("/{id}", axum::routing::get(owner::team::get_team_member))
        .route("/{id}", axum::routing::patch(owner::team::update_team_member))
        .route("/{id}", axum::routing::delete(owner::team::remove_team_member))
        .route("/{id}/tenants", axum::routing::put(owner::team::set_member_tenants));
```

Nest this under the existing `/platform/admin` prefix:

```rust
    .nest("/platform/admin/team", team_routes)
```

Apply the appropriate middleware layers: `require_auth` + `require_platform_member` + `require_platform_team_admin`.

- [ ] **Step 3: Add PgPool extension layer if needed**

If the middleware needs `PgPool` from extensions (as designed in Task 2), add:

```rust
    .layer(axum::Extension(state.db.clone()))
```

to the router that uses the new middleware.

- [ ] **Step 4: Update last_login_at on platform login**

Find the token/login handler that issues platform tokens. After successful login for `ten_platform`, update `last_login_at`:

```rust
    if tenant_id == "ten_platform" {
        let _ = sqlx::query(
            "UPDATE platform_members SET last_login_at = NOW(), updated_at = NOW() WHERE user_id = $1"
        )
        .bind(&user_id)
        .execute(&db)
        .await;
    }
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -10`

Test: `curl http://localhost:3001/platform/admin/team` with a valid owner token.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/handlers/mod.rs
git commit -m "feat(routes): register team API routes with RBAC middleware"
```

---

## Task 9: Frontend Store — Types, Signals, API Functions

**Files:**
- Modify: `admin/src/platform/console/store.ts`

- [ ] **Step 1: Add types**

After the existing type definitions, add:

```typescript
export interface PlatformMember {
  id: string;
  user_id: string;
  role: 'owner' | 'admin' | 'editor' | 'viewer';
  status: 'active' | 'suspended';
  invited_by_user_id: string | null;
  email: string;
  name: string;
  last_login_at: string | null;
  created_at: string;
  tenant_count: number;
  tenants?: { id: string; name: string; slug: string }[];
}

export type PlatformRole = 'owner' | 'admin' | 'editor' | 'viewer';
```

- [ ] **Step 2: Add AccountTab 'team' and platformRole signal**

Update the `AccountTab` type (line 264):

```typescript
export type AccountTab = 'tenants' | 'api-keys' | 'account-settings' | 'team';
```

Add new signals after the existing state section:

```typescript
const [platformRole, setPlatformRole] = createSignal<PlatformRole | null>(null);
export { platformRole };
const [teamMembers, setTeamMembers] = createSignal<PlatformMember[]>([]);
export { teamMembers };
const [teamLoading, setTeamLoading] = createSignal(false);
export { teamLoading };
```

- [ ] **Step 3: Update loadCurrentUser to extract platform_role**

In the `loadCurrentUser` function, after setting the current user, also set the platform role:

```typescript
async function loadCurrentUser(): Promise<void> {
  try {
    const data = await api.get<PlatformUser & { platform_role?: PlatformRole }>('/auth/me');
    setCurrentUser(data);
    if (data.platform_role) {
      setPlatformRole(data.platform_role);
    }
  } catch {
    setCurrentUser(null);
  }
}
```

- [ ] **Step 4: Add team API functions**

```typescript
export async function loadTeamMembers(): Promise<void> {
  setTeamLoading(true);
  try {
    const data = await api.get<{ members: PlatformMember[] }>('/platform/admin/team');
    setTeamMembers(data.members);
  } catch {
    setTeamMembers([]);
  } finally {
    setTeamLoading(false);
  }
}

export async function loadTeamMemberDetail(memberId: string): Promise<PlatformMember | null> {
  try {
    const data = await api.get<{ member: PlatformMember }>('/platform/admin/team/' + encodeURIComponent(memberId));
    return data.member;
  } catch {
    return null;
  }
}

export async function updateTeamMember(memberId: string, fields: { role?: string; status?: string }): Promise<boolean> {
  try {
    await api.request('/platform/admin/team/' + encodeURIComponent(memberId), { method: 'PATCH', body: fields });
    toast.show('Member updated', 'success');
    await loadTeamMembers();
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update member', 'error');
    return false;
  }
}

export async function removeTeamMember(memberId: string): Promise<boolean> {
  try {
    await api.del('/platform/admin/team/' + encodeURIComponent(memberId));
    toast.show('Member removed', 'success');
    await loadTeamMembers();
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to remove member', 'error');
    return false;
  }
}

export async function setMemberTenants(memberId: string, tenantIds: string[]): Promise<boolean> {
  try {
    await api.request('/platform/admin/team/' + encodeURIComponent(memberId) + '/tenants', {
      method: 'PUT',
      body: { tenant_ids: tenantIds },
    });
    toast.show('Tenant access updated', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update tenant access', 'error');
    return false;
  }
}

export async function sendTeamInvite(email: string, role: string, tenantIds: string[]): Promise<{ invite_url: string } | null> {
  try {
    const data = await api.post<{ invite_url: string }>('/platform/admin/invites', {
      email,
      role,
      tenant_ids: tenantIds.length > 0 ? tenantIds : undefined,
    });
    toast.show('Invite sent to ' + email, 'success');
    await loadTeamMembers();
    return data;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send invite', 'error');
    return null;
  }
}

export async function revokeTeamInvite(inviteId: string): Promise<boolean> {
  try {
    await api.post('/platform/admin/invites/' + encodeURIComponent(inviteId) + '/revoke', {});
    toast.show('Invite revoked', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke invite', 'error');
    return false;
  }
}

export async function loadTeamInvites(): Promise<any[]> {
  try {
    const data = await api.get<{ invites: any[] }>('/platform/admin/invites');
    return data.invites;
  } catch {
    return [];
  }
}
```

- [ ] **Step 5: Add resetTenantScopedState for team signals if needed**

Not needed — team state is account-level, not tenant-scoped. It persists across tenant switches.

- [ ] **Step 6: Build**

Run: `cd admin && npx tsc --noEmit --pretty 2>&1 | head -20`

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(store): add platform team types, signals, and API functions"
```

---

## Task 10: Frontend — TeamView Component

**Files:**
- Create: `admin/src/platform/console/views/TeamView.ts`

- [ ] **Step 1: Create TeamView with team list, invite modal, and member detail**

This is a large component. Build it following the patterns in `UsersView.ts` (expandable rows, filter chips, modals). The component should:

1. Load team members + invites on mount
2. Show a filterable table of members
3. "Invite Member" button opens a modal with email, role dropdown, tenant checkboxes
4. Expandable rows show member detail with tenant assignments
5. Kebab actions: Change Role, Remove Member, Revoke Invite

```typescript
// admin/src/platform/console/views/TeamView.ts

import { h, createSignal, createShow } from 'formajs';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import {
  teamMembers,
  teamLoading,
  loadTeamMembers,
  loadTeamInvites,
  sendTeamInvite,
  revokeTeamInvite,
  updateTeamMember,
  removeTeamMember,
  setMemberTenants,
  loadTeamMemberDetail,
  tenants,
  platformRole,
  type PlatformMember,
  type PlatformRole,
} from '../store';

type RoleFilter = 'all' | 'owner' | 'admin' | 'editor' | 'viewer';

export function TeamView() {
  const [filter, setFilter] = createSignal<RoleFilter>('all');
  const [invites, setInvites] = createSignal<any[]>([]);
  const [showInviteModal, setShowInviteModal] = createSignal(false);
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [expandedDetail, setExpandedDetail] = createSignal<PlatformMember | null>(null);

  // Load data on mount
  void loadTeamMembers();
  void loadTeamInvites().then(setInvites);

  const filtered = () => {
    const f = filter();
    const members = teamMembers();
    if (f === 'all') return members;
    return members.filter((m) => m.role === f);
  };

  const pendingInvites = () => invites().filter((inv) => inv.status === 'pending');

  const totalCount = () => teamMembers().length + pendingInvites().length;

  async function handleExpand(member: PlatformMember) {
    if (expandedId() === member.id) {
      setExpandedId(null);
      setExpandedDetail(null);
      return;
    }
    setExpandedId(member.id);
    const detail = await loadTeamMemberDetail(member.id);
    setExpandedDetail(detail);
  }

  async function handleRevokeInvite(inviteId: string) {
    if (await revokeTeamInvite(inviteId)) {
      setInvites(invites().filter((inv) => inv.id !== inviteId));
    }
  }

  async function handleRemoveMember(member: PlatformMember) {
    if (await removeTeamMember(member.id)) {
      if (expandedId() === member.id) {
        setExpandedId(null);
        setExpandedDetail(null);
      }
    }
  }

  async function handleChangeRole(member: PlatformMember, newRole: string) {
    await updateTeamMember(member.id, { role: newRole });
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'Team'),
        h('p', { class: 'view-subtitle text-muted' }, () => totalCount() + ' member' + (totalCount() !== 1 ? 's' : '')),
      ),
      h('div', { class: 'view-header-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-primary',
          onClick: () => setShowInviteModal(true),
        }, '+ Invite Member'),
      ),
    ),

    // Filter chips
    h('div', { class: 'filter-bar' },
      ...(['all', 'owner', 'admin', 'editor', 'viewer'] as RoleFilter[]).map((f) =>
        h('button', {
          type: 'button',
          class: () => 'filter-chip' + (filter() === f ? ' active' : ''),
          onClick: () => setFilter(f),
        }, f === 'all' ? 'All' : f.charAt(0).toUpperCase() + f.slice(1)),
      ),
    ),

    // Pending invites section
    createShow(
      () => pendingInvites().length > 0,
      () => h('div', { class: 'console-card' },
        h('div', { class: 'card-label' }, 'Pending Invites'),
        h('div', { class: 'team-list' },
          () => pendingInvites().map((inv) =>
            h('div', { class: 'team-row team-row-pending' },
              h('div', { class: 'team-row-info' },
                h('span', { class: 'team-row-email' }, inv.email),
                h('span', { class: 'team-role-badge role-' + inv.role }, inv.role),
                h('span', { class: 'team-status-badge pending' }, 'Pending'),
              ),
              h('div', { class: 'team-row-actions' },
                h('button', {
                  type: 'button',
                  class: 'btn btn-sm btn-ghost btn-danger',
                  onClick: () => handleRevokeInvite(inv.id),
                }, 'Revoke'),
              ),
            ),
          ),
        ),
      ),
    ),

    // Members list
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Team Members'),
      createShow(
        () => !teamLoading(),
        () => h('div', { class: 'team-list' },
          () => filtered().map((member) =>
            h('div', { class: () => 'team-member-shell' + (expandedId() === member.id ? ' expanded' : '') },
              h('div', {
                class: 'team-row hv',
                onClick: () => handleExpand(member),
              },
                h('div', { class: 'team-row-info' },
                  h('span', { class: 'team-row-name' }, member.name || member.email),
                  member.name ? h('span', { class: 'team-row-email text-muted' }, member.email) : null,
                  h('span', { class: 'team-role-badge role-' + member.role }, member.role),
                ),
                h('div', { class: 'team-row-meta' },
                  h('span', { class: 'text-muted' }, member.last_login_at
                    ? 'Last login: ' + new Date(member.last_login_at).toLocaleDateString()
                    : 'Never logged in'),
                  member.role !== 'owner'
                    ? h('div', { class: 'team-row-actions' },
                        RoleDropdown(member, platformRole()!, (r) => handleChangeRole(member, r)),
                        h('button', {
                          type: 'button',
                          class: 'btn btn-sm btn-ghost btn-danger',
                          onClick: (e: Event) => { e.stopPropagation(); handleRemoveMember(member); },
                        }, 'Remove'),
                      )
                    : null,
                ),
              ),
              // Expanded detail
              createShow(
                () => expandedId() === member.id && expandedDetail() != null,
                () => MemberDetail(expandedDetail()!, () => loadTeamMemberDetail(member.id).then(setExpandedDetail)),
              ),
            ),
          ),
        ),
        () => h('div', { class: 'team-loading' }, 'Loading team...'),
      ),
    ),

    // Invite modal
    createShow(
      () => showInviteModal(),
      () => InviteModal(
        () => setShowInviteModal(false),
        async () => {
          const inv = await loadTeamInvites();
          setInvites(inv);
        },
      ),
    ),
  );
}

function RoleDropdown(member: PlatformMember, callerRole: PlatformRole, onChange: (role: string) => void) {
  const canPromoteToAdmin = callerRole === 'owner';
  const options = canPromoteToAdmin
    ? ['admin', 'editor', 'viewer']
    : ['editor', 'viewer'];

  return h('select', {
    class: 'team-role-select',
    value: member.role,
    onClick: (e: Event) => e.stopPropagation(),
    onChange: (e: Event) => onChange((e.target as HTMLSelectElement).value),
  },
    ...options.map((r) =>
      h('option', { value: r, selected: member.role === r }, r.charAt(0).toUpperCase() + r.slice(1)),
    ),
  );
}

function MemberDetail(member: PlatformMember, refresh: () => void) {
  const assignedTenants = () => member.tenants || [];

  return h('div', { class: 'team-member-detail' },
    h('div', { class: 'detail-grid' },
      DetailRow('Email', member.email),
      DetailRow('Role', member.role),
      DetailRow('Joined', new Date(member.created_at).toLocaleDateString()),
      DetailRow('Last Login', member.last_login_at ? new Date(member.last_login_at).toLocaleString() : 'Never'),
    ),
    (member.role === 'editor' || member.role === 'viewer')
      ? h('div', { class: 'team-tenant-access' },
          h('div', { class: 'card-label' }, 'Tenant Access'),
          assignedTenants().length === 0
            ? h('p', { class: 'text-muted' }, 'No tenants assigned')
            : h('div', { class: 'team-tenant-list' },
                ...assignedTenants().map((t) =>
                  h('span', { class: 'team-tenant-chip' }, t.name + ' (/t/' + t.slug + ')'),
                ),
              ),
          TenantAssigner(member, refresh),
        )
      : null,
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}

function TenantAssigner(member: PlatformMember, onSave: () => void) {
  const allTenants = tenants();
  const assigned = new Set((member.tenants || []).map((t) => t.id));
  const [selected, setSelected] = createSignal<Set<string>>(assigned);
  const [saving, setSaving] = createSignal(false);

  async function save() {
    setSaving(true);
    await setMemberTenants(member.id, [...selected()]);
    onSave();
    setSaving(false);
  }

  return h('div', { class: 'team-tenant-assigner' },
    h('div', { class: 'team-tenant-checkboxes' },
      ...allTenants.map((t) =>
        h('label', { class: 'team-tenant-checkbox' },
          h('input', {
            type: 'checkbox',
            checked: selected().has(t.id),
            onChange: () => {
              const next = new Set(selected());
              if (next.has(t.id)) next.delete(t.id);
              else next.add(t.id);
              setSelected(next);
            },
          }),
          t.name,
        ),
      ),
    ),
    h('button', {
      type: 'button',
      class: 'btn btn-sm btn-primary',
      disabled: saving,
      onClick: save,
    }, () => saving() ? 'Saving...' : 'Update Access'),
  );
}

function InviteModal(onClose: () => void, onInvited: () => void) {
  const [email, setEmail] = createSignal('');
  const [role, setRole] = createSignal('editor');
  const [selectedTenants, setSelectedTenants] = createSignal<Set<string>>(new Set());
  const [sending, setSending] = createSignal(false);
  const [inviteUrl, setInviteUrl] = createSignal<string | null>(null);

  const needsTenants = () => role() === 'editor' || role() === 'viewer';
  const allTenants = tenants();

  async function submit() {
    if (!email().trim()) return;
    if (needsTenants() && selectedTenants().size === 0) return;

    setSending(true);
    const result = await sendTeamInvite(
      email().trim(),
      role(),
      needsTenants() ? [...selectedTenants()] : [],
    );
    setSending(false);

    if (result) {
      setInviteUrl(result.invite_url);
      onInvited();
    }
  }

  return h('div', { class: 'modal-overlay', onClick: (e: Event) => { if (e.target === e.currentTarget) onClose(); } },
    h('div', { class: 'modal-card' },
      createShow(
        () => !inviteUrl(),
        // Invite form
        () => h('div', null,
          h('h3', { class: 'modal-title' }, 'Invite Team Member'),
          h('div', { class: 'modal-field' },
            h('label', null, 'Email'),
            h('input', {
              type: 'email',
              class: 'field-input',
              placeholder: 'colleague@example.com',
              value: email(),
              onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
            }),
          ),
          h('div', { class: 'modal-field' },
            h('label', null, 'Role'),
            h('select', {
              class: 'field-input',
              value: role(),
              onChange: (e: Event) => setRole((e.target as HTMLSelectElement).value),
            },
              platformRole() === 'owner' ? h('option', { value: 'admin' }, 'Admin') : null,
              h('option', { value: 'editor' }, 'Editor'),
              h('option', { value: 'viewer' }, 'Viewer'),
            ),
          ),
          createShow(
            needsTenants,
            () => h('div', { class: 'modal-field' },
              h('label', null, 'Tenant Access'),
              h('div', { class: 'team-tenant-checkboxes' },
                ...allTenants.map((t) =>
                  h('label', { class: 'team-tenant-checkbox' },
                    h('input', {
                      type: 'checkbox',
                      checked: selectedTenants().has(t.id),
                      onChange: () => {
                        const next = new Set(selectedTenants());
                        if (next.has(t.id)) next.delete(t.id);
                        else next.add(t.id);
                        setSelectedTenants(next);
                      },
                    }),
                    t.name + ' (/t/' + t.slug + ')',
                  ),
                ),
              ),
            ),
          ),
          h('div', { class: 'modal-actions' },
            h('button', { type: 'button', class: 'btn btn-ghost', onClick: onClose }, 'Cancel'),
            h('button', {
              type: 'button',
              class: 'btn btn-primary',
              disabled: () => sending() || !email().trim() || (needsTenants() && selectedTenants().size === 0),
              onClick: submit,
            }, () => sending() ? 'Sending...' : 'Send Invite'),
          ),
        ),
        // Success state with copyable link
        () => h('div', null,
          h('h3', { class: 'modal-title' }, 'Invite Sent'),
          h('p', { class: 'card-copy' }, 'An email has been sent to ' + email() + '. You can also share this link directly:'),
          h('div', { class: 'invite-url-row' },
            h('code', { class: 'invite-url-code' }, inviteUrl()),
            CopyButton({ text: () => inviteUrl()!, title: 'Copy invite URL' }),
          ),
          h('div', { class: 'modal-actions' },
            h('button', { type: 'button', class: 'btn btn-primary', onClick: onClose }, 'Done'),
          ),
        ),
      ),
    ),
  );
}
```

- [ ] **Step 2: Build**

Run: `cd admin && npx tsc --noEmit --pretty 2>&1 | head -20`

Fix any type errors. The component follows the same patterns as UsersView.ts and OrganizationsView.ts.

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console/views/TeamView.ts
git commit -m "feat(ui): add TeamView with member list, invite modal, member detail"
```

---

## Task 11: Frontend — Wire Team Tab into ConsoleApp

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`

- [ ] **Step 1: Import TeamView and platformRole**

Add to the imports at the top of ConsoleApp.ts:

```typescript
import { TeamView } from './views/TeamView';
```

And add `platformRole` to the store import (line 25-32):

```typescript
  platformRole,
```

- [ ] **Step 2: Add Team tab button (conditionally visible)**

Find the tab buttons (around line 437-439). Add the Team tab, visible only to owner/admin:

```typescript
      AccountTabBtn('tenants', 'Tenants'),
      platformRole() === 'owner' || platformRole() === 'admin'
        ? AccountTabBtn('team', 'Team')
        : null,
      platformRole() === 'owner' || platformRole() === 'admin'
        ? AccountTabBtn('api-keys', 'API Keys')
        : null,
      AccountTabBtn('account-settings', 'Settings'),
```

Wait — this has the same non-reactive issue we just fixed. Use `createShow` instead:

```typescript
      AccountTabBtn('tenants', 'Tenants'),
      createShow(
        () => platformRole() === 'owner' || platformRole() === 'admin',
        () => AccountTabBtn('team', 'Team'),
      ),
      createShow(
        () => platformRole() === 'owner' || platformRole() === 'admin',
        () => AccountTabBtn('api-keys', 'API Keys'),
      ),
      AccountTabBtn('account-settings', 'Settings'),
```

- [ ] **Step 3: Add Team tab to createSwitch**

Find the `createSwitch` for account tabs (around line 444-450). Add the team case:

```typescript
      createSwitch(
        accountTab,
        [
          { match: 'tenants', render: () => TenantsList() },
          { match: 'team', render: () => TeamView() },
          { match: 'api-keys', render: () => ApiKeysView() },
          { match: 'account-settings', render: () => AccountSettingsView() },
        ],
      ),
```

- [ ] **Step 4: Build and verify**

Run: `cd admin && npx tsc --noEmit --pretty 2>&1 | head -20`

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/ConsoleApp.ts
git commit -m "feat(ui): wire Team tab into ConsoleApp with role-based visibility"
```

---

## Task 12: Frontend — Invite Acceptance Flow

**Files:**
- Modify: `admin/src/platform/console/store.ts` (init function)

- [ ] **Step 1: Read the init function**

Read the `init()` function in store.ts (around line 1380+).

- [ ] **Step 2: Add invite acceptance to init**

After `auth.silentRefresh()` and loading tenants/user, check for `owner_invite` query param:

```typescript
    // Check for pending invite acceptance
    const inviteParam = new URLSearchParams(window.location.search).get('owner_invite');
    if (inviteParam) {
      try {
        await api.post('/platform/admin/invites/accept', { token: inviteParam });
        toast.show('Invite accepted! Welcome to the team.', 'success');
      } catch (e) {
        const msg = e instanceof Error ? e.message : 'Failed to accept invite';
        toast.show(msg, 'error');
      }
      // Clean the URL
      const url = new URL(window.location.href);
      url.searchParams.delete('owner_invite');
      history.replaceState(null, '', url.pathname + url.search);

      // Reload user data to get fresh platform_role
      await Promise.all([loadTenants(), loadCurrentUser()]);
    }
```

Insert this after the initial `await Promise.all([loadTenants(), loadCurrentUser()])` call and before the URL routing logic.

- [ ] **Step 3: Build**

Run: `cd admin && npx tsc --noEmit --pretty 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(ui): handle invite acceptance in console init flow"
```

---

## Task 13: Frontend — Viewer Read-Only Enforcement

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`

- [ ] **Step 1: Hide create tenant button for viewers**

Find the "New Workspace" button in `TenantsList`. Wrap it in a `createShow` that checks `platformRole()`:

```typescript
createShow(
  () => platformRole() === 'owner' || platformRole() === 'admin',
  () => h('button', { ... }, '+ New Workspace'),
),
```

- [ ] **Step 2: Disable workspace actions for viewers in workspace mode**

In the workspace shell, viewers should not see destructive action buttons. The `platformRole()` signal is already available. For individual workspace views (OverviewView, etc.), the viewer enforcement is primarily server-side (403 on mutations). The UI can hide action buttons by checking `platformRole()` where needed.

For now, add a general pattern:

```typescript
export function canEdit(): boolean {
  const role = platformRole();
  return role === 'owner' || role === 'admin' || role === 'editor';
}
```

Add this to the store as a derived helper. Views can use `canEdit()` to conditionally show/hide edit buttons.

- [ ] **Step 3: Build and verify**

Run: `cd admin && npx tsc --noEmit --pretty 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/ConsoleApp.ts admin/src/platform/console/store.ts
git commit -m "feat(ui): viewer read-only enforcement with role-based button visibility"
```

---

## Task 14: CSS — Team View Styles

**Files:**
- Modify: `admin/src/platform/console/` CSS file (find the existing console styles)

- [ ] **Step 1: Find the console CSS file**

Search for where console-specific styles live. Add team-specific styles:

```css
/* Team view */
.team-list { display: flex; flex-direction: column; gap: 2px; }
.team-row { display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; border-radius: 8px; cursor: pointer; }
.team-row.hv:hover { background: var(--surface-hover); }
.team-row-pending { opacity: 0.7; cursor: default; }
.team-row-info { display: flex; align-items: center; gap: 12px; }
.team-row-name { font-weight: 500; }
.team-row-email { font-size: 13px; }
.team-row-meta { display: flex; align-items: center; gap: 12px; font-size: 13px; }
.team-row-actions { display: flex; gap: 8px; }
.team-role-badge { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; padding: 2px 8px; border-radius: 4px; background: var(--surface-2); }
.team-role-badge.role-owner { color: var(--accent); }
.team-role-badge.role-admin { color: var(--blue); }
.team-status-badge.pending { color: var(--yellow); }
.team-member-shell.expanded { background: var(--surface-1); border-radius: 8px; }
.team-member-detail { padding: 0 16px 16px; }
.team-tenant-access { margin-top: 16px; }
.team-tenant-list { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.team-tenant-chip { font-size: 13px; padding: 4px 10px; border-radius: 6px; background: var(--surface-2); }
.team-tenant-assigner { margin-top: 12px; }
.team-tenant-checkboxes { display: flex; flex-direction: column; gap: 6px; margin: 8px 0; }
.team-tenant-checkbox { display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer; }
.team-role-select { font-size: 13px; padding: 4px 8px; border-radius: 4px; background: var(--surface-2); border: 1px solid var(--border); color: inherit; }
.invite-url-row { display: flex; align-items: center; gap: 8px; margin: 12px 0; }
.invite-url-code { font-size: 12px; padding: 8px; border-radius: 6px; background: var(--surface-2); word-break: break-all; flex: 1; }
```

- [ ] **Step 2: Commit**

```bash
git add <css-file>
git commit -m "feat(css): add team view styles"
```

---

## Task 15: Integration Test — End-to-End Verify

- [ ] **Step 1: Start the server**

```bash
cargo run
```

- [ ] **Step 2: Verify bootstrap created owner platform_members row**

Check server logs for "platform_members: owner row ensured".

- [ ] **Step 3: Verify /me includes platform_role**

```bash
curl -H "Authorization: Bearer <token>" http://localhost:3001/auth/me | jq .platform_role
```

Expected: `"owner"`

- [ ] **Step 4: Verify Team tab appears in console**

Open `http://localhost:3001/platform/console`. Verify Team tab appears between Tenants and API Keys. Click it. Should show the owner as the sole member.

- [ ] **Step 5: Test invite flow**

1. Click "Invite Member"
2. Enter an email, select Editor, check one tenant
3. Submit — should show invite URL
4. Check server logs for Resend email delivery

- [ ] **Step 6: Verify tenant filtering**

Create a second platform user (via onboarding), accept an editor invite with scoped tenants. Verify they only see assigned tenants in the Tenants list.

- [ ] **Step 7: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration test adjustments"
```
