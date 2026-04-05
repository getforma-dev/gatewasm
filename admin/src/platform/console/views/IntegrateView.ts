import { h, createShow, createSignal, createSwitch } from 'formajs';
import {
  selectedTenant,
  integrateSection,
  setIntegrateSection,
  type IntegrateSection,
} from '../store';
import { SessionInspectorPanel } from './SessionInspectorPanel';
import { CopyButton } from '../../../core/components/ui/CopyButton';

export function IntegrateView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const tenantState = () => selectedTenant() || tenant;

  const loginUrl = () => serverUrl + '/t/' + tenantState().slug + '/login';
  const previewLoginUrl = () => loginUrl() + '?preview=1';
  const siteUrl = () => serverUrl + '/t/' + tenantState().slug + '/site';

  const bridgeSnippet = () => {
    const currentTenant = tenantState();
    return `<script src="${serverUrl}/bridge.js"><\/script>
<script>
  GateAuthBridge.init({
    serverUrl: '${serverUrl}',
    slug: '${currentTenant.slug}',
    onStateChange: ({ state, user }) => {
      if (state === 'authenticated') {
        console.log('Hello,', user.name);
      }
    }
  });
<\/script>`;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Developer Setup'),
        h('h2', { class: 'view-title' }, 'Integrate'),
        h('p', { class: 'view-subtitle text-muted' }, () => 'Everything you need to wire auth into /t/' + tenantState().slug),
      ),
      h('p', { class: 'view-header-note' },
        'Keep setup and debugging in one place: integrate the hosted login flow, then inspect the live tenant session without leaving the workspace.',
      ),
    ),
    h('section', { class: 'zone' },
      h('div', { class: 'zone-head' },
        h('div', { class: 'zone-label' }, 'Integration Workspace'),
        h('p', { class: 'zone-copy' },
          'Use the setup tab when you are wiring URLs and credentials. Switch to Session Inspector when you want to validate a real tenant-user session and see the callback payload your app receives.',
        ),
      ),
      h('div', { class: 'console-subtabs', role: 'tablist', 'aria-label': 'Integrate sections' },
        IntegrateTab('guide', 'Getting Started'),
        IntegrateTab('session-inspector', 'Session Inspector'),
      ),
      h('div', { class: 'integrate-pane' },
        createSwitch(
          integrateSection,
          [
            {
              match: 'guide',
              render: () => GuidePanel({
                bridgeSnippet,
                loginUrl,
                previewLoginUrl,
                siteUrl,
                tenantState,
                openInspector: () => setIntegrateSection('session-inspector'),
              }),
            },
            { match: 'session-inspector', render: () => SessionInspectorPanel() },
          ],
        ),
      ),
    ),
  );
}

function IntegrateTab(id: IntegrateSection, label: string) {
  return h('button', {
    type: 'button',
    role: 'tab',
    class: () => 'console-subtab' + (integrateSection() === id ? ' active' : ''),
    'aria-selected': () => String(integrateSection() === id),
    onClick: () => setIntegrateSection(id),
  }, label);
}

function GuidePanel({
  bridgeSnippet,
  loginUrl,
  previewLoginUrl,
  siteUrl,
  tenantState,
  openInspector,
}: {
  bridgeSnippet: () => string;
  loginUrl: () => string;
  previewLoginUrl: () => string;
  siteUrl: () => string;
  tenantState: () => { management_api_key?: string; name: string; slug: string };
  openInspector: () => void;
}) {
  return h('div', { class: 'integrate-guide-layout' },
    h('div', { class: 'integrate-main dashboard-stack' },
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 1'),
        h('h3', { class: 'section-title' }, 'Load the bridge script'),
        h('p', { class: 'card-copy' }, 'Drop this snippet into your app shell so you can observe auth state changes and access the signed-in user payload.'),
        h('pre', { class: 'code-block' }, bridgeSnippet),
        h('div', { class: 'card-actions' },
          CopyButton({ text: bridgeSnippet, label: 'Copy snippet' }),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 2'),
        h('h3', { class: 'section-title' }, 'Redirect to your login page'),
        h('p', { class: 'card-copy' }, 'Use this URL whenever a user needs to authenticate before returning to your app.'),
        h('div', { class: 'inline-copy-row' },
          h('code', { class: 'code-block code-inline-block' }, loginUrl),
          CopyButton({ text: loginUrl, label: 'Copy URL' }),
        ),
        h('div', { class: 'card-actions' },
          h('a', {
            class: 'btn btn-sm btn-ghost',
            href: previewLoginUrl,
            target: '_blank',
          }, 'Preview login page'),
        ),
      ),
    ),
    h('aside', { class: 'integrate-aside dashboard-stack' },
      h('div', { class: 'console-card integrate-summary-card' },
        h('div', { class: 'card-label' }, 'Tenant Endpoints'),
        h('h3', { class: 'section-title' }, 'Everything you need for QA'),
        h('p', { class: 'card-copy' },
          'Keep your most useful tenant links together so you can test the hosted flow without leaving the setup screen.',
        ),
        h('div', { class: 'surface-link-list' },
          EndpointRow('Login URL', loginUrl(), {
            label: 'Open',
            href: loginUrl(),
          }),
          EndpointRow('Login Preview', previewLoginUrl(), {
            label: 'Open',
            href: previewLoginUrl(),
          }),
          EndpointRow('Protected Site', siteUrl(), {
            label: 'Open',
            href: siteUrl(),
          }),
        ),
        h('div', { class: 'card-actions' },
          h('button', {
            type: 'button',
            class: 'btn btn-primary btn-sm',
            onClick: openInspector,
          }, 'Open Session Inspector'),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 3'),
        h('h3', { class: 'section-title' }, 'Use the management key on your backend'),
        h('p', { class: 'card-copy' }, 'Keep this key server-side only when you need to manage users, roles, or webhooks via the Management API.'),
        createShow(
          () => !!tenantState().management_api_key,
          () => ManagementKeyCard(tenantState().management_api_key || ''),
        ),
        createShow(
          () => !tenantState().management_api_key,
          () => h('p', { class: 'card-copy' },
            'The key is only shown at creation time. Rotate it if you need a fresh credential.',
          ),
        ),
      ),
    ),
  );
}

function EndpointRow(
  label: string,
  value: string,
  action: { label: string; href?: string; onClick?: () => void },
) {
  return h('div', { class: 'surface-link-row' },
    h('div', { class: 'surface-link-copy' },
      h('span', { class: 'detail-label' }, label),
      h('code', { class: 'surface-link-value' }, value),
    ),
    h('div', { class: 'surface-link-actions' },
      action.href
        ? h('a', {
            href: action.href,
            target: '_blank',
            class: 'btn btn-sm btn-ghost',
          }, action.label)
        : h('button', {
            type: 'button',
            class: 'btn btn-sm btn-ghost',
            onClick: action.onClick,
          }, action.label),
      CopyButton({ text: value, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
    ),
  );
}

function ManagementKeyCard(key: string) {
  const [reveal, setReveal] = createSignal(false);

  return h('div', { class: 'stack-sm' },
    () => reveal()
      ? h('code', { class: 'code-block code-inline-block break-all' }, key)
      : null,
    h('div', { class: 'card-actions' },
      h('button', {
        class: 'btn btn-sm btn-ghost',
        onClick: () => {
          setReveal(!reveal());
        },
      }, () => reveal() ? 'Hide key' : 'Reveal key'),
      () => reveal()
        ? CopyButton({ text: key, label: 'Copy key' })
        : null,
    ),
  );
}
