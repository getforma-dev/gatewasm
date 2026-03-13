import { h } from 'formajs';
import type { ToastController } from '../hooks/createToast';

export function ToastView(controller: ToastController) {
  return h('div', {
    class: () => {
      const t = controller.toast();
      if (!t) return 'toast';
      return 'toast show ' + t.kind;
    },
    onClick: () => controller.dismiss(),
  }, () => {
    const t = controller.toast();
    return t ? t.message : '';
  });
}
