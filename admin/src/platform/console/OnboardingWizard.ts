import { createShow, createSignal, h, onCleanup } from "formajs";
import { normalizeTenantSlugInput } from "../tenantSlug";
import { LoginPreview } from "./LoginPreview";
import type { Tenant } from "./store";
import {
  api,
  checkSlugAvailability,
  createTenant,
  currentUser,
  preferredTenantView,
  saveSecurityConfig,
  selectedTenant,
  serverUrl,
  setSelectedTenant,
  toast,
  updateTenant,
} from "./store";

// ── Types ──────────────────────────────────────────────────────────────

type TenantIntent =
  | "saas"
  | "internal"
  | "community"
  | "client-portal"
  | "marketplace";

interface IntentOption {
  key: TenantIntent;
  title: string;
  description: string;
}

const INTENT_OPTIONS: IntentOption[] = [
  {
    key: "saas",
    title: "SaaS Application",
    description: "Multi-tenant app with user accounts",
  },
  {
    key: "internal",
    title: "Internal Tool",
    description: "Company-internal app, employees only",
  },
  {
    key: "community",
    title: "Community Site",
    description: "Open registration, social features",
  },
  {
    key: "client-portal",
    title: "Client Portal",
    description: "Restricted access for your clients",
  },
  {
    key: "marketplace",
    title: "Marketplace / API",
    description: "Developer platform, API keys, OAuth clients",
  },
];

interface IntentDefaults {
  signup_mode: string;
  idle_timeout_minutes: number;
  max_sessions_per_user: number;
  mfa_policy: string;
  allowed_providers: string[];
  preferred_tab: string;
}

const INTENT_DEFAULTS: Record<TenantIntent, IntentDefaults> = {
  saas: {
    signup_mode: "open",
    idle_timeout_minutes: 30,
    max_sessions_per_user: 5,
    mfa_policy: "recommended",
    allowed_providers: ["email", "google"],
    preferred_tab: "integrate",
  },
  internal: {
    signup_mode: "invite_only",
    idle_timeout_minutes: 15,
    max_sessions_per_user: 2,
    mfa_policy: "required",
    allowed_providers: ["email", "google"],
    preferred_tab: "integrate",
  },
  community: {
    signup_mode: "open",
    idle_timeout_minutes: 60,
    max_sessions_per_user: 10,
    mfa_policy: "off",
    allowed_providers: ["email", "google", "github"],
    preferred_tab: "hosted-page",
  },
  "client-portal": {
    signup_mode: "invite_only",
    idle_timeout_minutes: 30,
    max_sessions_per_user: 3,
    mfa_policy: "recommended",
    allowed_providers: ["email"],
    preferred_tab: "hosted-page",
  },
  marketplace: {
    signup_mode: "open",
    idle_timeout_minutes: 60,
    max_sessions_per_user: 10,
    mfa_policy: "off",
    allowed_providers: ["email", "github"],
    preferred_tab: "integrate",
  },
};

const STEP_LABELS = [
  "Create",
  "Intent",
  "Auth",
  "Branding",
  "Mode",
  "Test",
  "Launch",
];

const RESERVED_SLUGS = [
  "platform",
  "api",
  "admin",
  "manage",
  "auth",
  "health",
  "docs",
  "www",
  "app",
  "dashboard",
  "billing",
  "support",
  "login",
  "register",
  "signup",
  "get-started",
  "bridge",
  "openapi",
  "t",
  "static",
  "assets",
];

// ── Shared Signals ─────────────────────────────────────────────────────

const [wizardStep, setWizardStep] = createSignal(0);
const [saving, setSaving] = createSignal(false);

// ── Hash navigation ────────────────────────────────────────────────────
//
// The wizard uses prefixed URL hash fragments (#wizard-create, #wizard-intent,
// #wizard-auth, #wizard-branding, #wizard-mode, #wizard-test, #wizard-launch)
// for in-session step navigation. The prefix avoids collision with the
// console's view-based hash routing (#overview, #integrate, etc.) and tab
// routing (#tenants, #team, etc.) — bare wizard step names like "auth" or
// "launch" could otherwise be mis-interpreted by the console's hashchange
// listener after the wizard unmounts.
//
// Forward navigation through the wizard is gated on the persisted
// onboarding_step — typing #wizard-launch in the URL won't let a user jump
// past auth validation. Backward navigation to any previously-reached step
// works normally.

const WIZARD_HASH_PREFIX = "wizard-";

// When the wizard writes to the hash itself (advanceStep / prevStep / mount /
// handleTenantCreated), the resulting hashchange event MUST be ignored by the
// listener. Otherwise the listener fires synchronously with stale tenant state
// (the underlying updateTenant call is fire-and-forget), reads a stale
// maxReachedStep, and bounces the user back to the previous step.
let wizardWroteHash = false;

function stepFromHash(): number {
  if (typeof window === "undefined") return -1;
  const raw = window.location.hash.slice(1).toLowerCase();
  if (!raw.startsWith(WIZARD_HASH_PREFIX)) return -1;
  const label = raw.slice(WIZARD_HASH_PREFIX.length);
  if (!label) return -1;
  return STEP_LABELS.findIndex((l) => l.toLowerCase() === label);
}

function setStepHash(step: number, opts: { replace?: boolean } = {}): void {
  if (typeof window === "undefined") return;
  const label = STEP_LABELS[step]?.toLowerCase();
  if (!label) return;
  const target = WIZARD_HASH_PREFIX + label;
  if (window.location.hash.slice(1) === target) return;
  wizardWroteHash = true;
  if (opts.replace) {
    // Same path/search, new hash — replaceState avoids piling onto history.
    history.replaceState(
      history.state,
      "",
      window.location.pathname + window.location.search + "#" + target,
    );
  } else {
    window.location.hash = target;
  }
}

/** Highest step index the operator has actually reached, given the persisted onboarding_step. */
function maxReachedStep(): number {
  const t = selectedTenant();
  if (!t) return 0;
  // onboarding_step is 1-indexed and points at the NEXT step to reach. The
  // highest step the user has unlocked is therefore onboarding_step - 1,
  // capped at the last index (6).
  return Math.min(Math.max(t.onboarding_step - 1, 1), 6);
}

// ── Navigation ─────────────────────────────────────────────────────────

function advanceStep(): void {
  const next = Math.min(wizardStep() + 1, 6);
  setWizardStep(next);
  setStepHash(next);
  const tenant = selectedTenant();
  if (tenant) {
    void updateTenant(tenant.id, { onboarding_step: next + 1 });
  }
}

function prevStep(): void {
  // Don't allow going back to the Create step (0) once tenant exists
  const min = selectedTenant() ? 1 : 0;
  const next = Math.max(wizardStep() - 1, min);
  setWizardStep(next);
  setStepHash(next);
}

async function skipWizard(): Promise<void> {
  const tenant = selectedTenant();
  if (!tenant) return;
  if (!tenant.tenant_intent) {
    const defaults = INTENT_DEFAULTS.saas;
    await updateTenant(tenant.id, {
      tenant_intent: "saas",
      preferred_tab: defaults.preferred_tab,
      allowed_providers: defaults.allowed_providers,
      signup_mode: defaults.signup_mode,
    });
    await saveSecurityConfig(tenant.id, {
      idle_timeout_minutes: defaults.idle_timeout_minutes,
      max_sessions_per_user: defaults.max_sessions_per_user,
      mfa_policy: defaults.mfa_policy,
    });
  } else {
    // Tenant already has an intent set, but allowed_providers may have been
    // emptied while the operator was navigating the wizard. Defensive backfill
    // with the intent's defaults — this matches what the skip modal preview
    // shows the operator they'll get, so the modal can't lie about providers.
    const currentProviders =
      (tenant.allowed_providers as string[] | undefined) || [];
    if (currentProviders.length === 0) {
      const intent = (tenant.tenant_intent as TenantIntent) || "saas";
      const intentDefaults =
        INTENT_DEFAULTS[intent] || INTENT_DEFAULTS.saas;
      await updateTenant(tenant.id, {
        allowed_providers: intentDefaults.allowed_providers,
      });
    }
  }
  const updated = await updateTenant(tenant.id, {
    onboarding_completed: true,
    onboarding_step: 7,
  });
  if (!updated) return;
  navigateToWorkspace(updated);
}

/**
 * Hard-navigate from the wizard into the launched workspace. The in-session
 * createShow transition from wizard → workspace shell can race with hydrate
 * and leave the workspace mounted with stale view-data signals (the operator
 * sees an empty pane until they refresh manually). Doing the navigation as a
 * full page load via window.location.href guarantees init() runs and the
 * workspace shell mounts cleanly with the correct view + plane + hydrated
 * data — exactly what the operator's manual refresh would have achieved.
 *
 * The welcome toast is persisted to sessionStorage so it survives the page
 * reload and gets displayed by init() after the workspace shell mounts; an
 * in-memory toast.show() before navigation would be destroyed by the unload.
 */
function navigateToWorkspace(tenant: Tenant): void {
  if (typeof window === "undefined") return;
  try {
    sessionStorage.setItem("forma:welcome-toast", "Your application is ready");
  } catch {
    // sessionStorage unavailable (private browsing) — toast just won't show.
  }
  const targetView = preferredTenantView(tenant);
  window.location.href =
    "/platform/console/" + encodeURIComponent(tenant.slug) + "#" + targetView;
}

// ── Step 0: Create Application ────────────────────────────────────────

function StepCreate(onCreated: (tenant: Tenant) => void): ReturnType<typeof h> {
  const [name, setName] = createSignal("");
  const [slug, setSlug] = createSignal("");
  const [slugEdited, setSlugEdited] = createSignal(false);
  const [slugStatus, setSlugStatus] = createSignal<
    "idle" | "checking" | "available" | "taken" | "invalid"
  >("idle");
  const [slugWarn, setSlugWarn] = createSignal("");
  const [slugError, setSlugError] = createSignal("");
  const [slugSuggestion, setSlugSuggestion] = createSignal("");
  let checkTimer: ReturnType<typeof setTimeout> | undefined;

  onCleanup(() => clearTimeout(checkTimer));

  function validateAndCheckSlug(raw: string, normalized: string) {
    clearTimeout(checkTimer);
    if (slugWarn()) setSlugWarn("");
    if (slugError()) setSlugError("");

    if (raw !== normalized && raw.length > 0) {
      const stripped = raw.replace(/[a-z0-9-]/g, "");
      if (stripped.length > 0) setSlugWarn("Invalid characters removed");
    }

    if (!normalized) {
      setSlugStatus("idle");
      return;
    }
    if (normalized.length < 2) {
      setSlugStatus("invalid");
      setSlugError("Slug must be at least 2 characters");
      return;
    }
    if (normalized.length > 63) {
      setSlugStatus("invalid");
      setSlugError("Slug must be 63 characters or fewer");
      return;
    }

    if (RESERVED_SLUGS.includes(normalized)) {
      setSlugStatus("invalid");
      setSlugError("\u201C" + normalized + "\u201D is reserved");
      return;
    }

    setSlugStatus("checking");
    checkTimer = setTimeout(async () => {
      try {
        const available = await checkSlugAvailability(normalized);
        if (slug() === normalized) {
          if (available) {
            setSlugStatus("available");
          } else {
            setSlugStatus("taken");
            setSlugSuggestion(
              normalized + "-" + Math.random().toString(36).slice(2, 5),
            );
          }
        }
      } catch {
        if (slug() === normalized) setSlugStatus("idle");
      }
    }, 500);
  }

  function handleNameChange(value: string) {
    setName(value);
    if (!slugEdited()) {
      const normalized = normalizeTenantSlugInput(value);
      setSlug(normalized);
      validateAndCheckSlug(normalized, normalized);
    }
  }

  function handleSlugInput(e: Event) {
    setSlugEdited(true);
    const input = e.target as HTMLInputElement;
    const raw = input.value;
    const normalized = normalizeTenantSlugInput(raw);
    if (input.value !== normalized) {
      const pos = input.selectionStart ?? normalized.length;
      const removed = raw.length - normalized.length;
      input.value = normalized;
      input.setSelectionRange(
        Math.max(0, pos - removed),
        Math.max(0, pos - removed),
      );
    }
    setSlug(normalized);
    validateAndCheckSlug(raw.trim().toLowerCase(), normalized);
  }

  async function handleCreate(): Promise<void> {
    const nextName = name().trim();
    const nextSlug = slug().trim();
    if (!nextName) {
      toast.show("App name is required", "error");
      return;
    }
    if (!nextSlug) {
      toast.show("Tenant slug is required", "error");
      return;
    }
    if (slugStatus() === "taken") {
      toast.show("This slug is already taken", "error");
      return;
    }
    setSaving(true);
    const created = await createTenant({ name: nextName, slug: nextSlug });
    setSaving(false);
    if (!created) return;
    onCreated(created);
  }

  const canCreate = () => {
    const n = name().trim();
    const s = slug().trim();
    const st = slugStatus();
    return (
      n &&
      s &&
      st !== "taken" &&
      st !== "checking" &&
      st !== "invalid" &&
      !saving()
    );
  };

  return h(
    "div",
    { class: "wizard-step wizard-step-centered" },
    h("h1", { class: "wizard-headline" }, "Create your application"),
    h(
      "p",
      { class: "wizard-subtitle" },
      "Choose a name and slug. The slug becomes part of your hosted URLs.",
    ),

    h(
      "div",
      { class: "wizard-create-form" },
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "App Name"),
        h("input", {
          type: "text",
          class: "input",
          placeholder: "Acme App",
          value: name(),
          onInput: (e: Event) =>
            handleNameChange((e.target as HTMLInputElement).value),
        }),
        h(
          "span",
          { class: "wizard-field-hint" },
          "This appears in the console and on your hosted sign-in page.",
        ),
      ),

      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Tenant Slug"),
        h("input", {
          type: "text",
          class: "input",
          placeholder: "acme-app",
          value: slug,
          onInput: handleSlugInput,
        }),
        h(
          "span",
          { class: "wizard-field-hint" },
          "Lowercase letters, numbers, and hyphens only.",
        ),
      ),

      h(
        "div",
        { class: "slug-status" },
        createShow(
          () => slugWarn().length > 0,
          () => h("span", { class: "slug-status-hint" }, slugWarn),
        ),
        createShow(
          () => slugStatus() === "checking",
          () =>
            h(
              "span",
              { class: "slug-status-checking" },
              "Checking availability...",
            ),
        ),
        createShow(
          () => slugStatus() === "available",
          () =>
            h(
              "span",
              { class: "slug-status-available" },
              () => "\u2713 " + slug() + " is available",
            ),
        ),
        createShow(
          () => slugStatus() === "invalid",
          () =>
            h(
              "span",
              { class: "slug-status-invalid" },
              () => "\u2717 " + slugError(),
            ),
        ),
        createShow(
          () => slugStatus() === "taken",
          () =>
            h(
              "div",
              { class: "slug-status-taken" },
              h("span", null, "\u2717 Already taken"),
              h(
                "button",
                {
                  type: "button",
                  class: "slug-suggestion",
                  onClick: () => {
                    const s = slugSuggestion();
                    setSlug(s);
                    setSlugEdited(true);
                    validateAndCheckSlug(s, s);
                  },
                },
                () => "Try " + slugSuggestion(),
              ),
            ),
        ),
      ),

      createShow(
        () => slug().length > 0,
        () =>
          h(
            "div",
            { class: "wizard-url-preview" },
            h("span", { class: "wizard-url-preview-label" }, "Tenant URL"),
            h("code", null, () => "/t/" + slug()),
          ),
      ),
    ),

    h(
      "div",
      { class: "wizard-actions" },
      h(
        "button",
        {
          type: "button",
          class: "btn btn-accent",
          disabled: () => !canCreate(),
          onClick: () => void handleCreate(),
        },
        () => (saving() ? "Creating\u2026" : "Create & Continue \u2192"),
      ),
    ),
  );
}

// ── Step 1: Intent Capture ────────────────────────────────────────────

function StepIntent(): ReturnType<typeof h> {
  const [selectedIntent, setSelectedIntent] = createSignal<TenantIntent | null>(
    (selectedTenant()?.tenant_intent as TenantIntent) || null,
  );

  async function handleContinue(): Promise<void> {
    const tenant = selectedTenant();
    const pick = selectedIntent();
    if (!tenant || !pick) return;

    setSaving(true);
    const defaults = INTENT_DEFAULTS[pick];
    await updateTenant(tenant.id, {
      tenant_intent: pick,
      preferred_tab: defaults.preferred_tab,
      allowed_providers: defaults.allowed_providers,
      signup_mode: defaults.signup_mode,
    });
    await saveSecurityConfig(tenant.id, {
      idle_timeout_minutes: defaults.idle_timeout_minutes,
      max_sessions_per_user: defaults.max_sessions_per_user,
      mfa_policy: defaults.mfa_policy,
    });
    setSaving(false);
    advanceStep();
  }

  return h(
    "div",
    { class: "wizard-step wizard-step-centered" },
    h("h1", { class: "wizard-headline" }, "What are you building?"),
    h(
      "p",
      { class: "wizard-subtitle" },
      "This helps us configure sensible defaults so you don\u2019t have to.",
    ),
    h(
      "div",
      { class: "intent-grid" },
      ...INTENT_OPTIONS.map((opt) =>
        h(
          "button",
          {
            type: "button",
            class: () =>
              "intent-card" +
              (selectedIntent() === opt.key ? " intent-card-selected" : ""),
            onClick: () => setSelectedIntent(opt.key),
          },
          h("div", { class: "intent-card-title" }, opt.title),
          h("div", { class: "intent-card-desc" }, opt.description),
        ),
      ),
    ),
    h(
      "div",
      { class: "wizard-actions" },
      h(
        "button",
        {
          type: "button",
          class: "btn btn-accent",
          disabled: () => selectedIntent() === null || saving(),
          onClick: () => void handleContinue(),
        },
        () => (saving() ? "Saving\u2026" : "Continue \u2192"),
      ),
    ),
  );
}

// ── Step 2: Auth Methods ──────────────────────────────────────────────

function StepAuthMethods(): ReturnType<typeof h> {
  const _t = selectedTenant();
  if (!_t) return h("div", null, "No tenant selected");
  const tenant: Tenant = _t;

  const current = (tenant.allowed_providers as string[] | undefined) || [
    "email",
  ];
  const [providers, setProviders] = createSignal<string[]>([...current]);

  const currentSignupMode = (tenant.signup_mode as string | undefined) || "open";
  const [signupMode, setSignupMode] = createSignal<string>(currentSignupMode);

  function toggleProvider(p: string): void {
    const list = providers();
    if (list.includes(p)) {
      // Always require at least one provider — invited users still need an
      // auth method to log in after accepting their invite. The visual
      // rejection happens at the click handler via preventDefault (see the
      // input below); this is a defense-in-depth so anything else that calls
      // toggleProvider also can't reach the empty state.
      if (list.length === 1) return;
      setProviders(list.filter((x) => x !== p));
    } else {
      setProviders([...list, p]);
    }
  }

  async function handleContinue(): Promise<void> {
    if (providers().length === 0) return; // defensive: button is also disabled
    setSaving(true);
    await updateTenant(tenant.id, {
      allowed_providers: providers(),
      signup_mode: signupMode(),
    });
    setSaving(false);
    advanceStep();
  }

  // Derive branding from tenant for preview
  const previewAppName = () => tenant.branding_app_name || tenant.name;
  const previewLogoUrl = () => tenant.branding_logo_url || "";
  const previewPrimaryColor = () => tenant.branding_primary_color || "#6366f1";
  const previewDarkMode = () => tenant.branding_dark_mode ?? true;
  const previewButtonText = () => tenant.branding_button_text || "Sign in";

  const CREDENTIAL_PROVIDERS = [
    { key: "email", label: "Email / Password" },
    {
      key: "magic_link",
      label: "Magic Link",
      subtitle: "Passwordless sign-in via email",
    },
    {
      key: "email_otp",
      label: "Email OTP",
      subtitle: "6-digit code sent to email",
      available: false,
    },
    {
      key: "passkey",
      label: "Passkey",
      subtitle: "Sign in with Face ID, fingerprint, or device PIN",
      available: false,
    },
  ];

  const SOCIAL_PROVIDERS = [
    { key: "google", label: "Google", available: true },
    { key: "github", label: "GitHub", available: true },
    { key: "microsoft", label: "Microsoft", available: false },
    { key: "apple", label: "Apple", available: false },
  ];

  function ProviderToggle(opt: {
    key: string;
    label: string;
    subtitle?: string;
    available?: boolean;
  }): ReturnType<typeof h> {
    const disabled = opt.available === false;
    // Outer is a <div>, NOT a <label>. The wizard-toggle inside is already a
    // <label> wrapping the <input>, so the click hits exactly one form-control
    // association. A nested-label structure here causes the input's change
    // event to fire twice — once via each label — which silently undoes the
    // toggle and made the social provider toggles appear to "flip back."
    return h(
      "div",
      {
        class: () =>
          "wizard-provider-toggle" +
          (disabled ? " wizard-provider-disabled" : ""),
        onClick: (e: Event) => {
          if (disabled) return;
          // Only react when the click landed OUTSIDE the toggle label — clicks
          // inside the toggle label are handled by the input's native change.
          const target = e.target as HTMLElement;
          if (target.closest(".wizard-toggle")) return;
          toggleProvider(opt.key);
        },
      },
      h(
        "div",
        { class: "wizard-provider-info" },
        h("span", { class: "wizard-provider-label" }, opt.label),
        opt.subtitle
          ? h("span", { class: "wizard-provider-subtitle" }, opt.subtitle)
          : null,
        disabled
          ? h("span", { class: "wizard-provider-badge" }, "Coming soon")
          : null,
      ),
      h(
        "label",
        { class: "wizard-toggle" },
        h("input", {
          type: "checkbox",
          checked: () => providers().includes(opt.key),
          disabled: disabled,
          // preventDefault on click cancels the browser's optimistic visual
          // flip BEFORE it happens, so the at-least-one-provider guard can
          // refuse the change without leaving the visual desynced from the
          // signal. Reactive boolean attribute bindings in formajs cache the
          // last applied value and won't re-write the DOM when the signal
          // hasn't changed, so we can't repair the desync after the fact.
          onClick: (e: Event) => {
            const list = providers();
            if (list.length === 1 && list.includes(opt.key)) {
              e.preventDefault();
            }
          },
          onChange: () => toggleProvider(opt.key),
        }),
        h("span", { class: "wizard-toggle-track" }),
      ),
    );
  }

  return h(
    "div",
    { class: "wizard-step wizard-split-pane" },
    h(
      "div",
      { class: "wizard-pane-left" },
      h("h1", { class: "wizard-headline" }, "How will users sign in?"),
      h(
        "p",
        { class: "wizard-subtitle" },
        "Choose authentication methods for your app.",
      ),

      h(
        "div",
        { class: "wizard-provider-group" },
        h("div", { class: "wizard-provider-group-label" }, "Access"),
        h(
          "div",
          { class: "wizard-access-segmented" },
          h(
            "button",
            {
              type: "button",
              class: () =>
                "wizard-access-segmented-option" +
                (signupMode() === "open" ? " active" : ""),
              onClick: () => setSignupMode("open"),
            },
            h("span", { class: "wizard-access-segmented-option-label" }, "Open"),
            h(
              "span",
              { class: "wizard-access-segmented-option-subtitle" },
              "Anyone can create an account.",
            ),
          ),
          h(
            "button",
            {
              type: "button",
              class: () =>
                "wizard-access-segmented-option" +
                (signupMode() === "invite_only" ? " active" : ""),
              onClick: () => setSignupMode("invite_only"),
            },
            h(
              "span",
              { class: "wizard-access-segmented-option-label" },
              "Invite-only",
            ),
            h(
              "span",
              { class: "wizard-access-segmented-option-subtitle" },
              "Only people you invite can sign up. You'll manage invites from the dashboard.",
            ),
          ),
        ),
      ),

      h(
        "div",
        { class: "wizard-provider-group" },
        h("div", { class: "wizard-provider-group-label" }, "Credentials"),
        ...CREDENTIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h(
        "div",
        { class: "wizard-provider-group" },
        h(
          "div",
          { class: "wizard-provider-group-label" },
          "Social Providers",
        ),
        ...SOCIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h(
        "div",
        { class: "wizard-pro-section" },
        h(
          "div",
          { class: "wizard-pro-header" },
          h("span", { class: "wizard-provider-badge" }, "Pro"),
          h("span", { class: "wizard-pro-label" }, "Upgrade to unlock"),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "SMS OTP"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "SAML / SSO"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Custom OIDC"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
      ),

      h(
        "div",
        {
          class: () =>
            "wizard-form-warning" +
            (providers().length === 0 ? "" : " wizard-form-warning-hidden"),
        },
        "Pick at least one sign-in method to continue. Invited users still need a way to sign in after they accept their invite.",
      ),

      h(
        "div",
        { class: "wizard-actions" },
        h(
          "button",
          {
            type: "button",
            class: "btn btn-accent",
            disabled: () => saving() || providers().length === 0,
            onClick: () => void handleContinue(),
          },
          () => (saving() ? "Saving\u2026" : "Continue \u2192"),
        ),
      ),
    ),

    h(
      "div",
      { class: "wizard-pane-right" },
      h("div", { class: "wizard-preview-label" }, "Live Preview"),
      LoginPreview({
        appName: previewAppName,
        logoUrl: previewLogoUrl,
        primaryColor: previewPrimaryColor,
        darkMode: previewDarkMode,
        buttonText: previewButtonText,
        providers,
        slug: () => tenant.slug,
        signupMode,
      }),
    ),
  );
}

// ── Step 3: Branding ──────────────────────────────────────────────────

function StepBranding(): ReturnType<typeof h> {
  const _t = selectedTenant();
  if (!_t) return h("div", null, "No tenant selected");
  const tenant: Tenant = _t;

  const [appName, setAppName] = createSignal(
    tenant.branding_app_name || tenant.name,
  );
  const [logoUrl, setLogoUrl] = createSignal(tenant.branding_logo_url || "");
  const [buttonColor, setButtonColor] = createSignal(
    tenant.branding_primary_color || "#6366f1",
  );
  const [buttonText, setButtonText] = createSignal(
    tenant.branding_button_text || "Sign in",
  );
  const [cardColor, setCardColor] = createSignal(
    tenant.branding_card_color || "",
  );
  const [fontFamily, setFontFamily] = createSignal(
    tenant.branding_font_family || "System default",
  );
  const [textColor, setTextColor] = createSignal(
    tenant.branding_text_color || "",
  );

  const defaultCard = () => "#111114";
  const defaultText = () => "#e0e0e0";

  const previewProviders = () =>
    (tenant.allowed_providers as string[] | undefined) || ["email"];

  const FONT_OPTIONS = [
    "System default",
    "Inter",
    "DM Sans",
    "Roboto",
    "Lato",
    "Source Sans Pro",
    "Poppins",
    "Nunito",
    "Outfit",
  ];

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      branding_app_name: appName(),
      branding_logo_url: logoUrl() || null,
      branding_primary_color: buttonColor(),
      branding_button_text: buttonText(),
      branding_card_color: cardColor() || null,
      branding_font_family:
        fontFamily() === "System default" ? null : fontFamily(),
      branding_text_color: textColor() || null,
    });
    setSaving(false);
    advanceStep();
  }

  return h(
    "div",
    { class: "wizard-step wizard-split-pane" },
    h(
      "div",
      { class: "wizard-pane-left" },
      h("h1", { class: "wizard-headline" }, "Style your sign-in card"),
      h(
        "p",
        { class: "wizard-subtitle" },
        "Customize how the login card looks to your end users.",
      ),

      // ── App Name ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "App name"),
        h("input", {
          type: "text",
          class: "input",
          value: appName(),
          onInput: (e: Event) =>
            setAppName((e.target as HTMLInputElement).value),
        }),
      ),

      // ── Logo (same as before) ──
      h(
        "div",
        { class: "wizard-field" },
        h("span", null, "Logo"),
        createShow(
          () => logoUrl() === "",
          () =>
            h(
              "div",
              { class: "logo-box logo-box-empty" },
              h(
                "div",
                { class: "logo-box-icon" },
                h(
                  "svg",
                  {
                    width: "24",
                    height: "24",
                    viewBox: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    "stroke-width": "1.5",
                  },
                  h("rect", {
                    x: "3",
                    y: "3",
                    width: "18",
                    height: "18",
                    rx: "2",
                  }),
                  h("circle", { cx: "8.5", cy: "8.5", r: "1.5" }),
                  h("path", { d: "M21 15l-5-5L5 21" }),
                ),
              ),
              h("div", { class: "logo-box-hint" }, "Paste an image URL below"),
              h("input", {
                type: "url",
                class: "input logo-box-input",
                placeholder: "https://example.com/logo.png",
                value: logoUrl(),
                onInput: (e: Event) =>
                  setLogoUrl((e.target as HTMLInputElement).value),
                onPaste: (e: ClipboardEvent) => {
                  const text = e.clipboardData?.getData("text") || "";
                  if (text) {
                    e.preventDefault();
                    setLogoUrl(text.trim());
                  }
                },
              }),
            ),
        ),
        createShow(
          () => logoUrl() !== "",
          () =>
            h(
              "div",
              { class: "logo-box logo-box-filled" },
              h(
                "div",
                { class: "logo-box-preview" },
                h("img", {
                  src: () => logoUrl(),
                  alt: "Logo preview",
                  class: "logo-box-img",
                  onError: (e: Event) =>
                    ((e.target as HTMLImageElement).style.display = "none"),
                  onLoad: (e: Event) =>
                    ((e.target as HTMLImageElement).style.display = ""),
                }),
              ),
              h(
                "div",
                { class: "logo-box-meta" },
                h("input", {
                  type: "url",
                  class: "input logo-box-input",
                  value: () => logoUrl(),
                  onInput: (e: Event) =>
                    setLogoUrl((e.target as HTMLInputElement).value),
                }),
                h(
                  "button",
                  {
                    type: "button",
                    class: "btn btn-ghost btn-sm logo-box-clear",
                    onClick: () => setLogoUrl(""),
                  },
                  "Remove",
                ),
              ),
            ),
        ),
      ),

      // ── Button Color (was "Primary color") ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Button color"),
        h(
          "div",
          { class: "wizard-color-row" },
          h("input", {
            type: "color",
            value: buttonColor(),
            onInput: (e: Event) =>
              setButtonColor((e.target as HTMLInputElement).value),
          }),
          h("input", {
            type: "text",
            class: "input input-sm",
            value: () => buttonColor(),
            onInput: (e: Event) =>
              setButtonColor((e.target as HTMLInputElement).value),
          }),
        ),
      ),

      // ── Button Text ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Button text"),
        h("input", {
          type: "text",
          class: "input",
          value: buttonText(),
          onInput: (e: Event) =>
            setButtonText((e.target as HTMLInputElement).value),
        }),
      ),

      // ── Card Background ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Card background"),
        h(
          "div",
          { class: "wizard-color-row" },
          h("input", {
            type: "color",
            value: () => cardColor() || defaultCard(),
            onInput: (e: Event) =>
              setCardColor((e.target as HTMLInputElement).value),
          }),
          h("input", {
            type: "text",
            class: "input input-sm",
            placeholder: () => defaultCard(),
            value: () => cardColor(),
            onInput: (e: Event) =>
              setCardColor((e.target as HTMLInputElement).value),
          }),
          createShow(
            () => cardColor() !== "",
            () =>
              h(
                "button",
                {
                  type: "button",
                  class: "btn btn-ghost btn-sm",
                  onClick: () => setCardColor(""),
                },
                "Reset",
              ),
          ),
        ),
      ),

      // ── Font Family ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Font"),
        h(
          "select",
          {
            class: "input",
            value: () => fontFamily(),
            onChange: (e: Event) =>
              setFontFamily((e.target as HTMLSelectElement).value),
          },
          ...FONT_OPTIONS.map((f) => h("option", { value: f }, f)),
        ),
      ),

      // ── Text Color ──
      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Text color"),
        h(
          "div",
          { class: "wizard-color-row" },
          h("input", {
            type: "color",
            value: () => textColor() || defaultText(),
            onInput: (e: Event) =>
              setTextColor((e.target as HTMLInputElement).value),
          }),
          h("input", {
            type: "text",
            class: "input input-sm",
            placeholder: () => defaultText(),
            value: () => textColor(),
            onInput: (e: Event) =>
              setTextColor((e.target as HTMLInputElement).value),
          }),
          createShow(
            () => textColor() !== "",
            () =>
              h(
                "button",
                {
                  type: "button",
                  class: "btn btn-ghost btn-sm",
                  onClick: () => setTextColor(""),
                },
                "Reset",
              ),
          ),
        ),
      ),

      // ── Pro Section (dark mode, custom CSS, page bg, remove branding) ──
      h(
        "div",
        { class: "wizard-pro-section" },
        h(
          "div",
          { class: "wizard-pro-header" },
          h("span", { class: "wizard-provider-badge" }, "Pro"),
          h("span", { class: "wizard-pro-label" }, "Upgrade to unlock"),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Dark / light mode"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Custom CSS"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Custom page background"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
        h(
          "div",
          { class: "wizard-pro-item" },
          h("span", null, "Remove fAuth branding"),
          h(
            "label",
            { class: "wizard-toggle" },
            h("input", { type: "checkbox", disabled: true }),
            h("span", { class: "wizard-toggle-track" }),
          ),
        ),
      ),

      h(
        "div",
        { class: "wizard-actions" },
        h(
          "button",
          {
            type: "button",
            class: "btn btn-accent",
            disabled: () => saving(),
            onClick: () => void handleContinue(),
          },
          () => (saving() ? "Saving…" : "Continue →"),
        ),
      ),
    ),

    h(
      "div",
      { class: "wizard-pane-right" },
      h("div", { class: "wizard-preview-label" }, "Live Preview"),
      LoginPreview({
        appName,
        logoUrl,
        primaryColor: buttonColor,
        darkMode: () => true, // hardcoded dark until Pro
        buttonText,
        providers: previewProviders,
        slug: () => tenant.slug,
        bgColor: () => "",
        cardColor,
        customCSS: () => "",
        fontFamily,
        textColor,
        signupMode: () => (tenant.signup_mode as string | undefined) || "open",
      }),
    ),
  );
}

// ── Step 4: Mode Selection ────────────────────────────────────────────

function StepModeSelection(): ReturnType<typeof h> {
  const _t = selectedTenant();
  if (!_t) return h("div", null, "No tenant selected");
  const tenant: Tenant = _t;

  const [modeChoice, setModeChoice] = createSignal<"hosted-page" | "integrate">(
    tenant.preferred_tab === "integrate" ? "integrate" : "hosted-page",
  );

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { preferred_tab: modeChoice() });
    setSaving(false);
    advanceStep();
  }

  return h(
    "div",
    { class: "wizard-step wizard-step-centered" },
    h(
      "h1",
      { class: "wizard-headline" },
      "How do you want to use authentication?",
    ),
    h(
      "p",
      { class: "wizard-subtitle" },
      "You can always switch later from the dashboard.",
    ),

    h(
      "div",
      { class: "mode-grid" },
      h(
        "button",
        {
          type: "button",
          class: () =>
            "mode-card" +
            (modeChoice() === "hosted-page" ? " mode-card-selected" : ""),
          onClick: () => setModeChoice("hosted-page"),
        },
        h("div", { class: "mode-card-badge" }, "Recommended"),
        h("div", { class: "mode-card-title" }, "Launch a hosted page"),
        h(
          "div",
          { class: "mode-card-subtitle" },
          "A working login page and protected site in 30 seconds. No code required.",
        ),
        h(
          "ul",
          { class: "mode-card-list" },
          h("li", null, "Hosted at /t/" + tenant.slug + "/site"),
          h("li", null, "End users sign in and see your branded page"),
          h("li", null, "Edit content anytime from the dashboard"),
        ),
      ),
      h(
        "button",
        {
          type: "button",
          class: () =>
            "mode-card" +
            (modeChoice() === "integrate" ? " mode-card-selected" : ""),
          onClick: () => setModeChoice("integrate"),
        },
        h("div", { class: "mode-card-title" }, "Integrate into your app"),
        h(
          "div",
          { class: "mode-card-subtitle" },
          "Drop our SDK into your codebase. Full control over the auth flow.",
        ),
        h(
          "ul",
          { class: "mode-card-list" },
          h("li", null, "API keys and code snippets"),
          h("li", null, "Framework-specific guides"),
          h("li", null, "Login page still works for testing"),
        ),
      ),
    ),

    h(
      "div",
      { class: "wizard-actions" },
      h(
        "button",
        {
          type: "button",
          class: "btn btn-accent",
          disabled: () => saving(),
          onClick: () => void handleContinue(),
        },
        () => (saving() ? "Saving\u2026" : "Continue \u2192"),
      ),
    ),
  );
}

// ── Step 5: Live Test ─────────────────────────────────────────────────

function StepLiveTest(): ReturnType<typeof h> {
  const _t = selectedTenant();
  if (!_t) return h("div", null, "No tenant selected");
  const tenant: Tenant = _t;

  type TestState = "prompt" | "waiting" | "success";
  const [testState, setTestState] = createSignal<TestState>("prompt");
  const [nudge, setNudge] = createSignal(false);
  const [sessionEmail, setSessionEmail] = createSignal("");
  let nudgeTimer: ReturnType<typeof setTimeout> | null = null;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  // Cleanup on unmount — prevent leaked intervals
  onCleanup(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (nudgeTimer) clearTimeout(nudgeTimer);
  });

  // Check for existing sessions on mount — skip to success if already tested
  (async () => {
    try {
      const data = await api.get<any>(
        "/platform/self-serve/tenants/" + encodeURIComponent(tenant.id),
      );
      if (data && data.active_sessions > 0) {
        setSessionEmail(data.owner_email || "End user");
        setTestState("success");
      }
    } catch {
      /* ignore */
    }
  })();

  const userEmail = currentUser()?.email || "";

  function openLoginPage(): void {
    window.open(
      serverUrl + "/t/" + tenant.slug + "/login",
      "fauth-test-login",
      "width=480,height=640,left=200,top=100",
    );
    setTestState("waiting");
    setNudge(false);
    nudgeTimer = setTimeout(() => setNudge(true), 60000);
    startPolling();
  }

  function startPolling(): void {
    if (pollInterval) clearInterval(pollInterval);
    pollInterval = setInterval(async () => {
      if (testState() !== "waiting") {
        if (pollInterval) clearInterval(pollInterval);
        return;
      }
      try {
        const data = await api.get<any>(
          "/platform/self-serve/tenants/" + encodeURIComponent(tenant.id),
        );
        if (data && data.active_sessions > 0) {
          if (pollInterval) clearInterval(pollInterval);
          if (nudgeTimer) clearTimeout(nudgeTimer);
          setSessionEmail(data.owner_email || "End user");
          setTestState("success");
        }
      } catch {
        /* ignore polling errors — auth token may be refreshing */
      }
    }, 3000);
  }

  return h(
    "div",
    { class: "wizard-step wizard-step-centered" },
    createShow(
      () => testState() === "prompt",
      () =>
        h(
          "div",
          { class: "wizard-test-prompt" },
          h(
            "h1",
            { class: "wizard-headline" },
            "Try signing in as an end user",
          ),
          h(
            "p",
            { class: "wizard-subtitle" },
            "You\u2019re your own first user \u2014 sign in with ",
            userEmail ? h("strong", null, userEmail) : "your fAuth credentials",
            ". We\u2019ll detect your session in real-time.",
          ),
          h(
            "button",
            {
              type: "button",
              class: "btn btn-accent btn-lg",
              onClick: openLoginPage,
            },
            "Open Login Page \u2192",
          ),
          h(
            "div",
            { style: { marginTop: "16px" } },
            h(
              "button",
              {
                type: "button",
                class: "btn btn-ghost btn-sm",
                onClick: advanceStep,
              },
              "Skip this step",
            ),
          ),
        ),
    ),

    createShow(
      () => testState() === "waiting",
      () =>
        h(
          "div",
          { class: "wizard-test-waiting" },
          h("div", { class: "wizard-pulse" }),
          h(
            "h1",
            { class: "wizard-headline" },
            "Waiting for your first sign-in\u2026",
          ),
          h("p", { class: "wizard-subtitle" }, () =>
            nudge()
              ? "Still waiting \u2014 make sure you\u2019re signing in at the page we opened."
              : "Sign in at the page we just opened. This will update the moment we detect your session.",
          ),
          h(
            "button",
            {
              type: "button",
              class: "btn btn-ghost btn-sm",
              onClick: () =>
                window.open(
                  serverUrl + "/t/" + tenant.slug + "/login",
                  "fauth-test-login",
                  "width=480,height=640,left=200,top=100",
                ),
            },
            "Reopen login page \u2192",
          ),
          h(
            "div",
            { style: { marginTop: "24px" } },
            h(
              "button",
              {
                type: "button",
                class: "btn btn-ghost btn-sm",
                onClick: () => {
                  if (pollInterval) clearInterval(pollInterval);
                  if (nudgeTimer) clearTimeout(nudgeTimer);
                  advanceStep();
                },
              },
              "Skip this step",
            ),
          ),
        ),
    ),

    createShow(
      () => testState() === "success",
      () =>
        h(
          "div",
          { class: "wizard-test-success" },
          h("div", { class: "wizard-checkmark" }, "\u2713"),
          h(
            "h1",
            { class: "wizard-headline" },
            "It works! Your first session is live.",
          ),
          h(
            "div",
            { class: "wizard-session-card" },
            h("div", null, () => sessionEmail()),
            h("div", { class: "wizard-session-meta" }, "Just now"),
          ),
          h(
            "div",
            { class: "wizard-actions" },
            h(
              "button",
              {
                type: "button",
                class: "btn btn-accent",
                onClick: advanceStep,
              },
              "Continue \u2192",
            ),
          ),
        ),
    ),
  );
}

// ── Step 6: Launch ────────────────────────────────────────────────────

function StepLaunch(): ReturnType<typeof h> {
  const _t = selectedTenant();
  if (!_t) return h("div", null, "No tenant selected");
  const tenant: Tenant = _t;

  const isHosted = (tenant.preferred_tab || "hosted-page") === "hosted-page";
  const siteUrl = window.location.origin + "/t/" + tenant.slug + "/site";
  const loginUrl = window.location.origin + "/t/" + tenant.slug + "/login";
  const [copied, setCopied] = createSignal(false);
  const [activeFramework, setActiveFramework] = createSignal("bridge");

  function copyText(text: string): void {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  async function handleLaunch(): Promise<void> {
    setSaving(true);
    // Defensive backfill: if the operator somehow reached Launch with empty
    // providers (e.g. legacy data, manual API patch), restore the intent
    // defaults so the launched tenant always has at least one usable sign-in
    // method. The wizard's normal flow can't reach this state, but Launch is
    // the last gate before a tenant goes live so we belt-and-suspender it.
    const currentProviders =
      (tenant.allowed_providers as string[] | undefined) || [];
    if (currentProviders.length === 0) {
      const intent = (tenant.tenant_intent as TenantIntent) || "saas";
      const intentDefaults = INTENT_DEFAULTS[intent] || INTENT_DEFAULTS.saas;
      await updateTenant(tenant.id, {
        allowed_providers: intentDefaults.allowed_providers,
      });
    }
    const updated = await updateTenant(tenant.id, {
      onboarding_completed: true,
      onboarding_step: 7,
    });
    if (!updated) {
      setSaving(false);
      return;
    }
    // Note: setSaving is intentionally NOT cleared on the happy path — the
    // navigateToWorkspace call below triggers a full page reload, so the
    // current saving signal will be discarded along with the rest of the
    // module state. Clearing it would just flicker the button label.
    navigateToWorkspace(updated);
  }

  const SNIPPETS: Record<string, string> = {
    bridge:
      '<script src="' +
      serverUrl +
      "/bridge.js\"><\/script>\n<script>\n  GateAuthBridge.init({\n    serverUrl: '" +
      serverUrl +
      "',\n    slug: '" +
      tenant.slug +
      "',\n    onStateChange: ({ state, user }) => {\n      if (state === 'authenticated') {\n        console.log('Hello,', user.name);\n      }\n    }\n  });\n<\/script>",
    rest:
      "# Login\ncurl -X POST " +
      serverUrl +
      "/t/" +
      tenant.slug +
      '/auth/login \\\n  -H "Content-Type: application/json" \\\n  -d \'{"email": "user@example.com", "password": "..."}\'\n\n# Check session\ncurl ' +
      serverUrl +
      "/t/" +
      tenant.slug +
      '/auth/me \\\n  -H "Cookie: refresh_token=..."',
  };

  if (isHosted) {
    return h(
      "div",
      { class: "wizard-step wizard-step-centered" },
      h("h1", { class: "wizard-headline" }, "Your hosted page is live"),
      h(
        "p",
        { class: "wizard-subtitle" },
        "End users can sign in and see your branded page right now.",
      ),

      h(
        "div",
        { class: "wizard-url-display" },
        h("code", null, siteUrl),
        h(
          "button",
          {
            type: "button",
            class: "btn btn-sm btn-ghost",
            onClick: () => copyText(siteUrl),
          },
          () => (copied() ? "Copied!" : "Copy"),
        ),
      ),

      h(
        "a",
        { href: siteUrl, target: "_blank", class: "wizard-link" },
        "Open in new tab \u2192",
      ),

      h(
        "div",
        { class: "wizard-summary-card" },
        h("div", { class: "wizard-summary-label" }, "Your configuration"),
        h(
          "div",
          { class: "wizard-summary-row" },
          h("span", null, "App name"),
          h("span", null, tenant.branding_app_name || tenant.name),
        ),
        h(
          "div",
          { class: "wizard-summary-row" },
          h("span", null, "Primary color"),
          h(
            "span",
            null,
            h("span", {
              class: "wizard-color-swatch",
              style: { background: tenant.branding_primary_color || "#6366f1" },
            }),
            " " + (tenant.branding_primary_color || "#6366f1"),
          ),
        ),
        h(
          "div",
          { class: "wizard-summary-row" },
          h("span", null, "Providers"),
          h("span", null, (tenant.allowed_providers || ["email"]).join(", ")),
        ),
        h(
          "div",
          { class: "wizard-summary-row" },
          h("span", null, "Mode"),
          h("span", null, "Hosted Page"),
        ),
      ),

      h(
        "div",
        { class: "wizard-actions" },
        h(
          "button",
          {
            type: "button",
            class: "btn btn-accent btn-lg",
            disabled: () => saving(),
            onClick: () => void handleLaunch(),
          },
          () => (saving() ? "Launching\u2026" : "Launch Dashboard \u2192"),
        ),
      ),
    );
  }

  return h(
    "div",
    { class: "wizard-step wizard-step-centered" },
    h("h1", { class: "wizard-headline" }, "Add authentication to your app"),
    h(
      "p",
      { class: "wizard-subtitle" },
      "Your login page is ready. Pick a framework to get started.",
    ),

    h(
      "div",
      { class: "wizard-client-id" },
      h("span", { class: "wizard-client-id-label" }, "Tenant slug"),
      h("code", null, tenant.slug),
      h(
        "button",
        {
          type: "button",
          class: "btn btn-sm btn-ghost",
          onClick: () => copyText(tenant.slug),
        },
        () => (copied() ? "Copied!" : "Copy"),
      ),
    ),

    h(
      "div",
      { class: "wizard-framework-tabs" },
      ...(
        [
          ["bridge", "Bridge (any framework)"],
          ["rest", "REST API"],
        ] as const
      ).map(([key, label]) =>
        h(
          "button",
          {
            type: "button",
            class: () =>
              "wizard-fw-tab" +
              (activeFramework() === key ? " wizard-fw-tab-active" : ""),
            onClick: () => setActiveFramework(key),
          },
          label,
        ),
      ),
    ),

    h(
      "pre",
      { class: "wizard-code-block" },
      h("code", null, () => SNIPPETS[activeFramework()] || ""),
    ),

    h(
      "div",
      { class: "wizard-url-display wizard-url-sm" },
      h("span", null, "Login page for testing: "),
      h("code", null, loginUrl),
      h(
        "button",
        {
          type: "button",
          class: "btn btn-sm btn-ghost",
          onClick: () => copyText(loginUrl),
        },
        "Copy",
      ),
    ),

    h(
      "div",
      { class: "wizard-actions" },
      h(
        "button",
        {
          type: "button",
          class: "btn btn-accent btn-lg",
          disabled: () => saving(),
          onClick: () => void handleLaunch(),
        },
        () => (saving() ? "Launching\u2026" : "Launch Dashboard \u2192"),
      ),
    ),
  );
}

// ── Main Component ────────────────────────────────────────────────────

export function OnboardingWizard(): ReturnType<typeof h> {
  // Always reset signals on mount — prevents stale state when switching tenants
  const tenant = selectedTenant();

  // Determine initial step. URL hash takes precedence (deep-linkable / refresh-
  // safe), but is clamped to the highest step the operator has actually reached
  // so #wizard-launch in the URL can't bypass earlier validation. Falls back to
  // the persisted onboarding_step. If no tenant exists yet (fresh create flow),
  // start at step 0.
  const minStep = tenant ? 1 : 0;
  const ceiling = tenant ? maxReachedStep() : 0;
  const hashStep = stepFromHash();
  const step =
    hashStep >= 0
      ? Math.min(Math.max(hashStep, minStep), ceiling)
      : tenant
        ? Math.min(Math.max(tenant.onboarding_step - 1, 1), 6)
        : 0;
  setWizardStep(step);
  setSaving(false);

  // Sync the URL hash to the resolved step on first mount, using replaceState
  // so we don't add a history entry. This makes the create step also have a
  // visible #wizard-create hash and keeps the address bar consistent across
  // the wizard's lifetime.
  setStepHash(step, { replace: true });

  // Skip-with-defaults confirmation modal state. Local to the wizard mount.
  const [skipModalOpen, setSkipModalOpen] = createSignal(false);

  async function handleAcceptDefaults(): Promise<void> {
    setSkipModalOpen(false);
    await skipWizard();
  }

  // Browser back/forward syncs the wizard step from the new hash. Clamped to
  // the persisted onboarding_step so forward jumps via the URL bar can't skip
  // validation. Programmatic hash writes from the wizard itself are ignored
  // via the wizardWroteHash sentinel — without that, this listener races with
  // the in-flight updateTenant call and reads a stale onboarding_step.
  const onHashChange = (): void => {
    if (wizardWroteHash) {
      wizardWroteHash = false;
      return;
    }
    const idx = stepFromHash();
    if (idx < 0) return;
    const m = selectedTenant() ? 1 : 0;
    const max = maxReachedStep();
    const clamped = Math.min(Math.max(idx, m), max);
    if (clamped !== wizardStep()) {
      setWizardStep(clamped);
    }
    // If the URL hash was higher than the user could actually reach, rewrite
    // the hash to match the clamped step so the address bar stays honest.
    if (idx !== clamped) {
      setStepHash(clamped);
    }
  };
  if (typeof window !== "undefined") {
    window.addEventListener("hashchange", onHashChange);
    onCleanup(() => {
      window.removeEventListener("hashchange", onHashChange);
    });
  }

  function handleTenantCreated(created: Tenant): void {
    // Set the tenant in the store so subsequent wizard steps can access it
    setSelectedTenant(created);
    setWizardStep(1);
    // Push the URL with the slug AND the wizard hash so it matches the
    // console's path convention (/platform/console/{slug}#...). Without the
    // slug in the path, refresh-resume bounces the operator to the account
    // view because init() can't match the tenant from the URL.
    if (typeof window !== "undefined") {
      history.pushState(
        null,
        "",
        "/platform/console/" + created.slug + "#" + WIZARD_HASH_PREFIX + "intent",
      );
    }
    void updateTenant(created.id, { onboarding_step: 2 });
  }

  return h(
    "div",
    { class: "wizard-root" },
    h(
      "div",
      { class: "wizard-header" },
      h(
        "div",
        { class: "wizard-header-left" },
        h("div", { class: "wizard-logo" }, "fAuth"),
        createShow(
          () => selectedTenant() !== null,
          () =>
            h(
              "span",
              { class: "wizard-tenant-name" },
              () => selectedTenant()!.name,
            ),
        ),
      ),
      // Skip is only meaningful for steps where defaults can do the work for
      // the operator: Intent (1), Auth (2), Branding (3). Create (0) needs a
      // name + slug, Mode (4) needs an explicit choice, Test (5) has its own
      // inline "Skip this step" UI, and Launch (6) IS the launch action.
      createShow(
        () => {
          const s = wizardStep();
          return selectedTenant() !== null && s >= 1 && s <= 3;
        },
        () =>
          h(
            "button",
            {
              type: "button",
              class: "wizard-skip",
              onClick: () => setSkipModalOpen(true),
            },
            "Skip setup \u2192",
          ),
      ),
    ),

    h(
      "div",
      { class: "wizard-step-indicator" },
      ...STEP_LABELS.map((label, i) =>
        h(
          "div",
          {
            class: () =>
              "wizard-step-dot" +
              (wizardStep() === i ? " wizard-step-dot-active" : "") +
              (wizardStep() > i ? " wizard-step-dot-done" : ""),
          },
          label,
        ),
      ),
    ),

    createShow(
      () => wizardStep() === 0,
      () => StepCreate(handleTenantCreated),
    ),
    createShow(() => wizardStep() === 1, StepIntent),
    createShow(() => wizardStep() === 2, StepAuthMethods),
    createShow(() => wizardStep() === 3, StepBranding),
    createShow(() => wizardStep() === 4, StepModeSelection),
    createShow(() => wizardStep() === 5, StepLiveTest),
    createShow(() => wizardStep() === 6, StepLaunch),

    createShow(
      () => wizardStep() > (selectedTenant() ? 1 : 0),
      () =>
        h(
          "div",
          { class: "wizard-back" },
          h(
            "button",
            {
              type: "button",
              class: "btn btn-ghost btn-sm",
              onClick: prevStep,
            },
            "\u2190 Back",
          ),
        ),
    ),

    // Skip-with-defaults confirmation modal. Renders the live preview of
    // the default sign-in page so the operator can see what they're getting.
    createShow(
      skipModalOpen,
      () => {
        const t = selectedTenant();
        const intent =
          ((t?.tenant_intent as TenantIntent | null) || "saas") as TenantIntent;
        const defaults = INTENT_DEFAULTS[intent];
        const previewName = () => t?.branding_app_name || t?.name || "Your App";
        const previewSlug = () => t?.slug || "your-app";
        const previewProviders = () =>
          ((t?.allowed_providers as string[] | undefined) ?? []).length > 0
            ? (t!.allowed_providers as string[])
            : defaults.allowed_providers;
        const previewSignupMode = () =>
          (t?.signup_mode as string | undefined) || defaults.signup_mode;
        const surfaceLabel =
          defaults.preferred_tab === "hosted-page"
            ? "Hosted login page"
            : "SDK / Bridge integration";

        return h(
          "div",
          {
            class: "wizard-modal-overlay",
            onClick: (e: Event) => {
              if (e.target === e.currentTarget) setSkipModalOpen(false);
            },
          },
          h(
            "div",
            { class: "wizard-modal wizard-skip-modal" },
            h(
              "h2",
              { class: "wizard-modal-title" },
              "Skip setup with defaults?",
            ),
            h(
              "p",
              { class: "wizard-modal-body" },
              "Here\u2019s how your sign-in page will look. We\u2019ll fill in sensible defaults for anything you haven\u2019t configured yet.",
            ),

            h(
              "div",
              { class: "wizard-skip-modal-preview" },
              LoginPreview({
                appName: previewName,
                logoUrl: () => t?.branding_logo_url || "",
                primaryColor: () =>
                  t?.branding_primary_color || "#6366f1",
                darkMode: () => t?.branding_dark_mode ?? true,
                buttonText: () => t?.branding_button_text || "Sign in",
                providers: previewProviders,
                slug: previewSlug,
                signupMode: previewSignupMode,
              }),
            ),

            h(
              "div",
              { class: "wizard-modal-list" },
              h(
                "div",
                { class: "wizard-modal-list-item" },
                h("strong", null, "Sign-in: "),
                () => previewProviders().join(", "),
              ),
              h(
                "div",
                { class: "wizard-modal-list-item" },
                h("strong", null, "Access: "),
                () =>
                  previewSignupMode() === "invite_only"
                    ? "Invite-only"
                    : "Open \u2014 anyone can sign up",
              ),
              h(
                "div",
                { class: "wizard-modal-list-item" },
                h("strong", null, "Surface: "),
                surfaceLabel,
              ),
            ),

            h(
              "p",
              { class: "wizard-modal-hint" },
              "You can change all of these later from Settings.",
            ),

            h(
              "div",
              { class: "wizard-modal-actions" },
              h(
                "button",
                {
                  type: "button",
                  class: "btn btn-ghost",
                  onClick: () => setSkipModalOpen(false),
                },
                "Customize instead",
              ),
              h(
                "button",
                {
                  type: "button",
                  class: "btn btn-accent",
                  disabled: () => saving(),
                  onClick: () => void handleAcceptDefaults(),
                },
                () => (saving() ? "Saving\u2026" : "Accept defaults \u2192"),
              ),
            ),
          ),
        );
      },
    ),
  );
}
