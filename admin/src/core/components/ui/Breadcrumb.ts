import { h } from 'formajs';
import { Icon } from './Icon';

interface BreadcrumbSegment {
  label: string;
  onClick?: () => void;
}

interface BreadcrumbProps {
  segments: () => BreadcrumbSegment[];
}

export function Breadcrumb(props: BreadcrumbProps): HTMLElement {
  return h('nav', { class: 'breadcrumb', 'aria-label': 'Breadcrumb' },
    () => {
      const segs = props.segments();
      const nodes: (HTMLElement | null)[] = [];
      segs.forEach((seg, i) => {
        if (i > 0) {
          nodes.push(Icon({ name: 'chevron-right', size: 14, class: 'breadcrumb-sep' }));
        }
        const isLast = i === segs.length - 1;
        const clickable = !!seg.onClick && !isLast;
        nodes.push(
          h('button', {
            type: 'button',
            class: isLast ? 'breadcrumb-link breadcrumb-current' : 'breadcrumb-link',
            disabled: !clickable,
            'aria-current': isLast ? 'page' : undefined,
            ...(clickable ? { onClick: seg.onClick } : {}),
          }, seg.label),
        );
      });
      return nodes;
    },
  ) as HTMLElement;
}
