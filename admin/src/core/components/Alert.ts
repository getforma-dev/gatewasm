import { h } from 'formajs';

export function Alert(props: { message: () => string | null; variant?: 'error' | 'info' | 'success' }) {
  return h('div', {
    class: () => {
      const msg = props.message();
      const base = 'alert ' + (props.variant || 'error');
      return msg ? base + ' show' : base;
    },
  }, () => props.message() || '');
}
