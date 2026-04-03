import { h } from 'formajs';
import { Breadcrumb } from './Breadcrumb';

interface TopbarProps {
  breadcrumbs: () => { label: string; onClick?: () => void }[];
  actions?: () => HTMLElement | HTMLElement[] | null;
}

export function Topbar(props: TopbarProps): HTMLElement {
  return h('header', { class: 'topbar' },
    h('div', { class: 'topbar-left' },
      Breadcrumb({ segments: props.breadcrumbs }),
    ),
    h('div', { class: 'topbar-right' },
      props.actions ? props.actions() : null,
    ),
  ) as HTMLElement;
}
