import { h } from 'formajs';

export function Badge(label: string, variant?: 'success' | 'warning' | 'danger' | 'info') {
  const cls = variant ? 'badge badge-' + variant : 'badge';
  return h('span', { class: cls }, label);
}
