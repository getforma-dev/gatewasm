import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Topbar } from '../../core/components/ui/Topbar';
import { Icon } from '../../core/components/ui/Icon';
import { ICONS } from '../../core/components/ui/icon-paths';
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

/** Render an Icon component and return its outerHTML for Sidebar innerHTML usage. */
function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

/** Brand mark SVG — uses the 64x64 viewBox variant stored in the icon registry. */
const FORMAJS_MARK = '<svg width="64" height="64" viewBox="0 0 64 64" xmlns="http://www.w3.org/2000/svg">' + ICONS['formajs-mark'] + '</svg>';

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
