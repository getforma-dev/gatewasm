# Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full GateWASM design system — shell grid, topbar, breadcrumbs, icon system, page templates, animation library, responsive strategy, skeleton/tooltip/empty-state/modal components, micro-interactions, and dark mode.

**Architecture:** Four-phase build: (1) Foundation — expand tokens, add animation system + typography scale. (2) Shell & Layout — CSS Grid shell, topbar with breadcrumbs, enhanced sidebar with sections/tooltips/mobile overlay. (3) Components — Icon, Skeleton, Tooltip, EmptyState, Modal, Dropdown. (4) Templates & Polish — page template CSS, micro-interactions, keyboard shortcuts, dark mode. Each phase produces a working build before the next begins.

**Tech Stack:** FormaJS (`@getforma/core`), Tailwind CSS v4 (via `@forma/build`), CSS custom properties, CSS Grid + Flexbox, inline SVG icons from Iconify MDI set.

**Codebase context:** Multi-entry-point micro-frontends. 9 JS bundles, 2 CSS outputs (`owner.css` for Plane 1, `forma-platform.css` for Planes 2/3). Components in `admin/src/core/components/ui/`. Hooks in `admin/src/core/hooks/`. Theme in `admin/src/core/theme/`. Current tokens use unprefixed names (`--bg`, `--surface`, `--accent`). FormaJS uses `h()`, `createSignal`, `createShow`, `createList`, `createSwitch`, `onCleanup`.

---

## File Structure

### New files to create:
```
admin/src/core/components/ui/Icon.ts           — Icon component (SVG renderer)
admin/src/core/components/ui/icon-paths.ts      — SVG path data registry
admin/src/core/components/ui/Topbar.ts          — Topbar with breadcrumbs + context
admin/src/core/components/ui/Breadcrumb.ts      — Breadcrumb navigation
admin/src/core/components/ui/Skeleton.ts        — Skeleton loading placeholders
admin/src/core/components/ui/Tooltip.ts         — Tooltip wrapper
admin/src/core/components/ui/EmptyState.ts      — Designed empty state
admin/src/core/components/ui/Modal.ts           — Modal dialog
admin/src/core/components/ui/Dropdown.ts        — Dropdown menu
admin/src/core/components/ui/Shell.ts           — Shell layout (grid + sidebar + topbar + content)
admin/src/core/hooks/createKeyboard.ts          — Keyboard shortcut manager
admin/src/core/utils/relativeTime.ts            — Relative timestamp formatting
admin/src/core/utils/clipboard.ts               — Copy-to-clipboard with feedback
```

### Files to modify:
```
admin/src/core/theme/tokens.css                 — Add timing, easing, typography, breakpoint tokens
admin/src/core/theme/components.css              — Animation system, shell grid, page templates, responsive
admin/src/core/theme/platform.css                — Update @theme block with new tokens
admin/src/styles/main.css                        — Update @theme block with new tokens
admin/src/core/components/ui/Sidebar.ts          — Section headers, tooltips, mobile overlay
admin/src/core/components/ui/DataTable.ts        — Use EmptyState, add skeleton loading
admin/src/core/components/index.ts               — Export new components
admin/src/core/hooks/index.ts                    — Export new hooks
admin/src/platform/console/ConsoleApp.ts         — Use Shell, Icon, Topbar
admin/src/admin/dashboard/OwnerApp.ts            — Use Shell, Icon, Topbar
```

---

## Phase 1: Foundation

### Task 1: Expand token set (timing, easing, typography, breakpoints)

**Files:**
- Modify: `admin/src/core/theme/tokens.css`

- [ ] **Step 1: Add timing and easing tokens to tokens.css**

After the existing `--duration: 200ms;` line (around line 92), add:

```css
  /* ── Timing ─────────────────────────────────────────────────────────── */
  --duration-fast:   120ms;
  --duration-normal: 200ms;
  --duration-slow:   300ms;

  --ease-out:    cubic-bezier(0.4, 0, 0.2, 1);
  --ease-in:     cubic-bezier(0.4, 0, 1, 1);
  --ease-in-out: cubic-bezier(0.4, 0, 0.6, 1);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
```

- [ ] **Step 2: Add typography scale tokens**

After the spacing tokens (around line 107), add:

```css
  /* ── Typography ─────────────────────────────────────────────────────── */
  --text-xs:   11px;
  --text-sm:   12px;
  --text-base: 13px;
  --text-md:   14px;
  --text-lg:   16px;
  --text-xl:   20px;
  --text-2xl:  24px;

  /* ── Breakpoints (reference only — used in @media queries) ──────────── */
  --bp-mobile:  640px;
  --bp-tablet:  768px;
  --bp-desktop: 1024px;
  --bp-wide:    1280px;

  /* ── Shell ──────────────────────────────────────────────────────────── */
  --sidebar-width: 240px;
  --sidebar-collapsed-width: 64px;
  --topbar-height: 52px;
```

- [ ] **Step 3: Update @theme blocks in platform.css and main.css**

In both `admin/src/core/theme/platform.css` and `admin/src/styles/main.css`, add to the `@theme {}` block:

```css
  /* ── Timing ──────────────────────────────────────────────────────────── */
  --duration-fast: 120ms;
  --duration-normal: 200ms;
  --duration-slow: 300ms;

  /* ── Typography ──────────────────────────────────────────────────────── */
  --text-xs: 11px;
  --text-sm: 12px;
  --text-base: 13px;
  --text-md: 14px;
  --text-lg: 16px;
  --text-xl: 20px;
  --text-2xl: 24px;
```

- [ ] **Step 4: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Both CSS outputs compile. No errors.

- [ ] **Step 5: Commit**

```
git add admin/src/core/theme/tokens.css admin/src/core/theme/platform.css admin/src/styles/main.css
git commit -m "feat(tokens): add timing, easing, typography, breakpoint, and shell tokens"
```

---

### Task 2: Animation system

**Files:**
- Modify: `admin/src/core/theme/components.css`

- [ ] **Step 1: Add prefers-reduced-motion support**

At the TOP of components.css (before any other rules), add:

```css
/* ── Reduced motion ──────────────────────────────────────────────────── */

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

- [ ] **Step 2: Add animation keyframes catalog**

Replace the existing `@keyframes fade-up`, `card-in`, `pulse` (around lines 755-781) with the expanded set:

```css
/* ── Animations ──────────────────────────────────────────────────────── */

@keyframes fade-up {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}

@keyframes card-in {
  from { opacity: 0; transform: scale(0.98) translateY(10px); }
  to   { opacity: 1; transform: scale(1) translateY(0); }
}

@keyframes pulse {
  0%, 100% { opacity: 0.3; }
  50%      { opacity: 1; }
}

@keyframes page-enter {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: translateY(0); }
}

@keyframes modal-in {
  from { opacity: 0; transform: scale(0.95); }
  to   { opacity: 1; transform: scale(1); }
}

@keyframes fade-in {
  from { opacity: 0; }
  to   { opacity: 1; }
}

@keyframes toast-slide-in {
  from { opacity: 0; transform: translateX(100%); }
  to   { opacity: 1; transform: translateX(0); }
}

@keyframes toast-slide-out {
  from { opacity: 1; transform: translateX(0); }
  to   { opacity: 0; transform: translateX(100%); }
}

@keyframes dropdown-in {
  from { opacity: 0; transform: scaleY(0.95); transform-origin: top; }
  to   { opacity: 1; transform: scaleY(1); }
}

@keyframes skeleton-pulse {
  0%, 100% { opacity: 0.4; }
  50%      { opacity: 1; }
}

@keyframes card-stagger {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}
```

- [ ] **Step 3: Add animation utility classes**

After the keyframes, add:

```css
/* ── Animation utilities ─────────────────────────────────────────────── */

.page-enter    { animation: page-enter var(--duration-normal, 200ms) var(--ease-out, cubic-bezier(0.4, 0, 0.2, 1)); }
.card-stagger  { animation: card-stagger var(--duration-normal, 200ms) var(--ease-out, cubic-bezier(0.4, 0, 0.2, 1)) both; }
.animate-fade  { animation: fade-in var(--duration-normal, 200ms) ease; }

.skeleton {
  background: var(--border);
  border-radius: var(--radius-sm);
  animation: skeleton-pulse 1.5s var(--ease-in-out, cubic-bezier(0.4, 0, 0.6, 1)) infinite;
}

.btn { transition: all var(--duration-fast, 120ms) ease; }
.btn:active { transform: scale(0.97); }
```

- [ ] **Step 4: Add global focus-visible styles**

```css
/* ── Focus states ────────────────────────────────────────────────────── */

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}
:focus:not(:focus-visible) {
  outline: none;
}
```

- [ ] **Step 5: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Build succeeds.

- [ ] **Step 6: Commit**

```
git add admin/src/core/theme/components.css
git commit -m "feat(css): add animation system with prefers-reduced-motion, focus states"
```

---

## Phase 2: Shell & Layout

### Task 3: Icon component + registry

**Files:**
- Create: `admin/src/core/components/ui/icon-paths.ts`
- Create: `admin/src/core/components/ui/Icon.ts`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create icon-paths.ts with SVG path data**

Create `admin/src/core/components/ui/icon-paths.ts`:

```typescript
/**
 * SVG path data extracted from Iconify MDI set.
 * Only include icons actually used — add more as needed.
 * Each entry is a path `d` attribute for a 24x24 viewBox.
 */
export const ICON_PATHS: Record<string, string> = {
  // Navigation
  'dashboard':       'M13 3v6h8V3m-8 18v-6h8v6M3 21h8V11H3m0-8v6h8V3',
  'users':           'M16 17v2H2v-2s0-4 7-4 7 4 7 4m-3.5-9.5A3.5 3.5 0 1 0 9 4a3.5 3.5 0 0 0 3.5 3.5M15.94 13A5.32 5.32 0 0 1 18 17v2h4v-2s0-3.63-6.06-4M15 4a3.39 3.39 0 0 0-1.93.59 5 5 0 0 1 0 5.82A3.39 3.39 0 0 0 15 11a3.5 3.5 0 0 0 0-7',
  'user':            'M12 4a4 4 0 0 1 4 4 4 4 0 0 1-4 4 4 4 0 0 1-4-4 4 4 0 0 1 4-4m0 10c4.42 0 8 1.79 8 4v2H4v-2c0-2.21 3.58-4 8-4',
  'key':             'M7 14c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2m5.65-4A6 6 0 0 0 1 12a6 6 0 0 0 12 0h-1.5a4.5 4.5 0 1 1-4.5-4.5c1.33 0 2.55.58 3.38 1.5H18v3h3v-3h2v-3h-5.35',
  'shield':          'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4m-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9',
  'settings':        'M12 15.5A3.5 3.5 0 0 1 8.5 12 3.5 3.5 0 0 1 12 8.5a3.5 3.5 0 0 1 3.5 3.5 3.5 3.5 0 0 1-3.5 3.5m7.43-2.53c.04-.32.07-.64.07-.97s-.03-.66-.07-1l2.11-1.63c.19-.15.24-.42.12-.64l-2-3.46c-.12-.22-.39-.31-.61-.22l-2.49 1c-.52-.4-1.08-.73-1.69-.98l-.38-2.65A.49.49 0 0 0 14 2h-4c-.25 0-.46.18-.5.42l-.37 2.65c-.63.25-1.17.59-1.69.98l-2.49-1c-.23-.09-.49 0-.61.22l-2 3.46c-.13.22-.07.49.12.64L4.57 11c-.04.34-.07.67-.07 1s.03.65.07.97l-2.11 1.66c-.19.15-.25.42-.12.64l2 3.46c.12.22.39.3.61.22l2.49-1.01c.52.4 1.08.73 1.69.98l.38 2.65c.03.24.25.42.5.42h4c.25 0 .46-.18.49-.42l.38-2.65c.61-.25 1.17-.58 1.69-.98l2.49 1.01c.22.08.49 0 .61-.22l2-3.46c.12-.22.07-.49-.12-.64l-2.11-1.66',
  'billing':         'M20 8H4V6h16m0 12H4v-6h16m0-8H4c-1.11 0-2 .89-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2',
  'webhook':         'M10 15l5.88-4L10 7v8M15 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V9l-6-6',
  'code':            'M14.6 16.6l4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4m-5.2 0L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4',
  'globe':           'M16.36 14c.08-.66.14-1.32.14-2s-.06-1.34-.14-2h3.38c.16.64.26 1.31.26 2s-.1 1.36-.26 2m-5.15 5.56c.6-1.11 1.06-2.31 1.38-3.56h2.95a8.03 8.03 0 0 1-4.33 3.56M14.34 14H9.66c-.1-.66-.16-1.32-.16-2s.06-1.35.16-2h4.68c.09.65.16 1.32.16 2s-.07 1.34-.16 2M12 19.96c-.83-1.2-1.48-2.53-1.91-3.96h3.82c-.43 1.43-1.08 2.76-1.91 3.96M8 8H5.08A7.92 7.92 0 0 1 9.4 4.44C8.8 5.55 8.35 6.75 8 8m-2.92 8H8c.35 1.25.8 2.45 1.4 3.56A8 8 0 0 1 5.08 16m-.82-2C4.1 13.36 4 12.69 4 12s.1-1.36.26-2h3.38c-.08.66-.14 1.32-.14 2s.06 1.34.14 2M12 4.03c.83 1.2 1.48 2.54 1.91 3.97H10.09c.43-1.43 1.08-2.77 1.91-3.97M18.92 8h-2.95a15.65 15.65 0 0 0-1.38-3.56c1.84.63 3.37 1.9 4.33 3.56M12 2C6.47 2 2 6.5 2 12a10 10 0 0 0 10 10 10 10 0 0 0 10-10A10 10 0 0 0 12 2',

  // Actions
  'plus':            'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2',
  'search':          'M9.5 3A6.5 6.5 0 0 1 16 9.5c0 1.61-.59 3.09-1.56 4.23l.27.27h.79l5 5-1.5 1.5-5-5v-.79l-.27-.27A6.52 6.52 0 0 1 9.5 16 6.5 6.5 0 0 1 3 9.5 6.5 6.5 0 0 1 9.5 3m0 2C7 5 5 7 5 9.5S7 14 9.5 14 14 12 14 9.5 12 5 9.5 5',
  'filter':          'M14 12v7.88c.04.3-.06.62-.29.83a.996.996 0 0 1-1.41 0l-2.01-2.01a.99.99 0 0 1-.29-.83V12l-4.21-4.74A1 1 0 0 1 6.51 5h10.97c.72 0 1.13.82.71 1.37L14 12',
  'edit':            'M20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25',
  'trash':           'M19 4h-3.5l-1-1h-5l-1 1H5v2h14M6 19a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V7H6v12',
  'copy':            'M19 21H8V7h11m0-2H8a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2m-3-4H4a2 2 0 0 0-2 2v14h2V3h12V1',
  'external-link':   'M14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3m-2 16H5V5h7V3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7h-2v7',
  'chevron-right':   'M8.59 16.58L13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.42',
  'chevron-down':    'M7.41 8.58L12 13.17l4.59-4.59L18 10l-6 6-6-6 1.41-1.42',
  'close':           'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41',
  'menu':            'M3 6h18v2H3V6m0 5h18v2H3v-2m0 5h18v2H3v-2',
  'collapse-left':   'M20 9V6H8v3h12M20 18v-3H8v3h12M20 13.5V10.5H8v3h12M6 19V5l-4 7 4 7',
  'expand-right':    'M4 9V6h12v3H4M4 18v-3h12v3H4M4 13.5V10.5h12v3H4M18 19V5l4 7-4 7',

  // Status
  'check-circle':    'M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10 10-4.5 10-10S17.5 2 12 2m-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9',
  'alert-circle':    'M13 13h-2V7h2m0 10h-2v-2h2M12 2A10 10 0 0 0 2 12a10 10 0 0 0 10 10 10 10 0 0 0 10-10A10 10 0 0 0 12 2',
  'info':            'M13 9h-2V7h2m0 10h-2v-6h2m-1-9A10 10 0 0 0 2 12a10 10 0 0 0 10 10 10 10 0 0 0 10-10A10 10 0 0 0 12 2',
  'clock':           'M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10 10-4.5 10-10S17.5 2 12 2m4.2 14.2L11 13V7h1.5v5.2l4.5 2.7-.8 1.3',
  'block':           'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2M4 12c0-4.42 3.58-8 8-8 1.85 0 3.55.63 4.9 1.69L5.69 16.9A7.902 7.902 0 0 1 4 12m8 8c-1.85 0-3.55-.63-4.9-1.69L18.31 7.1A7.902 7.902 0 0 1 20 12c0 4.42-3.58 8-8 8',
  'refresh':         'M17.65 6.35A7.958 7.958 0 0 0 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35',

  // Auth providers
  'google':          'M21.35 11.1h-9.18v2.73h5.51c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09 0-.57-.05-1.12-.14-1.65l-.83-.07M12.17 22c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 19.78 7.7 22 12.17 22M6.01 13.1A5.93 5.93 0 0 1 5.67 12c0-.38.07-.75.16-1.1V8.06H2.18C1.43 9.55 1 11.22 1 13s.43 3.45 1.18 4.94l3.83-2.84M12.17 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12.17 1 7.7 1 3.99 3.22 2.18 7.06l3.83 2.84c.87-2.6 3.3-4.52 6.16-4.52',
  'github':          'M12 2A10 10 0 0 0 2 12c0 4.42 2.87 8.17 6.84 9.5.5.08.66-.23.66-.5v-1.69c-2.77.6-3.36-1.34-3.36-1.34-.46-1.16-1.11-1.47-1.11-1.47-.91-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.87 1.52 2.34 1.07 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.92 0-1.11.38-2 1.03-2.71-.1-.25-.45-1.29.1-2.64 0 0 .84-.27 2.75 1.02.79-.22 1.65-.33 2.5-.33.85 0 1.71.11 2.5.33 1.91-1.29 2.75-1.02 2.75-1.02.55 1.35.2 2.39.1 2.64.65.71 1.03 1.6 1.03 2.71 0 3.82-2.34 4.66-4.57 4.91.36.31.69.92.69 1.85V21c0 .27.16.59.67.5C19.14 20.16 22 16.42 22 12A10 10 0 0 0 12 2',
  'apple':           'M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11',
  'microsoft':       'M11.4 24H0V12.6h11.4V24M24 24H12.6V12.6H24V24M11.4 11.4H0V0h11.4v11.4M24 11.4H12.6V0H24v11.4',

  // Misc
  'moon':            'M17.75 4.09L15.22 6.03l-2.53-1.94.91 3.06-2.53 1.95h3.13l.91 3.06.92-3.06h3.13l-2.53-1.95M4.54 13A8 8 0 0 0 12 21a8 8 0 0 0 7.46-5.23c-1.23.44-2.58.68-4 .55-4.58-.4-8.32-4.05-8.86-8.63-.07-.63-.07-1.24-.03-1.83A8.06 8.06 0 0 0 4.54 13',
  'sun':             'M12 7a5 5 0 0 1 5 5 5 5 0 0 1-5 5 5 5 0 0 1-5-5 5 5 0 0 1 5-5m0 2a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3m0-7l2.39 3.42C13.65 5.15 12.84 5 12 5s-1.65.15-2.39.42L12 2m6.89 3.11l-3.42 2.39A7.4 7.4 0 0 1 18.58 9.4l3.42-2.39-3.11-2.9M22 12l-3.42 2.39a7.4 7.4 0 0 1-.42 2.39L22 12m-3.11 6.89l-2.39-3.42a7.4 7.4 0 0 1-2.39.42l2.39 3.42 2.39-.42M12 22l-2.39-3.42c.74.27 1.55.42 2.39.42s1.65-.15 2.39-.42L12 22M5.11 18.89l3.42-2.39a7.4 7.4 0 0 1-2.39-.42l-3.42 2.39 2.39.42M2 12l3.42-2.39A7.4 7.4 0 0 1 5.84 7.5L2 12m3.11-6.89l2.39 3.42a7.4 7.4 0 0 1 2.39-.42L7.5 5.11 5.11 5.11',
  'lock':            'M12 17a2 2 0 0 0 2-2 2 2 0 0 0-2-2 2 2 0 0 0-2 2 2 2 0 0 0 2 2m6-9a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2h1V6a5 5 0 0 1 5-5 5 5 0 0 1 5 5v2h1m-6-5a3 3 0 0 0-3 3v2h6V6a3 3 0 0 0-3-3',
  'eye':             'M12 9a3 3 0 0 1 3 3 3 3 0 0 1-3 3 3 3 0 0 1-3-3 3 3 0 0 1 3-3m0-4.5c5 0 9.27 3.11 11 7.5-1.73 4.39-6 7.5-11 7.5S2.73 16.39 1 12c1.73-4.39 6-7.5 11-7.5',
  'eye-off':         'M11.83 9L15 12.16V12a3 3 0 0 0-3-3h-.17m-4.3.8l1.55 1.55c-.05.21-.08.42-.08.65a3 3 0 0 0 3 3c.22 0 .44-.03.65-.08l1.55 1.55A4.96 4.96 0 0 1 12 17a5 5 0 0 1-5-5c0-.72.15-1.41.43-2.05m3.81-3.81L10 4.27a11.56 11.56 0 0 1 2-.27c5 0 9.27 3.11 11 7.5a11.77 11.77 0 0 1-4 5.19l-3.12-3.12A5 5 0 0 0 12 7c-.34 0-.67.04-1 .1l-1.36-1.36M2 4.27l2.28 2.28.46.46A11.8 11.8 0 0 0 1 12c1.73 4.39 6 7.5 11 7.5 1.55 0 3.03-.3 4.38-.84l.42.42L19.73 22 21 20.73 3.27 3 2 4.27',
  'download':        'M5 20h14v-2H5v2M19 9h-4V3H9v6H5l7 7 7-7',
  'logout':          'M16 17v-3H9v-4h7V7l5 5-5 5M14 2a2 2 0 0 1 2 2v2h-2V4H5v16h9v-2h2v2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9',
  'home':            'M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8',
};
```

- [ ] **Step 2: Create Icon.ts component**

Create `admin/src/core/components/ui/Icon.ts`:

```typescript
import { h } from 'formajs';
import { ICON_PATHS } from './icon-paths';

interface IconProps {
  name: string;
  size?: number;
  class?: string;
}

export function Icon(props: IconProps): HTMLElement {
  const size = props.size || 20;
  const path = ICON_PATHS[props.name];

  if (!path) {
    if (typeof console !== 'undefined') console.warn(`Icon "${props.name}" not found`);
    return h('span', { class: 'icon-missing', style: `width:${size}px;height:${size}px;display:inline-block` }) as HTMLElement;
  }

  return h('svg', {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    'aria-hidden': 'true',
    class: `icon ${props.class || ''}`.trim(),
    innerHTML: `<path d="${path}" fill="currentColor"/>`,
  }) as HTMLElement;
}
```

- [ ] **Step 3: Add icon CSS to components.css**

```css
/* ── Icon ────────────────────────────────────────────────────────────── */

.icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
}
.icon-missing {
  background: var(--danger-dim);
  border-radius: var(--radius-sm);
}
```

- [ ] **Step 4: Export from index.ts**

Add to `admin/src/core/components/index.ts`:

```typescript
// Icon system
export { Icon } from './ui/Icon';
export { ICON_PATHS } from './ui/icon-paths';
```

- [ ] **Step 5: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Build succeeds.

- [ ] **Step 6: Commit**

```
git add admin/src/core/components/ui/Icon.ts admin/src/core/components/ui/icon-paths.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(icon): add Icon component with SVG path registry (MDI set)"
```

---

### Task 4: Skeleton component

**Files:**
- Create: `admin/src/core/components/ui/Skeleton.ts`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create Skeleton.ts**

Create `admin/src/core/components/ui/Skeleton.ts`:

```typescript
import { h } from 'formajs';

interface SkeletonProps {
  width?: string;
  height?: string;
  radius?: string;
  class?: string;
}

/** Single skeleton pulse block. */
export function Skeleton(props: SkeletonProps): HTMLElement {
  return h('div', {
    class: `skeleton ${props.class || ''}`.trim(),
    style: [
      props.width ? `width:${props.width}` : '',
      props.height ? `height:${props.height}` : '',
      props.radius ? `border-radius:${props.radius}` : '',
    ].filter(Boolean).join(';'),
  }) as HTMLElement;
}

/** Skeleton table — header + N rows matching real table shape. */
export function SkeletonTable(props: { cols?: number; rows?: number }): HTMLElement {
  const cols = props.cols || 4;
  const rows = props.rows || 5;

  return h('div', { class: 'skeleton-table' },
    h('div', { class: 'skeleton-row skeleton-header' },
      ...Array.from({ length: cols }, () =>
        h('div', { class: 'skeleton', style: 'height:12px;flex:1' })
      ),
    ),
    ...Array.from({ length: rows }, () =>
      h('div', { class: 'skeleton-row' },
        ...Array.from({ length: cols }, () =>
          h('div', { class: 'skeleton', style: 'height:14px;flex:1' })
        ),
      ),
    ),
  ) as HTMLElement;
}

/** Skeleton stat card grid — 4 placeholder cards. */
export function SkeletonStats(props: { count?: number }): HTMLElement {
  const count = props.count || 4;
  return h('div', { class: 'stat-grid' },
    ...Array.from({ length: count }, () =>
      h('div', { class: 'stat-card' },
        h('div', { class: 'skeleton', style: 'height:12px;width:60%' }),
        h('div', { class: 'skeleton', style: 'height:20px;width:40%;margin-top:8px' }),
      ),
    ),
  ) as HTMLElement;
}
```

- [ ] **Step 2: Add skeleton CSS**

Add to `admin/src/core/theme/components.css`:

```css
/* ── Skeleton ────────────────────────────────────────────────────────── */

.skeleton-table {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
}
.skeleton-row {
  display: flex;
  gap: 1rem;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.skeleton-row:last-child { border-bottom: none; }
.skeleton-header { background: var(--surface-raised); }
```

- [ ] **Step 3: Export from index.ts**

Add to `admin/src/core/components/index.ts`:

```typescript
export { Skeleton, SkeletonTable, SkeletonStats } from './ui/Skeleton';
```

- [ ] **Step 4: Commit**

```
git add admin/src/core/components/ui/Skeleton.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(skeleton): add Skeleton, SkeletonTable, SkeletonStats loading components"
```

---

### Task 5: Tooltip component

**Files:**
- Create: `admin/src/core/components/ui/Tooltip.ts`
- Modify: `admin/src/core/theme/components.css`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create Tooltip.ts**

Create `admin/src/core/components/ui/Tooltip.ts`:

```typescript
import { h } from 'formajs';

interface TooltipProps {
  text: string;
  position?: 'right' | 'top' | 'bottom';
  class?: string;
}

/**
 * Wraps children in a container that shows a tooltip on hover.
 * Usage: Tooltip({ text: 'Overview' }, h(Icon, { name: 'dashboard' }))
 */
export function Tooltip(props: TooltipProps, ...children: any[]): HTMLElement {
  return h('div', { class: `tooltip-wrap ${props.class || ''}`.trim() },
    ...children,
    h('span', {
      class: `tooltip tooltip-${props.position || 'right'}`,
    }, props.text),
  ) as HTMLElement;
}
```

- [ ] **Step 2: Add tooltip CSS**

Add to `admin/src/core/theme/components.css`:

```css
/* ── Tooltip ─────────────────────────────────────────────────────────── */

.tooltip-wrap { position: relative; display: inline-flex; }

.tooltip {
  position: absolute;
  background: var(--text);
  color: var(--bg);
  font-size: var(--text-xs, 11px);
  padding: 4px 8px;
  border-radius: 4px;
  white-space: nowrap;
  pointer-events: none;
  opacity: 0;
  transition: opacity var(--duration-fast, 120ms) ease;
  z-index: 50;
}
.tooltip-wrap:hover > .tooltip { opacity: 1; }

.tooltip-right {
  left: calc(100% + 8px);
  top: 50%;
  transform: translateY(-50%);
}
.tooltip-top {
  bottom: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
}
.tooltip-bottom {
  top: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
}
```

- [ ] **Step 3: Export**

Add to `admin/src/core/components/index.ts`:

```typescript
export { Tooltip } from './ui/Tooltip';
```

- [ ] **Step 4: Commit**

```
git add admin/src/core/components/ui/Tooltip.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(tooltip): add Tooltip component with right/top/bottom positions"
```

---

### Task 6: EmptyState component

**Files:**
- Create: `admin/src/core/components/ui/EmptyState.ts`
- Modify: `admin/src/core/theme/components.css`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create EmptyState.ts**

Create `admin/src/core/components/ui/EmptyState.ts`:

```typescript
import { h } from 'formajs';
import { Icon } from './Icon';

interface EmptyStateProps {
  icon?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState(props: EmptyStateProps): HTMLElement {
  return h('div', { class: 'empty-state-designed' },
    props.icon ? Icon({ name: props.icon, size: 48, class: 'empty-state-icon' }) : null,
    h('h3', { class: 'empty-state-title' }, props.title),
    props.description
      ? h('p', { class: 'empty-state-desc' }, props.description)
      : null,
    props.action
      ? h('button', {
          class: 'btn btn-primary mt-md',
          onClick: props.action.onClick,
        },
          Icon({ name: 'plus', size: 16 }),
          h('span', null, props.action.label),
        )
      : null,
  ) as HTMLElement;
}
```

- [ ] **Step 2: Add empty state CSS**

```css
/* ── Empty State ─────────────────────────────────────────────────────── */

.empty-state-designed {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 3rem 2rem;
  text-align: center;
  gap: 0.75rem;
}
.empty-state-icon { color: var(--text-muted); opacity: 0.6; }
.empty-state-title {
  font-size: var(--text-lg, 16px);
  font-weight: 500;
  color: var(--text);
}
.empty-state-desc {
  font-size: var(--text-base, 13px);
  color: var(--text-dim);
  max-width: 320px;
}
```

- [ ] **Step 3: Export**

Add to `admin/src/core/components/index.ts`:

```typescript
export { EmptyState } from './ui/EmptyState';
```

- [ ] **Step 4: Commit**

```
git add admin/src/core/components/ui/EmptyState.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(empty-state): add designed EmptyState with icon, title, description, CTA"
```

---

### Task 7: Modal component

**Files:**
- Create: `admin/src/core/components/ui/Modal.ts`
- Modify: `admin/src/core/theme/components.css`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create Modal.ts**

Create `admin/src/core/components/ui/Modal.ts`:

```typescript
import { h, createShow, onCleanup } from 'formajs';
import { Icon } from './Icon';

interface ModalProps {
  open: () => boolean;
  onClose: () => void;
  title: string;
  children: () => HTMLElement | HTMLElement[];
}

export function Modal(props: ModalProps): HTMLElement {
  // Close on Escape
  const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') props.onClose(); };
  document.addEventListener('keydown', handleKey);
  onCleanup(() => document.removeEventListener('keydown', handleKey));

  return createShow(props.open, () =>
    h('div', { class: 'modal-overlay animate-fade', onClick: (e: Event) => {
      if ((e.target as HTMLElement).classList.contains('modal-overlay')) props.onClose();
    }},
      h('div', { class: 'modal-content' },
        h('div', { class: 'modal-header' },
          h('h2', { class: 'modal-title' }, props.title),
          h('button', { class: 'modal-close', onClick: props.onClose },
            Icon({ name: 'close', size: 18 }),
          ),
        ),
        h('div', { class: 'modal-body' }, props.children()),
      ),
    ),
  ) as HTMLElement;
}
```

- [ ] **Step 2: Add modal CSS**

```css
/* ── Modal ───────────────────────────────────────────────────────────── */

.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  padding: 1rem;
}
.modal-content {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  width: 100%;
  max-width: 520px;
  max-height: 80vh;
  overflow-y: auto;
  animation: modal-in var(--duration-normal, 200ms) var(--ease-spring, cubic-bezier(0.34, 1.56, 0.64, 1));
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.25rem;
  border-bottom: 1px solid var(--border);
}
.modal-title {
  font-size: var(--text-md, 14px);
  font-weight: 500;
  color: var(--text);
}
.modal-close {
  background: none;
  border: none;
  color: var(--text-dim);
  cursor: pointer;
  padding: 4px;
  border-radius: var(--radius-sm);
  display: flex;
  transition: color var(--duration-fast, 120ms) ease;
}
.modal-close:hover { color: var(--text); }
.modal-body { padding: 1.25rem; }
```

- [ ] **Step 3: Export**

Add to `admin/src/core/components/index.ts`:

```typescript
export { Modal } from './ui/Modal';
```

- [ ] **Step 4: Commit**

```
git add admin/src/core/components/ui/Modal.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(modal): add Modal component with overlay, escape-to-close, animation"
```

---

### Task 8: Dropdown component

**Files:**
- Create: `admin/src/core/components/ui/Dropdown.ts`
- Modify: `admin/src/core/theme/components.css`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create Dropdown.ts**

Create `admin/src/core/components/ui/Dropdown.ts`:

```typescript
import { h, createSignal, createShow, onCleanup } from 'formajs';

interface DropdownItem {
  id: string;
  label: string;
  icon?: string;
  danger?: boolean;
}

interface DropdownProps {
  trigger: HTMLElement;
  items: DropdownItem[];
  onSelect: (id: string) => void;
}

export function Dropdown(props: DropdownProps): HTMLElement {
  const [open, setOpen] = createSignal(false);

  const handleClickOutside = (e: Event) => {
    const target = e.target as Node;
    if (container && !container.contains(target)) setOpen(false);
  };
  document.addEventListener('click', handleClickOutside);
  onCleanup(() => document.removeEventListener('click', handleClickOutside));

  const container = h('div', { class: 'dropdown' },
    h('div', { class: 'dropdown-trigger', onClick: () => setOpen((o: boolean) => !o) },
      props.trigger,
    ),
    createShow(open, () =>
      h('div', { class: 'dropdown-menu' },
        ...props.items.map(item =>
          h('button', {
            class: `dropdown-item${item.danger ? ' danger' : ''}`,
            onClick: () => { props.onSelect(item.id); setOpen(false); },
          }, item.label),
        ),
      ),
    ),
  ) as HTMLElement;

  return container;
}
```

- [ ] **Step 2: Add dropdown CSS**

```css
/* ── Dropdown ────────────────────────────────────────────────────────── */

.dropdown { position: relative; display: inline-flex; }
.dropdown-trigger { cursor: pointer; }

.dropdown-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  min-width: 160px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md);
  z-index: 40;
  padding: 4px 0;
  animation: dropdown-in var(--duration-fast, 120ms) var(--ease-out, cubic-bezier(0.4, 0, 0.2, 1));
}
.dropdown-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 6px 12px;
  font-size: var(--text-base, 13px);
  color: var(--text);
  background: none;
  border: none;
  cursor: pointer;
  transition: background var(--duration-fast, 120ms) ease;
}
.dropdown-item:hover { background: var(--surface-raised); }
.dropdown-item.danger { color: var(--danger); }
.dropdown-item.danger:hover { background: var(--danger-dim); }
```

- [ ] **Step 3: Export**

```typescript
export { Dropdown } from './ui/Dropdown';
```

- [ ] **Step 4: Commit**

```
git add admin/src/core/components/ui/Dropdown.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(dropdown): add Dropdown component with click-outside close, animation"
```

---

### Task 9: Topbar + Breadcrumb components

**Files:**
- Create: `admin/src/core/components/ui/Breadcrumb.ts`
- Create: `admin/src/core/components/ui/Topbar.ts`
- Modify: `admin/src/core/theme/components.css`
- Modify: `admin/src/core/components/index.ts`

- [ ] **Step 1: Create Breadcrumb.ts**

Create `admin/src/core/components/ui/Breadcrumb.ts`:

```typescript
import { h } from 'formajs';
import { Icon } from './Icon';

interface BreadcrumbSegment {
  label: string;
  onClick?: () => void;
}

interface BreadcrumbProps {
  segments: () => BreadcrumbSegment[];
}

export function Breadcrumb(props: BreadcrumbProps): HTMLElement {
  return h('nav', { class: 'breadcrumb', 'aria-label': 'Breadcrumb' },
    () => {
      const segs = props.segments();
      const nodes: (HTMLElement | null)[] = [];
      segs.forEach((seg, i) => {
        if (i > 0) {
          nodes.push(Icon({ name: 'chevron-right', size: 14, class: 'breadcrumb-sep' }));
        }
        const isLast = i === segs.length - 1;
        nodes.push(
          h(isLast ? 'span' : 'button', {
            class: isLast ? 'breadcrumb-current' : 'breadcrumb-link',
            ...(seg.onClick && !isLast ? { onClick: seg.onClick } : {}),
          }, seg.label),
        );
      });
      return nodes;
    },
  ) as HTMLElement;
}
```

- [ ] **Step 2: Create Topbar.ts**

Create `admin/src/core/components/ui/Topbar.ts`:

```typescript
import { h } from 'formajs';
import { Breadcrumb } from './Breadcrumb';

interface TopbarProps {
  breadcrumbs: () => { label: string; onClick?: () => void }[];
  actions?: () => HTMLElement | HTMLElement[] | null;
}

export function Topbar(props: TopbarProps): HTMLElement {
  return h('header', { class: 'topbar' },
    h('div', { class: 'topbar-left' },
      Breadcrumb({ segments: props.breadcrumbs }),
    ),
    h('div', { class: 'topbar-right' },
      props.actions ? props.actions() : null,
    ),
  ) as HTMLElement;
}
```

- [ ] **Step 3: Add CSS**

```css
/* ── Topbar ──────────────────────────────────────────────────────────── */

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--topbar-height, 52px);
  padding: 0 1.5rem;
  border-bottom: 1px solid var(--border);
  background: var(--surface);
  gap: 1rem;
}
.topbar-left { display: flex; align-items: center; gap: 0.5rem; min-width: 0; }
.topbar-right { display: flex; align-items: center; gap: 0.5rem; flex-shrink: 0; }

/* ── Breadcrumb ──────────────────────────────────────────────────────── */

.breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  font-size: var(--text-base, 13px);
  min-width: 0;
}
.breadcrumb-link {
  background: none;
  border: none;
  color: var(--text-dim);
  cursor: pointer;
  padding: 2px 4px;
  border-radius: var(--radius-sm);
  font-size: inherit;
  transition: color var(--duration-fast, 120ms) ease;
}
.breadcrumb-link:hover { color: var(--text); }
.breadcrumb-current {
  color: var(--text);
  font-weight: 500;
  padding: 2px 4px;
}
.breadcrumb-sep { color: var(--text-muted); flex-shrink: 0; }
```

- [ ] **Step 4: Export**

```typescript
export { Breadcrumb } from './ui/Breadcrumb';
export { Topbar } from './ui/Topbar';
```

- [ ] **Step 5: Commit**

```
git add admin/src/core/components/ui/Breadcrumb.ts admin/src/core/components/ui/Topbar.ts admin/src/core/theme/components.css admin/src/core/components/index.ts
git commit -m "feat(topbar): add Topbar + Breadcrumb components with responsive layout"
```

---

### Task 10: Shell grid layout + page template CSS

**Files:**
- Modify: `admin/src/core/theme/components.css`

- [ ] **Step 1: Replace the existing .shell flex layout with CSS Grid**

Replace the existing `.shell` block (around line 787) with:

```css
/* ── Shell — CSS Grid ────────────────────────────────────────────────── */

.shell {
  display: grid;
  grid-template-columns: var(--sidebar-width, 240px) 1fr;
  grid-template-rows: var(--topbar-height, 52px) 1fr;
  height: 100dvh;
  min-height: 100vh; /* fallback for browsers without dvh */
  position: relative;
  z-index: 1;
}

.shell .sidebar { grid-row: 1 / -1; }
.shell .topbar  { grid-column: 2; }
.shell .content { grid-column: 2; overflow-y: auto; padding: 1.5rem 2rem; }

.shell--collapsed {
  grid-template-columns: var(--sidebar-collapsed-width, 64px) 1fr;
}
```

- [ ] **Step 2: Add page template classes**

```css
/* ── Page Templates ──────────────────────────────────────────────────── */

.page-overview { display: flex; flex-direction: column; gap: 1.5rem; animation: page-enter 200ms var(--ease-out, ease); }

.stat-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-md, 1rem);
}

.chart-grid {
  display: grid;
  grid-template-columns: 2fr 1fr;
  gap: var(--space-md, 1rem);
}

.page-table { display: flex; flex-direction: column; gap: 1rem; animation: page-enter 200ms var(--ease-out, ease); }

.table-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.5rem;
}
.table-toolbar-left {
  display: flex;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
}

.page-detail {
  display: grid;
  grid-template-columns: 1fr 300px;
  gap: 1.5rem;
  align-items: start;
  animation: page-enter 200ms var(--ease-out, ease);
}
.page-detail .main-col { display: flex; flex-direction: column; gap: 1rem; }
.page-detail .aside-col { display: flex; flex-direction: column; gap: 1rem; position: sticky; top: 0; }

.page-settings {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  max-width: 640px;
  animation: page-enter 200ms var(--ease-out, ease);
}

.settings-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 1.25rem;
}
.settings-card-title { font-size: var(--text-md, 14px); font-weight: 500; color: var(--text); }
.settings-card-desc  { font-size: var(--text-sm, 12px); color: var(--text-dim); margin-top: 4px; }
.settings-card-footer {
  display: flex;
  justify-content: flex-end;
  padding-top: 1rem;
  margin-top: 1rem;
  border-top: 1px solid var(--border);
}
.settings-card.danger { border-left: 3px solid var(--danger); }
```

- [ ] **Step 3: Add responsive breakpoints**

```css
/* ── Responsive: Tablet (768–1023px) ─────────────────────────────────── */

@media (max-width: 1023px) {
  .stat-grid  { grid-template-columns: repeat(2, 1fr); }
  .chart-grid { grid-template-columns: 1fr; }
  .page-detail { grid-template-columns: 1fr; }
  .page-detail .aside-col { position: static; }

  .shell { grid-template-columns: var(--sidebar-collapsed-width, 64px) 1fr; }
  .shell .sidebar .nav-label,
  .shell .sidebar .nav-section-header,
  .shell .sidebar .brand-text { opacity: 0; pointer-events: none; width: 0; overflow: hidden; }
  .shell .sidebar .nav button { justify-content: center; padding: 0.5rem; }
  .shell .sidebar { width: var(--sidebar-collapsed-width, 64px); padding: 1rem 0.5rem; align-items: center; }
}

/* ── Responsive: Mobile (<768px) ─────────────────────────────────────── */

@media (max-width: 767px) {
  .shell {
    grid-template-columns: 1fr;
    grid-template-rows: var(--topbar-height, 52px) 1fr;
  }
  .shell .sidebar {
    position: fixed;
    top: 0; left: 0;
    height: 100dvh;
    width: var(--sidebar-width, 240px);
    z-index: 50;
    transform: translateX(-100%);
    transition: transform 250ms var(--ease-out, cubic-bezier(0.4, 0, 0.2, 1));
  }
  .shell .sidebar.open { transform: translateX(0); }

  .sidebar-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    z-index: 49;
    opacity: 0;
    pointer-events: none;
    transition: opacity 250ms ease;
  }
  .sidebar-backdrop.visible { opacity: 1; pointer-events: auto; }

  .stat-grid { grid-template-columns: 1fr; }
  .page-settings { max-width: none; }
  .shell .content { padding: 1rem; }

  /* Touch targets */
  .nav button { min-height: 44px; padding: 10px 12px; }
  .btn-sm { min-height: 44px; padding: 10px 16px; }
}

/* ── Responsive: Wide (≥1280px) ──────────────────────────────────────── */

@media (min-width: 1280px) {
  .shell .content { padding: 2rem 2.5rem; max-width: 1200px; }
}
```

- [ ] **Step 4: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```
git add admin/src/core/theme/components.css
git commit -m "feat(layout): CSS Grid shell, 4 page templates, responsive breakpoints"
```

---

## Phase 3: Utilities & Polish

### Task 11: Relative time utility

**Files:**
- Create: `admin/src/core/utils/relativeTime.ts`

- [ ] **Step 1: Create relativeTime.ts**

Create `admin/src/core/utils/relativeTime.ts`:

```typescript
/**
 * Format a timestamp as relative time ("2 minutes ago", "Yesterday").
 * Returns the formatted string.
 */
export function relativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return 'Just now';
  if (diffMin < 60) return `${diffMin} minute${diffMin === 1 ? '' : 's'} ago`;
  if (diffHr < 24) return `${diffHr} hour${diffHr === 1 ? '' : 's'} ago`;
  if (diffDay === 1) return 'Yesterday';
  if (diffDay < 30) return `${diffDay} days ago`;
  if (diffDay < 365) return `${Math.floor(diffDay / 30)} month${Math.floor(diffDay / 30) === 1 ? '' : 's'} ago`;
  return `${Math.floor(diffDay / 365)} year${Math.floor(diffDay / 365) === 1 ? '' : 's'} ago`;
}

/**
 * Format a date as ISO-like string for tooltip display.
 */
export function formatExact(dateStr: string): string {
  const d = new Date(dateStr);
  return d.toISOString().replace('T', ' ').replace(/\.\d{3}Z$/, ' UTC');
}
```

- [ ] **Step 2: Commit**

```
git add admin/src/core/utils/relativeTime.ts
git commit -m "feat(utils): add relativeTime + formatExact timestamp utilities"
```

---

### Task 12: Clipboard utility

**Files:**
- Create: `admin/src/core/utils/clipboard.ts`

- [ ] **Step 1: Create clipboard.ts**

Create `admin/src/core/utils/clipboard.ts`:

```typescript
/**
 * Copy text to clipboard and return true on success.
 * Falls back to execCommand for older browsers.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Fallback
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  }
}
```

- [ ] **Step 2: Commit**

```
git add admin/src/core/utils/clipboard.ts
git commit -m "feat(utils): add copyToClipboard utility with fallback"
```

---

### Task 13: Keyboard shortcuts hook

**Files:**
- Create: `admin/src/core/hooks/createKeyboard.ts`
- Modify: `admin/src/core/hooks/index.ts`

- [ ] **Step 1: Create createKeyboard.ts**

Create `admin/src/core/hooks/createKeyboard.ts`:

```typescript
import { onCleanup } from 'formajs';

interface Shortcut {
  keys: string;             // e.g. "Escape", "Meta+k", "g o" (sequence)
  handler: () => void;
  description?: string;
}

/**
 * Register global keyboard shortcuts.
 * Supports single keys, modifier combos (Meta+k), and sequences (g o).
 * Call inside a component — cleans up automatically via onCleanup.
 */
export function createKeyboard(shortcuts: Shortcut[]) {
  let pendingKey: string | null = null;
  let pendingTimeout: ReturnType<typeof setTimeout> | null = null;

  const handleKeydown = (e: KeyboardEvent) => {
    // Don't trigger if user is typing in an input/textarea
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
      // Exception: Escape always works
      if (e.key !== 'Escape') return;
    }

    for (const s of shortcuts) {
      // Modifier combo: "Meta+k"
      if (s.keys.includes('+')) {
        const parts = s.keys.split('+');
        const mod = parts[0];
        const key = parts[1];
        const modPressed =
          (mod === 'Meta' && e.metaKey) ||
          (mod === 'Ctrl' && e.ctrlKey) ||
          (mod === 'Alt' && e.altKey) ||
          (mod === 'Shift' && e.shiftKey);
        if (modPressed && e.key.toLowerCase() === key.toLowerCase()) {
          e.preventDefault();
          s.handler();
          return;
        }
        continue;
      }

      // Sequence: "g o"
      if (s.keys.includes(' ')) {
        const [first, second] = s.keys.split(' ');
        if (pendingKey === first && e.key.toLowerCase() === second.toLowerCase()) {
          pendingKey = null;
          if (pendingTimeout) clearTimeout(pendingTimeout);
          e.preventDefault();
          s.handler();
          return;
        }
        if (e.key.toLowerCase() === first.toLowerCase()) {
          pendingKey = first;
          if (pendingTimeout) clearTimeout(pendingTimeout);
          pendingTimeout = setTimeout(() => { pendingKey = null; }, 500);
          return;
        }
        continue;
      }

      // Single key
      if (e.key === s.keys || e.key.toLowerCase() === s.keys.toLowerCase()) {
        // Special keys (Escape, /) don't need preventDefault checks
        if (s.keys === '/' || s.keys === '[') e.preventDefault();
        s.handler();
        return;
      }
    }

    // Reset sequence on non-matching key
    pendingKey = null;
  };

  document.addEventListener('keydown', handleKeydown);
  onCleanup(() => {
    document.removeEventListener('keydown', handleKeydown);
    if (pendingTimeout) clearTimeout(pendingTimeout);
  });
}
```

- [ ] **Step 2: Export**

Add to `admin/src/core/hooks/index.ts`:

```typescript
export { createKeyboard } from './createKeyboard';
```

- [ ] **Step 3: Commit**

```
git add admin/src/core/hooks/createKeyboard.ts admin/src/core/hooks/index.ts
git commit -m "feat(keyboard): add createKeyboard hook for global shortcuts with sequences"
```

---

### Task 14: Dark mode token overrides

**Files:**
- Modify: `admin/src/core/theme/tokens.css`

- [ ] **Step 1: Add light mode overrides via prefers-color-scheme**

The current tokens define a dark theme by default. Add a `prefers-color-scheme: light` media query at the end of tokens.css:

```css
/* ── Light Mode ──────────────────────────────────────────────────────── */

@media (prefers-color-scheme: light) {
  :root {
    --bg:             #f8fafc;
    --surface:        #ffffff;
    --surface-raised: #f1f5f9;
    --surface-overlay:#e2e8f0;

    --border:         #e2e8f0;
    --border-accent:  #cbd5e1;
    --border-strong:  #94a3b8;

    --text:           #0f172a;
    --text-dim:       #475569;
    --text-muted:     #94a3b8;

    --accent:         #b8860b;
    --accent-hover:   #d4a017;
    --accent-dim:     rgba(184, 134, 11, 0.1);
    --accent-glow:    rgba(184, 134, 11, 0.05);
    --accent-border:  rgba(184, 134, 11, 0.2);

    --success:        #16a34a;
    --success-dim:    rgba(22, 163, 74, 0.08);
    --success-border: rgba(22, 163, 74, 0.15);
    --warning:        #ca8a04;
    --warning-dim:    rgba(202, 138, 4, 0.08);
    --warning-border: rgba(202, 138, 4, 0.15);
    --danger:         #dc2626;
    --danger-dim:     rgba(220, 38, 38, 0.08);
    --danger-border:  rgba(220, 38, 38, 0.15);
    --info:           #2563eb;

    --shadow-sm:      0 1px 2px rgba(0,0,0,0.05);
    --shadow-md:      0 2px 8px rgba(0,0,0,0.08);
    --shadow-lg:      0 4px 12px rgba(0,0,0,0.1);

    --grid-opacity:   0.06;
    --glow-size:      900px;
    --glow-intensity: 0.03;
  }
}
```

- [ ] **Step 2: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```
git add admin/src/core/theme/tokens.css
git commit -m "feat(theme): add light mode token overrides via prefers-color-scheme"
```

---

### Task 15: Scroll shadows for content area

**Files:**
- Modify: `admin/src/core/theme/components.css`

- [ ] **Step 1: Add scroll shadow CSS for .content**

```css
/* ── Scroll shadows ──────────────────────────────────────────────────── */

.scroll-shadow {
  background:
    linear-gradient(var(--surface, #111114) 30%, transparent),
    linear-gradient(transparent, var(--surface, #111114) 70%),
    radial-gradient(farthest-side at 50% 0, rgba(0,0,0,0.08), transparent),
    radial-gradient(farthest-side at 50% 100%, rgba(0,0,0,0.08), transparent);
  background-position: top, bottom, top, bottom;
  background-size: 100% 40px, 100% 40px, 100% 12px, 100% 12px;
  background-repeat: no-repeat;
  background-attachment: local, local, scroll, scroll;
}
```

- [ ] **Step 2: Commit**

```
git add admin/src/core/theme/components.css
git commit -m "feat(css): add scroll shadow utility class"
```

---

### Task 16: Wire up Shell in ConsoleApp and OwnerApp

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`
- Modify: `admin/src/admin/dashboard/OwnerApp.ts`

- [ ] **Step 1: Update ConsoleApp to use Topbar + new shell grid**

In ConsoleApp.ts, replace the current shell structure to add a topbar between the sidebar and main content. The shell grid has 3 areas: sidebar (spanning both rows), topbar, and content.

Replace the return statement in `ConsoleApp()`:

```typescript
return h('div', { class: () => collapsed() ? 'shell shell--collapsed' : 'shell' },
  Sidebar({ /* ... existing sidebar props ... */ }),
  Topbar({
    breadcrumbs: () => {
      const v = view();
      const t = selectedTenant();
      const crumbs = [{ label: 'Console' }];
      if (t) crumbs.push({ label: t.name, onClick: () => backToTenants() });
      if (v !== 'tenants' && v !== 'wizard') {
        const labels: Record<string, string> = {
          overview: 'Overview', users: 'Users', integrate: 'Integrate',
          'hosted-page': 'Hosted Page', settings: 'Settings',
        };
        crumbs.push({ label: labels[v] || v });
      }
      return crumbs;
    },
  }),
  h('main', { class: 'content page-enter' },
    LoadingBar(() => loading()),
    createSwitch(/* ... existing view switch ... */),
  ),
  ToastView(toast),
);
```

Import `Topbar` at the top of the file:
```typescript
import { Topbar } from '../../core/components/ui/Topbar';
```

- [ ] **Step 2: Update OwnerApp similarly**

Add Topbar import and insert between sidebar and main. Breadcrumbs derive from `store.view`:

```typescript
Topbar({
  breadcrumbs: () => {
    const v = store.view;
    const labels: Record<string, string> = {
      overview: 'Overview', customers: 'Customers', billing: 'Billing',
      timeline: 'Timeline', invites: 'Invites', security: 'Security',
    };
    return [{ label: 'Control Plane' }, { label: labels[v] || v }];
  },
}),
```

- [ ] **Step 3: Verify build**

Run: `cd admin && npx tsx build.ts`
Expected: Build succeeds.

- [ ] **Step 4: Commit**

```
git add admin/src/platform/console/ConsoleApp.ts admin/src/admin/dashboard/OwnerApp.ts
git commit -m "feat(shell): wire Topbar + breadcrumbs into ConsoleApp and OwnerApp"
```

---

### Task 17: Final integration — verify everything works together

- [ ] **Step 1: Run full build**

```bash
cd admin && npx tsx build.ts
```

Expected: All 9 JS bundles + 2 CSS bundles compile. No errors.

- [ ] **Step 2: Check TypeScript**

```bash
cd admin && npx tsc --noEmit 2>&1 | grep -v node_modules | head -20
```

Expected: No NEW errors from our changes (pre-existing errors are acceptable).

- [ ] **Step 3: Verify CSS output sizes are reasonable**

```bash
wc -c admin/dist/forma-platform.css admin/dist/owner.css
```

Expected: forma-platform.css ~60-80KB (was 54KB, added animation + layout CSS). owner.css ~35KB.

- [ ] **Step 4: Final commit**

If any adjustments needed, fix and commit.

```
git commit -m "chore: final design system integration verification"
```

---

## Summary

| Phase | Tasks | What Ships |
|-------|-------|-----------|
| **1: Foundation** | Tasks 1-2 | Expanded tokens (timing, easing, typography, breakpoints), animation system with prefers-reduced-motion, focus states |
| **2: Shell & Layout** | Tasks 3-10 | Icon component + registry, Skeleton, Tooltip, EmptyState, Modal, Dropdown, Topbar + Breadcrumb, CSS Grid shell, 4 page templates, responsive breakpoints |
| **3: Utilities & Polish** | Tasks 11-16 | Relative time, clipboard, keyboard shortcuts, dark mode, scroll shadows, shell wiring |
| **4: Integration** | Task 17 | Full build verification |

Total: 17 tasks, ~17 commits. Each task is independently buildable.
