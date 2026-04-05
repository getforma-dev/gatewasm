import { h, createShow } from 'formajs';
import { Icon } from './Icon';
import type { ToastController } from '../../hooks/createToast';

export function ToastView(controller: ToastController) {
  return createShow(
    () => controller.toast().length > 0,
    () => h('div', {
      class: 'toast-stack',
      'aria-live': 'polite',
      'aria-atomic': 'false',
    }, () => controller.toast().map((item) =>
      h('div', {
        class: 'toast ' + item.kind,
        key: item.id,
        onClick: () => controller.dismiss(item.id),
      },
        h('div', { class: 'toast-message' }, item.message),
        h('button', {
          type: 'button',
          class: 'toast-close',
          'aria-label': 'Dismiss notification',
          onClick: (event: Event) => {
            event.stopPropagation();
            controller.dismiss(item.id);
          },
        }, Icon({ name: 'close', size: 14 })),
      ),
    )),
  );
}
