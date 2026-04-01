import { createSignal, batch } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';

// ── Module-level singletons (shared across ConsoleApp island) ───────────

const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const auth = createAuth({ serverUrl, tenant: 'platform' });
const api = createApi(auth);
export const toast = createToast();

// ── Types ───────────────────────────────────────────────────────────────

export interface Tenant {
  id: string;
  name: string;
  slug: string;
  plan: string;
  status: string;
  onboarding_completed: boolean;
  onboarding_step: number;
  preferred_tab: string | null;
  primary_color: string | null;
  login_redirect_url: string | null;
  management_api_key?: string;
  created_at: string;
}

export interface TenantStats {
  total_users: number;
  active_sessions: number;
  logins_24h: number;
  latest_user: { email: string; provider: string; created_at: string } | null;
}

export interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  is_locked: boolean;
  provider: string | null;
  roles: string[];
  created_at: string;
}

export interface HostedPageData {
  headline: string;
  body: string;
  cta_text: string;
  cta_link: string;
  custom_html: string | null;
  updated_at: string | null;
}

// ── State ───────────────────────────────────────────────────────────────

export const [tenants, setTenants] = createSignal<Tenant[]>([]);
export const [selectedTenant, setSelectedTenant] = createSignal<Tenant | null>(null);
export const [selectedTenantStats, setSelectedTenantStats] = createSignal<TenantStats | null>(null);
export const [loading, setLoading] = createSignal(false);
export const [view, setView] = createSignal<'tenants' | 'wizard' | 'overview' | 'users' | 'hosted-page' | 'integrate' | 'settings'>('tenants');
export const [users, setUsers] = createSignal<TenantUser[]>([]);
export const [usersLoading, setUsersLoading] = createSignal(false);
export const [hostedPage, setHostedPage] = createSignal<HostedPageData | null>(null);

// ── API Functions ───────────────────────────────────────────────────────

export async function loadTenants(): Promise<void> {
  setLoading(true);
  try {
    const data = await api.get<{ tenants: Tenant[] }>('/platform/self-serve/tenants');
    setTenants(data.tenants || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load tenants', 'error');
  } finally {
    setLoading(false);
  }
}

export async function loadTenantDetail(id: string): Promise<void> {
  if (!id) return;
  setLoading(true);
  try {
    const encodedId = encodeURIComponent(id);
    const [tenant, stats] = await Promise.all([
      api.get<Tenant>('/platform/self-serve/tenants/' + encodedId),
      api.get<TenantStats>('/platform/self-serve/tenants/' + encodedId + '/stats'),
    ]);
    batch(() => {
      setSelectedTenant(tenant);
      setSelectedTenantStats(stats);
      setLoading(false);
    });
  } catch (e) {
    setLoading(false);
    toast.show(e instanceof Error ? e.message : 'Failed to load tenant', 'error');
  }
}

export async function updateTenant(
  id: string,
  fields: Partial<Pick<Tenant, 'name' | 'primary_color' | 'login_redirect_url' | 'onboarding_completed' | 'onboarding_step' | 'preferred_tab'>>,
): Promise<void> {
  try {
    const updated = await api.patch<Tenant>(
      '/platform/self-serve/tenants/' + encodeURIComponent(id),
      fields,
    );
    // Refresh selected tenant if it matches
    const current = selectedTenant();
    if (current && current.id === id) {
      setSelectedTenant(updated);
    }
    // Refresh the tenant list so the card reflects changes
    const list = tenants();
    setTenants(list.map((t) => (t.id === id ? { ...t, ...updated } : t)));
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update tenant', 'error');
  }
}

export async function pollTenantStats(id: string): Promise<TenantStats> {
  return api.get<TenantStats>(
    '/platform/self-serve/tenants/' + encodeURIComponent(id) + '/stats',
  );
}

export async function loadUsers(tenantId: string): Promise<void> {
  setUsersLoading(true);
  try {
    const data = await api.get<{ users: TenantUser[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users',
    );
    setUsers(data.users || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load users', 'error');
  } finally {
    setUsersLoading(false);
  }
}

export async function blockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/block', {});
    toast.show('User blocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to block user', 'error');
  }
}

export async function unblockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/unblock', {});
    toast.show('User unblocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to unblock user', 'error');
  }
}

export async function revokeUserSessions(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/revoke-sessions', {});
    toast.show('Sessions revoked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke sessions', 'error');
  }
}

export async function loadHostedPage(tenantId: string): Promise<void> {
  try {
    const data = await api.get<HostedPageData>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
    );
    setHostedPage(data);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load hosted page', 'error');
  }
}

export async function saveHostedPage(tenantId: string, data: Partial<HostedPageData>): Promise<void> {
  try {
    await api.put(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
      data,
    );
    toast.show('Page saved', 'success');
    await loadHostedPage(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to save page', 'error');
  }
}

// ── Navigation ──────────────────────────────────────────────────────────

export function selectTenant(tenant: Tenant): void {
  batch(() => {
    setSelectedTenant(tenant);
    setSelectedTenantStats(null);
    if (tenant.onboarding_completed) {
      const tab = tenant.preferred_tab;
      if (tab === 'hosted-page') setView('hosted-page');
      else if (tab === 'integrate') setView('integrate');
      else setView('overview');
    } else {
      setView('wizard');
    }
  });
}

export function backToTenants(): void {
  batch(() => {
    setSelectedTenant(null);
    setSelectedTenantStats(null);
    setView('tenants');
  });
}

// ── Lifecycle ───────────────────────────────────────────────────────────

export async function init(): Promise<void> {
  try {
    await auth.silentRefresh();
    await loadTenants();
    auth.startAutoRefresh();

    // Auto-select if there is exactly one unboarded tenant
    const list = tenants();
    const unboarded = list.filter((t) => !t.onboarding_completed);
    if (unboarded.length === 1) {
      selectTenant(unboarded[0]);
    }
  } catch {
    window.location.href = '/platform/login';
    throw new Error('auth_redirect');
  }
}

export async function signout(): Promise<void> {
  auth.stopAutoRefresh();
  await auth.logout();
  window.location.href = '/platform/login';
}
