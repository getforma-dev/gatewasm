// ---------------------------------------------------------------------------
// Quickstart.ts — Integration reference guide (dismissible banner)
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { CodeBlock } from '../../../core/components/CodeBlock';
import { Card, CardHead } from '../../../core/components/Card';
import type { ConsoleStore } from '../store';

export function Quickstart(cs: ConsoleStore) {
  const { store, dismissQuickstart } = cs;

  const banner = Card({},
    CardHead(
      h('h3', { class: 'card-title' }, 'Integration Guide'),
      h('button', { class: 'btn small', onClick: () => dismissQuickstart() }, 'Dismiss'),
    ),

    // Access model
    h('div', { class: 'quickstart-section' },
      h('h4', null, 'Access Model'),
      h('ul', { class: 'quickstart-access-list' },
        h('li', null,
          h('strong', null, 'Tenant admins'),
          ' \u2014 sign in at ',
          h('code', null, '/t/{slug}/admin'),
          ' (JWT/cookie auth)',
        ),
        h('li', null,
          h('strong', null, 'Backend automation'),
          ' \u2014 use ',
          h('code', null, '/manage/*'),
          ' with tenant ',
          h('code', null, 'mgmt_'),
          ' key (server-side only)',
        ),
        h('li', null,
          h('strong', null, 'End-users'),
          ' \u2014 sign in at ',
          h('code', null, '/t/{slug}/login'),
        ),
      ),
    ),

    // Step 1
    h('div', { class: 'quickstart-section' },
      h('h4', null, 'Step 1: Add bridge.js'),
      CodeBlock('<script src="https://cdn.getforma.dev/bridge.js"></script>'),
    ),

    // Step 2
    h('div', { class: 'quickstart-section' },
      h('h4', null, 'Step 2: Initialize and authenticate'),
      CodeBlock(
        "const bridge = await GateAuthBridge.init({\n" +
        "  serverUrl: 'https://auth.getforma.dev',\n" +
        "  clientId: 'your-slug-hosted',\n" +
        "});\n" +
        "\n" +
        "bridge.onStateChange(({ state, user }) => {\n" +
        "  if (state === 'authenticated') {\n" +
        "    console.log('Welcome,', user.name);\n" +
        "  }\n" +
        "});\n" +
        "\n" +
        "bridge.login('google'); // or 'github', 'email'",
      ),
    ),

    // Step 3
    h('div', { class: 'quickstart-section' },
      h('h4', null, 'Step 3: Backend Automation'),
      CodeBlock(
        "# backend env (never in frontend)\n" +
        "GATEWASM_MGMT_API_KEY=mgmt_...\n" +
        "\n" +
        "curl https://auth.getforma.dev/manage/users \\\n" +
        '  -H "Authorization: Bearer $GATEWASM_MGMT_API_KEY"',
      ),
    ),

    // Docs link
    h('div', { class: 'quickstart-section' },
      h('a', { href: '/docs', class: 'btn small' }, 'View full documentation \u2192'),
    ),
  );

  return createShow(
    () => !store.quickstartDismissed && store.tenants.length > 0,
    () => banner,
  );
}
