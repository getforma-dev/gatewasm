// ---------------------------------------------------------------------------
// JwtViewer — Decoded JWT viewer with raw toggle and refresh token section.
// Store-agnostic: takes token string and slug as props.
// ---------------------------------------------------------------------------

import { h, createSignal, createShow } from 'formajs';

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

export function decodeJwt(token: string): { header: string; payload: string; signature: string } | null {
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

interface JwtViewerProps {
  token: string;
  slug: string;
  serverUrl: string;
}

export function JwtViewer({ token, slug, serverUrl }: JwtViewerProps) {
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
            'POST ' + serverUrl + '/t/' + slug + '/auth/token\n'
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
