import type { HeaderProps } from '../types';

import { h, createShow } from 'formajs';

export function Header(props: HeaderProps) {
  return h('header', { class: 'header' },
    h('div', null,
      h('h1', null, props.title),
      props.subtitle ? h('p', { class: 'text-muted' }, props.subtitle) : null,
    ),
    props.actions ? h('div', { class: 'header-actions' }, ...props.actions) : null,
  );
}
