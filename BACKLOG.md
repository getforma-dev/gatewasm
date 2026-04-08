# Backlog

Engineering backlog — tracked items not blocking current work. Review monthly and prune stale entries.

*Last reviewed: 2026-04-08*

---

## Performance

- [ ] **Bundle size exceeds 195KB budget on all pages** (added 2026-04-08)
  Every route ships 310-390KB JS, ~2x the threshold. Investigate tree-shaking, code splitting, and lazy-loading shared modules. Biggest offender: `/platform/admin/dashboard` at 424KB in dev mode.

## Accessibility

- [ ] **Color contrast fails WCAG 2.0 AA on dark theme** (added 2026-04-08)
  axe-core reports `color-contrast` violations across all pages. Key issues: muted text (`#6b6a72` on `#111114` = 3.52:1, needs 4.5:1), form labels (`#52525b` on `#111114` = 2.43:1). Requires design-system CSS changes.

- [ ] **Link-in-text-block on onboarding page** (added 2026-04-08)
  "Sign in" link in the onboarding footer has insufficient contrast with surrounding text (2.22:1, needs 3:1) and no underline/style to distinguish it from body text.

## Testing

- [ ] **Remaining `waitForTimeout` calls in persona-workflows.spec.ts** (added 2026-04-08)
  ~30 instances of `waitForTimeout(500-800ms)` after tab/plane clicks. Should be replaced with condition-based waits when the UI exposes stable indicators (e.g., content headings, loading states).

- [ ] **Visual regression snapshots are platform-specific** (added 2026-04-08)
  Baseline PNGs are named `*-darwin.png` — CI on Linux will generate different baselines. Add Linux snapshots when e2e runs in CI, or configure `toHaveScreenshot` with platform-agnostic thresholds.

## Infrastructure

- [ ] **E2e tests not yet validated in CI** (added 2026-04-08)
  The `e2e-tests` job was added to `ci.yml` but hasn't run on a real PR yet. Verify Mailpit installs on Ubuntu runners (may need `apt` or a Docker sidecar instead of `brew`).
