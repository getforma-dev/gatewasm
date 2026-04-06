import type { SidebarProps } from './types';

import { h, createSignal } from 'formajs';
import { Icon } from './Icon';

function markupSpan(className: string, markup: string): HTMLElement {
  const el = h('span', { class: className }) as HTMLElement;
  if (typeof document !== 'undefined') {
    el.innerHTML = markup;
  }
  return el;
}

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
      markupSpan('nav-icon', item.icon),
      h('span', { class: 'nav-label-text' }, item.label),
    );
  }

  function NavGroup(label: string | undefined, items: { id: string; label: string; icon: string; danger?: boolean }[]) {
    return h('div', { class: 'sidebar-group' },
      label ? h('div', { class: 'sidebar-group-label' }, label) : null,
      h('nav', { class: 'nav' },
        ...items.map((item) => NavItem(item)),
      ),
    );
  }

  const navGroups = () => {
    const groups = typeof props.groups === 'function' ? props.groups() : props.groups;
    if (groups && groups.length > 0) {
      return groups.map((group) => {
        const items = typeof group.items === 'function' ? group.items() : group.items;
        return NavGroup(group.label, items);
      });
    }

    const items = typeof props.items === 'function' ? props.items() : (props.items || []);
    return [NavGroup(undefined, items)];
  };

  return (
    h('aside', {
      class: () => collapsed() ? 'sidebar-rail collapsed' : 'sidebar-rail',
    },
      h('div', {
        class: () => collapsed() ? 'sidebar collapsed' : 'sidebar',
      },
        h('div', { class: 'sidebar-top' },
          h('div', { class: 'brand' },
            typeof props.brand.mark === 'string'
              ? markupSpan('brand-mark', props.brand.mark)
              : h('span', { class: 'brand-mark' }, props.brand.mark),
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
        h('div', { class: 'sidebar-context-slot' },
          () => props.context ? props.context() : null,
        ),
        h('div', { class: 'sidebar-groups' },
          () => navGroups(),
        ),
        h('div', { class: 'sidebar-spacer' }),
        props.env ? h('div', { class: 'sidebar-env' }, props.env) : null,
        h('nav', { class: 'nav sidebar-footer' },
          ...props.footer.map((item) => NavItem(item)),
        ),
        props.account ? h('div', { class: 'sidebar-account-slot' },
          () => props.account ? props.account() : null,
        ) : null,
      ),
    ) as HTMLElement
  );
}
