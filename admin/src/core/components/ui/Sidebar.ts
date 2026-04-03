import type { SidebarProps } from './types';

import { h, createSignal } from 'formajs';
import { Icon } from './Icon';

export function Sidebar(props: SidebarProps): HTMLElement {
  const [collapsed, setCollapsed] = createSignal(false);

  function NavItem(item: { id: string; label: string; icon: string; danger?: boolean }) {
    return h('button', {
      class: () => {
        let cls = 'nav-item';
        if (props.activeView() === item.id) cls += ' active';
        if (item.danger) cls += ' danger';
        return cls;
      },
      onClick: () => props.onNavigate(item.id),
    },
      h('span', { class: 'nav-icon', innerHTML: item.icon }),
      h('span', { class: 'nav-label-text' }, item.label),
    );
  }

  return h('aside', {
    class: () => collapsed() ? 'sidebar collapsed' : 'sidebar',
  },
    h('div', { class: 'sidebar-top' },
      h('div', { class: 'brand' },
        h('span', { class: 'brand-mark', innerHTML: props.brand.mark }),
        h('div', { class: 'brand-text' },
          h('span', { class: 'brand-name' }, props.brand.name),
          h('small', null, props.brand.subtitle),
        ),
      ),
      h('button', {
        type: 'button',
        class: 'sidebar-toggle',
        onClick: () => setCollapsed((c: boolean) => !c),
        'aria-label': 'Toggle sidebar',
        title: 'Toggle sidebar',
      },
        h('span', { class: 'toggle-collapse' }, Icon({ name: 'collapse-left', size: 14 })),
        h('span', { class: 'toggle-expand' }, Icon({ name: 'expand-right', size: 14 })),
      ),
    ),
    h('nav', { class: 'nav' },
      () => {
        const items = typeof props.items === 'function' ? props.items() : props.items;
        return items.map((item) => NavItem(item));
      },
    ),
    h('div', { class: 'sidebar-spacer' }),
    props.env ? h('div', { class: 'sidebar-env' }, props.env) : null,
    h('nav', { class: 'nav sidebar-footer' },
      ...props.footer.map((item) => NavItem(item)),
    ),
  ) as HTMLElement;
}
