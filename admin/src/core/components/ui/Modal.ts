import { h, createShow, onCleanup } from 'formajs';
import { Icon } from './Icon';

interface ModalProps {
  open: () => boolean;
  onClose: () => void;
  title: string;
  children: () => HTMLElement | HTMLElement[];
  overlayClassName?: string;
  contentClassName?: string;
  bodyClassName?: string;
}

export function Modal(props: ModalProps): HTMLElement {
  const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') props.onClose(); };
  document.addEventListener('keydown', handleKey);
  onCleanup(() => document.removeEventListener('keydown', handleKey));

  return createShow(props.open, () =>
    h('div', { class: 'modal-overlay animate-fade' + (props.overlayClassName ? ' ' + props.overlayClassName : ''), onClick: (e: Event) => {
      if ((e.target as HTMLElement).classList.contains('modal-overlay')) props.onClose();
    }},
      h('div', { class: 'modal-content' + (props.contentClassName ? ' ' + props.contentClassName : '') },
        h('div', { class: 'modal-header' },
          h('h2', { class: 'modal-title' }, props.title),
          h('button', { class: 'modal-close', onClick: props.onClose },
            Icon({ name: 'close', size: 18 }),
          ),
        ),
        h('div', { class: 'modal-body' + (props.bodyClassName ? ' ' + props.bodyClassName : '') }, props.children()),
      ),
    ),
  ) as unknown as HTMLElement;
}
