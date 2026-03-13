// ---------------------------------------------------------------------------
// store.ts — Reactive state management for the owner (admin) dashboard
// ---------------------------------------------------------------------------

import { createStore, batch } from 'formajs';
import type { Api } from '../../core/hooks/createApi';
import type { ToastController } from '../../core/hooks/createToast';
import { createFilters } from '../../core/hooks/createFilters';
import { createPager } from '../../core/hooks/createPager';
import type {
  OwnerView, Stats, Analytics, Customer, CustomerInsights,
  Payment, PaymentSummary, TimelineEvent, Invite,
  CustomerFilters, PaymentFilters, TimelineFilters, Facets,
  TenantInventoryItem, TenantFilters,
  CsvColumn,
} from '../../core/types';
import { toCsv, downloadCsv, copyText } from '../../core/utils';

// -- CSV column definitions --------------------------------------------------

const CUSTOMER_CSV_COLUMNS: CsvColumn[] = [
  { key: 'user_id', label: 'user_id' },
  { key: 'email', label: 'email' },
  { key: 'status', label: 'status' },
  { key: 'category', label: 'category' },
  { key: 'tenant_count', label: 'tenant_count' },
  { key: 'lifetime_revenue_cents', label: 'lifetime_revenue_cents' },
  { key: 'trial_expiring_tenants', label: 'trial_expiring_tenants' },
  { key: 'failed_logins_24h', label: 'failed_logins_24h' },
  { key: 'payment_failed_30d', label: 'payment_failed_30d' },
  { key: 'created_at', label: 'created_at' },
];

const TENANT_CSV_COLUMNS: CsvColumn[] = [
  { key: 'id', label: 'id' },
  { key: 'slug', label: 'slug' },
  { key: 'name', label: 'name' },
  { key: 'owner_email', label: 'owner_email' },
  { key: 'status', label: 'status' },
  { key: 'plan', label: 'plan' },
  { key: 'user_count', label: 'user_count' },
  { key: 'active_users_7d', label: 'active_users_7d' },
  { key: 'active_sessions', label: 'active_sessions' },
  { key: 'failed_logins_24h', label: 'failed_logins_24h' },
  { key: 'payment_failed_30d', label: 'payment_failed_30d' },
  { key: 'last_login_at', label: 'last_login_at' },
  { key: 'trial_ends_at', label: 'trial_ends_at' },
];

const PAYMENT_CSV_COLUMNS: CsvColumn[] = [
  { key: 'id', label: 'id' },
  { key: 'customer_user_id', label: 'customer_user_id' },
  { key: 'customer_email', label: 'customer_email' },
  { key: 'tenant_slug', label: 'tenant_slug' },
  { key: 'amount_cents', label: 'amount_cents' },
  { key: 'currency', label: 'currency' },
  { key: 'status', label: 'status' },
  { key: 'external_invoice_id', label: 'external_invoice_id' },
  { key: 'receipt_url', label: 'receipt_url' },
  { key: 'created_at', label: 'created_at' },
];

// -- Factory -----------------------------------------------------------------

export function createOwnerStore(api: Api, toast: ToastController) {

  // -- Core store (view state + data) ----------------------------------------

  const [store, setStore] = createStore({
    view: 'overview' as OwnerView,
    loading: false,
    loadedViews: {
      overview: false,
      customers: false,
      billing: false,
      timeline: false,
      invites: false,
      security: false,
    } as Record<OwnerView, boolean>,

    // Overview
    stats: null as Stats | null,
    analytics: null as Analytics | null,

    // Customers
    customers: [] as Customer[],
    customerTotal: 0,
    customerFacets: { status: [], plan: [], category: [] } as Facets,
    selectedCustomer: null as Customer | null,
    customerInsights: null as CustomerInsights | null,
    customerTenants: [] as TenantInventoryItem[],

    // Tenant inventory
    tenantInventory: [] as TenantInventoryItem[],
    tenantInventoryTotal: 0,
    tenantInventoryFacets: { status: [], plan: [] } as Facets,

    // Payments / billing
    payments: [] as Payment[],
    paymentSummary: null as PaymentSummary | null,
    paymentTotal: 0,

    // Timeline
    timelineEvents: [] as TimelineEvent[],
    timelineTotal: 0,

    // Invites
    invites: [] as Invite[],
  });

  // -- Filters (signal-based, persisted to localStorage) ---------------------

  const customerFilters = createFilters('owner:customers', {
    q: '', status: '', category: '', plan: '', created_from: '', created_to: '',
  } as CustomerFilters & Record<string, string>);

  const tenantFilters = createFilters('owner:tenants', {
    q: '', status: '', plan: '', owner_user_id: '',
  } as TenantFilters & Record<string, string>);

  const paymentFilters = createFilters('owner:payments', {
    status: '', customer_user_id: '', from: '', to: '',
  } as PaymentFilters & Record<string, string>);

  const timelineFilters = createFilters('owner:timeline', {
    actor: '', action: '', resource: '', request_id: '', from: '', to: '',
  } as TimelineFilters & Record<string, string>);

  // -- Pagers (signal-based) -------------------------------------------------

  const customerPager = createPager(50);
  const tenantPager = createPager(50);
  const paymentPager = createPager(50);
  const timelinePager = createPager(50);

  // -- Helpers ---------------------------------------------------------------

  function markViewLoaded(view: OwnerView, loaded: boolean): void {
    setStore({
      loadedViews: { ...store.loadedViews, [view]: loaded },
    });
  }

  function invalidateView(view: OwnerView): void {
    markViewLoaded(view, false);
  }

  // -- Data loaders ----------------------------------------------------------

  async function loadStats(): Promise<void> {
    const stats = await api.get<Stats>('/platform/admin/stats');
    setStore({ stats });
  }

  async function loadAnalytics(): Promise<void> {
    const analytics = await api.get<Analytics>('/platform/admin/analytics', { days: 30 });
    setStore({ analytics });
  }

  async function loadCustomers(): Promise<void> {
    const f = customerFilters.filters();
    const p = customerPager.page();
    customerFilters.save();
    const data = await api.get<{
      customers?: Customer[];
      total?: number;
      facets?: Facets;
    }>('/platform/admin/customers', {
      limit: p.limit,
      offset: p.offset,
      sort: 'created_desc',
      ...f,
    });
    batch(() => {
      setStore({
        customers: data.customers ?? [],
        customerTotal: data.total ?? 0,
        customerFacets: data.facets ?? { status: [], plan: [], category: [] },
      });
    });
  }

  async function loadTenantInventory(): Promise<void> {
    const f = tenantFilters.filters();
    const p = tenantPager.page();
    tenantFilters.save();
    const data = await api.get<{
      tenants?: TenantInventoryItem[];
      total?: number;
      facets?: Facets;
    }>('/platform/admin/tenants', {
      limit: p.limit,
      offset: p.offset,
      ...f,
    });
    batch(() => {
      setStore({
        tenantInventory: data.tenants ?? [],
        tenantInventoryTotal: data.total ?? 0,
        tenantInventoryFacets: data.facets ?? { status: [], plan: [] },
      });
    });
  }

  async function loadCustomerDetail(userId: string): Promise<void> {
    if (!userId) return;
    const base = '/platform/admin/customers/' + encodeURIComponent(userId);
    const [detail, tenants, insights] = await Promise.all([
      api.get<{ customer?: Customer }>(base),
      api.get<{ tenants?: TenantInventoryItem[] }>(base + '/tenants'),
      api.get<CustomerInsights>(base + '/insights'),
    ]);
    setStore({
      selectedCustomer: detail.customer ?? null,
      customerTenants: tenants.tenants ?? [],
      customerInsights: insights ?? null,
    });
  }

  async function loadPayments(): Promise<void> {
    const f = paymentFilters.filters();
    const p = paymentPager.page();
    paymentFilters.save();
    const data = await api.get<{
      payments?: Payment[];
      summary?: PaymentSummary;
      total?: number;
    }>('/platform/admin/payments', {
      limit: p.limit,
      offset: p.offset,
      ...f,
    });
    setStore({
      payments: data.payments ?? [],
      paymentSummary: data.summary ?? null,
      paymentTotal: data.total ?? 0,
    });
  }

  async function loadTimeline(): Promise<void> {
    const f = timelineFilters.filters();
    const p = timelinePager.page();
    timelineFilters.save();
    const data = await api.get<{
      events?: TimelineEvent[];
      total?: number;
    }>('/platform/admin/timeline', {
      limit: p.limit,
      offset: p.offset,
      ...f,
    });
    setStore({
      timelineEvents: data.events ?? [],
      timelineTotal: data.total ?? 0,
    });
  }

  async function loadInvites(): Promise<void> {
    const data = await api.get<{ invites?: Invite[] }>('/platform/admin/invites');
    setStore({ invites: data.invites ?? [] });
  }

  // -- View data orchestration -----------------------------------------------

  async function ensureViewData(view?: OwnerView, force?: boolean): Promise<void> {
    const v: OwnerView = view ?? 'overview';
    if (!force && store.loadedViews[v]) return;

    if (v === 'overview') {
      await Promise.all([loadStats(), loadAnalytics()]);
    } else if (v === 'customers') {
      await Promise.all([loadCustomers(), loadTenantInventory()]);
    } else if (v === 'billing') {
      await Promise.all([loadPayments(), loadCustomers()]);
    } else if (v === 'timeline') {
      await loadTimeline();
    } else if (v === 'invites') {
      await loadInvites();
    }

    markViewLoaded(v, true);
  }

  async function refreshAll(): Promise<void> {
    setStore({ loading: true });
    try {
      const view = store.view;
      await ensureViewData(view, true);
      // Always ensure overview stats are available for the header
      if (view !== 'overview' && store.stats == null) {
        await ensureViewData('overview', true);
      }
      setStore({ loading: false });
    } catch (err: unknown) {
      setStore({ loading: false });
      const message = err instanceof Error ? err.message : 'Failed to load control plane data';
      toast.show(message, 'error');
    }
  }

  // -- Navigation ------------------------------------------------------------

  function navigate(view: OwnerView): void {
    setStore({ view });
    ensureViewData(view).catch((e) => {
      toast.show(e instanceof Error ? e.message : 'Failed to load view', 'error');
    });
  }

  function selectCustomer(userId: string): void {
    loadCustomerDetail(userId).catch((e) => {
      toast.show(e instanceof Error ? e.message : 'Failed to load customer', 'error');
    });
  }

  function clearSelectedCustomer(): void {
    setStore({ selectedCustomer: null, customerInsights: null, customerTenants: [] });
  }

  // -- Pagination helpers ----------------------------------------------------

  async function changeCustomerPage(dir: 'prev' | 'next'): Promise<void> {
    customerPager.navigate(dir, store.customerTotal);
    await loadCustomers();
  }

  async function changeTenantPage(dir: 'prev' | 'next'): Promise<void> {
    tenantPager.navigate(dir, store.tenantInventoryTotal);
    await loadTenantInventory();
  }

  async function changePaymentPage(dir: 'prev' | 'next'): Promise<void> {
    paymentPager.navigate(dir, store.paymentTotal);
    await loadPayments();
  }

  async function changeTimelinePage(dir: 'prev' | 'next'): Promise<void> {
    timelinePager.navigate(dir, store.timelineTotal);
    await loadTimeline();
  }

  // -- Filter apply/clear helpers --------------------------------------------

  async function applyCustomerFilters(): Promise<void> {
    customerPager.reset();
    await loadCustomers();
  }

  async function clearCustomerFilters(): Promise<void> {
    customerFilters.clear();
    customerPager.reset();
    await loadCustomers();
  }

  async function applyTenantFilters(): Promise<void> {
    tenantPager.reset();
    await loadTenantInventory();
  }

  async function clearTenantFilters(): Promise<void> {
    tenantFilters.clear();
    tenantPager.reset();
    await loadTenantInventory();
  }

  async function applyPaymentFilters(): Promise<void> {
    paymentPager.reset();
    await loadPayments();
  }

  async function clearPaymentFilters(): Promise<void> {
    paymentFilters.clear();
    paymentPager.reset();
    await loadPayments();
  }

  async function applyTimelineFilters(): Promise<void> {
    timelinePager.reset();
    await loadTimeline();
  }

  async function clearTimelineFilters(): Promise<void> {
    timelineFilters.clear();
    timelinePager.reset();
    await loadTimeline();
  }

  // -- CSV export ------------------------------------------------------------

  async function fetchAll(
    basePath: string,
    key: string,
    filters: Record<string, unknown>,
    extra: Record<string, unknown>,
  ): Promise<Record<string, unknown>[]> {
    const limit = 500;
    let offset = 0;
    let rows: Record<string, unknown>[] = [];

    for (;;) {
      const data = await api.get<Record<string, unknown>>(basePath, {
        limit, offset, ...extra, ...filters,
      });
      const batchRows = (data[key] as Record<string, unknown>[]) ?? [];
      rows = rows.concat(batchRows);
      if (
        !batchRows.length ||
        batchRows.length < limit ||
        rows.length >= Number(data.total ?? rows.length)
      ) {
        break;
      }
      offset += batchRows.length;
    }

    return rows;
  }

  async function exportCsv(kind: string): Promise<void> {
    try {
      if (kind === 'customers') {
        const rows = await fetchAll(
          '/platform/admin/customers', 'customers',
          customerFilters.filters() as unknown as Record<string, unknown>,
          { sort: 'created_desc' },
        );
        downloadCsv('customers-export.csv', toCsv(rows, CUSTOMER_CSV_COLUMNS));
      } else if (kind === 'tenants') {
        const rows = await fetchAll(
          '/platform/admin/tenants', 'tenants',
          tenantFilters.filters() as unknown as Record<string, unknown>,
          {},
        );
        downloadCsv('tenants-export.csv', toCsv(rows, TENANT_CSV_COLUMNS));
      } else if (kind === 'payments') {
        const rows = await fetchAll(
          '/platform/admin/payments', 'payments',
          paymentFilters.filters() as unknown as Record<string, unknown>,
          {},
        );
        downloadCsv('payments-export.csv', toCsv(rows, PAYMENT_CSV_COLUMNS));
      }
      toast.show('Export downloaded', 'success');
    } catch (e) {
      toast.show(e instanceof Error ? e.message : 'Export failed', 'error');
    }
  }

  // -- Mutations: customer lifecycle -----------------------------------------

  async function updateCustomerLifecycle(
    userId: string,
    body: Record<string, unknown>,
  ): Promise<void> {
    await api.patch(
      '/platform/admin/customers/' + encodeURIComponent(userId) + '/lifecycle',
      body,
    );
    invalidateView('timeline');
    await loadCustomers();
    if (store.selectedCustomer && store.selectedCustomer.user_id === userId) {
      await loadCustomerDetail(userId);
    }
    await loadStats();
  }

  async function extendAccess(userId: string, days: number): Promise<void> {
    await api.post(
      '/platform/admin/customers/' + encodeURIComponent(userId) + '/extend-access',
      { days },
    );
    invalidateView('timeline');
    if (store.selectedCustomer && store.selectedCustomer.user_id === userId) {
      await loadCustomerDetail(userId);
    }
    toast.show('Access extended by ' + days + ' days', 'success');
  }

  async function updateTenantStatus(
    customerId: string,
    tenantId: string,
    status: string,
  ): Promise<void> {
    await api.post(
      '/platform/admin/customers/' + encodeURIComponent(customerId) +
      '/tenants/' + encodeURIComponent(tenantId) + '/status',
      { status },
    );
    invalidateView('timeline');
    if (store.selectedCustomer && store.selectedCustomer.user_id === customerId) {
      await loadCustomerDetail(customerId);
    }
    await Promise.all([loadStats(), loadTenantInventory()]);
  }

  async function deleteCustomerTenant(
    customerId: string,
    tenantId: string,
    _confirmSlug: string,
  ): Promise<void> {
    await api.del(
      '/platform/admin/customers/' + encodeURIComponent(customerId) +
      '/tenants/' + encodeURIComponent(tenantId),
    );
    invalidateView('timeline');
    if (store.selectedCustomer && store.selectedCustomer.user_id === customerId) {
      await loadCustomerDetail(customerId);
    }
    await Promise.all([loadCustomers(), loadTenantInventory(), loadStats()]);
    toast.show('Tenant deleted', 'success');
  }

  // -- Mutations: invites ----------------------------------------------------

  async function createInvite(email: string): Promise<void> {
    const data = await api.post<{ invite_url?: string }>(
      '/platform/admin/invites',
      { email, expires_in_hours: 72 },
    );
    if (data?.invite_url) {
      try {
        await copyText(data.invite_url);
        toast.show('Invite created and copied to clipboard', 'success');
      } catch {
        toast.show('Invite created: ' + data.invite_url, 'success');
      }
    }
    invalidateView('timeline');
    await loadInvites();
  }

  async function revokeInvite(id: string): Promise<void> {
    await api.post(
      '/platform/admin/invites/' + encodeURIComponent(id) + '/revoke',
    );
    invalidateView('timeline');
    await loadInvites();
    toast.show('Invite revoked', 'success');
  }

  // -- Mutations: payments ---------------------------------------------------

  interface PaymentFormData {
    customer_user_id: string;
    tenant_id?: string;
    amount: string;
    status?: string;
    currency?: string;
    invoice?: string;
    receipt_url?: string;
    notes?: string;
  }

  async function createPayment(formData: PaymentFormData): Promise<void> {
    const amountCents = Math.round(Number(formData.amount || 0) * 100);
    if (!formData.customer_user_id || amountCents <= 0) {
      throw new Error('Customer and valid amount are required');
    }
    await api.post('/platform/admin/payments', {
      customer_user_id: formData.customer_user_id,
      tenant_id: formData.tenant_id || null,
      amount_cents: amountCents,
      status: formData.status || 'paid',
      currency: (formData.currency || 'usd').toLowerCase(),
      external_invoice_id: formData.invoice || null,
      receipt_url: formData.receipt_url || null,
      notes: formData.notes || null,
    });
    invalidateView('timeline');
    await loadPayments();
    await loadStats();
    toast.show('Payment event recorded', 'success');
  }

  // -- Public API ------------------------------------------------------------

  return {
    // State
    store,
    setStore,

    // Filters
    customerFilters,
    tenantFilters,
    paymentFilters,
    timelineFilters,

    // Pagers
    customerPager,
    tenantPager,
    paymentPager,
    timelinePager,

    // Navigation
    navigate,
    selectCustomer,
    clearSelectedCustomer,

    // Data loading
    loadStats,
    loadAnalytics,
    loadCustomers,
    loadTenantInventory,
    loadCustomerDetail,
    loadPayments,
    loadTimeline,
    loadInvites,
    ensureViewData,
    refreshAll,

    // Pagination
    changeCustomerPage,
    changeTenantPage,
    changePaymentPage,
    changeTimelinePage,

    // Filter apply/clear
    applyCustomerFilters,
    clearCustomerFilters,
    applyTenantFilters,
    clearTenantFilters,
    applyPaymentFilters,
    clearPaymentFilters,
    applyTimelineFilters,
    clearTimelineFilters,

    // Export
    exportCsv,

    // Mutations
    updateCustomerLifecycle,
    extendAccess,
    updateTenantStatus,
    deleteCustomerTenant,
    createInvite,
    revokeInvite,
    createPayment,
  };
}

export type OwnerStore = ReturnType<typeof createOwnerStore>;
