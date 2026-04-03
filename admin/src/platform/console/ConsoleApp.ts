import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Topbar } from '../../core/components/ui/Topbar';
import { Icon } from '../../core/components/ui/Icon';
import { ICONS } from '../../core/components/ui/icon-paths';
import { detectEnv } from '../../core/utils';
import { WizardFlow } from './wizard/WizardFlow';
import { OverviewView } from './views/OverviewView';
import { UsersView } from './views/UsersView';
import { IntegrateView } from './views/IntegrateView';
import { HostedPageView } from './views/HostedPageView';
import { SettingsView } from './views/SettingsView';
import {
  view, setView, loading, toast, backToTenants, loadTenants, signout,
  selectedTenant, tenants, selectTenant,
} from './store';

function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

const FORMAJS_MARK = '<svg width="64" height="64" viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg">' + ICONS['formajs-mark'] + '</svg>';

function TenantsList() {
  const tenantCountLabel = () => {
    const count = tenants().length;
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
        'Choose a tenant to jump straight into its overview, users, integrations, and hosted page controls.',
      ),
    ),
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
            TenantMeta('Surface', tenant.onboarding_completed ? 'Console' : 'Wizard'),
          ),
          h('div', { class: 'tenant-card-footer' },
            tenant.onboarding_completed ? 'Open workspace' : 'Continue onboarding',
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

function TopbarTenantContext() {
  return h('div', { class: 'topbar-context' },
    h('span', { class: 'topbar-chip topbar-chip-mono' }, () => '/t/' + (selectedTenant()?.slug || '')),
    h('span', { class: 'topbar-chip' }, () => selectedTenant()?.plan || ''),
    h('span', { class: 'topbar-chip' }, () => selectedTenant()?.status || ''),
  );
}

export function ConsoleApp() {
  const isOnboarded = () => {
    const tenant = selectedTenant();
    return tenant != null && tenant.onboarding_completed;
  };

  const sidebarItems = () => {
    const items: { id: string; label: string; icon: string }[] = [
      { id: 'tenants', label: 'Tenants', icon: iconSvg('tenants') },
    ];
    if (isOnboarded()) {
      items.push(
        { id: 'overview', label: 'Overview', icon: iconSvg('overview') },
        { id: 'users', label: 'Users', icon: iconSvg('users-stroke') },
        { id: 'integrate', label: 'Integrate', icon: iconSvg('integrate') },
        { id: 'hosted-page', label: 'Hosted Page', icon: iconSvg('hosted-page') },
        { id: 'settings', label: 'Settings', icon: iconSvg('settings-stroke') },
      );
    }
    return items;
  };

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'FormaJS', mark: FORMAJS_MARK, subtitle: 'GATEWASM CONSOLE' },
      items: sidebarItems,
      footer: [
        { id: 'refresh', label: 'Refresh', icon: iconSvg('refresh-stroke') },
        { id: 'signout', label: 'Sign Out', icon: iconSvg('signout'), danger: true },
      ],
      activeView: () => view(),
      onNavigate: (id: string) => {
        if (id === 'refresh') { loadTenants(); return; }
        if (id === 'signout') { signout(); return; }
        if (id === 'tenants') { backToTenants(); return; }
        if (['overview', 'users', 'integrate', 'hosted-page', 'settings'].includes(id)) {
          setView(id as any);
          return;
        }
      },
      env: detectEnv(),
    }),
    h('div', { class: 'shell-stage' },
      Topbar({
        breadcrumbs: () => {
          const currentView = view();
          const tenant = selectedTenant();
          const crumbs: { label: string; onClick?: () => void }[] = [{ label: 'Console' }];
          if (tenant) crumbs.push({ label: tenant.name, onClick: () => backToTenants() });
          if (currentView !== 'tenants' && currentView !== 'wizard') {
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
        actions: () => selectedTenant() ? TopbarTenantContext() : null,
      }),
      h('main', { class: 'main page-enter' },
        LoadingBar(() => loading()),
        createSwitch(
          () => view(),
          [
            { match: 'tenants', render: () => TenantsList() },
            { match: 'wizard', render: () => WizardFlow() },
            { match: 'overview', render: () => OverviewView() },
            { match: 'users', render: () => UsersView() },
            { match: 'integrate', render: () => IntegrateView() },
            { match: 'hosted-page', render: () => HostedPageView() },
            { match: 'settings', render: () => SettingsView() },
          ],
        ),
      ),
    ),
    ToastView(toast),
  );
}
