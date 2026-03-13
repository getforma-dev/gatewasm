// FilterBar.ts — Island: Search, sort, and row count controls
// Sort, filter, and direction changes are instant (client-side derived signal).
// Only row count increases trigger a server fetch.
import { h } from 'formajs';
import {
  query, setQuery,
  sortCol, setSortCol,
  sortDir, setSortDir,
  rowCount, setRowCount,
  fetchData,
} from './store';

const SORT_COLUMNS = [
  { value: 'id', label: 'ID' },
  { value: 'name', label: 'Name' },
  { value: 'email', label: 'Email' },
  { value: 'role', label: 'Role' },
  { value: 'status', label: 'Status' },
  { value: 'score', label: 'Score' },
];

export function FilterBar() {
  return h('div', { class: 'benchmark-filter-bar' },
    h('div', { class: 'filter-group' },
      h('label', { class: 'filter-label', for: 'bench-search' }, 'Search'),
      h('input', {
        id: 'bench-search',
        type: 'text',
        class: 'filter-input',
        placeholder: 'Filter rows...',
        value: query,
        onInput: (e: Event) => {
          setQuery((e.target as HTMLInputElement).value);
          // No fetchData — derived signal recomputes instantly
        },
      }),
    ),
    h('div', { class: 'filter-group' },
      h('label', { class: 'filter-label', for: 'bench-sort' }, 'Sort by'),
      h('select', {
        id: 'bench-sort',
        class: 'filter-select',
        value: sortCol,
        onChange: (e: Event) => {
          setSortCol((e.target as HTMLSelectElement).value);
          // No fetchData — derived signal recomputes instantly
        },
      },
        ...SORT_COLUMNS.map(col => h('option', { value: col.value }, col.label)),
      ),
    ),
    h('div', { class: 'filter-group' },
      h('label', { class: 'filter-label' }, 'Direction'),
      h('button', {
        class: 'filter-btn',
        type: 'button',
        onClick: () => {
          setSortDir(sortDir() === 'asc' ? 'desc' : 'asc');
          // No fetchData — derived signal recomputes instantly
        },
      }, () => sortDir() === 'asc' ? 'ASC' : 'DESC'),
    ),
    h('div', { class: 'filter-group' },
      h('label', { class: 'filter-label', for: 'bench-rows' }, 'Rows'),
      h('select', {
        id: 'bench-rows',
        class: 'filter-select',
        value: () => String(rowCount()),
        onChange: (e: Event) => {
          setRowCount(Number((e.target as HTMLSelectElement).value));
          // fetchData only fetches from server if we need more data
          fetchData();
        },
      },
        h('option', { value: '100' }, '100'),
        h('option', { value: '500' }, '500'),
        h('option', { value: '1000' }, '1,000'),
        h('option', { value: '5000' }, '5,000'),
        h('option', { value: '10000' }, '10,000'),
        h('option', { value: '50000' }, '50,000'),
        h('option', { value: '100000' }, '100,000'),
      ),
    ),
  );
}
