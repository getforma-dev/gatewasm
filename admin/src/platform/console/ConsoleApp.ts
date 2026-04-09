import { h, createSignal, createShow, createSwitch, onCleanup } from 'formajs';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { createKeyboard } from '../../core/hooks/createKeyboard';
import { getThemeMode, toggleThemeMode, type ThemeMode } from '../../core/utils';
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
import { OnboardingWizard } from './OnboardingWizard';
import {
  view, loading, toast, backToTenants, signout,
  selectedTenant, tenants, selectTenant, currentUser,
  navigateTenantView, refreshCurrentView, openIntegrateGuide, openSessionInspector,
  plane, setPlane, type Plane, type ConsoleView,
  type TenantWorkspaceView,
  mode, accountTab, setAccountTab, type AccountTab,
  canEdit,
  serverUrl,
  resendVerification, resendCooldown, resendAttempts, resendMaxed,
  pendingInvites,
  acceptInviteFromDashboard,
  startNewWorkspaceWizard,
} from './store';

function TenantsList() {
  const ownTenants = () => tenants().filter((t: any) => t.access !== 'team');
  const teamTenants = () => tenants().filter((t: any) => t.access === 'team');

  const tenantCountLabel = () => {
    const own = ownTenants().length;
    const team = teamTenants().length;
    if (own === 0 && team === 0) return 'Create your first application to get started';
    const parts: string[] = [];
    if (own > 0) parts.push(own === 1 ? '1 application' : own + ' applications');
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
          : TenantMeta('Surface', tenant.onboarding_completed ? 'Application' : 'Setup'),
      ),
      h('div', { class: 'tenant-card-footer' }, isTeam ? 'Open shared application' : 'Open application'),
    );
  }

  return h('div', { class: 'console-view console-view-tenants' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'Your Applications'),
        h('p', { class: 'view-subtitle text-muted' }, tenantCountLabel),
      ),
      h('div', { class: 'view-header-actions' },
        h('button', {
          type: 'button',
          class: 'clip-btn',
          disabled: () => !canEdit(),
          onClick: () => startNewWorkspaceWizard(),
        }, '+ New Application'),
      ),
    ),

    // Empty state (no applications at all)
    createShow(
      () => tenants().length === 0,
      () => h('div', { class: 'console-card tenant-empty-hero' },
        h('div', { class: 'tenant-empty-content' },
          h('div', { class: 'diamond-label' }, 'No Applications Yet'),
          h('h3', { class: 'section-title' }, 'Create an application to get started'),
          h('p', { class: 'card-copy' },
            'Each application has its own users, login page, roles, and security settings. You need at least one to start authenticating people.',
          ),
          h('div', { class: 'tenant-empty-actions' },
            h('button', {
              type: 'button',
              class: 'clip-btn',
              disabled: () => !canEdit(),
              onClick: () => startNewWorkspaceWizard(),
            }, '+ Create Application'),
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
  );
}

function TenantMeta(label: string, value: string) {
  return h('div', { class: 'tenant-card-stat' },
    h('span', { class: 'tenant-card-stat-label' }, label),
    h('span', { class: 'tenant-card-stat-value' }, value),
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
          h('span', null, () => {
            if (resendMaxed()) return 'Too many resend attempts. Please check your inbox or spam folder, or try again later.';
            if (resendAttempts() > 0) return 'Verification email sent! Check your inbox or spam folder.';
            return 'Verify your email address to secure your account. Check your inbox.';
          }),
          createShow(
            () => !resendMaxed(),
            () => h('button', {
              type: 'button',
              class: 'btn btn-sm',
              disabled: () => resendCooldown() !== 0,
              onClick: () => void resendVerification(),
            }, () => {
              const cd = resendCooldown();
              if (cd === -1) return 'Sending\u2026';
              if (cd > 0) return 'Resend in ' + cd + 's';
              return 'Resend';
            }),
          ),
        ),
      ),
      // Pending invite banners
      () => pendingInvites().map((inv) =>
        h('div', { class: 'invite-accept-banner', key: inv.id },
          h('span', null,
            (inv.inviter_email || 'A team member') + ' invited you as ' +
            inv.role.charAt(0).toUpperCase() + inv.role.slice(1) +
            ' on ' + inv.tenant_count + ' application' + (inv.tenant_count !== 1 ? 's' : ''),
          ),
          h('button', {
            type: 'button',
            class: 'btn btn-sm btn-primary',
            onClick: () => void acceptInviteFromDashboard(inv.id),
          }, 'Accept'),
        ),
      ),
      h('div', { class: 'account-tabs' },
        AccountTabBtn('tenants', 'Applications'),
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
            title: 'All Applications',
            subtitle: 'Return to the account view and application list.',
            keywords: ['console tenants applications workspace list account'],
            onSelect: openConsoleHome,
          },
          {
            id: 'palette-reload',
            title: 'Reload Data',
            subtitle: currentTenant
              ? 'Refresh the current application data without leaving the page.'
              : 'Refresh the application list and latest console state.',
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
          subtitle: 'Return to the application dashboard for ' + currentTenantPath,
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
          render: () => createShow(
            () => selectedTenant() !== null && selectedTenant()!.onboarding_completed,
            () => h('div', {
              class: 'shell',
              'data-plane': () => plane(),
            },
            PlaneNav({
              plane: () => plane(),
              view: () => view(),
              onPlaneSwitch: (p: Plane) => setPlane(p),
              onTabSwitch: (v: ConsoleView) => navigateTenantView(v),
              onSettings: () => backToTenants('account-settings'),
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
            () => OnboardingWizard(),
          ),
        },
      ],
    ),
  );
}
