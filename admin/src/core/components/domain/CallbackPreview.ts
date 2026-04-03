// ---------------------------------------------------------------------------
// CallbackPreview — Code snippet tabs (JS/TS/curl) showing what the app
// receives when a user authenticates. Store-agnostic: takes props.
// ---------------------------------------------------------------------------

import { h, createSignal } from 'formajs';
import type { IntrospectData } from '../../types';

interface CallbackPreviewProps {
  data: IntrospectData;
  slug: string;
  serverUrl: string;
}

export function CallbackPreview({ data: d, slug, serverUrl }: CallbackPreviewProps) {
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
