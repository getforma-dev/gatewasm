import { h, createEffect, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import { selectedTenant, updateTenant, deleteTenant, toast, backToTenants, loadTenants } from '../store';
import { normalizeTenantSlugInput } from '../../tenantSlug';

export function SettingsView() {
  const tenant = selectedTenant()!;
  const tenantState = () => selectedTenant() || tenant;
  const loginUrl = () => '/t/' + tenantState().slug + '/login';
  const previewLoginUrl = () => loginUrl() + '?preview=1';
  const siteUrl = () => '/t/' + tenantState().slug + '/site';
  const [appName, setAppName] = createSignal(tenant.name || '');
  const [slug, setSlug] = createSignal(tenant.slug || '');
  const [color, setColor] = createSignal(tenant.primary_color || '#6366f1');
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal('');
  const [syncedTenantId, setSyncedTenantId] = createSignal(tenant.id);
  const [syncedName, setSyncedName] = createSignal(tenant.name || '');
  const [syncedSlug, setSyncedSlug] = createSignal(tenant.slug || '');
  const [syncedColor, setSyncedColor] = createSignal(tenant.primary_color || '#6366f1');
  const isDirty = () => appName() !== syncedName() || slug() !== syncedSlug() || color() !== syncedColor();

  createEffect(() => {
    const currentTenant = tenantState();
    const nextName = currentTenant.name || '';
    const nextSlug = currentTenant.slug || '';
    const nextColor = currentTenant.primary_color || '#6366f1';
    const tenantChanged = syncedTenantId() !== currentTenant.id;
    const pristine = appName() === syncedName() && slug() === syncedSlug() && color() === syncedColor();

    if (tenantChanged || pristine) {
      setAppName(nextName);
      setSlug(nextSlug);
      setColor(nextColor);
    }
    if (tenantChanged) {
      setConfirmDelete('');
    }
    setSyncedTenantId(currentTenant.id);
    setSyncedName(nextName);
    setSyncedSlug(nextSlug);
    setSyncedColor(nextColor);
  });

  function resetDraft() {
    setAppName(syncedName());
    setSlug(syncedSlug());
    setColor(syncedColor());
  }

  async function saveBranding() {
    const nextName = appName().trim();
    const nextSlug = slug().trim();

    if (!nextName) {
      toast.show('App name is required', 'error');
      return;
    }
    if (!nextSlug) {
      toast.show('Tenant slug is required', 'error');
      return;
    }

    setSaving(true);
    const updated = await updateTenant(tenantState().id, {
      name: nextName,
      slug: nextSlug,
      primary_color: color(),
    });
    setSaving(false);
    if (updated) {
      toast.show('Settings saved', 'success');
    }
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Tenant Configuration'),
        h('h2', { class: 'view-title' }, 'Settings'),
        h('p', { class: 'view-subtitle text-muted' }, () => 'Manage naming, URLs, branding, and lifecycle settings for /t/' + tenantState().slug),
      ),
      h('p', { class: 'view-header-note' },
        'This is where owners can tighten the tenant identity, including changing the shareable slug when the generated URL is not good enough.',
      ),
    ),
    h('div', { class: 'settings-layout' },
      h('div', { class: 'settings-main' },
        h('div', { class: 'console-card' },
        h('div', { class: 'card-label' }, 'Identity & Branding'),
        h('p', { class: 'card-copy' }, 'Set the app name, tenant URL, and primary accent used across the hosted tenant experience.' ),
        h('div', { class: 'form-stack' },
            Field({
              label: 'App Name',
              name: 'app-name',
              value: appName,
              onChange: setAppName,
              placeholder: 'My App',
              hint: 'Shown in the console and hosted tenant pages.',
            }),
            Field({
              label: 'Tenant Slug',
              name: 'tenant-slug',
              value: slug,
              onChange: (value) => setSlug(normalizeTenantSlugInput(value)),
              placeholder: 'my-app',
              hint: 'Changing this updates every /t/{slug}/... URL for the tenant.',
            }),
            h('div', { class: 'tenant-create-preview' },
              h('span', { class: 'detail-label' }, 'Tenant URL'),
              h('div', { class: 'inline-copy-row' },
                h('code', { class: 'code-block code-inline-block break-all' },
                  () => slug() ? '/t/' + slug() : '/t/your-slug',
                ),
                CopyButton({
                  text: () => slug() ? '/t/' + slug() : '/t/your-slug',
                  label: 'Copy URL',
                }),
              ),
            ),
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
          h('div', { class: () => 'editor-dirty-bar' + (isDirty() ? ' active' : '') },
            h('div', { class: 'editor-dirty-copy' },
              h('span', { class: 'editor-dirty-label' }, 'Unsaved changes'),
              h('span', { class: 'editor-dirty-description' }, 'Save the tenant identity update before sharing the new URL with anyone else.'),
            ),
            h('div', { class: 'editor-dirty-actions' },
              h('button', {
                type: 'button',
                class: 'btn btn-sm btn-ghost',
                disabled: () => saving() || !isDirty(),
                onClick: resetDraft,
              }, 'Reset'),
              h('button', {
                type: 'button',
                class: 'btn btn-primary btn-sm',
                disabled: () => saving() || !isDirty(),
                onClick: saveBranding,
              }, () => saving() ? 'Saving...' : 'Save'),
            ),
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
            placeholder: () => 'Type "' + tenantState().slug + '" to confirm',
            value: confirmDelete,
            onInput: (e: Event) => setConfirmDelete((e.target as HTMLInputElement).value),
          }),
          h('div', { class: 'card-actions' },
            h('button', {
              class: 'btn btn-sm btn-danger',
              disabled: () => confirmDelete() !== tenantState().slug,
              onClick: async () => {
                const ok = await deleteTenant(tenantState().id, confirmDelete());
                if (ok) {
                  await loadTenants();
                  backToTenants();
                }
              },
            }, 'Delete Tenant'),
          ),
        ),
      ),
      h('aside', { class: 'settings-sidebar settings-sidebar-stack' },
        h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Tenant Info'),
          h('div', { class: 'detail-grid' },
            InfoRow('Slug', tenantState().slug),
            InfoRow('Login URL', loginUrl()),
            InfoRow('Plan', tenantState().plan),
            InfoRow('Status', tenantState().status),
            InfoRow('Created', new Date(tenantState().created_at).toLocaleDateString()),
          ),
        ),
        h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Surface URLs'),
          h('div', { class: 'surface-link-list' },
            SettingsLinkRow('Login Preview', previewLoginUrl()),
            SettingsLinkRow('Protected Site', siteUrl()),
          ),
        ),
      ),
    ),
  );
}

function SettingsLinkRow(label: string, href: string) {
  return h('div', { class: 'surface-link-row' },
    h('div', { class: 'surface-link-copy' },
      h('span', { class: 'detail-label' }, label),
      h('code', { class: 'surface-link-value' }, href),
    ),
    h('div', { class: 'surface-link-actions' },
      h('a', {
        href,
        target: '_blank',
        class: 'btn btn-sm btn-ghost',
      }, 'Open'),
      CopyButton({ text: href, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
    ),
  );
}

function InfoRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
