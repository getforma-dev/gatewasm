import { h } from 'formajs';
import type { PagerProps } from '../types';

export function Pager(props: PagerProps) {
  return h('div', { class: 'pager' },
    h('button', {
      class: 'btn small',
      disabled: () => props.page().offset === 0,
      onClick: () => props.onNavigate('prev'),
    }, 'Prev'),
    h('span', { class: 'pager-info' }, () => {
      const p = props.page();
      const t = props.total();
      const start = Math.min(p.offset + 1, t);
      const end = Math.min(p.offset + p.limit, t);
      return start + '\u2013' + end + ' of ' + t;
    }),
    h('button', {
      class: 'btn small',
      disabled: () => props.page().offset + props.page().limit >= props.total(),
      onClick: () => props.onNavigate('next'),
    }, 'Next'),
  );
}
