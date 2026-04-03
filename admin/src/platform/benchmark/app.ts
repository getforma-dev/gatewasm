/**
 * PLANE 2 — Performance Benchmark
 * USER: Authenticated Member (internal testing)
 * SERVER DATA: window.__FORMA_SERVER_URL__, window.__BENCHMARK_CONFIG__, window.__BENCHMARK_DATA__
 * ENTRY POINT: platform-benchmark.js
 * ROUTE: /platform/benchmark
 */
import { activateIslands, mount } from 'formajs';
import { BenchmarkPage } from './BenchmarkPage';
import { BenchmarkFilterBar } from './BenchmarkFilterBar';
import { BenchmarkDataTable } from './BenchmarkDataTable';
import { PerfPanel, PerfStat } from './PerfPanel';
import { detectWasm } from './store';

detectWasm();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ BenchmarkFilterBar, BenchmarkDataTable, PerfPanel, PerfStat });
} else {
  mount(() => BenchmarkPage(), '#app');
}
