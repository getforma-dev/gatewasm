# GateWASM Dashboard — Layout, UX & Design System Spec

> Validated design spec. See implementation plan at `../plans/2026-04-03-design-system.md`.

## Decisions

- **Token naming**: Keep current unprefixed names (`--bg`, `--surface`, `--accent`, etc.). Do NOT rename to `--gw-*`.
- **Icon strategy**: Icon component with path registry. SVGs from Iconify MDI set (offline, build-time). Use `currentColor`.
- **Shell**: Upgrade from flexbox to CSS Grid. Add topbar row with breadcrumbs.
- **Sidebar**: Add section headers, tooltip on collapsed state, mobile overlay with backdrop.
- **Animation**: Add `prefers-reduced-motion` support. Expand timing/easing tokens.
- **Dark mode**: Currently single dark theme. Add `prefers-color-scheme` media query for light mode override.
- **Phases**: Foundation -> Shell & Layout -> Components -> Templates & Polish.

## Scope

Covers: shell grid, topbar, breadcrumbs, sidebar enhancements, icon system, 4 page templates, animation system, skeleton/tooltip/empty-state/modal/dropdown components, responsive strategy, micro-interactions, typography tokens, keyboard shortcuts, dark mode.

Does NOT cover: new pages/views, API changes, store refactoring, Rust backend changes.
