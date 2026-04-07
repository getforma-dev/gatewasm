import { h, createSignal, createShow, createSwitch, onCleanup } from 'formajs';
import { Field } from '../../core/components/ui/Field';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { createKeyboard } from '../../core/hooks/createKeyboard';
import { getThemeMode, toggleThemeMode, type ThemeMode } from '../../core/utils';
import { normalizeTenantSlugInput } from '../tenantSlug';
import { CommandPalette, type CommandPaletteSection } from './CommandPalette';
import { PlaneNav } from './views/PlaneNav';
import { userDisplayName, userInitials, FAuthMark } from './helpers';
import { OverviewView } from './views/OverviewView';
import { UsersView } from './views/UsersView';
import { IntegrateView } from './views/IntegrateView';
import { HostedPageView } from './views/HostedPageView';
import { AuditLogView } from './views/AuditLogView';
import { ActivityView } from './views/ActivityView';
import { SessionsView } from './views/SessionsView';
import { PasskeysView } from './views/PasskeysView';
import { OrganizationsView } from './views/OrganizationsView';
import { RolesView } from './views/RolesView';
import { SecurityConfigView } from './views/SecurityConfigView';
import { TokensView } from './views/TokensView';
import { ApiKeysView } from './views/ApiKeysView';
import { AccountSettingsView } from './views/AccountSettingsView';
import { TeamView } from './views/TeamView';
import {
  view, loading, toast, backToTenants, signout,
  selectedTenant, tenants, selectTenant, createTenant, checkSlugAvailability, currentUser,
  navigateTenantView, refreshCurrentView, openIntegrateGuide, openSessionInspector,
  plane, setPlane, type Plane, type ConsoleView,
  type TenantWorkspaceView,
  mode, accountTab, setAccountTab, type AccountTab,
  accountOwner,
  canEdit,
  serverUrl,
  resendVerification,
} from './store';

function TenantsList() {
  const [showCreate, setShowCreate] = createSignal(false);

  const ownTenants = () => tenants().filter((t: any) => t.access !== 'team');
  const teamTenants = () => tenants().filter((t: any) => t.access === 'team');

  const tenantCountLabel = () => {
    const own = ownTenants().length;
    const team = teamTenants().length;
    if (own === 0 && team === 0) return 'Create your first workspace to get started';
    const parts: string[] = [];
    if (own > 0) parts.push(own === 1 ? '1 workspace' : own + ' workspaces');
    if (team > 0) parts.push(team === 1 ? '1 shared' : team + ' shared');
    return parts.join(' · ');
  };

  function TenantCard(tenant: any) {
    const isTeam = tenant.access === 'team';
    const teamRole = tenant.team_role as string | undefined;
    return h('button', {
      type: 'button',
      class: 'tenant-card' + (isTeam ? ' tenant-card-team' : ''),
      key: tenant.id,
      onClick: () => selectTenant(tenant),
    },
      h('div', { class: 'tenant-card-top' },
        h('div', { class: 'tenant-card-copy' },
          h('div', { class: 'tenant-card-title' }, tenant.name),
          h('div', { class: 'tenant-card-path' }, '/t/' + tenant.slug),
        ),
        isTeam && teamRole
          ? h('span', { class: 'tenant-card-badge tenant-card-badge-role' },
              teamRole.charAt(0).toUpperCase() + teamRole.slice(1))
          : h('span', {
              class: 'tenant-card-badge' + (tenant.onboarding_completed ? '' : ' tenant-card-badge-pending'),
            }, tenant.onboarding_completed ? 'Ready' : 'Setup'),
      ),
      h('div', { class: 'tenant-card-meta' },
        TenantMeta('Plan', tenant.plan),
        TenantMeta('Status', tenant.status),
        isTeam
          ? TenantMeta('Access', teamRole || 'member')
          : TenantMeta('Surface', tenant.onboarding_completed ? 'Workspace' : 'Setup'),
      ),
      h('div', { class: 'tenant-card-footer' }, isTeam ? 'Open shared workspace' : 'Open workspace'),
    );
  }

  return h('div', { class: 'console-view console-view-tenants' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'Your Workspaces'),
        h('p', { class: 'view-subtitle text-muted' }, tenantCountLabel),
      ),
      h('div', { class: 'view-header-actions' },
        h('button', {
          type: 'button',
          class: 'clip-btn',
          disabled: () => !canEdit(),
          onClick: () => setShowCreate(true),
        }, '+ New Workspace'),
      ),
    ),

    // Empty state (no workspaces at all)
    createShow(
      () => tenants().length === 0,
      () => h('div', { class: 'console-card tenant-empty-hero' },
        h('div', { class: 'tenant-empty-content' },
          h('div', { class: 'diamond-label' }, 'No Workspaces Yet'),
          h('h3', { class: 'section-title' }, 'Create a workspace to get started'),
          h('p', { class: 'card-copy' },
            'Each workspace is a tenant with its own users, login page, roles, and security settings. You need at least one to start authenticating people.',
          ),
          h('div', { class: 'tenant-empty-actions' },
            h('button', {
              type: 'button',
              class: 'clip-btn',
              disabled: () => !canEdit(),
              onClick: () => setShowCreate(true),
            }, '+ Create Workspace'),
          ),
        ),
      ),
    ),

    // Own workspace cards
    createShow(
      () => ownTenants().length > 0,
      () => h('div', { class: 'tenant-card-grid' },
        () => ownTenants().map((tenant: any) => TenantCard(tenant)),
      ),
    ),

    // Team workspace cards (separate section)
    createShow(
      () => teamTenants().length > 0,
      () => h('div', { class: 'team-tenants-section' },
        h('div', { class: 'card-label' }, 'Shared With You'),
        h('div', { class: 'tenant-card-grid' },
          () => teamTenants().map((tenant: any) => TenantCard(tenant)),
        ),
      ),
    ),

    // Create modal
    createShow(
      () => showCreate(),
      () => CreateTenantModal(() => setShowCreate(false)),
    ),
  );
}

function TenantMeta(label: string, value: string) {
  return h('div', { class: 'tenant-card-stat' },
    h('span', { class: 'tenant-card-stat-label' }, label),
    h('span', { class: 'tenant-card-stat-value' }, value),
  );
}

function CreateTenantModal(onClose: () => void) {
  const [name, setName] = createSignal('');
  const [slug, setSlug] = createSignal('');
  const [slugEdited, setSlugEdited] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [slugStatus, setSlugStatus] = createSignal<'idle' | 'checking' | 'available' | 'taken' | 'invalid'>('idle');
  const [slugWarn, setSlugWarn] = createSignal('');
  const [slugError, setSlugError] = createSignal('');
  const [slugSuggestion, setSlugSuggestion] = createSignal('');
  let checkTimer: ReturnType<typeof setTimeout> | undefined;

  function closeModal() {
    clearTimeout(checkTimer);
    onClose();
  }

  function validateAndCheckSlug(raw: string, normalized: string) {
    clearTimeout(checkTimer);
    setSlugWarn('');
    setSlugError('');

    // Detect stripped characters — yellow warning only
    if (raw !== normalized && raw.length > 0) {
      const stripped = raw.replace(/[a-z0-9-]/g, '');
      if (stripped.length > 0) {
        setSlugWarn('Invalid characters removed');
      }
    }

    // Validate the normalized slug
    if (!normalized) {
      setSlugStatus('idle');
      return;
    }

    if (normalized.length < 2) {
      setSlugStatus('invalid');
      setSlugError('Slug must be at least 2 characters');
      return;
    }

    if (normalized.length > 63) {
      setSlugStatus('invalid');
      setSlugError('Slug must be 63 characters or fewer');
      return;
    }

    // Check reserved slugs client-side
    const reserved = ['platform', 'api', 'admin', 'manage', 'auth', 'health', 'docs', 'www',
      'app', 'dashboard', 'billing', 'support', 'login', 'register', 'signup',
      'get-started', 'bridge', 'openapi', 't', 'static', 'assets'];
    if (reserved.includes(normalized)) {
      setSlugStatus('invalid');
      setSlugError('\u201C' + normalized + '\u201D is reserved');
      return;
    }

    // All local checks pass — check availability
    setSlugStatus('checking');
    checkTimer = setTimeout(async () => {
      try {
        const available = await checkSlugAvailability(normalized);
        if (slug() === normalized) {
          if (available) {
            setSlugStatus('available');
          } else {
            setSlugStatus('taken');
            setSlugSuggestion(normalized + '-' + Math.random().toString(36).slice(2, 5));
          }
        }
      } catch {
        // Network error — don't show false "available", reset to idle
        if (slug() === normalized) {
          setSlugStatus('idle');
        }
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
      const newPos = Math.max(0, pos - removed);
      input.setSelectionRange(newPos, newPos);
    }
    setSlug(normalized);
    validateAndCheckSlug(raw.trim().toLowerCase(), normalized);
  }

  async function handleSubmit(event: Event) {
    event.preventDefault();
    clearTimeout(checkTimer);
    const nextName = name().trim();
    const nextSlug = slug().trim();
    if (!nextName) { toast.show('App name is required', 'error'); return; }
    if (!nextSlug) { toast.show('Tenant slug is required', 'error'); return; }
    if (slugStatus() === 'taken') { toast.show('This slug is already taken', 'error'); return; }
    setSaving(true);
    const created = await createTenant({ name: nextName, slug: nextSlug });
    setSaving(false);
    if (!created) return;
    closeModal();
    selectTenant(created);
  }

  return h('div', { class: 'modal-overlay', onClick: (e: Event) => {
    if ((e.target as HTMLElement).classList.contains('modal-overlay')) closeModal();
  }},
    h('div', { class: 'modal-content' },
      h('div', { class: 'modal-header' },
        h('h3', { class: 'section-title' }, 'Create Workspace'),
        h('button', {
          type: 'button',
          class: 'modal-close',
          onClick: closeModal,
        }, '\u00D7'),
      ),
      h('p', { class: 'card-copy', style: 'padding: 0 var(--space-lg)' },
        'Choose a name and slug for your new workspace. The slug becomes part of your hosted URLs.',
      ),
      h('form', { class: 'tenant-create-form', style: 'padding: var(--space-md) var(--space-lg) var(--space-lg)', onSubmit: handleSubmit },
        Field({
          label: 'App Name',
          name: 'tenant-name',
          value: name,
          onChange: handleNameChange,
          placeholder: 'Acme Workspace',
          hint: 'This appears in the console and the hosted tenant experience.',
          required: true,
        }),
        h('div', { class: 'field' },
          h('label', { class: 'field-label', for: 'field-tenant-slug' }, 'Tenant Slug'),
          h('input', {
            id: 'field-tenant-slug',
            type: 'text',
            name: 'tenant-slug',
            value: slug,
            placeholder: 'acme-workspace',
            required: true,
            onInput: handleSlugInput,
          }),
          h('div', { class: 'field-hint' }, 'Lowercase letters, numbers, and hyphens only.'),
        ),
        // Slug validation + availability indicator
        h('div', { class: 'slug-status' },
          createShow(
            () => slugWarn().length > 0,
            () => h('span', { class: 'slug-status-hint' }, slugWarn),
          ),
          createShow(
            () => slugStatus() === 'checking',
            () => h('span', { class: 'slug-status-checking' }, 'Checking availability...'),
          ),
          createShow(
            () => slugStatus() === 'available',
            () => h('span', { class: 'slug-status-available' }, () => '\u2713 ' + slug() + ' is available'),
          ),
          createShow(
            () => slugStatus() === 'invalid',
            () => h('span', { class: 'slug-status-invalid' }, () => '\u2717 ' + slugError()),
          ),
          createShow(
            () => slugStatus() === 'taken',
            () => h('div', { class: 'slug-status-taken' },
              h('span', null, '\u2717 Already taken'),
              h('button', {
                type: 'button',
                class: 'slug-suggestion',
                onClick: () => {
                  const s = slugSuggestion();
                  const input = document.getElementById('field-tenant-slug') as HTMLInputElement;
                  if (input) input.value = s;
                  setSlug(s);
                  setSlugEdited(true);
                  validateAndCheckSlug(s, s);
                },
              }, () => 'Try ' + slugSuggestion()),
            ),
          ),
        ),
        h('div', { class: 'tenant-create-preview' },
          h('span', { class: 'detail-label' }, 'Tenant URL'),
          h('code', { class: 'code-block code-inline-block break-all' },
            () => slug() ? '/t/' + slug() : '/t/your-slug',
          ),
        ),
        h('div', { class: 'modal-actions' },
          h('button', {
            type: 'button',
            class: 'btn btn-ghost btn-sm',
            onClick: closeModal,
          }, 'Cancel'),
          h('button', {
            type: 'submit',
            class: 'clip-btn',
            disabled: () => saving() || !name().trim() || !slug().trim() || slugStatus() === 'taken' || slugStatus() === 'checking' || slugStatus() === 'invalid',
          }, () => saving() ? 'Creating...' : 'Create Workspace'),
        ),
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

  const [themeMode, setThemeMode] = createSignal<ThemeMode>(getThemeMode());
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const shortcutLabel = /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K';

  const handleThemeToggle = () => {
    setThemeMode(toggleThemeMode(themeMode()));
  };

  function AccountShell() {
    const [accountMenuOpen, setAccountMenuOpen] = createSignal(false);

    // Close on outside click
    const accountAvatarShell = h('div', { class: 'plane-avatar-shell' },
      h('button', {
        type: 'button',
        class: 'plane-avatar hv',
        'aria-haspopup': 'menu',
        'aria-expanded': () => String(accountMenuOpen()),
        onClick: () => setAccountMenuOpen((v: boolean) => !v),
      }, () => {
        const u = currentUser();
        return u ? userInitials(u.name, u.email) : '--';
      }),
      createShow(accountMenuOpen, () =>
        h('div', { class: 'plane-avatar-menu', role: 'menu' },
          h('div', { class: 'plane-avatar-menu-header' },
            h('div', { class: 'plane-avatar-menu-name' }, () => {
              const u = currentUser();
              return u ? userDisplayName(u.name, u.email) : '';
            }),
            h('div', { class: 'plane-avatar-menu-email' }, () => currentUser()?.email || ''),
          ),
          h('div', { class: 'plane-avatar-menu-divider' }),
          h('button', {
            type: 'button',
            class: 'plane-avatar-menu-item hv',
            role: 'menuitem',
            onClick: () => { setAccountMenuOpen(false); handleThemeToggle(); },
          }, () => themeMode() === 'dark' ? 'Light Mode' : 'Dark Mode'),
          h('button', {
            type: 'button',
            class: 'plane-avatar-menu-item plane-avatar-menu-item-danger hv',
            role: 'menuitem',
            onClick: () => { setAccountMenuOpen(false); void signout(); },
          }, 'Sign Out'),
        ),
      ),
    ) as HTMLElement;

    // Close menu on outside click
    function handleAccountMenuClick(e: Event) {
      if (!accountMenuOpen()) return;
      if (!accountAvatarShell.contains(e.target as Node)) setAccountMenuOpen(false);
    }
    document.addEventListener('click', handleAccountMenuClick);
    onCleanup(() => document.removeEventListener('click', handleAccountMenuClick));

    return h('div', { class: 'account-shell' },
      h('header', { class: 'account-header' },
        h('div', { class: 'account-header-left' },
          h('div', { class: 'plane-logo-mark' }, FAuthMark(28)),
          h('span', { class: 'plane-product-name' }, 'fAuth'),
        ),
        h('div', { class: 'account-header-right' },
          accountAvatarShell,
        ),
      ),
      // Email verification banner
      createShow(
        () => currentUser() !== null && !currentUser()!.email_verified,
        () => h('div', { class: 'verify-banner' },
          h('span', null, 'Verify your email address to secure your account. Check your inbox.'),
          h('button', {
            type: 'button',
            class: 'btn btn-sm btn-ghost',
            onClick: () => void resendVerification(),
          }, 'Resend'),
        ),
      ),
      h('div', { class: 'account-tabs' },
        AccountTabBtn('tenants', 'Tenants'),
        AccountTabBtn('team', 'Members'),
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
              { match: 'team', render: () => TeamView() },
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
              onSettings: () => { backToTenants(); setAccountTab('account-settings'); },
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
                    { match: 'organizations', render: () => OrganizationsView() },
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
