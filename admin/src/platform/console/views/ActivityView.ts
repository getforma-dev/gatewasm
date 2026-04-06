import { h, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import { auditLog, auditLogLoading, type AuditLogEntry } from '../store';

export function ActivityView() {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'End User'),
        h('h2', { class: 'view-title' }, 'Activity'),
        h('p', { class: 'view-subtitle text-muted' }, () => auditLog().length + ' authentication events'),
      ),
    ),
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Authentication Timeline'),
      createShow(
        () => auditLogLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '50%', height: '12px' }),
          Skeleton({ width: '80%', height: '12px' }),
          Skeleton({ width: '65%', height: '12px' }),
        ),
      ),
      createShow(
        () => !auditLogLoading() && auditLog().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Activity Yet'),
          h('p', { class: 'card-copy' }, 'Authentication events will appear here as end users sign in.'),
        ),
      ),
      createShow(
        () => !auditLogLoading() && auditLog().length > 0,
        () => h('div', { class: 'activity-timeline' },
          () => auditLog().map((entry) => ActivityEntry(entry)),
        ),
      ),
    ),
  );
}

function ActivityEntry(entry: AuditLogEntry) {
  const time = new Date(entry.created_at).toLocaleString();
  const description = entry.success
    ? entry.email + ' signed in'
    : entry.email + ' failed to sign in';
  const detail = entry.success
    ? 'via ' + (entry.ip_address || 'unknown IP')
    : (entry.failure_reason || 'unknown reason') + ' \u00b7 ' + (entry.ip_address || 'unknown IP');

  return h('div', { class: 'activity-entry' },
    h('div', { class: 'activity-dot-col' },
      h('span', { class: entry.success ? 'activity-dot activity-dot-success' : 'activity-dot activity-dot-fail' }),
    ),
    h('div', { class: 'activity-content' },
      h('div', { class: 'activity-desc' }, description),
      h('div', { class: 'activity-detail text-muted' }, detail),
    ),
    h('div', { class: 'activity-time text-muted' }, time),
  );
}
