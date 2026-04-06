import { h, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant, allSessions, allSessionsLoading,
  revokeUserSessions, loadAllSessions,
  type TenantSession,
} from '../store';

export function SessionsView() {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'End User'),
        h('h2', { class: 'view-title' }, 'Sessions'),
        h('p', { class: 'view-subtitle text-muted' }, () => allSessions().length + ' active sessions'),
      ),
    ),
    h('section', { class: 'console-card list-shell section-box section-box-bottom' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'Active Sessions'),
          h('p', { class: 'card-copy' }, () => allSessions().length + ' sessions across all users'),
        ),
      ),
      createShow(
        () => allSessionsLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '50%', height: '12px' }),
          Skeleton({ width: '80%', height: '12px' }),
        ),
      ),
      createShow(
        () => !allSessionsLoading() && allSessions().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Active Sessions'),
          h('p', { class: 'card-copy' }, 'Sessions will appear here when end users sign in.'),
        ),
      ),
      createShow(
        () => !allSessionsLoading() && allSessions().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => allSessions().map((s) => SessionRow(s)),
        ),
      ),
    ),
  );
}

function SessionRow(s: TenantSession) {
  const tenantId = selectedTenant()?.id || '';
  const timeLeft = formatTimeLeft(Date.parse(s.expires_at));

  return h('div', { class: 'user-row' },
    h('div', { class: 'user-row-summary' },
      h('div', { class: 'user-row-main' },
        h('div', { class: 'user-email' },
          s.email,
          h('span', { class: 'badge badge-subtle' }, (s.auth_provider || s.auth_method) + (s.mfa_verified ? ' + MFA' : '')),
        ),
        h('div', { class: 'user-meta text-muted' },
          (s.login_ip || 'Unknown IP') + ' \u00b7 ' + new Date(s.created_at).toLocaleString(),
          h('span', { class: 'session-card-expiry' }, ' \u00b7 ' + timeLeft),
        ),
      ),
      h('button', {
        class: 'btn btn-sm btn-ghost',
        onClick: async () => {
          await revokeUserSessions(tenantId, s.user_id);
          await loadAllSessions(tenantId);
        },
      }, 'Revoke'),
    ),
  );
}

function formatTimeLeft(expiresMs: number): string {
  const diff = expiresMs - Date.now();
  if (diff <= 0) return 'Expired';
  const days = Math.floor(diff / 86400000);
  const hours = Math.floor((diff % 86400000) / 3600000);
  const mins = Math.floor((diff % 3600000) / 60000);
  if (days > 0) return days + 'd ' + hours + 'h';
  if (hours > 0) return hours + 'h ' + mins + 'm';
  const secs = Math.floor((diff % 60000) / 1000);
  return mins + 'm ' + secs + 's';
}
