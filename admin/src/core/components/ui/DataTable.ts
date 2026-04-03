import type { DataTableProps } from './types';

import { h, createList, createShow } from 'formajs';

export function DataTable<T>(props: DataTableProps<T>): HTMLElement {
  const thead = h('thead', null,
    h('tr', null,
      ...props.headers.map((hdr: string) => h('th', null, hdr)),
    ),
  );

  const tbody = h('tbody', null,
    createList(
      props.rows,
      props.keyFn,
      (row: T) => props.renderRow(row),
    ),
  );

  const table = h('div', { class: 'table-wrap' },
    h('table', null, thead, tbody),
  );

  const empty = h('div', { class: 'empty empty-state' },
    props.emptyText || 'No data',
  );

  return h('div', null,
    createShow(
      () => props.rows().length > 0,
      () => table,
      () => empty,
    ),
  ) as HTMLElement;
}
