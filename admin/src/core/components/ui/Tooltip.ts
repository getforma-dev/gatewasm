import { h } from 'formajs';

interface TooltipProps {
  text: string;
  position?: 'right' | 'top' | 'bottom';
  class?: string;
}

/**
 * Wraps children in a container that shows a tooltip on hover.
 * Usage: Tooltip({ text: 'Overview' }, h(Icon, { name: 'dashboard' }))
 */
export function Tooltip(props: TooltipProps, ...children: any[]): HTMLElement {
  return h('div', { class: ('tooltip-wrap ' + (props.class || '')).trim() },
    ...children,
    h('span', {
      class: 'tooltip tooltip-' + (props.position || 'right'),
    }, props.text),
  ) as HTMLElement;
}
