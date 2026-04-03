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
        nodes.push(
          h(isLast ? 'span' : 'button', {
            class: isLast ? 'breadcrumb-current' : 'breadcrumb-link',
            ...(seg.onClick && !isLast ? { onClick: seg.onClick } : {}),
          }, seg.label),
        );
      });
      return nodes;
    },
  ) as HTMLElement;
}
