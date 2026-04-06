import { h } from 'formajs';

export function FAuthMark(size: number = 28) {
  const height = Math.round(size * 34 / 28);
  return h('svg', {
    width: String(size), height: String(height),
    viewBox: '0 0 100 120', xmlns: 'http://www.w3.org/2000/svg',
    class: 'fauth-mark',
  },
    h('defs', null,
      h('linearGradient', { id: 'fauth-g', x1: '50', y1: '0', x2: '50', y2: '120', gradientUnits: 'userSpaceOnUse' },
        h('stop', { offset: '0%', 'stop-color': '#c4a265', 'stop-opacity': '0.14' }),
        h('stop', { offset: '100%', 'stop-color': '#c4a265', 'stop-opacity': '0.03' }),
      ),
    ),
    h('path', { d: 'M6 4L94 4L100 10L100 78L50 118L0 78L0 10Z', fill: 'url(#fauth-g)' }),
    h('path', { d: 'M6 4L94 4L100 10L100 78L50 118L0 78L0 10Z', fill: 'none', stroke: '#c4a265', 'stroke-width': '2.5' }),
    h('text', { x: '50', y: '72', 'text-anchor': 'middle', 'font-family': "'Orbitron', sans-serif", 'font-weight': '900', 'font-size': '50', fill: '#c4a265' }, 'F'),
  );
}

export function userDisplayName(name: string | null | undefined, email: string): string {
  const trimmed = (name || '').trim();
  if (trimmed) return trimmed;
  const local = email.split('@')[0] || email;
  return local.replace(/[._-]+/g, ' ');
}

export function userInitials(name: string | null | undefined, email: string): string {
  const parts = userDisplayName(name, email).split(/\s+/).filter(Boolean);
  if (parts.length === 0) return email.slice(0, 2).toUpperCase();
  return parts.slice(0, 2).map((p) => p[0] || '').join('').toUpperCase() || email.slice(0, 2).toUpperCase();
}
