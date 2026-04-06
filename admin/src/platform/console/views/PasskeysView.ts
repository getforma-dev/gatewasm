import { h, createShow } from 'formajs';
import { Skeleton } from '../../../core/components/ui/Skeleton';
import { passkeys, passkeysLoading, type Passkey } from '../store';

export function PasskeysView() {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'End User'),
        h('h2', { class: 'view-title' }, 'Passkeys'),
        h('p', { class: 'view-subtitle text-muted' }, () => passkeys().length + ' registered credentials'),
      ),
    ),
    h('section', { class: 'console-card list-shell section-box section-box-bottom' },
      h('div', { class: 'list-shell-head' },
        h('div', { class: 'section-copy' },
          h('div', { class: 'card-label' }, 'WebAuthn Credentials'),
          h('p', { class: 'card-copy' }, () => passkeys().length + ' passkeys across all users'),
        ),
      ),
      createShow(
        () => passkeysLoading(),
        () => h('div', { class: 'list-empty-state list-empty-state-loading' },
          Skeleton({ width: '50%', height: '12px' }),
          Skeleton({ width: '80%', height: '12px' }),
        ),
      ),
      createShow(
        () => !passkeysLoading() && passkeys().length === 0,
        () => h('div', { class: 'list-empty-state' },
          h('div', { class: 'card-label' }, 'No Passkeys Registered'),
          h('p', { class: 'card-copy' }, 'Passkeys will appear here when end users register WebAuthn credentials.'),
        ),
      ),
      createShow(
        () => !passkeysLoading() && passkeys().length > 0,
        () => h('div', { class: 'user-list user-list-shell' },
          () => passkeys().map((pk) => PasskeyRow(pk)),
        ),
      ),
    ),
  );
}

function PasskeyRow(pk: Passkey) {
  return h('div', { class: 'user-row' },
    h('div', { class: 'user-row-summary' },
      h('div', { class: 'user-row-main' },
        h('div', { class: 'user-email' },
          pk.name,
          pk.is_synced
            ? h('span', { class: 'badge badge-subtle' }, 'Synced')
            : h('span', { class: 'badge badge-subtle' }, 'Device-bound'),
        ),
        h('div', { class: 'user-meta text-muted' },
          pk.email + ' \u00b7 Registered ' + new Date(pk.created_at).toLocaleDateString(),
          pk.last_used_at
            ? ' \u00b7 Last used ' + new Date(pk.last_used_at).toLocaleDateString()
            : ' \u00b7 Never used',
        ),
      ),
    ),
  );
}
