import { h, createSignal, onCleanup } from 'formajs';
import { copyText } from '../../utils';
import { Icon } from './Icon';

interface CopyButtonProps {
  text: string | (() => string);
  label?: string;
  copiedLabel?: string;
  class?: string;
  iconOnly?: boolean;
  title?: string;
  onCopied?: () => void;
}

export function CopyButton(props: CopyButtonProps): HTMLElement {
  const [copied, setCopied] = createSignal(false);
  let resetTimer: ReturnType<typeof setTimeout> | null = null;

  function readText(): string {
    return typeof props.text === 'function' ? props.text() : props.text;
  }

  async function handleCopy(event: Event) {
    event.preventDefault();
    event.stopPropagation();
    await copyText(readText());
    props.onCopied?.();
    setCopied(true);
    if (resetTimer) clearTimeout(resetTimer);
    resetTimer = setTimeout(() => setCopied(false), 1400);
  }

  onCleanup(() => {
    if (resetTimer) {
      clearTimeout(resetTimer);
    }
  });

  return h('button', {
    type: 'button',
    class: () => (props.class || 'btn btn-sm btn-ghost copy-btn') + (props.iconOnly ? ' copy-btn-icon-only' : ''),
    title: () => copied() ? (props.copiedLabel || 'Copied') : (props.title || props.label || 'Copy'),
    'aria-label': () => copied() ? (props.copiedLabel || 'Copied') : (props.title || props.label || 'Copy'),
    onClick: handleCopy,
  },
    () => copied()
      ? Icon({ name: 'check-circle', size: 14, class: 'copy-btn-icon' })
      : Icon({ name: 'copy', size: 14, class: 'copy-btn-icon' }),
    () => props.iconOnly ? null : (copied() ? (props.copiedLabel || 'Copied') : (props.label || 'Copy')),
  ) as HTMLElement;
}
