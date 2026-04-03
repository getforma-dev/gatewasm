import { h, createSignal, createShow } from 'formajs';
import { selectedTenant, toast } from '../store';

export function IntegrateView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const [showKey, setShowKey] = createSignal(false);

  const loginUrl = () => serverUrl + '/t/' + (selectedTenant()?.slug || '') + '/login';

  const bridgeSnippet = () => {
    const currentTenant = selectedTenant()!;
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
        h('p', { class: 'view-subtitle text-muted' }, 'Everything you need to wire auth into /t/' + tenant.slug),
      ),
      h('p', { class: 'view-header-note' },
        'The layout below keeps the setup path linear: load the bridge, direct unauthenticated users to login, and use the management key server-side only.',
      ),
    ),
    h('div', { class: 'integrate-grid' },
      h('div', { class: 'console-card integrate-card integrate-card-wide' },
        h('div', { class: 'step-label' }, 'Step 1'),
        h('h3', { class: 'section-title' }, 'Load the bridge script'),
        h('p', { class: 'card-copy' }, 'Drop this snippet into your app shell so you can observe auth state changes and access the signed-in user payload.' ),
        h('pre', { class: 'code-block' }, bridgeSnippet),
        h('div', { class: 'card-actions' },
          h('button', {
            class: 'btn btn-sm btn-ghost',
            onClick: () => { navigator.clipboard.writeText(bridgeSnippet()); toast.show('Copied', 'success'); },
          }, 'Copy snippet'),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 2'),
        h('h3', { class: 'section-title' }, 'Redirect to your login page'),
        h('p', { class: 'card-copy' }, 'Use this URL whenever a user needs to authenticate before returning to your app.' ),
        h('code', { class: 'code-block code-inline-block' }, loginUrl),
        h('div', { class: 'card-actions' },
          h('button', {
            class: 'btn btn-sm btn-ghost',
            onClick: () => { navigator.clipboard.writeText(loginUrl()); toast.show('Copied', 'success'); },
          }, 'Copy URL'),
        ),
      ),
      h('div', { class: 'console-card integrate-card' },
        h('div', { class: 'step-label' }, 'Step 3'),
        h('h3', { class: 'section-title' }, 'Use the management key on your backend'),
        h('p', { class: 'card-copy' }, 'Keep this key server-side only when you need to manage users, roles, or webhooks via the Management API.' ),
        createShow(
          () => !!selectedTenant()?.management_api_key,
          () => h('div', { class: 'stack-sm' },
            createShow(
              () => showKey(),
              () => h('code', { class: 'code-block code-inline-block break-all' },
                () => selectedTenant()?.management_api_key || '',
              ),
            ),
            h('div', { class: 'card-actions' },
              h('button', { class: 'btn btn-sm btn-ghost', onClick: () => setShowKey(!showKey()) },
                () => showKey() ? 'Hide key' : 'Reveal key',
              ),
              createShow(
                () => showKey(),
                () => h('button', {
                  class: 'btn btn-sm btn-ghost',
                  onClick: () => { navigator.clipboard.writeText(selectedTenant()?.management_api_key || ''); toast.show('Copied', 'success'); },
                }, 'Copy key'),
              ),
            ),
          ),
        ),
        createShow(
          () => !selectedTenant()?.management_api_key,
          () => h('p', { class: 'card-copy' },
            'The key is only shown at creation time. Rotate it if you need a fresh credential.',
          ),
        ),
      ),
    ),
  );
}
