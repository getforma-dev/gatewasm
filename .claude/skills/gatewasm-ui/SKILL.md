---
name: gatewasm-ui
description: Use when building, styling, debugging, or reviewing any gatewasm UI — console views, the onboarding wizard, hosted/tenant login, the owner control plane, or theme CSS. Read BEFORE adding a component, writing CSS, picking a color, or diagnosing unstyled/overlapping/overflowing UI in this repo.
---

# gatewasm UI

Framework rules (islands, signals, the compiler contract) live in the
**formajs-ui** skill — read that first if you are writing components. This
skill is the app: its design system, its surfaces, and how to see them.

## The identity — refine, never replace

Dark ground, warm gold, serif headings over mono data. Victor likes this;
changing its character is not a refactor you make in passing.

| Token | Value | Use |
|---|---|---|
| `--bg` / `--surface` / `--surface-raised` | `#0a0a0c` / `#111114` / `#16161a` | page, cards, raised cards |
| `--accent` / `--accent-hover` | `#c4a265` / `#d4b275` | the one primary action per view |
| `--text` / `--text-dim` / `--text-muted` | `#e8e6e3` / `#6b6a72` / `#52525b` | body, secondary, labels |
| `--border` / `--border-accent` / `--border-strong` | `#1e1e24` / `#2a2a33` / `#38383f` | hairlines, hover, emphasis |
| plane accents | `#c4a265` Operator · `#2E9BFF` Organization · `#1EDC82` End User | console plane identity only |
| semantic | `#4ade80` `#fbbf24` `#f87171` `#60a5fa` | success / warning / danger / info |

All of them live in `admin/src/core/theme/tokens.css` (dark at `:root`,
light under `:root[data-theme='light']`). **Never hardcode a hex in a
component or a page stylesheet — use the token.** The light theme is not a
copy of the dark one: the plane hues are darkened there because the dark
values sit near 2:1 on white.

**Type is addressed by role, never by family** — `--font-display`
(headings), `--font-body`, `--font-data` (labels, eyebrows, identifiers,
code), `--font-nav`, `--font-numeral` (wordmark and display numerals).
Writing `font-family: 'DM Mono'` in a component is the bug; a role can be
re-cast in one line, a scattered family name cannot. Rajdhani was retired
this way — it dressed nav labels and nothing else — so `--font-nav` is
currently the data face.

**Breakpoints are a 4-stop scale**: 480 / 768 / 1024 / 1360. Sidebar rails
become top bands at 768; multi-column layouts collapse at 1024. Adding a
fifth stop is how the sidebar ended up responding at a different width than
the grid beside it.

**Theme selection**: a stored choice wins, otherwise `prefers-color-scheme`
(`initThemeMode()` in `core/utils.ts` — call it in every entrypoint). The
Playwright config pins `colorScheme: 'dark'` so the suite is deterministic.

## Where CSS lives

One bundle, `forma-platform.css`, on every route — entry
`core/theme/platform.css`, which imports in order:

| File | Holds |
|---|---|
| `tokens.css` | `@font-face` + every custom property |
| `base.css` | reset, element defaults, grid + glow backdrops |
| `components.css` | buttons, cards, shell, PlaneNav, modal, wizard |
| `kit.css` | shared `core/components/ui` kit: panel, pill, pager, filters, table-wrap, charts, avatars |
| `pages.css` | per-page layouts + the `body.owner-dashboard` section |

Route→asset map is `admin/build.ts`. Per-page ambience (`--grid-opacity`,
`--glow-*`, tenant `--accent`) is injected server-side as
`personality_css` by each Rust handler.

**Rule: a class you write must be defined in this bundle.** `npm run
check:css` fails the build when a component names a class no stylesheet on
its route defines — the failure mode that shipped ghost buttons and an
unstyled modal for months. The count may only fall.

## Surfaces

| Route | Mode | Root component |
|---|---|---|
| `/platform/login`, `/platform/onboarding`, `/platform/benchmark` | **SSR + islands** (the allowlist in `crates/server/src/ir/mod.rs`) | `platform/{login,onboarding,benchmark}/` |
| `/platform/console[/{slug}]` | CSR | `platform/console/ConsoleApp.ts` — 4 account tabs, 12 workspace views (hash-routed, `VALID_CONSOLE_VIEWS` in `store.ts`), 7-step wizard (`OnboardingWizard.ts`) |
| `/platform/admin/dashboard` | CSR, owner-gated | `admin/dashboard/OwnerApp.ts` — styled via `body.owner-dashboard` |
| `/t/{slug}/login`, `/dashboard`, `/admin` | CSR | `tenant/*/` |
| `/t/{slug}/site`, `/docs`, 404 | server-rendered | Rust handlers, inline styles |

## Seeing it (do this before you judge any UI change)

```bash
npm run build:ssr && cargo build -p gatewasm-server
cd admin && npx playwright test e2e/layout-guard.spec.ts   # harness owns :3102
```
For screenshots, copy the harness pattern: `loginAsTestUser` (auth-helper),
the `testTenant` fixture, deep-link `/platform/console/{slug}#{view}`, and
PATCH `onboarding_step` then click the tenant card to enter the wizard at a
step. Owner dashboard needs a `platform` user in `PLATFORM_OWNER_EMAILS`
**and** `role_admin` — the client boot demands the second one
(`admin/dashboard/app.ts` `ensureRole('admin')`).

## Layout rules this codebase learned the hard way

- **The element-level `button` rule leaks.** `base.css` gives every button
  `min-height: 36px`, `align-items: center`, and `white-space: nowrap`. A
  card built as a `<button>` (`.mode-card`, `.intent-card`, `.tenant-card`)
  must reset `align-items: stretch` and `white-space: normal`, or its copy
  is centred, unwrappable, and clipped at both ends by `overflow: hidden`.
  Anything meant to be a small tab must reset `min-height`.
- **Clamp every track and flex child.** `minmax(0, 1fr)` not `1fr`;
  `min(180px, 100%)` not `min-width: 180px`; `min-width: 0` on any flex or
  grid child holding text that can be long.
- **Truncate identifiers, keep badges.** Wrap the email/name in
  `<span class="truncate">` and let the status badge stay `flex: none`.
- **Absolutely-positioned menus need `max-width: calc(100vw - 2rem)`.**
- **Tables live in `.table-wrap`** — that is what makes them scroll, and
  the only place sticky `th` is allowed.
- **No `overflow-x: hidden` on `html, body`.** It was removed deliberately;
  it converted every overflow into silent, unreachable clipping. Fix the
  source instead.
- **`.shell` is column-flex** for the console's horizontal PlaneNav; a
  `.sidebar-rail` child flips it to row via `:has()`. Don't re-add height
  constraints there.
- Verify with `admin/e2e/layout-guard.spec.ts` — add your route to
  `COVERED_ROUTES` once it passes 375/768/1280.

## Recurring failure modes

Stale-branch rendering (a nested `createShow` inside a cached branch keeps
the old DOM — the "wizard flash", and the benchmark table mounting inside
its own empty state); toggle desync in the wizard; visual baselines are
macOS-named (`*-darwin.png`) so they fail on Linux CI; every route exceeds
the 195KB budget because all 11 font files ship everywhere.

## Conventions

Design decisions get written to `docs/superpowers/{plans,specs}/`. Ratchets
that must not regress: `npm run check:ssr` (IR degradations, ≤110),
`npm run check:css` (undefined classes), `scripts/check-warning-budget.sh`
(bundle=8, clippy=25).
