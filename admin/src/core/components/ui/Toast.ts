import { h, createShow } from 'formajs';
import type { ToastController } from '../../hooks/createToast';

export function ToastView(controller: ToastController) {
  return createShow(
    () => controller.toast() != null,
    () => h('div', {
      class: () => 'toast show ' + (controller.toast()?.kind || 'success'),
      onClick: () => controller.dismiss(),
    }, () => controller.toast()?.message || ''),
  );
}
