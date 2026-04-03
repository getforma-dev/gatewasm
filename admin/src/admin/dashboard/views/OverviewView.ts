// ---------------------------------------------------------------------------
// OverviewView.ts — Overview tab: stat cards + analytics charts
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { StatCard } from '../../../core/components/ui/StatCard';
import { Panel, PanelTitle } from '../../../core/components/ui/Panel';
import { Badge } from '../../../core/components/ui/Badge';
import { money } from '../../../core/utils';
import type { OwnerStore } from '../store';

// -- Chart bar helper -------------------------------------------------------

function ChartRow(props: { day: () => string; value: () => number; max: () => number }) {
  return h('div', { class: 'chart-row' },
    h('span', null, () => props.day()),
    h('div', { class: 'bar' },
      h('span', {
        style: () => {
          const pct = Math.max(4, Math.round((props.value() / Math.max(props.max(), 1)) * 100));
          return 'width:' + pct + '%';
        },
      }),
    ),
    h('strong', null, () => String(props.value())),
  );
}

// -- Component --------------------------------------------------------------

export function OverviewView(os: OwnerStore) {
  const { store } = os;

  // Stat cards
  const statGrid = h('div', { class: 'stat-grid' },
    StatCard({ label: 'Customers', value: () => store.stats?.customers ?? 0 }),
    StatCard({ label: 'Active', value: () => store.stats?.active_customers ?? 0 }),
    StatCard({ label: 'Suspended', value: () => store.stats?.suspended_customers ?? 0 }),
    StatCard({ label: 'Tenants', value: () => store.stats?.tenants ?? 0 }),
    StatCard({ label: 'Trial Expiring (7d)', value: () => store.stats?.trial_expiring_7d ?? 0 }),
    StatCard({ label: 'Revenue (30d)', value: () => money(store.stats?.revenue_30d_cents ?? 0) }),
  );

  // Signups chart
  const signupsChart = Panel({},
    PanelTitle(
      h('span', null, 'Customer Signups (30d)'),
      h('span', { class: 'text-muted' }, 'Daily'),
    ),
    createShow(
      () => (store.analytics?.customer_signups?.length ?? 0) > 0,
      () => {
        const signups = store.analytics?.customer_signups ?? [];
        const data = signups.slice(-8);
        const maxVal = signups.reduce(
          (mx: number, i: { count: number }) => Math.max(mx, i.count), 1,
        );
        return h('div', { class: 'chart-list' },
          ...data.map((item: { day: string; count: number }) =>
            ChartRow({
              day: () => item.day ?? '-',
              value: () => item.count ?? 0,
              max: () => maxVal,
            }),
          ),
        );
      },
      () => h('div', { class: 'empty' }, 'No data for this window'),
    ),
  );

  // Revenue chart
  const revenueChart = Panel({},
    PanelTitle(
      h('span', null, 'Paid Revenue (30d)'),
      h('span', { class: 'text-muted' }, 'Daily cents'),
    ),
    createShow(
      () => (store.analytics?.revenue_paid_cents?.length ?? 0) > 0,
      () => {
        const rev = store.analytics?.revenue_paid_cents ?? [];
        const data = rev.slice(-8);
        const maxVal = rev.reduce(
          (mx: number, i: { amount_cents: number }) => Math.max(mx, i.amount_cents), 1,
        );
        return h('div', { class: 'chart-list' },
          ...data.map((item: { day: string; amount_cents: number }) =>
            ChartRow({
              day: () => item.day ?? '-',
              value: () => item.amount_cents ?? 0,
              max: () => maxVal,
            }),
          ),
        );
      },
      () => h('div', { class: 'empty' }, 'No data for this window'),
    ),
  );

  // Operating notes
  const notes = Panel({},
    PanelTitle(
      h('span', null, 'Operating Notes'),
      Badge('Provider Plane'),
    ),
    h('div', { class: 'text-muted' },
      'Use ', h('strong', null, 'Customers'),
      ' to suspend/reject/reactivate accounts and extend access. Use ',
      h('strong', null, 'Billing'),
      ' to track receipts and payment statuses. Browser UI uses owner JWT; management keys stay backend-only.',
    ),
  );

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' }, statGrid),
    h('div', { class: 'view-section' },
      h('div', { class: 'split' }, signupsChart, revenueChart),
    ),
    h('div', { class: 'view-section' }, notes),
  );
}
