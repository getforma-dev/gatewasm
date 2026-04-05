import { h, createShow } from 'formajs';
import {
  selectedTenant,
  introspectData,
  introspectToken,
  introspectUnavailable,
} from '../store';
import { CallbackPreview } from '../../../core/components/domain/CallbackPreview';
import { UserProfile, SessionInfo } from '../../../core/components/domain/AuthDataCards';
import { JwtViewer } from '../../../core/components/domain/JwtViewer';
import { CopyButton } from '../../../core/components/ui/CopyButton';

export function SessionInspectorPanel() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const tenantState = () => selectedTenant() || tenant;
  const loginUrl = () => serverUrl + '/t/' + tenantState().slug + '/login?redirect=/t/' + tenantState().slug + '/site';

  const authPrompt = () => h('div', { class: 'inspector-empty-layout' },
    h('div', { class: 'console-card auth-prompt inspector-empty-main' },
      h('div', { class: 'card-label' }, 'Live Tenant Session Required'),
      h('h3', { class: 'section-title' }, 'Sign into the tenant once to unlock inspection'),
      h('p', { class: 'auth-prompt-text' },
        'Session Inspector only appears after a real tenant sign-in in this browser. Platform login stays separate from tenant-user login so the data here reflects an actual customer session.',
      ),
      h('a', {
        href: loginUrl,
        target: '_blank',
        class: 'btn btn-primary',
      }, () => 'Start tenant sign-in for ' + (tenantState().name || tenantState().slug)),
      h('div', { class: 'card-actions' },
        CopyButton({ text: loginUrl, label: 'Copy sign-in URL' }),
      ),
    ),
    h('div', { class: 'console-card inspector-side-note' },
      h('div', { class: 'card-label' }, 'What You Will See'),
      h('div', { class: 'info-list' },
        InfoBullet('The callback payload your app receives after login'),
        InfoBullet('The current user profile, roles, and session timing'),
        InfoBullet('The live access token so you can inspect claims quickly'),
      ),
    ),
  );

  const loadingState = () => h('div', { class: 'console-card' },
    h('div', { class: 'card-label' }, 'Checking Session'),
    h('div', { class: 'card-value card-value-strong' }, 'Looking for a tenant session in this browser'),
    h('p', { class: 'card-copy' },
      'If you have already signed into the tenant app in another tab, this view will populate as soon as the browser sends the tenant refresh cookie.',
    ),
  );

  return h('div', { class: 'stack-sm' },
    h('div', { class: 'dashboard-shell inspector-shell' },
      h('div', { class: 'dashboard-main' },
        h('div', { class: 'console-card integrate-pane-intro' },
          h('div', { class: 'card-label' }, 'Session Inspector'),
          h('h3', { class: 'section-title' }, 'Inspect the live auth payload'),
          h('p', { class: 'card-copy' },
            'This is the owner-side developer inspector for a real tenant session. Use it when you need to verify what your app actually receives after authentication.',
          ),
        ),
      ),
      h('div', { class: 'dashboard-aside' },
        h('div', { class: 'console-card inspector-side-note' },
          h('div', { class: 'card-label' }, 'Live Session Data'),
          h('p', { class: 'card-copy' },
            'Callback preview, current user profile, refresh-session details, and the active access token are all shown together here.',
          ),
        ),
      ),
    ),
    createShow(
      () => introspectData() != null && introspectToken() != null,
      () => {
        const data = introspectData()!;
        const token = introspectToken()!;
        return h('div', { class: 'integration-ref-content' },
          CallbackPreview({ data, slug: tenantState().slug, serverUrl }),
          UserProfile(data),
          SessionInfo(data),
          JwtViewer({ token, slug: tenantState().slug, serverUrl }),
        );
      },
    ),
    createShow(
      () => introspectUnavailable(),
      authPrompt,
    ),
    createShow(
      () => introspectData() == null && introspectToken() == null && !introspectUnavailable(),
      loadingState,
    ),
  );
}

function InfoBullet(copy: string) {
  return h('div', { class: 'info-bullet' }, copy);
}
