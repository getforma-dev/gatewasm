import { h } from 'formajs';
import { Icon } from './Icon';

interface EmptyStateProps {
  icon?: string;
  title: string;
  description?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState(props: EmptyStateProps): HTMLElement {
  return h('div', { class: 'empty-state-designed' },
    props.icon ? Icon({ name: props.icon, size: 48, class: 'empty-state-icon' }) : null,
    h('h3', { class: 'empty-state-title' }, props.title),
    props.description
      ? h('p', { class: 'empty-state-desc' }, props.description)
      : null,
    props.action
      ? h('button', {
          class: 'btn btn-primary mt-md',
          onClick: props.action.onClick,
        },
          Icon({ name: 'plus', size: 16 }),
          h('span', null, props.action.label),
        )
      : null,
  ) as HTMLElement;
}
