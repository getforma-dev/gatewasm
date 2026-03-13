import { createSignal } from 'formajs';
import type { Toast } from '../types';

export function createToast(dismissMs = 4200) {
  const [toast, setToast] = createSignal<Toast | null>(null);
  let timer: ReturnType<typeof setTimeout> | null = null;

  function show(message: string, kind: Toast['kind'] = 'success'): void {
    if (timer) clearTimeout(timer);
    setToast({ message, kind });
    timer = setTimeout(() => setToast(null), dismissMs);
  }

  function dismiss(): void {
    if (timer) clearTimeout(timer);
    setToast(null);
  }

  return { toast, show, dismiss };
}

export type ToastController = ReturnType<typeof createToast>;
