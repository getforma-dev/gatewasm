import { h } from 'formajs';

export function StatCard(props: { label: string; value: () => string | number; hint?: string }) {
  return h('article', { class: 'stat-card' },
    h('h3', { class: 'stat-label' }, props.label),
    h('div', { class: 'stat-value' }, () => String(props.value())),
    props.hint ? h('small', { class: 'stat-hint' }, props.hint) : null,
  );
}
