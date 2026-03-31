# Platform Console Redesign Plan

The platform console (`/platform/console`) is the first thing a new customer sees after signup. It needs to guide them through setup, not overwhelm them with configuration.

---

## Current Problems

### Tenant List View
- **Integration Guide is dismissable and never comes back.** If dismissed, the customer loses the only onboarding content.
- **No clear next step.** Customer sees stats and a table row but doesn't know what to do.
- **Quickstart is too much at once.** Bridge.js code, SDK init, backend automation, all in one wall of text.

### Tenant Detail View
- **Everything dumped on one page.** Configuration, branding, auth config, API keys, stats, quick links, delete button — no hierarchy.
- **No visual separation.** Labels like "TENANT NAME", "URL SLUG", "PLAN", "CREATED" are just stacked text with inputs.
- **Delete Tenant is casually placed** next to quick links. Dangerous action with no visual weight difference from "Login page" link.
- **No guidance.** A new customer with 0 users and 0 sessions doesn't know what to configure first or why.
- **Branding section is premature.** Customer hasn't even tested their login page yet and we're asking about logo URLs and primary colors.

---

## Redesign Direction

### Principle: Progressive disclosure with guided setup

Show what matters NOW, reveal complexity as the customer grows.

### New Customer Experience (0 users, trial)

After signup and tenant creation, the console should feel like a **setup wizard**, not a config dump:

```
Step 1: Test your login page
  → "Your login page is live. Try it:"
  → Big button: "Open login page" → /t/{slug}/login
  → "Sign up as a test user to see what your customers will experience."

Step 2: Connect your app
  → Show the bridge.js snippet (pre-filled with their slug and server URL)
  → Copy button
  → "Add this to your app's HTML to enable authentication."

Step 3: Get your API key
  → Show the masked key with Reveal/Copy
  → "Use this in your backend to manage users programmatically."
  → curl example pre-filled with their key and slug

Step 4: Customize (optional, collapsed by default)
  → Branding: app name, logo, color
  → Auth config: providers, allowed origins, redirect URL
```

### Returning Customer Experience (has users, active)

Once the customer has users and is past initial setup, the console shifts to an **operational dashboard**:

```
Tenant overview:
  - Stats: users, active sessions, logins today
  - Quick actions: Open login page, Admin console, API docs
  - Recent activity (last 5 logins or signups)

Configuration (tabs or collapsible sections):
  - General: name, slug, redirect URL
  - Branding: logo, color, app name
  - Auth: providers, allowed origins
  - API: key management (reveal, copy, rotate)
  - Danger zone: delete tenant (bottom, red, with confirmation)
```

### Integration Guide

- **Never dismissable.** Always accessible from a "Setup Guide" or "Docs" nav item.
- **Contextual.** Show relevant steps based on what the customer has/hasn't done.
- **Pre-filled.** Code snippets use the customer's actual slug, server URL, and API key — not placeholders.

---

## Implementation Phases

### Phase 1: Fix the critical UX issues (quick wins)
- Remove dismiss on integration guide (or make it a sidebar nav item that's always accessible)
- Move Delete Tenant to a "Danger Zone" section at the very bottom with red styling
- Add visual separation between sections (cards or bordered sections)
- Pre-fill code snippets with real tenant data (slug, server URL)

### Phase 2: Guided setup for new tenants
- Detect if tenant has 0 users → show setup wizard view
- Step-by-step flow: test login → add bridge.js → get API key
- Collapse branding/config into "Customize" section

### Phase 3: Operational dashboard for active tenants
- Stats and recent activity front and center
- Configuration behind tabs
- Quick actions prominent

---

## Notes

- The tenant detail page is currently `src/platform/console/views/TenantDetail.ts`
- The tenant list is `src/platform/console/views/TenantsView.ts`
- The quickstart is `src/platform/console/views/Quickstart.ts`
- Store is `src/platform/console/store.ts`
- This is a frontend-only change — all API endpoints already exist
