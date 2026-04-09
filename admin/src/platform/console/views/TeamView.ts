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

// ── Helpers ───────────────────────────────────────────────────────────

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return mins + 'm ago';
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return hrs + 'h ago';
  const days = Math.floor(hrs / 24);
  if (days < 30) return days + 'd ago';
  const months = Math.floor(days / 30);
  if (months < 12) return months + 'mo ago';
  return Math.floor(months / 12) + 'y ago';
}

function expiresIn(iso: string): string {
  const diff = new Date(iso).getTime() - Date.now();
  if (diff <= 0) return 'Expired';
  const hrs = Math.floor(diff / 3600000);
  if (hrs < 1) return 'Expires in <1h';
  if (hrs < 24) return 'Expires in ' + hrs + 'h';
  const days = Math.floor(hrs / 24);
  return 'Expires in ' + days + 'd';
}

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
        h('div', { class: 'user-list user-list-shell' },
          () => pendingInvites().map((invite: any) =>
            h('div', { class: 'user-row', key: invite.id },
              h('div', { class: 'user-row-summary', style: 'cursor:default;' },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' },
                    invite.email,
                    h('span', { class: 'badge badge-subtle' }, invite.role),
                    h('span', { class: 'badge badge-warning' }, 'Pending'),
                  ),
                  h('div', { class: 'user-meta text-muted' },
                    'Sent ' + timeAgo(invite.created_at),
                    ' \u00b7 ',
                    expiresIn(invite.expires_at),
                  ),
                ),
                h('div', { class: 'user-row-actions' },
                  h('button', {
                    type: 'button',
                    class: 'btn btn-sm btn-ghost',
                    onClick: () => {
                      void sendTeamInvite(invite.email, invite.role, invite.tenant_ids || []).then(() => {
                        void loadTeamInvites().then(setInvites);
                      });
                    },
                  }, 'Resend'),
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
          filter() !== 'all'
            ? h('div', null,
                h('div', { class: 'card-label' }, 'No Matches'),
                h('p', { class: 'card-copy' }, 'No members match the current filter.'),
              )
            : h('div', null,
                h('div', { class: 'card-label' }, 'No Team Members Yet'),
                h('p', { class: 'card-copy' }, 'Invite colleagues to help manage your applications. They\'ll get per-application roles (Admin, Editor, or Viewer).'),
                accountOwner()
                  ? h('button', {
                      type: 'button',
                      class: 'btn btn-primary',
                      style: 'margin-top:12px;',
                      onClick: () => setShowInviteModal(true),
                    }, 'Invite Your First Member')
                  : null,
              ),
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
                    'Joined ' + timeAgo(member.created_at),
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
  const [emailsRaw, setEmailsRaw] = createSignal('');
  const [role, setRole] = createSignal<string>('admin');
  const [selectedTenantIds, setSelectedTenantIds] = createSignal<string[]>([]);
  const [sending, setSending] = createSignal(false);
  const [results, setResults] = createSignal<{ email: string; ok: boolean; url?: string; error?: string }[]>([]);

  // Escape key closes modal
  const handleKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { window.removeEventListener('keydown', handleKeydown); onClose(); }
  };
  window.addEventListener('keydown', handleKeydown);

  const parsedEmails = () =>
    emailsRaw().split(/[,;\n]+/).map((e) => e.trim().toLowerCase()).filter((e) => e && e.includes('@'));

  function toggleTenant(id: string) {
    const current = selectedTenantIds();
    if (current.includes(id)) {
      setSelectedTenantIds(current.filter((t) => t !== id));
    } else {
      setSelectedTenantIds([...current, id]);
    }
  }

  const canSend = () => {
    if (parsedEmails().length === 0) return false;
    if (selectedTenantIds().length === 0) return false;
    return true;
  };

  async function handleSend() {
    if (!canSend()) return;
    setSending(true);
    const emails = parsedEmails();
    const sent: { email: string; ok: boolean; url?: string; error?: string }[] = [];
    for (const addr of emails) {
      const result = await sendTeamInvite(addr, role(), selectedTenantIds());
      if (result) {
        sent.push({ email: addr, ok: true, url: result.invite_url });
      } else {
        sent.push({ email: addr, ok: false, error: 'Failed to send' });
      }
    }
    setResults(sent);
    setSending(false);
    const fresh = await loadTeamInvites();
    onInvitesUpdate(fresh);
  }

  const [tenantSearch, setTenantSearch] = createSignal('');
  const ownTenants = () => tenants().filter((t: any) => t.access === 'own' || !t.access);
  const filteredTenants = () => {
    const q = tenantSearch().toLowerCase().trim();
    if (!q) return ownTenants();
    return ownTenants().filter((t) => t.name.toLowerCase().includes(q) || t.slug.toLowerCase().includes(q));
  };
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
          () => results().length > 0,
          () => h('div', null,
            h('p', { style: 'margin:0 0 12px;font-weight:500;' },
              () => {
                const r = results();
                const ok = r.filter((x) => x.ok).length;
                const fail = r.length - ok;
                if (fail === 0) return h('span', { style: 'color:var(--success)' }, ok === 1 ? 'Invite sent!' : ok + ' invites sent!');
                return h('span', null,
                  ok > 0 ? h('span', { style: 'color:var(--success)' }, ok + ' sent') : null,
                  ok > 0 && fail > 0 ? ', ' : '',
                  fail > 0 ? h('span', { style: 'color:var(--danger)' }, fail + ' failed') : null,
                );
              },
            ),
            h('div', { style: 'display:flex;flex-direction:column;gap:6px;margin-bottom:12px;' },
              () => results().map((r) =>
                h('div', { style: 'display:flex;align-items:center;gap:8px;font-size:0.82rem;' },
                  h('span', { style: r.ok ? 'color:var(--success)' : 'color:var(--danger)' }, r.ok ? '\u2713' : '\u2717'),
                  h('span', null, r.email),
                  r.ok && r.url ? CopyButton({ text: () => r.url || '', label: 'Copy link' }) : null,
                ),
              ),
            ),
            h('div', { class: 'modal-actions' },
              h('button', { type: 'button', class: 'btn btn-primary', onClick: onClose }, 'Done'),
            ),
          ),
        ),

        // ── Form state ────────────────────────────────────────────
        createShow(
          () => results().length === 0,
          () => h('div', null,
            // Email + Role
            h('div', { class: 'invite-form-row' },
              h('div', { class: 'invite-form-field invite-form-email' },
                h('label', { class: 'field-label' }, () => 'Email' + (parsedEmails().length > 1 ? 's (' + parsedEmails().length + ')' : '')),
                h('textarea', {
                  class: 'input',
                  rows: 2,
                  placeholder: 'colleague@example.com, another@team.com',
                  value: emailsRaw,
                  onInput: (e: Event) => setEmailsRaw((e.target as HTMLTextAreaElement).value),
                  style: 'resize:vertical;min-height:38px;font-family:inherit;',
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
                if (r === 'admin') return 'Can manage settings, users, and roles on assigned applications.';
                if (r === 'editor') return 'Can edit settings on assigned applications. Cannot manage users.';
                return 'Read-only access to assigned applications.';
              },
            ),

            // Tenant access
            h('div', { style: 'margin-bottom:16px;' },
              h('div', { style: 'display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;' },
                h('label', { class: 'field-label', style: 'margin:0;' }, 'Application Access'),
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
                () => h('p', { class: 'text-muted', style: 'font-size:0.82rem;' }, 'Create an application first to invite members.'),
              ),
              // Search (show when 4+ tenants)
              createShow(
                () => ownTenants().length >= 4,
                () => h('input', {
                  type: 'text',
                  class: 'input invite-tenant-search',
                  placeholder: 'Search applications...',
                  value: tenantSearch,
                  onInput: (e: Event) => setTenantSearch((e.target as HTMLInputElement).value),
                }),
              ),
              h('div', { class: 'invite-tenant-list' },
                () => filteredTenants().map((t) =>
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
                // No results
                createShow(
                  () => ownTenants().length > 0 && filteredTenants().length === 0,
                  () => h('div', { class: 'invite-tenant-empty' }, 'No applications match "' + tenantSearch() + '"'),
                ),
              ),
              // Selection count
              createShow(
                () => selectedTenantIds().length > 0,
                () => h('p', { class: 'text-muted', style: 'font-size:0.75rem;margin:6px 0 0;' },
                  () => selectedTenantIds().length + ' application' + (selectedTenantIds().length !== 1 ? 's' : '') + ' selected'),
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
          DetailRow('Joined', timeAgo(member.created_at)),
          DetailRow('Last Login', member.last_login_at ? timeAgo(member.last_login_at) : 'Never'),
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
