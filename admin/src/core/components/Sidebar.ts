import type { SidebarProps } from '../types';

import { h, createSignal } from 'formajs';

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
      h('span', { class: 'nav-label' }, item.label),
    );
  }

  return h('aside', {
    class: () => collapsed() ? 'sidebar collapsed' : 'sidebar',
  },
    // Brand
    h('div', { class: 'brand', onClick: () => setCollapsed((c: boolean) => !c) },
      h('span', { class: 'brand-mark', innerHTML: props.brand.mark }),
      h('div', { class: 'brand-text' },
        h('span', { class: 'brand-name' }, props.brand.name),
        h('span', { class: 'brand-subtitle' }, props.brand.subtitle),
      ),
    ),
    // Main nav
    h('nav', { class: 'nav' },
      () => {
        const items = typeof props.items === 'function' ? props.items() : props.items;
        return items.map((item) => NavItem(item));
      },
    ),
    // Spacer
    h('div', { class: 'sidebar-spacer' }),
    // Environment badge
    props.env ? h('div', { class: 'env-badge' }, props.env) : null,
    // Footer nav
    h('nav', { class: 'nav nav-footer' },
      ...props.footer.map((item) => NavItem(item)),
    ),
  ) as HTMLElement;
}
