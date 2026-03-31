# SSR Migration Plan — Phase 2 Everywhere

Convert all CSR (`mount()`) pages to the island architecture (SSR page component + `activateIslands` hydration) for zero-flash rendering, consistent UX, and to showcase the full Forma SSR pipeline.

---

## Already Done (3 pages)

| Route | Page Component | Islands |
|-------|---------------|---------|
| `/platform/login` | `LoginPage.ts` | AuthCard, MfaPanel |
| `/platform/onboarding` | `OnboardingPage.ts` | RegistrationForm |
| `/platform/benchmark` | `BenchmarkPage.ts` | FilterBar, BenchmarkDataTable, PerfPanel, PerfStat |

These use the dual-mode `app.ts` pattern: `activateIslands` when SSR markers exist, `mount()` fallback for dev mode.

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

5. Restart server — it loads IR modules and enables Phase 2 for the route
```

---

## Prerequisites

- All pages already use `render_page` in their Rust handlers — no server changes needed
- The `render_mode` in each handler defaults to Phase1 and auto-upgrades to Phase2 when IR exists
- The `@getforma/compiler` traces `PageComponent` imports to emit IR

## Testing

- `npm run build:ssr` — verify IR files generated for each page
- Restart server — verify "Phase 2 SSR enabled for N route(s)" log message
- Check page source — should contain full HTML, not empty `<div id="app">`
- Refresh spam test — no white flash
- Dev mode (`npm run dev` without `--ssr`) — verify fallback `mount()` still works

## Notes

- Stores don't change — islands import from the same singleton store modules
- CSS doesn't change — class names stay the same
- The SSR page component is only for the compiler to trace — at runtime with SSR, it's never called (islands hydrate instead)
- In dev mode (no IR), the page component IS called via `mount()` as a CSR fallback
