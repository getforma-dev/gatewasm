import { h, createSignal, createShow } from 'formajs';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import {
  teamMembers, teamLoading, platformRole, tenants,
  loadTeamMembers, loadTeamMemberDetail,
  updateTeamMember, removeTeamMember, setMemberTenants,
  sendTeamInvite, revokeTeamInvite, loadTeamInvites,
  type PlatformMember, type PlatformRole,
} from '../store';

// ── Types ──────────────────────────────────────────────────────────────

type RoleFilter = 'all' | PlatformRole;

// ── TeamView (main) ────────────────────────────────────────────────────

export function TeamView() {
  const [filter, setFilter] = createSignal<RoleFilter>('all');
  const [invites, setInvites] = createSignal<any[]>([]);
  const [showInviteModal, setShowInviteModal] = createSignal(false);
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [expandedDetail, setExpandedDetail] = createSignal<PlatformMember | null>(null);

  // Load data on mount
  void loadTeamMembers();
  void loadTeamInvites().then(setInvites);

  const filtered = () => {
    const f = filter();
    const members = teamMembers();
    if (f === 'all') return members;
    return members.filter((m) => m.role === f);
  };

  const pendingInvites = () => invites().filter((i) => i.status === 'pending');

  function handleExpand(memberId: string) {
    if (expandedId() === memberId) {
      setExpandedId(null);
      setExpandedDetail(null);
      return;
    }
    setExpandedId(memberId);
    setExpandedDetail(null);
    void loadTeamMemberDetail(memberId).then((detail) => {
      if (expandedId() === memberId) setExpandedDetail(detail);
    });
  }

  async function handleRoleChange(member: PlatformMember, newRole: string) {
    await updateTeamMember(member.id, { role: newRole });
  }

  async function handleRemove(member: PlatformMember) {
    if (!confirm('Remove ' + member.email + ' from the team?')) return;
    await removeTeamMember(member.id);
    if (expandedId() === member.id) {
      setExpandedId(null);
      setExpandedDetail(null);
    }
  }

  return h('div', { class: 'console-view' },
    // ── Header ──────────────────────────────────────────────────────
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Platform'),
        h('h2', { class: 'view-title' }, 'Team'),
        h('p', { class: 'view-subtitle text-muted' }, () => teamMembers().length + ' team ' + (teamMembers().length === 1 ? 'member' : 'members')),
      ),
      h('div', { class: 'view-header-actions' },
        createShow(
          () => platformRole() === 'owner' || platformRole() === 'admin',
          () => h('button', {
            type: 'button',
            class: 'btn btn-primary',
            onClick: () => setShowInviteModal(true),
          }, 'Invite Member'),
        ),
      ),
    ),

    // ── Filter chips ────────────────────────────────────────────────
    h('div', { class: 'console-card' },
      h('div', { class: 'users-filter-group' },
        FilterChip('All', () => filter() === 'all', () => setFilter('all')),
        FilterChip('Owner', () => filter() === 'owner', () => setFilter('owner')),
        FilterChip('Admin', () => filter() === 'admin', () => setFilter('admin')),
        FilterChip('Editor', () => filter() === 'editor', () => setFilter('editor')),
        FilterChip('Viewer', () => filter() === 'viewer', () => setFilter('viewer')),
      ),
    ),

    // ── Pending invites ─────────────────────────────────────────────
    createShow(
      () => pendingInvites().length > 0,
      () => h('section', { class: 'console-card' },
        h('div', { class: 'card-label' }, () => 'Pending Invites (' + pendingInvites().length + ')'),
        h('div', { class: 'invite-list' },
          () => pendingInvites().map((invite) =>
            h('div', { class: 'invite-row', key: invite.id },
              h('div', { class: 'invite-row-main' },
                h('span', { class: 'invite-email' }, invite.email),
                h('span', { class: 'badge badge-subtle' }, invite.role),
                h('span', { class: 'badge badge-success' }, 'pending'),
              ),
              h('div', { class: 'invite-row-meta text-muted' },
                'Sent ' + new Date(invite.created_at).toLocaleDateString(),
              ),
              h('button', {
                type: 'button',
                class: 'btn btn-sm btn-ghost btn-danger',
                onClick: () => {
                  void revokeTeamInvite(invite.id).then((ok) => {
                    if (ok) void loadTeamInvites().then(setInvites);
                  });
                },
              }, 'Revoke'),
            ),
          ),
        ),
      ),
    ),

    // ── Members list ────────────────────────────────────────────────
    h('section', { class: 'console-card list-shell' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'Members'),
          h('p', { class: 'card-copy' }, () => filtered().length + ' matching'),
        ),
      ),
      createShow(
        () => teamLoading(),
        () => h('div', { class: 'list-empty-state' }, 'Loading team members...'),
      ),
      createShow(
        () => !teamLoading() && filtered().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Members'),
          h('p', { class: 'card-copy' }, 'No team members match the current filter.'),
        ),
      ),
      createShow(
        () => filtered().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => filtered().map((member) =>
            h('div', {
              class: () => 'user-row' + (expandedId() === member.id ? ' expanded' : ''),
              key: member.id,
            },
              // ── Row summary ───────────────────────────────────────
              h('div', {
                class: 'user-row-summary',
                onClick: () => handleExpand(member.id),
              },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' },
                    member.email || member.name,
                    h('span', { class: 'badge badge-subtle' }, member.role),
                    member.status === 'suspended'
                      ? h('span', { class: 'badge badge-danger' }, 'Suspended')
                      : null,
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    'Joined ' + new Date(member.created_at).toLocaleDateString(),
                    member.tenant_count > 0
                      ? ' \u00b7 ' + member.tenant_count + ' ' + (member.tenant_count === 1 ? 'tenant' : 'tenants')
                      : '',
                  ),
                ),
                h('div', { class: 'user-row-actions' },
                  // Role dropdown — only for non-owner members, and only if caller can edit
                  member.role !== 'owner' && (platformRole() === 'owner' || platformRole() === 'admin')
                    ? h('select', {
                        class: 'filter-select',
                        value: member.role,
                        onClick: (e: Event) => e.stopPropagation(),
                        onChange: (e: Event) => {
                          e.stopPropagation();
                          void handleRoleChange(member, (e.target as HTMLSelectElement).value);
                        },
                      },
                        platformRole() === 'owner'
                          ? h('option', { value: 'admin' }, 'Admin')
                          : null,
                        h('option', { value: 'editor' }, 'Editor'),
                        h('option', { value: 'viewer' }, 'Viewer'),
                      )
                    : null,
                  // Remove button — only for non-owner members
                  member.role !== 'owner' && (platformRole() === 'owner' || platformRole() === 'admin')
                    ? h('button', {
                        type: 'button',
                        class: 'btn btn-sm btn-ghost btn-danger',
                        onClick: (e: Event) => { e.stopPropagation(); void handleRemove(member); },
                      }, 'Remove')
                    : null,
                  h('div', { class: 'user-row-chevron' }, () => expandedId() === member.id ? 'Hide' : 'Details'),
                ),
              ),

              // ── Expanded detail ───────────────────────────────────
              createShow(
                () => expandedId() === member.id && expandedDetail() !== null,
                () => MemberDetail(expandedDetail()!),
              ),
              createShow(
                () => expandedId() === member.id && expandedDetail() === null,
                () => h('div', { class: 'user-detail' }, 'Loading details...'),
              ),
            ),
          ),
        ),
      ),
    ),

    // ── Invite modal ────────────────────────────────────────────────
    createShow(
      () => showInviteModal(),
      () => InviteModal(
        () => setShowInviteModal(false),
        (newInvites: any[]) => setInvites(newInvites),
      ),
    ),
  );
}

// ── InviteModal ────────────────────────────────────────────────────────

function InviteModal(onClose: () => void, onInvitesUpdate: (inv: any[]) => void) {
  const [email, setEmail] = createSignal('');
  const [role, setRole] = createSignal<string>('editor');
  const [selectedTenantIds, setSelectedTenantIds] = createSignal<string[]>([]);
  const [sending, setSending] = createSignal(false);
  const [inviteUrl, setInviteUrl] = createSignal<string | null>(null);

  const isScopedRole = () => role() === 'editor' || role() === 'viewer';

  function toggleTenant(id: string) {
    const current = selectedTenantIds();
    if (current.includes(id)) {
      setSelectedTenantIds(current.filter((t) => t !== id));
    } else {
      setSelectedTenantIds([...current, id]);
    }
  }

  const canSend = () => {
    if (!email().trim()) return false;
    if (isScopedRole() && selectedTenantIds().length === 0) return false;
    return true;
  };

  async function handleSend() {
    if (!canSend()) return;
    setSending(true);
    const result = await sendTeamInvite(email().trim(), role(), selectedTenantIds());
    setSending(false);
    if (result) {
      setInviteUrl(result.invite_url);
      // Refresh invites list
      const fresh = await loadTeamInvites();
      onInvitesUpdate(fresh);
    }
  }

  return h('div', {
    class: 'modal-overlay',
    onClick: (e: Event) => { if (e.target === e.currentTarget) onClose(); },
  },
    h('div', { class: 'modal-card' },
      h('h3', { class: 'modal-title' }, 'Invite Team Member'),

      // ── Success state ───────────────────────────────────────────
      createShow(
        () => inviteUrl() !== null,
        () => h('div', { class: 'modal-field' },
          h('label', null, 'Invite sent! Share this link:'),
          h('div', { class: 'invite-url-row' },
            h('code', { class: 'invite-url-display' }, () => inviteUrl()!),
            CopyButton({ text: () => inviteUrl() || '', label: 'Copy invite URL' }),
          ),
          h('div', { class: 'modal-actions' },
            h('button', {
              type: 'button',
              class: 'btn btn-primary',
              onClick: onClose,
            }, 'Done'),
          ),
        ),
      ),

      // ── Form state ─────────────────────────────────────────────
      createShow(
        () => inviteUrl() === null,
        () => h('div', null,
          h('div', { class: 'modal-field' },
            h('label', null, 'Email'),
            h('input', {
              type: 'email',
              class: 'search-input',
              placeholder: 'colleague@example.com',
              value: email,
              onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
            }),
          ),
          h('div', { class: 'modal-field' },
            h('label', null, 'Role'),
            h('select', {
              class: 'filter-select',
              value: role,
              onChange: (e: Event) => setRole((e.target as HTMLSelectElement).value),
            },
              platformRole() === 'owner'
                ? h('option', { value: 'admin' }, 'Admin')
                : null,
              h('option', { value: 'editor' }, 'Editor'),
              h('option', { value: 'viewer' }, 'Viewer'),
            ),
          ),

          // Tenant multi-select — only for scoped roles
          createShow(
            isScopedRole,
            () => h('div', { class: 'modal-field' },
              h('label', null, 'Tenant Access'),
              h('div', { class: 'tenant-checkbox-list' },
                () => tenants().map((t) =>
                  h('label', { class: 'tenant-checkbox-item', key: t.id },
                    h('input', {
                      type: 'checkbox',
                      checked: () => selectedTenantIds().includes(t.id),
                      onChange: () => toggleTenant(t.id),
                    }),
                    h('span', null, t.name + ' (/t/' + t.slug + ')'),
                  ),
                ),
              ),
              createShow(
                () => tenants().length === 0,
                () => h('p', { class: 'text-muted' }, 'No tenants available.'),
              ),
            ),
          ),

          h('div', { class: 'modal-actions' },
            h('button', {
              type: 'button',
              class: 'btn btn-sm btn-ghost',
              onClick: onClose,
            }, 'Cancel'),
            h('button', {
              type: 'button',
              class: 'btn btn-primary',
              disabled: () => sending() || !canSend(),
              onClick: () => void handleSend(),
            }, () => sending() ? 'Sending...' : 'Send Invite'),
          ),
        ),
      ),
    ),
  );
}

// ── MemberDetail ───────────────────────────────────────────────────────

function MemberDetail(member: PlatformMember) {
  return h('div', { class: 'user-detail' },
    h('div', { class: 'user-detail-columns' },
      // Left: profile info
      h('div', { class: 'user-detail-profile' },
        h('div', { class: 'card-label' }, 'Profile'),
        h('div', { class: 'detail-grid' },
          DetailRow('Email', member.email),
          DetailRow('Role', member.role),
          DetailRow('Status', member.status),
          DetailRow('Joined', new Date(member.created_at).toLocaleDateString()),
          DetailRow('Last Login', member.last_login_at ? new Date(member.last_login_at).toLocaleDateString() : 'Never'),
        ),
        h('div', { class: 'user-detail-footer' },
          CopyButton({ text: member.id, label: 'Copy member ID' }),
        ),
      ),

      // Right: tenant access (only for editor/viewer)
      (member.role === 'editor' || member.role === 'viewer')
        ? h('div', { class: 'user-detail-sessions' },
            h('div', { class: 'card-label' }, 'Tenant Access'),
            member.tenants && member.tenants.length > 0
              ? h('div', { class: 'tenant-access-list' },
                  ...member.tenants.map((t) =>
                    h('div', { class: 'tenant-access-item' },
                      h('span', null, t.name),
                      h('span', { class: 'text-muted' }, '/t/' + t.slug),
                    ),
                  ),
                )
              : h('p', { class: 'text-muted' }, 'No tenant access assigned.'),
            TenantAssigner(member),
          )
        : null,
    ),
  );
}

// ── TenantAssigner ─────────────────────────────────────────────────────

function TenantAssigner(member: PlatformMember) {
  const currentIds = (member.tenants || []).map((t) => t.id);
  const [selected, setSelected] = createSignal<string[]>(currentIds);
  const [saving, setSaving] = createSignal(false);

  const allTenants = tenants;

  function toggleTenant(id: string) {
    const cur = selected();
    if (cur.includes(id)) {
      setSelected(cur.filter((x) => x !== id));
    } else {
      setSelected([...cur, id]);
    }
  }

  async function handleSave() {
    setSaving(true);
    await setMemberTenants(member.id, selected());
    setSaving(false);
  }

  return h('div', { class: 'tenant-assigner' },
    h('div', { class: 'card-label' }, 'Update Tenant Access'),
    h('div', { class: 'tenant-checkbox-list' },
      () => allTenants().map((t) =>
        h('label', { class: 'tenant-checkbox-item', key: t.id },
          h('input', {
            type: 'checkbox',
            checked: () => selected().includes(t.id),
            onChange: () => toggleTenant(t.id),
          }),
          h('span', null, t.name + ' (/t/' + t.slug + ')'),
        ),
      ),
    ),
    h('button', {
      type: 'button',
      class: 'btn btn-primary btn-sm',
      disabled: () => saving(),
      onClick: () => void handleSave(),
    }, () => saving() ? 'Saving...' : 'Update Access'),
  );
}

// ── Shared helpers ─────────────────────────────────────────────────────

function FilterChip(label: string, active: () => boolean, onClick: () => void) {
  return h('button', {
    type: 'button',
    class: () => 'list-filter-chip' + (active() ? ' active' : ''),
    onClick,
  }, label);
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
