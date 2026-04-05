import { h, createSignal, createShow } from 'formajs';
import {
  selectedTenant,
  introspectData,
  introspectToken,
  introspectUnavailable,
} from '../store';
import { UserProfile, SessionInfo } from '../../../core/components/domain/AuthDataCards';
import { JwtViewer } from '../../../core/components/domain/JwtViewer';
import { CopyButton } from '../../../core/components/ui/CopyButton';

export function SessionInspectorPanel() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const tenantState = () => selectedTenant() || tenant;
  const slug = () => tenantState().slug;
  const loginUrl = () => serverUrl + '/t/' + slug() + '/login?redirect=/t/' + slug() + '/site';

  return h('div', { class: 'stack-sm' },
    // ── Header ──────────────────────────────────────────────────────
    h('div', { class: 'dashboard-shell inspector-shell' },
      h('div', { class: 'dashboard-main' },
        h('div', { class: 'console-card integrate-pane-intro' },
          h('div', { class: 'card-label' }, 'Auth Reference'),
          h('h3', { class: 'section-title' }, 'Understand the auth payload your app receives'),
          h('p', { class: 'card-copy' },
            'This reference shows the shape of the data your application receives when a tenant end user authenticates. Use it to plan your integration before anyone signs in.',
          ),
        ),
      ),
      h('div', { class: 'dashboard-aside' },
        h('div', { class: 'console-card inspector-side-note' },
          h('div', { class: 'card-label' }, 'Always Available'),
          h('p', { class: 'card-copy' },
            'No active sessions required. See the auth contract, token format, and endpoints for this tenant at a glance.',
          ),
        ),
      ),
    ),

    // ── Static reference content (always visible) ───────────────────
    h('div', { class: 'integration-ref-content' },
      CallbackReference({ slug, serverUrl }),
      UserObjectReference(),
      TokenFormatReference(),
      AuthEndpointsReference({ slug, serverUrl }),
    ),

    // ── Live preview (if a session exists in this browser) ──────────
    createShow(
      () => introspectData() != null && introspectToken() != null,
      () => {
        const data = introspectData()!;
        const token = introspectToken()!;
        return h('div', { class: 'auth-ref-live-section' },
          h('div', { class: 'auth-ref-live-banner' },
            h('span', { class: 'auth-ref-live-dot' }),
            h('span', null, 'Live session detected \u2014 showing real data from ', h('strong', null, data.user.email)),
          ),
          h('div', { class: 'integration-ref-content' },
            UserProfile(data),
            SessionInfo(data),
            JwtViewer({ token, slug: slug(), serverUrl }),
          ),
        );
      },
    ),
    createShow(
      () => introspectUnavailable(),
      () => h('div', { class: 'console-card auth-ref-hint' },
        h('div', { class: 'card-label' }, 'Live Preview'),
        h('p', { class: 'card-copy' },
          'Sign into the tenant from any browser tab to see a live preview with real session data here. This is optional \u2014 the reference above is always available.',
        ),
        h('div', { class: 'card-actions auth-ref-hint-actions' },
          h('a', {
            href: loginUrl,
            target: '_blank',
            class: 'btn btn-sm btn-ghost',
          }, () => 'Start tenant sign-in for ' + (tenantState().name || slug())),
          CopyButton({ text: loginUrl, label: 'Copy sign-in URL' }),
        ),
      ),
    ),
  );
}

// ── Callback Payload Reference ────────────────────────────────────────

function CallbackReference({ slug, serverUrl }: { slug: () => string; serverUrl: string }) {
  const [lang, setLang] = createSignal<'js' | 'ts' | 'curl'>('js');

  const jsCode =
`bridge.onStateChange(({ state, user }) => {
  if (state === 'authenticated') {
    console.log('Hello,', user.name);
    // user.id    = "usr_<uuid>"
    // user.email = "jane@example.com"
    // user.name  = "Jane Doe"
  }
});`;

  const tsCode =
`interface AuthState {
  state: 'authenticated' | 'unauthenticated';
  user: { id: string; email: string; name: string | null };
}

bridge.onStateChange(({ state, user }: AuthState) => {
  if (state === 'authenticated') {
    console.log('Hello,', user.name);
    // user.id    = "usr_<uuid>"
    // user.email = "jane@example.com"
    // user.name  = "Jane Doe"
  }
});`;

  const curlCode = () =>
`curl ${serverUrl}/t/${slug()}/auth/me \\
  -H "Authorization: Bearer <access_token>"

# Response:
# {
#   "id": "usr_<uuid>",
#   "email": "jane@example.com",
#   "name": "Jane Doe"
# }`;

  function codeForLang(): string {
    const l = lang();
    if (l === 'ts') return tsCode;
    if (l === 'curl') return curlCode();
    return jsCode;
  }

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-head' },
      h('div', { class: 'sandbox-section-title' }, 'Callback Payload'),
      CopyButton({ text: codeForLang, label: 'Copy snippet' }),
    ),
    h('div', { class: 'sandbox-tabs-row' },
      h('div', { class: 'sandbox-tabs' },
        h('button', {
          class: () => 'sandbox-tab' + (lang() === 'js' ? ' active' : ''),
          onClick: () => setLang('js'),
        }, 'JavaScript'),
        h('button', {
          class: () => 'sandbox-tab' + (lang() === 'ts' ? ' active' : ''),
          onClick: () => setLang('ts'),
        }, 'TypeScript'),
        h('button', {
          class: () => 'sandbox-tab' + (lang() === 'curl' ? ' active' : ''),
          onClick: () => setLang('curl'),
        }, 'curl'),
      ),
    ),
    h('pre', { class: 'sandbox-code' }, () => codeForLang()),
  );
}

// ── User Object Shape ─────────────────────────────────────────────────

function UserObjectReference() {
  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'User Object Shape'),
    h('div', null,
      ObjectRow('id', 'string', 'usr_<uuid>'),
      ObjectRow('email', 'string', 'jane@example.com'),
      ObjectRow('name', 'string | null', 'Jane Doe'),
      ObjectRow('roles', 'string[]', '["role_admin"]'),
      ObjectRow('created_at', 'string (ISO 8601)', '2026-01-15T...'),
    ),
  );
}

function ObjectRow(field: string, type: string, example: string) {
  return h('div', { class: 'sandbox-detail-row' },
    h('code', { class: 'sandbox-detail-label auth-ref-field' }, field),
    h('span', { class: 'auth-ref-type' }, type),
    h('code', { class: 'sandbox-detail-value' }, example),
  );
}

// ── Token Format ──────────────────────────────────────────────────────

function TokenFormatReference() {
  const header = JSON.stringify({ typ: 'JWT', alg: 'EdDSA', kid: '<key-id>' }, null, 2);
  const payload = JSON.stringify({
    sub: 'usr_<uuid>',
    iss: 'gatewasm',
    aud: 'gatewasm-app',
    iat: '<issued-at>',
    exp: '<expires-at>',
    tenant_id: '<tenant-uuid>',
  }, null, 2);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Token Format'),
    h('div', null,
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Algorithm'),
        h('code', { class: 'sandbox-detail-value' }, 'EdDSA'),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Issuer'),
        h('code', { class: 'sandbox-detail-value' }, 'gatewasm'),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Audience'),
        h('code', { class: 'sandbox-detail-value' }, 'gatewasm-app'),
      ),
    ),
    h('div', { class: 'auth-ref-jwt-preview' },
      h('div', { class: 'sandbox-jwt-block' },
        h('div', { class: 'sandbox-jwt-label sandbox-jwt-header' }, 'Header'),
        h('pre', { class: 'sandbox-code sandbox-jwt-header' }, header),
      ),
      h('div', { class: 'sandbox-jwt-block' },
        h('div', { class: 'sandbox-jwt-label sandbox-jwt-payload' }, 'Payload'),
        h('pre', { class: 'sandbox-code sandbox-jwt-payload' }, payload),
      ),
    ),
  );
}

// ── Auth Endpoints ────────────────────────────────────────────────────

function AuthEndpointsReference({ slug, serverUrl }: { slug: () => string; serverUrl: string }) {
  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Auth Endpoints'),
    h('div', null,
      EndpointRefRow('Login', () => serverUrl + '/t/' + slug() + '/login'),
      EndpointRefRow('Token Refresh', () => 'POST ' + serverUrl + '/t/' + slug() + '/auth/token'),
      EndpointRefRow('Introspect', () => 'GET ' + serverUrl + '/t/' + slug() + '/auth/introspect'),
      EndpointRefRow('User Profile', () => 'GET ' + serverUrl + '/t/' + slug() + '/auth/me'),
    ),
  );
}

function EndpointRefRow(label: string, value: () => string) {
  return h('div', { class: 'sandbox-detail-row' },
    h('span', { class: 'sandbox-detail-label' }, label),
    h('code', { class: 'sandbox-detail-value auth-ref-endpoint' }, value),
    CopyButton({ text: value, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
  );
}
