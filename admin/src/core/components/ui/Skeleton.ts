import { h } from 'formajs';

interface SkeletonProps {
  width?: string;
  height?: string;
  radius?: string;
  class?: string;
}

/** Single skeleton pulse block. */
export function Skeleton(props: SkeletonProps): HTMLElement {
  return h('div', {
    class: ('skeleton ' + (props.class || '')).trim(),
    style: [
      props.width ? 'width:' + props.width : '',
      props.height ? 'height:' + props.height : '',
      props.radius ? 'border-radius:' + props.radius : '',
    ].filter(Boolean).join(';'),
  }) as HTMLElement;
}

/** Skeleton table — header + N rows matching real table shape. */
export function SkeletonTable(props: { cols?: number; rows?: number }): HTMLElement {
  const cols = props.cols || 4;
  const rows = props.rows || 5;

  return h('div', { class: 'skeleton-table' },
    h('div', { class: 'skeleton-row skeleton-header' },
      ...Array.from({ length: cols }, () =>
        h('div', { class: 'skeleton', style: 'height:12px;flex:1' })
      ),
    ),
    ...Array.from({ length: rows }, () =>
      h('div', { class: 'skeleton-row' },
        ...Array.from({ length: cols }, () =>
          h('div', { class: 'skeleton', style: 'height:14px;flex:1' })
        ),
      ),
    ),
  ) as HTMLElement;
}

/** Skeleton stat card grid — placeholder cards. */
export function SkeletonStats(props: { count?: number }): HTMLElement {
  const count = props.count || 4;
  return h('div', { class: 'stat-grid' },
    ...Array.from({ length: count }, () =>
      h('div', { class: 'stat-card' },
        h('div', { class: 'skeleton', style: 'height:12px;width:60%' }),
        h('div', { class: 'skeleton', style: 'height:20px;width:40%;margin-top:8px' }),
      ),
    ),
  ) as HTMLElement;
}
