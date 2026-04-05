// ---------------------------------------------------------------------------
// AuthDataCards — UserProfile and SessionInfo components.
// Store-agnostic: take IntrospectData as props.
// ---------------------------------------------------------------------------

import { h, createSignal, createShow } from 'formajs';
import type { IntrospectData } from '../../types';
import { CopyButton } from '../ui/CopyButton';

function fmtDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

function relativeExpiry(unixSeconds: number): string {
  const diff = unixSeconds * 1000 - Date.now();
  if (diff <= 0) return 'Expired';
  const mins = Math.floor(diff / 60000);
  const secs = Math.floor((diff % 60000) / 1000);
  if (mins > 0) return mins + 'm ' + secs + 's';
  return secs + 's';
}

export function UserProfile(d: IntrospectData) {
  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'User Profile'),
    h('div', null,
      CopyableSandboxRow('User ID', d.user.id, true),
      CopyableSandboxRow('Email', d.user.email),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Name'),
        h('span', { class: 'sandbox-detail-value' }, d.user.name || '(not set)'),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Roles'),
        h('span', { class: 'sandbox-roles' },
          ...(d.user.roles.length > 0
            ? d.user.roles.map((r) => h('span', { class: 'sandbox-role-badge' }, r))
            : [h('span', { class: 'sandbox-role-badge' }, 'user')]),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Created'),
        h('span', { class: 'sandbox-detail-value' }, fmtDate(d.user.created_at)),
      ),
    ),
  );
}

export function SessionInfo(d: IntrospectData) {
  const [expiry, setExpiry] = createSignal(relativeExpiry(d.token.expires_at));

  const timer = setInterval(() => {
    setExpiry(relativeExpiry(d.token.expires_at));
  }, 1000);

  setTimeout(() => {
    const check = setInterval(() => {
      if (!document.querySelector('.sandbox-expiry-live')) {
        clearInterval(timer);
        clearInterval(check);
      }
    }, 5000);
  }, 0);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Session'),
    h('div', null,
      createShow(
        () => d.session != null,
        () => h('div', null,
          CopyableSandboxRow('Session ID', d.session!.id, true),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Created'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.created_at)),
          ),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Expires'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.expires_at)),
          ),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Token Expires In'),
        h('span', { class: 'sandbox-expiry-live' }, expiry),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Algorithm'),
        h('code', { class: 'sandbox-detail-value' }, d.token.algorithm),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Issuer'),
        h('span', { class: 'sandbox-detail-value' }, d.token.issuer),
        CopyButton({ text: d.token.issuer, iconOnly: true, title: 'Copy issuer' }),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Audience'),
        h('span', { class: 'sandbox-detail-value' }, d.token.audience),
        CopyButton({ text: d.token.audience, iconOnly: true, title: 'Copy audience' }),
      ),
    ),
  );
}

function CopyableSandboxRow(label: string, value: string, monospace = false) {
  return h('div', { class: 'sandbox-detail-row' },
    h('span', { class: 'sandbox-detail-label' }, label),
    monospace
      ? h('code', { class: 'sandbox-detail-value' }, value)
      : h('span', { class: 'sandbox-detail-value' }, value),
    CopyButton({ text: value, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
  );
}
