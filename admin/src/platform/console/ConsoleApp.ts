import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/Toast';
import { TenantsView } from './views/TenantsView';
import { TenantDetail } from './views/TenantDetail';
import { CreateTenant } from './views/CreateTenant';
import { consoleStore, toast, signout } from './store';

// SVG icons — Lucide, stroke-width 1.5
const TENANTS_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>';
const PLUS_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>';
const REFRESH_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>';
const SIGNOUT_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>';

export function ConsoleApp() {
  const { store, navigate } = consoleStore;

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Forma Auth', mark: '\u25C7', subtitle: 'PLATFORM CONSOLE' },
      items: [
        { id: 'tenants', label: 'Tenants', icon: TENANTS_ICON },
        { id: 'create', label: 'New Tenant', icon: PLUS_ICON },
      ],
      footer: [
        { id: 'refresh', label: 'Refresh', icon: REFRESH_ICON },
        { id: 'signout', label: 'Sign Out', icon: SIGNOUT_ICON, danger: true },
      ],
      activeView: () => store.view,
      onNavigate: (id: string) => {
        if (id === 'refresh') { consoleStore.ensureData(); return; }
        if (id === 'signout') { signout(); return; }
        navigate(id as any);
      },
    }),
    h('main', { class: 'main' },
      LoadingBar(() => store.loading),
      createSwitch(
        () => store.view,
        [
          { match: 'tenants', render: () => TenantsView(consoleStore) },
          { match: 'detail', render: () => TenantDetail(consoleStore) },
          { match: 'create', render: () => CreateTenant(consoleStore) },
        ],
      ),
    ),
    ToastView(toast),
  );
}
