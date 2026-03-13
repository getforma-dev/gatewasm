import { activateIslands } from 'formajs';
import { BenchmarkPage } from './BenchmarkPage';
import { FilterBar } from './FilterBar';
import { BenchmarkDataTable } from './DataTable';
import { PerfPanel, PerfStat } from './PerfPanel';
import { detectWasm } from './store';

// BenchmarkPage import: SSR compiler traces this for IR generation.
// At runtime, esbuild tree-shakes it — only activateIslands() runs.
void BenchmarkPage;

detectWasm();

activateIslands({
  FilterBar,
  BenchmarkDataTable,
  PerfPanel,
  PerfStat,
});
