import { h } from 'formajs';

export function Card(props: { class?: string }, ...children: unknown[]) {
  return h('article', { class: 'card' + (props.class ? ' ' + props.class : '') }, ...children);
}

export function CardHead(...children: unknown[]) {
  return h('div', { class: 'card-head' }, ...children);
}
