// ---------------------------------------------------------------------------
// SandboxPage.ts — Sandbox introspection page. Uses shared components.
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { data, error, rawToken, tenantName, slug, serverUrl, logout } from './store';
import { CallbackPreview } from '../../core/components/CallbackPreview';
import { UserProfile, SessionInfo } from '../../core/components/AuthDataCards';
import { JwtViewer } from '../../core/components/JwtViewer';

export function SandboxPage() {
  const consoleUrl = '/platform/console';

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

  const mainView = createShow(
    () => data() != null && error() == null,
    () => {
      const d = data()!;
      const token = rawToken()!;
      return h('div', null,
        h('div', { class: 'sandbox-columns' },
          h('div', { class: 'flex-col gap-lg' },
            CallbackPreview({ data: d, slug, serverUrl }),
            UserProfile(d),
            SessionInfo(d),
          ),
          h('div', { class: 'flex-col gap-lg' },
            JwtViewer({ token, slug, serverUrl }),
          ),
        ),
      );
    },
  );

  return h('div', { class: 'sandbox-page' },
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
