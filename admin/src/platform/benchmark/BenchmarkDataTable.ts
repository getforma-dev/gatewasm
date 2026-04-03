// BenchmarkDataTable.ts — Island: Data table with createList + createShow
// Sort header clicks are instant (derived signal). No server fetch.
import { h, createList, createShow } from 'formajs';
import { rows, loading, error, sortCol, setSortCol, sortDir, setSortDir } from './store';
import type { BenchmarkRow } from './store';

const COLUMNS = [
  { key: 'id', label: 'ID' },
  { key: 'name', label: 'Name' },
  { key: 'email', label: 'Email' },
  { key: 'role', label: 'Role' },
  { key: 'status', label: 'Status' },
  { key: 'score', label: 'Score' },
];

function sortIndicator(colKey: string): () => string {
  return () => {
    if (sortCol() !== colKey) return '';
    return sortDir() === 'asc' ? ' \u25B2' : ' \u25BC';
  };
}

function handleSort(col: string) {
  if (sortCol() === col) {
    // Same column — toggle direction
    setSortDir(sortDir() === 'asc' ? 'desc' : 'asc');
  } else {
    setSortCol(col);
    setSortDir('asc');
  }
  // No fetchData — derived signal recomputes instantly
}

export function BenchmarkDataTable() {
  const thead = h('thead', null,
    h('tr', null,
      ...COLUMNS.map(col =>
        h('th', {
          class: 'benchmark-th',
          onClick: () => handleSort(col.key),
        },
          col.label,
          h('span', { class: 'sort-indicator' }, sortIndicator(col.key)),
        ),
      ),
    ),
  );

  const tbody = h('tbody', null,
    createList(
      rows,
      (row: BenchmarkRow) => String(row.id),
      (row: BenchmarkRow) =>
        h('tr', null,
          h('td', null, String(row.id)),
          h('td', null, row.name),
          h('td', null, row.email),
          h('td', null, row.role),
          h('td', null, row.status),
          h('td', null, row.score),
        ),
    ),
  );

  const table = h('div', { class: 'table-wrap benchmark-table-wrap' },
    h('table', { class: 'benchmark-table' }, thead, tbody),
  );

  return h('div', { class: 'benchmark-data-table' },
    createShow(
      loading,
      () => h('div', { class: 'benchmark-loading' }, 'Loading...'),
      () => createShow(
        () => error() !== null,
        () => h('div', { class: 'benchmark-error' }, () => error() || 'Error loading data'),
        () => createShow(
          () => rows().length > 0,
          () => table,
          () => h('div', { class: 'benchmark-empty' }, 'No matching rows'),
        ),
      ),
    ),
  );
}
