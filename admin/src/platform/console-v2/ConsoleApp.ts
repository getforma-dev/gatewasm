import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/Toast';
import { WizardFlow } from './wizard/WizardFlow';
import {
  view, loading, toast, backToTenants, loadTenants, signout, selectedTenant,
} from './store';

// SVG icons — Lucide, stroke-width 1.5
const TENANTS_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>';
const REFRESH_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>';
const SIGNOUT_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>';

function TenantsStub() {
  return h('p', { style: 'padding:2rem;color:var(--text-2)' }, 'Tenants list — Phase 2');
}

function DetailStub() {
  return h('p', { style: 'padding:2rem;color:var(--text-2)' }, 'Tenant detail — Phase 2');
}

export function ConsoleApp() {
  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Forma Auth', mark: '\u25C7', subtitle: 'SELF-SERVE CONSOLE' },
      items: [
        { id: 'tenants', label: 'Tenants', icon: TENANTS_ICON },
      ],
      footer: [
        { id: 'refresh', label: 'Refresh', icon: REFRESH_ICON },
        { id: 'signout', label: 'Sign Out', icon: SIGNOUT_ICON, danger: true },
      ],
      activeView: () => view(),
      onNavigate: (id: string) => {
        if (id === 'refresh') { loadTenants(); return; }
        if (id === 'signout') { signout(); return; }
        if (id === 'tenants') {
          const t = selectedTenant();
          if (t && !t.onboarding_completed) return; // stay on wizard
          backToTenants();
          return;
        }
      },
    }),
    h('main', { class: 'main' },
      LoadingBar(() => loading()),
      createSwitch(
        () => view(),
        [
          { match: 'tenants', render: () => TenantsStub() },
          { match: 'wizard', render: () => WizardFlow() },
          { match: 'detail', render: () => DetailStub() },
        ],
      ),
    ),
    ToastView(toast),
  );
}
