// ---------------------------------------------------------------------------
// TenantAdmin.ts — Tenant admin console with sidebar + view switching
// ---------------------------------------------------------------------------

import { detectEnv } from '../../core/utils';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { StatCard } from '../../core/components/ui/StatCard';
import { DataTable } from '../../core/components/ui/DataTable';
import { Pager } from '../../core/components/ui/Pager';
import { Panel, PanelTitle } from '../../core/components/ui/Panel';
import { Badge } from '../../core/components/ui/Badge';
import { Pill } from '../../core/components/ui/Pill';
import { fmtDate } from '../../core/utils';
import type { TenantAdminStore } from './store';
import type { TenantUser, TenantRole, TenantWebhook, AuditEntry } from './store';
import type { ToastController } from '../../core/hooks/createToast';
import type { TenantAdminView } from '../../core/types';

import { h, createSignal, createShow, createSwitch } from 'formajs';
import { Icon } from '../../core/components/ui/Icon';
import { FAuthMark } from '../../platform/console/helpers';

/** Render an Icon component and return its outerHTML for Sidebar innerHTML usage. */
function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

// -- Modal overlay helper ----------------------------------------------------

function Modal(
  visible: () => boolean,
  onClose: () => void,
  title: string,
  ...children: unknown[]
) {
  return createShow(
    visible,
    () => h('div', {
      class: 'modal-overlay',
      onClick: (e: MouseEvent) => { if ((e.target as HTMLElement).classList.contains('modal-overlay')) onClose(); },
    },
      h('div', { class: 'modal-content' },
        h('div', { class: 'modal-header' },
          h('h2', null, title),
          h('button', { class: 'btn small', onClick: onClose }, 'X'),
        ),
        h('div', { class: 'modal-body' }, ...children),
      ),
    ),
  );
}

// ============================================================================
// Views
// ============================================================================

// -- Stats View -------------------------------------------------------------

function StatsView(s: TenantAdminStore) {
  const { store } = s;

  const statGrid = h('div', { class: 'stat-grid' },
    StatCard({ label: 'Total Users', value: () => store.stats?.total_users ?? 0 }),
    StatCard({ label: 'Verified', value: () => store.stats?.verified_users ?? 0 }),
    StatCard({ label: 'Blocked', value: () => store.stats?.blocked_users ?? 0 }),
    StatCard({ label: 'Active Sessions', value: () => store.stats?.active_sessions ?? 0 }),
    StatCard({ label: 'Logins (24h)', value: () => store.stats?.logins_24h ?? 0 }),
    StatCard({ label: 'Failed Logins (24h)', value: () => store.stats?.failed_logins_24h ?? 0 }),
  );

  const providerPanel = Panel({},
    PanelTitle(
      h('span', null, 'Auth Provider Breakdown'),
      Badge('credentials'),
    ),
    createShow(
      () => store.stats?.providers != null && Object.keys(store.stats.providers).length > 0,
      () => {
        const providers = store.stats?.providers ?? {};
        const entries = Object.entries(providers);
        const maxVal = entries.reduce((mx, [, c]) => Math.max(mx, c), 1);
        return h('div', { class: 'chart-list' },
          ...entries.map(([provider, count]) =>
            h('div', { class: 'chart-row' },
              h('span', null, provider),
              h('div', { class: 'bar' },
                h('span', {
                  style: 'width:' + Math.max(4, Math.round((count / maxVal) * 100)) + '%',
                }),
              ),
              h('strong', null, String(count)),
            ),
          ),
        );
      },
      () => h('div', { class: 'empty p-md text-muted-dim' }, 'No provider data'),
    ),
  );

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' }, statGrid),
    h('div', { class: 'view-section' }, providerPanel),
  );
}

// -- Users View -------------------------------------------------------------

function UsersView(s: TenantAdminStore) {
  const { store, userPager, changeUserPage } = s;
  const { createUser, deleteUser, blockUser, unblockUser } = s;
  const { openCreateUser, closeCreateUser } = s;

  // Create user form signals
  const [newEmail, setNewEmail] = createSignal('');
  const [newPassword, setNewPassword] = createSignal('');
  const [formError, setFormError] = createSignal('');

  async function handleCreate() {
    const email = newEmail().trim();
    const password = newPassword();
    if (!email || !password) {
      setFormError('Email and password are required');
      return;
    }
    try {
      setFormError('');
      await createUser(email, password);
      setNewEmail('');
      setNewPassword('');
    } catch (e) {
      setFormError(e instanceof Error ? e.message : 'Create failed');
    }
  }

  const modal = Modal(
    () => store.showCreateUser,
    closeCreateUser,
    'Create User',
    h('div', { class: 'form-stack' },
      createShow(() => formError() !== '', () => h('div', { class: 'alert error show' }, () => formError())),
      h('label', null, 'Email'),
      h('input', {
        type: 'email',
        class: 'input',
        placeholder: 'user@example.com',
        value: newEmail,
        onInput: (e: Event) => setNewEmail((e.target as HTMLInputElement).value),
      }),
      h('label', null, 'Password'),
      h('input', {
        type: 'password',
        class: 'input',
        placeholder: 'Strong password',
        value: newPassword,
        onInput: (e: Event) => setNewPassword((e.target as HTMLInputElement).value),
      }),
      h('button', { class: 'btn primary', onClick: handleCreate }, 'Create User'),
    ),
  );

  const table = DataTable<TenantUser>({
    headers: ['Email', 'Name', 'Verified', 'Created', 'Actions'],
    rows: () => store.users,
    keyFn: (u) => u.id,
    renderRow: (u) => h('tr', null,
      h('td', null,
        h('span', null, u.email),
        h('br'),
        h('code', { class: 'text-muted text-xs' }, u.id),
      ),
      h('td', null, u.name || '-'),
      h('td', null, u.email_verified ? Pill('verified') : Pill('unverified')),
      h('td', null, fmtDate(u.created_at)),
      h('td', null,
        h('div', { class: 'action-row' },
          h('button', {
            class: 'btn small warning',
            onClick: () => blockUser(u.id),
          }, 'Block'),
          h('button', {
            class: 'btn small',
            onClick: () => unblockUser(u.id),
          }, 'Unblock'),
          h('button', {
            class: 'btn small danger',
            onClick: () => {
              if (confirm('Delete user ' + u.email + '?')) deleteUser(u.id);
            },
          }, 'Delete'),
        ),
      ),
    ),
    emptyText: 'No users found',
  });

  const pager = Pager({
    total: () => store.userTotal,
    page: () => userPager.page(),
    onNavigate: (dir) => changeUserPage(dir),
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Users'),
          h('div', { class: 'flex gap-sm items-center' },
            h('span', { class: 'badge' }, () => String(store.userTotal) + ' total'),
            h('button', { class: 'btn small primary', onClick: openCreateUser }, '+ Create User'),
          ),
        ),
        table,
        pager,
      ),
    ),
    modal,
  );
}

// -- Roles View -------------------------------------------------------------

function RolesView(s: TenantAdminStore) {
  const { store, createRole, deleteRole, openCreateRole, closeCreateRole } = s;

  const [newName, setNewName] = createSignal('');
  const [newDesc, setNewDesc] = createSignal('');
  const [newPerms, setNewPerms] = createSignal('');
  const [formError, setFormError] = createSignal('');

  async function handleCreate() {
    const name = newName().trim();
    const permsRaw = newPerms().trim();
    if (!name) {
      setFormError('Role name is required');
      return;
    }
    const permissions = permsRaw ? permsRaw.split(',').map((p: string) => p.trim()).filter(Boolean) : [];
    try {
      setFormError('');
      await createRole(name, permissions, newDesc());
      setNewName('');
      setNewDesc('');
      setNewPerms('');
    } catch (e) {
      setFormError(e instanceof Error ? e.message : 'Create failed');
    }
  }

  const modal = Modal(
    () => store.showCreateRole,
    closeCreateRole,
    'Create Role',
    h('div', { class: 'form-stack' },
      createShow(() => formError() !== '', () => h('div', { class: 'alert error show' }, () => formError())),
      h('label', null, 'Name'),
      h('input', {
        type: 'text',
        class: 'input',
        placeholder: 'e.g. editor',
        value: newName,
        onInput: (e: Event) => setNewName((e.target as HTMLInputElement).value),
      }),
      h('label', null, 'Description'),
      h('input', {
        type: 'text',
        class: 'input',
        placeholder: 'Optional description',
        value: newDesc,
        onInput: (e: Event) => setNewDesc((e.target as HTMLInputElement).value),
      }),
      h('label', null, 'Permissions (comma-separated)'),
      h('input', {
        type: 'text',
        class: 'input',
        placeholder: 'user:read, user:write, audit:read',
        value: newPerms,
        onInput: (e: Event) => setNewPerms((e.target as HTMLInputElement).value),
      }),
      h('button', { class: 'btn primary', onClick: handleCreate }, 'Create Role'),
    ),
  );

  const table = DataTable<TenantRole>({
    headers: ['Name', 'System', 'Permissions', 'Created', 'Actions'],
    rows: () => store.roles,
    keyFn: (r) => r.id,
    renderRow: (r) => h('tr', null,
      h('td', null,
        h('strong', null, r.name),
        r.description ? h('div', { class: 'text-muted text-sm' }, r.description) : null,
      ),
      h('td', null, r.is_system ? Badge('system', 'info') : Badge('custom')),
      h('td', null,
        h('div', { class: 'flex flex-wrap gap-xs' },
          ...((r.permissions || []) as string[]).map((p: string) =>
            h('code', { class: 'code-tag' }, p),
          ),
        ),
      ),
      h('td', null, fmtDate(r.created_at)),
      h('td', null,
        r.is_system
          ? h('span', { class: 'text-muted' }, '-')
          : h('button', {
              class: 'btn small danger',
              onClick: () => {
                if (confirm('Delete role "' + r.name + '"?')) deleteRole(r.id);
              },
            }, 'Delete'),
      ),
    ),
    emptyText: 'No roles defined',
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Roles'),
          h('button', { class: 'btn small primary', onClick: openCreateRole }, '+ Create Role'),
        ),
        table,
      ),
    ),
    modal,
  );
}

// -- Webhooks View ----------------------------------------------------------

function WebhooksView(s: TenantAdminStore) {
  const { store, createWebhook, deleteWebhook, testWebhook } = s;
  const { openCreateWebhook, closeCreateWebhook, dismissWebhookSecret } = s;

  const WEBHOOK_EVENTS = [
    'user.registered', 'user.logged_in', 'user.logged_out', 'user.deleted',
    'user.email_verified', 'user.password_reset', 'user.role_changed',
  ];

  const [newUrl, setNewUrl] = createSignal('');
  const [newDesc, setNewDesc] = createSignal('');
  const [selectedEvents, setSelectedEvents] = createSignal<string[]>([]);
  const [formError, setFormError] = createSignal('');

  function toggleEvent(ev: string) {
    const current = selectedEvents();
    if (current.includes(ev)) {
      setSelectedEvents(current.filter((e: string) => e !== ev));
    } else {
      setSelectedEvents([...current, ev]);
    }
  }

  async function handleCreate() {
    const url = newUrl().trim();
    const events = selectedEvents();
    if (!url) {
      setFormError('URL is required');
      return;
    }
    if (events.length === 0) {
      setFormError('Select at least one event');
      return;
    }
    try {
      setFormError('');
      await createWebhook(url, events, newDesc());
      setNewUrl('');
      setNewDesc('');
      setSelectedEvents([]);
    } catch (e) {
      setFormError(e instanceof Error ? e.message : 'Create failed');
    }
  }

  const modal = Modal(
    () => store.showCreateWebhook,
    closeCreateWebhook,
    'Create Webhook',
    h('div', { class: 'form-stack' },
      createShow(() => formError() !== '', () => h('div', { class: 'alert error show' }, () => formError())),
      h('label', null, 'Endpoint URL'),
      h('input', {
        type: 'url',
        class: 'input',
        placeholder: 'https://example.com/webhooks',
        value: newUrl,
        onInput: (e: Event) => setNewUrl((e.target as HTMLInputElement).value),
      }),
      h('label', null, 'Description'),
      h('input', {
        type: 'text',
        class: 'input',
        placeholder: 'Optional description',
        value: newDesc,
        onInput: (e: Event) => setNewDesc((e.target as HTMLInputElement).value),
      }),
      h('label', null, 'Events'),
      h('div', { class: 'flex flex-wrap gap-sm mb-md' },
        ...WEBHOOK_EVENTS.map((ev) =>
          h('label', { class: 'checkbox-label' },
            h('input', {
              type: 'checkbox',
              checked: () => selectedEvents().includes(ev),
              onChange: () => toggleEvent(ev),
            }),
            h('code', null, ev),
          ),
        ),
      ),
      h('button', { class: 'btn primary', onClick: handleCreate }, 'Create Webhook'),
    ),
  );

  // Secret banner
  const secretBanner = createShow(
    () => store.webhookSecret != null,
    () => Panel({},
      PanelTitle(
        h('span', null, 'Webhook Secret'),
        Badge('shown once', 'warning'),
      ),
      h('div', { class: 'flex items-center gap-md' },
        h('code', { class: 'secret-display' }, () => store.webhookSecret ?? ''),
        h('button', { class: 'btn small', onClick: () => {
          navigator.clipboard.writeText(store.webhookSecret ?? '').catch(() => {});
        } }, 'Copy'),
        h('button', { class: 'btn small', onClick: dismissWebhookSecret }, 'Dismiss'),
      ),
    ),
  );

  const table = DataTable<TenantWebhook>({
    headers: ['URL', 'Events', 'Active', 'Created', 'Actions'],
    rows: () => store.webhooks,
    keyFn: (w) => w.id,
    renderRow: (w) => h('tr', null,
      h('td', null,
        h('code', { class: 'text-sm break-all' }, w.url),
        w.description ? h('div', { class: 'text-muted text-sm' }, w.description) : null,
      ),
      h('td', null,
        h('div', { class: 'flex flex-wrap gap-xs' },
          ...(w.events || []).map((ev: string) =>
            h('code', { class: 'code-tag' }, ev),
          ),
        ),
      ),
      h('td', null, w.active ? Badge('active', 'success') : Badge('inactive', 'warning')),
      h('td', null, fmtDate(w.created_at)),
      h('td', null,
        h('div', { class: 'action-row' },
          h('button', {
            class: 'btn small',
            onClick: () => testWebhook(w.id),
          }, 'Test'),
          h('button', {
            class: 'btn small danger',
            onClick: () => {
              if (confirm('Delete this webhook?')) deleteWebhook(w.id);
            },
          }, 'Delete'),
        ),
      ),
    ),
    emptyText: 'No webhooks configured',
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' }, secretBanner),
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Webhooks'),
          h('button', { class: 'btn small primary', onClick: openCreateWebhook }, '+ Create Webhook'),
        ),
        table,
      ),
    ),
    modal,
  );
}

// -- Audit View -------------------------------------------------------------

function AuditView(s: TenantAdminStore) {
  const { store, auditPager, changeAuditPage } = s;

  const table = DataTable<AuditEntry>({
    headers: ['Time', 'Email', 'Result', 'IP', 'Failure Reason'],
    rows: () => store.auditLog,
    keyFn: (entry) => entry.id,
    renderRow: (entry) => h('tr', null,
      h('td', null, fmtDate(entry.created_at)),
      h('td', null,
        h('span', null, entry.email),
        entry.user_id ? h('div', null, h('code', { class: 'text-muted text-xxs' }, entry.user_id)) : null,
      ),
      h('td', null, entry.success ? Badge('success', 'success') : Badge('failed', 'danger')),
      h('td', null, h('code', { class: 'text-sm' }, entry.ip_address || '-')),
      h('td', null, entry.failure_reason || '-'),
    ),
    emptyText: 'No audit log entries',
  });

  const pager = Pager({
    total: () => store.auditTotal,
    page: () => auditPager.page(),
    onNavigate: (dir) => changeAuditPage(dir),
  });

  return h('div', { class: 'content' },
    h('div', { class: 'view-section' },
      Panel({},
        PanelTitle(
          h('span', null, 'Audit Log'),
          h('span', { class: 'text-muted' }, 'Login attempts + authentication events'),
        ),
        table,
        pager,
      ),
    ),
  );
}

// ============================================================================
// TenantAdmin — App Shell
// ============================================================================

export function TenantAdmin(
  adminStore: TenantAdminStore,
  toast: ToastController,
  onSignout: () => void,
) {
  const { store, navigate, refreshAll } = adminStore;

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Admin Console', mark: FAuthMark(20), subtitle: 'TENANT ADMIN' },
      items: [
        { id: 'stats', label: 'Stats', icon: iconSvg('overview-grid') },
        { id: 'users', label: 'Users', icon: iconSvg('customers') },
        { id: 'roles', label: 'Roles', icon: iconSvg('security') },
        { id: 'webhooks', label: 'Webhooks', icon: iconSvg('webhooks') },
        { id: 'audit', label: 'Audit', icon: iconSvg('timeline') },
      ],
      footer: [
        { id: 'refresh', label: 'Refresh', icon: iconSvg('refresh-stroke') },
        { id: 'signout', label: 'Sign Out', icon: iconSvg('signout'), danger: true },
      ],
      activeView: () => store.view,
      onNavigate: (id: string) => {
        if (id === 'refresh') { refreshAll(); return; }
        if (id === 'signout') { onSignout(); return; }
        navigate(id as TenantAdminView);
      },
      env: detectEnv(),
    }),
    h('main', { class: 'main' },
      LoadingBar(() => store.loading),
      createSwitch(
        () => store.view,
        [
          { match: 'stats', render: () => StatsView(adminStore) },
          { match: 'users', render: () => UsersView(adminStore) },
          { match: 'roles', render: () => RolesView(adminStore) },
          { match: 'webhooks', render: () => WebhooksView(adminStore) },
          { match: 'audit', render: () => AuditView(adminStore) },
        ],
      ),
    ),
    ToastView(toast),
  );
}
