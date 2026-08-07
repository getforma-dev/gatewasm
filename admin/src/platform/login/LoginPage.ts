// ---------------------------------------------------------------------------
// LoginPage.ts — SSR root component for platform login page
// Calls island components as functions — compiler Rule 11 detects these
// as opaque calls and emits ISLAND_START/ISLAND_END opcodes.
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { AuthCard } from './AuthCard';
import { MfaPanel } from './MfaPanel';
import { FAuthMark } from '../console/helpers';
import { inviteInfo, inviteLoading, ownerInviteToken } from './store';

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
      // Invite banner (shown when invite token resolves)
      createShow(
        () => inviteInfo() !== null && !inviteLoading(),
        () => h('div', { class: 'invite-banner' },
          h('p', { class: 'invite-banner-text' },
            () => {
              const info = inviteInfo()!;
              const who = info.inviter_email || 'A team member';
              const role = info.role.charAt(0).toUpperCase() + info.role.slice(1);
              const where = info.tenant_names.length > 0
                ? ' on ' + info.tenant_names.join(', ')
                : '';
              return who + ' invited you to join as ' + role + where;
            },
          ),
        ),
      ),

      h('div', { class: 'card' },
        // Card head (dynamic when invite present)
        h('div', { class: 'card-head' },
          h('h2', null, () => {
            const info = inviteInfo();
            if (!info) return 'Platform Login';
            return info.user_exists ? 'Sign in to accept invite' : 'Create an account to accept';
          }),
          h('p', null, () => {
            const info = inviteInfo();
            if (!info) return 'Sign in to manage your tenants and account settings.';
            return 'You need a fAuth account to join the team.';
          }),
        ),

        // Islands: function calls trigger Rule 11 → ISLAND_START/END
        AuthCard(),
        MfaPanel(),

        // "New here?" footer
        h('div', {
          class: 'new-here mt-lg divider-top text-center text-sm text-muted-dim',
        },
          'New here? ',
          h('a', {
            // Ternary with a literal else-branch, not a concatenation: the
            // compiler folds the branch it can evaluate (the signal defaults to
            // '') into the server-rendered href. Written as
            // `'/platform/onboarding' + (tok ? … : '')` the whole expression is
            // one unresolvable concat, so the attribute was omitted entirely
            // and the no-JS shell shipped a dead link.
            href: () =>
              ownerInviteToken()
                ? '/platform/onboarding?owner_invite=' + encodeURIComponent(ownerInviteToken())
                : '/platform/onboarding',
          }, 'Start onboarding'),
        ),
      ),
    ),

    // ── Footer (pure static) ──────────────────────────────────────
    h('footer', { class: 'page-footer' },
      h('div', { class: 'footer-badge' },
        FAuthMark(16),
        h('span', { class: 'footer-brand-text' },
          h('span', { class: 'footer-brand-forma' }, 'Forma'),
          h('span', { class: 'footer-brand-auth' }, 'Auth'),
        ),
      ),
    ),
  );
}
