import { h } from 'formajs';

export function Panel(props?: { class?: string }, ...children: unknown[]) {
  return h('section', { class: 'panel' + (props?.class ? ' ' + props.class : '') }, ...children);
}

export function PanelTitle(...children: unknown[]) {
  return h('div', { class: 'panel-title' }, ...children);
}
