// ---------------------------------------------------------------------------
// OnboardingPage.ts — SSR root component for onboarding page
// Calls island component as function — compiler Rule 11 detects this
// as opaque call and emits ISLAND_START/ISLAND_END opcodes.
// ---------------------------------------------------------------------------

import { h } from "formajs";
import { RegistrationForm } from "./RegistrationForm";

// ── Capability card data ────────────────────────────────────────────────

const CAPABILITIES = [
  {
    title: "Multi-Tenant Auth",
    description:
      "Isolated user pools per tenant with automatic provisioning and scoped access control.",
  },
  {
    title: "OAuth + MFA",
    description:
      "Google and GitHub SSO with TOTP multi-factor authentication enforcement.",
  },
  {
    title: "Webhook Events",
    description:
      "Real-time notifications for auth lifecycle events with automatic retry delivery.",
  },
  {
    title: "Bridge SDK",
    description:
      "Drop-in JavaScript SDK for instant auth integration in any web application.",
  },
];

// ── Page component ──────────────────────────────────────────────────────
// Static shell (header, hero, capabilities, CTA, footer) + island call.

export function OnboardingPage() {
  return h(
    "div",
    { class: "onboarding-page" },

    // ── Brand header (pure static) ──────────────────────────────────
    h(
      "div",
      { class: "brand-header" },
      h("span", { class: "brand-mark" }, "\u25C7"),
      h("span", null, "Forma Auth"),
    ),

    // ── Hero section (pure static) ──────────────────────────────────
    h(
      "section",
      { class: "hero-section" },
      h(
        "h1",
        { class: "hero-heading" },
        "AuthZ infrastructure for modern SaaS",
      ),
      h(
        "p",
        { class: "hero-subtext" },
        "Multi-tenant authentication with OAuth, MFA, webhooks, and a drop-in JavaScript SDK. Set up in minutes.",
      ),
    ),

    // ── Capabilities grid (pure static) ─────────────────────────────
    h(
      "div",
      { class: "capabilities" },
      ...CAPABILITIES.map((cap) =>
        h(
          "div",
          { class: "cap-card" },
          h("h3", null, cap.title),
          h("p", null, cap.description),
        ),
      ),
    ),

    // ── CTA (progressive enhancement — works without JS) ────────────
    h(
      "div",
      { class: "cta-section" },
      h(
        "a",
        {
          class: "cta-btn",
          href: "#auth-section",
        },
        "Get Started",
      ),
    ),

    // ── Auth section ────────────────────────────────────────────────
    h(
      "section",
      { class: "auth-section", id: "auth-section" },
      h(
        "div",
        { class: "card p-lg" },
        // Card heading (pure static)
        h("h2", { class: "card-heading" }, "Create your account"),
        h(
          "p",
          { class: "card-subtitle" },
          "Start building in minutes. You will choose your first workspace name and tenant URL right after signing in.",
        ),

        // Island: RegistrationForm (alerts + OAuth + form)
        RegistrationForm(),

        // Sign-in link (pure static)
        h(
          "div",
          {
            class:
              "signin-link mt-lg divider-top text-center text-sm text-muted-dim",
          },
          "Already have an account? ",
          h("a", { href: "/platform/login", class: "link-accent" }, "Sign in"),
        ),
      ),
    ),

    // ── Footer (pure static) ────────────────────────────────────────
    h(
      "footer",
      { class: "page-footer text-center pt-sm" },
      h(
        "div",
        {
          class: "footer-badge inline-flex-center gap-xs text-xxs footer-muted",
        },
        h("span", { class: "brand-mark-gradient" }, "\u25C7"),
        "Forma Auth",
      ),
    ),
  );
}
