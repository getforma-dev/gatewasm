import { h, createShow } from 'formajs';
import {
  selectedTenant, selectedTenantStats, loadTenantDetail,
  introspectData, introspectToken, introspectUnavailable,
  loadIntrospect, startVisibilityRefresh,
} from '../store';
import { CallbackPreview } from '../../../core/components/CallbackPreview';
import { UserProfile, SessionInfo } from '../../../core/components/AuthDataCards';
import { JwtViewer } from '../../../core/components/JwtViewer';

export function OverviewView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

  loadTenantDetail(tenant.id);
  loadIntrospect(tenant.slug);
  const cleanup = startVisibilityRefresh(tenant.slug);

  // ── Quick Check Zone ──────────────────────────────────────────────────

  const quickCheck = h('div', { class: 'zone' },
    h('div', { class: 'zone-label' }, 'Quick Check'),
    h('div', { class: 'stats-grid' },
      StatCard('Total Users', () => String(selectedTenantStats()?.total_users ?? '\u2014')),
      StatCard('Active Sessions', () => String(selectedTenantStats()?.active_sessions ?? '\u2014')),
      StatCard('Logins (24h)', () => String(selectedTenantStats()?.logins_24h ?? '\u2014')),
    ),
    createShow(
      () => selectedTenantStats()?.latest_user != null,
      () => {
        const lu = selectedTenantStats()!.latest_user!;
        return h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Latest User'),
          h('div', { class: 'card-value' }, lu.email),
          h('div', { class: 'card-meta text-muted' },
            new Date(lu.created_at).toLocaleString() + ' \u2014 ' + lu.provider,
          ),
        );
      },
    ),
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Quick Links'),
      h('div', { class: 'quick-links' },
        h('a', { href: '/t/' + tenant.slug + '/login', target: '_blank', class: 'btn-link' }, 'Login Page'),
        h('a', { href: '/t/' + tenant.slug + '/site', target: '_blank', class: 'btn-link' }, 'Your Gated Site'),
        h('a', { href: '/t/' + tenant.slug + '/sandbox', target: '_blank', class: 'btn-link' }, 'Console Sandbox'),
      ),
    ),
  );

  // ── Integration Reference Zone ────────────────────────────────────────

  const authPrompt = h('div', { class: 'auth-prompt' },
    h('p', { class: 'auth-prompt-text' },
      'Sign in to your gated app to see live auth data. This shows you exactly what your end users experience and what your app receives.',
    ),
    h('a', {
      href: '/t/' + tenant.slug + '/login?redirect=/t/' + tenant.slug + '/sandbox',
      target: '_blank',
      class: 'btn btn-primary',
    }, 'Sign in to ' + (tenant.name || tenant.slug)),
  );

  const integrationRef = h('div', { class: 'zone' },
    h('div', { class: 'zone-label' }, 'Integration Reference'),
    createShow(
      () => introspectData() != null && introspectToken() != null,
      () => {
        const d = introspectData()!;
        const token = introspectToken()!;
        return h('div', { class: 'integration-ref-content' },
          CallbackPreview({ data: d, slug: tenant.slug, serverUrl }),
          UserProfile(d),
          SessionInfo(d),
          JwtViewer({ token, slug: tenant.slug, serverUrl }),
        );
      },
    ),
    createShow(
      () => introspectUnavailable(),
      () => authPrompt,
    ),
  );

  // ── Root ───────────────────────────────────────────────────────────────

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, () => selectedTenant()?.name || 'Overview'),
      h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + (selectedTenant()?.slug || '')),
    ),
    quickCheck,
    integrationRef,
  );
}

function StatCard(label: string, value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, label),
  );
}
