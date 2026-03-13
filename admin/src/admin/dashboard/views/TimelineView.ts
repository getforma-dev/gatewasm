// ---------------------------------------------------------------------------
// TimelineView.ts — Audit timeline tab: filters + event table + pager
// ---------------------------------------------------------------------------

import { h } from 'formajs';
import { FilterBar } from '../../../core/components/FilterBar';
import { DataTable } from '../../../core/components/DataTable';
import { Pager } from '../../../core/components/Pager';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { fmtDate } from '../../../core/utils';
import type { OwnerStore } from '../store';
import type { TimelineEvent, FilterField } from '../../../core/types';

// -- Component --------------------------------------------------------------

export function TimelineView(os: OwnerStore) {
  const { store, timelineFilters, timelinePager } = os;
  const { applyTimelineFilters, clearTimelineFilters, changeTimelinePage } = os;

  // Filter fields
  const fields: FilterField[] = [
    {
      name: 'actor', label: 'Actor', type: 'text',
      value: () => timelineFilters.filters().actor,
      onChange: (v) => timelineFilters.update({ actor: v } as any),
      placeholder: 'Actor email or user id',
    },
    {
      name: 'action', label: 'Action', type: 'text',
      value: () => timelineFilters.filters().action,
      onChange: (v) => timelineFilters.update({ action: v } as any),
      placeholder: 'e.g. customer.lifecycle.updated',
    },
    {
      name: 'resource', label: 'Resource', type: 'text',
      value: () => timelineFilters.filters().resource,
      onChange: (v) => timelineFilters.update({ resource: v } as any),
      placeholder: 'Resource',
    },
    {
      name: 'request_id', label: 'Request ID', type: 'text',
      value: () => timelineFilters.filters().request_id,
      onChange: (v) => timelineFilters.update({ request_id: v } as any),
      placeholder: 'Request id',
    },
    {
      name: 'from', label: 'From', type: 'datetime',
      value: () => timelineFilters.filters().from,
      onChange: (v) => timelineFilters.update({ from: v } as any),
    },
    {
      name: 'to', label: 'To', type: 'datetime',
      value: () => timelineFilters.filters().to,
      onChange: (v) => timelineFilters.update({ to: v } as any),
    },
  ];

  const filterBar = Panel({},
    PanelTitle(
      h('span', null, 'Audit Timeline Filters'),
      h('span', { class: 'badge' }, () => 'Total: ' + (store.timelineTotal ?? 0)),
    ),
    FilterBar({
      id: 'timeline-filters',
      fields,
      onApply: () => applyTimelineFilters(),
      onClear: () => clearTimelineFilters(),
      onSave: () => timelineFilters.save(),
      onLoad: () => { timelineFilters.load(); applyTimelineFilters(); },
    }),
  );

  const table = DataTable<TimelineEvent>({
    headers: ['At', 'Actor', 'Action', 'Resource', 'Request ID', 'Source', 'Result'],
    rows: () => store.timelineEvents,
    keyFn: (ev: TimelineEvent) => ev.created_at + ':' + (ev.action || '') + ':' + (ev.request_id || ''),
    renderRow: (ev) => h('tr', null,
      h('td', null, fmtDate(ev.created_at)),
      h('td', null, ev.actor || '-'),
      h('td', null, h('code', null, ev.action || '-')),
      h('td', null, h('code', null, ev.resource || '-')),
      h('td', null, h('code', null, ev.request_id || '-')),
      h('td', null, ev.source || '-'),
      h('td', null, ev.result || '-'),
    ),
    emptyText: 'No timeline events for current filters',
  });

  const pager = Pager({
    total: () => store.timelineTotal,
    page: () => timelinePager.page(),
    onNavigate: (dir) => changeTimelinePage(dir),
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' }, filterBar),
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Timeline'),
          h('span', { class: 'text-muted' }, 'Owner events + audit log'),
        ),
        table,
        pager,
      ),
    ),
  );
}
