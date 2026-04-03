import { h, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { selectedTenant, updateTenant, deleteTenant, toast, backToTenants, loadTenants, type Tenant } from '../store';

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
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Settings'),
    ),

    // Branding
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Branding'),
      Field({ label: 'App Name', name: 'app-name', value: appName, onChange: setAppName, placeholder: 'My App' }),
      h('div', { class: 'field' },
        h('label', null, 'Primary Color'),
        h('div', { class: 'flex items-center gap-sm' },
          h('input', {
            type: 'color', value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            class: 'color-picker-btn',
          }),
          h('input', {
            type: 'text', value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            class: 'flex-1',
          }),
        ),
      ),
      h('button', {
        class: 'btn btn-primary btn-sm mt-md',
        disabled: saving,
        onClick: saveBranding,
      }, () => saving() ? 'Saving...' : 'Save'),
    ),

    // Info
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Tenant Info'),
      h('div', { class: 'detail-grid' },
        InfoRow('Slug', tenant.slug),
        InfoRow('Plan', tenant.plan),
        InfoRow('Status', tenant.status),
        InfoRow('Created', new Date(tenant.created_at).toLocaleDateString()),
      ),
    ),

    // Danger zone
    h('div', { class: 'console-card danger-zone' },
      h('div', { class: 'card-label' }, 'Danger Zone'),
      h('p', { class: 'text-muted text-xs mb-md' },
        'Deleting a tenant removes all users, sessions, and configuration. This cannot be undone.',
      ),
      h('input', {
        type: 'text',
        class: 'search-input mb-sm',
        placeholder: () => 'Type "' + tenant.slug + '" to confirm',
        value: confirmDelete,
        onInput: (e: Event) => setConfirmDelete((e.target as HTMLInputElement).value),
      }),
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
  );
}

function InfoRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
