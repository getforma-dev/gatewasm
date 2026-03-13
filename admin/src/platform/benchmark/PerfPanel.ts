// PerfPanel.ts — Island: Performance metrics display
import { h } from 'formajs';
import { renderTime, rows, wasmStatus, cacheStatus } from './store';

export function PerfStat(label: string, value: () => string) {
  return h('div', { class: 'perf-stat' },
    h('span', { class: 'perf-stat-label' }, label),
    h('span', { class: 'perf-stat-value' }, value),
  );
}

export function PerfPanel() {
  return h('div', { class: 'benchmark-perf-panel' },
    h('h3', { class: 'perf-panel-title' }, 'Performance'),
    h('div', { class: 'perf-stats-grid' },
      PerfStat('Render time', () => renderTime() + ' ms'),
      PerfStat('Row count', () => String(rows().length)),
      PerfStat('WASM', wasmStatus),
      PerfStat('Cache', cacheStatus),
    ),
  );
}
