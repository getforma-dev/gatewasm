import { activateIslands, mount } from 'formajs';
import { BenchmarkPage } from './BenchmarkPage';
import { FilterBar } from './FilterBar';
import { BenchmarkDataTable } from './DataTable';
import { PerfPanel, PerfStat } from './PerfPanel';
import { detectWasm } from './store';

detectWasm();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ FilterBar, BenchmarkDataTable, PerfPanel, PerfStat });
} else {
  mount(() => BenchmarkPage(), '#app');
}
