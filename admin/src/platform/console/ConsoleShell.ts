// ---------------------------------------------------------------------------
// ConsoleShell.ts — SSR root component for platform console
// Calls ConsoleApp as island — compiler Rule 11 detects the function call
// and emits ISLAND_START/ISLAND_END opcodes.
// ---------------------------------------------------------------------------

import { h } from 'formajs';
import { ConsoleApp } from './ConsoleApp';

export function ConsoleShell() {
  return h('div', { class: 'console-shell' }, ConsoleApp());
}
