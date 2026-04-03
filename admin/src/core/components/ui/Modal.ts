import { h, createShow, onCleanup } from 'formajs';
import { Icon } from './Icon';

interface ModalProps {
  open: () => boolean;
  onClose: () => void;
  title: string;
  children: () => HTMLElement | HTMLElement[];
}

export function Modal(props: ModalProps): HTMLElement {
  const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') props.onClose(); };
  document.addEventListener('keydown', handleKey);
  onCleanup(() => document.removeEventListener('keydown', handleKey));

  return createShow(props.open, () =>
    h('div', { class: 'modal-overlay animate-fade', onClick: (e: Event) => {
      if ((e.target as HTMLElement).classList.contains('modal-overlay')) props.onClose();
    }},
      h('div', { class: 'modal-content' },
        h('div', { class: 'modal-header' },
          h('h2', { class: 'modal-title' }, props.title),
          h('button', { class: 'modal-close', onClick: props.onClose },
            Icon({ name: 'close', size: 18 }),
          ),
        ),
        h('div', { class: 'modal-body' }, props.children()),
      ),
    ),
  ) as HTMLElement;
}
