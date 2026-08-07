# SSR Migration Plan — Phase 2 Everywhere

Convert all CSR (`mount()`) pages to the island architecture (SSR page component + `activateIslands` hydration) for zero-flash rendering, consistent UX, and to showcase the full Forma SSR pipeline.

---

## Status

| Route | Page Component | Islands | Serving SSR? |
|-------|---------------|---------|---|
| `/platform/benchmark` | `BenchmarkPage.ts` | FilterBar, BenchmarkDataTable, PerfPanel, PerfStat | **Yes** |
| `/platform/login` | `LoginPage.ts` | AuthCard, MfaPanel | No — see below |
| `/platform/onboarding` | `OnboardingPage.ts` | RegistrationForm | No — see below |

These use the dual-mode `app.ts` pattern: `activateIslands` when SSR markers exist, `mount()` fallback for dev mode.

**The island work being done is not the same as the route serving correct SSR.**
`/platform/login` and `/platform/onboarding` have their components split, but
their server-rendered markup is wrong and they are held on Phase 1:

- `/platform/login` renders without its `a[href="/platform/onboarding"]` link.
  The client render produces it; the SSR walk does not.
- `/platform/onboarding` emits `<svg width="">`, which the browser reports as
  `Unexpected end of attribute. Expected length, ""`.

Both were caught by `admin/e2e/smoke.spec.ts` and `login.spec.ts` the first time
the e2e suite was able to run. Fix the SSR output before opting either route in.

---

## Migration Order

Start with the simplest page, build confidence with the pattern, then tackle complex ones.

### 1. Tenant Login (`/t/:slug/login`) — Low complexity

**Current:** `mount(() => TenantLoginCard(), '#app')`

**Plan:**
- Create `src/tenant/login/LoginPage.ts` — static shell (`.tl-page` wrapper, logo area, powered-by footer)
- Extract interactive parts into island: `TenantAuthForm.ts` (form, mode switching, OAuth buttons, alerts)
- Update `app.ts` to dual-mode pattern
- The `.tl-page` centering wrapper moves into the SSR page component (static, rendered server-side)

**Islands:** TenantAuthForm (1 island)

**Server handler:** `hosted_login.rs` — already calls `render_page` with `PageConfig`, just needs IR module loading

---

### 2. Tenant Dashboard (`/t/:slug/dashboard`) — Medium complexity

**Current:** `mount(() => TenantDashboard(), '#app')`

**Plan:**
- Create `src/tenant/dashboard/DashboardPage.ts` — static shell (sidebar/nav, page structure)
- Extract islands: ProfilePanel, IntegrationPanel, SettingsPanel (or however the tabs are structured)
- Update `app.ts` to dual-mode pattern

**Islands:** ~3 islands (one per tab/section)

**Server handler:** `dashboard.rs` — already calls `render_page`

---

### 3. Tenant Admin (`/t/:slug/admin`) — Medium-High complexity

**Current:** `mount(() => TenantAdmin(), '#app')`

**Plan:**
- Create `src/tenant/admin/AdminPage.ts` — static shell (sidebar with nav items, page header)
- Extract islands: UserList, RoleManager, WebhookManager, AuditLog (one per admin section)
- Each island is self-contained with its own data fetching

**Islands:** ~4 islands

**Server handler:** `dashboard.rs` (admin_console_page) — already calls `render_page`

---

### 4. Platform Console (`/platform/console`) — Medium-High complexity

**Current:** `mount(() => ConsoleApp(), '#app')`

**Plan:**
- Create `src/platform/console/ConsolePage.ts` — static shell (sidebar, quickstart banner placeholder)
- Extract islands: TenantList, TenantDetail, CreateTenantForm, Quickstart
- View switching (list → detail → create) stays client-side within islands

**Islands:** ~4 islands

**Server handler:** `platform_dashboard.rs` — already calls `render_page`

---

### 5. Owner Dashboard (`/platform/admin/dashboard`) — High complexity

**Current:** `mount(() => OwnerApp(), '#app')`

**Plan:**
- Create `src/admin/dashboard/DashboardPage.ts` — static shell (sidebar with 7 nav items, page header)
- Extract islands per view: OverviewIsland, CustomersIsland, CustomerDrawerIsland, BillingIsland, TimelineIsland, InvitesIsland, SecurityIsland
- The sidebar nav can be static (SSR) with client-side view switching in the content area
- Consider: one large ContentIsland that handles all view switching internally (simpler) vs separate islands per view (more granular SSR)

**Islands:** 1 large content island OR 7 view islands (decide during implementation)

**Server handler:** `owner/dashboard.rs` — already calls `render_page`

---

## Pattern for Each Migration

```
1. Create PageComponent.ts
   - Static shell: layout, nav, headers, footers
   - Call island functions where interactive content goes
   - Compiler Rule 11 detects function calls → ISLAND_START/END opcodes

2. Extract Island components
   - Move interactive logic from the monolithic component into island files
   - Each island imports from the shared store.ts (no change to stores)

3. Update app.ts to dual-mode
   import { activateIslands, mount } from 'formajs';
   import { PageComponent } from './PageComponent';
   import { Island1 } from './Island1';
   // ...
   init();
   if (document.querySelector('[data-forma-island]')) {
     activateIslands({ Island1, Island2, ... });
   } else {
     mount(() => PageComponent(), '#app');
   }

4. Run `npm run build:ssr` to generate IR bytecode

5. Wire the handler: copy the `frontend.render_mode()` / `ir_module()` / slots
   lookup from `benchmark.rs`

6. Verify the SSR output is correct BEFORE opting in — diff it against the
   client render, and run the route's e2e specs

7. Add the route to `SSR_ROUTES` in `crates/server/src/ir/mod.rs`, then restart
```

---

## Prerequisites

- The `@getforma/compiler` traces `PageComponent` imports to emit IR.
- **Each handler needs a server change.** Calling `render_page` is not enough:
  `hosted_login.rs`, `dashboard.rs` (which covers two routes), `owner/dashboard.rs`
  and `platform_dashboard.rs` pass a literal `RenderMode::Phase1ClientMount` with
  `ir_module: None, slots: None` and never consult `frontend.render_mode()`. Copy
  the three-line lookup from `benchmark.rs`. An earlier version of this document
  claimed no server changes were needed, and that is why the frontend work in
  steps 1-3 appears to do nothing.
- **Phase 2 is opt-in per route**, via `SSR_ROUTES` in `crates/server/src/ir/mod.rs`.
  It does *not* auto-upgrade when IR exists. It used to, and that was the bug:
  `@getforma/compiler` 0.3.1 emits IR for every configured route where 0.1.8 did
  not, so upgrading the compiler silently switched server rendering on for pages
  that had never been migrated, and they served broken markup.

## Testing

- `npm run build:ssr` — verify IR files generated for each page
- Restart server — verify "Phase 2 SSR enabled for N route(s)" log message
- Check page source — should contain full HTML, not empty `<div id="app">`
- **Diff the SSR markup against the client render.** Missing elements are the
  failure mode seen so far, and they are invisible once JS hydrates: the page
  looks right in a browser while the no-JS shell and the first paint are wrong.
- Watch the browser console — a malformed attribute in the SSR output (e.g.
  `width=""`) shows up there and nowhere else
- Refresh spam test — no white flash
- Dev mode (`npm run dev` without `--ssr`) — verify fallback `mount()` still works

## Notes

- Stores don't change — islands import from the same singleton store modules
- CSS doesn't change — class names stay the same
- The SSR page component is only for the compiler to trace — at runtime with SSR, it's never called (islands hydrate instead)
- In dev mode (no IR), the page component IS called via `mount()` as a CSR fallback
