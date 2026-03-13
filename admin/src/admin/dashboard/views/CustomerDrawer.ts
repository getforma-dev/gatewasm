// ---------------------------------------------------------------------------
// CustomerDrawer.ts — Customer 360 detail drawer (side panel)
// ---------------------------------------------------------------------------

import { StatCard } from '../../../core/components/StatCard';
import { Pill } from '../../../core/components/Pill';
import { Badge } from '../../../core/components/Badge';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { DataTable } from '../../../core/components/DataTable';
import { money, fmtDate } from '../../../core/utils';
import type { OwnerStore } from '../store';
import type {
  AuditEvent, AuthEvent, Payment, TenantInventoryItem,
} from '../../../core/types';

import { h, createShow } from 'formajs';

// -- Component --------------------------------------------------------------

export function CustomerDrawer(os: OwnerStore) {
  const { store, clearSelectedCustomer, updateCustomerLifecycle, extendAccess } = os;
  const { updateTenantStatus, deleteCustomerTenant } = os;

  return createShow(
    () => store.selectedCustomer != null && store.customerInsights != null,
    () => {
      const c = store.selectedCustomer!;
      const insights = store.customerInsights!;
      const lifecycle = insights.lifecycle_history ?? [];
      const authEvents = insights.recent_auth_events ?? [];
      const payments = insights.recent_payments ?? [];
      const paymentSummary = insights.payment_summary ?? { paid: 0, pending: 0, failed: 0, refunded: 0 };
      const tenants = store.customerTenants ?? [];

      // Profile section
      const profile = Panel({},
        PanelTitle(h('span', null, 'Profile'), Pill(c.status)),
        h('div', { class: 'customer-detail-meta' },
          h('div', { class: 'meta-kv' },
            h('span', { class: 'k' }, 'Customer ID'),
            h('span', { class: 'v' }, h('code', null, c.user_id || '-')),
          ),
          h('div', { class: 'meta-kv' },
            h('span', { class: 'k' }, 'Company'),
            h('span', { class: 'v' }, c.company_name || '-'),
          ),
          h('div', { class: 'meta-kv' },
            h('span', { class: 'k' }, 'Category'),
            h('span', { class: 'v' }, c.category || '-'),
          ),
          h('div', { class: 'meta-kv' },
            h('span', { class: 'k' }, 'Revenue'),
            h('span', { class: 'v' }, money(c.lifetime_revenue_cents)),
          ),
        ),
      );

      // Payment summary stat cards
      const paySummary = Panel({},
        PanelTitle(h('span', null, 'Payment Summary')),
        h('div', { class: 'stat-grid stat-grid-2' },
          StatCard({ label: 'Paid', value: () => money(paymentSummary.paid) }),
          StatCard({ label: 'Pending', value: () => money(paymentSummary.pending) }),
          StatCard({ label: 'Failed', value: () => money(paymentSummary.failed) }),
          StatCard({ label: 'Refunded', value: () => money(paymentSummary.refunded) }),
        ),
      );

      // Tenant table
      const tenantTable = Panel({},
        PanelTitle(
          h('span', null, 'Customer Tenants'),
          Badge(String(tenants.length)),
        ),
        DataTable<TenantInventoryItem>({
          headers: ['Tenant', 'Status', 'Plan', 'Users', 'Active 7d', 'Fails 24h', 'Actions'],
          rows: () => tenants,
          keyFn: (t) => t.id,
          renderRow: (t) => h('tr', null,
            h('td', null, h('code', null, t.slug || t.id)),
            h('td', null, Pill(t.status)),
            h('td', null, Pill(t.plan)),
            h('td', null, String(t.user_count || 0)),
            h('td', null, String(t.active_users_7d || 0)),
            h('td', null, String(t.failed_logins_24h || 0)),
            h('td', null,
              h('div', { class: 'action-row' },
                t.status === 'suspended'
                  ? h('button', {
                      class: 'btn small',
                      onClick: () => updateTenantStatus(c.user_id, t.id, 'active'),
                    }, 'Activate')
                  : h('button', {
                      class: 'btn small warning',
                      onClick: () => updateTenantStatus(c.user_id, t.id, 'suspended'),
                    }, 'Suspend'),
                h('button', {
                  class: 'btn small danger',
                  onClick: () => deleteCustomerTenant(c.user_id, t.id, t.slug),
                }, 'Delete'),
              ),
            ),
          ),
          emptyText: 'No tenants',
        }),
      );

      // Lifecycle history table
      const lifecycleTable = Panel({},
        PanelTitle(
          h('span', null, 'Lifecycle History'),
          h('span', { class: 'text-muted' }, 'Latest 24'),
        ),
        DataTable<AuditEvent>({
          headers: ['At', 'Action', 'Actor', 'Request ID'],
          rows: () => lifecycle.slice(0, 24),
          keyFn: (ev: AuditEvent) => ev.created_at + ':' + (ev.action || '') + ':' + (ev.request_id || ''),
          renderRow: (ev: AuditEvent) => h('tr', null,
            h('td', null, fmtDate(ev.created_at)),
            h('td', null, ev.action || '-'),
            h('td', null, ev.actor_email || ev.actor_user_id || '-'),
            h('td', null, h('code', null, ev.request_id || '-')),
          ),
          emptyText: 'No lifecycle events yet',
        }),
      );

      // Auth events table
      const authTable = Panel({},
        PanelTitle(
          h('span', null, 'Recent Auth Events'),
          h('span', { class: 'text-muted' }, 'Latest 24'),
        ),
        DataTable<AuthEvent>({
          headers: ['At', 'Source', 'Action', 'Result', 'Failure'],
          rows: () => authEvents.slice(0, 24),
          keyFn: (ev: AuthEvent) => ev.created_at + ':' + (ev.action || '') + ':' + (ev.source || ''),
          renderRow: (ev: AuthEvent) => h('tr', null,
            h('td', null, fmtDate(ev.created_at)),
            h('td', null, ev.source || '-'),
            h('td', null, ev.action || '-'),
            h('td', null, ev.result || '-'),
            h('td', null, ev.failure_reason || '-'),
          ),
          emptyText: 'No recent auth events',
        }),
      );

      // Payment table
      const paymentTable = Panel({},
        PanelTitle(
          h('span', null, 'Recent Payments'),
          h('span', { class: 'text-muted' }, 'Latest 20'),
        ),
        DataTable<Payment>({
          headers: ['At', 'Tenant', 'Amount', 'Status', 'Invoice'],
          rows: () => payments.slice(0, 20),
          keyFn: (p) => p.id,
          renderRow: (p) => h('tr', null,
            h('td', null, fmtDate(p.created_at)),
            h('td', null, p.tenant_id ? h('code', null, p.tenant_id) : '-'),
            h('td', null, money(p.amount_cents)),
            h('td', null, Pill(p.status)),
            h('td', null, p.external_invoice_id ? h('code', null, p.external_invoice_id) : '-'),
          ),
          emptyText: 'No payment events recorded',
        }),
      );

      return h('div', null,
        h('div', { class: 'drawer-backdrop', onClick: () => clearSelectedCustomer() }),
        h('aside', {
          class: 'customer-drawer',
          role: 'dialog',
          'aria-modal': 'true',
          'aria-label': 'Customer 360',
        },
          h('div', { class: 'customer-drawer-header' },
            h('div', null,
              h('strong', null, 'Customer 360'),
              h('div', { class: 'text-muted text-xxs' },
                c.email || c.user_id || '-',
              ),
            ),
            h('div', { class: 'action-row' },
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
              }, '+30d'),
              h('button', {
                class: 'btn small',
                onClick: () => clearSelectedCustomer(),
              }, 'Close'),
            ),
          ),
          h('div', { class: 'customer-drawer-body' },
            profile,
            h('div', { class: 'customer-drawer-grid' },
              paySummary,
              tenantTable,
            ),
            lifecycleTable,
            authTable,
            paymentTable,
          ),
        ),
      );
    },
  );
}
