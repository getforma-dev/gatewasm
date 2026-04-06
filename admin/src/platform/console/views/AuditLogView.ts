import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import { auditLog, auditLogLoading, type AuditLogEntry } from '../store';

export function AuditLogView() {
  const [filter, setFilter] = createSignal<'all' | 'success' | 'failed'>('all');

  const filteredEntries = () => {
    const entries = auditLog();
    if (filter() === 'success') return entries.filter((e) => e.success);
    if (filter() === 'failed') return entries.filter((e) => !e.success);
    return entries;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Organization'),
        h('h2', { class: 'view-title' }, 'Audit Log'),
        h('p', { class: 'view-subtitle text-muted' }, () => filteredEntries().length + ' events'),
      ),
    ),
    h('section', { class: 'console-card list-shell section-box section-box-bottom' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'Login Attempts'),
          h('p', { class: 'card-copy' }, () => filteredEntries().length + ' matching records'),
        ),
        h('div', { class: 'list-shell-tools' },
          h('div', { class: 'users-filter-groups' },
            h('div', { class: 'users-filter-group' },
              FilterChip('All', () => filter() === 'all', () => setFilter('all')),
              FilterChip('Successful', () => filter() === 'success', () => setFilter('success')),
              FilterChip('Failed', () => filter() === 'failed', () => setFilter('failed')),
            ),
          ),
        ),
      ),
      createShow(
        () => auditLogLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '60%', height: '12px' }),
          Skeleton({ width: '88%', height: '12px' }),
          Skeleton({ width: '75%', height: '12px' }),
        ),
      ),
      createShow(
        () => !auditLogLoading() && filteredEntries().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Events'),
          h('p', { class: 'card-copy' }, 'Login attempts will appear here once end users sign in.'),
        ),
      ),
      createShow(
        () => filteredEntries().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => filteredEntries().map((entry) => AuditRow(entry)),
        ),
      ),
    ),
  );
}

function AuditRow(entry: AuditLogEntry) {
  const timeAgo = formatRelativeTime(entry.created_at);
  return h('div', { class: 'user-row' },
    h('div', { class: 'user-row-summary' },
      h('div', { class: 'user-row-main' },
        h('div', { class: 'user-email' },
          entry.email,
          entry.success
            ? h('span', { class: 'badge badge-subtle' }, 'Success')
            : h('span', { class: 'badge badge-danger' }, 'Failed'),
        ),
        h('div', { class: 'user-meta text-muted' },
          (entry.ip_address || 'Unknown IP') + ' \u00b7 ' + timeAgo,
          entry.failure_reason
            ? h('span', { class: 'text-muted' }, ' \u2014 ' + entry.failure_reason)
            : null,
        ),
      ),
    ),
  );
}

function FilterChip(label: string, active: () => boolean, onClick: () => void) {
  return h('button', {
    type: 'button',
    class: () => 'list-filter-chip' + (active() ? ' active' : ''),
    onClick,
  }, label);
}

function formatRelativeTime(iso: string): string {
  const diff = Date.now() - Date.parse(iso);
  if (diff < 60000) return 'Just now';
  if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
  if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
  return new Date(iso).toLocaleDateString();
}
