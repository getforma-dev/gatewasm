import { h, createShow } from 'formajs';
import {
  selectedTenant, selectedTenantStats, loadTenantDetail,
  introspectData, introspectToken, introspectUnavailable,
  loadIntrospect, startVisibilityRefresh,
} from '../store';
import { CallbackPreview } from '../../../core/components/domain/CallbackPreview';
import { UserProfile, SessionInfo } from '../../../core/components/domain/AuthDataCards';
import { JwtViewer } from '../../../core/components/domain/JwtViewer';

export function OverviewView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

  loadTenantDetail(tenant.id);
  loadIntrospect(tenant.slug);
  startVisibilityRefresh(tenant.slug);

  const latestUserCard = () => {
    const latestUser = selectedTenantStats()?.latest_user;
    return h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Latest User'),
      latestUser
        ? [
            h('div', { class: 'card-value card-value-strong' }, latestUser.email),
            h('div', { class: 'card-meta text-muted' },
              new Date(latestUser.created_at).toLocaleString() + ' - ' + latestUser.provider,
            ),
          ]
        : h('p', { class: 'card-copy' }, 'User activity will appear here as soon as someone signs in.'),
    );
  };

  const quickLinksCard = h('div', { class: 'console-card' },
    h('div', { class: 'card-label' }, 'Quick Links'),
    h('div', { class: 'quick-links-grid' },
      QuickLink(
        'Login Page',
        'Preview the sign-in flow your users see before entering the tenant app.',
        '/t/' + tenant.slug + '/login',
      ),
      QuickLink(
        'Your Gated Site',
        'Open the hosted page that sits behind auth for this tenant.',
        '/t/' + tenant.slug + '/site',
      ),
      QuickLink(
        'Console Sandbox',
        'Jump to the live auth sandbox to inspect tokens, callbacks, and session data.',
        '/t/' + tenant.slug + '/sandbox',
      ),
    ),
  );

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

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Workspace'),
        h('h2', { class: 'view-title' }, () => selectedTenant()?.name || 'Overview'),
        h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + (selectedTenant()?.slug || '')),
      ),
      h('div', { class: 'view-header-actions' },
        HeaderMetric('Users', () => String(selectedTenantStats()?.total_users ?? '-')),
        HeaderMetric('Sessions', () => String(selectedTenantStats()?.active_sessions ?? '-')),
      ),
    ),
    h('section', { class: 'zone' },
      h('div', { class: 'zone-head' },
        h('div', { class: 'zone-label' }, 'Quick Check'),
        h('p', { class: 'zone-copy' }, 'Core tenant health and shortcuts that help you orient quickly before diving deeper.'),
      ),
      h('div', { class: 'stats-grid' },
        StatCard('Total Users', () => String(selectedTenantStats()?.total_users ?? '-')),
        StatCard('Active Sessions', () => String(selectedTenantStats()?.active_sessions ?? '-')),
        StatCard('Logins (24h)', () => String(selectedTenantStats()?.logins_24h ?? '-')),
      ),
      h('div', { class: 'cards-row overview-secondary-grid' },
        latestUserCard,
        quickLinksCard,
      ),
    ),
    h('section', { class: 'zone' },
      h('div', { class: 'zone-head' },
        h('div', { class: 'zone-label' }, 'Integration Reference'),
        h('p', { class: 'zone-copy' }, 'Live request and session data pulled from the tenant auth plane so you can validate real-world behavior.' ),
      ),
      createShow(
        () => introspectData() != null && introspectToken() != null,
        () => {
          const data = introspectData()!;
          const token = introspectToken()!;
          return h('div', { class: 'integration-ref-content' },
            CallbackPreview({ data, slug: tenant.slug, serverUrl }),
            UserProfile(data),
            SessionInfo(data),
            JwtViewer({ token, slug: tenant.slug, serverUrl }),
          );
        },
      ),
      createShow(
        () => introspectUnavailable(),
        () => authPrompt,
      ),
    ),
  );
}

function HeaderMetric(label: string, value: () => string) {
  return h('div', { class: 'header-metric' },
    h('span', { class: 'header-metric-label' }, label),
    h('span', { class: 'header-metric-value' }, value),
  );
}

function QuickLink(title: string, description: string, href: string) {
  return h('a', {
    href,
    target: '_blank',
    class: 'quick-link-card',
  },
    h('span', { class: 'quick-link-title' }, title),
    h('span', { class: 'quick-link-description' }, description),
  );
}

function StatCard(label: string, value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, label),
  );
}
