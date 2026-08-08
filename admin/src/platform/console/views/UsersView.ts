import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import {
  selectedTenant, users, usersLoading,
  blockUser, unblockUser, revokeUserSessions,
  loadUserSessions, userSessions, userSessionsLoading, setUserSessions,
  openSessionInspector,
  invites, invitesLoading, sendInvite, revokeInvite,
  loadUserDetail, userDetail, setUserDetail,
  canEdit,
  type UserSession, type TenantInvite,
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
        h('div', { class: 'view-eyebrow' }, 'Organization'),
        h('h2', { class: 'view-title' }, 'Members'),
        h('p', { class: 'view-subtitle text-muted' }, () => users().length + ' total members in /t/' + tenantState().slug),
      ),
      h('div', { class: 'view-header-actions' },
        MetricCard('Verified', () => String(verifiedCount())),
        MetricCard('Blocked', () => String(blockedCount())),
      ),
    ),
    InvitesPanel(tenantState().id),
    h('section', { class: 'console-card list-shell' },
      h('div', { class: 'list-shell-head list-shell-head-users' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'Recent Members'),
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
          h('div', { class: 'card-label' }, search().trim() ? 'No Matches' : 'No Members Yet'),
          h('p', { class: 'card-copy' },
            () => search().trim()
              ? 'Try a different name or email search to find a member.'
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
                onClick: () => {
                  const nextId = expandedId() === user.id ? null : user.id;
                  setExpandedId(nextId);
                  if (nextId) {
                    void loadUserSessions(tenantState().id, user.id);
                    void loadUserDetail(tenantState().id, user.id);
                  } else {
                    setUserSessions([]);
                    setUserDetail(null);
                  }
                },
              },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' },
                    h('span', { class: 'truncate' }, user.email),
                    user.is_locked ? h('span', { class: 'badge badge-danger' }, 'Blocked') : null,
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    (user.provider || 'email') + ' \u00b7 ' + new Date(user.created_at).toLocaleDateString(),
                    user.active_sessions > 0
                      ? h('span', { class: 'badge badge-subtle' }, user.active_sessions + ' active ' + (user.active_sessions === 1 ? 'session' : 'sessions'))
                      : null,
                  ),
                ),
                h('div', { class: 'user-row-chevron' }, expandedId() === user.id ? 'Hide' : 'Inspect'),
              ),
              createShow(
                () => expandedId() === user.id,
                () => h('div', { class: 'user-detail' },
                  // ── Two-column layout: profile + sessions ──────────
                  h('div', { class: 'user-detail-columns' },
                    // Left: profile
                    h('div', { class: 'user-detail-profile' },
                      h('div', { class: 'card-label' }, 'Profile'),
                      h('div', { class: 'detail-grid' },
                        DetailRow('User ID', user.id),
                        DetailRow('Name', user.name || '(not set)'),
                        DetailRow('Verified', user.email_verified ? 'Yes' : 'No'),
                        DetailRow('Roles', user.roles.join(', ') || 'none'),
                      ),
                      createShow(
                        () => userDetail() != null && userDetail()!.id === user.id,
                        () => {
                          const d = userDetail()!;
                          return h('div', { class: 'detail-grid' },
                            DetailRow('Login Attempts', String(d.login_attempts)),
                            DetailRow('Lock Status', d.locked_until ? 'Locked until ' + new Date(d.locked_until).toLocaleString() : 'Not locked'),
                            DetailRow('Providers', d.providers.join(', ') || 'none'),
                            DetailRow('Permissions', d.permissions.join(', ') || 'none'),
                          );
                        },
                      ),
                    ),
                    // Right: sessions
                    h('div', { class: 'user-detail-sessions' },
                      h('div', { class: 'card-label' },
                        () => user.active_sessions > 0
                          ? 'Active Sessions (' + user.active_sessions + ')'
                          : 'Sessions',
                      ),
                      createShow(
                        () => userSessionsLoading(),
                        () => h('div', { class: 'user-sessions-loading' },
                          Skeleton({ width: '100%', height: '12px' }),
                          Skeleton({ width: '80%', height: '12px' }),
                        ),
                      ),
                      createShow(
                        () => !userSessionsLoading() && userSessions().length > 0,
                        () => h('div', { class: 'user-sessions-list' },
                          () => userSessions().map((s: UserSession) => SessionCard(s)),
                        ),
                      ),
                      createShow(
                        () => !userSessionsLoading() && userSessions().length === 0 && user.active_sessions === 0,
                        () => h('div', { class: 'user-sessions-empty text-muted' }, 'No active sessions'),
                      ),
                    ),
                  ),
                  // ── Footer: meta + actions in one bar ──────────────
                  h('div', { class: 'user-detail-footer' },
                    h('div', { class: 'user-detail-meta text-muted' },
                      'EdDSA \u00b7 gatewasm \u00b7 gatewasm-app',
                    ),
                    h('div', { class: 'user-detail-toolbar' },
                      CopyButton({ text: user.id, label: 'Copy user ID' }),
                      h('button', {
                        type: 'button',
                        class: 'btn btn-sm btn-ghost',
                        onClick: openSessionInspector,
                      }, 'Auth Reference'),
                      h('div', { class: 'user-detail-toolbar-sep' }),
                      user.is_locked
                        ? h('button', { class: 'btn btn-sm btn-ghost', disabled: () => !canEdit(), onClick: () => unblockUser(tenantState().id, user.id) }, 'Unblock')
                        : h('button', { class: 'btn btn-sm btn-ghost', disabled: () => !canEdit(), onClick: () => blockUser(tenantState().id, user.id) }, 'Block'),
                      h('button', {
                        class: 'btn btn-sm btn-ghost',
                        disabled: () => user.active_sessions === 0 || !canEdit(),
                        onClick: () => revokeUserSessions(tenantState().id, user.id),
                      }, () => user.active_sessions > 0
                        ? 'Revoke ' + user.active_sessions + ' ' + (user.active_sessions === 1 ? 'Session' : 'Sessions')
                        : 'No Sessions'),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
      createShow(
        () => users().length >= 100,
        () => h('div', { class: 'list-truncated' }, '100 most recent members shown'),
      ),
    ),
  );
}

function InvitesPanel(tenantId: string) {
  const [open, setOpen] = createSignal(false);
  const [inviteEmail, setInviteEmail] = createSignal('');
  const [inviteRole, setInviteRole] = createSignal('role_admin');
  const [sending, setSending] = createSignal(false);

  const pendingInvites = () => invites().filter((i) => i.status === 'pending');
  const pendingCount = () => pendingInvites().length;

  async function handleSend() {
    const email = inviteEmail().trim();
    if (!email) return;
    setSending(true);
    const ok = await sendInvite(tenantId, email, inviteRole());
    if (ok) {
      setInviteEmail('');
      setInviteRole('role_admin');
    }
    setSending(false);
  }

  return h('section', { class: 'console-card invites-panel' },
    h('div', {
      class: 'invites-panel-head',
      onClick: () => setOpen(!open()),
    },
      h('div', { class: 'section-copy' },
        h('div', { class: 'card-label' }, 'Pending Invites'),
        h('p', { class: 'card-copy text-muted' },
          () => invitesLoading()
            ? 'Loading...'
            : pendingCount() + (pendingCount() === 1 ? ' pending invite' : ' pending invites'),
        ),
      ),
      h('button', {
        type: 'button',
        class: 'btn btn-ghost btn-sm invites-toggle',
        'aria-label': 'Toggle invites panel',
      }, () => open() ? 'Collapse' : 'Expand'),
    ),
    createShow(
      () => open(),
      () => h('div', { class: 'invites-panel-body' },
        // Send invite form
        h('div', { class: 'invite-form' },
          h('div', { class: 'card-label' }, 'Send Invite'),
          h('div', { class: 'invite-form-row' },
            h('input', {
              type: 'email',
              class: 'search-input invite-email-input',
              placeholder: 'colleague@example.com',
              value: inviteEmail,
              disabled: () => !canEdit(),
              onInput: (e: Event) => setInviteEmail((e.target as HTMLInputElement).value),
              onKeyDown: (e: KeyboardEvent) => { if (e.key === 'Enter') void handleSend(); },
            }),
            h('select', {
              class: 'filter-select invite-role-select',
              value: inviteRole,
              disabled: () => !canEdit(),
              onChange: (e: Event) => setInviteRole((e.target as HTMLSelectElement).value),
            },
              h('option', { value: 'role_admin' }, 'Admin'),
              h('option', { value: 'role_member' }, 'Member'),
              h('option', { value: 'role_viewer' }, 'Viewer'),
            ),
            h('button', {
              type: 'button',
              class: 'clip-btn',
              disabled: () => sending() || !inviteEmail().trim() || !canEdit(),
              onClick: () => void handleSend(),
            }, () => sending() ? 'Sending...' : 'Send Invite'),
          ),
        ),
        // Invite list
        createShow(
          () => invitesLoading(),
          () => h('div', { class: 'list-empty-state list-empty-state-loading' },
            Skeleton({ width: '60%', height: '12px' }),
            Skeleton({ width: '80%', height: '12px' }),
          ),
        ),
        createShow(
          () => !invitesLoading() && invites().length === 0,
          () => h('div', { class: 'list-empty-state' },
            h('div', { class: 'card-label' }, 'No Invites'),
            h('p', { class: 'card-copy' }, 'Use the form above to invite someone to this tenant.'),
          ),
        ),
        createShow(
          () => !invitesLoading() && invites().length > 0,
          () => h('div', { class: 'invite-list' },
            () => invites().map((invite: TenantInvite) =>
              h('div', { class: 'invite-row', key: invite.id },
                h('div', { class: 'invite-row-main' },
                  h('span', { class: 'invite-email' }, invite.email),
                  h('span', { class: 'badge badge-subtle invite-role-badge' }, invite.role_name || invite.role_id),
                  h('span', {
                    class: () => 'badge invite-status-badge' + (
                      invite.status === 'pending' ? ' badge-success' :
                      invite.status === 'expired' ? ' badge-warning' :
                      ' badge-danger'
                    ),
                  }, invite.status),
                ),
                h('div', { class: 'invite-row-meta text-muted' },
                  'Expires ' + new Date(invite.expires_at).toLocaleDateString(),
                ),
                invite.status === 'pending'
                  ? h('button', {
                      type: 'button',
                      class: 'btn btn-sm btn-ghost invite-revoke-btn',
                      disabled: () => !canEdit(),
                      onClick: () => void revokeInvite(tenantId, invite.id),
                    }, 'Revoke')
                  : null,
              ),
            ),
          ),
        ),
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

function SessionCard(s: UserSession) {
  const expiresMs = Date.parse(s.expires_at);
  const [timeLeft, setTimeLeft] = createSignal(formatTimeLeft(expiresMs));
  const timer = setInterval(() => setTimeLeft(formatTimeLeft(expiresMs)), 1000);
  setTimeout(() => {
    const check = setInterval(() => {
      if (!document.querySelector('.session-card-expiry-' + s.id.slice(0, 8))) {
        clearInterval(timer);
        clearInterval(check);
      }
    }, 5000);
  }, 0);

  return h('div', { class: 'session-card' },
    h('div', { class: 'session-card-head' },
      h('code', { class: 'session-card-id' }, s.id.slice(0, 20) + '...'),
      h('span', { class: 'session-card-expiry session-card-expiry-' + s.id.slice(0, 8) }, timeLeft),
    ),
    h('div', { class: 'session-card-body' },
      SessionMeta('Method', (s.auth_provider || s.auth_method) + (s.mfa_verified ? ' + MFA' : '')),
      SessionMeta('Created', new Date(s.created_at).toLocaleString()),
      SessionMeta('Expires', new Date(s.expires_at).toLocaleString()),
      s.last_activity_at
        ? SessionMeta('Last active', new Date(s.last_activity_at).toLocaleString())
        : null,
      s.login_ip
        ? SessionMeta('IP', s.login_ip)
        : null,
    ),
  );
}

function SessionMeta(label: string, value: string) {
  return h('div', { class: 'session-meta-row' },
    h('span', { class: 'session-meta-label' }, label),
    h('span', { class: 'session-meta-value' }, value),
  );
}

function formatTimeLeft(expiresMs: number): string {
  const diff = expiresMs - Date.now();
  if (diff <= 0) return 'Expired';
  const days = Math.floor(diff / 86400000);
  const hours = Math.floor((diff % 86400000) / 3600000);
  const mins = Math.floor((diff % 3600000) / 60000);
  if (days > 0) return days + 'd ' + hours + 'h';
  if (hours > 0) return hours + 'h ' + mins + 'm';
  const secs = Math.floor((diff % 60000) / 1000);
  return mins + 'm ' + secs + 's';
}
