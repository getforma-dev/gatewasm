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
  try {
    await auth.silentRefresh();
    await store.loadProfile();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/t/' + (config?.slug || 'default') + '/login';
    return;
  }
  mount(() => TenantDashboard(store, toast, async () => {
    auth.stopAutoRefresh();
    await auth.logout();
    window.location.href = '/t/' + (config?.slug || 'default') + '/login';
  }), '#app');
}

boot();
