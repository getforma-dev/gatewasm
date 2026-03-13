import { mount } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import { OwnerApp } from './OwnerApp';
import { createOwnerStore } from './store';

const serverUrl = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const auth = createAuth({ serverUrl, tenant: 'platform' });
const api = createApi(auth);
const toast = createToast();
const ownerStore = createOwnerStore(api, toast);

async function boot() {
  try {
    await auth.silentRefresh();
    await auth.ensureRole('admin');
    await ownerStore.refreshAll();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/platform/login';
    return;
  }

  mount(() => OwnerApp(ownerStore, toast, async () => {
    auth.stopAutoRefresh();
    await auth.logout();
    window.location.href = '/platform/login';
  }), '#app');
}

boot();
