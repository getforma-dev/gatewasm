import { h, createSignal, createShow } from 'formajs';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import { tenants, rotateTenantKey, type Tenant } from '../store';

export function ApiKeysView() {
  const ownTenants = () => tenants().filter((t) => t.access !== 'team');
  const teamTenants = () => tenants().filter((t) => t.access === 'team');

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Account'),
        h('h2', { class: 'view-title' }, 'API Keys'),
        h('p', { class: 'view-subtitle text-muted' }, 'Management API keys for your tenants'),
      ),
    ),

    // No tenants at all
    createShow(
      () => tenants().length === 0,
      () => h('div', { class: 'console-card' },
        h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Tenants'),
          h('p', { class: 'card-copy' }, 'Create a tenant first to get a management API key.'),
        ),
      ),
    ),

    // Your Workspaces section
    createShow(
      () => ownTenants().length > 0,
      () => h('div', { class: 'console-card list-shell' },
        h('div', { class: 'list-shell-head' },
          h('div', { class: 'section-copy' },
            h('div', { class: 'card-label' }, 'Your Workspaces'),
            h('p', { class: 'card-copy' }, () => ownTenants().length + ' tenant' + (ownTenants().length !== 1 ? 's' : '')),
          ),
        ),
        h('div', { class: 'user-list user-list-shell' },
          () => ownTenants().map((t) => TenantKeyRow(t)),
        ),
      ),
    ),

    // Team Workspaces section
    createShow(
      () => teamTenants().length > 0,
      () => h('div', { class: 'console-card list-shell' },
        h('div', { class: 'list-shell-head' },
          h('div', { class: 'section-copy' },
            h('div', { class: 'card-label' }, 'Team Workspaces'),
            h('p', { class: 'card-copy' }, () => teamTenants().length + ' tenant' + (teamTenants().length !== 1 ? 's' : '') + ' shared with you'),
          ),
        ),
        h('div', { class: 'user-list user-list-shell' },
          () => teamTenants().map((t) => TenantKeyRow(t)),
        ),
      ),
    ),
  );
}

function TenantKeyRow(tenant: Tenant) {
  const [revealed, setRevealed] = createSignal(false);
  const [rotating, setRotating] = createSignal(false);

  async function handleRotate() {
    setRotating(true);
    const newKey = await rotateTenantKey(tenant.id);
    setRotating(false);
    if (newKey) setRevealed(true);
  }

  return h('div', { class: 'user-row' },
    h('div', { class: 'user-detail', style: 'padding: var(--space-md) var(--space-lg)' },
      h('div', { class: 'detail-grid' },
        h('div', { class: 'detail-row' },
          h('span', { class: 'detail-label' }, 'Tenant'),
          h('span', { class: 'detail-value' }, tenant.name),
        ),
        h('div', { class: 'detail-row' },
          h('span', { class: 'detail-label' }, 'Slug'),
          h('code', { class: 'detail-value' }, '/t/' + tenant.slug),
        ),
        h('div', { class: 'detail-row' },
          h('span', { class: 'detail-label' }, 'Key'),
          h('span', { class: 'detail-value' },
            tenant.management_api_key
              ? (revealed()
                  ? h('code', null, tenant.management_api_key)
                  : 'mgmt_\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022')
              : h('span', { class: 'text-muted' }, 'Shown at creation only'),
          ),
        ),
      ),
      h('div', { class: 'user-actions' },
        tenant.management_api_key
          ? h('button', {
              class: 'btn btn-sm btn-ghost',
              onClick: () => setRevealed(!revealed()),
            }, () => revealed() ? 'Hide' : 'Reveal')
          : null,
        createShow(
          () => revealed() && !!tenant.management_api_key,
          () => CopyButton({ text: tenant.management_api_key!, label: 'Copy key' }),
        ),
        h('button', {
          class: 'btn btn-sm btn-ghost',
          disabled: rotating,
          onClick: handleRotate,
        }, () => rotating() ? 'Rotating...' : 'Rotate Key'),
      ),
    ),
  );
}
