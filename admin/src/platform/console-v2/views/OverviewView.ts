import { h, createShow } from 'formajs';
import { selectedTenant, selectedTenantStats, loadTenantDetail, type Tenant, type TenantStats } from '../store';

export function OverviewView() {
  const tenant = selectedTenant()!;
  loadTenantDetail(tenant.id);

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, () => selectedTenant()?.name || 'Overview'),
      h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + (selectedTenant()?.slug || '')),
    ),
    h('div', { class: 'stats-grid' },
      StatCard('Total Users', () => String(selectedTenantStats()?.total_users ?? '—')),
      StatCard('Active Sessions', () => String(selectedTenantStats()?.active_sessions ?? '—')),
      StatCard('Logins (24h)', () => String(selectedTenantStats()?.logins_24h ?? '—')),
    ),
    createShow(
      () => selectedTenantStats()?.latest_user != null,
      () => {
        const lu = selectedTenantStats()!.latest_user!;
        return h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Latest User'),
          h('div', { class: 'card-value' }, lu.email),
          h('div', { class: 'card-meta text-muted' },
            new Date(lu.created_at).toLocaleString() + ' — ' + lu.provider,
          ),
        );
      },
    ),
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Quick Links'),
      h('div', { class: 'quick-links' },
        h('a', { href: () => '/t/' + (selectedTenant()?.slug || '') + '/login', target: '_blank', class: 'btn-link' }, 'Login Page'),
        h('a', { href: () => '/t/' + (selectedTenant()?.slug || '') + '/sandbox', target: '_blank', class: 'btn-link' }, 'Sandbox'),
      ),
    ),
  );
}

function StatCard(label: string, value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, label),
  );
}
