// ---------------------------------------------------------------------------
// SandboxPage.ts — Sandbox introspection page: JWT viewer, callback preview,
//                  session info, user profile. Two-column layout.
// ---------------------------------------------------------------------------

import { h, createSignal, createShow } from 'formajs';
import { data, error, rawToken, tenantName, slug, serverUrl, logout } from './store';
import type { IntrospectData } from './store';

// ── Helpers ────────────────────────────────────────────────────────────────

function fmtDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

function relativeExpiry(unixSeconds: number): string {
  const diff = unixSeconds * 1000 - Date.now();
  if (diff <= 0) return 'Expired';
  const mins = Math.floor(diff / 60000);
  const secs = Math.floor((diff % 60000) / 1000);
  if (mins > 0) return mins + 'm ' + secs + 's';
  return secs + 's';
}

/** Decode a base64url string to a UTF-8 string. */
function b64decode(str: string): string {
  const padded = str.replace(/-/g, '+').replace(/_/g, '/');
  return decodeURIComponent(
    atob(padded)
      .split('')
      .map((c) => '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2))
      .join(''),
  );
}

function decodeJwt(token: string): { header: string; payload: string; signature: string } | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    return {
      header: JSON.stringify(JSON.parse(b64decode(parts[0])), null, 2),
      payload: JSON.stringify(JSON.parse(b64decode(parts[1])), null, 2),
      signature: parts[2],
    };
  } catch {
    return null;
  }
}

// ── Scoped styles (injected once) ──────────────────────────────────────────

// Styles are in pages.css (sandbox section) — no runtime injection needed.

// ── CallbackPreview ────────────────────────────────────────────────────────

function CallbackPreview(d: IntrospectData) {
  const [lang, setLang] = createSignal<'js' | 'ts' | 'curl'>('js');

  const userName = d.user.name || d.user.email;
  const userId = d.user.id;

  const jsCode =
`bridge.onStateChange(({ state, user }) => {
  if (state === 'authenticated') {
    console.log('Hello,', user.name);
    // user.id    = "${userId}"
    // user.email = "${d.user.email}"
    // user.name  = "${userName}"
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
    // user.id    = "${userId}"
    // user.email = "${d.user.email}"
    // user.name  = "${userName}"
  }
});`;

  const curlCode =
`curl ${serverUrl}/t/${slug}/auth/me \\
  -H "Authorization: Bearer <token>"

# Response:
# {
#   "id": "${userId}",
#   "email": "${d.user.email}",
#   "name": "${userName}"
# }`;

  function codeForLang(): string {
    const l = lang();
    if (l === 'ts') return tsCode;
    if (l === 'curl') return curlCode;
    return jsCode;
  }

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Your app receives this'),
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
    h('pre', { class: 'sandbox-code' }, () => codeForLang()),
  );
}

// ── UserProfile ────────────────────────────────────────────────────────────

function UserProfile(d: IntrospectData) {
  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'User Profile'),
    h('div', null,
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'User ID'),
        h('code', { class: 'sandbox-detail-value' }, d.user.id),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Email'),
        h('span', { class: 'sandbox-detail-value' }, d.user.email),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Name'),
        h('span', { class: 'sandbox-detail-value' }, d.user.name || '(not set)'),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Roles'),
        h('span', { class: 'sandbox-roles' },
          ...(d.user.roles.length > 0
            ? d.user.roles.map((r) => h('span', { class: 'sandbox-role-badge' }, r))
            : [h('span', { class: 'sandbox-role-badge' }, 'user')]),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Created'),
        h('span', { class: 'sandbox-detail-value' }, fmtDate(d.user.created_at)),
      ),
    ),
  );
}

// ── SessionInfo ────────────────────────────────────────────────────────────

function SessionInfo(d: IntrospectData) {
  // Live countdown for token expiry
  const [expiry, setExpiry] = createSignal(relativeExpiry(d.token.expires_at));

  const timer = setInterval(() => {
    setExpiry(relativeExpiry(d.token.expires_at));
  }, 1000);

  // Clean up when the element is removed (best-effort)
  setTimeout(() => {
    const check = setInterval(() => {
      if (!document.querySelector('.sandbox-expiry-live')) {
        clearInterval(timer);
        clearInterval(check);
      }
    }, 5000);
  }, 0);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Session'),
    h('div', null,
      createShow(
        () => d.session != null,
        () => h('div', null,
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Session ID'),
            h('code', { class: 'sandbox-detail-value' }, d.session!.id),
          ),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Created'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.created_at)),
          ),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Expires'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.expires_at)),
          ),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Token Expires In'),
        h('span', { class: 'sandbox-expiry-live' }, expiry),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Algorithm'),
        h('code', { class: 'sandbox-detail-value' }, d.token.algorithm),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Issuer'),
        h('span', { class: 'sandbox-detail-value' }, d.token.issuer),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Audience'),
        h('span', { class: 'sandbox-detail-value' }, d.token.audience),
      ),
    ),
  );
}

// ── JwtViewer ──────────────────────────────────────────────────────────────

function JwtViewer(token: string) {
  const [showRaw, setShowRaw] = createSignal(false);
  const [showRefresh, setShowRefresh] = createSignal(false);
  const decoded = decodeJwt(token);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-toggle-row' },
      h('div', { class: 'sandbox-section-title', style: 'margin-bottom:0' }, 'JWT Access Token'),
      h('button', {
        class: 'sandbox-toggle-btn',
        onClick: () => setShowRaw(!showRaw()),
      }, () => showRaw() ? 'Decoded' : 'Raw'),
    ),
    createShow(
      () => !showRaw() && decoded != null,
      () => h('div', null,
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-header' }, 'Header'),
          h('pre', { class: 'sandbox-code sandbox-jwt-header' }, decoded!.header),
        ),
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-payload' }, 'Payload'),
          h('pre', { class: 'sandbox-code sandbox-jwt-payload' }, decoded!.payload),
        ),
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-signature' }, 'Signature'),
          h('pre', { class: 'sandbox-code sandbox-jwt-signature' }, decoded!.signature),
        ),
      ),
    ),
    createShow(
      () => showRaw(),
      () => h('div', { class: 'sandbox-jwt-raw' }, token),
    ),
    // Refresh token section (collapsed)
    h('div', { class: 'sandbox-refresh-section' },
      h('button', {
        class: 'sandbox-refresh-toggle',
        onClick: () => setShowRefresh(!showRefresh()),
      }, () => (showRefresh() ? '- ' : '+ ') + 'Refresh token'),
      createShow(
        () => showRefresh(),
        () => h('div', { class: 'mt-sm' },
          h('p', { class: 'text-xs', style: 'color: var(--text-muted); margin-bottom: 0.5rem' },
            'Refresh tokens are httpOnly cookies. They are not visible to JavaScript. Your server exchanges them silently via POST /auth/token.',
          ),
          h('pre', { class: 'sandbox-code' },
            'POST /t/' + slug + '/auth/token\n'
            + 'Content-Type: application/json\n'
            + '{ "grant_type": "refresh_token" }\n'
            + '\n'
            + '// Cookie is sent automatically.\n'
            + '// Response: { "access_token": "eyJ..." }',
          ),
        ),
      ),
    ),
  );
}

// ── SandboxPage (root) ─────────────────────────────────────────────────────

export function SandboxPage() {
  const consoleUrl = '/platform/console';

  // Error state
  const errView = createShow(
    () => error() != null,
    () => h('div', { class: 'sandbox-error' },
      h('p', null, 'Failed to load sandbox data.'),
      h('p', { class: 'text-xs mt-sm', style: 'color: var(--text-muted)' }, error),
      h('button', {
        class: 'btn mt-md',
        onClick: () => { window.location.reload(); },
      }, 'Retry'),
    ),
  );

  // Main content
  const mainView = createShow(
    () => data() != null && error() == null,
    () => {
      const d = data()!;
      const token = rawToken()!;
      return h('div', null,
        // Two-column layout
        h('div', { class: 'sandbox-columns' },
          // Left column
          h('div', { class: 'flex-col gap-lg' },
            CallbackPreview(d),
            UserProfile(d),
            SessionInfo(d),
          ),
          // Right column
          h('div', { class: 'flex-col gap-lg' },
            JwtViewer(token),
          ),
        ),
      );
    },
  );

  return h('div', { class: 'sandbox-page' },
    // Top banner
    h('div', { class: 'sandbox-banner' },
      h('span', { class: 'sandbox-banner-text' },
        'Sandbox ',
        h('strong', null, tenantName),
        ' \u2014 this is a developer-only view, separate from your production app.',
      ),
      h('a', { href: consoleUrl }, 'Back to Console \u2192'),
    ),
    errView,
    mainView,
    // Footer
    h('div', { class: 'sandbox-footer' },
      h('span', { class: 'sandbox-footer-text' },
        'Your end users never see this page.',
      ),
      h('button', {
        class: 'btn',
        onClick: logout,
      }, 'Log out and try again'),
    ),
  );
}
