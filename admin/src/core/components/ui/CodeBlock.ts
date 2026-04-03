import { copyText } from '../../utils';

import { h } from 'formajs';

export function CodeBlock(code: string) {
  return h('div', { class: 'relative' },
    h('pre', { class: 'code-block' }, code),
    h('button', {
      class: 'btn small code-block-copy',
      onClick: () => copyText(code),
    }, 'Copy'),
  );
}
