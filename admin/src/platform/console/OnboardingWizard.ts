import { createShow, createSignal, h, onCleanup } from "formajs";
import { normalizeTenantSlugInput } from "../tenantSlug";
import { LoginPreview } from "./LoginPreview";
import type { Tenant } from "./store";
import {
  api,
  checkSlugAvailability,
  completeOnboarding,
  createTenant,
  currentUser,
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

// ── Navigation ─────────────────────────────────────────────────────────

function advanceStep(): void {
  const next = Math.min(wizardStep() + 1, 6);
  setWizardStep(next);
  const tenant = selectedTenant();
  if (tenant) {
    void updateTenant(tenant.id, { onboarding_step: next + 1 });
  }
}

function prevStep(): void {
  // Don't allow going back to the Create step (0) once tenant exists
  const min = selectedTenant() ? 1 : 0;
  setWizardStep(Math.max(wizardStep() - 1, min));
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
  }
  const updated = await updateTenant(tenant.id, {
    onboarding_completed: true,
    onboarding_step: 7,
  });
  toast.show("Your workspace is ready", "success");
  if (updated) completeOnboarding(updated);
}

// ── Step 0: Create Workspace ──────────────────────────────────────────

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
    h("h1", { class: "wizard-headline" }, "Create your workspace"),
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
          placeholder: "Acme Workspace",
          value: name(),
          onInput: (e: Event) =>
            handleNameChange((e.target as HTMLInputElement).value),
        }),
        h(
          "span",
          { class: "wizard-field-hint" },
          "This appears in the console and the hosted tenant experience.",
        ),
      ),

      h(
        "label",
        { class: "wizard-field" },
        h("span", null, "Tenant Slug"),
        h("input", {
          type: "text",
          class: "input",
          placeholder: "acme-workspace",
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
      if (p === "email" && list.length === 1) return;
      setProviders(list.filter((x) => x !== p));
    } else {
      setProviders([...list, p]);
    }
  }

  async function handleContinue(): Promise<void> {
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
    return h(
      "label",
      {
        class: () =>
          "wizard-provider-toggle" +
          (disabled ? " wizard-provider-disabled" : ""),
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
        {
          class: () =>
            "wizard-invite-only-note" +
            (signupMode() === "invite_only" ? "" : " wizard-section-hidden"),
        },
        "Invited users sign in via the link in their invite email \u2014 no additional setup needed. Switch to Open to enable public sign-up methods.",
      ),

      h(
        "div",
        {
          class: () =>
            "wizard-provider-group" +
            (signupMode() === "open" ? "" : " wizard-section-hidden"),
        },
        h("div", { class: "wizard-provider-group-label" }, "Credentials"),
        ...CREDENTIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h(
        "div",
        {
          class: () =>
            "wizard-provider-group" +
            (signupMode() === "open" ? "" : " wizard-section-hidden"),
        },
        h(
          "div",
          { class: "wizard-provider-group-label" },
          "Social Providers",
        ),
        ...SOCIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h(
        "div",
        {
          class: () =>
            "wizard-pro-section" +
            (signupMode() === "open" ? "" : " wizard-section-hidden"),
        },
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
    const updated = await updateTenant(tenant.id, {
      onboarding_completed: true,
      onboarding_step: 7,
    });
    setSaving(false);
    toast.show("Your workspace is ready", "success");
    if (updated) completeOnboarding(updated);
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
  // If no tenant exists yet (fresh create flow), start at step 0
  // Otherwise resume from where they left off (step 1+)
  const step = tenant
    ? Math.min(Math.max(tenant.onboarding_step - 1, 1), 6)
    : 0;
  setWizardStep(step);
  setSaving(false);

  function handleTenantCreated(created: Tenant): void {
    // Set the tenant in the store so subsequent wizard steps can access it
    setSelectedTenant(created);
    setWizardStep(1);
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
      createShow(
        () => selectedTenant() !== null,
        () =>
          h(
            "button",
            {
              type: "button",
              class: "wizard-skip",
              onClick: () => void skipWizard(),
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
  );
}
