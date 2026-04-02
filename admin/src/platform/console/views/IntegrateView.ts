import { h, createSignal, createShow } from 'formajs';
import { selectedTenant, updateTenant, toast } from '../store';

export function IntegrateView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const [showKey, setShowKey] = createSignal(false);

  const bridgeSnippet = () => {
    const t = selectedTenant()!;
    return `<script src="${serverUrl}/bridge.js"><\/script>
<script>
  GateAuthBridge.init({
    serverUrl: '${serverUrl}',
    slug: '${t.slug}',
    onStateChange: ({ state, user }) => {
      if (state === 'authenticated') {
        console.log('Hello,', user.name);
      }
    }
  });
<\/script>`;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Integrate'),
      h('p', { class: 'view-subtitle text-muted' }, 'Add auth to your app in three steps'),
    ),

    // Step 1: Script tag
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 1 — Add the script tag'),
      h('pre', { class: 'code-block' }, bridgeSnippet),
      h('button', {
        class: 'btn btn-sm btn-ghost',
        style: { marginTop: '0.5rem' },
        onClick: () => { navigator.clipboard.writeText(bridgeSnippet()); toast.show('Copied', 'success'); },
      }, 'Copy'),
    ),

    // Step 2: Login URL
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 2 — Your login page URL'),
      h('code', { class: 'code-block', style: { display: 'block' } },
        () => serverUrl + '/t/' + (selectedTenant()?.slug || '') + '/login',
      ),
      h('p', { class: 'text-muted', style: { fontSize: '0.75rem', marginTop: '0.5rem' } },
        'Redirect unauthenticated users here. After login, they return to your app with a session cookie.',
      ),
    ),

    // Step 3: API Key
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 3 — Management API key (backend only)'),
      createShow(
        () => !!selectedTenant()?.management_api_key,
        () => h('div', null,
          createShow(
            () => showKey(),
            () => h('code', { class: 'code-block', style: { display: 'block', wordBreak: 'break-all' } },
              () => selectedTenant()?.management_api_key || '',
            ),
          ),
          h('div', { style: { display: 'flex', gap: '0.5rem', marginTop: '0.5rem' } },
            h('button', { class: 'btn btn-sm btn-ghost', onClick: () => setShowKey(!showKey()) },
              () => showKey() ? 'Hide' : 'Reveal',
            ),
            createShow(
              () => showKey(),
              () => h('button', {
                class: 'btn btn-sm btn-ghost',
                onClick: () => { navigator.clipboard.writeText(selectedTenant()?.management_api_key || ''); toast.show('Copied', 'success'); },
              }, 'Copy'),
            ),
          ),
        ),
      ),
      createShow(
        () => !selectedTenant()?.management_api_key,
        () => h('p', { class: 'text-muted', style: { fontSize: '0.75rem' } },
          'API key was shown once at tenant creation. Use the rotate button if you need a new one.',
        ),
      ),
      h('p', { class: 'text-muted', style: { fontSize: '0.72rem', marginTop: '0.5rem' } },
        'Use this key from your server to manage users, roles, and webhooks via the Management API.',
      ),
    ),
  );
}
