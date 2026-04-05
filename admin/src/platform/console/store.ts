import { createSignal, batch } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { createToast } from '../../core/hooks/createToast';
import type { IntrospectData } from '../../core/types';

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
  failed_logins_24h: number;
  window: OverviewWindow;
  logins_in_window: number;
  failed_logins_in_window: number;
  series: TenantStatsSeriesPoint[];
  activity: TenantActivityItem[];
  providers: Record<string, number>;
  latest_user: { email: string; provider: string; created_at: string } | null;
}

export interface TenantStatsSeriesPoint {
  bucket_start: string;
  label: string;
  logins: number;
  failed_logins: number;
  sessions_started: number;
}

export interface TenantActivityItem {
  id: string;
  kind: 'user_created' | 'login_succeeded' | 'login_failed' | 'session_started';
  email: string;
  provider: string | null;
  description: string;
  created_at: string;
}

export interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  is_locked: boolean;
  provider: string | null;
  roles: string[];
  active_sessions: number;
  created_at: string;
}

export interface UserSession {
  id: string;
  created_at: string;
  expires_at: string;
  auth_method: string;
  auth_provider: string | null;
  last_activity_at: string | null;
  login_ip: string | null;
  mfa_verified: boolean;
}

export interface HostedPageData {
  headline: string;
  body: string;
  cta_text: string;
  cta_link: string;
  custom_html: string | null;
  updated_at: string | null;
}

export interface PlatformUser {
  id: string;
  email: string;
  name: string | null;
  avatar: string | null;
  email_verified: boolean;
  roles: string[];
  auth_method: string;
  mfa_verified: boolean;
  mfa_enabled: boolean;
}

export type ConsoleView =
  | 'tenants'
  | 'overview'
  | 'users'
  | 'hosted-page'
  | 'integrate'
  | 'settings';

export type TenantWorkspaceView = Exclude<ConsoleView, 'tenants'>;
export type IntegrateSection = 'guide' | 'session-inspector';
export type OverviewWindow = '24h' | '7d' | '30d';

interface PersistedConsoleWorkspace {
  tenantId: string | null;
  view: ConsoleView;
  integrateSection: IntegrateSection;
}

// ── State ───────────────────────────────────────────────────────────────

export const [tenants, setTenants] = createSignal<Tenant[]>([]);
export const [selectedTenant, setSelectedTenant] = createSignal<Tenant | null>(null);
export const [selectedTenantStats, setSelectedTenantStats] = createSignal<TenantStats | null>(null);
export const [loading, setLoading] = createSignal(false);
export const [view, setView] = createSignal<ConsoleView>('tenants');
export const [users, setUsers] = createSignal<TenantUser[]>([]);
export const [usersLoading, setUsersLoading] = createSignal(false);
export const [hostedPage, setHostedPage] = createSignal<HostedPageData | null>(null);
export const [introspectData, setIntrospectData] = createSignal<IntrospectData | null>(null);
export const [introspectToken, setIntrospectToken] = createSignal<string | null>(null);
export const [introspectUnavailable, setIntrospectUnavailable] = createSignal(false);
export const [userSessions, setUserSessions] = createSignal<UserSession[]>([]);
export const [userSessionsLoading, setUserSessionsLoading] = createSignal(false);
export const [integrateSection, setIntegrateSectionSignal] = createSignal<IntegrateSection>('guide');
export const [overviewWindow, setOverviewWindowSignal] = createSignal<OverviewWindow>('7d');
export const [currentUser, setCurrentUser] = createSignal<PlatformUser | null>(null);

let tenantDetailRequestSeq = 0;
let usersRequestSeq = 0;
let hostedPageRequestSeq = 0;
let introspectRequestSeq = 0;
let sessionRequestSeq = 0;
let stopOverviewRefresh: (() => void) | null = null;
const WORKSPACE_STORAGE_PREFIX = 'forma-console-workspace:';

function withTransientTenantFields(nextTenant: Tenant, existingTenant?: Tenant | null): Tenant {
  if (nextTenant.management_api_key || !existingTenant?.management_api_key) {
    return nextTenant;
  }

  return { ...nextTenant, management_api_key: existingTenant.management_api_key };
}

function findKnownTenant(id: string): Tenant | null {
  const current = selectedTenant();
  if (current?.id === id) {
    return current;
  }

  return tenants().find((tenant) => tenant.id === id) || null;
}

function resetTenantScopedState(): void {
  batch(() => {
    setSelectedTenantStats(null);
    setUsers([]);
    setUsersLoading(false);
    setHostedPage(null);
    setIntrospectData(null);
    setIntrospectToken(null);
    setIntrospectUnavailable(false);
    setIntegrateSectionSignal('guide');
    setOverviewWindowSignal('7d');
  });
}

function workspaceStorageKey(): string | null {
  const user = currentUser();
  if (!user || typeof window === 'undefined') {
    return null;
  }
  return WORKSPACE_STORAGE_PREFIX + user.id;
}

function readWorkspaceState(): PersistedConsoleWorkspace | null {
  const storageKey = workspaceStorageKey();
  if (!storageKey) {
    return null;
  }

  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw) as Partial<PersistedConsoleWorkspace>;
    const nextView = parsed.view;
    const nextSection = parsed.integrateSection;
    if (
      typeof nextView !== 'string' ||
      !['tenants', 'overview', 'users', 'hosted-page', 'integrate', 'settings'].includes(nextView)
    ) {
      return null;
    }
    return {
      tenantId: typeof parsed.tenantId === 'string' ? parsed.tenantId : null,
      view: nextView as ConsoleView,
      integrateSection: nextSection === 'session-inspector' ? 'session-inspector' : 'guide',
    };
  } catch {
    return null;
  }
}

function persistWorkspaceState(): void {
  const storageKey = workspaceStorageKey();
  if (!storageKey) {
    return;
  }

  try {
    const snapshot: PersistedConsoleWorkspace = {
      tenantId: selectedTenant()?.id || null,
      view: view(),
      integrateSection: integrateSection(),
    };
    window.localStorage.setItem(storageKey, JSON.stringify(snapshot));
  } catch {
    // Ignore storage failures.
  }
}

async function restoreWorkspaceState(): Promise<void> {
  const snapshot = readWorkspaceState();
  if (!snapshot || !snapshot.tenantId || snapshot.view === 'tenants') {
    persistWorkspaceState();
    return;
  }

  const tenant = tenants().find((item) => item.id === snapshot.tenantId);
  if (!tenant) {
    persistWorkspaceState();
    return;
  }

  const restoredView = snapshot.view as TenantWorkspaceView;

  batch(() => {
    setSelectedTenant(tenant);
    resetTenantScopedState();
    setIntegrateSectionSignal(snapshot.integrateSection);
    setView(restoredView);
  });

  persistWorkspaceState();
  await hydrateTenantView(restoredView, tenant);
}

// ── API Functions ───────────────────────────────────────────────────────

export async function loadTenants(): Promise<void> {
  setLoading(true);
  try {
    const data = await api.get<{ tenants: Tenant[] }>('/platform/self-serve/tenants');
    const nextTenants = (data.tenants || []).map((tenant) =>
      withTransientTenantFields(tenant, findKnownTenant(tenant.id)),
    );
    setTenants(nextTenants);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load tenants', 'error');
  } finally {
    setLoading(false);
  }
}

export async function loadCurrentUser(): Promise<void> {
  try {
    const data = await api.get<PlatformUser>('/auth/me');
    setCurrentUser(data);
  } catch (e) {
    setCurrentUser(null);
    toast.show(e instanceof Error ? e.message : 'Failed to load account profile', 'error');
  }
}

export async function loadTenantDetail(id: string, window: OverviewWindow = overviewWindow()): Promise<void> {
  if (!id) return;
  const requestSeq = ++tenantDetailRequestSeq;
  setLoading(true);
  try {
    const encodedId = encodeURIComponent(id);
    const [tenant, stats] = await Promise.all([
      api.get<Tenant>('/platform/self-serve/tenants/' + encodedId),
      api.get<TenantStats>('/platform/self-serve/tenants/' + encodedId + '/stats', { window }),
    ]);
    if (requestSeq !== tenantDetailRequestSeq || selectedTenant()?.id !== id) {
      return;
    }
    batch(() => {
      setSelectedTenant(withTransientTenantFields(tenant, findKnownTenant(id)));
      setSelectedTenantStats(stats);
      setLoading(false);
    });
  } catch (e) {
    if (requestSeq === tenantDetailRequestSeq && selectedTenant()?.id === id) {
      setLoading(false);
      toast.show(e instanceof Error ? e.message : 'Failed to load tenant', 'error');
    }
  }
}

export async function updateTenant(
  id: string,
  fields: Partial<Pick<Tenant, 'name' | 'slug' | 'primary_color' | 'login_redirect_url' | 'onboarding_completed' | 'onboarding_step' | 'preferred_tab'>>,
): Promise<Tenant | null> {
  try {
    const updatedTenant = await api.patch<Tenant>(
      '/platform/self-serve/tenants/' + encodeURIComponent(id),
      fields,
    );
    const updated = withTransientTenantFields(updatedTenant, findKnownTenant(id));
    // Refresh selected tenant if it matches
    const current = selectedTenant();
    if (current && current.id === id) {
      setSelectedTenant(updated);
    }
    // Refresh the tenant list so the card reflects changes
    const list = tenants();
    setTenants(list.map((t) => (t.id === id ? { ...t, ...updated } : t)));
    return updated;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to update tenant', 'error');
    return null;
  }
}

export async function createTenant(fields: { name: string; slug: string }): Promise<Tenant | null> {
  try {
    const data = await api.post<{ tenant: Tenant; api_key?: string }>(
      '/platform/self-serve/tenants',
      fields,
    );
    const created = withTransientTenantFields(
      {
        ...data.tenant,
        management_api_key: data.api_key || undefined,
      },
      null,
    );

    setTenants([created, ...tenants()]);
    toast.show('Tenant created', 'success');
    return created;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to create tenant', 'error');
    return null;
  }
}

export async function pollTenantStats(id: string): Promise<TenantStats> {
  return api.get<TenantStats>(
    '/platform/self-serve/tenants/' + encodeURIComponent(id) + '/stats',
    { window: overviewWindow() },
  );
}

export async function loadUsers(tenantId: string): Promise<void> {
  const requestSeq = ++usersRequestSeq;
  setUsersLoading(true);
  try {
    const data = await api.get<{ users: TenantUser[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users',
    );
    if (requestSeq !== usersRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setUsers(data.users || []);
  } catch (e) {
    if (requestSeq === usersRequestSeq && selectedTenant()?.id === tenantId) {
      toast.show(e instanceof Error ? e.message : 'Failed to load users', 'error');
    }
  } finally {
    if (requestSeq === usersRequestSeq) {
      setUsersLoading(false);
    }
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
    setUserSessions([]);
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke sessions', 'error');
  }
}

export async function loadUserSessions(tenantId: string, userId: string): Promise<void> {
  const seq = ++sessionRequestSeq;
  setUserSessionsLoading(true);
  try {
    const data = await api.get<{ sessions: UserSession[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/sessions',
    );
    if (seq !== sessionRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setUserSessions(data.sessions || []);
  } catch (e) {
    if (seq === sessionRequestSeq) {
      toast.show(e instanceof Error ? e.message : 'Failed to load sessions', 'error');
    }
  } finally {
    if (seq === sessionRequestSeq) {
      setUserSessionsLoading(false);
    }
  }
}

export async function loadHostedPage(tenantId: string): Promise<void> {
  const requestSeq = ++hostedPageRequestSeq;
  try {
    const data = await api.get<HostedPageData>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
    );
    if (requestSeq !== hostedPageRequestSeq || selectedTenant()?.id !== tenantId) {
      return;
    }
    setHostedPage(data);
  } catch (e) {
    if (requestSeq === hostedPageRequestSeq && selectedTenant()?.id === tenantId) {
      toast.show(e instanceof Error ? e.message : 'Failed to load hosted page', 'error');
    }
  }
}

export async function saveHostedPage(tenantId: string, data: Partial<HostedPageData>): Promise<boolean> {
  try {
    await api.put(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
      data,
    );
    toast.show('Page saved', 'success');
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to save page', 'error');
    return false;
  }

  await loadHostedPage(tenantId);
  return true;
}

export async function deleteTenant(tenantId: string, confirmSlug: string): Promise<boolean> {
  try {
    await api.request('/platform/self-serve/tenants/' + encodeURIComponent(tenantId), {
      method: 'DELETE',
      body: { confirm_slug: confirmSlug },
    });
    toast.show('Tenant deleted', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to delete tenant', 'error');
    return false;
  }
}

export async function loadIntrospect(slug: string): Promise<void> {
  const requestSeq = ++introspectRequestSeq;
  try {
    // Plane 3 introspect: uses tenant's refresh cookie (same-origin, sent automatically)
    const res = await fetch('/t/' + encodeURIComponent(slug) + '/auth/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ grant_type: 'refresh_token' }),
    });
    if (!res.ok) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }
    const tokenData = await res.json();
    const accessToken = tokenData.access_token;
    if (!accessToken) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }

    const introspectRes = await fetch('/t/' + encodeURIComponent(slug) + '/auth/introspect', {
      headers: { 'Authorization': 'Bearer ' + accessToken },
      credentials: 'include',
    });
    if (!introspectRes.ok) {
      if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
        batch(() => {
          setIntrospectData(null);
          setIntrospectToken(null);
          setIntrospectUnavailable(true);
        });
      }
      return;
    }

    const data = await introspectRes.json();
    if (requestSeq !== introspectRequestSeq || selectedTenant()?.slug !== slug) {
      return;
    }
    batch(() => {
      setIntrospectData(data);
      setIntrospectToken(accessToken);
      setIntrospectUnavailable(false);
    });
  } catch {
    if (requestSeq === introspectRequestSeq && selectedTenant()?.slug === slug) {
      batch(() => {
        setIntrospectData(null);
        setIntrospectToken(null);
        setIntrospectUnavailable(true);
      });
    }
  }
}

export function startVisibilityRefresh(slug: string): () => void {
  const handler = () => {
    if (document.visibilityState === 'visible') {
      loadIntrospect(slug);
    }
  };
  document.addEventListener('visibilitychange', handler);
  return () => document.removeEventListener('visibilitychange', handler);
}

function clearOverviewRefresh(): void {
  if (stopOverviewRefresh) {
    stopOverviewRefresh();
    stopOverviewRefresh = null;
  }
}

function syncIntrospectRefresh(slug: string): void {
  clearOverviewRefresh();
  stopOverviewRefresh = startVisibilityRefresh(slug);
}

function preferredTenantView(tenant: Tenant): TenantWorkspaceView {
  const tab = tenant.preferred_tab;
  if (tab === 'hosted-page') return 'hosted-page';
  if (tab === 'integrate') return 'integrate';
  return 'overview';
}

async function hydrateTenantView(nextView: TenantWorkspaceView, tenant: Tenant): Promise<void> {
  const tasks: Promise<void>[] = [
    loadTenantDetail(tenant.id, overviewWindow()),
    loadIntrospect(tenant.slug),
  ];

  syncIntrospectRefresh(tenant.slug);

  if (nextView === 'users') {
    tasks.push(loadUsers(tenant.id));
  } else if (nextView === 'hosted-page') {
    tasks.push(loadHostedPage(tenant.id));
  }

  await Promise.all(tasks);
}

export function navigateTenantView(nextView: TenantWorkspaceView): void {
  const tenant = selectedTenant();
  if (!tenant) return;

  setView(nextView);
  persistWorkspaceState();
  void hydrateTenantView(nextView, tenant);
}

export function changeOverviewWindow(nextWindow: OverviewWindow): void {
  if (overviewWindow() === nextWindow) {
    return;
  }

  setOverviewWindowSignal(nextWindow);

  const tenant = selectedTenant();
  if (!tenant || view() !== 'overview') {
    return;
  }

  void loadTenantDetail(tenant.id, nextWindow);
}

export function openIntegrateGuide(): void {
  setIntegrateSectionSignal('guide');
  persistWorkspaceState();
  navigateTenantView('integrate');
}

export function openSessionInspector(): void {
  const tenant = selectedTenant();

  setIntegrateSectionSignal('session-inspector');
  persistWorkspaceState();

  if (!tenant) return;

  if (view() === 'integrate') {
    void hydrateTenantView('integrate', tenant);
    return;
  }

  navigateTenantView('integrate');
}

export function setIntegrateSection(nextSection: IntegrateSection): void {
  if (integrateSection() === nextSection) {
    return;
  }

  setIntegrateSectionSignal(nextSection);
  persistWorkspaceState();

  const tenant = selectedTenant();
  if (!tenant || view() !== 'integrate') {
    return;
  }

  if (nextSection === 'session-inspector') {
    void hydrateTenantView('integrate', tenant);
  }
}

export async function refreshCurrentView(): Promise<void> {
  const currentView = view();
  const tenant = selectedTenant();

  if (currentView === 'tenants' || !tenant) {
    await Promise.all([loadTenants(), loadCurrentUser()]);
    return;
  }

  await Promise.all([
    loadTenants(),
    loadCurrentUser(),
    hydrateTenantView(currentView, tenant),
  ]);
}

// ── Navigation ──────────────────────────────────────────────────────────

export function selectTenant(tenant: Tenant): void {
  const nextView = preferredTenantView(tenant);

  batch(() => {
    setSelectedTenant(tenant);
    resetTenantScopedState();
    setView(nextView);
  });

  persistWorkspaceState();
  void hydrateTenantView(nextView, tenant);
}

export function backToTenants(): void {
  clearOverviewRefresh();

  batch(() => {
    tenantDetailRequestSeq += 1;
    usersRequestSeq += 1;
    hostedPageRequestSeq += 1;
    introspectRequestSeq += 1;
    setLoading(false);
    setSelectedTenant(null);
    resetTenantScopedState();
    setView('tenants');
  });
  persistWorkspaceState();
}

// ── Lifecycle ───────────────────────────────────────────────────────────

export async function init(): Promise<void> {
  try {
    await auth.silentRefresh();
    await Promise.all([loadTenants(), loadCurrentUser()]);
    await restoreWorkspaceState();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/platform/login';
    throw new Error('auth_redirect');
  }
}

export async function signout(): Promise<void> {
  clearOverviewRefresh();
  auth.stopAutoRefresh();
  setCurrentUser(null);
  await auth.logout();
  window.location.href = '/platform/login';
}
