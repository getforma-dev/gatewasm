// ---------------------------------------------------------------------------
// CustomersView.ts — Customer list view with filters, table, pager
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { FilterBar } from '../../../core/components/FilterBar';
import { DataTable } from '../../../core/components/DataTable';
import { Pager } from '../../../core/components/Pager';
import { Pill } from '../../../core/components/Pill';
import { Badge } from '../../../core/components/Badge';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { money } from '../../../core/utils';
import type { OwnerStore } from '../store';
import type { Customer, FilterField } from '../../../core/types';

// -- Alert chip helper ------------------------------------------------------

function alertChips(c: Customer): HTMLElement {
  const chips: HTMLElement[] = [];
  if (c.status === 'suspended' || c.status === 'rejected') {
    chips.push(Badge('suspended', 'danger'));
  }
  if ((c.trial_expiring_tenants || 0) > 0) {
    chips.push(Badge('trial expiring', 'warning'));
  }
  if ((c.failed_logins_24h || 0) >= 20) {
    chips.push(Badge('high failures', 'danger'));
  }
  if (c.payment_failed_30d) {
    chips.push(Badge('payment failed', 'warning'));
  }
  return h('div', { class: 'chip-row' }, ...chips);
}

// -- Component --------------------------------------------------------------

export function CustomersView(os: OwnerStore) {
  const { store, customerFilters, customerPager, selectCustomer, clearSelectedCustomer } = os;
  const { applyCustomerFilters, clearCustomerFilters, changeCustomerPage, exportCsv } = os;
  const { updateCustomerLifecycle, extendAccess } = os;

  // Filter bar
  const fields: FilterField[] = [
    {
      name: 'q', label: 'Search', type: 'text',
      value: () => customerFilters.filters().q,
      onChange: (v) => customerFilters.update({ q: v } as any),
      placeholder: 'Email / company / name',
    },
    {
      name: 'status', label: 'Status', type: 'select',
      value: () => customerFilters.filters().status,
      onChange: (v) => customerFilters.update({ status: v } as any),
      options: () => (store.customerFacets.status || []).map((s: string) => ({ value: s, label: s })),
    },
    {
      name: 'category', label: 'Category', type: 'select',
      value: () => customerFilters.filters().category,
      onChange: (v) => customerFilters.update({ category: v } as any),
      options: () => (store.customerFacets.category || []).map((s: string) => ({ value: s, label: s })),
    },
    {
      name: 'plan', label: 'Plan', type: 'select',
      value: () => customerFilters.filters().plan,
      onChange: (v) => customerFilters.update({ plan: v } as any),
      options: () => (store.customerFacets.plan || []).map((s: string) => ({ value: s, label: s })),
    },
    {
      name: 'created_from', label: 'From', type: 'datetime',
      value: () => customerFilters.filters().created_from,
      onChange: (v) => customerFilters.update({ created_from: v } as any),
    },
    {
      name: 'created_to', label: 'To', type: 'datetime',
      value: () => customerFilters.filters().created_to,
      onChange: (v) => customerFilters.update({ created_to: v } as any),
    },
  ];

  const filterBar = FilterBar({
    id: 'customer-filters',
    fields,
    onApply: () => applyCustomerFilters(),
    onClear: () => clearCustomerFilters(),
    onSave: () => customerFilters.save(),
    onLoad: () => { customerFilters.load(); applyCustomerFilters(); },
    onExport: () => exportCsv('customers'),
    total: () => store.customerTotal,
  });

  // Data table
  const table = DataTable<Customer>({
    headers: ['Customer', 'Status', 'Category', 'Tenants', 'Revenue', 'Alerts', 'Actions'],
    rows: () => store.customers,
    keyFn: (c) => c.user_id,
    renderRow: (c) => {
      const selected = store.selectedCustomer?.user_id === c.user_id;
      return h('tr', { class: selected ? 'selected-row' : '' },
        h('td', null,
          h('code', null, c.user_id),
          h('br'),
          h('span', { class: 'text-muted' }, c.email),
        ),
        h('td', null, Pill(c.status)),
        h('td', null, Pill(c.category || 'standard')),
        h('td', null, String(c.tenant_count || 0)),
        h('td', null, money(c.lifetime_revenue_cents)),
        h('td', null, alertChips(c)),
        h('td', null,
          h('div', { class: 'action-row' },
            h('button', {
              class: 'btn small',
              onClick: () => selectCustomer(c.user_id),
            }, 'Open'),
            h('button', {
              class: 'btn small warning',
              onClick: () => updateCustomerLifecycle(c.user_id, { status: 'suspended' }),
            }, 'Suspend'),
            h('button', {
              class: 'btn small',
              onClick: () => updateCustomerLifecycle(c.user_id, { status: 'active' }),
            }, 'Activate'),
            h('button', {
              class: 'btn small danger',
              onClick: () => updateCustomerLifecycle(c.user_id, { status: 'rejected' }),
            }, 'Reject'),
            h('button', {
              class: 'btn small primary',
              onClick: () => extendAccess(c.user_id, 14),
            }, '+14d'),
          ),
        ),
      );
    },
    emptyText: 'No customers match current filters',
  });

  // Pager
  const pager = Pager({
    total: () => store.customerTotal,
    page: () => customerPager.page(),
    onNavigate: (dir) => changeCustomerPage(dir),
  });

  // Selected customer summary
  const selectedSummary = createShow(
    () => store.selectedCustomer != null,
    () => {
      const c = store.selectedCustomer!;
      return Panel({},
        PanelTitle(
          h('span', null, 'Selected Customer'),
          Pill(c.status),
        ),
        h('div', { class: 'text-muted' },
          h('strong', null, c.email),
          ' \u00b7 ', c.company_name || 'No company',
        ),
        h('div', { class: 'text-muted' },
          'Tenants: ', h('strong', null, String(c.tenant_count || 0)),
          ' \u00b7 Lifetime: ', h('strong', null, money(c.lifetime_revenue_cents)),
        ),
        h('div', { class: 'action-row' },
          h('button', {
            class: 'btn small primary',
            onClick: () => selectCustomer(c.user_id),
          }, 'Open Customer 360'),
          h('button', {
            class: 'btn small warning',
            onClick: () => updateCustomerLifecycle(c.user_id, { status: 'suspended' }),
          }, 'Suspend'),
          h('button', {
            class: 'btn small',
            onClick: () => updateCustomerLifecycle(c.user_id, { status: 'active' }),
          }, 'Activate'),
          h('button', {
            class: 'btn small primary',
            onClick: () => extendAccess(c.user_id, 30),
          }, 'Extend +30d'),
          h('button', {
            class: 'btn small',
            onClick: () => clearSelectedCustomer(),
          }, 'Clear'),
        ),
      );
    },
    () => h('div', { class: 'empty' },
      'Select a customer and open Customer 360 drawer for lifecycle/auth/payment details.',
    ),
  );

  return h('div', { class: 'content' },
    Panel({},
      PanelTitle(
        h('span', null, 'Customer Filters'),
        Badge(String(store.customerTotal || 0) + ' total'),
      ),
      filterBar,
    ),
    h('div', { class: 'customers-layout' },
      Panel({},
        PanelTitle(
          h('span', null, 'Customers'),
          h('span', { class: 'text-muted' }, 'Lifecycle + access controls'),
        ),
        table,
        pager,
      ),
      Panel({}, selectedSummary),
    ),
  );
}
