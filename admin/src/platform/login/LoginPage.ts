// ---------------------------------------------------------------------------
// LoginPage.ts — SSR root component for platform login page
// Calls island components as functions — compiler Rule 11 detects these
// as opaque calls and emits ISLAND_START/ISLAND_END opcodes.
// ---------------------------------------------------------------------------

import { h } from 'formajs';
import { AuthCard } from './AuthCard';
import { MfaPanel } from './MfaPanel';

// ── Page component ──────────────────────────────────────────────────────
// Static shell (hero, card head, footer) + island function calls.
// The compiler walks this tree: static elements become opcodes,
// function calls become ISLAND_START/ISLAND_END boundaries.

export function LoginPage() {
  return h('div', { class: 'page' },

    // ── Hero (pure static — rendered as opcodes) ──────────────────
    h('section', { class: 'hero', 'aria-hidden': 'true' },
      h('div', { class: 'hero-content' },
        h('span', { class: 'hero-badge' }, 'Forma Auth Control Plane'),
        h('h1', null, 'Operate customer auth infrastructure from one secure surface.'),
        h('p', null,
          'Provider-grade identity for B2B2C. Manage tenant admins, monitor login health, and protect owner access with SSO + MFA.',
        ),
        h('div', { class: 'hero-grid' },
          h('div', { class: 'hero-stat' },
            h('span', { class: 'k' }, 'Auth Plane'),
            h('span', { class: 'v' }, 'Provider + Tenant'),
          ),
          h('div', { class: 'hero-stat' },
            h('span', { class: 'k' }, 'Owner Security'),
            h('span', { class: 'v' }, 'SSO + TOTP MFA'),
          ),
          h('div', { class: 'hero-stat' },
            h('span', { class: 'k' }, 'Session Model'),
            h('span', { class: 'v' }, 'JWT + Rotating Refresh'),
          ),
          h('div', { class: 'hero-stat' },
            h('span', { class: 'k' }, 'Availability'),
            h('span', { class: 'v' }, 'auth.getforma.dev'),
          ),
        ),
      ),
    ),

    // ── Auth section ──────────────────────────────────────────────
    h('section', { class: 'auth' },
      h('div', { class: 'card' },
        // Card head (pure static)
        h('div', { class: 'card-head' },
          h('h2', null, 'Platform Login'),
          h('p', null, 'Sign in to manage your tenants and account settings.'),
        ),

        // Islands: function calls trigger Rule 11 → ISLAND_START/END
        AuthCard(),
        MfaPanel(),

        // "New here?" footer (pure static)
        h('div', {
          class: 'new-here mt-lg divider-top text-center text-sm text-muted-dim',
        },
          'New here? ',
          h('a', { href: '/platform/onboarding' }, 'Start onboarding'),
        ),
      ),
    ),

    // ── Footer (pure static) ──────────────────────────────────────
    h('footer', { class: 'page-footer' },
      h('div', { class: 'footer-badge' },
        h('span', { class: 'brand-mark brand-mark-gradient' }, '\u25C7'),
        'Forma Auth',
      ),
    ),
  );
}
