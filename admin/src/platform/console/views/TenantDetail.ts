// ---------------------------------------------------------------------------
// TenantDetail.ts — Full tenant configuration & actions view
// ---------------------------------------------------------------------------

import { StatCard } from '../../../core/components/StatCard';
import { Badge } from '../../../core/components/Badge';
import { Pill } from '../../../core/components/Pill';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { Field } from '../../../core/components/Field';
import { fmtDate, copyText } from '../../../core/utils';
import type { ConsoleStore } from '../store';

import { h, createSignal, createShow } from 'formajs';

const serverUrl = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

// -- Helpers ----------------------------------------------------------------

function planBadgeVariant(plan: string): 'warning' | 'info' | undefined {
  if (plan === 'trial') return 'warning';
  if (plan === 'paid' || plan === 'pro' || plan === 'enterprise') return 'info';
  return undefined;
}

function maskedKey(key: string | undefined): string {
  if (!key) return 'mgmt_****\u2026****';
  if (key.length <= 12) return key;
  return key.substring(0, 5) + '****\u2026****' + key.substring(key.length - 4);
}

// -- Component --------------------------------------------------------------

export function TenantDetail(cs: ConsoleStore) {
  const { store, navigate, configureTenant, rotateTenantKey, deleteTenant } = cs;

  // Local form signals
  const [name, setName] = createSignal('');
  const [origins, setOrigins] = createSignal('');
  const [redirectUrl, setRedirectUrl] = createSignal('');
  const [brandAppName, setBrandAppName] = createSignal('');
  const [brandColor, setBrandColor] = createSignal('#6366f1');
  const [brandLogo, setBrandLogo] = createSignal('');
  const [provEmail, setProvEmail] = createSignal(false);
  const [provGoogle, setProvGoogle] = createSignal(false);
  const [provGithub, setProvGithub] = createSignal(false);
  const [keyRevealed, setKeyRevealed] = createSignal(false);

  // Sync local signals from store whenever view mounts
  function syncForm() {
    const t = store.selectedTenant;
    if (!t) return;
    setName(t.name);
    const originsArr = Array.isArray(t.allowed_origins) ? t.allowed_origins.join(', ') : '';
    setOrigins(originsArr);
    setRedirectUrl((t.branding && t.branding.login_redirect_url) || '');
    setBrandAppName((t.branding && t.branding.app_name) || '');
    setBrandColor((t.branding && t.branding.primary_color) || '#6366f1');
    setBrandLogo((t.branding && t.branding.logo_url) || '');
    const providers = t.allowed_providers || [];
    setProvEmail(providers.indexOf('email') >= 0);
    setProvGoogle(providers.indexOf('google') >= 0);
    setProvGithub(providers.indexOf('github') >= 0);
    setKeyRevealed(false);
  }

  // Call sync immediately and re-sync when tenant changes
  syncForm();

  // -- Save handler ---------------------------------------------------------
  function handleSave() {
    const t = store.selectedTenant;
    if (!t) return;

    const providers: string[] = [];
    if (provEmail()) providers.push('email');
    if (provGoogle()) providers.push('google');
    if (provGithub()) providers.push('github');

    const allowedOrigins = origins()
      .split(',')
      .map((s: string) => s.trim())
      .filter(Boolean);

    configureTenant(t.id, {
      name: name(),
      allowed_providers: providers,
      allowed_origins: allowedOrigins,
      branding: {
        app_name: brandAppName(),
        primary_color: brandColor(),
        logo_url: brandLogo(),
        login_redirect_url: redirectUrl(),
      },
    });
  }

  // -- Delete handler -------------------------------------------------------
  function handleDelete() {
    const t = store.selectedTenant;
    if (!t) return;
    const answer = prompt('Type "' + t.slug + '" to confirm deletion:');
    if (answer === t.slug) {
      deleteTenant(t.id);
    }
  }

  // -- Header ---------------------------------------------------------------
  const header = () => {
    const t = store.selectedTenant;
    if (!t) return h('div');
    const loginUrl = serverUrl + '/t/' + encodeURIComponent(t.slug) + '/login';

    return h('div', { class: 'detail-header' },
      h('button', { class: 'btn small', onClick: () => navigate('tenants') }, '\u2190 Back to Tenants'),
      h('div', { class: 'detail-title flex items-center gap-sm mt-sm' },
        h('h2', null, () => store.selectedTenant?.name || ''),
        () => {
          const tenant = store.selectedTenant;
          return tenant ? Badge(tenant.plan, planBadgeVariant(tenant.plan)) : h('span');
        },
        () => {
          const tenant = store.selectedTenant;
          return tenant ? Pill(tenant.status || 'active') : h('span');
        },
        h('a', {
          href: loginUrl,
          target: '_blank',
          rel: 'noopener',
          class: 'btn small',
        }, 'Open login page'),
      ),
    );
  };

  // -- Stats row ------------------------------------------------------------
  const statsRow = h('div', { class: 'stat-grid stat-grid--compact' },
    StatCard({
      label: 'Total Users',
      value: () => store.selectedTenantStats?.total_users ?? '-',
    }),
    StatCard({
      label: 'Active Sessions',
      value: () => store.selectedTenantStats?.active_sessions ?? '-',
    }),
    StatCard({
      label: 'Logins (24h)',
      value: () => store.selectedTenantStats?.logins_24h ?? '-',
    }),
  );

  // -- Config form panel (left) ---------------------------------------------
  const configPanel = Panel({},
    PanelTitle(h('h3', { class: 'card-title' }, 'Configuration')),

    Field({
      label: 'Tenant Name',
      name: 'name',
      value: name,
      onChange: setName,
    }),

    h('div', { class: 'field' },
      h('label', null, 'URL Slug'),
      h('code', null, () => store.selectedTenant?.slug || ''),
    ),

    h('div', { class: 'field' },
      h('label', null, 'Login Page URL'),
      h('a', {
        href: () => {
          const t = store.selectedTenant;
          return t ? serverUrl + '/t/' + encodeURIComponent(t.slug) + '/login' : '#';
        },
        target: '_blank',
        rel: 'noopener',
      }, () => {
        const t = store.selectedTenant;
        return t ? serverUrl + '/t/' + encodeURIComponent(t.slug) + '/login' : '';
      }),
    ),

    h('div', { class: 'field field-row' },
      h('div', { class: 'field-half' },
        h('label', null, 'Plan'),
        h('div', null, () => {
          const t = store.selectedTenant;
          return t ? Badge(t.plan, planBadgeVariant(t.plan)) : h('span');
        }),
      ),
      h('div', { class: 'field-half' },
        h('label', null, 'Created'),
        h('div', null, () => fmtDate(store.selectedTenant?.created_at)),
      ),
    ),

    // Branding section
    h('h4', { class: 'section-heading' }, 'Branding'),

    Field({
      label: 'App Name',
      name: 'branding_app_name',
      value: brandAppName,
      onChange: setBrandAppName,
      placeholder: 'My App',
    }),

    Field({
      label: 'Primary Color',
      name: 'branding_primary_color',
      type: 'color',
      value: brandColor,
      onChange: setBrandColor,
    }),

    Field({
      label: 'Logo URL',
      name: 'branding_logo_url',
      value: brandLogo,
      onChange: setBrandLogo,
      placeholder: 'https://example.com/logo.svg',
    }),

    // Auth Config section
    h('h4', { class: 'section-heading' }, 'Auth Config'),

    Field({
      label: 'Allowed Origins',
      name: 'allowed_origins',
      type: 'textarea',
      value: origins,
      onChange: setOrigins,
      placeholder: 'https://example.com, https://app.example.com',
    }),

    Field({
      label: 'Login Redirect URL',
      name: 'login_redirect_url',
      value: redirectUrl,
      onChange: setRedirectUrl,
      placeholder: 'https://app.example.com/callback',
    }),

    h('div', { class: 'field' },
      h('label', null, 'Providers'),
      h('div', { class: 'checkbox-group flex gap-md' },
        h('label', null,
          h('input', {
            type: 'checkbox',
            checked: provEmail,
            onChange: (e: Event) => setProvEmail((e.target as HTMLInputElement).checked),
          }),
          ' Email',
        ),
        h('label', null,
          h('input', {
            type: 'checkbox',
            checked: provGoogle,
            onChange: (e: Event) => setProvGoogle((e.target as HTMLInputElement).checked),
          }),
          ' Google',
        ),
        h('label', null,
          h('input', {
            type: 'checkbox',
            checked: provGithub,
            onChange: (e: Event) => setProvGithub((e.target as HTMLInputElement).checked),
          }),
          ' GitHub',
        ),
      ),
    ),

    h('button', { class: 'btn primary', onClick: handleSave }, 'Save Changes'),
  );

  // -- Actions panel (right) ------------------------------------------------
  const actionsPanel = Panel({},
    PanelTitle(h('h3', { class: 'card-title' }, 'API Key')),

    h('div', { class: 'key-display flex items-center gap-sm flex-wrap' },
      h('code', { class: 'key-value' }, () => {
        if (keyRevealed()) {
          return store.selectedTenant?.management_api_key || 'mgmt_****\u2026****';
        }
        return maskedKey(store.selectedTenant?.management_api_key);
      }),
      h('div', { class: 'key-actions flex gap-xs' },
        h('button', {
          class: 'btn small',
          onClick: () => setKeyRevealed(!keyRevealed()),
        }, () => keyRevealed() ? 'Hide' : 'Reveal'),
        h('button', {
          class: 'btn small',
          onClick: () => {
            const key = store.selectedTenant?.management_api_key;
            if (key) copyText(key);
          },
        }, 'Copy'),
        h('button', {
          class: 'btn small',
          onClick: () => {
            const t = store.selectedTenant;
            if (t) rotateTenantKey(t.id);
          },
        }, 'Rotate'),
      ),
    ),

    // Tenant stats
    createShow(
      () => store.selectedTenantStats != null,
      () => h('div', null,
        h('h4', { class: 'section-heading' }, 'Tenant Stats'),
        statsRow,
      ),
    ),

    // Quick links
    h('h4', { class: 'section-heading' }, 'Quick Links'),
    h('div', { class: 'quick-links flex gap-sm flex-wrap' },
      h('a', {
        href: () => {
          const t = store.selectedTenant;
          return t ? serverUrl + '/t/' + encodeURIComponent(t.slug) + '/login' : '#';
        },
        target: '_blank',
        rel: 'noopener',
        class: 'btn small',
      }, 'Login page'),
      h('a', {
        href: () => {
          const t = store.selectedTenant;
          return t ? serverUrl + '/t/' + encodeURIComponent(t.slug) + '/admin' : '#';
        },
        target: '_blank',
        rel: 'noopener',
        class: 'btn small',
      }, 'Admin console'),
      h('a', { href: '/docs', class: 'btn small' }, 'API docs'),
    ),

    // Danger zone
    h('div', { class: 'danger-zone mt-lg divider-top' },
      h('button', {
        class: 'btn danger',
        onClick: handleDelete,
      }, 'Delete Tenant'),
    ),
  );

  // -- No-tenant fallback ---------------------------------------------------
  const noTenant = h('div', { class: 'content' },
    h('div', { class: 'card p-lg' },
      h('p', null, 'No tenant selected.'),
      h('button', { class: 'btn small', onClick: () => navigate('tenants') }, '\u2190 Back to Tenants'),
    ),
  );

  return createShow(
    () => store.selectedTenant != null,
    () => h('div', { class: 'content' },
      header(),
      h('div', { class: 'split split-2col' },
        configPanel,
        actionsPanel,
      ),
    ),
    () => noTenant,
  );
}
