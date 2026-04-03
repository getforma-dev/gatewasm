import { h, createSignal, createShow } from 'formajs';
import {
  selectedTenant, users, usersLoading, loadUsers,
  blockUser, unblockUser, revokeUserSessions,
} from '../store';

export function UsersView() {
  const tenant = selectedTenant()!;
  const [search, setSearch] = createSignal('');
  const [expandedId, setExpandedId] = createSignal<string | null>(null);

  loadUsers(tenant.id);

  const filteredUsers = () => {
    const q = search().toLowerCase();
    if (!q) return users();
    return users().filter((user) =>
      user.email.toLowerCase().includes(q) || (user.name || '').toLowerCase().includes(q),
    );
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Directory'),
        h('h2', { class: 'view-title' }, 'Users'),
        h('p', { class: 'view-subtitle text-muted' }, () => users().length + ' total users in /t/' + tenant.slug),
      ),
      h('p', { class: 'view-header-note' },
        'Search recent users, inspect account details, and take action on blocking or session revocation without leaving the tenant workspace.',
      ),
    ),
    h('section', { class: 'console-card list-shell' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'Recent Users'),
          h('p', { class: 'card-copy' }, () => filteredUsers().length + ' matching records'),
        ),
        h('div', { class: 'list-shell-tools' },
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: 'Search by email or name...',
            value: search,
            onInput: (e: Event) => setSearch((e.target as HTMLInputElement).value),
          }),
        ),
      ),
      createShow(
        () => usersLoading(),
        () => h('p', { class: 'card-copy' }, 'Loading users...'),
      ),
      createShow(
        () => !usersLoading() && filteredUsers().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Matches'),
          h('p', { class: 'card-copy' }, 'Try a different name or email search to find a user.'),
        ),
      ),
      createShow(
        () => filteredUsers().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => filteredUsers().map((user) =>
            h('div', {
              class: () => 'user-row' + (expandedId() === user.id ? ' expanded' : ''),
              key: user.id,
            },
              h('div', {
                class: 'user-row-summary',
                onClick: () => setExpandedId(expandedId() === user.id ? null : user.id),
              },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' },
                    user.email,
                    user.is_locked ? h('span', { class: 'badge badge-danger' }, 'Blocked') : null,
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    (user.provider || 'email') + ' - ' + new Date(user.created_at).toLocaleDateString(),
                  ),
                ),
                h('div', { class: 'user-row-chevron' }, expandedId() === user.id ? 'Hide' : 'Inspect'),
              ),
              createShow(
                () => expandedId() === user.id,
                () => h('div', { class: 'user-detail' },
                  h('div', { class: 'detail-grid' },
                    DetailRow('User ID', user.id),
                    DetailRow('Name', user.name || '(not set)'),
                    DetailRow('Verified', user.email_verified ? 'Yes' : 'No'),
                    DetailRow('Roles', user.roles.join(', ') || 'none'),
                  ),
                  h('div', { class: 'user-actions' },
                    user.is_locked
                      ? h('button', { class: 'btn btn-sm btn-ghost', onClick: () => unblockUser(tenant.id, user.id) }, 'Unblock')
                      : h('button', { class: 'btn btn-sm btn-ghost', onClick: () => blockUser(tenant.id, user.id) }, 'Block'),
                    h('button', { class: 'btn btn-sm btn-ghost', onClick: () => revokeUserSessions(tenant.id, user.id) }, 'Revoke Sessions'),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
      createShow(
        () => users().length >= 100,
        () => h('div', { class: 'list-truncated' }, '100 most recent users shown'),
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
