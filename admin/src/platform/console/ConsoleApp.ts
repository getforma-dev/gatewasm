import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Topbar } from '../../core/components/ui/Topbar';
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

const FORMAJS_MARK = '<svg width="64" height="64" viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg"><rect width="64" height="64" rx="12" fill="#282828"/><g transform="translate(12, 10)"><rect x="8" y="4" width="7" height="36" rx="1.5" fill="#ebdbb2"/><rect x="8" y="4" width="26" height="8" rx="1.5" fill="#83a598"/><rect x="8" y="18" width="20" height="7" rx="1.5" fill="#8ec07c"/><line x1="34" y1="5" x2="34" y2="11" stroke="#39FF14" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/><line x1="28" y1="19" x2="28" y2="24" stroke="#39FF14" stroke-width="1.5" stroke-linecap="round" opacity="0.4"/><rect x="36" y="5" width="3" height="3" rx="1" fill="#39FF14" opacity="0.8"/><rect x="30" y="19" width="2.5" height="2.5" rx="0.8" fill="#39FF14" opacity="0.6"/></g></svg>';

// SVG icons — Lucide, stroke-width 1.5
const ICONS = {
  tenants: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>',
  overview: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>',
  users: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  integrate: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>',
  hostedPage: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/></svg>',
  settings: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>',
  refresh: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>',
  signout: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>',
};

function TenantsList() {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Your Tenants'),
    ),
    h('div', { class: 'user-list' },
      () => tenants().map(t =>
        h('div', {
          class: 'user-row',
          key: t.id,
        },
          h('div', {
            class: 'user-row-summary',
            onClick: () => selectTenant(t),
          },
            h('div', { class: 'user-email' }, t.name),
            h('div', { class: 'user-meta text-muted' },
              '/t/' + t.slug + ' — ' + t.plan + ' — ' + t.status,
            ),
          ),
        ),
      ),
    ),
  );
}

export function ConsoleApp() {
  const isOnboarded = () => {
    const t = selectedTenant();
    return t != null && t.onboarding_completed;
  };

  const sidebarItems = () => {
    const items: { id: string; label: string; icon: string }[] = [
      { id: 'tenants', label: 'Tenants', icon: ICONS.tenants },
    ];
    if (isOnboarded()) {
      items.push(
        { id: 'overview', label: 'Overview', icon: ICONS.overview },
        { id: 'users', label: 'Users', icon: ICONS.users },
        { id: 'integrate', label: 'Integrate', icon: ICONS.integrate },
        { id: 'hosted-page', label: 'Hosted Page', icon: ICONS.hostedPage },
        { id: 'settings', label: 'Settings', icon: ICONS.settings },
      );
    }
    return items;
  };

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'FormaJS', mark: FORMAJS_MARK, subtitle: 'GATEWASM CONSOLE' },
      items: sidebarItems,
      footer: [
        { id: 'refresh', label: 'Refresh', icon: ICONS.refresh },
        { id: 'signout', label: 'Sign Out', icon: ICONS.signout, danger: true },
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
    }),
    Topbar({
      breadcrumbs: () => {
        const v = view();
        const t = selectedTenant();
        const crumbs: { label: string; onClick?: () => void }[] = [{ label: 'Console' }];
        if (t) crumbs.push({ label: t.name, onClick: () => backToTenants() });
        if (v !== 'tenants' && v !== 'wizard') {
          const labels: Record<string, string> = {
            overview: 'Overview', users: 'Users', integrate: 'Integrate',
            'hosted-page': 'Hosted Page', settings: 'Settings',
          };
          crumbs.push({ label: labels[v] || v });
        }
        return crumbs;
      },
    }),
    h('main', { class: 'content page-enter' },
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
    ToastView(toast),
  );
}
