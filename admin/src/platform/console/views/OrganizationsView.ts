import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant, organizations, organizationsLoading,
  createOrganization, deleteOrganization,
  loadOrgMembers, addOrgMember, removeOrgMember,
  type Organization, type OrgMember,
} from '../store';

export function OrganizationsView() {
  const [showCreate, setShowCreate] = createSignal(false);
  const [newName, setNewName] = createSignal('');
  const [newSlug, setNewSlug] = createSignal('');
  const [slugEdited, setSlugEdited] = createSignal(false);
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [members, setMembers] = createSignal<OrgMember[]>([]);
  const [membersLoading, setMembersLoading] = createSignal(false);
  const [addUserId, setAddUserId] = createSignal('');
  const [addRole, setAddRole] = createSignal('member');

  const tenantId = () => selectedTenant()?.id || '';

  function handleNameInput(value: string) {
    setNewName(value);
    if (!slugEdited()) {
      setNewSlug(
        value
          .toLowerCase()
          .replace(/[^a-z0-9-]/g, '-')
          .replace(/-+/g, '-')
          .replace(/^-|-$/g, '')
          .slice(0, 64),
      );
    }
  }

  function handleSlugInput(value: string) {
    setSlugEdited(true);
    setNewSlug(
      value
        .toLowerCase()
        .replace(/[^a-z0-9-]/g, '-')
        .replace(/-+/g, '-')
        .slice(0, 64),
    );
  }

  async function handleCreate() {
    const name = newName().trim();
    const slug = newSlug().trim();
    if (!name || !slug) return;
    const ok = await createOrganization(tenantId(), name, slug);
    if (ok) {
      setNewName('');
      setNewSlug('');
      setSlugEdited(false);
      setShowCreate(false);
    }
  }

  async function toggleExpand(org: Organization) {
    if (expandedId() === org.id) {
      setExpandedId(null);
      setMembers([]);
      return;
    }
    setExpandedId(org.id);
    setMembersLoading(true);
    const m = await loadOrgMembers(tenantId(), org.id);
    if (expandedId() === org.id) {
      setMembers(m);
    }
    setMembersLoading(false);
  }

  async function handleAddMember(orgId: string) {
    const userId = addUserId().trim();
    if (!userId) return;
    const ok = await addOrgMember(tenantId(), orgId, userId, addRole());
    if (ok) {
      setAddUserId('');
      setAddRole('member');
      // Reload members
      setMembersLoading(true);
      const m = await loadOrgMembers(tenantId(), orgId);
      if (expandedId() === orgId) {
        setMembers(m);
      }
      setMembersLoading(false);
    }
  }

  async function handleRemoveMember(orgId: string, userId: string) {
    await removeOrgMember(tenantId(), orgId, userId);
    setMembersLoading(true);
    const m = await loadOrgMembers(tenantId(), orgId);
    if (expandedId() === orgId) {
      setMembers(m);
    }
    setMembersLoading(false);
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Organization'),
        h('h2', { class: 'view-title' }, 'Organizations'),
        h('p', { class: 'view-subtitle text-muted' }, () => organizations().length + ' organizations'),
      ),
      h('div', { class: 'view-header-actions' },
        h('button', {
          class: () => showCreate() ? 'btn btn-sm btn-ghost' : 'clip-btn',
          onClick: () => setShowCreate(!showCreate()),
        }, () => showCreate() ? 'Cancel' : 'Create Organization'),
      ),
    ),

    // ── Create form ────────────────────────────────────────────────
    createShow(showCreate, () =>
      h('div', { class: 'console-card' },
        h('div', { class: 'card-label' }, 'New Organization'),
        h('div', { class: 'form-stack' },
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: 'Organization name (e.g. Acme Corp)',
            value: newName,
            onInput: (e: Event) => handleNameInput((e.target as HTMLInputElement).value),
          }),
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: 'Slug (e.g. acme-corp)',
            value: newSlug,
            onInput: (e: Event) => handleSlugInput((e.target as HTMLInputElement).value),
          }),
          h('div', { class: 'card-actions' },
            h('button', {
              class: 'btn btn-sm btn-primary',
              disabled: () => !newName().trim() || !newSlug().trim(),
              onClick: handleCreate,
            }, 'Create'),
          ),
        ),
      ),
    ),

    // ── Org list ──────────────────────────────────────────────────
    h('section', { class: 'console-card list-shell section-box section-box-bottom' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'All Organizations'),
          h('p', { class: 'card-copy' }, () => organizations().length + ' organizations in this tenant'),
        ),
      ),
      createShow(
        () => organizationsLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '24%', height: '12px' }),
          Skeleton({ width: '88%', height: '12px' }),
          Skeleton({ width: '82%', height: '12px' }),
        ),
      ),
      createShow(
        () => !organizationsLoading() && organizations().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Organizations Yet'),
          h('p', { class: 'card-copy' },
            'Create your first organization to group members and manage access at the team level.',
          ),
        ),
      ),
      createShow(
        () => organizations().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => organizations().map((org: Organization) =>
            h('div', {
              class: () => 'user-row' + (expandedId() === org.id ? ' expanded' : ''),
              key: org.id,
            },
              h('div', {
                class: 'user-row-summary',
                onClick: () => toggleExpand(org),
              },
                h('div', { class: 'user-row-main' },
                  h('div', { class: 'user-email' }, org.name),
                  h('div', { class: 'user-meta text-muted' },
                    org.slug + ' \u00b7 ' + new Date(org.created_at).toLocaleDateString(),
                  ),
                ),
                h('div', { class: 'user-row-chevron' }, () => expandedId() === org.id ? 'Hide' : 'Inspect'),
              ),
              createShow(
                () => expandedId() === org.id,
                () => h('div', { class: 'user-detail' },
                  h('div', { class: 'user-detail-columns' },
                    // Left: org info
                    h('div', { class: 'user-detail-profile' },
                      h('div', { class: 'card-label' }, 'Details'),
                      h('div', { class: 'detail-grid' },
                        DetailRow('ID', org.id),
                        DetailRow('Slug', org.slug),
                        DetailRow('Created', new Date(org.created_at).toLocaleString()),
                        DetailRow('Updated', new Date(org.updated_at).toLocaleString()),
                      ),
                    ),
                    // Right: members
                    h('div', { class: 'user-detail-sessions' },
                      h('div', { class: 'card-label' }, () => 'Members (' + members().length + ')'),
                      createShow(
                        () => membersLoading(),
                        () => h('div', { class: 'user-sessions-loading' },
                          Skeleton({ width: '100%', height: '12px' }),
                          Skeleton({ width: '80%', height: '12px' }),
                        ),
                      ),
                      createShow(
                        () => !membersLoading() && members().length > 0,
                        () => h('div', { class: 'user-sessions-list' },
                          () => members().map((m: OrgMember) =>
                            h('div', { class: 'session-card', key: m.user_id },
                              h('div', { class: 'session-card-main' },
                                h('div', { class: 'session-card-method' }, m.email),
                                h('div', { class: 'session-card-meta text-muted' },
                                  m.role + (m.name ? ' \u00b7 ' + m.name : ''),
                                ),
                              ),
                              h('button', {
                                class: 'btn btn-xs btn-outline btn-danger',
                                onClick: () => handleRemoveMember(org.id, m.user_id),
                              }, 'Remove'),
                            ),
                          ),
                        ),
                      ),
                      createShow(
                        () => !membersLoading() && members().length === 0,
                        () => h('div', { class: 'user-sessions-empty text-muted' }, 'No members yet'),
                      ),
                      // ── Add member form ──
                      h('div', { class: 'form-stack', style: 'margin-top:0.75rem' },
                        h('div', { class: 'card-label' }, 'Add Member'),
                        h('input', {
                          type: 'text',
                          class: 'search-input',
                          placeholder: 'User ID (e.g. usr_...)',
                          value: addUserId,
                          onInput: (e: Event) => setAddUserId((e.target as HTMLInputElement).value),
                        }),
                        h('select', {
                          class: 'filter-select',
                          value: addRole,
                          onChange: (e: Event) => setAddRole((e.target as HTMLSelectElement).value),
                        },
                          h('option', { value: 'member' }, 'member'),
                          h('option', { value: 'admin' }, 'admin'),
                          h('option', { value: 'owner' }, 'owner'),
                        ),
                        h('button', {
                          class: 'btn btn-xs btn-primary',
                          disabled: () => !addUserId().trim(),
                          onClick: () => handleAddMember(org.id),
                        }, 'Add'),
                      ),
                    ),
                  ),
                  // Footer actions
                  h('div', { class: 'user-detail-footer' },
                    h('div', { class: 'user-detail-footer-meta text-muted' },
                      'ID: ' + org.id,
                    ),
                    h('div', { class: 'user-detail-footer-actions' },
                      h('button', {
                        class: 'btn btn-xs btn-outline btn-danger',
                        onClick: () => {
                          if (confirm('Delete organization "' + org.name + '"? This cannot be undone.')) {
                            void deleteOrganization(tenantId(), org.id);
                            setExpandedId(null);
                            setMembers([]);
                          }
                        },
                      }, 'Delete Organization'),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
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
