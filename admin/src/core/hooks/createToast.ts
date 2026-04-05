import { createSignal } from 'formajs';
import type { Toast } from '../types';

export function createToast(dismissMs = 4200) {
  const [toast, setToast] = createSignal<Toast[]>([]);
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  let nextToastId = 1;

  function show(message: string, kind: Toast['kind'] = 'success'): void {
    const id = nextToastId++;
    const entry: Toast = { id, message, kind };

    setToast((items: Toast[]) => [entry, ...items].slice(0, 3));

    if (kind === 'error') {
      return;
    }

    const timeoutMs = kind === 'success' ? 2400 : dismissMs;
    const timer = setTimeout(() => dismiss(id), timeoutMs);
    timers.set(id, timer);
  }

  function dismiss(id?: number): void {
    if (typeof id === 'number') {
      const timer = timers.get(id);
      if (timer) {
        clearTimeout(timer);
        timers.delete(id);
      }
      setToast((items: Toast[]) => items.filter((item) => item.id !== id));
      return;
    }

    const first = toast()[0];
    if (first) {
      dismiss(first.id);
    }
  }

  function dismissAll(): void {
    timers.forEach((timer) => clearTimeout(timer));
    timers.clear();
    setToast([]);
  }

  return { toast, show, dismiss, dismissAll };
}

export type ToastController = ReturnType<typeof createToast>;
