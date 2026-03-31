import { mount } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import { TenantDashboard } from './TenantDashboard';
import { createTenantDashboardStore } from './store';

const config = (window as any).__TENANT__;

const auth = createAuth({ serverUrl: config?.server_url || window.location.origin, tenant: config?.slug || 'default' });
const api = createApi(auth);
const toast = createToast();
const store = createTenantDashboardStore(api, toast);

async function boot() {
  const slug = config?.slug || 'default';
  const loopKey = 'gatewasm_dashboard_boot_' + slug;

  try {
    await auth.silentRefresh();
    await store.loadProfile();
    auth.startAutoRefresh();
    // Success — clear any loop marker
    try { sessionStorage.removeItem(loopKey); } catch {}
  } catch {
    // Check if we already tried and failed — prevent redirect loop
    try {
      if (sessionStorage.getItem(loopKey)) {
        sessionStorage.removeItem(loopKey);
        document.getElementById('app')!.innerHTML =
          '<div style="display:flex;align-items:center;justify-content:center;min-height:100vh;color:#e8e6e3;font-size:0.85rem;flex-direction:column;gap:1rem;">' +
          '<p>Unable to load dashboard. Please sign in again.</p>' +
          '<a href="/t/' + slug + '/login" style="color:#6366f1;">Go to login</a></div>';
        return;
      }
      sessionStorage.setItem(loopKey, '1');
    } catch {}
    window.location.href = '/t/' + slug + '/login';
    return;
  }
  mount(() => TenantDashboard(store, toast, async () => {
    auth.stopAutoRefresh();
    await auth.logout();
    window.location.href = '/t/' + slug + '/login';
  }), '#app');
}

boot();
