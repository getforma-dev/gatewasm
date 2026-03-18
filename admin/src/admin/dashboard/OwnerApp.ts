// ---------------------------------------------------------------------------
// OwnerApp.ts — Owner dashboard app shell with sidebar + view switching
// ---------------------------------------------------------------------------

import { h, createSwitch } from 'formajs';
import { detectEnv } from '../../core/utils';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/Toast';
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

// -- SVG icons (Lucide, stroke-width 1.5) ------------------------------------

const OVERVIEW_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>';
const CUSTOMERS_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>';
const BILLING_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="20" height="14" rx="2"/><line x1="2" y1="10" x2="22" y2="10"/></svg>';
const TIMELINE_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>';
const INVITES_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/></svg>';
const SECURITY_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>';
const REFRESH_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>';
const SIGNOUT_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>';

// -- Environment badge -------------------------------------------------------

// -- OwnerApp component ------------------------------------------------------

export function OwnerApp(ownerStore: OwnerStore, toast: ToastController, onSignout: () => void) {
  const { store, navigate, refreshAll } = ownerStore;

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Forma Auth', mark: '\u25C7', subtitle: 'CONTROL PLANE' },
      items: [
        { id: 'overview', label: 'Overview', icon: OVERVIEW_ICON },
        { id: 'customers', label: 'Customers', icon: CUSTOMERS_ICON },
        { id: 'billing', label: 'Billing', icon: BILLING_ICON },
        { id: 'timeline', label: 'Timeline', icon: TIMELINE_ICON },
        { id: 'invites', label: 'Invites', icon: INVITES_ICON },
        { id: 'security', label: 'Security', icon: SECURITY_ICON },
      ],
      footer: [
        { id: 'refresh', label: 'Refresh', icon: REFRESH_ICON },
        { id: 'signout', label: 'Sign Out', icon: SIGNOUT_ICON, danger: true },
      ],
      activeView: () => store.view,
      onNavigate: (id: string) => {
        if (id === 'refresh') { refreshAll(); return; }
        if (id === 'signout') { onSignout(); return; }
        navigate(id as OwnerView);
      },
      env: detectEnv(),
    }),
    h('main', { class: 'main' },
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
    CustomerDrawer(ownerStore),
    ToastView(toast),
  );
}
