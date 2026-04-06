import { h, createSignal, createShow } from 'formajs';
import { currentUser, loadLoginHistory, loadMfaStatus, signout, myPasskeys, loadMyPasskeys, renameMyPasskey, deleteMyPasskey, type MyPasskey } from '../store';

export function AccountSettingsView() {
  const [loginHistory, setLoginHistory] = createSignal<any[]>([]);
  const [_mfaStatus, setMfaStatus] = createSignal<any>(null);
  const [historyLoaded, setHistoryLoaded] = createSignal(false);

  // Load data on mount
  loadLoginHistory().then((h) => { setLoginHistory(h); setHistoryLoaded(true); });
  loadMfaStatus().then((s) => setMfaStatus(s));
  loadMyPasskeys();

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

    // ── Passkeys Section ──────────────────────────────────────────────
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Passkeys'),
      createShow(
        () => myPasskeys().length === 0,
        () => h('p', { class: 'card-copy' }, 'No passkeys registered. Use your tenant login flow to register WebAuthn credentials.'),
      ),
      createShow(
        () => myPasskeys().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => myPasskeys().map((pk: MyPasskey) => PasskeyRow(pk)),
        ),
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

function PasskeyRow(pk: MyPasskey) {
  const [editing, setEditing] = createSignal(false);
  const [newName, setNewName] = createSignal(pk.name);

  return h('div', { class: 'user-row' },
    h('div', { class: 'user-row-summary' },
      h('div', { class: 'user-row-main' },
        createShow(
          () => !editing(),
          () => h('div', null,
            h('div', { class: 'user-email' },
              pk.name,
              pk.is_synced
                ? h('span', { class: 'badge badge-subtle' }, 'Synced')
                : h('span', { class: 'badge badge-subtle' }, 'Device-bound'),
            ),
            h('div', { class: 'user-meta text-muted' },
              'Registered ' + new Date(pk.created_at).toLocaleDateString(),
              pk.last_used_at
                ? ' \u00b7 Last used ' + new Date(pk.last_used_at).toLocaleDateString()
                : ' \u00b7 Never used',
            ),
          ),
        ),
        createShow(
          editing,
          () => h('div', { class: 'invite-form-row' },
            h('input', {
              type: 'text',
              class: 'search-input',
              value: newName,
              onInput: (e: Event) => setNewName((e.target as HTMLInputElement).value),
            }),
            h('button', {
              class: 'btn btn-sm btn-primary',
              onClick: async () => {
                await renameMyPasskey(pk.id, newName());
                setEditing(false);
              },
            }, 'Save'),
            h('button', {
              class: 'btn btn-sm btn-ghost',
              onClick: () => { setEditing(false); setNewName(pk.name); },
            }, 'Cancel'),
          ),
        ),
      ),
      h('div', { class: 'user-actions' },
        h('button', {
          class: 'btn btn-sm btn-ghost',
          onClick: () => setEditing(!editing()),
        }, 'Rename'),
        h('button', {
          class: 'btn btn-sm btn-ghost',
          style: 'color: var(--danger)',
          onClick: () => void deleteMyPasskey(pk.id),
        }, 'Delete'),
      ),
    ),
  );
}
