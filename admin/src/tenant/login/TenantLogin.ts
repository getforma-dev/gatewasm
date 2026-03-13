/**
 * TenantLogin.ts — SSR root component for tenant login page
 *
 * Static shell (page wrapper) + island function call.
 * Compiler Rule 11 detects TenantLoginCard() as an opaque call
 * and emits ISLAND_START/ISLAND_END opcodes.
 */
import { h } from 'formajs';
import { TenantLoginCard } from './TenantLoginCard';

export function TenantLogin() {
  return h('div', { class: 'tl-page' },
    TenantLoginCard(),
  );
}
