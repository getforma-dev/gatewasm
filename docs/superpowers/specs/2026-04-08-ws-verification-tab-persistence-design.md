# WS Email Verification, Resend Fix, and Tab Persistence

**Date:** 2026-04-08
**Status:** Approved

## Problem

Three UX issues in the platform console:

1. After verifying email in another tab, the verification banner persists until page refresh. The WS infrastructure exists but isn't used for this.
2. The "Resend" verification email button does nothing — needs diagnosis and fix.
3. Tab state (Members, API Keys, etc.) resets to Tenants on page refresh. No URL-based persistence.

## Solution

### 1. Real-time email verification via WebSocket

**Backend (`crates/server/src/`):**

- Add `"account:updated"` to `VALID_TOPICS` in `ws/handler.rs`
- In `verify_email()` (handlers/auth.rs), after the DB update and webhook dispatch, emit a `WsEvent` via the EventBus:
  - `topic: "account:updated"`
  - `hint: Some("email_verified")`
  - `tenant_id`: the user's tenant_id (already queried for webhook dispatch)
- The event bus broadcasts to all connected clients for that tenant

**Frontend (`admin/src/platform/console/store.ts`):**

- Add subscription alongside existing WS subscriptions:
  ```ts
  socket.subscribe('account:updated', () => {
    loadCurrentUser();
  });
  ```
- `loadCurrentUser()` re-fetches from `/auth/me`, updating `currentUser` signal
- The banner's reactive condition `!currentUser()!.email_verified` automatically hides it
- Also prevents stale "Resend" button from being clicked after verification

### 2. Fix resend verification button

**Diagnosis needed:** The handler code (auth.rs lines 1908-1971) looks correct. Likely causes:
- API path resolution issue with tenant middleware
- SMTP/Mailpit connection failure not surfacing in UI
- Frontend error swallowed by catch block

**Fix:** Trace the request path, fix the root cause, confirm email arrives in Mailpit at localhost:8025.

### 3. URL hash for tab/view persistence

**Hash format:** Maps directly to signal values — `#team`, `#api-keys`, `#sessions`, `#integrate`, etc.

**Account mode examples:**
- `/platform/console` → Tenants tab (default)
- `/platform/console#team` → Members tab
- `/platform/console#api-keys` → API Keys tab
- `/platform/console#account-settings` → Settings tab

**Workspace mode examples:**
- `/platform/console/my-app` → Overview (default)
- `/platform/console/my-app#sessions` → Sessions view
- `/platform/console/my-app#security` → Security view

**State change → URL update:**
- Wrap `setAccountTab` and `setView` to call `history.replaceState` with updated hash
- `replaceState` (not `pushState`) so tab switches don't flood browser history

**URL → state restore:**
- In `init()`, after determining mode, read `location.hash` and restore matching tab/view
- Listen for `hashchange` event to handle back/forward
- Invalid or empty hash defaults to `tenants` (account) or `overview` (workspace)

**Scope:** Top-level tab and view only. Sub-sections (overview window, integrate section, plane) stay signal-only.

## Files to modify

| File | Change |
|------|--------|
| `crates/server/src/ws/handler.rs` | Add `"account:updated"` to VALID_TOPICS |
| `crates/server/src/handlers/auth.rs` | Emit WS event in `verify_email()` |
| `admin/src/platform/console/store.ts` | WS subscription, hash persistence, resend fix |
| `admin/src/platform/console/ConsoleApp.ts` | Possibly no changes (reactive banner already works) |

## Out of scope

- Sub-section persistence (overview window, integrate tab, plane selector)
- Deep-linking to specific tenant detail views
- Browser notification API for verification
