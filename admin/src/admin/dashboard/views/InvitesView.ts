// ---------------------------------------------------------------------------
// InvitesView.ts — Invites tab: create invite form + invites table
// ---------------------------------------------------------------------------

import { Pill } from '../../../core/components/Pill';
import { Badge } from '../../../core/components/Badge';
import { DataTable } from '../../../core/components/DataTable';
import { Panel, PanelTitle } from '../../../core/components/Panel';
import { fmtDate } from '../../../core/utils';
import type { OwnerStore } from '../store';
import type { Invite } from '../../../core/types';

import { h, createSignal } from 'formajs';

// -- Component --------------------------------------------------------------

export function InvitesView(os: OwnerStore) {
  const { store, createInvite, revokeInvite } = os;

  const [email, setEmail] = createSignal('');

  function handleSubmit(e: Event) {
    e.preventDefault();
    const addr = email();
    if (!addr) return;
    createInvite(addr).then(() => setEmail(''));
  }

  // Create invite form
  const createForm = Panel({},
    PanelTitle(
      h('span', null, 'Create Owner Invite'),
      Badge('Invite-only'),
    ),
    h('form', { class: 'inline', onSubmit: handleSubmit },
      h('input', {
        name: 'email',
        type: 'email',
        required: true,
        placeholder: 'cto@company.com',
        value: email,
        onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
      }),
      h('button', { class: 'btn primary', type: 'submit' }, 'Create Invite'),
    ),
    h('p', { class: 'text-muted mt-sm' },
      'Invited operators must sign in via SSO and complete MFA before owner APIs are available.',
    ),
  );

  // Access policy
  const accessPolicy = Panel({},
    PanelTitle(h('span', null, 'Owner Access Policy')),
    h('ul', { class: 'text-muted policy-list' },
      h('li', null, 'Owner UI auth: SSO + MFA + platform_owner role.'),
      h('li', null, 'Tenant management keys (', h('code', null, 'mgmt_*'), ') are backend-only.'),
      h('li', null, 'Public docs stay customer-focused at ', h('code', null, '/docs'), '.'),
      h('li', null, 'Internal API docs remain protected at ', h('code', null, '/internal/docs'), '.'),
    ),
  );

  // Invites table
  const table = DataTable<Invite>({
    headers: ['Email', 'Status', 'Expires', 'Accepted', 'Actions'],
    rows: () => store.invites,
    keyFn: (inv) => inv.id,
    renderRow: (inv) => h('tr', null,
      h('td', null, inv.email),
      h('td', null, Pill(inv.status)),
      h('td', null, fmtDate(inv.expires_at)),
      h('td', null, fmtDate(inv.accepted_at)),
      h('td', { class: 'action-row' },
        inv.status === 'pending'
          ? h('button', {
              class: 'btn small danger',
              onClick: () => revokeInvite(inv.id),
            }, 'Revoke')
          : null,
      ),
    ),
    emptyText: 'No invites created',
  });

  return h('div', { class: 'content' },
    h('div', { class: 'split invites-split' }, createForm, accessPolicy),
    Panel({ class: 'mt-3' },
      PanelTitle(
        h('span', null, 'Invites'),
        h('span', { class: 'text-muted' }, 'Latest 200'),
      ),
      table,
    ),
  );
}
