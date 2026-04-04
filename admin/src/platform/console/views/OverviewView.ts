import { h, createSignal } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant,
  selectedTenantStats,
  loading,
  overviewWindow,
  changeOverviewWindow,
  introspectData,
  introspectUnavailable,
  navigateTenantView,
  openIntegrateGuide,
  openSessionInspector,
  updateTenant,
  type OverviewWindow as OverviewWindowValue,
  type TenantActivityItem,
  type TenantStats,
  type TenantStatsSeriesPoint,
} from '../store';

const WINDOW_LABELS: Record<OverviewWindowValue, string> = {
  '24h': '24h',
  '7d': '7d',
  '30d': '30d',
};

export function OverviewView() {
  const tenant = selectedTenant()!;
  const tenantState = () => selectedTenant() || tenant;
  const [hidingChecklist, setHidingChecklist] = createSignal(false);

  function chooseStartingView(nextView: 'integrate' | 'hosted-page') {
    void updateTenant(tenantState().id, { preferred_tab: nextView });
    if (nextView === 'integrate') {
      openIntegrateGuide();
      return;
    }
    navigateTenantView(nextView);
  }

  async function hideChecklist() {
    setHidingChecklist(true);
    await updateTenant(tenantState().id, { onboarding_completed: true });
    setHidingChecklist(false);
  }

  const stats = () => selectedTenantStats();
  const activeWindow = () => stats()?.window || overviewWindow();
  const isBootstrapping = () => loading() && !stats();

  const setupChecklistCard = () => {
    if (tenantState().onboarding_completed) {
      return null;
    }

    const hasLiveSession = !!introspectData();
    const sessionStatus = hasLiveSession
      ? 'Live session'
      : introspectUnavailable()
        ? 'Needs sign-in'
        : 'Checking';
    const preferredTab = tenantState().preferred_tab === 'hosted-page'
      ? 'Hosted Page'
      : tenantState().preferred_tab === 'integrate'
        ? 'Integrate'
        : 'Choose focus';

    return h('div', { class: 'console-card setup-checklist-card' },
      h('div', { class: 'card-label' }, 'Recommended Next Steps'),
      h('h3', { class: 'section-title' }, 'Get the tenant into a working state quickly'),
      h('p', { class: 'card-copy' },
        'This stays lightweight on purpose: claim the URL, preview the hosted flow, create one real session, and decide which workspace you will spend time in most.',
      ),
      h('div', { class: 'setup-checklist' },
        ChecklistItem(
          'Claim the tenant URL you actually want',
          'Your hosted URLs already resolve through /t/' + tenantState().slug + '. Rename it now if you want something cleaner before sharing it.',
          'Ready',
          h('button', {
            type: 'button',
            class: 'btn btn-sm btn-ghost',
            onClick: () => navigateTenantView('settings'),
          }, 'Open Settings'),
        ),
        ChecklistItem(
          'Preview the login page',
          'Review the hosted sign-in surface without following the active redirect into the protected site.',
          'Preview',
          h('a', {
            href: () => '/t/' + tenantState().slug + '/login?preview=1',
            target: '_blank',
            class: 'btn btn-sm btn-ghost',
          }, 'Preview Login'),
        ),
        ChecklistItem(
          'Create one real tenant session',
          hasLiveSession
            ? 'A real tenant-user session is already present in this browser.'
            : introspectUnavailable()
              ? 'Session Inspector will unlock after one tenant sign-in in this browser.'
              : 'The console is checking whether this browser already has a tenant session.',
          sessionStatus,
          hasLiveSession
            ? h('button', {
                type: 'button',
                class: 'btn btn-sm btn-ghost',
                onClick: openSessionInspector,
              }, 'Open Session Inspector')
            : h('a', {
                href: () => '/t/' + tenantState().slug + '/login?redirect=/t/' + tenantState().slug + '/site',
                target: '_blank',
                class: 'btn btn-sm btn-ghost',
              }, 'Start Tenant Sign-In'),
        ),
        ChecklistItem(
          'Choose the area you will live in most',
          'Pin the working area that should open first when you come back to this tenant.',
          preferredTab,
          h('div', { class: 'card-actions' },
            h('button', {
              type: 'button',
              class: 'btn btn-sm btn-ghost',
              onClick: () => chooseStartingView('integrate'),
            }, 'Use Integrate'),
            h('button', {
              type: 'button',
              class: 'btn btn-sm btn-ghost',
              onClick: () => chooseStartingView('hosted-page'),
            }, 'Use Hosted Page'),
          ),
        ),
      ),
      h('div', { class: 'card-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-primary btn-sm',
          disabled: hidingChecklist,
          onClick: hideChecklist,
        }, () => hidingChecklist() ? 'Hiding...' : 'Hide Checklist'),
      ),
    );
  };

  const quickLinksCard = () => h('div', { class: 'console-card' },
    h('div', { class: 'card-label' }, 'Tenant Surfaces'),
    h('div', { class: 'quick-links-grid' },
      QuickLink(
        'Login Page',
        'Preview the hosted sign-in surface without auto-following an existing tenant session.',
        () => '/t/' + tenantState().slug + '/login?preview=1',
        'Preview login',
      ),
      QuickLink(
        'Protected Site',
        'Open the real end-user route. If you are signed out, it starts at the tenant login page.',
        () => '/t/' + tenantState().slug + '/site',
        'Open site',
      ),
      QuickAction(
        'Session Inspector',
        'Jump into Integrate and inspect the live tenant session, callback payload, and JWT claims.',
        openSessionInspector,
        'Inspect session',
      ),
    ),
  );

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Workspace'),
        h('h2', { class: 'view-title' }, () => tenantState().name || 'Overview'),
        h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + tenantState().slug),
      ),
      h('div', { class: 'view-header-actions' },
        HeaderMetric('Users', () => String(stats()?.total_users ?? '-')),
        HeaderMetric('Sessions', () => String(stats()?.active_sessions ?? '-')),
        HeaderMetric('Window', () => WINDOW_LABELS[activeWindow()]),
      ),
    ),
    h('section', { class: 'zone' },
      h('div', { class: 'zone-head zone-head-split' },
        h('div', { class: 'zone-copy-stack' },
          h('div', { class: 'zone-label' }, 'Quick Check'),
          h('p', { class: 'zone-copy' }, 'Core tenant health, authentication momentum, and recent account activity so you can orient quickly before diving deeper.'),
        ),
        TimeWindowPicker(activeWindow, () => loading()),
      ),
      OverviewStatsRow(stats, activeWindow, isBootstrapping),
      h('div', { class: 'overview-insights-grid' },
        OverviewTrendCard(stats, activeWindow, isBootstrapping),
        ProviderMixCard(stats, isBootstrapping),
      ),
      h('div', { class: 'dashboard-shell overview-shell' },
        h('div', { class: 'dashboard-main dashboard-stack' },
          ActivityFeedCard(stats, activeWindow, isBootstrapping),
          setupChecklistCard,
          quickLinksCard,
        ),
        h('div', { class: 'dashboard-aside dashboard-stack' },
          authStatusCard(),
          latestUserCard(stats, isBootstrapping),
        ),
      ),
    ),
  );
}

function TimeWindowPicker(activeWindow: () => OverviewWindowValue, busy: () => boolean) {
  return h('div', { class: 'overview-window-picker', role: 'tablist', 'aria-label': 'Overview time window' },
    ...(['24h', '7d', '30d'] as OverviewWindowValue[]).map((windowValue) =>
      h('button', {
        type: 'button',
        role: 'tab',
        class: () => 'overview-window-chip' + (activeWindow() === windowValue ? ' active' : ''),
        'aria-selected': () => String(activeWindow() === windowValue),
        disabled: busy(),
        onClick: () => changeOverviewWindow(windowValue),
      }, windowValue),
    ),
  );
}

function OverviewStatsRow(
  stats: () => TenantStats | null,
  activeWindow: () => OverviewWindowValue,
  isBootstrapping: () => boolean,
) {
  if (isBootstrapping()) {
    return h('div', { class: 'stats-grid' },
      StatCardSkeleton(),
      StatCardSkeleton(),
      StatCardSkeleton(),
      StatCardSkeleton(),
    );
  }

  return h('div', { class: 'stats-grid stats-grid-four' },
    StatCard('Total Users', () => String(stats()?.total_users ?? '-')),
    StatCard('Active Sessions', () => String(stats()?.active_sessions ?? '-')),
    StatCard(() => 'Logins (' + WINDOW_LABELS[activeWindow()] + ')', () => String(stats()?.logins_in_window ?? '-')),
    StatCard(() => 'Failed Sign-Ins (' + WINDOW_LABELS[activeWindow()] + ')', () => String(stats()?.failed_logins_in_window ?? '-')),
  );
}

function OverviewTrendCard(
  stats: () => TenantStats | null,
  activeWindow: () => OverviewWindowValue,
  isBootstrapping: () => boolean,
) {
  if (isBootstrapping()) {
    return OverviewLoadingCard('Authentication Trend');
  }

  const points = () => stats()?.series || [];
  const hasTrendData = () => points().some((point) => point.logins > 0 || point.sessions_started > 0 || point.failed_logins > 0);
  const maxValue = () => Math.max(1, ...points().map((point) => Math.max(point.logins, point.sessions_started)));

  return h('div', { class: 'console-card overview-chart-card overview-trend-card' },
    h('div', { class: 'overview-card-head' },
      h('div', { class: 'stack-sm' },
        h('div', { class: 'card-label' }, 'Authentication Trend'),
        h('h3', { class: 'section-title' }, () => 'Logins and session starts over ' + WINDOW_LABELS[activeWindow()]),
        h('p', { class: 'card-copy' }, 'Track the moments where users successfully authenticate versus when browsers actually receive a tenant session.'),
      ),
      h('div', { class: 'overview-chart-summary' },
        SummaryMetric('Successful', String(stats()?.logins_in_window ?? 0)),
        SummaryMetric('Failed', String(stats()?.failed_logins_in_window ?? 0)),
      ),
    ),
    !hasTrendData()
      ? OverviewEmptyPanel(
          'No auth traffic yet',
          'As soon as people sign in or sessions start appearing, the trend chart will begin filling in automatically.',
        )
      : h('div', { class: 'overview-trend-shell' },
          h('div', { class: 'overview-trend-legend' },
            LegendSwatch('Logins', 'logins'),
            LegendSwatch('Sessions Started', 'sessions'),
          ),
          h('div', { class: 'overview-trend-chart' },
            () => points().map((point, index) => TrendColumn(point, maxValue(), index, activeWindow())),
          ),
        ),
  );
}

function ProviderMixCard(
  stats: () => TenantStats | null,
  isBootstrapping: () => boolean,
) {
  if (isBootstrapping()) {
    return OverviewLoadingCard('Provider Mix');
  }

  const providers = () => Object.entries(stats()?.providers || {}).sort((a, b) => b[1] - a[1]);
  const total = () => providers().reduce((sum, [, count]) => sum + count, 0);
  const maxCount = () => Math.max(1, ...providers().map(([, count]) => count));

  return h('div', { class: 'console-card overview-chart-card overview-provider-card' },
    h('div', { class: 'card-label' }, 'Provider Mix'),
    h('h3', { class: 'section-title' }, 'How users are getting in'),
    h('p', { class: 'card-copy' }, 'A quick read on whether this tenant is mostly email/password, social login, or a blend of both.'),
    providers().length === 0
      ? OverviewEmptyPanel(
          'No provider data yet',
          'Provider distribution appears after the first account is created or signs in to this tenant.',
        )
      : h('div', { class: 'provider-mix-list' },
          () => providers().map(([provider, count]) =>
            h('div', { class: 'provider-mix-row' },
              h('div', { class: 'provider-mix-copy' },
                h('span', { class: 'provider-mix-name' }, providerLabel(provider)),
                h('span', { class: 'provider-mix-meta' }, Math.round((count / total()) * 100) + '%'),
              ),
              h('div', { class: 'provider-mix-track' },
                h('span', {
                  class: 'provider-mix-fill',
                  style: () => 'width:' + Math.max(12, Math.round((count / maxCount()) * 100)) + '%',
                }),
              ),
              h('span', { class: 'provider-mix-count' }, String(count)),
            ),
          ),
        ),
  );
}

function ActivityFeedCard(
  stats: () => TenantStats | null,
  activeWindow: () => OverviewWindowValue,
  isBootstrapping: () => boolean,
) {
  if (isBootstrapping()) {
    return h('div', { class: 'console-card overview-activity-card' },
      h('div', { class: 'card-label' }, 'Recent Activity'),
      h('h3', { class: 'section-title' }, 'What changed most recently'),
      h('div', { class: 'activity-feed-list' },
        ActivitySkeletonRow(),
        ActivitySkeletonRow(),
        ActivitySkeletonRow(),
      ),
    );
  }

  const items = () => stats()?.activity || [];
  return h('div', { class: 'console-card overview-activity-card' },
    h('div', { class: 'overview-card-head' },
      h('div', { class: 'stack-sm' },
        h('div', { class: 'card-label' }, 'Recent Activity'),
        h('h3', { class: 'section-title' }, 'What changed most recently'),
        h('p', { class: 'card-copy' }, () => 'A running feed of sign-ins, new accounts, and session issuance over the last ' + WINDOW_LABELS[activeWindow()] + '.'),
      ),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        onClick: () => navigateTenantView('users'),
      }, 'Open Users'),
    ),
    items().length === 0
      ? OverviewEmptyPanel(
          'No activity in this window',
          'Try a wider time range or create a fresh sign-in to watch the tenant activity feed come alive.',
        )
      : h('div', { class: 'activity-feed-list' },
          () => items().map((item) => ActivityRow(item)),
        ),
  );
}

function latestUserCard(stats: () => TenantStats | null, isBootstrapping: () => boolean) {
  if (isBootstrapping()) {
    return h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Latest User'),
      Skeleton({ width: '72%', height: '16px' }),
      Skeleton({ width: '52%', height: '12px' }),
    );
  }

  const latestUser = stats()?.latest_user;
  return h('div', { class: 'console-card' },
    h('div', { class: 'card-label' }, 'Latest User'),
    latestUser
      ? h('div', { class: 'stack-sm' },
          h('div', { class: 'card-value card-value-strong' }, latestUser.email),
          h('div', { class: 'card-meta text-muted' },
            new Date(latestUser.created_at).toLocaleString() + ' - ' + latestUser.provider,
          ),
        )
      : h('p', { class: 'card-copy' }, 'User activity will appear here as soon as someone signs in.'),
  );
}

function authStatusCard() {
  const data = introspectData();

  if (data) {
    return h('div', { class: 'console-card overview-auth-card' },
      h('div', { class: 'card-label' }, 'Tenant Auth Status'),
      h('div', { class: 'card-value card-value-strong' }, 'Live session detected'),
      h('p', { class: 'card-copy' },
        data.user.email + ' is authenticated in this browser. Open Session Inspector inside Integrate for the full callback, JWT, and session payload.',
      ),
      h('div', { class: 'detail-grid' },
        OverviewDetailRow('Roles', data.user.roles.join(', ') || 'user'),
        OverviewDetailRow('Session', data.session ? 'Active in this browser' : 'Token only'),
      ),
      h('div', { class: 'card-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-primary btn-sm',
          onClick: openSessionInspector,
        }, 'Open Session Inspector'),
      ),
    );
  }

  if (introspectUnavailable()) {
    return h('div', { class: 'console-card overview-auth-card' },
      h('div', { class: 'card-label' }, 'Tenant Auth Status'),
      h('div', { class: 'card-value card-value-strong' }, 'No live tenant session yet'),
      h('p', { class: 'card-copy' },
        'Platform ownership does not automatically sign you into the tenant app. Complete one tenant sign-in in this browser to unlock Session Inspector inside Integrate.',
      ),
      h('div', { class: 'card-actions' },
        h('a', {
          href: () => '/t/' + selectedTenant()!.slug + '/login?redirect=/t/' + selectedTenant()!.slug + '/site',
          target: '_blank',
          class: 'btn btn-primary btn-sm',
        }, 'Start tenant sign-in'),
      ),
    );
  }

  return h('div', { class: 'console-card overview-auth-card' },
    h('div', { class: 'card-label' }, 'Tenant Auth Status'),
    h('div', { class: 'card-value card-value-strong' }, 'Checking current browser session'),
    h('p', { class: 'card-copy' },
      'If this browser already has a tenant session, the console will surface it automatically in a moment.',
    ),
  );
}

function ChecklistItem(title: string, description: string, status: string, action: HTMLElement | DocumentFragment) {
  return h('div', { class: 'setup-checklist-item' },
    h('div', { class: 'setup-checklist-heading' },
      h('div', { class: 'quick-link-title' }, title),
      h('span', { class: 'setup-status' }, status),
    ),
    h('div', { class: 'setup-checklist-copy' },
      h('div', { class: 'quick-link-description' }, description),
    ),
    h('div', { class: 'setup-checklist-action' }, action),
  );
}

function HeaderMetric(label: string, value: () => string) {
  return h('div', { class: 'header-metric' },
    h('span', { class: 'header-metric-label' }, label),
    h('span', { class: 'header-metric-value' }, value),
  );
}

function SummaryMetric(label: string, value: string) {
  return h('div', { class: 'overview-summary-metric' },
    h('span', { class: 'overview-summary-label' }, label),
    h('span', { class: 'overview-summary-value' }, value),
  );
}

function OverviewDetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}

function QuickLink(title: string, description: string, href: () => string, actionLabel: string) {
  return h('a', {
    href,
    target: '_blank',
    class: 'quick-link-card',
  },
    h('span', { class: 'quick-link-copy' },
      h('span', { class: 'quick-link-title' }, title),
      h('span', { class: 'quick-link-description' }, description),
    ),
    h('span', { class: 'quick-link-footer' }, actionLabel),
  );
}

function QuickAction(title: string, description: string, onClick: () => void, actionLabel: string) {
  return h('button', {
    type: 'button',
    class: 'quick-link-card quick-link-card-button',
    onClick,
  },
    h('span', { class: 'quick-link-copy' },
      h('span', { class: 'quick-link-title' }, title),
      h('span', { class: 'quick-link-description' }, description),
    ),
    h('span', { class: 'quick-link-footer' }, actionLabel),
  );
}

function StatCard(label: string | (() => string), value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, typeof label === 'function' ? label : label),
  );
}

function StatCardSkeleton() {
  return h('div', { class: 'stat-card stat-card-skeleton' },
    Skeleton({ width: '38%', height: '32px' }),
    Skeleton({ width: '62%', height: '12px' }),
  );
}

function OverviewLoadingCard(label: string) {
  return h('div', { class: 'console-card overview-chart-card' },
    h('div', { class: 'card-label' }, label),
    Skeleton({ width: '48%', height: '16px' }),
    Skeleton({ width: '88%', height: '12px' }),
    h('div', { class: 'overview-loading-bars' },
      ...Array.from({ length: 8 }, (_, index) =>
        Skeleton({
          class: 'overview-loading-bar',
          height: 48 + (index % 4) * 18 + 'px',
        }),
      ),
    ),
  );
}

function OverviewEmptyPanel(title: string, description: string) {
  return h('div', { class: 'overview-empty-panel' },
    h('div', { class: 'overview-empty-title' }, title),
    h('p', { class: 'overview-empty-copy' }, description),
  );
}

function LegendSwatch(label: string, tone: 'logins' | 'sessions') {
  return h('span', { class: 'overview-legend-chip' },
    h('span', { class: 'overview-legend-swatch overview-legend-swatch-' + tone }),
    label,
  );
}

function TrendColumn(
  point: TenantStatsSeriesPoint,
  maxValue: number,
  index: number,
  activeWindow: OverviewWindowValue,
) {
  const sessionsHeight = Math.max(8, Math.round((point.sessions_started / maxValue) * 100));
  const loginsHeight = Math.max(10, Math.round((point.logins / maxValue) * 100));
  const showLabel = activeWindow === '24h'
    ? index % 3 === 0 || index === 23
    : activeWindow === '7d'
      ? true
      : index % 5 === 0 || index === 29;

  return h('div', {
    class: 'overview-trend-column' + (showLabel ? ' show-label' : ''),
    title: point.label + ' - ' + point.logins + ' logins, ' + point.sessions_started + ' sessions',
  },
    h('div', { class: 'overview-trend-bars' },
      h('span', {
        class: 'overview-trend-bar overview-trend-bar-sessions',
        style: 'height:' + sessionsHeight + '%',
      }),
      h('span', {
        class: 'overview-trend-bar overview-trend-bar-logins',
        style: 'height:' + loginsHeight + '%',
      }),
    ),
    h('span', { class: 'overview-trend-column-label' }, point.label),
  );
}

function providerLabel(provider: string) {
  if (provider === 'email') return 'Email / Password';
  return provider.charAt(0).toUpperCase() + provider.slice(1);
}

function ActivityRow(item: TenantActivityItem) {
  const title = activityTitle(item);
  return h('div', { class: 'activity-feed-row' },
    h('div', { class: 'activity-feed-dot activity-feed-dot-' + item.kind }),
    h('div', { class: 'activity-feed-copy' },
      h('div', { class: 'activity-feed-head' },
        h('span', { class: 'activity-feed-title' }, title),
        h('span', { class: 'activity-feed-time' }, new Date(item.created_at).toLocaleString()),
      ),
      h('div', { class: 'activity-feed-meta' },
        item.email + (item.provider ? ' - ' + providerLabel(item.provider) : ''),
      ),
      h('p', { class: 'activity-feed-description' }, item.description),
    ),
  );
}

function ActivitySkeletonRow() {
  return h('div', { class: 'activity-feed-row activity-feed-row-skeleton' },
    h('div', { class: 'activity-feed-dot' }),
    h('div', { class: 'activity-feed-copy' },
      Skeleton({ width: '42%', height: '12px' }),
      Skeleton({ width: '58%', height: '10px' }),
      Skeleton({ width: '84%', height: '10px' }),
    ),
  );
}

function activityTitle(item: TenantActivityItem) {
  if (item.kind === 'user_created') return 'New account created';
  if (item.kind === 'login_failed') return 'Failed sign-in attempt';
  if (item.kind === 'session_started') return 'Browser session issued';
  return 'Successful sign-in';
}
