// ---------------------------------------------------------------------------
// store.ts — Reactive state management for the tenant admin console
// ---------------------------------------------------------------------------

import { createStore, batch } from 'formajs';
import type { Api } from '../../core/hooks/createApi';
import type { ToastController } from '../../core/hooks/createToast';
import type { Socket } from '../../core/hooks/createSocket';
import { createPager } from '../../core/hooks/createPager';
import type { TenantAdminView } from '../../core/types';

// -- Data types (server response shapes) ------------------------------------

export interface TenantStats {
  total_users: number;
  verified_users: number;
  blocked_users: number;
  active_sessions: number;
  logins_24h: number;
  failed_logins_24h: number;
  providers: Record<string, number>;
}

export interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  avatar: string | null;
  email_verified: boolean;
  created_at: string;
}

export interface TenantRole {
  id: string;
  name: string;
  description: string | null;
  is_system: boolean;
  permissions: string[];
  tenant_id: string | null;
  created_at: string | null;
}

export interface TenantWebhook {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  description: string | null;
  secret?: string;
  created_at: string;
  updated_at: string;
}

export interface AuditEntry {
  id: string;
  email: string;
  user_id: string | null;
  success: boolean;
  ip_address: string | null;
  user_agent: string | null;
  failure_reason: string | null;
  created_at: string;
}

// -- Factory ----------------------------------------------------------------

export function createTenantAdminStore(api: Api, toast: ToastController, socket: Socket) {

  // -- Core store -----------------------------------------------------------

  const [store, setStore] = createStore({
    view: 'stats' as TenantAdminView,
    loading: false,
    loadedViews: {
      stats: false,
      users: false,
      roles: false,
      webhooks: false,
      audit: false,
    } as Record<TenantAdminView, boolean>,

    // Stats
    stats: null as TenantStats | null,

    // Users
    users: [] as TenantUser[],
    userTotal: 0,

    // Roles
    roles: [] as TenantRole[],

    // Webhooks
    webhooks: [] as TenantWebhook[],

    // Audit
    auditLog: [] as AuditEntry[],
    auditTotal: 0,

    // Modal state
    showCreateUser: false,
    showCreateRole: false,
    showCreateWebhook: false,
    webhookSecret: null as string | null,
  });

  // -- Pagers ---------------------------------------------------------------

  const userPager = createPager(50);
  const auditPager = createPager(50);

  // -- Helpers --------------------------------------------------------------

  function markViewLoaded(view: TenantAdminView, loaded: boolean): void {
    setStore({
      loadedViews: { ...store.loadedViews, [view]: loaded },
    });
  }

  function invalidateView(view: TenantAdminView): void {
    markViewLoaded(view, false);
  }

  // -- Data loaders ---------------------------------------------------------

  async function loadStats(): Promise<void> {
    const data = await api.get<TenantStats>('/admin/stats');
    setStore({ stats: data });
  }

  async function loadUsers(): Promise<void> {
    const p = userPager.page();
    const data = await api.get<{ users?: TenantUser[]; total?: number }>(
      '/admin/users',
      { limit: p.limit, offset: p.offset },
    );
    batch(() => {
      setStore({
        users: data.users ?? [],
        userTotal: data.total ?? 0,
      });
    });
  }

  async function loadRoles(): Promise<void> {
    const data = await api.get<{ roles?: TenantRole[] }>('/admin/roles');
    setStore({ roles: data.roles ?? [] });
  }

  async function loadWebhooks(): Promise<void> {
    const data = await api.get<{ webhooks?: TenantWebhook[] }>('/admin/webhooks');
    setStore({ webhooks: data.webhooks ?? [] });
  }

  async function loadAuditLog(): Promise<void> {
    const p = auditPager.page();
    const data = await api.get<{ audit_log?: AuditEntry[]; total?: number }>(
      '/admin/audit-log',
      { limit: p.limit, offset: p.offset },
    );
    batch(() => {
      setStore({
        auditLog: data.audit_log ?? [],
        auditTotal: data.total ?? (data.audit_log?.length ?? 0),
      });
    });
  }

  // -- WebSocket invalidation subscriptions ----------------------------------

  socket.subscribe('tenant:activity', () => {
    if (store.view === 'stats') {
      loadStats();
    }
    if (store.view === 'users') {
      loadUsers();
    }
    if (store.view === 'audit') {
      loadAuditLog();
    }
  });

  // -- View data orchestration ----------------------------------------------

  async function ensureViewData(view?: TenantAdminView, force?: boolean): Promise<void> {
    const v: TenantAdminView = view ?? 'stats';
    if (!force && store.loadedViews[v]) return;

    if (v === 'stats') {
      await loadStats();
    } else if (v === 'users') {
      await loadUsers();
    } else if (v === 'roles') {
      await loadRoles();
    } else if (v === 'webhooks') {
      await loadWebhooks();
    } else if (v === 'audit') {
      await loadAuditLog();
    }

    markViewLoaded(v, true);
  }

  async function refreshAll(): Promise<void> {
    setStore({ loading: true });
    try {
      const view = store.view;
      await ensureViewData(view, true);
      if (view !== 'stats' && store.stats == null) {
        await ensureViewData('stats', true);
      }
      setStore({ loading: false });
    } catch (err: unknown) {
      setStore({ loading: false });
      const message = err instanceof Error ? err.message : 'Failed to load data';
      toast.show(message, 'error');
    }
  }

  // -- Navigation -----------------------------------------------------------

  function navigate(view: TenantAdminView): void {
    setStore({ view });
    ensureViewData(view).catch((e) => {
      toast.show(e instanceof Error ? e.message : 'Failed to load view', 'error');
    });
  }

  // -- Pagination -----------------------------------------------------------

  async function changeUserPage(dir: 'prev' | 'next'): Promise<void> {
    userPager.navigate(dir, store.userTotal);
    await loadUsers();
  }

  async function changeAuditPage(dir: 'prev' | 'next'): Promise<void> {
    auditPager.navigate(dir, store.auditTotal);
    await loadAuditLog();
  }

  // -- Mutations: Users -----------------------------------------------------

  async function createUser(email: string, password: string): Promise<void> {
    await api.post('/admin/users', { email, password });
    invalidateView('stats');
    await loadUsers();
    setStore({ showCreateUser: false });
    toast.show('User created', 'success');
  }

  async function deleteUser(userId: string): Promise<void> {
    await api.del('/admin/users/' + encodeURIComponent(userId));
    invalidateView('stats');
    await loadUsers();
    toast.show('User deleted', 'success');
  }

  async function blockUser(userId: string): Promise<void> {
    await api.post('/admin/users/' + encodeURIComponent(userId) + '/block');
    invalidateView('stats');
    await loadUsers();
    toast.show('User blocked', 'success');
  }

  async function unblockUser(userId: string): Promise<void> {
    await api.post('/admin/users/' + encodeURIComponent(userId) + '/unblock');
    invalidateView('stats');
    await loadUsers();
    toast.show('User unblocked', 'success');
  }

  // -- Mutations: Roles -----------------------------------------------------

  async function createRole(name: string, permissions: string[], description?: string): Promise<void> {
    await api.post('/admin/roles', { name, permissions, description: description || '' });
    await loadRoles();
    setStore({ showCreateRole: false });
    toast.show('Role created', 'success');
  }

  async function deleteRole(roleId: string): Promise<void> {
    await api.del('/admin/roles/' + encodeURIComponent(roleId));
    await loadRoles();
    toast.show('Role deleted', 'success');
  }

  // -- Mutations: Webhooks --------------------------------------------------

  async function createWebhook(url: string, events: string[], description?: string): Promise<void> {
    const data = await api.post<{ webhook?: TenantWebhook }>(
      '/admin/webhooks',
      { url, events, description: description || '' },
    );
    const secret = data?.webhook?.secret ?? null;
    setStore({ webhookSecret: secret, showCreateWebhook: false });
    await loadWebhooks();
    if (secret) {
      toast.show('Webhook created. Secret shown once -- copy it now.', 'success');
    } else {
      toast.show('Webhook created', 'success');
    }
  }

  async function deleteWebhook(webhookId: string): Promise<void> {
    await api.del('/admin/webhooks/' + encodeURIComponent(webhookId));
    await loadWebhooks();
    toast.show('Webhook deleted', 'success');
  }

  async function testWebhook(webhookId: string): Promise<void> {
    await api.post('/admin/webhooks/' + encodeURIComponent(webhookId) + '/test');
    toast.show('Test event sent', 'success');
  }

  function dismissWebhookSecret(): void {
    setStore({ webhookSecret: null });
  }

  // -- Modal toggles --------------------------------------------------------

  function openCreateUser(): void {
    setStore({ showCreateUser: true });
  }

  function closeCreateUser(): void {
    setStore({ showCreateUser: false });
  }

  function openCreateRole(): void {
    setStore({ showCreateRole: true });
  }

  function closeCreateRole(): void {
    setStore({ showCreateRole: false });
  }

  function openCreateWebhook(): void {
    setStore({ showCreateWebhook: true });
  }

  function closeCreateWebhook(): void {
    setStore({ showCreateWebhook: false });
  }

  // -- Public API -----------------------------------------------------------

  return {
    // State
    store,
    setStore,

    // Pagers
    userPager,
    auditPager,

    // Navigation
    navigate,

    // Data loading
    loadStats,
    loadUsers,
    loadRoles,
    loadWebhooks,
    loadAuditLog,
    ensureViewData,
    refreshAll,

    // Pagination
    changeUserPage,
    changeAuditPage,

    // Mutations: Users
    createUser,
    deleteUser,
    blockUser,
    unblockUser,

    // Mutations: Roles
    createRole,
    deleteRole,

    // Mutations: Webhooks
    createWebhook,
    deleteWebhook,
    testWebhook,
    dismissWebhookSecret,

    // Modal toggles
    openCreateUser,
    closeCreateUser,
    openCreateRole,
    closeCreateRole,
    openCreateWebhook,
    closeCreateWebhook,
  };
}

export type TenantAdminStore = ReturnType<typeof createTenantAdminStore>;
