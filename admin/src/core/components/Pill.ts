import { h } from 'formajs';
import { normalizePillStatus } from '../utils';

export function Pill(status: string) {
  const normalized = normalizePillStatus(status);
  return h('span', { class: 'pill ' + normalized }, status);
}
