import { h } from 'formajs';
import { ICONS } from './icon-paths';

interface IconProps {
  name: string;
  size?: number;
  class?: string;
}

export function Icon(props: IconProps): HTMLElement {
  const size = props.size || 20;
  const content = ICONS[props.name];

  if (!content) {
    if (typeof console !== 'undefined') console.warn('Icon "' + props.name + '" not found');
    return h('span', { class: 'icon-missing', style: 'width:' + size + 'px;height:' + size + 'px;display:inline-block' }) as HTMLElement;
  }

  return h('svg', {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    'aria-hidden': 'true',
    class: ('icon ' + (props.class || '')).trim(),
    innerHTML: content,
  }) as HTMLElement;
}
