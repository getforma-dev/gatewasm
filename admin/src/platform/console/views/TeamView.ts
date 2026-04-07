import { h, createSignal, createShow } from 'formajs';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import {
  teamMembers, teamLoading, accountOwner, tenants,
  loadTeamMembers, loadTeamMemberDetail,
  updateTeamMember, removeTeamMember,
  sendTeamInvite, revokeTeamInvite, loadTeamInvites,
  type PlatformMember, type PlatformRole,
} from '../store';

// ── Types ──────────────────────────────────────────────────────────────

type RoleFilter = 'all' | 'owner' | PlatformRole;

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
    if (f === 'owner') return members.filter((m) => m.is_account_owner);
    return members.filter((m) => m.tenant_assignments?.some((a) => a.role === f));
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

  async function handleRemove(member: PlatformMember) {
    if (!confirm('Remove ' + member.email + '?')) return;
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
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'Members'),
        h('p', { class: 'view-subtitle text-muted' }, () => teamMembers().length + ' ' + (teamMembers().length === 1 ? 'member' : 'members')),
      ),
      h('div', { class: 'view-header-actions' },
        createShow(
          () => accountOwner(),
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
        () => h('div', { class: 'list-empty-state' }, 'Loading members...'),
      ),
      createShow(
        () => !teamLoading() && filtered().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Members'),
          h('p', { class: 'card-copy' }, 'No members match the current filter.'),
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
                    member.is_account_owner
                      ? h('span', { class: 'badge badge-subtle' }, 'Owner')
                      : member.tenant_assignments?.length > 0
                        ? h('span', { class: 'badge badge-subtle' },
                            member.tenant_assignments.length + ' ' +
                            (member.tenant_assignments.length === 1 ? 'tenant' : 'tenants'))
                        : null,
                    member.status === 'suspended'
                      ? h('span', { class: 'badge badge-danger' }, 'Suspended')
                      : null,
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    'Joined ' + new Date(member.created_at).toLocaleDateString(),
                    !member.is_account_owner && member.tenant_assignments?.length > 0
                      ? ' \u00b7 ' + [...new Set(member.tenant_assignments.map((a) => a.role))].join(', ')
                      : '',
                  ),
                ),
                h('div', { class: 'user-row-actions' },
                  // Remove button — only for non-owner members, and only if caller is account owner
                  !member.is_account_owner && accountOwner()
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
  const [role, setRole] = createSignal<string>('admin');
  const [selectedTenantIds, setSelectedTenantIds] = createSignal<string[]>([]);
  const [sending, setSending] = createSignal(false);
  const [inviteUrl, setInviteUrl] = createSignal<string | null>(null);

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
    if (selectedTenantIds().length === 0) return false; // all roles require tenant selection
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

  const ownTenants = () => tenants().filter((t: any) => t.access === 'own' || !t.access);
  const allSelected = () => ownTenants().length > 0 && selectedTenantIds().length === ownTenants().length;

  function selectAll() {
    if (allSelected()) {
      setSelectedTenantIds([]);
    } else {
      setSelectedTenantIds(ownTenants().map((t) => t.id));
    }
  }

  return h('div', {
    class: 'modal-overlay',
    onClick: (e: Event) => { if (e.target === e.currentTarget) onClose(); },
  },
    h('div', { class: 'modal-content' },
      // ── Header ──────────────────────────────────────────────────
      h('div', { class: 'modal-header' },
        h('span', { class: 'section-title' }, 'Invite Member'),
        h('button', { class: 'modal-close', type: 'button', onClick: onClose }, '\u00D7'),
      ),

      // ── Body ────────────────────────────────────────────────────
      h('div', { class: 'modal-body' },

        // ── Success state ─────────────────────────────────────────
        createShow(
          () => inviteUrl() !== null,
          () => h('div', null,
            h('p', { style: 'margin:0 0 12px;color:var(--success);font-weight:500;' }, 'Invite sent!'),
            h('p', { class: 'text-muted', style: 'margin:0 0 8px;font-size:0.82rem;' }, 'Share this link with the invitee:'),
            h('div', { class: 'invite-url-row' },
              h('code', { class: 'invite-url-code' }, () => inviteUrl()!),
              CopyButton({ text: () => inviteUrl() || '', label: 'Copy' }),
            ),
            h('div', { class: 'modal-actions' },
              h('button', { type: 'button', class: 'btn btn-primary', onClick: onClose }, 'Done'),
            ),
          ),
        ),

        // ── Form state ────────────────────────────────────────────
        createShow(
          () => inviteUrl() === null,
          () => h('div', null,
            // Email + Role side by side
            h('div', { class: 'invite-form-row' },
              h('div', { class: 'invite-form-field invite-form-email' },
                h('label', { class: 'field-label' }, 'Email'),
                h('input', {
                  type: 'email',
                  class: 'input',
                  placeholder: 'colleague@example.com',
                  value: email,
                  onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
                }),
              ),
              h('div', { class: 'invite-form-field invite-form-role' },
                h('label', { class: 'field-label' }, 'Role'),
                h('select', {
                  class: 'input',
                  value: role,
                  onChange: (e: Event) => setRole((e.target as HTMLSelectElement).value),
                },
                  h('option', { value: 'admin' }, 'Admin'),
                  h('option', { value: 'editor' }, 'Editor'),
                  h('option', { value: 'viewer' }, 'Viewer'),
                ),
              ),
            ),

            // Role description
            h('p', { class: 'text-muted', style: 'font-size:0.78rem;margin:4px 0 16px;' },
              () => {
                const r = role();
                if (r === 'admin') return 'Can manage settings, users, and roles on assigned workspaces.';
                if (r === 'editor') return 'Can edit settings on assigned workspaces. Cannot manage users.';
                return 'Read-only access to assigned workspaces.';
              },
            ),

            // Tenant access
            h('div', { style: 'margin-bottom:16px;' },
              h('div', { style: 'display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;' },
                h('label', { class: 'field-label', style: 'margin:0;' }, 'Workspace Access'),
                createShow(
                  () => ownTenants().length > 1,
                  () => h('button', {
                    type: 'button',
                    class: 'btn-link',
                    style: 'font-size:0.78rem;',
                    onClick: selectAll,
                  }, () => allSelected() ? 'Deselect all' : 'Select all'),
                ),
              ),
              createShow(
                () => ownTenants().length === 0,
                () => h('p', { class: 'text-muted', style: 'font-size:0.82rem;' }, 'Create a workspace first to invite members.'),
              ),
              h('div', { class: 'invite-tenant-list' },
                () => ownTenants().map((t) =>
                  h('label', { class: 'invite-tenant-item', key: t.id },
                    h('input', {
                      type: 'checkbox',
                      checked: () => selectedTenantIds().includes(t.id),
                      onChange: () => toggleTenant(t.id),
                    }),
                    h('div', { class: 'invite-tenant-info' },
                      h('span', { class: 'invite-tenant-name' }, t.name),
                      h('span', { class: 'invite-tenant-slug' }, '/t/' + t.slug),
                    ),
                  ),
                ),
              ),
            ),

            // Actions
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
          DetailRow('Role', member.is_account_owner ? 'Owner' : (member.tenant_assignments?.length > 0 ? [...new Set(member.tenant_assignments.map((a) => a.role))].join(', ') : 'No assignments')),
          DetailRow('Status', member.status),
          DetailRow('Joined', new Date(member.created_at).toLocaleDateString()),
          DetailRow('Last Login', member.last_login_at ? new Date(member.last_login_at).toLocaleDateString() : 'Never'),
        ),
        h('div', { class: 'user-detail-footer' },
          CopyButton({ text: member.id, label: 'Copy member ID' }),
        ),
      ),

      // Right: tenant access (for all non-owner members)
      !member.is_account_owner
        ? h('div', { class: 'user-detail-sessions' },
            h('div', { class: 'card-label' }, 'Tenant Access'),
            member.tenant_assignments && member.tenant_assignments.length > 0
              ? h('div', { class: 'tenant-access-list' },
                  ...member.tenant_assignments.map((a) =>
                    h('div', { class: 'tenant-access-item' },
                      h('span', null, a.tenant_name),
                      h('span', { class: 'badge badge-subtle' }, a.role),
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
  // Build initial state: { tenantId -> role } from current assignments
  const currentAssignments = new Map(
    (member.tenant_assignments || []).map((a) => [a.tenant_id, a.role]),
  );
  const [assignments, setAssignments] = createSignal<Map<string, string>>(currentAssignments);
  const [saving, setSaving] = createSignal(false);

  // Only show tenants owned by the account owner (access === 'own'), not team-assigned from other accounts
  const allTenants = () => tenants().filter((t: any) => t.access === 'own' || !t.access);

  function toggleTenant(id: string) {
    const cur = new Map(assignments());
    if (cur.has(id)) {
      cur.delete(id);
    } else {
      cur.set(id, 'editor'); // default role for new assignment
    }
    setAssignments(cur);
  }

  function setTenantRole(id: string, role: string) {
    const cur = new Map(assignments());
    cur.set(id, role);
    setAssignments(cur);
  }

  async function handleSave() {
    setSaving(true);
    const tenantAssignments = Array.from(assignments().entries()).map(([tenant_id, role]) => ({
      tenant_id,
      role,
    }));
    await updateTeamMember(member.id, { tenant_assignments: tenantAssignments });
    setSaving(false);
  }

  return h('div', { class: 'tenant-assigner' },
    h('div', { class: 'card-label' }, 'Update Tenant Access'),
    h('div', { class: 'tenant-checkbox-list' },
      () => allTenants().map((t) =>
        h('label', { class: 'tenant-checkbox-item', key: t.id },
          h('input', {
            type: 'checkbox',
            checked: () => assignments().has(t.id),
            onChange: () => toggleTenant(t.id),
          }),
          h('span', null, t.name + ' (/t/' + t.slug + ')'),
          assignments().has(t.id)
            ? h('select', {
                class: 'filter-select',
                value: () => assignments().get(t.id) || 'editor',
                onClick: (e: Event) => e.stopPropagation(),
                onChange: (e: Event) => setTenantRole(t.id, (e.target as HTMLSelectElement).value),
              },
                h('option', { value: 'admin' }, 'Admin'),
                h('option', { value: 'editor' }, 'Editor'),
                h('option', { value: 'viewer' }, 'Viewer'),
              )
            : null,
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
