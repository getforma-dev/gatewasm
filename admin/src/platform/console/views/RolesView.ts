import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant, roles, rolesLoading,
  createRole, deleteRole, canEdit, type Role,
} from '../store';

export function RolesView() {
  const [showCreate, setShowCreate] = createSignal(false);
  const [newName, setNewName] = createSignal('');
  const [newDesc, setNewDesc] = createSignal('');

  const tenantId = () => selectedTenant()?.id || '';

  async function handleCreate() {
    const name = newName().trim();
    if (!name) return;
    const ok = await createRole(tenantId(), name, newDesc().trim());
    if (ok) {
      setNewName('');
      setNewDesc('');
      setShowCreate(false);
    }
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Organization'),
        h('h2', { class: 'view-title' }, 'Roles'),
        h('p', { class: 'view-subtitle text-muted' }, () => roles().length + ' roles'),
      ),
      h('div', { class: 'view-header-actions' },
        h('button', {
          class: () => showCreate() ? 'btn btn-sm btn-ghost' : 'clip-btn',
          disabled: () => !canEdit(),
          onClick: () => setShowCreate(!showCreate()),
        }, () => showCreate() ? 'Cancel' : 'Create Role'),
      ),
    ),

    // ── Create form ────────────────────────────────────────────────
    createShow(showCreate, () =>
      h('div', { class: 'console-card' },
        h('div', { class: 'card-label' }, 'New Role'),
        h('div', { class: 'form-stack' },
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: 'Role name (e.g. editor)',
            value: newName,
            onInput: (e: Event) => setNewName((e.target as HTMLInputElement).value),
          }),
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: 'Description (optional)',
            value: newDesc,
            onInput: (e: Event) => setNewDesc((e.target as HTMLInputElement).value),
          }),
          h('div', { class: 'card-actions' },
            h('button', {
              class: 'btn btn-sm btn-primary',
              onClick: handleCreate,
            }, 'Create'),
          ),
        ),
      ),
    ),

    // ── Role list ──────────────────────────────────────────────────
    h('section', { class: 'console-card list-shell section-box section-box-bottom' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'All Roles'),
          h('p', { class: 'card-copy' }, () => roles().length + ' roles (system + custom)'),
        ),
      ),
      createShow(
        () => rolesLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '50%', height: '12px' }),
          Skeleton({ width: '80%', height: '12px' }),
        ),
      ),
      createShow(
        () => !rolesLoading() && roles().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Roles'),
          h('p', { class: 'card-copy' }, 'Create a custom role to get started.'),
        ),
      ),
      createShow(
        () => !rolesLoading() && roles().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => roles().map((role) => RoleRow(role, tenantId())),
        ),
      ),
    ),
  );
}

function RoleRow(role: Role, tenantId: string) {
  return h('div', { class: 'user-row' },
    h('div', { class: 'user-row-summary' },
      h('div', { class: 'user-row-main' },
        h('div', { class: 'user-email' },
          role.name,
          role.is_system
            ? h('span', { class: 'badge badge-subtle' }, 'System')
            : h('span', { class: 'badge badge-subtle' }, 'Custom'),
        ),
        h('div', { class: 'user-meta text-muted' },
          (role.description || 'No description') + ' \u00b7 ' + new Date(role.created_at).toLocaleDateString(),
        ),
      ),
      !role.is_system
        ? createShow(() => canEdit(), () =>
            h('button', {
              class: 'btn btn-sm btn-ghost',
              onClick: () => deleteRole(tenantId, role.id),
            }, 'Delete'),
          )
        : null,
    ),
  );
}
