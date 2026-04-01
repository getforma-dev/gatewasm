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
    return users().filter(u =>
      u.email.toLowerCase().includes(q) || (u.name || '').toLowerCase().includes(q),
    );
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Users'),
      h('p', { class: 'view-subtitle text-muted' }, () => users().length + ' total'),
    ),
    h('div', { class: 'console-card' },
      h('input', {
        type: 'text',
        class: 'search-input',
        placeholder: 'Search by email or name...',
        value: search,
        onInput: (e: Event) => setSearch((e.target as HTMLInputElement).value),
      }),
    ),
    createShow(
      () => usersLoading(),
      () => h('p', { class: 'text-muted', style: { padding: '1rem' } }, 'Loading...'),
    ),
    h('div', { class: 'user-list' },
      () => filteredUsers().map(u =>
        h('div', {
          class: () => 'user-row' + (expandedId() === u.id ? ' expanded' : ''),
          key: u.id,
        },
          h('div', {
            class: 'user-row-summary',
            onClick: () => setExpandedId(expandedId() === u.id ? null : u.id),
          },
            h('div', { class: 'user-email' },
              u.email,
              u.is_locked ? h('span', { class: 'badge badge-danger' }, 'Blocked') : null,
            ),
            h('div', { class: 'user-meta text-muted' },
              (u.provider || 'email') + ' — ' + new Date(u.created_at).toLocaleDateString(),
            ),
          ),
          createShow(
            () => expandedId() === u.id,
            () => h('div', { class: 'user-detail' },
              h('div', { class: 'detail-grid' },
                DetailRow('User ID', u.id),
                DetailRow('Name', u.name || '(not set)'),
                DetailRow('Verified', u.email_verified ? 'Yes' : 'No'),
                DetailRow('Roles', u.roles.join(', ') || 'none'),
              ),
              h('div', { class: 'user-actions' },
                u.is_locked
                  ? h('button', { class: 'btn btn-sm btn-ghost', onClick: () => unblockUser(tenant.id, u.id) }, 'Unblock')
                  : h('button', { class: 'btn btn-sm btn-ghost', onClick: () => blockUser(tenant.id, u.id) }, 'Block'),
                h('button', { class: 'btn btn-sm btn-ghost', onClick: () => revokeUserSessions(tenant.id, u.id) }, 'Revoke Sessions'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'sandbox-detail-row' },
    h('span', { class: 'sandbox-detail-label' }, label),
    h('span', { class: 'sandbox-detail-value' }, value),
  );
}
