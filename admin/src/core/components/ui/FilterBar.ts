import { h } from 'formajs';
import type { FilterBarProps, FilterField } from './types';

function renderFilterField(field: FilterField): HTMLElement {
  if (field.type === 'select') {
    return h('div', { class: 'filter-field' },
      h('label', null, field.label),
      h('select', {
        name: field.name,
        value: field.value,
        onChange: (e: Event) => field.onChange((e.target as HTMLSelectElement).value),
      },
        h('option', { value: '' }, 'All'),
        ...(field.options ? field.options().map((opt) =>
          h('option', { value: opt.value }, opt.label)
        ) : []),
      ),
    );
  }

  if (field.type === 'datetime') {
    return h('div', { class: 'filter-field' },
      h('label', null, field.label),
      h('input', {
        type: 'datetime-local',
        name: field.name,
        value: field.value,
        onInput: (e: Event) => field.onChange((e.target as HTMLInputElement).value),
      }),
    );
  }

  return h('div', { class: 'filter-field' },
    h('label', null, field.label),
    h('input', {
      type: 'text',
      name: field.name,
      value: field.value,
      placeholder: field.placeholder || '',
      onInput: (e: Event) => field.onChange((e.target as HTMLInputElement).value),
    }),
  );
}

export function FilterBar(props: FilterBarProps): HTMLElement {
  return h('div', { class: 'filters' },
    h('div', { class: 'filter-row' },
      ...props.fields.map(renderFilterField),
    ),
    h('div', { class: 'filter-actions' },
      h('button', { class: 'btn small', onClick: props.onApply }, 'Apply'),
      h('button', { class: 'btn small outline', onClick: props.onClear }, 'Clear'),
      props.onSave ? h('button', { class: 'btn small outline', onClick: props.onSave }, 'Save') : null,
      props.onLoad ? h('button', { class: 'btn small outline', onClick: props.onLoad }, 'Load') : null,
      props.onExport ? h('button', { class: 'btn small outline', onClick: props.onExport }, 'Export CSV') : null,
      props.total ? h('span', { class: 'filter-total' }, () => props.total!() + ' results') : null,
    ),
  ) as HTMLElement;
}
