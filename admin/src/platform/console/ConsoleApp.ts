import { h, createSignal, createSwitch, createShow, onCleanup } from 'formajs';
import { Field } from '../../core/components/ui/Field';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Topbar } from '../../core/components/ui/Topbar';
import { Icon } from '../../core/components/ui/Icon';
import { ICONS } from '../../core/components/ui/icon-paths';
import { createKeyboard } from '../../core/hooks/createKeyboard';
import { detectEnv, getThemeMode, toggleThemeMode, type ThemeMode } from '../../core/utils';
import { normalizeTenantSlugInput } from '../tenantSlug';
import { CommandPalette, type CommandPaletteSection } from './CommandPalette';
import { OverviewView } from './views/OverviewView';
import { UsersView } from './views/UsersView';
import { IntegrateView } from './views/IntegrateView';
import { HostedPageView } from './views/HostedPageView';
import { SettingsView } from './views/SettingsView';
import {
  view, loading, toast, backToTenants, signout,
  selectedTenant, tenants, selectTenant, createTenant, currentUser,
  navigateTenantView, refreshCurrentView, openIntegrateGuide, openSessionInspector,
  introspectData, introspectUnavailable,
  type TenantWorkspaceView,
} from './store';

function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

function userDisplayName(name: string | null | undefined, email: string): string {
  const nextName = (name || '').trim();
  if (nextName) {
    return nextName;
  }

  const local = email.split('@')[0] || email;
  return local.replace(/[._-]+/g, ' ');
}

function userInitials(name: string | null | undefined, email: string): string {
  const source = userDisplayName(name, email)
    .split(/\s+/)
    .map((part) => part.trim())
    .filter(Boolean);

  if (source.length === 0) {
    return email.slice(0, 2).toUpperCase();
  }

  const initials = source
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase();

  return initials || email.slice(0, 2).toUpperCase();
}

const FORMAJS_MARK = '<svg width="64" height="64" viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg">' + ICONS['formajs-mark'] + '</svg>';

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
        'Create a tenant with the URL you actually want, then jump straight into overview, users, integrations, hosted page controls, and the live session inspector.',
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

function QuickSwitchTrigger(shortcutLabel: string, onOpen: () => void) {
  return h('button', {
    type: 'button',
    class: 'topbar-chip topbar-chip-button topbar-chip-command',
    onClick: onOpen,
    title: 'Open quick switcher',
    'aria-label': 'Open quick switcher',
  }, [
    Icon({ name: 'search', size: 14, class: 'topbar-chip-icon' }),
    h('span', null, 'Quick Switch'),
    h('span', { class: 'topbar-chip-shortcut' }, shortcutLabel),
  ]);
}

function TopbarActions(
  serverUrl: string,
  onOpenPalette: () => void,
  shortcutLabel: string,
) {
  const tenant = () => selectedTenant();

  return h('div', { class: 'topbar-context' },
    QuickSwitchTrigger(shortcutLabel, onOpenPalette),
    SessionStatusChip(serverUrl),
    () => tenant()
      ? [
          h('span', { class: 'topbar-chip topbar-chip-mono' }, '/t/' + (tenant()?.slug || '')),
          h('span', { class: 'topbar-chip' }, tenant()?.plan || ''),
          h('span', { class: 'topbar-chip' }, tenant()?.status || ''),
        ]
      : null,
  );
}

function SessionStatusChip(serverUrl: string) {
  const tenant = () => selectedTenant();
  const active = () => introspectData() != null;
  const unavailable = () => introspectUnavailable();

  return () => {
    const currentTenant = tenant();
    if (!currentTenant) {
      return null;
    }

    if (active()) {
      return h('button', {
        type: 'button',
        class: 'topbar-chip topbar-chip-button topbar-chip-status topbar-chip-status-active',
        onClick: openSessionInspector,
        title: 'Open Session Inspector',
        'aria-label': 'Open Session Inspector',
      },
        Icon({ name: 'check-circle', size: 14, class: 'topbar-chip-icon' }),
        'Tenant Session Active',
      );
    }

    if (unavailable()) {
      return h('button', {
        type: 'button',
        class: 'topbar-chip topbar-chip-button topbar-chip-status topbar-chip-status-missing',
        onClick: () => {
          window.open(
            serverUrl + '/t/' + currentTenant.slug + '/login?redirect=/t/' + currentTenant.slug + '/site',
            '_blank',
            'noopener',
          );
        },
        title: 'Start a tenant sign-in in a new tab',
        'aria-label': 'Start a tenant sign-in in a new tab',
      },
        Icon({ name: 'alert-circle', size: 14, class: 'topbar-chip-icon' }),
        'No Tenant Session',
      );
    }

    return h('span', {
      class: 'topbar-chip topbar-chip-status topbar-chip-status-pending',
      title: 'Checking whether this browser already has a tenant session',
    },
      Icon({ name: 'clock', size: 14, class: 'topbar-chip-icon' }),
      'Checking Session',
    );
  };
}

function SidebarAccountMenu(themeMode: () => ThemeMode, onToggleTheme: () => void) {
  const user = () => currentUser();
  if (!user()) {
    return null;
  }

  const [open, setOpen] = createSignal(false);
  const handleDocumentClick = (event: Event) => {
    const target = event.target as Node;
    if (container && !container.contains(target)) {
      setOpen(false);
    }
  };

  if (typeof document !== 'undefined') {
    document.addEventListener('click', handleDocumentClick);
    onCleanup(() => document.removeEventListener('click', handleDocumentClick));
  }

  const container = h('div', { class: 'sidebar-account-shell' },
    h('button', {
      type: 'button',
      class: 'sidebar-account sidebar-account-trigger',
      'aria-haspopup': 'menu',
      'aria-expanded': () => String(open()),
      onClick: () => setOpen((value: boolean) => !value),
    },
      h('div', { class: 'sidebar-account-avatar', 'aria-hidden': 'true' },
        () => user()?.avatar
          ? h('img', {
              class: 'sidebar-account-image',
              src: user()!.avatar!,
              alt: '',
              referrerPolicy: 'no-referrer',
            })
          : h('span', { class: 'sidebar-account-initials' }, () => userInitials(user()?.name, user()?.email || '')),
      ),
      h('div', { class: 'sidebar-account-copy' },
        h('div', { class: 'sidebar-account-name' }, () => userDisplayName(user()?.name, user()?.email || '')),
        h('div', { class: 'sidebar-account-email' }, () => user()?.email || ''),
      ),
      h('span', { class: 'sidebar-account-caret', 'aria-hidden': 'true' }, Icon({ name: 'chevron-down', size: 14 })),
    ),
    createShow(
      open,
      () => h('div', { class: 'sidebar-account-menu', role: 'menu' },
        h('div', { class: 'sidebar-account-menu-header' },
          h('div', { class: 'sidebar-account-menu-title' }, userDisplayName(user()?.name, user()?.email || '')),
          h('div', { class: 'sidebar-account-menu-meta' }, user()?.email || ''),
        ),
        h('div', { class: 'sidebar-account-menu-actions' },
          h('button', {
            type: 'button',
            class: 'sidebar-account-menu-item',
            role: 'menuitem',
            onClick: () => {
              onToggleTheme();
              setOpen(false);
            },
          },
            Icon({ name: themeMode() === 'dark' ? 'sun' : 'moon', size: 14 }),
            themeMode() === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode',
          ),
          h('button', {
            type: 'button',
            class: 'sidebar-account-menu-item danger',
            role: 'menuitem',
            onClick: () => {
              setOpen(false);
              signout();
            },
          },
            Icon({ name: 'signout', size: 14 }),
            'Sign Out',
          ),
        ),
      ),
    ),
  ) as HTMLElement;

  return container;
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

function TenantWorkspaceGuard(render: () => HTMLElement) {
  return () => selectedTenant() ? render() : TenantsList();
}

export function ConsoleApp() {
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const [themeMode, setThemeMode] = createSignal<ThemeMode>(getThemeMode());
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const hasTenantSelection = () => selectedTenant() != null;
  const shortcutLabel = /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K';

  const handleThemeToggle = () => {
    setThemeMode(toggleThemeMode(themeMode()));
  };

  const openCommandPalette = () => setPaletteOpen(true);
  const closeCommandPalette = () => setPaletteOpen(false);
  const openConsoleHome = () => backToTenants();

  const openTenantHome = () => {
    const tenant = selectedTenant();
    if (!tenant) return;
    navigateTenantView('overview');
  };

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
            id: 'palette-tenants',
            title: 'Your Tenants',
            subtitle: 'Back to the tenant list and workspace hub.',
            meta: currentView === 'tenants' ? 'Current page' : undefined,
            keywords: ['console tenants workspace list'],
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
          id: 'palette-users',
          title: 'Users',
          subtitle: 'Inspect members, providers, and session controls.',
          meta: currentView === 'users' ? 'Current page' : undefined,
          keywords: ['users members sessions'],
          onSelect: () => navigateTenantView('users'),
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
          title: 'Session Inspector',
          subtitle: 'Jump straight into the live tenant session debugging tools.',
          meta: currentView === 'integrate' ? 'Tool' : undefined,
          keywords: ['session inspector auth inspect callback token'],
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
          id: 'palette-settings',
          title: 'Settings',
          subtitle: 'Rename the tenant, update its slug, and manage defaults.',
          meta: currentView === 'settings' ? 'Current page' : undefined,
          keywords: ['settings slug name config'],
          onSelect: () => navigateTenantView('settings'),
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

  const sidebarGroups = () => {
    const groups: { label?: string; items: { id: string; label: string; icon: string }[] }[] = [
      {
        label: 'Platform',
        items: [
          { id: 'tenants', label: 'Tenants', icon: iconSvg('tenants') },
        ],
      },
    ];

    if (hasTenantSelection()) {
      groups.push(
        {
          label: 'Workspace',
          items: [
            { id: 'overview', label: 'Overview', icon: iconSvg('overview') },
            { id: 'users', label: 'Users', icon: iconSvg('users-stroke') },
          ],
        },
        {
          label: 'Build',
          items: [
            { id: 'integrate', label: 'Integrate', icon: iconSvg('integrate') },
            { id: 'hosted-page', label: 'Hosted Page', icon: iconSvg('hosted-page') },
            { id: 'settings', label: 'Settings', icon: iconSvg('settings-stroke') },
          ],
        },
      );
    }

    return groups;
  };

  const sidebarContext = () => {
    const tenant = selectedTenant();
    if (!tenant) {
      return null;
    }

    return h('div', { class: 'sidebar-context-card' },
      h('div', { class: 'sidebar-context-copy' },
        h('div', { class: 'sidebar-context-label' }, 'Active Tenant'),
        h('div', { class: 'sidebar-context-title' }, tenant.name),
        h('div', { class: 'sidebar-context-meta' }, '/t/' + tenant.slug),
      ),
      h('button', {
        type: 'button',
        class: 'sidebar-context-action',
        onClick: openCommandPalette,
      }, 'Quick switch'),
    );
  };

  const sidebarAccount = () => {
    return SidebarAccountMenu(themeMode, handleThemeToggle);
  };

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'FormaJS', mark: FORMAJS_MARK, subtitle: 'GATEWASM CONSOLE' },
      groups: sidebarGroups,
      footer: [
        { id: 'reload-data', label: 'Reload Data', icon: iconSvg('refresh-stroke') },
      ],
      activeView: () => view(),
      onNavigate: (id: string) => {
        if (id === 'reload-data') { void refreshCurrentView(); return; }
        if (id === 'tenants') { backToTenants(); return; }
        if (id === 'integrate') { openIntegrateGuide(); return; }
        if (['overview', 'users', 'hosted-page', 'settings'].includes(id)) {
          navigateTenantView(id as TenantWorkspaceView);
          return;
        }
      },
      env: detectEnv(),
      context: sidebarContext,
      account: sidebarAccount,
    }),
    h('div', { class: 'shell-stage' },
      Topbar({
        breadcrumbs: () => {
          const currentView = view();
          const tenant = selectedTenant();
          const crumbs: { label: string; onClick?: () => void }[] = [
            { label: 'Console', onClick: currentView !== 'tenants' ? openConsoleHome : undefined },
          ];
          if (tenant) crumbs.push({
            label: tenant.name,
            onClick: currentView !== 'overview' ? openTenantHome : undefined,
          });
          if (currentView !== 'tenants') {
            const labels: Record<string, string> = {
              overview: 'Overview',
              users: 'Users',
              integrate: 'Integrate',
              'hosted-page': 'Hosted Page',
              settings: 'Settings',
            };
            crumbs.push({ label: labels[currentView] || currentView });
          }
          return crumbs;
        },
        actions: () => TopbarActions(serverUrl, openCommandPalette, shortcutLabel),
      }),
      h('main', { class: 'main page-enter' },
        LoadingBar(() => loading()),
        createSwitch(
          () => view(),
          [
            { match: 'tenants', render: () => TenantsList() },
            { match: 'overview', render: TenantWorkspaceGuard(() => OverviewView()) },
            { match: 'users', render: TenantWorkspaceGuard(() => UsersView()) },
            { match: 'integrate', render: TenantWorkspaceGuard(() => IntegrateView()) },
            { match: 'hosted-page', render: TenantWorkspaceGuard(() => HostedPageView()) },
            { match: 'settings', render: TenantWorkspaceGuard(() => SettingsView()) },
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
  );
}
