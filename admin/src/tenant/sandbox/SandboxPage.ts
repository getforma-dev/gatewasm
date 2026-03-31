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

const SANDBOX_STYLES = `
.sandbox-page {
  max-width: 1200px;
  margin: 0 auto;
  padding: 2rem 1.5rem;
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  min-height: 100vh;
}

.sandbox-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.65rem 1rem;
  border: 1px solid var(--border-accent);
  background: var(--surface);
  animation: fade-up 0.4s var(--ease) both;
}

.sandbox-banner-text {
  font-size: 0.75rem;
  color: var(--text-dim);
  font-weight: 300;
}

.sandbox-banner-text strong {
  color: var(--text);
  font-weight: 400;
}

.sandbox-banner a {
  font-size: 0.72rem;
  color: var(--accent);
  text-decoration: none;
}

.sandbox-banner a:hover {
  text-decoration: underline;
}

.sandbox-columns {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1.5rem;
  animation: fade-up 0.5s var(--ease) 0.1s both;
}

.sandbox-section {
  border: 1px solid var(--border);
  background: var(--surface);
  padding: 1rem 1.1rem;
}

.sandbox-section-title {
  font-size: 0.6rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: var(--text-muted);
  margin-bottom: 0.75rem;
  font-weight: 400;
}

.sandbox-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  margin-bottom: 0.75rem;
}

.sandbox-tab {
  padding: 0.4rem 0.75rem;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-dim);
  font-size: 0.72rem;
  font-family: inherit;
  cursor: pointer;
  transition: all var(--duration) ease;
}

.sandbox-tab:hover {
  color: var(--text);
  border-color: transparent;
}

.sandbox-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

.sandbox-code {
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 0.75rem 0.85rem;
  font-family: 'DM Mono', monospace;
  font-size: 0.72rem;
  font-weight: 300;
  color: var(--text-dim);
  line-height: 1.65;
  overflow-x: auto;
  white-space: pre;
}

.sandbox-detail-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  padding: 0.4rem 0;
  border-bottom: 1px solid var(--border);
  gap: 0.75rem;
}

.sandbox-detail-row:last-child {
  border-bottom: none;
}

.sandbox-detail-label {
  font-size: 0.6rem;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: var(--text-muted);
  flex-shrink: 0;
}

.sandbox-detail-value {
  font-size: 0.78rem;
  color: var(--text-dim);
  font-weight: 300;
  text-align: right;
  word-break: break-all;
}

.sandbox-jwt-header {
  color: #60a5fa;
}

.sandbox-jwt-payload {
  color: #c4a265;
}

.sandbox-jwt-signature {
  color: #4ade80;
}

.sandbox-jwt-label {
  font-size: 0.55rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  margin-bottom: 0.25rem;
  font-weight: 400;
}

.sandbox-jwt-block {
  margin-bottom: 0.75rem;
}

.sandbox-jwt-block:last-child {
  margin-bottom: 0;
}

.sandbox-jwt-raw {
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 0.65rem 0.75rem;
  font-family: 'DM Mono', monospace;
  font-size: 0.68rem;
  font-weight: 300;
  color: var(--text-dim);
  word-break: break-all;
  line-height: 1.55;
  white-space: pre-wrap;
}

.sandbox-toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.sandbox-toggle-btn {
  font-size: 0.68rem;
  padding: 0.25rem 0.55rem;
  border: 1px solid var(--border-accent);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  font-family: inherit;
}

.sandbox-toggle-btn:hover {
  color: var(--text);
  border-color: var(--border-strong);
}

.sandbox-roles {
  display: flex;
  gap: 0.35rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.sandbox-role-badge {
  font-size: 0.625rem;
  padding: 0.15rem 0.45rem;
  border: 1px solid rgba(96, 165, 250, 0.25);
  background: rgba(96, 165, 250, 0.06);
  color: var(--info);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.sandbox-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border: 1px solid var(--border);
  background: var(--surface);
  animation: fade-up 0.5s var(--ease) 0.2s both;
}

.sandbox-footer-text {
  font-size: 0.72rem;
  color: var(--text-muted);
  font-weight: 300;
}

.sandbox-expiry-live {
  color: var(--success);
  font-family: 'DM Mono', monospace;
  font-size: 0.78rem;
}

.sandbox-refresh-section {
  border-top: 1px solid var(--border);
  margin-top: 0.75rem;
  padding-top: 0.75rem;
}

.sandbox-refresh-toggle {
  font-size: 0.68rem;
  color: var(--text-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  padding: 0.2rem 0;
  font-family: inherit;
  letter-spacing: 0.04em;
}

.sandbox-refresh-toggle:hover {
  color: var(--text-dim);
  border: none;
}

.sandbox-error {
  padding: 2rem;
  text-align: center;
  color: var(--danger);
  font-size: 0.82rem;
}

@media (max-width: 768px) {
  .sandbox-page {
    padding: 1.25rem 1rem;
  }
  .sandbox-columns {
    grid-template-columns: 1fr;
  }
}
`;

let stylesInjected = false;
function injectStyles(): void {
  if (stylesInjected) return;
  stylesInjected = true;
  const el = document.createElement('style');
  el.textContent = SANDBOX_STYLES;
  document.head.appendChild(el);
}

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
  injectStyles();

  const consoleUrl = '/platform/console-v2';

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
