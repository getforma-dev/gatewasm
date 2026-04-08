import { h, createSignal, createShow } from 'formajs';
import {
  selectedTenant, updateTenant, toast, serverUrl, saveSecurityConfig, api, selectTenant,
} from './store';
import type { Tenant } from './store';

// ── Types ──────────────────────────────────────────────────────────────

type TenantIntent = 'saas' | 'internal' | 'community' | 'client-portal' | 'marketplace';

interface IntentOption {
  key: TenantIntent;
  title: string;
  description: string;
}

const INTENT_OPTIONS: IntentOption[] = [
  { key: 'saas', title: 'SaaS Application', description: 'Multi-tenant app with user accounts' },
  { key: 'internal', title: 'Internal Tool', description: 'Company-internal app, employees only' },
  { key: 'community', title: 'Community Site', description: 'Open registration, social features' },
  { key: 'client-portal', title: 'Client Portal', description: 'Restricted access for your clients' },
  { key: 'marketplace', title: 'Marketplace / API', description: 'Developer platform, API keys, OAuth clients' },
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
  saas: { signup_mode: 'open', idle_timeout_minutes: 30, max_sessions_per_user: 5, mfa_policy: 'recommended', allowed_providers: ['email', 'google'], preferred_tab: 'integrate' },
  internal: { signup_mode: 'invite_only', idle_timeout_minutes: 15, max_sessions_per_user: 2, mfa_policy: 'required', allowed_providers: ['email', 'google'], preferred_tab: 'integrate' },
  community: { signup_mode: 'open', idle_timeout_minutes: 60, max_sessions_per_user: 10, mfa_policy: 'off', allowed_providers: ['email', 'google', 'github'], preferred_tab: 'hosted-page' },
  'client-portal': { signup_mode: 'invite_only', idle_timeout_minutes: 30, max_sessions_per_user: 3, mfa_policy: 'recommended', allowed_providers: ['email'], preferred_tab: 'hosted-page' },
  marketplace: { signup_mode: 'open', idle_timeout_minutes: 60, max_sessions_per_user: 10, mfa_policy: 'off', allowed_providers: ['email', 'github'], preferred_tab: 'integrate' },
};

const STEP_LABELS = ['Intent', 'Branding', 'Auth', 'Mode', 'Test', 'Launch'];

// ── Shared Signals ─────────────────────────────────────────────────────

const [wizardStep, setWizardStep] = createSignal(0);
const [saving, setSaving] = createSignal(false);

// ── Navigation ─────────────────────────────────────────────────────────

function advanceStep(): void {
  const next = Math.min(wizardStep() + 1, 5);
  setWizardStep(next);
  const tenant = selectedTenant();
  if (tenant) {
    void updateTenant(tenant.id, { onboarding_step: (next + 1) as any });
  }
}

function prevStep(): void {
  setWizardStep(Math.max(wizardStep() - 1, 0));
}

async function skipWizard(): Promise<void> {
  const tenant = selectedTenant();
  if (!tenant) return;
  if (!tenant.tenant_intent) {
    const defaults = INTENT_DEFAULTS.saas;
    await updateTenant(tenant.id, {
      tenant_intent: 'saas',
      preferred_tab: defaults.preferred_tab,
      allowed_providers: defaults.allowed_providers,
    });
    await saveSecurityConfig(tenant.id, {
      idle_timeout_minutes: defaults.idle_timeout_minutes,
      max_sessions_per_user: defaults.max_sessions_per_user,
      mfa_policy: defaults.mfa_policy,
    } as any);
  }
  const updated = await updateTenant(tenant.id, { onboarding_completed: true, onboarding_step: 6 as any });
  toast.show('Your workspace is ready', 'success');
  if (updated) selectTenant(updated);
}

// ── Step 0: Intent Capture ─────────────────────────────────────────────

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
    });
    await saveSecurityConfig(tenant.id, {
      idle_timeout_minutes: defaults.idle_timeout_minutes,
      max_sessions_per_user: defaults.max_sessions_per_user,
      mfa_policy: defaults.mfa_policy,
    } as any);
    setSaving(false);
    advanceStep();
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    h('h1', { class: 'wizard-headline' }, 'What are you building?'),
    h('p', { class: 'wizard-subtitle' }, 'This helps us configure sensible defaults so you don\u2019t have to.'),
    h('div', { class: 'intent-grid' },
      ...INTENT_OPTIONS.map((opt) =>
        h('button', {
          type: 'button',
          class: () => 'intent-card' + (selectedIntent() === opt.key ? ' intent-card-selected' : ''),
          onClick: () => setSelectedIntent(opt.key),
        },
          h('div', { class: 'intent-card-title' }, opt.title),
          h('div', { class: 'intent-card-desc' }, opt.description),
        ),
      ),
    ),
    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent',
        disabled: () => selectedIntent() === null || saving(),
        onClick: () => void handleContinue(),
      }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
    ),
  );
}

// ── Step 1: Branding ───────────────────────────────────────────────────

function StepBranding(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const [appName, setAppName] = createSignal(tenant.branding_app_name || tenant.name);
  const [logoUrl, setLogoUrl] = createSignal(tenant.branding_logo_url || '');
  const [primaryColor, setPrimaryColor] = createSignal(tenant.branding_primary_color || '#6366f1');
  const [darkMode, setDarkMode] = createSignal(tenant.branding_dark_mode ?? true);
  const [buttonText, setButtonText] = createSignal(tenant.branding_button_text || 'Sign in');

  let iframeRef: HTMLIFrameElement | null = null;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function postBranding(): void {
    if (!iframeRef?.contentWindow) return;
    iframeRef.contentWindow.postMessage({
      type: 'forma:branding',
      primaryColor: primaryColor(),
      logoUrl: logoUrl(),
      appName: appName(),
      darkMode: darkMode(),
      buttonText: buttonText(),
    }, serverUrl);
  }

  function debouncedPost(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(postBranding, 100);
  }

  function immediatePost(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    postBranding();
  }

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      branding_app_name: appName(),
      branding_logo_url: logoUrl() || null,
      branding_primary_color: primaryColor(),
      branding_dark_mode: darkMode(),
      branding_button_text: buttonText(),
    });
    setSaving(false);
    advanceStep();
  }

  const iframeSrc = serverUrl + '/t/' + tenant.slug + '/login?preview=1';

  return h('div', { class: 'wizard-step wizard-split-pane' },
    h('div', { class: 'wizard-pane-left' },
      h('h1', { class: 'wizard-headline' }, 'Customize your login page'),
      h('p', { class: 'wizard-subtitle' }, 'Your users will see this when they sign in.'),

      h('label', { class: 'wizard-field' },
        h('span', null, 'App name'),
        h('input', {
          type: 'text',
          class: 'input',
          value: appName(),
          onInput: (e: Event) => { setAppName((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Logo URL'),
        h('input', {
          type: 'url',
          class: 'input',
          placeholder: 'https://example.com/logo.png',
          value: logoUrl(),
          onInput: (e: Event) => { setLogoUrl((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
        createShow(
          () => logoUrl() !== '',
          () => h('img', { src: () => logoUrl(), class: 'wizard-logo-preview', alt: 'Logo preview' }),
        ),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Primary color'),
        h('div', { class: 'wizard-color-row' },
          h('input', {
            type: 'color',
            value: primaryColor(),
            onInput: (e: Event) => { setPrimaryColor((e.target as HTMLInputElement).value); immediatePost(); },
          }),
          h('input', {
            type: 'text',
            class: 'input input-sm',
            value: () => primaryColor(),
            onInput: (e: Event) => { setPrimaryColor((e.target as HTMLInputElement).value); debouncedPost(); },
          }),
        ),
      ),

      h('div', { class: 'wizard-field wizard-field-row' },
        h('span', null, 'Dark mode'),
        h('label', { class: 'wizard-toggle' },
          h('input', {
            type: 'checkbox',
            checked: () => darkMode(),
            onChange: (e: Event) => { setDarkMode((e.target as HTMLInputElement).checked); immediatePost(); },
          }),
          h('span', { class: 'wizard-toggle-track' }),
        ),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Button text'),
        h('input', {
          type: 'text',
          class: 'input',
          value: buttonText(),
          onInput: (e: Event) => { setButtonText((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent',
          disabled: () => saving(),
          onClick: () => void handleContinue(),
        }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
      ),
    ),

    h('div', { class: 'wizard-pane-right' },
      h('div', { class: 'wizard-preview-label' }, 'Live Preview'),
      h('iframe', {
        src: iframeSrc,
        class: 'wizard-preview-iframe',
        ref: (el: HTMLIFrameElement) => { iframeRef = el; },
        onLoad: () => setTimeout(postBranding, 300),
      }),
    ),
  );
}

// ── Step 2: Auth Methods ───────────────────────────────────────────────

function StepAuthMethods(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const current = (tenant.allowed_providers as string[] | undefined) || ['email'];
  const [providers, setProviders] = createSignal<string[]>([...current]);

  let iframeRef: HTMLIFrameElement | null = null;

  function toggleProvider(p: string): void {
    const list = providers();
    if (list.includes(p)) {
      if (p === 'email' && list.length === 1) return;
      setProviders(list.filter((x) => x !== p));
    } else {
      setProviders([...list, p]);
    }
    postProviders();
  }

  function postProviders(): void {
    if (!iframeRef?.contentWindow) return;
    iframeRef.contentWindow.postMessage({
      type: 'forma:providers',
      providers: providers(),
      noCredentials: [],
    }, serverUrl);
  }

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { allowed_providers: providers() });
    setSaving(false);
    advanceStep();
  }

  const iframeSrc = serverUrl + '/t/' + tenant.slug + '/login?preview=1';

  const CREDENTIAL_PROVIDERS = [
    { key: 'email', label: 'Email / Password' },
    { key: 'magic_link', label: 'Magic Link', subtitle: 'Passwordless sign-in via email' },
  ];

  const SOCIAL_PROVIDERS = [
    { key: 'google', label: 'Google', available: true },
    { key: 'github', label: 'GitHub', available: true },
    { key: 'microsoft', label: 'Microsoft', available: false },
    { key: 'apple', label: 'Apple', available: false },
  ];

  function ProviderToggle(opt: { key: string; label: string; subtitle?: string; available?: boolean }): ReturnType<typeof h> {
    const disabled = opt.available === false;
    return h('label', {
      class: () => 'wizard-provider-toggle' + (disabled ? ' wizard-provider-disabled' : ''),
    },
      h('div', { class: 'wizard-provider-info' },
        h('span', { class: 'wizard-provider-label' }, opt.label),
        opt.subtitle ? h('span', { class: 'wizard-provider-subtitle' }, opt.subtitle) : null,
        disabled ? h('span', { class: 'wizard-provider-badge' }, 'Coming soon') : null,
      ),
      h('label', { class: 'wizard-toggle' },
        h('input', {
          type: 'checkbox',
          checked: () => providers().includes(opt.key),
          disabled: disabled,
          onChange: () => toggleProvider(opt.key),
        }),
        h('span', { class: 'wizard-toggle-track' }),
      ),
    );
  }

  return h('div', { class: 'wizard-step wizard-split-pane' },
    h('div', { class: 'wizard-pane-left' },
      h('h1', { class: 'wizard-headline' }, 'How will users sign in?'),
      h('p', { class: 'wizard-subtitle' }, 'Choose authentication methods for your app.'),

      h('div', { class: 'wizard-provider-group' },
        h('div', { class: 'wizard-provider-group-label' }, 'Credentials'),
        ...CREDENTIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h('div', { class: 'wizard-provider-group' },
        h('div', { class: 'wizard-provider-group-label' }, 'Social Providers'),
        ...SOCIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent',
          disabled: () => saving(),
          onClick: () => void handleContinue(),
        }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
      ),
    ),

    h('div', { class: 'wizard-pane-right' },
      h('div', { class: 'wizard-preview-label' }, 'Live Preview'),
      h('iframe', {
        src: iframeSrc,
        class: 'wizard-preview-iframe',
        ref: (el: HTMLIFrameElement) => { iframeRef = el; },
        onLoad: () => setTimeout(postProviders, 300),
      }),
    ),
  );
}

// ── Step 3: Mode Selection ─────────────────────────────────────────────

function StepModeSelection(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const [modeChoice, setModeChoice] = createSignal<'hosted-page' | 'integrate'>(
    tenant.preferred_tab === 'integrate' ? 'integrate' : 'hosted-page',
  );

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { preferred_tab: modeChoice() });
    setSaving(false);
    advanceStep();
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    h('h1', { class: 'wizard-headline' }, 'How do you want to use authentication?'),
    h('p', { class: 'wizard-subtitle' }, 'You can always switch later from the dashboard.'),

    h('div', { class: 'mode-grid' },
      h('button', {
        type: 'button',
        class: () => 'mode-card' + (modeChoice() === 'hosted-page' ? ' mode-card-selected' : ''),
        onClick: () => setModeChoice('hosted-page'),
      },
        h('div', { class: 'mode-card-badge' }, 'Recommended'),
        h('div', { class: 'mode-card-title' }, 'Launch a hosted page'),
        h('div', { class: 'mode-card-subtitle' }, 'A working login page and protected site in 30 seconds. No code required.'),
        h('ul', { class: 'mode-card-list' },
          h('li', null, 'Hosted at /t/' + tenant.slug + '/site'),
          h('li', null, 'End users sign in and see your branded page'),
          h('li', null, 'Edit content anytime from the dashboard'),
        ),
      ),
      h('button', {
        type: 'button',
        class: () => 'mode-card' + (modeChoice() === 'integrate' ? ' mode-card-selected' : ''),
        onClick: () => setModeChoice('integrate'),
      },
        h('div', { class: 'mode-card-title' }, 'Integrate into your app'),
        h('div', { class: 'mode-card-subtitle' }, 'Drop our SDK into your codebase. Full control over the auth flow.'),
        h('ul', { class: 'mode-card-list' },
          h('li', null, 'API keys and code snippets'),
          h('li', null, 'Framework-specific guides'),
          h('li', null, 'Login page still works for testing'),
        ),
      ),
    ),

    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent',
        disabled: () => saving(),
        onClick: () => void handleContinue(),
      }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
    ),
  );
}

// ── Step 4: Live Test ──────────────────────────────────────────────────

function StepLiveTest(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  type TestState = 'prompt' | 'waiting' | 'success';
  const [testState, setTestState] = createSignal<TestState>('prompt');
  const [nudge, setNudge] = createSignal(false);
  const [sessionEmail, setSessionEmail] = createSignal('');
  let nudgeTimer: ReturnType<typeof setTimeout> | null = null;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  function openLoginPage(): void {
    window.open(serverUrl + '/t/' + tenant.slug + '/login', '_blank');
    setTestState('waiting');
    setNudge(false);
    nudgeTimer = setTimeout(() => setNudge(true), 60000);
    startPolling();
  }

  function startPolling(): void {
    if (pollInterval) clearInterval(pollInterval);
    pollInterval = setInterval(async () => {
      if (testState() !== 'waiting') {
        if (pollInterval) clearInterval(pollInterval);
        return;
      }
      try {
        const data = await api.get<any>('/platform/self-serve/tenants/' + encodeURIComponent(tenant.id));
        if (data && data.active_sessions > 0) {
          if (pollInterval) clearInterval(pollInterval);
          if (nudgeTimer) clearTimeout(nudgeTimer);
          setSessionEmail(data.owner_email || 'End user');
          setTestState('success');
        }
      } catch { /* ignore polling errors — auth token may be refreshing */ }
    }, 3000);
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    createShow(() => testState() === 'prompt', () =>
      h('div', { class: 'wizard-test-prompt' },
        h('h1', { class: 'wizard-headline' }, 'Try signing in as an end user'),
        h('p', { class: 'wizard-subtitle' }, 'You\u2019re your own first user \u2014 the same credentials you used to sign into fAuth will work here. We\u2019ll open the login page and detect your session in real-time.'),
        h('button', {
          type: 'button',
          class: 'btn btn-accent btn-lg',
          onClick: openLoginPage,
        }, 'Open Login Page \u2192'),
        h('div', { style: 'margin-top:16px' },
          h('button', {
            type: 'button',
            class: 'btn btn-ghost btn-sm',
            onClick: advanceStep,
          }, 'Skip this step'),
        ),
      ),
    ),

    createShow(() => testState() === 'waiting', () =>
      h('div', { class: 'wizard-test-waiting' },
        h('div', { class: 'wizard-pulse' }),
        h('h1', { class: 'wizard-headline' }, 'Waiting for your first sign-in\u2026'),
        h('p', { class: 'wizard-subtitle' }, () =>
          nudge()
            ? 'Still waiting \u2014 make sure you\u2019re signing in at the page we opened.'
            : 'Sign in at the page we just opened. This will update the moment we detect your session.',
        ),
        h('button', {
          type: 'button',
          class: 'btn btn-ghost btn-sm',
          onClick: () => window.open(serverUrl + '/t/' + tenant.slug + '/login', '_blank'),
        }, 'Reopen login page \u2192'),
        h('div', { style: 'margin-top:24px' },
          h('button', {
            type: 'button',
            class: 'btn btn-ghost btn-sm',
            onClick: () => { if (pollInterval) clearInterval(pollInterval); if (nudgeTimer) clearTimeout(nudgeTimer); advanceStep(); },
          }, 'Skip this step'),
        ),
      ),
    ),

    createShow(() => testState() === 'success', () =>
      h('div', { class: 'wizard-test-success' },
        h('div', { class: 'wizard-checkmark' }, '\u2713'),
        h('h1', { class: 'wizard-headline' }, 'It works! Your first session is live.'),
        h('div', { class: 'wizard-session-card' },
          h('div', null, () => sessionEmail()),
          h('div', { class: 'wizard-session-meta' }, 'Just now'),
        ),
        h('div', { class: 'wizard-actions' },
          h('button', {
            type: 'button',
            class: 'btn btn-accent',
            onClick: advanceStep,
          }, 'Continue \u2192'),
        ),
      ),
    ),
  );
}

// ── Step 5: Launch ─────────────────────────────────────────────────────

function StepLaunch(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const isHosted = (tenant.preferred_tab || 'hosted-page') === 'hosted-page';
  const siteUrl = window.location.origin + '/t/' + tenant.slug + '/site';
  const loginUrl = window.location.origin + '/t/' + tenant.slug + '/login';
  const [copied, setCopied] = createSignal(false);
  const [activeFramework, setActiveFramework] = createSignal('bridge');

  function copyText(text: string): void {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  async function handleLaunch(): Promise<void> {
    setSaving(true);
    const updated = await updateTenant(tenant.id, { onboarding_completed: true, onboarding_step: 6 as any });
    setSaving(false);
    toast.show('Your workspace is ready', 'success');
    // Re-select the tenant to trigger full workspace data loading
    // The createShow in ConsoleApp will now render the dashboard since onboarding_completed is true
    if (updated) {
      selectTenant(updated);
    }
  }

  const SNIPPETS: Record<string, string> = {
    bridge: '<script src="' + serverUrl + '/bridge.js"><\/script>\n<script>\n  GateAuthBridge.init({\n    serverUrl: \'' + serverUrl + '\',\n    slug: \'' + tenant.slug + '\',\n    onStateChange: ({ state, user }) => {\n      if (state === \'authenticated\') {\n        console.log(\'Hello,\', user.name);\n      }\n    }\n  });\n<\/script>',
    rest: '# Login\ncurl -X POST ' + serverUrl + '/t/' + tenant.slug + '/auth/login \\\n  -H "Content-Type: application/json" \\\n  -d \'{"email": "user@example.com", "password": "..."}\'\n\n# Check session\ncurl ' + serverUrl + '/t/' + tenant.slug + '/auth/me \\\n  -H "Cookie: refresh_token=..."',
  };

  if (isHosted) {
    return h('div', { class: 'wizard-step wizard-step-centered' },
      h('h1', { class: 'wizard-headline' }, 'Your hosted page is live'),
      h('p', { class: 'wizard-subtitle' }, 'End users can sign in and see your branded page right now.'),

      h('div', { class: 'wizard-url-display' },
        h('code', null, siteUrl),
        h('button', {
          type: 'button',
          class: 'btn btn-sm btn-ghost',
          onClick: () => copyText(siteUrl),
        }, () => copied() ? 'Copied!' : 'Copy'),
      ),

      h('a', { href: siteUrl, target: '_blank', class: 'wizard-link' }, 'Open in new tab \u2192'),

      h('div', { class: 'wizard-summary-card' },
        h('div', { class: 'wizard-summary-label' }, 'Your configuration'),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'App name'), h('span', null, tenant.branding_app_name || tenant.name),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Primary color'),
          h('span', null,
            h('span', { class: 'wizard-color-swatch', style: 'background:' + (tenant.branding_primary_color || '#6366f1') }),
            ' ' + (tenant.branding_primary_color || '#6366f1'),
          ),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Providers'), h('span', null, (tenant.allowed_providers || ['email']).join(', ')),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Mode'), h('span', null, 'Hosted Page'),
        ),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent btn-lg',
          disabled: () => saving(),
          onClick: () => void handleLaunch(),
        }, () => saving() ? 'Launching\u2026' : 'Launch Dashboard \u2192'),
      ),
    );
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    h('h1', { class: 'wizard-headline' }, 'Add authentication to your app'),
    h('p', { class: 'wizard-subtitle' }, 'Your login page is ready. Pick a framework to get started.'),

    h('div', { class: 'wizard-client-id' },
      h('span', { class: 'wizard-client-id-label' }, 'Tenant slug'),
      h('code', null, tenant.slug),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        onClick: () => copyText(tenant.slug),
      }, () => copied() ? 'Copied!' : 'Copy'),
    ),

    h('div', { class: 'wizard-framework-tabs' },
      ...([['bridge', 'Bridge (any framework)'], ['rest', 'REST API']] as const).map(([key, label]) =>
        h('button', {
          type: 'button',
          class: () => 'wizard-fw-tab' + (activeFramework() === key ? ' wizard-fw-tab-active' : ''),
          onClick: () => setActiveFramework(key),
        }, label),
      ),
    ),

    h('pre', { class: 'wizard-code-block' },
      h('code', null, () => SNIPPETS[activeFramework()] || ''),
    ),

    h('div', { class: 'wizard-url-display wizard-url-sm' },
      h('span', null, 'Login page for testing: '),
      h('code', null, loginUrl),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        onClick: () => copyText(loginUrl),
      }, 'Copy'),
    ),

    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent btn-lg',
        disabled: () => saving(),
        onClick: () => void handleLaunch(),
      }, () => saving() ? 'Launching\u2026' : 'Launch Dashboard \u2192'),
    ),
  );
}

// ── Main Component ─────────────────────────────────────────────────────

export function OnboardingWizard(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (tenant && tenant.onboarding_step > 1) {
    setWizardStep(Math.min(tenant.onboarding_step - 1, 5));
  }

  return h('div', { class: 'wizard-root' },
    h('div', { class: 'wizard-header' },
      h('div', { class: 'wizard-logo' }, 'fAuth'),
      h('button', {
        type: 'button',
        class: 'wizard-skip',
        onClick: () => void skipWizard(),
      }, 'Skip setup \u2192'),
    ),

    h('div', { class: 'wizard-step-indicator' },
      ...STEP_LABELS.map((label, i) =>
        h('div', {
          class: () => 'wizard-step-dot'
            + (wizardStep() === i ? ' wizard-step-dot-active' : '')
            + (wizardStep() > i ? ' wizard-step-dot-done' : ''),
        }, label),
      ),
    ),

    createShow(() => wizardStep() === 0, StepIntent),
    createShow(() => wizardStep() === 1, StepBranding),
    createShow(() => wizardStep() === 2, StepAuthMethods),
    createShow(() => wizardStep() === 3, StepModeSelection),
    createShow(() => wizardStep() === 4, StepLiveTest),
    createShow(() => wizardStep() === 5, StepLaunch),

    createShow(
      () => wizardStep() > 0,
      () => h('div', { class: 'wizard-back' },
        h('button', {
          type: 'button',
          class: 'btn btn-ghost btn-sm',
          onClick: prevStep,
        }, '\u2190 Back'),
      ),
    ),
  );
}
