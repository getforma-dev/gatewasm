// store.ts — Shared reactive state for benchmark islands
// ES module singleton: all islands import from this file, sharing the same signals.
//
// Sort, filter, and direction operate client-side on the rawRows dataset.
// Only row count changes that exceed the current dataset trigger a server fetch.
import { createSignal } from 'formajs';

export interface BenchmarkRow {
  id: number;
  name: string;
  email: string;
  role: string;
  status: string;
  score: number;
}

// --- Server-injected data ---
const serverUrl: string =
  (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const benchConfig: { rows?: number; sort?: string; dir?: string; search?: string } =
  (window as any).__BENCHMARK_CONFIG__ || {};
const serverData: BenchmarkRow[] =
  (window as any).__BENCHMARK_DATA__ || [];

// --- Raw dataset (only changes when we need MORE data from the server) ---
const [rawRows, setRawRows] = createSignal<BenchmarkRow[]>(serverData);

// --- User control signals ---
export const [query, setQuery] = createSignal(benchConfig.search || '');
export const [sortCol, setSortCol] = createSignal(benchConfig.sort || 'id');
export const [sortDir, setSortDir] = createSignal(benchConfig.dir || 'asc');
export const [rowCount, setRowCount] = createSignal(benchConfig.rows || 1000);
export const [loading, setLoading] = createSignal(false);
export const [error, setError] = createSignal<string | null>(null);
export const [renderTime, setRenderTime] = createSignal('0');
export const [wasmStatus, setWasmStatus] = createSignal('checking...');
export const [cacheStatus, setCacheStatus] = createSignal('none');

// --- Derived signal: sorted + filtered view of rawRows ---
// Recomputes automatically when any dependency (rawRows, query, sortCol,
// sortDir, rowCount) changes. No network request. No flash.
export function rows(): BenchmarkRow[] {
  let result = rawRows();
  const count = rowCount();

  // Filter
  const q = query().toLowerCase();
  if (q) {
    result = result.filter(r =>
      r.name.toLowerCase().includes(q) ||
      r.email.toLowerCase().includes(q) ||
      r.role.includes(q) ||
      r.status.includes(q)
    );
  }

  // Slice to requested count (only if we have more than needed)
  if (result.length > count) {
    result = result.slice(0, count);
  }

  // Sort
  const col = sortCol() as keyof BenchmarkRow;
  const asc = sortDir() === 'asc';
  result = [...result].sort((a, b) => {
    const av = a[col];
    const bv = b[col];
    const cmp = av < bv ? -1 : av > bv ? 1 : 0;
    return asc ? cmp : -cmp;
  });

  return result;
}

// --- WASM detection ---
export function detectWasm() {
  const wasmConfig = (window as any).__FORMA_WASM__;
  if (wasmConfig) {
    setWasmStatus('available');
  } else {
    setWasmStatus('not loaded');
  }
}

// --- Fetch: only when we need MORE data than currently cached ---
export async function fetchData() {
  const needed = rowCount();
  if (needed <= rawRows().length) {
    // We already have enough data — derived signal handles the rest
    const start = performance.now();
    // Force a re-read of rows() to trigger any downstream effects
    rows();
    setRenderTime((performance.now() - start).toFixed(1));
    return;
  }

  setLoading(true);
  const start = performance.now();

  try {
    const res = await fetch(`${serverUrl}/platform/api/benchmark?rows=${needed}`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    setRawRows(data.rows || []);
    setError(null);
  } catch (e) {
    setError(e instanceof Error ? e.message : 'Fetch failed');
  }

  setRenderTime((performance.now() - start).toFixed(1));
  setLoading(false);
}
