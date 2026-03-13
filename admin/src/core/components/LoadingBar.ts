import { h } from 'formajs';

export function LoadingBar(visible: () => boolean) {
  return h('div', {
    class: () => visible() ? 'loading-bar active' : 'loading-bar',
  });
}
