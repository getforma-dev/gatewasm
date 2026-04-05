import { h } from 'formajs';
import { CopyButton } from './CopyButton';

export function CodeBlock(code: string) {
  return h('div', { class: 'relative' },
    h('pre', { class: 'code-block' }, code),
    CopyButton({
      text: code,
      class: 'btn small code-block-copy',
      label: 'Copy',
      copiedLabel: 'Copied',
    }),
  );
}
