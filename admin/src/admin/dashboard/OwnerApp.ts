// ---------------------------------------------------------------------------
// OwnerApp.ts — Owner dashboard app shell with sidebar + view switching
// ---------------------------------------------------------------------------

import { h, createSwitch } from 'formajs';
import { detectEnv } from '../../core/utils';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Topbar } from '../../core/components/ui/Topbar';
import { Icon } from '../../core/components/ui/Icon';
import { FAuthMark } from '../../platform/console/helpers';
import { OverviewView } from './views/OverviewView';
import { CustomersView } from './views/CustomersView';
import { CustomerDrawer } from './views/CustomerDrawer';
import { BillingView } from './views/BillingView';
import { TimelineView } from './views/TimelineView';
import { InvitesView } from './views/InvitesView';
import { SecurityView } from './views/SecurityView';
import type { OwnerStore } from './store';
import type { ToastController } from '../../core/hooks/createToast';
import type { OwnerView } from '../../core/types';

/** Render an Icon component and return its outerHTML for Sidebar innerHTML usage. */
function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

// -- OwnerApp component ------------------------------------------------------

export function OwnerApp(ownerStore: OwnerStore, toast: ToastController, onSignout: () => void) {
  const { store, navigate, refreshAll } = ownerStore;

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Forma Auth', mark: FAuthMark(20), subtitle: 'CONTROL PLANE' },
      items: [
        { id: 'overview', label: 'Overview', icon: iconSvg('overview-grid') },
        { id: 'customers', label: 'Customers', icon: iconSvg('customers') },
        { id: 'billing', label: 'Billing', icon: iconSvg('billing-card') },
        { id: 'timeline', label: 'Timeline', icon: iconSvg('timeline') },
        { id: 'invites', label: 'Invites', icon: iconSvg('invites') },
        { id: 'security', label: 'Security', icon: iconSvg('security') },
      ],
      footer: [
        { id: 'refresh', label: 'Refresh', icon: iconSvg('refresh-stroke') },
        { id: 'signout', label: 'Sign Out', icon: iconSvg('signout'), danger: true },
      ],
      activeView: () => store.view,
      onNavigate: (id: string) => {
        if (id === 'refresh') { refreshAll(); return; }
        if (id === 'signout') { onSignout(); return; }
        navigate(id as OwnerView);
      },
      env: detectEnv(),
    }),
    h('div', { class: 'shell-stage' },
      Topbar({
        breadcrumbs: () => {
          const v = store.view;
          const labels: Record<string, string> = {
            overview: 'Overview', customers: 'Customers', billing: 'Billing',
            timeline: 'Timeline', invites: 'Invites', security: 'Security',
          };
          return [{ label: 'Control Plane' }, { label: labels[v] || v }];
        },
      }),
      h('main', { class: 'main page-enter' },
        LoadingBar(() => store.loading),
        createSwitch(
          () => store.view,
          [
            { match: 'overview', render: () => OverviewView(ownerStore) },
            { match: 'customers', render: () => CustomersView(ownerStore) },
            { match: 'billing', render: () => BillingView(ownerStore) },
            { match: 'timeline', render: () => TimelineView(ownerStore) },
            { match: 'invites', render: () => InvitesView(ownerStore) },
            { match: 'security', render: () => SecurityView() },
          ],
        ),
      ),
    ),
    CustomerDrawer(ownerStore),
    ToastView(toast),
  );
}
