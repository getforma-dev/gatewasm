import { onCleanup } from 'formajs';

interface Shortcut {
  keys: string;
  handler: () => void;
  description?: string;
}

/**
 * Register global keyboard shortcuts.
 * Supports single keys, modifier combos (Meta+k), and sequences (g o).
 * Call inside a component — cleans up automatically via onCleanup.
 */
export function createKeyboard(shortcuts: Shortcut[]) {
  let pendingKey: string | null = null;
  let pendingTimeout: ReturnType<typeof setTimeout> | null = null;

  const handleKeydown = (e: KeyboardEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
      if (e.key !== 'Escape') return;
    }

    for (const s of shortcuts) {
      // Modifier combo: "Meta+k"
      if (s.keys.includes('+')) {
        const parts = s.keys.split('+');
        const mod = parts[0];
        const key = parts[1];
        const modPressed =
          (mod === 'Meta' && e.metaKey) ||
          (mod === 'Ctrl' && e.ctrlKey) ||
          (mod === 'Alt' && e.altKey) ||
          (mod === 'Shift' && e.shiftKey);
        if (modPressed && e.key.toLowerCase() === key.toLowerCase()) {
          e.preventDefault();
          s.handler();
          return;
        }
        continue;
      }

      // Sequence: "g o"
      if (s.keys.includes(' ')) {
        const [first, second] = s.keys.split(' ');
        if (pendingKey === first && e.key.toLowerCase() === second.toLowerCase()) {
          pendingKey = null;
          if (pendingTimeout) clearTimeout(pendingTimeout);
          e.preventDefault();
          s.handler();
          return;
        }
        if (e.key.toLowerCase() === first.toLowerCase()) {
          pendingKey = first;
          if (pendingTimeout) clearTimeout(pendingTimeout);
          pendingTimeout = setTimeout(() => { pendingKey = null; }, 500);
          return;
        }
        continue;
      }

      // Single key
      if (e.key === s.keys || e.key.toLowerCase() === s.keys.toLowerCase()) {
        if (s.keys === '/' || s.keys === '[') e.preventDefault();
        s.handler();
        return;
      }
    }

    pendingKey = null;
  };

  document.addEventListener('keydown', handleKeydown);
  onCleanup(() => {
    document.removeEventListener('keydown', handleKeydown);
    if (pendingTimeout) clearTimeout(pendingTimeout);
  });
}
