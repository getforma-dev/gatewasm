import { h, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { selectedTenant, updateTenant, deleteTenant, toast, backToTenants, loadTenants } from '../store';

export function SettingsView() {
  const tenant = selectedTenant()!;
  const [appName, setAppName] = createSignal(tenant.name || '');
  const [color, setColor] = createSignal(tenant.primary_color || '#6366f1');
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal('');

  async function saveBranding() {
    setSaving(true);
    await updateTenant(tenant.id, { name: appName(), primary_color: color() });
    setSaving(false);
    toast.show('Settings saved', 'success');
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Configuration'),
        h('h2', { class: 'view-title' }, 'Settings'),
        h('p', { class: 'view-subtitle text-muted' }, 'Manage branding and lifecycle settings for /t/' + tenant.slug),
      ),
      h('p', { class: 'view-header-note' },
        'Branding controls stay separated from irreversible actions so the page is easier to scan and safer to use.',
      ),
    ),
    h('div', { class: 'settings-layout' },
      h('div', { class: 'settings-main' },
        h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Branding'),
          h('p', { class: 'card-copy' }, 'Set the app name and primary accent used across the hosted tenant experience.' ),
          h('div', { class: 'form-stack' },
            Field({ label: 'App Name', name: 'app-name', value: appName, onChange: setAppName, placeholder: 'My App' }),
            h('div', { class: 'field' },
              h('label', null, 'Primary Color'),
              h('div', { class: 'inline-input-row' },
                h('input', {
                  type: 'color',
                  value: color,
                  onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
                  class: 'color-picker-btn',
                }),
                h('input', {
                  type: 'text',
                  value: color,
                  onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
                  class: 'flex-1',
                }),
              ),
            ),
          ),
          h('div', { class: 'card-actions' },
            h('button', {
              class: 'btn btn-primary btn-sm',
              disabled: saving,
              onClick: saveBranding,
            }, () => saving() ? 'Saving...' : 'Save Settings'),
          ),
        ),
        h('div', { class: 'console-card danger-zone' },
          h('div', { class: 'card-label' }, 'Danger Zone'),
          h('p', { class: 'card-copy' },
            'Deleting a tenant removes all users, sessions, and configuration. This cannot be undone.',
          ),
          h('input', {
            type: 'text',
            class: 'search-input',
            placeholder: () => 'Type "' + tenant.slug + '" to confirm',
            value: confirmDelete,
            onInput: (e: Event) => setConfirmDelete((e.target as HTMLInputElement).value),
          }),
          h('div', { class: 'card-actions' },
            h('button', {
              class: 'btn btn-sm btn-danger',
              disabled: () => confirmDelete() !== tenant.slug,
              onClick: async () => {
                const ok = await deleteTenant(tenant.id, confirmDelete());
                if (ok) {
                  await loadTenants();
                  backToTenants();
                }
              },
            }, 'Delete Tenant'),
          ),
        ),
      ),
      h('aside', { class: 'console-card settings-sidebar' },
        h('div', { class: 'card-label' }, 'Tenant Info'),
        h('div', { class: 'detail-grid' },
          InfoRow('Slug', tenant.slug),
          InfoRow('Plan', tenant.plan),
          InfoRow('Status', tenant.status),
          InfoRow('Created', new Date(tenant.created_at).toLocaleDateString()),
        ),
      ),
    ),
  );
}

function InfoRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
