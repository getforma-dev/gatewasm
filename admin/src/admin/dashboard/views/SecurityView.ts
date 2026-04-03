// ---------------------------------------------------------------------------
// SecurityView.ts — Security / operational guardrails tab (playbook)
// ---------------------------------------------------------------------------

import { Panel, PanelTitle } from '../../../core/components/ui/Panel';

import { h } from 'formajs';

// -- Component --------------------------------------------------------------

export function SecurityView() {
  const guardrails = Panel({},
    PanelTitle(h('span', null, 'Operational Guardrails')),
    h('div', { class: 'table-wrap' },
      h('table', null,
        h('tbody', null,
          h('tr', null,
            h('td', null, h('strong', null, 'Provider plane (this dashboard)')),
            h('td', null, 'JWT owner session with SSO + MFA required.'),
          ),
          h('tr', null,
            h('td', null, h('strong', null, 'Customer admin plane')),
            h('td', null, h('code', null, '/t/{slug}/admin'), ' using tenant admin JWT/cookie auth.'),
          ),
          h('tr', null,
            h('td', null, h('strong', null, 'End-user plane')),
            h('td', null, h('code', null, '/t/{slug}/login'), ' for customer app end-users.'),
          ),
          h('tr', null,
            h('td', null, h('strong', null, 'Automation API')),
            h('td', null,
              h('code', null, '/manage/*'),
              ' requires ',
              h('code', null, 'mgmt_*'),
              ' key from server-side secrets only.',
            ),
          ),
        ),
      ),
    ),
  );

  const playbook = Panel({ class: 'mt-3' },
    PanelTitle(h('span', null, 'Owner Playbook')),
    h('ol', { class: 'text-muted playbook-list' },
      h('li', null, 'Review at-risk customers in ', h('strong', null, 'Customers'), '.'),
      h('li', null, 'Suspend/reject bad actors and reactivate approved customers.'),
      h('li', null, 'Extend trials where needed, then validate tenant status transitions.'),
      h('li', null, 'Log billing events and receipts in ', h('strong', null, 'Billing'), '.'),
      h('li', null, 'Invite operators in ', h('strong', null, 'Invites'), ' as your team grows.'),
    ),
  );

  return h('div', { class: 'content' }, guardrails, playbook);
}
