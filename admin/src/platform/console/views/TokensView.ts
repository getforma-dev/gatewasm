import { h, createSignal, createShow } from 'formajs';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import { selectedTenant, jwks } from '../store';

export function TokensView() {
  const tenantState = () => selectedTenant();

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'End User'),
        h('h2', { class: 'view-title' }, 'Tokens'),
        h('p', { class: 'view-subtitle text-muted' }, 'JWKS public keys, API key management, and token configuration'),
      ),
    ),

    // ── JWKS Public Keys ──────────────────────────────────────────────
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'JWKS Public Keys'),
      h('p', { class: 'card-copy' }, () => {
        const keys = jwks()?.keys;
        return keys ? keys.length + ' public key' + (keys.length !== 1 ? 's' : '') + ' available' : 'Loading...';
      }),
      createShow(
        () => jwks() != null,
        () => {
          const keys = jwks()?.keys || [];
          return h('div', { class: 'stack-sm' },
            ...keys.map((key: any) =>
              h('div', { class: 'console-card' },
                h('div', { class: 'detail-grid' },
                  DetailRow('Key ID (kid)', key.kid || '—'),
                  DetailRow('Algorithm', key.alg || key.kty || '—'),
                  DetailRow('Key Type', key.kty || '—'),
                  DetailRow('Use', key.use || '—'),
                  DetailRow('Curve', key.crv || '—'),
                ),
              ),
            ),
            h('details', null,
              h('summary', { class: 'btn btn-sm btn-ghost' }, 'Show raw JWKS'),
              h('pre', { class: 'sandbox-code' }, () => JSON.stringify(jwks(), null, 2)),
            ),
          );
        },
      ),
    ),

    // ── Management API Key ────────────────────────────────────────────
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Management API Key'),
      createShow(
        () => !!tenantState()?.management_api_key,
        () => ManagementKeySection(tenantState()!.management_api_key!),
      ),
      createShow(
        () => !tenantState()?.management_api_key,
        () => h('p', { class: 'card-copy' },
          'The management API key is only shown at creation time. Rotate it from Settings if you need a fresh credential.',
        ),
      ),
    ),

    // ── Token Configuration ───────────────────────────────────────────
    h('section', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Token Configuration'),
      h('div', { class: 'detail-grid' },
        DetailRow('Algorithm', 'EdDSA'),
        DetailRow('Issuer', 'gatewasm'),
        DetailRow('Audience', 'gatewasm-app'),
        DetailRow('Token Lifetime', '15 minutes (access) / 30 days (refresh)'),
        DetailRow('Signing', 'Ed25519 (asymmetric)'),
      ),
    ),
  );
}

function ManagementKeySection(key: string) {
  const [revealed, setRevealed] = createSignal(false);

  return h('div', { class: 'stack-sm' },
    createShow(
      revealed,
      () => h('code', { class: 'sandbox-code' }, key),
    ),
    h('div', { class: 'card-actions' },
      h('button', {
        class: 'btn btn-sm btn-ghost',
        onClick: () => setRevealed(!revealed()),
      }, () => revealed() ? 'Hide key' : 'Reveal key'),
      createShow(revealed, () => CopyButton({ text: key, label: 'Copy key' })),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
