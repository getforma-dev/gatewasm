// ---------------------------------------------------------------------------
// InviteIslands.ts — the invite-reactive pieces of the login page, as ISLANDS
//
// On the SSR route, `activateIslands` hydrates only `[data-forma-island]`
// elements — the static shell is never adopted, so any reactive binding left
// in it is dead after load. That is exactly what happened when this page
// first went into SSR_ROUTES: an invited user's `?owner_invite=TOKEN`
// resolved in the store, but the banner, the card heading, and the
// invite-carrying onboarding link all lived in the static shell and never
// updated — the page sat on "Platform Login" as though no invite existed.
//
// Everything that READS inviteInfo/ownerInviteToken therefore lives here, in
// island components. Server-side the compiler inlines each island's subtree
// with the store's folded defaults (no banner, "Platform Login", the plain
// onboarding href), so the no-JS paint is unchanged; client-side the islands
// hydrate and the invite flow works again.
// Verified by: admin/e2e/login.spec.ts > "SSR page reacts to a resolved owner invite"
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { inviteInfo, inviteLoading, ownerInviteToken } from './store';

/** Banner naming the inviter, shown once the invite token resolves. */
export function InviteBanner() {
  // An island root must be an h() tree the walker can follow — a bare
  // createShow return emits an empty island hole with a diagnostic. The
  // wrapper renders in both modes, so CSR and SSR stay structurally equal.
  return h('div', { class: 'invite-banner-slot' },
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
  );
}

/** Card heading + subtitle — swaps to the invite copy when one resolves. */
export function LoginCardHead() {
  return h('div', { class: 'card-head' },
    // Expression bodies, not blocks: the compiler folds a ternary over the
    // store's null default, so the server renders the real heading instead
    // of an empty <h2> until hydration.
    h('h2', null, () =>
      !inviteInfo()
        ? 'Platform Login'
        : inviteInfo()!.user_exists ? 'Sign in to accept invite' : 'Create an account to accept'),
    h('p', null, () =>
      !inviteInfo()
        ? 'Sign in to manage your tenants and account settings.'
        : 'You need a fAuth account to join the team.'),
  );
}

/** "New here?" footer — the onboarding link carries the invite token. */
export function OnboardingCta() {
  return h('div', {
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
  );
}
