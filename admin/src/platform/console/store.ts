import { createStore, batch } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import type { Tenant, TenantDetail, TenantStats, ConsoleView } from '../../core/types';

// ── Module-level singletons (shared across ConsoleApp island) ───────────

const serverUrl = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const auth = createAuth({ serverUrl, tenant: 'platform' });
const api = createApi(auth);
export const toast = createToast();

// ── Console store ───────────────────────────────────────────────────────

const [store, setStore] = createStore({
  view: 'tenants' as ConsoleView,
  tenants: [] as Tenant[],
  tenantStats: {} as Record<string, TenantStats | null>,
  selectedTenant: null as TenantDetail | null,
  selectedTenantStats: null as TenantStats | null,
  loading: false,
  quickstartDismissed: localStorage.getItem('forma_console_quickstart_dismissed') === '1',
});

async function loadTenants() {
  setStore({ loading: true });
  try {
    const data = await api.get<{ tenants: Tenant[] }>('/platform/self-serve/tenants');
    setStore({ tenants: data.tenants || [], loading: false });
  } catch (e) {
    setStore({ loading: false });
    toast.show(e instanceof Error ? e.message : 'Failed to load tenants', 'error');
  }
}

async function loadTenantStats() {
  const tenants = store.tenants;
  const statsMap: Record<string, TenantStats | null> = {};

  const promises = tenants.map(async (t: Tenant) => {
    try {
      const s = await api.get<TenantStats>(
        '/platform/self-serve/tenants/' + encodeURIComponent(t.id) + '/stats',
      );
      statsMap[t.id] = s;
    } catch {
      statsMap[t.id] = null;
    }
  });

  await Promise.all(promises);
  setStore({ tenantStats: statsMap });
}

async function loadTenantDetail(tenantId: string) {
  if (!tenantId) return;
  setStore({ loading: true });
  try {
    const encodedId = encodeURIComponent(tenantId);
    const [detail, stats] = await Promise.all([
      api.get<TenantDetail>('/platform/self-serve/tenants/' + encodedId),
      api.get<TenantStats>('/platform/self-serve/tenants/' + encodedId + '/stats'),
    ]);
    batch(() => {
      setStore({
        selectedTenant: detail,
        selectedTenantStats: stats,
        view: 'detail',
        loading: false,
      });
    });
  } catch (e) {
    setStore({ loading: false });
    toast.show(e instanceof Error ? e.message : 'Failed to load tenant', 'error');
  }
}

async function createTenant(name: string, slug: string) {
  setStore({ loading: true });
  try {
    await api.post<{ tenant: Tenant }>('/platform/self-serve/tenants', { name, slug });
    await loadTenants();
    await loadTenantStats();
    batch(() => {
      setStore({ view: 'tenants', loading: false });
    });
    toast.show('Tenant "' + name + '" created successfully', 'success');
  } catch (e) {
    setStore({ loading: false });
    toast.show(e instanceof Error ? e.message : 'Failed to create tenant', 'error');
  }
}

async function configureTenant(tenantId: string, config: Record<string, unknown>) {
  setStore({ loading: true });
  try {
    await api.patch('/platform/self-serve/tenants/' + encodeURIComponent(tenantId), config);
    await loadTenantDetail(tenantId);
    toast.show('Configuration saved', 'success');
  } catch (e) {
    setStore({ loading: false });
    toast.show(e instanceof Error ? e.message : 'Failed to save configuration', 'error');
  }
}

async function rotateTenantKey(tenantId: string) {
  try {
    const result = await api.post<{ api_key?: string }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/rotate-key',
    );
    if (result.api_key) {
      const current = store.selectedTenant;
      if (current && current.id === tenantId) {
        setStore({
          selectedTenant: { ...current, management_api_key: result.api_key },
        });
      }
      toast.show("API key rotated \u2014 save it now, it won't be shown again", 'success');
    } else {
      toast.show('Failed to rotate key', 'error');
    }
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to rotate key', 'error');
  }
}

async function deleteTenant(tenantId: string) {
  setStore({ loading: true });
  try {
    await api.del('/platform/self-serve/tenants/' + encodeURIComponent(tenantId));
    await loadTenants();
    await loadTenantStats();
    batch(() => {
      setStore({
        view: 'tenants',
        selectedTenant: null,
        selectedTenantStats: null,
        loading: false,
      });
    });
    toast.show('Tenant deleted', 'success');
  } catch (e) {
    setStore({ loading: false });
    toast.show(e instanceof Error ? e.message : 'Failed to delete tenant', 'error');
  }
}

async function ensureData() {
  setStore({ loading: true });
  try {
    await loadTenants();
    await loadTenantStats();
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load data', 'error');
  } finally {
    setStore({ loading: false });
  }
}

function navigate(view: ConsoleView) {
  setStore({ view });
}

function dismissQuickstart() {
  setStore({ quickstartDismissed: true });
  try { localStorage.setItem('forma_console_quickstart_dismissed', '1'); } catch {}
}

export const consoleStore = {
  store, setStore, loadTenants, loadTenantStats, loadTenantDetail,
  createTenant, configureTenant, rotateTenantKey,
  deleteTenant, ensureData, navigate, dismissQuickstart,
};

export type ConsoleStore = typeof consoleStore;

// ── Signout ─────────────────────────────────────────────────────────────

export async function signout() {
  auth.stopAutoRefresh();
  await auth.logout();
  window.location.href = '/platform/login';
}

// ── Init (called from app.ts) ───────────────────────────────────────────

export async function init() {
  try {
    await auth.silentRefresh();
    await consoleStore.ensureData();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/platform/login';
    // Re-throw so callers know init failed (prevents mount during redirect)
    throw new Error('auth_redirect');
  }
}
