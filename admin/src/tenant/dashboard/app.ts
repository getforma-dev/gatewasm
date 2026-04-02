import { mount } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import { TenantDashboard } from './TenantDashboard';
import { createTenantDashboardStore } from './store';

const config = (window as any).__TENANT__;

const auth = createAuth({ serverUrl: '', tenant: config?.slug || 'default' });
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
        const app = document.getElementById('app')!;
        app.textContent = '';
        const wrapper = document.createElement('div');
        wrapper.className = 'tl-page';
        const card = document.createElement('div');
        card.className = 'tl-card';
        const msg = document.createElement('p');
        msg.textContent = 'Unable to load dashboard. Please sign in again.';
        const link = document.createElement('a');
        link.href = '/t/' + slug + '/login';
        link.textContent = 'Go to login';
        link.className = 'tl-link';
        card.appendChild(msg);
        card.appendChild(link);
        wrapper.appendChild(card);
        app.appendChild(wrapper);
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
