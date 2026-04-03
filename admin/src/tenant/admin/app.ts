/**
 * PLANE 3 — Tenant Admin
 * USER: Authenticated Member managing a specific Tenant
 * SERVER DATA: window.__TENANT__ { slug }
 * ENTRY POINT: tenant-admin.js
 * ROUTE: /t/:slug/admin
 */
import { mount } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import { TenantAdmin } from './TenantAdmin';
import { createTenantAdminStore } from './store';

const config = (window as any).__TENANT__;

const auth = createAuth({ serverUrl: '', tenant: config?.slug || 'default' });
const api = createApi(auth);
const toast = createToast();
const store = createTenantAdminStore(api, toast);

async function boot() {
  try {
    await auth.silentRefresh();
    await auth.ensureRole('admin');
    await store.loadStats();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/t/' + (config?.slug || 'default') + '/login';
    return;
  }
  mount(() => TenantAdmin(store, toast, async () => {
    auth.stopAutoRefresh();
    await auth.logout();
    window.location.href = '/t/' + (config?.slug || 'default') + '/login';
  }), '#app');
}

boot();
