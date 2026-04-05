import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant, users, usersLoading,
  blockUser, unblockUser, revokeUserSessions,
} from '../store';

export function UsersView() {
  const tenant = selectedTenant()!;
  const tenantState = () => selectedTenant() || tenant;
  const [search, setSearch] = createSignal('');
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [statusFilter, setStatusFilter] = createSignal<'all' | 'verified' | 'unverified' | 'blocked'>('all');
  const [providerFilter, setProviderFilter] = createSignal<'all' | 'google' | 'github' | 'email'>('all');
  const [sortOrder, setSortOrder] = createSignal<'newest' | 'oldest' | 'email'>('newest');
  const blockedCount = () => users().filter((user) => user.is_locked).length;
  const verifiedCount = () => users().filter((user) => user.email_verified).length;

  const filteredUsers = () => {
    const q = search().toLowerCase();
    let nextUsers = users().filter((user) =>
      user.email.toLowerCase().includes(q) || (user.name || '').toLowerCase().includes(q),
    );

    if (statusFilter() === 'verified') {
      nextUsers = nextUsers.filter((user) => user.email_verified);
    } else if (statusFilter() === 'unverified') {
      nextUsers = nextUsers.filter((user) => !user.email_verified);
    } else if (statusFilter() === 'blocked') {
      nextUsers = nextUsers.filter((user) => user.is_locked);
    }

    if (providerFilter() !== 'all') {
      nextUsers = nextUsers.filter((user) => (user.provider || 'email') === providerFilter());
    }

    if (sortOrder() === 'oldest') {
      nextUsers = [...nextUsers].sort((a, b) => Date.parse(a.created_at) - Date.parse(b.created_at));
    } else if (sortOrder() === 'email') {
      nextUsers = [...nextUsers].sort((a, b) => a.email.localeCompare(b.email));
    } else {
      nextUsers = [...nextUsers].sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
    }

    return nextUsers;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Directory'),
        h('h2', { class: 'view-title' }, 'Users'),
        h('p', { class: 'view-subtitle text-muted' }, () => users().length + ' total users in /t/' + tenantState().slug),
      ),
      h('div', { class: 'view-header-actions' },
        MetricCard('Verified', () => String(verifiedCount())),
        MetricCard('Blocked', () => String(blockedCount())),
      ),
    ),
    h('section', { class: 'console-card list-shell' },
      h('div', { class: 'list-shell-head list-shell-head-users' },
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
          h('div', { class: 'users-filter-groups' },
            h('div', { class: 'users-filter-group' },
              FilterChip('All', () => statusFilter() === 'all', () => setStatusFilter('all')),
              FilterChip('Verified', () => statusFilter() === 'verified', () => setStatusFilter('verified')),
              FilterChip('Unverified', () => statusFilter() === 'unverified', () => setStatusFilter('unverified')),
              FilterChip('Blocked', () => statusFilter() === 'blocked', () => setStatusFilter('blocked')),
            ),
            h('div', { class: 'users-filter-group' },
              FilterChip('Any Provider', () => providerFilter() === 'all', () => setProviderFilter('all')),
              FilterChip('Google', () => providerFilter() === 'google', () => setProviderFilter('google')),
              FilterChip('GitHub', () => providerFilter() === 'github', () => setProviderFilter('github')),
              FilterChip('Email', () => providerFilter() === 'email', () => setProviderFilter('email')),
            ),
            h('select', {
              class: 'filter-select users-sort-select',
              value: sortOrder,
              onChange: (e: Event) => setSortOrder((e.target as HTMLSelectElement).value as 'newest' | 'oldest' | 'email'),
            },
              h('option', { value: 'newest' }, 'Newest first'),
              h('option', { value: 'oldest' }, 'Oldest first'),
              h('option', { value: 'email' }, 'Email A-Z'),
            ),
          ),
        ),
      ),
      createShow(
        () => usersLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '24%', height: '12px' }),
          Skeleton({ width: '88%', height: '12px' }),
          Skeleton({ width: '82%', height: '12px' }),
        ),
      ),
      createShow(
        () => !usersLoading() && filteredUsers().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, search().trim() ? 'No Matches' : 'No Users Yet'),
          h('p', { class: 'card-copy' },
            () => search().trim()
              ? 'Try a different name or email search to find a user.'
              : 'The first tenant members will appear here as soon as someone signs up or completes a login flow.',
          ),
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
                      ? h('button', { class: 'btn btn-sm btn-ghost', onClick: () => unblockUser(tenantState().id, user.id) }, 'Unblock')
                      : h('button', { class: 'btn btn-sm btn-ghost', onClick: () => blockUser(tenantState().id, user.id) }, 'Block'),
                    h('button', { class: 'btn btn-sm btn-ghost', onClick: () => revokeUserSessions(tenantState().id, user.id) }, 'Revoke Sessions'),
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

function FilterChip(label: string, active: () => boolean, onClick: () => void) {
  return h('button', {
    type: 'button',
    class: () => 'list-filter-chip' + (active() ? ' active' : ''),
    onClick,
  }, label);
}

function MetricCard(label: string, value: () => string) {
  return h('div', { class: 'header-metric' },
    h('span', { class: 'header-metric-label' }, label),
    h('span', { class: 'header-metric-value' }, value),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
