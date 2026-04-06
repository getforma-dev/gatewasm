import { h, createSignal, createShow } from 'formajs';
import { currentUser, loadLoginHistory, loadMfaStatus, signout } from '../store';

export function AccountSettingsView() {
  const [loginHistory, setLoginHistory] = createSignal<any[]>([]);
  const [_mfaStatus, setMfaStatus] = createSignal<any>(null);
  const [historyLoaded, setHistoryLoaded] = createSignal(false);

  // Load data on mount
  loadLoginHistory().then((h) => { setLoginHistory(h); setHistoryLoaded(true); });
  loadMfaStatus().then((s) => setMfaStatus(s));

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'Settings'),
        h('p', { class: 'view-subtitle text-muted' }, 'Manage your account profile and security'),
      ),
    ),

    // -- Profile Section --
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Profile'),
      createShow(
        () => currentUser() != null,
        () => {
          const user = currentUser()!;
          return h('div', { class: 'detail-grid' },
            DetailRow('Email', user.email),
            DetailRow('Name', user.name || '(not set)'),
            DetailRow('Email Verified', user.email_verified ? 'Yes' : 'No'),
            DetailRow('Auth Method', user.auth_method),
            DetailRow('Roles', user.roles.join(', ') || 'none'),
          );
        },
      ),
    ),

    // -- MFA Section --
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Multi-Factor Authentication'),
      createShow(
        () => currentUser() != null,
        () => {
          const user = currentUser()!;
          return h('div', null,
            h('div', { class: 'detail-grid' },
              DetailRow('MFA Enabled', user.mfa_enabled ? 'Yes' : 'No'),
              DetailRow('Current Session MFA', user.mfa_verified ? 'Verified' : 'Not verified'),
            ),
            !user.mfa_enabled
              ? h('p', { class: 'card-copy', style: 'margin-top: var(--space-sm)' },
                  'MFA is not yet configured. Use the tenant login flow to set up TOTP-based multi-factor authentication.',
                )
              : null,
          );
        },
      ),
    ),

    // -- Login History --
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Recent Login History'),
      createShow(
        () => !historyLoaded(),
        () => h('p', { class: 'card-copy' }, 'Loading...'),
      ),
      createShow(
        () => historyLoaded() && loginHistory().length === 0,
        () => h('p', { class: 'card-copy' }, 'No login history available.'),
      ),
      createShow(
        () => historyLoaded() && loginHistory().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => loginHistory().slice(0, 10).map((entry: any) =>
            h('div', { class: 'user-row' },
              h('div', { class: 'user-row-summary' },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' },
                    entry.email || 'Unknown',
                    entry.success
                      ? h('span', { class: 'badge badge-subtle' }, 'Success')
                      : h('span', { class: 'badge badge-danger' }, 'Failed'),
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    (entry.ip_address || 'Unknown IP') + ' \u00b7 ' + new Date(entry.created_at).toLocaleString(),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    ),

    // -- Danger Zone --
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Danger Zone'),
      h('p', { class: 'card-copy' },
        'Deleting your account is permanent. All tenants you own will need a new owner or will be deleted.',
      ),
      h('p', { class: 'card-copy text-muted', style: 'margin-top: var(--space-xs)' },
        'Account deletion requires a separate confirmation flow that is not yet available in this console.',
      ),
      h('div', { class: 'card-actions', style: 'margin-top: var(--space-sm)' },
        h('button', {
          class: 'btn btn-sm btn-ghost',
          style: 'color: var(--danger)',
          onClick: () => void signout(),
        }, 'Sign Out'),
      ),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
