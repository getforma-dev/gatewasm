import { h, createSignal, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import {
  selectedTenant, securityConfig, securityConfigLoading, tenantPlan,
  saveSecurityConfig, canEdit, type SecurityConfig,
} from '../store';

function ToggleRow(label: string, value: () => boolean, onChange: () => void, locked?: boolean) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    locked
      ? h('span', { class: 'text-muted' }, 'Requires higher plan')
      : h('button', {
          class: () => 'btn btn-sm ' + (value() ? 'btn-primary' : 'btn-ghost'),
          disabled: () => !canEdit(),
          onClick: onChange,
        }, () => value() ? 'Enabled' : 'Disabled'),
  );
}

function NumberRow(label: string, value: () => number, onSave: (v: number) => void, locked?: boolean) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    locked
      ? h('span', { class: 'text-muted' }, 'Requires higher plan')
      : h('input', {
          type: 'number',
          class: 'search-input',
          style: 'width:6rem',
          value: value,
          disabled: () => !canEdit(),
          onBlur: (e: Event) => {
            const parsed = parseInt((e.target as HTMLInputElement).value, 10);
            if (!isNaN(parsed) && parsed !== value()) {
              onSave(parsed);
            }
          },
        }),
  );
}

function SectionCard(label: string, planTag: string | null, children: any[]) {
  return h('div', { class: 'console-card' },
    h('div', { class: 'card-label' },
      label,
      planTag ? h('span', { class: 'text-muted', style: 'margin-left:0.5rem;font-weight:400' }, planTag) : null,
    ),
    h('div', { class: 'detail-grid' }, ...children),
  );
}

export function SecurityConfigView() {
  const tenantId = () => selectedTenant()?.id || '';
  const config = () => securityConfig();
  const plan = () => tenantPlan();

  const isPro = () => {
    const p = plan();
    return p === 'pro' || p === 'enterprise' || p === 'compliance';
  };
  const isEnterprise = () => {
    const p = plan();
    return p === 'enterprise' || p === 'compliance';
  };
  const isCompliance = () => plan() === 'compliance';

  const [showAdvanced, setShowAdvanced] = createSignal(false);

  function patch(field: keyof SecurityConfig, value: SecurityConfig[keyof SecurityConfig]) {
    void saveSecurityConfig(tenantId(), { [field]: value } as Partial<SecurityConfig>);
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Operator'),
        h('h2', { class: 'view-title' }, 'Security'),
        h('p', { class: 'view-subtitle text-muted' },
          () => 'Plan: ' + plan(),
        ),
      ),
    ),

    () => securityConfigLoading() && !config()
      ? Skeleton({ height: '240px' })
      : config()
        ? h('div', { class: 'console-view-body' },

            // ── Foundation ─────────────────────────────────────────
            SectionCard('Foundation', null, [
              NumberRow(
                'Idle Timeout (minutes)',
                () => config()!.idle_timeout_minutes,
                (v) => patch('idle_timeout_minutes', v),
              ),
              NumberRow(
                'Max Sessions per End User',
                () => config()!.max_sessions_per_user,
                (v) => patch('max_sessions_per_user', v),
              ),
              ToggleRow(
                'Host-Prefix Cookies',
                () => config()!.host_prefix_cookies,
                () => patch('host_prefix_cookies', !config()!.host_prefix_cookies),
              ),
              ToggleRow(
                'Content Security Policy',
                () => config()!.csp_enabled,
                () => patch('csp_enabled', !config()!.csp_enabled),
              ),
              ToggleRow(
                'Client Fingerprint',
                () => config()!.client_fingerprint,
                () => patch('client_fingerprint', !config()!.client_fingerprint),
              ),
            ]),

            // ── Advanced tiers toggle ──────────────────────────────
            h('button', {
              type: 'button',
              class: 'btn btn-ghost btn-sm',
              onClick: () => setShowAdvanced(!showAdvanced()),
              style: 'margin: var(--space-sm) 0',
            }, () => showAdvanced() ? 'Hide advanced tiers' : 'Show Pro \u00b7 Enterprise \u00b7 Compliance'),

            // ── Pro / Enterprise / Compliance (collapsed by default) ─
            createShow(showAdvanced, () => h('div', { class: 'stack-sm' },

              // ── Pro ────────────────────────────────────────────────
              SectionCard('Pro', !isPro() ? 'Requires Pro plan' : null, [
                ToggleRow(
                  'WebAuthn / Passkeys',
                  () => config()!.webauthn_enabled,
                  () => patch('webauthn_enabled', !config()!.webauthn_enabled),
                  !isPro(),
                ),
                ToggleRow(
                  'FedCM',
                  () => config()!.fedcm_enabled,
                  () => patch('fedcm_enabled', !config()!.fedcm_enabled),
                  !isPro(),
                ),
              ]),

              // ── Enterprise ─────────────────────────────────────────
              SectionCard('Enterprise', !isEnterprise() ? 'Requires Enterprise plan' : null, [
                ToggleRow(
                  'DPoP Required',
                  () => config()!.dpop_required,
                  () => patch('dpop_required', !config()!.dpop_required),
                  !isEnterprise(),
                ),
                ToggleRow(
                  'PAR Required',
                  () => config()!.par_required,
                  () => patch('par_required', !config()!.par_required),
                  !isEnterprise(),
                ),
                ToggleRow(
                  'mTLS',
                  () => config()!.mtls_enabled,
                  () => patch('mtls_enabled', !config()!.mtls_enabled),
                  !isEnterprise(),
                ),
                ToggleRow(
                  'Token Introspection',
                  () => config()!.introspection_enabled,
                  () => patch('introspection_enabled', !config()!.introspection_enabled),
                  !isEnterprise(),
                ),
              ]),

              // ── Compliance ─────────────────────────────────────────
              SectionCard('Compliance', !isCompliance() ? 'Requires Compliance plan' : null, [
                ToggleRow(
                  'CAEP (Continuous Access Evaluation)',
                  () => config()!.caep_enabled,
                  () => patch('caep_enabled', !config()!.caep_enabled),
                  !isCompliance(),
                ),
                ToggleRow(
                  'Impossible Travel Detection',
                  () => config()!.impossible_travel_enabled,
                  () => patch('impossible_travel_enabled', !config()!.impossible_travel_enabled),
                  !isCompliance(),
                ),
                NumberRow(
                  'Travel Speed Threshold (km/h)',
                  () => config()!.impossible_travel_speed_threshold_kmh,
                  (v) => patch('impossible_travel_speed_threshold_kmh', v),
                  !isCompliance(),
                ),
              ]),
            )),
          )
        : h('div', { class: 'console-view-body' },
            SectionCard('Foundation (Defaults)', null, [
              NumberRow(
                'Idle Timeout (minutes)',
                () => 30,
                () => {},
              ),
              NumberRow(
                'Max Sessions per End User',
                () => 10,
                () => {},
              ),
              ToggleRow(
                'Host-Prefix Cookies',
                () => true,
                () => {},
              ),
              ToggleRow(
                'Content Security Policy',
                () => true,
                () => {},
              ),
              ToggleRow(
                'Client Fingerprint',
                () => true,
                () => {},
              ),
            ]),
          ),
  );
}
