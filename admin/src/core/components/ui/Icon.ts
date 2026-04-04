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

  if (typeof document !== 'undefined') {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', String(size));
    svg.setAttribute('height', String(size));
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('aria-hidden', 'true');
    svg.setAttribute('class', ('icon ' + (props.class || '')).trim());
    svg.innerHTML = content;
    return svg as unknown as HTMLElement;
  }

  return h('svg', {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    'aria-hidden': 'true',
    class: ('icon ' + (props.class || '')).trim(),
  }) as HTMLElement;
}
