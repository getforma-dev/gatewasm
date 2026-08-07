// PerfPanel.ts — Island: Performance metrics display
import { h } from 'formajs';
import { renderTime, rows, wasmStatus, cacheStatus } from './store';

// The stats are written out longhand rather than through a
// PerfStat(label, value) helper: the IR walker cannot see through a function
// PARAMETER, so a param-carried label/value child degraded to a minted
// `island_<n>` shell that no client registry hydrates — every stat rendered
// permanently empty on the SSR'd page. A direct `() => signal()` child binds
// the signal's own slot and hydrates.
export function PerfPanel() {
  return h('div', { class: 'benchmark-perf-panel' },
    h('h3', { class: 'perf-panel-title' }, 'Performance'),
    h('div', { class: 'perf-stats-grid' },
      h('div', { class: 'perf-stat' },
        h('span', { class: 'perf-stat-label' }, 'Render time'),
        h('span', { class: 'perf-stat-value' }, () => renderTime() + ' ms'),
      ),
      h('div', { class: 'perf-stat' },
        h('span', { class: 'perf-stat-label' }, 'Row count'),
        h('span', { class: 'perf-stat-value' }, () => String(rows().length)),
      ),
      h('div', { class: 'perf-stat' },
        h('span', { class: 'perf-stat-label' }, 'WASM'),
        h('span', { class: 'perf-stat-value' }, () => wasmStatus()),
      ),
      h('div', { class: 'perf-stat' },
        h('span', { class: 'perf-stat-label' }, 'Cache'),
        h('span', { class: 'perf-stat-value' }, () => cacheStatus()),
      ),
    ),
  );
}
