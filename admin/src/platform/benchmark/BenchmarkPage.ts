// ---------------------------------------------------------------------------
// BenchmarkPage.ts — SSR root component for benchmark page
// Calls island components as functions — compiler Rule 11 detects these
// as opaque calls and emits ISLAND_START/ISLAND_END opcodes.
// ---------------------------------------------------------------------------

import { h } from 'formajs';
import { BenchmarkFilterBar } from './BenchmarkFilterBar';
import { BenchmarkDataTable } from './BenchmarkDataTable';
import { PerfPanel } from './PerfPanel';
import { FAuthMark } from '../console/helpers';

// ── Page component ──────────────────────────────────────────────────────
// Static shell (header, layout, footer) + island function calls.
// The compiler walks this tree: static elements become opcodes,
// function calls become ISLAND_START/ISLAND_END boundaries.

export function BenchmarkPage() {
  return h('div', { class: 'benchmark-page' },

    // ── Brand header (pure static — rendered as opcodes) ──────────
    h('div', { class: 'brand-header' },
      FAuthMark(18),
      h('span', null, 'Forma Auth'),
      h('span', { class: 'brand-separator' }, '/'),
      h('span', { class: 'brand-page-title' }, 'Benchmark'),
    ),

    h('div', { class: 'benchmark-layout' },
      // ── Islands: function calls trigger Rule 11 → ISLAND_START/END ──
      BenchmarkFilterBar(),
      PerfPanel(),
      BenchmarkDataTable(),
    ),

    // ── Footer (pure static) ──────────────────────────────────────
    h('footer', { class: 'page-footer text-center pt-sm' },
      h('div', { class: 'footer-badge inline-flex-center gap-xs text-xxs footer-muted' },
        FAuthMark(16),
        'Forma Auth',
      ),
    ),
  );
}
