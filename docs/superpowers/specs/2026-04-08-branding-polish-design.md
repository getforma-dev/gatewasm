# Branding Step Polish — Design Spec

**Date:** 2026-04-08
**Status:** Approved
**Scope:** Wizard branding step — new controls, pixel-accurate preview, scoped CSS editor

---

## Problem

The onboarding wizard's branding preview does not match the real tenant login page. Color tokens are approximated instead of exact, card proportions differ, and operators lack control over background/card colors. Power users have no way to customize fonts, spacing, or other CSS properties.

## Solution

Three changes:
1. Add background color, card color, and scoped custom CSS controls to the branding step
2. Update the LoginPreview to use actual theme token values for pixel accuracy
3. Add server-side support for the three new fields with CSS sanitization

---

## 1. New Branding Controls

The branding step's left pane, top to bottom:

| # | Control | Field | Default (dark) | Default (light) |
|---|---------|-------|-----------------|------------------|
| 1 | App name | `branding_app_name` | tenant name | tenant name |
| 2 | Logo | `branding_logo_url` | null | null |
| 3 | Primary color | `branding_primary_color` | `#6366f1` | `#6366f1` |
| 4 | Background color | `branding_bg_color` **new** | `#0a0a0c` | `#f8fafc` |
| 5 | Card background | `branding_card_color` **new** | `#111114` | `#ffffff` |
| 6 | Dark mode toggle | `branding_dark_mode` | true | false |
| 7 | Button text | `branding_button_text` | "Sign in" | "Sign in" |
| 8 | Remove fAuth branding | (disabled, paid plan) | — | — |
| 9 | Custom CSS | `branding_custom_css` **new** | null | null |

### Dark mode toggle behavior

Toggling dark/light flips the *default* values for background and card colors. If the operator has manually set a custom value for either, the toggle preserves those manual values. A "Reset to default" link appears next to each color picker when the value differs from the theme default for the current mode.

### Custom CSS section

- Collapsed by default, expands on click
- Monospace `<textarea>`, dark background, ~8 lines tall
- Hint text: "Scoped to the login card. Properties like `font-family`, `color`, `border-radius` work. No `url()`, `@import`, or `expression()`."
- Max 2000 characters
- Updates the preview live as the operator types
- Sanitized both client-side (for preview) and server-side (on save)

---

## 2. Preview Accuracy

### Token-accurate colors

The LoginPreview CSS custom properties will use the exact values from `tokens.css`:

**Dark mode tokens:**
- `--lp-bg: #0a0a0c`
- `--lp-card-bg: #111114`
- `--lp-card-border: #2a2a33`
- `--lp-text: #e8e6e3`
- `--lp-text-dim: #6b6a72`
- `--lp-text-muted: #52525b`
- `--lp-input-bg: #111114`
- `--lp-input-border: #2a2a33`
- `--lp-oauth-bg: #16161a`

**Light mode tokens:**
- `--lp-bg: #f8fafc`
- `--lp-card-bg: #ffffff`
- `--lp-card-border: #cbd5e1`
- `--lp-text: #0f172a`
- `--lp-text-dim: #475569`
- `--lp-text-muted: #94a3b8`
- `--lp-input-bg: #f1f5f9`
- `--lp-input-border: #cbd5e1`
- `--lp-oauth-bg: #f1f5f9`

When the operator sets custom bg/card colors, those override the token defaults.

### Card proportions

Match the real `.tl-card`:
- `max-width: 400px`
- `padding: 2rem 1.75rem`
- `gap: 1rem`
- Font sizes match the real page (not scaled down)

### Missing elements

Add to the preview:
- "Don't have an account? Sign up" toggle text at the bottom
- Proper `box-shadow: var(--shadow-lg)` equivalent on the card

### Custom CSS in preview

The sanitized CSS snippet is applied to the `.lp-card` element reactively. The operator sees exactly what end users will see.

---

## 3. Data Model & Server Changes

### New tenant fields

| Column | Type | Nullable | Max length | Default |
|--------|------|----------|------------|---------|
| `branding_bg_color` | text | yes | 20 | null |
| `branding_card_color` | text | yes | 20 | null |
| `branding_custom_css` | text | yes | 2000 | null |

### Database migration

Add three nullable text columns to the `tenants` table.

### API changes

The existing `PATCH /platform/self-serve/tenants/:id` endpoint gains three new allowed fields. No new endpoints.

### TypeScript type update

Add to the `Tenant` interface:
```typescript
branding_bg_color?: string | null;
branding_card_color?: string | null;
branding_custom_css?: string | null;
```

Add to `updateTenant` allowed fields.

---

## 4. CSS Sanitization

Runs in two places — client-side (preview feedback) and server-side (authority). Same logic.

### Input model

The operator writes CSS **declarations only** (no selectors, no at-rules):
```css
font-family: 'Inter', sans-serif;
border-radius: 12px;
font-size: 1.1rem;
```

The system wraps them in `.tl-card { ... }` at render time.

### Blocked patterns (stripped)

| Pattern | Reason |
|---------|--------|
| `url(` | Blocks external resource loading |
| `@import` | Blocks stylesheet injection |
| `expression(` | Blocks IE expression attacks |
| `javascript:` | Blocks script protocol |
| `-moz-binding` | Blocks XBL binding |
| `behavior:` | Blocks IE HTC behaviors |
| `var(--` containing `url` | Blocks indirect URL injection |
| `<` and `>` characters | Blocks HTML injection |
| `\` backslash sequences | Blocks Unicode escape obfuscation |

### Server rendering

In `hosted_login.rs`, the personality CSS becomes:
```css
body {
  background: <bg_color or #0a0a0c>;
  color: #e8e6e3;
  --accent: <primary_color>;
}
.tl-card {
  <card_color ? "background: <card_color>;" : "">
  <sanitized_custom_css>
}
```

Injected into the nonce-tagged `<style>` block. CSP safe.

### Limits

- Max 2000 characters pre-sanitization
- If entirely stripped, store null

---

## 5. Deliberate Cuts

- No font picker UI — operators use the CSS snippet
- No HTML editor — CSS-only keeps the attack surface small
- No image upload — URL-only for logos
- No per-element styling UI — the CSS snippet covers power users

---

## 6. Files to Change

### Frontend
- `admin/src/platform/console/OnboardingWizard.ts` — add bg color, card color pickers; add custom CSS collapsible; pass new props to LoginPreview
- `admin/src/platform/console/LoginPreview.ts` — update token values to match `tokens.css`; support bg/card color overrides; apply custom CSS to card; fix card proportions; add missing elements
- `admin/src/platform/console/store.ts` — add new fields to `Tenant` interface and `updateTenant` allowed fields
- `admin/src/core/theme/components.css` — update `.lp-*` classes for accurate proportions; add custom CSS editor styles
- New: `admin/src/platform/console/sanitizeCSS.ts` — shared client-side CSS sanitizer

### Backend
- `crates/server/src/handlers/hosted_login.rs` — update personality CSS to include bg color, card color, custom CSS
- `crates/server/src/handlers/platform_dashboard.rs` — already has relaxed CSP for img-src
- Tenant model/migration — add three new columns
- Self-serve PATCH handler — add new fields to allowed list
- New: server-side CSS sanitizer (Rust) — same blocked patterns as client-side
