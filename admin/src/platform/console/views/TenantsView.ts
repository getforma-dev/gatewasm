// ---------------------------------------------------------------------------
// TenantsView.ts — Tenant list with stat cards, data table, and quickstart
// ---------------------------------------------------------------------------

import { StatCard } from '../../../core/components/StatCard';
import { DataTable } from '../../../core/components/DataTable';
import { Badge } from '../../../core/components/Badge';
import { Pill } from '../../../core/components/Pill';
import { Header } from '../../../core/components/Header';
import { fmtDate } from '../../../core/utils';
import { Quickstart } from './Quickstart';
import type { ConsoleStore } from '../store';
import type { Tenant } from '../../../core/types';

import { h, createShow } from 'formajs';

const serverUrl = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

// -- Helpers ----------------------------------------------------------------

function planBadgeVariant(plan: string): 'warning' | 'info' | undefined {
  if (plan === 'trial') return 'warning';
  if (plan === 'paid' || plan === 'pro' || plan === 'enterprise') return 'info';
  return undefined;
}

function healthDot(plan: string, trialEndsAt: string | null): HTMLElement {
  let cls = 'health-dot green';
  let title = 'Active';
  if (plan === 'trial' && trialEndsAt) {
    const now = Date.now();
    const end = new Date(trialEndsAt).getTime();
    if (end < now) {
      cls = 'health-dot red';
      title = 'Trial expired';
    } else if (end <= now + 7 * 24 * 60 * 60 * 1000) {
      cls = 'health-dot yellow';
      title = 'Trial expiring soon';
    }
  }
  return h('span', { class: cls, title });
}

// -- Component --------------------------------------------------------------

export function TenantsView(cs: ConsoleStore) {
  const { store, navigate, loadTenantDetail } = cs;

  // Stat cards
  const stats = h('div', { class: 'stat-grid' },
    StatCard({
      label: 'Total Tenants',
      value: () => store.tenants.length,
    }),
    StatCard({
      label: 'Total Users',
      value: () => {
        let total = 0;
        for (const t of store.tenants) {
          const s = store.tenantStats[t.id];
          if (s) total += s.total_users || 0;
        }
        return total;
      },
    }),
    StatCard({
      label: 'Active Trials',
      value: () => store.tenants.filter((t: Tenant) => t.plan === 'trial').length,
    }),
  );

  // Table
  const table = DataTable<Tenant>({
    headers: ['', 'Name', 'Slug', 'Plan', 'Users', 'Login', 'Created'],
    rows: () => store.tenants,
    keyFn: (t: Tenant) => t.id,
    renderRow: (t: Tenant) => {
      const statsEntry = store.tenantStats[t.id];
      const users = statsEntry ? String(statsEntry.total_users || 0) : '-';
      const loginUrl = serverUrl + '/t/' + encodeURIComponent(t.slug) + '/login';

      return h('tr', {
        class: 'cursor-pointer',
        onClick: () => loadTenantDetail(t.id),
      },
        h('td', null, healthDot(t.plan, t.trial_ends_at)),
        h('td', null, h('strong', null, t.name)),
        h('td', null, h('code', null, t.slug)),
        h('td', null, Badge(t.plan, planBadgeVariant(t.plan))),
        h('td', null, users),
        h('td', null,
          h('a', {
            href: loginUrl,
            target: '_blank',
            rel: 'noopener',
            onClick: (e: Event) => e.stopPropagation(),
          }, loginUrl),
        ),
        h('td', null, fmtDate(t.created_at)),
      );
    },
    emptyText: 'No tenants yet.',
  });

  // Empty-state CTA
  const emptyCta = h('div', { class: 'card text-center p-lg' },
    h('p', null, 'No tenants yet.'),
    h('button', {
      class: 'btn primary',
      onClick: () => navigate('create'),
    }, 'Create Your First Tenant'),
  );

  return h('div', { class: 'content' },
    Header({
      title: 'Tenants',
      actions: [
        h('button', { class: 'btn primary', onClick: () => navigate('create') }, '+ New Tenant'),
      ],
    }),
    stats,
    createShow(
      () => store.tenants.length > 0,
      () => table,
      () => emptyCta,
    ),
    Quickstart(cs),
  );
}
