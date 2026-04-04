import { h, createEffect, createSignal } from 'formajs';
import { Modal } from '../../core/components/ui/Modal';
import { Icon } from '../../core/components/ui/Icon';

export interface CommandPaletteItem {
  id: string;
  title: string;
  subtitle?: string;
  meta?: string;
  keywords?: string[];
  tone?: 'default' | 'danger';
  onSelect: () => void;
}

export interface CommandPaletteSection {
  id: string;
  label: string;
  items: CommandPaletteItem[];
}

interface CommandPaletteProps {
  open: () => boolean;
  onClose: () => void;
  shortcutLabel: string;
  placeholder?: string;
  sections: () => CommandPaletteSection[];
}

function normalize(input: string): string {
  return input.trim().toLowerCase();
}

function matchesQuery(item: CommandPaletteItem, query: string): boolean {
  if (!query) {
    return true;
  }

  const haystack = [
    item.title,
    item.subtitle || '',
    item.meta || '',
    ...(item.keywords || []),
  ].join(' ').toLowerCase();

  return query
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => haystack.includes(term));
}

export function CommandPalette(props: CommandPaletteProps): HTMLElement {
  const [query, setQuery] = createSignal('');
  const [activeIndex, setActiveIndex] = createSignal(0);
  let inputEl: HTMLInputElement | null = null;

  const visibleSections = () => {
    const q = normalize(query());
    return props.sections()
      .map((section) => ({
        ...section,
        items: section.items.filter((item) => matchesQuery(item, q)),
      }))
      .filter((section) => section.items.length > 0);
  };

  const visibleItems = () => visibleSections().flatMap((section) => section.items);

  createEffect(() => {
    if (props.open()) {
      setTimeout(() => {
        inputEl?.focus();
        inputEl?.select();
      }, 0);
      return;
    }

    setQuery('');
    setActiveIndex(0);
  });

  createEffect(() => {
    const items = visibleItems();
    if (items.length === 0) {
      setActiveIndex(0);
      return;
    }

    if (activeIndex() >= items.length) {
      setActiveIndex(items.length - 1);
    }
  });

  function choose(item: CommandPaletteItem) {
    props.onClose();
    item.onSelect();
  }

  function moveSelection(direction: 1 | -1) {
    const items = visibleItems();
    if (items.length === 0) {
      return;
    }

    const nextIndex = activeIndex() + direction;
    if (nextIndex < 0) {
      setActiveIndex(items.length - 1);
      return;
    }
    if (nextIndex >= items.length) {
      setActiveIndex(0);
      return;
    }

    setActiveIndex(nextIndex);
  }

  function handleInputKeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      moveSelection(1);
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      moveSelection(-1);
      return;
    }

    if (event.key === 'Enter') {
      const item = visibleItems()[activeIndex()];
      if (!item) {
        return;
      }
      event.preventDefault();
      choose(item);
    }
  }

  return Modal({
    open: props.open,
    onClose: props.onClose,
    title: 'Quick Switch',
    contentClassName: 'command-palette-modal',
    bodyClassName: 'command-palette-body',
    children: () => h('div', { class: 'command-palette' },
      h('div', { class: 'command-palette-search' },
        Icon({ name: 'search', size: 18, class: 'command-palette-search-icon' }),
        h('input', {
          ref: (el: HTMLInputElement) => { inputEl = el; },
          type: 'text',
          class: 'command-palette-input',
          placeholder: props.placeholder || 'Search tenants, pages, and tools...',
          value: query,
          onInput: (event: Event) => {
            setQuery((event.target as HTMLInputElement).value);
            setActiveIndex(0);
          },
          onKeydown: handleInputKeydown,
          'aria-label': 'Search tenants, pages, and tools',
          autocomplete: 'off',
          spellcheck: 'false',
        }),
      ),
      h('div', { class: 'command-palette-results' },
        () => {
          const sections = visibleSections();
          if (sections.length === 0) {
            return h('div', { class: 'command-palette-empty' },
              h('div', { class: 'command-palette-empty-title' }, 'No matches yet'),
              h('p', { class: 'command-palette-empty-copy' },
                'Try a tenant name, slug, page, or tool like overview, integrate, session, or settings.',
              ),
            );
          }

          let flatIndex = 0;
          return sections.map((section) => {
            const startIndex = flatIndex;
            flatIndex += section.items.length;

            return h('section', { class: 'command-palette-section', key: section.id },
              h('div', { class: 'command-palette-section-label' }, section.label),
              h('div', { class: 'command-palette-list' },
                ...section.items.map((item, itemIndex) => {
                  const absoluteIndex = startIndex + itemIndex;
                  return h('button', {
                    type: 'button',
                    class: () => {
                      let className = 'command-palette-item';
                      if (activeIndex() === absoluteIndex) {
                        className += ' active';
                      }
                      if (item.tone === 'danger') {
                        className += ' danger';
                      }
                      return className;
                    },
                    onMouseEnter: () => setActiveIndex(absoluteIndex),
                    onClick: () => choose(item),
                  },
                    h('div', { class: 'command-palette-item-copy' },
                      h('div', { class: 'command-palette-item-title-row' },
                        h('span', { class: 'command-palette-item-title' }, item.title),
                        item.meta ? h('span', { class: 'command-palette-item-meta' }, item.meta) : null,
                      ),
                      item.subtitle ? h('div', { class: 'command-palette-item-subtitle' }, item.subtitle) : null,
                    ),
                  );
                }),
              ),
            );
          });
        },
      ),
      h('div', { class: 'command-palette-footer' },
        h('span', { class: 'command-palette-footer-copy' },
          'Press ',
          h('span', { class: 'shortcut-key' }, 'Enter'),
          ' to jump and ',
          h('span', { class: 'shortcut-key' }, props.shortcutLabel),
          ' to reopen anytime.',
        ),
      ),
    ),
  });
}
