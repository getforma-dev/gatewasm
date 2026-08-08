// ---------------------------------------------------------------------------
// TenantDashboard.ts — Tenant dashboard shell with sidebar + view switching
// ---------------------------------------------------------------------------

import { detectEnv } from '../../core/utils';
import { Sidebar } from '../../core/components/ui/Sidebar';
import { LoadingBar } from '../../core/components/ui/LoadingBar';
import { ToastView } from '../../core/components/ui/Toast';
import { Header } from '../../core/components/ui/Header';
import { Badge } from '../../core/components/ui/Badge';
import { CodeBlock } from '../../core/components/ui/CodeBlock';
import { CopyButton } from '../../core/components/ui/CopyButton';
import { DataTable } from '../../core/components/ui/DataTable';
import { Panel } from '../../core/components/ui/Panel';
import { Field } from '../../core/components/ui/Field';
import { fmtDate } from '../../core/utils';
import type { TenantDashboardStore, LoginHistoryEntry } from './store';
import type { ToastController } from '../../core/hooks/createToast';
import type { TenantDashboardView } from '../../core/types';

import { h, createSignal, createSwitch, createShow } from 'formajs';
import { Icon } from '../../core/components/ui/Icon';
import { FAuthMark } from '../../platform/console/helpers';

/** Render an Icon component and return its outerHTML for Sidebar innerHTML usage. */
function iconSvg(name: string, size = 18): string {
  return Icon({ name, size }).outerHTML;
}

// -- Profile View ------------------------------------------------------------

function ProfileView(dashStore: TenantDashboardStore, toast: ToastController, onSignout: () => void) {
  const [curPw, setCurPw] = createSignal('');
  const [newPw, setNewPw] = createSignal('');
  const [confirmDelete, setConfirmDelete] = createSignal(false);

  function handleChangePassword() {
    const cur = curPw();
    const pw = newPw();
    if (!cur || !pw) {
      toast.show('Both fields are required', 'error');
      return;
    }
    if (pw.length < 8) {
      toast.show('New password must be at least 8 characters', 'error');
      return;
    }
    dashStore.changePassword(cur, pw).then(() => {
      setCurPw('');
      setNewPw('');
    }).catch(() => {});
  }

  function handleDeleteAccount() {
    if (!confirmDelete()) {
      setConfirmDelete(true);
      return;
    }
    dashStore.deleteAccount().then(() => {
      onSignout();
    }).catch(() => {
      setConfirmDelete(false);
    });
  }

  return h('div', { class: 'content' },
    Header({ title: 'Profile' }),

    // Profile card
    createShow(
      () => dashStore.store.profile != null,
      () => {
        const p = dashStore.store.profile!;
        const initials = (p.name || p.email || '?').charAt(0).toUpperCase();

        return h('div', { class: 'flex-col gap-lg' },
          // Avatar + name header
          Panel(undefined,
            h('div', { class: 'flex items-center gap-md mb-md' },
              h('div', { class: 'avatar large' }, initials),
              h('div', null,
                h('div', { class: 'heading-md' }, p.name || 'No name set'),
                h('div', { class: 'text-muted' }, p.email),
              ),
            ),
            // Fields
            h('div', { class: 'detail-grid' },
              h('div', { class: 'detail-row' },
                h('span', { class: 'detail-label' }, 'User ID'),
                h('code', { class: 'detail-value mono' }, p.id),
              ),
              h('div', { class: 'detail-row' },
                h('span', { class: 'detail-label' }, 'Email Verified'),
                h('span', { class: 'detail-value' },
                  p.email_verified
                    ? Badge('Verified', 'success')
                    : Badge('Unverified', 'danger'),
                ),
              ),
              h('div', { class: 'detail-row' },
                h('span', { class: 'detail-label' }, 'Roles'),
                h('span', { class: 'detail-value' },
                  p.roles && p.roles.length > 0
                    ? h('span', { class: 'flex gap-xs flex-wrap' },
                        ...p.roles.map((r: string) => Badge(r, 'info')),
                      )
                    : Badge('user', 'info'),
                ),
              ),
              h('div', { class: 'detail-row' },
                h('span', { class: 'detail-label' }, 'Created'),
                h('span', { class: 'detail-value' }, fmtDate(p.created_at)),
              ),
            ),
          ),

          // Change password
          Panel(undefined,
            h('h3', { class: 'heading-sm mb-md' }, 'Change Password'),
            Field({
              label: 'Current Password',
              name: 'current_password',
              type: 'password',
              value: curPw,
              onChange: setCurPw,
              placeholder: 'Enter current password',
            }),
            Field({
              label: 'New Password',
              name: 'new_password',
              type: 'password',
              value: newPw,
              onChange: setNewPw,
              placeholder: 'Min 8 characters',
            }),
            h('button', {
              class: 'btn primary mt-sm',
              onClick: handleChangePassword,
            }, 'Update Password'),
          ),

          // Danger zone
          Panel({ class: 'danger-zone' },
            h('h3', { class: 'heading-sm mb-xs danger-heading' }, 'Danger Zone'),
            h('p', { class: 'text-muted text-sm mb-md' },
              'Permanently delete your account. This action cannot be undone.',
            ),
            h('button', {
              class: () => confirmDelete() ? 'btn danger' : 'btn danger-outline',
              onClick: handleDeleteAccount,
            }, () => confirmDelete() ? 'Confirm Delete' : 'Delete Account'),
          ),
        );
      },
    ),
  );
}

// -- History View ------------------------------------------------------------

function HistoryView(dashStore: TenantDashboardStore) {
  return h('div', { class: 'content' },
    Header({ title: 'Login History', subtitle: 'Recent authentication attempts' }),
    DataTable<LoginHistoryEntry>({
      headers: ['Date', 'Status', 'IP Address', 'User Agent'],
      rows: () => dashStore.store.history,
      keyFn: (entry) => entry.created_at + (entry.ip_address || ''),
      renderRow: (entry) =>
        h('tr', null,
          h('td', null, fmtDate(entry.created_at)),
          h('td', null,
            entry.success
              ? Badge('Success', 'success')
              : Badge('Failed', 'danger'),
          ),
          h('td', { class: 'mono text-xs' }, entry.ip_address || '-'),
          h('td', { class: 'history-agent text-xs text-muted-dim truncate' }, entry.user_agent || '-'),
        ),
      emptyText: 'No login history yet',
    }),
  );
}

// -- Integration View --------------------------------------------------------

function IntegrationView() {
  const config = (window as any).__TENANT__ || {};
  const serverUrl = config.server_url || window.location.origin;
  const cdnUrl = 'https://cdn.getforma.dev/bridge/v1/gate-bridge.min.js';
  const directUrl = window.location.origin + '/bridge.js';

  const quickstartCode =
`<!-- 1. Add the bridge -->
<script src="${cdnUrl}"><\/script>

<!-- 2. Initialize -->
<script>
  const bridge = await GateAuthBridge.init({
    serverUrl: "${serverUrl}",
    clientId:  "your-client-id",
  });

  // 3. Listen for auth state changes
  bridge.onStateChange(({ state, user }) => {
    if (state === 'authenticated') {
      console.log('Welcome,', user.name);
    }
  });

  // 4. Trigger login
  bridge.login('google');  // or "github", "email"
<\/script>`;

  return h('div', { class: 'content' },
    Header({ title: 'Integration', subtitle: 'Add authentication to your app' }),

    h('div', { class: 'flex-col gap-lg' },
      // Quick Start
      Panel(undefined,
        h('h3', { class: 'heading-sm mb-md' }, 'Quick Start'),
        h('p', { class: 'text-muted text-sm mb-md' },
          'Add Forma Auth to your app in under 5 minutes.',
        ),
        CodeBlock(quickstartCode),
      ),

      // Distribution Options
      Panel(undefined,
        h('h3', { class: 'heading-sm mb-md' }, 'Distribution Options'),
        h('div', { class: 'detail-grid' },
          h('div', { class: 'detail-row' },
            h('span', { class: 'detail-label' }, 'CDN'),
            h('div', { class: 'detail-value flex items-center gap-sm' },
              h('code', { class: 'mono text-xs break-all' }, cdnUrl),
              CopyButton({ text: cdnUrl, class: 'btn small', label: 'Copy', copiedLabel: 'Copied' }),
            ),
          ),
          h('div', { class: 'detail-row' },
            h('span', { class: 'detail-label' }, 'Direct'),
            h('div', { class: 'detail-value flex items-center gap-sm' },
              h('code', { class: 'mono text-xs break-all' }, directUrl),
              CopyButton({ text: directUrl, class: 'btn small', label: 'Copy', copiedLabel: 'Copied' }),
            ),
          ),
          h('div', { class: 'detail-row' },
            h('span', { class: 'detail-label' }, 'Download'),
            h('span', { class: 'detail-value' },
              h('a', {
                href: '/bridge.js',
                download: 'gate-bridge.js',
                class: 'link-accent text-sm',
              }, 'gate-bridge.js'),
              h('span', { class: 'text-muted text-xs ml-sm' }, '(~3.4KB)'),
            ),
          ),
          h('div', { class: 'detail-row' },
            h('span', { class: 'detail-label' }, 'npm'),
            h('span', { class: 'detail-value text-muted text-sm' }, 'Coming soon'),
          ),
        ),
      ),

      // API Reference
      Panel(undefined,
        h('h3', { class: 'heading-sm mb-md' }, 'API Reference'),
        h('p', { class: 'text-muted text-sm mb-md' },
          'Full API documentation and OpenAPI spec:',
        ),
        h('a', {
          href: '/docs',
          target: '_blank',
          class: 'btn outline inline-flex-center api-docs-btn',
        },
          Icon({ name: 'external-link', size: 14 }),
          'Open API Docs',
        ),
      ),
    ),
  );
}

// -- TenantDashboard component -----------------------------------------------

export function TenantDashboard(
  dashStore: TenantDashboardStore,
  toast: ToastController,
  onSignout: () => void,
) {
  const { store, navigate } = dashStore;
  const config = (window as any).__TENANT__ || {};
  const appName = config.name || config.slug || 'Dashboard';

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: appName, mark: FAuthMark(20), subtitle: 'DASHBOARD' },
      items: [
        { id: 'profile', label: 'Profile', icon: iconSvg('profile') },
        { id: 'history', label: 'Login History', icon: iconSvg('timeline') },
        { id: 'integration', label: 'Integration', icon: iconSvg('integrate') },
      ],
      footer: [
        { id: 'signout', label: 'Sign Out', icon: iconSvg('signout'), danger: true },
      ],
      activeView: () => store.view,
      onNavigate: (id: string) => {
        if (id === 'signout') { onSignout(); return; }
        navigate(id as TenantDashboardView);
      },
      env: detectEnv(),
    }),
    h('main', { class: 'main' },
      LoadingBar(() => store.loading),
      createSwitch(
        () => store.view,
        [
          { match: 'profile', render: () => ProfileView(dashStore, toast, onSignout) },
          { match: 'history', render: () => HistoryView(dashStore) },
          { match: 'integration', render: () => IntegrationView() },
        ],
      ),
    ),
    ToastView(toast),
  );
}
