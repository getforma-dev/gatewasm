import { h, createSignal, createShow, onCleanup } from 'formajs';

interface DropdownItem {
  id: string;
  label: string;
  icon?: string;
  danger?: boolean;
}

interface DropdownProps {
  trigger: HTMLElement;
  items: DropdownItem[];
  onSelect: (id: string) => void;
}

export function Dropdown(props: DropdownProps): HTMLElement {
  const [open, setOpen] = createSignal(false);

  const handleClickOutside = (e: Event) => {
    const target = e.target as Node;
    if (container && !container.contains(target)) setOpen(false);
  };
  document.addEventListener('click', handleClickOutside);
  onCleanup(() => document.removeEventListener('click', handleClickOutside));

  const container = h('div', { class: 'dropdown' },
    h('div', { class: 'dropdown-trigger', onClick: () => setOpen((o: boolean) => !o) },
      props.trigger,
    ),
    createShow(open, () =>
      h('div', { class: 'dropdown-menu' },
        ...props.items.map(item =>
          h('button', {
            class: 'dropdown-item' + (item.danger ? ' danger' : ''),
            onClick: () => { props.onSelect(item.id); setOpen(false); },
          }, item.label),
        ),
      ),
    ),
  ) as HTMLElement;

  return container;
}
