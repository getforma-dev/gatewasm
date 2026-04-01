import { h, createSwitch, createShow } from 'formajs';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/Toast';
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
      brand: { name: 'Forma Auth', mark: '\u25C7', subtitle: 'SELF-SERVE CONSOLE' },
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
    h('main', { class: 'main' },
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
