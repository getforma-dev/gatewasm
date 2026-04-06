import { h, createSignal, createSwitch } from 'formajs';
import { Field } from '../../core/components/ui/Field';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { createKeyboard } from '../../core/hooks/createKeyboard';
import { getThemeMode, toggleThemeMode, type ThemeMode } from '../../core/utils';
import { normalizeTenantSlugInput } from '../tenantSlug';
import { CommandPalette, type CommandPaletteSection } from './CommandPalette';
import { PlaneNav } from './views/PlaneNav';
import { userDisplayName, userInitials } from './helpers';
import { OverviewView } from './views/OverviewView';
import { UsersView } from './views/UsersView';
import { IntegrateView } from './views/IntegrateView';
import { HostedPageView } from './views/HostedPageView';
import { AuditLogView } from './views/AuditLogView';
import { ActivityView } from './views/ActivityView';
import { SessionsView } from './views/SessionsView';
import { PasskeysView } from './views/PasskeysView';
import { RolesView } from './views/RolesView';
import { SecurityConfigView } from './views/SecurityConfigView';
import { TokensView } from './views/TokensView';
import { ApiKeysView } from './views/ApiKeysView';
import { AccountSettingsView } from './views/AccountSettingsView';
import {
  view, loading, toast, backToTenants, signout,
  selectedTenant, tenants, selectTenant, createTenant, currentUser,
  navigateTenantView, refreshCurrentView, openIntegrateGuide, openSessionInspector,
  plane, setPlane, type Plane, type ConsoleView,
  type TenantWorkspaceView,
  mode, accountTab, setAccountTab, type AccountTab,
} from './store';

function TenantsList() {
  const tenantCountLabel = () => {
    const count = tenants().length;
    if (count === 0) {
      return 'Create your first tenant to open the workspace';
    }
    return count === 1
      ? '1 tenant ready to manage'
      : count + ' tenants ready to manage';
  };

  return h('div', { class: 'console-view console-view-tenants' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Platform Console'),
        h('h2', { class: 'view-title' }, 'Your Tenants'),
        h('p', { class: 'view-subtitle text-muted' }, tenantCountLabel),
      ),
      h('p', { class: 'view-header-note' },
        'Create a tenant with the URL you actually want, then jump straight into overview, users, integrations, hosted page controls, and the auth reference.',
      ),
    ),
    h('div', { class: 'tenant-hub-grid' },
      TenantCreatePanel(),
      h('section', { class: 'tenant-list-stack' },
        h('div', { class: 'tenant-list-head' },
          h('div', { class: 'card-label' }, 'Existing Workspaces'),
          h('p', { class: 'card-copy' },
            () => tenants().length === 0
              ? 'Your first tenant will appear here as soon as you create it.'
              : 'Select a tenant to jump directly into its workspace.',
          ),
        ),
        () => tenants().length === 0
          ? h('div', { class: 'console-card tenant-empty-state' },
              h('div', { class: 'card-label' }, 'No tenants yet'),
              h('h3', { class: 'section-title' }, 'Start by naming the workspace you want people to visit'),
              h('p', { class: 'card-copy' },
                'Your tenant slug becomes part of every hosted URL, so choosing it explicitly here is clearer than inheriting a generated suffix.',
              ),
            )
          : null,
        h('div', { class: 'tenant-card-grid' },
          () => tenants().map((tenant) =>
            h('button', {
              type: 'button',
              class: 'tenant-card',
              key: tenant.id,
              onClick: () => selectTenant(tenant),
            },
              h('div', { class: 'tenant-card-top' },
                h('div', { class: 'tenant-card-copy' },
                  h('div', { class: 'tenant-card-title' }, tenant.name),
                  h('div', { class: 'tenant-card-path' }, '/t/' + tenant.slug),
                ),
                h('span', {
                  class: 'tenant-card-badge' + (tenant.onboarding_completed ? '' : ' tenant-card-badge-pending'),
                }, tenant.onboarding_completed ? 'Ready' : 'Setup'),
              ),
              h('div', { class: 'tenant-card-meta' },
                TenantMeta('Plan', tenant.plan),
                TenantMeta('Status', tenant.status),
                TenantMeta('Surface', tenant.onboarding_completed ? 'Workspace' : 'Setup'),
              ),
              h('div', { class: 'tenant-card-footer' }, 'Open workspace'),
            ),
          ),
        ),
      ),
    ),
  );
}

function TenantMeta(label: string, value: string) {
  return h('div', { class: 'tenant-card-stat' },
    h('span', { class: 'tenant-card-stat-label' }, label),
    h('span', { class: 'tenant-card-stat-value' }, value),
  );
}

function TenantCreatePanel() {
  const [name, setName] = createSignal('');
  const [slug, setSlug] = createSignal('');
  const [slugEdited, setSlugEdited] = createSignal(false);
  const [saving, setSaving] = createSignal(false);

  function handleNameChange(value: string) {
    setName(value);
    if (!slugEdited()) {
      setSlug(normalizeTenantSlugInput(value));
    }
  }

  function handleSlugChange(value: string) {
    setSlugEdited(true);
    setSlug(normalizeTenantSlugInput(value));
  }

  async function handleSubmit(event: Event) {
    event.preventDefault();

    const nextName = name().trim();
    const nextSlug = slug().trim();

    if (!nextName) {
      toast.show('App name is required', 'error');
      return;
    }
    if (!nextSlug) {
      toast.show('Tenant slug is required', 'error');
      return;
    }

    setSaving(true);
    const created = await createTenant({ name: nextName, slug: nextSlug });
    setSaving(false);

    if (!created) {
      return;
    }

    setName('');
    setSlug('');
    setSlugEdited(false);
    selectTenant(created);
  }

  return h('section', { class: 'console-card tenant-create-panel' },
    h('div', { class: 'card-label' }, tenants().length === 0 ? 'Create your first tenant' : 'Create another tenant'),
    h('h3', { class: 'section-title' }, tenants().length === 0 ? 'Choose the name and URL you want' : 'Add a new workspace'),
    h('p', { class: 'card-copy' },
      'This slug becomes part of your hosted login, site, and developer URLs, so it should feel intentional and easy to share.',
    ),
    h('form', { class: 'tenant-create-form', onSubmit: handleSubmit },
      Field({
        label: 'App Name',
        name: 'tenant-name',
        value: name,
        onChange: handleNameChange,
        placeholder: 'Acme Workspace',
        hint: 'This appears in the console and the hosted tenant experience.',
        required: true,
      }),
      Field({
        label: 'Tenant Slug',
        name: 'tenant-slug',
        value: slug,
        onChange: handleSlugChange,
        placeholder: 'acme-workspace',
        hint: 'Lowercase letters, numbers, and hyphens only.',
        required: true,
      }),
      h('div', { class: 'tenant-create-preview' },
        h('span', { class: 'detail-label' }, 'Tenant URL'),
        h('code', { class: 'code-block code-inline-block break-all' },
          () => slug() ? '/t/' + slug() : '/t/your-slug',
        ),
      ),
      h('div', { class: 'card-actions' },
        h('button', {
          type: 'submit',
          class: 'btn btn-primary btn-sm',
          disabled: () => saving() || !name().trim() || !slug().trim(),
        }, () => saving() ? 'Creating...' : 'Create Tenant'),
      ),
    ),
  );
}


function AccountTabBtn(id: AccountTab, label: string) {
  return h('button', {
    type: 'button',
    class: () => 'account-tab' + (accountTab() === id ? ' active' : ''),
    onClick: () => setAccountTab(id),
  }, label);
}

export function ConsoleApp() {
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const [themeMode, setThemeMode] = createSignal<ThemeMode>(getThemeMode());
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const shortcutLabel = /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K';

  const handleThemeToggle = () => {
    setThemeMode(toggleThemeMode(themeMode()));
  };

  function AccountShell() {
    return h('div', { class: 'account-shell' },
      h('header', { class: 'account-header' },
        h('div', { class: 'account-header-left' },
          h('div', { class: 'plane-logo-mark' }),
          h('span', { class: 'plane-product-name' }, 'GateWASM'),
        ),
        h('div', { class: 'account-header-right' },
          h('button', {
            type: 'button',
            class: 'plane-avatar hv',
            onClick: handleThemeToggle,
            title: () => themeMode() === 'dark' ? 'Switch to light mode' : 'Switch to dark mode',
          }, () => themeMode() === 'dark' ? '\u263E' : '\u2600'),
          h('button', {
            type: 'button',
            class: 'plane-avatar hv',
            onClick: () => void signout(),
            title: 'Sign out',
          }, () => {
            const u = currentUser();
            return u ? userInitials(u.name, u.email) : '--';
          }),
        ),
      ),
      h('div', { class: 'account-tabs' },
        AccountTabBtn('tenants', 'Tenants'),
        AccountTabBtn('api-keys', 'API Keys'),
        AccountTabBtn('account-settings', 'Settings'),
      ),
      h('div', { class: 'account-stage' },
        h('main', { class: 'main page-enter' },
          LoadingBar(() => loading()),
          createSwitch(
            accountTab,
            [
              { match: 'tenants', render: () => TenantsList() },
              { match: 'api-keys', render: () => ApiKeysView() },
              { match: 'account-settings', render: () => AccountSettingsView() },
            ],
          ),
        ),
      ),
    );
  }

  const openCommandPalette = () => setPaletteOpen(true);
  const closeCommandPalette = () => setPaletteOpen(false);
  const openConsoleHome = () => backToTenants();

  createKeyboard([
    { keys: 'Meta+k', handler: openCommandPalette },
    { keys: 'Ctrl+k', handler: openCommandPalette },
  ]);

  const paletteSections = (): CommandPaletteSection[] => {
    const currentTenant = selectedTenant();
    const currentView = view();
    const currentTenantPath = currentTenant ? '/t/' + currentTenant.slug : '';
    const sections: CommandPaletteSection[] = [
      {
        id: 'navigate',
        label: 'Navigate',
        items: [
          {
            id: 'palette-all-tenants',
            title: 'All Tenants',
            subtitle: 'Return to the account view and tenant list.',
            keywords: ['console tenants workspace list account'],
            onSelect: openConsoleHome,
          },
          {
            id: 'palette-reload',
            title: 'Reload Data',
            subtitle: currentTenant
              ? 'Refresh the current workspace data without leaving the page.'
              : 'Refresh the tenant list and latest console state.',
            keywords: ['refresh reload sync data'],
            onSelect: () => { void refreshCurrentView(); },
          },
        ],
      },
    ];

    if (currentTenant) {
      sections[0].items.push(
        {
          id: 'palette-overview',
          title: 'Overview',
          subtitle: 'Return to the workspace dashboard for ' + currentTenantPath,
          meta: currentView === 'overview' ? 'Current page' : undefined,
          keywords: ['dashboard overview tenant summary'],
          onSelect: () => navigateTenantView('overview'),
        },
        {
          id: 'palette-members',
          title: 'Members',
          subtitle: 'Inspect members, providers, and session controls.',
          meta: currentView === 'members' ? 'Current page' : undefined,
          keywords: ['users members sessions people'],
          onSelect: () => navigateTenantView('members' as TenantWorkspaceView),
        },
        {
          id: 'palette-integrate',
          title: 'Integrate',
          subtitle: 'Open setup steps, URLs, and implementation guidance.',
          meta: currentView === 'integrate' ? 'Current page' : undefined,
          keywords: ['integrate setup guide bridge snippet'],
          onSelect: openIntegrateGuide,
        },
        {
          id: 'palette-inspector',
          title: 'Auth Reference',
          subtitle: 'View the auth payload shape, token format, and tenant endpoints.',
          meta: currentView === 'integrate' ? 'Tool' : undefined,
          keywords: ['auth reference inspect callback token payload'],
          onSelect: openSessionInspector,
        },
        {
          id: 'palette-hosted-page',
          title: 'Hosted Page',
          subtitle: 'Edit the public gated page and preview the hosted surface.',
          meta: currentView === 'hosted-page' ? 'Current page' : undefined,
          keywords: ['hosted page site preview login page'],
          onSelect: () => navigateTenantView('hosted-page'),
        },
        {
          id: 'palette-roles',
          title: 'Roles',
          subtitle: 'Manage tenant roles and permission assignments.',
          meta: currentView === 'roles' ? 'Current page' : undefined,
          keywords: ['roles permissions rbac access'],
          onSelect: () => navigateTenantView('roles' as TenantWorkspaceView),
        },
        {
          id: 'palette-audit-log',
          title: 'Audit Log',
          subtitle: 'Review the event log for authentication and admin actions.',
          meta: currentView === 'audit-log' ? 'Current page' : undefined,
          keywords: ['audit log events history trail'],
          onSelect: () => navigateTenantView('audit-log' as TenantWorkspaceView),
        },
        {
          id: 'palette-sessions',
          title: 'Sessions',
          subtitle: 'View and manage active tenant sessions.',
          meta: currentView === 'sessions' ? 'Current page' : undefined,
          keywords: ['sessions active login tokens'],
          onSelect: () => navigateTenantView('sessions' as TenantWorkspaceView),
        },
        {
          id: 'palette-passkeys',
          title: 'Passkeys',
          subtitle: 'Manage WebAuthn passkeys and biometric credentials.',
          meta: currentView === 'passkeys' ? 'Current page' : undefined,
          keywords: ['passkeys webauthn biometric fido'],
          onSelect: () => navigateTenantView('passkeys' as TenantWorkspaceView),
        },
        {
          id: 'palette-tokens',
          title: 'Tokens',
          subtitle: 'Inspect and manage issued tokens and refresh grants.',
          meta: currentView === 'tokens' ? 'Current page' : undefined,
          keywords: ['tokens jwt refresh access grant'],
          onSelect: () => navigateTenantView('tokens' as TenantWorkspaceView),
        },
        {
          id: 'palette-activity',
          title: 'Activity',
          subtitle: 'View real-time activity and usage metrics.',
          meta: currentView === 'activity' ? 'Current page' : undefined,
          keywords: ['activity metrics usage analytics'],
          onSelect: () => navigateTenantView('activity' as TenantWorkspaceView),
        },
      );

      sections.push({
        id: 'links',
        label: 'Tenant Surfaces',
        items: [
          {
            id: 'palette-login-preview',
            title: 'Preview Login Page',
            subtitle: serverUrl + '/t/' + currentTenant.slug + '/login?preview=1',
            meta: 'Open',
            keywords: ['login preview hosted login page'],
            onSelect: () => window.open(serverUrl + '/t/' + currentTenant.slug + '/login?preview=1', '_blank', 'noopener'),
          },
          {
            id: 'palette-protected-site',
            title: 'Open Protected Site',
            subtitle: serverUrl + '/t/' + currentTenant.slug + '/site',
            meta: 'Open',
            keywords: ['site protected page gated app'],
            onSelect: () => window.open(serverUrl + '/t/' + currentTenant.slug + '/site', '_blank', 'noopener'),
          },
        ],
      });
    }

    if (tenants().length > 0) {
      sections.push({
        id: 'tenants',
        label: 'Switch Tenant',
        items: tenants().map((tenant) => ({
          id: 'tenant-' + tenant.id,
          title: tenant.name,
          subtitle: '/t/' + tenant.slug,
          meta: selectedTenant()?.id === tenant.id ? 'Active' : tenant.status,
          keywords: [tenant.name, tenant.slug, tenant.plan, tenant.status],
          onSelect: () => selectTenant(tenant),
        })),
      });
    }

    sections.push({
      id: 'account',
      label: 'Account',
      items: [
        {
          id: 'palette-theme',
          title: themeMode() === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode',
          subtitle: 'Flip the console theme without reloading the page.',
          meta: themeMode() === 'dark' ? 'Dark' : 'Light',
          keywords: ['theme dark light appearance'],
          onSelect: handleThemeToggle,
        },
        {
          id: 'palette-signout',
          title: 'Sign Out',
          subtitle: 'Leave the platform console and return to login.',
          keywords: ['logout sign out leave account'],
          tone: 'danger',
          onSelect: signout,
        },
      ],
    });

    return sections;
  };

  return h('div', null,
    createSwitch(
      mode,
      [
        { match: 'account', render: () => AccountShell() },
        {
          match: 'workspace',
          render: () => h('div', {
            class: 'shell',
            'data-plane': () => plane(),
          },
            PlaneNav({
              plane: () => plane(),
              view: () => view(),
              onPlaneSwitch: (p: Plane) => setPlane(p),
              onTabSwitch: (v: ConsoleView) => navigateTenantView(v),
              onSettings: () => backToTenants(),
              onSignOut: () => void signout(),
              onThemeToggle: handleThemeToggle,
              themeLabel: () => themeMode() === 'dark' ? 'Light Mode' : 'Dark Mode',
              user: () => currentUser(),
              tenant: () => selectedTenant(),
              tenants: () => tenants(),
              onTenantSwitch: (t) => selectTenant(t),
              onBackToAccount: () => backToTenants(),
            }),
            h('div', { class: 'shell-stage' },
              h('main', { class: 'main page-enter' },
                LoadingBar(() => loading()),
                createSwitch(
                  () => view(),
                  [
                    { match: 'overview', render: () => OverviewView() },
                    { match: 'members', render: () => UsersView() },
                    { match: 'integrate', render: () => IntegrateView() },
                    { match: 'security', render: () => SecurityConfigView() },
                    { match: 'hosted-page', render: () => HostedPageView() },
                    { match: 'organizations', render: () => h('div', { class: 'console-view' }, h('h2', { class: 'view-title' }, 'Organizations')) },
                    { match: 'roles', render: () => RolesView() },
                    { match: 'audit-log', render: () => AuditLogView() },
                    { match: 'sessions', render: () => SessionsView() },
                    { match: 'passkeys', render: () => PasskeysView() },
                    { match: 'tokens', render: () => TokensView() },
                    { match: 'activity', render: () => ActivityView() },
                  ],
                ),
              ),
            ),
            CommandPalette({
              open: paletteOpen,
              onClose: closeCommandPalette,
              shortcutLabel,
              sections: paletteSections,
            }),
            ToastView(toast),
          ),
        },
      ],
    ),
  );
}
