import { h, createEffect, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { CopyButton } from '../../../core/components/ui/CopyButton';
import { selectedTenant, hostedPage, saveHostedPage } from '../store';

export function HostedPageView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const tenantState = () => selectedTenant() || tenant;
  const siteUrl = () => serverUrl + '/t/' + tenantState().slug + '/site';

  const [headline, setHeadline] = createSignal('');
  const [body, setBody] = createSignal('');
  const [ctaText, setCtaText] = createSignal('');
  const [ctaLink, setCtaLink] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [syncedTenantId, setSyncedTenantId] = createSignal<string | null>(null);
  const [syncedHeadline, setSyncedHeadline] = createSignal('Welcome');
  const [syncedBody, setSyncedBody] = createSignal('');
  const [syncedCtaText, setSyncedCtaText] = createSignal('');
  const [syncedCtaLink, setSyncedCtaLink] = createSignal('');

  createEffect(() => {
    const currentTenantId = tenantState().id;
    const page = hostedPage();
    const nextHeadline = page?.headline || 'Welcome';
    const nextBody = page?.body || '';
    const nextCtaText = page?.cta_text || '';
    const nextCtaLink = page?.cta_link || '';
    const tenantChanged = syncedTenantId() !== currentTenantId;
    const pristine = !isDirty();

    if (tenantChanged || pristine) {
      setHeadline(nextHeadline);
      setBody(nextBody);
      setCtaText(nextCtaText);
      setCtaLink(nextCtaLink);
    }

    setSyncedTenantId(currentTenantId);
    setSyncedHeadline(nextHeadline);
    setSyncedBody(nextBody);
    setSyncedCtaText(nextCtaText);
    setSyncedCtaLink(nextCtaLink);
  });

  function isDirty() {
    return headline() !== syncedHeadline()
      || body() !== syncedBody()
      || ctaText() !== syncedCtaText()
      || ctaLink() !== syncedCtaLink();
  }

  function resetDraft() {
    setHeadline(syncedHeadline());
    setBody(syncedBody());
    setCtaText(syncedCtaText());
    setCtaLink(syncedCtaLink());
  }

  async function save() {
    if (!isDirty()) {
      return;
    }
    setSaving(true);
    try {
      await saveHostedPage(tenantState().id, {
        headline: headline(),
        body: body(),
        cta_text: ctaText(),
        cta_link: ctaLink(),
      });
    } finally {
      setSaving(false);
    }
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Hosted Experience'),
        h('h2', { class: 'view-title' }, 'Hosted Page'),
        h('p', { class: 'view-subtitle text-muted' }, () => 'Shape the first page people see after authenticating into /t/' + tenantState().slug),
      ),
      h('p', { class: 'view-header-note' },
        'Keep the public-facing copy on the right track while preserving a direct preview link for quick QA.',
      ),
    ),
    h('div', { class: 'hosted-layout' },
      h('aside', { class: 'hosted-summary-rail' },
        h('div', { class: 'console-card hosted-summary-card' },
        h('div', { class: 'card-label' }, 'Published URL'),
        h('div', { class: 'inline-copy-row' },
          h('code', { class: 'code-block code-inline-block break-all' }, siteUrl),
          CopyButton({ text: siteUrl, label: 'Copy URL' }),
        ),
        h('div', { class: 'card-actions card-actions-vertical' },
          h('a', { href: siteUrl, target: '_blank', class: 'btn btn-sm btn-ghost' }, 'Preview page'),
        ),
        h('div', { class: 'detail-grid hosted-summary-list' },
          DetailRow('Headline', hostedPage()?.headline || 'Welcome'),
            DetailRow('CTA', hostedPage()?.cta_text || '(not set)'),
            DetailRow('Updated', hostedPage()?.updated_at ? new Date(hostedPage()!.updated_at!).toLocaleString() : 'Draft'),
          ),
        ),
        h('div', { class: 'console-card hosted-summary-card' },
          h('div', { class: 'card-label' }, 'Preview Flow'),
          h('p', { class: 'card-copy' },
            'Use the hosted site link for the real gated experience, or open the login preview when you want to review the surface without following the auth redirect.',
          ),
          h('div', { class: 'surface-link-list' },
            SurfaceLinkRow('Hosted Site', siteUrl(), 'Open'),
            SurfaceLinkRow('Login Preview', serverUrl + '/t/' + tenantState().slug + '/login?preview=1', 'Open'),
          ),
        ),
      ),
      h('section', { class: 'console-card hosted-editor-card' },
        h('div', { class: 'card-label' }, 'Page Content'),
        h('p', { class: 'card-copy' }, 'Refine the message, supporting text, and call to action without leaving the tenant console.' ),
        h('div', { class: 'form-stack' },
          Field({
            label: 'Headline',
            name: 'headline',
            value: headline,
            onChange: setHeadline,
            placeholder: 'Welcome to My App',
          }),
          h('div', { class: 'field' },
            h('label', null, 'Body Text'),
            h('textarea', {
              value: body,
              onInput: (e: Event) => setBody((e.target as HTMLTextAreaElement).value),
              placeholder: 'Tell your visitors what this is about...',
              rows: 6,
              class: 'full-width-resize',
            }),
          ),
          h('div', { class: 'form-grid-two' },
            Field({
              label: 'CTA Button Text',
              name: 'cta-text',
              value: ctaText,
              onChange: setCtaText,
              placeholder: 'Get Started',
            }),
            Field({
              label: 'CTA Button Link',
              name: 'cta-link',
              value: ctaLink,
              onChange: setCtaLink,
              placeholder: 'https://...',
            }),
          ),
        ),
        h('div', { class: () => 'editor-dirty-bar' + (isDirty() ? ' active' : '') },
          h('div', { class: 'editor-dirty-copy' },
            h('span', { class: 'editor-dirty-label' }, 'Unsaved changes'),
            h('span', { class: 'editor-dirty-description' }, 'Review the hosted page draft before publishing it live.'),
          ),
          h('div', { class: 'editor-dirty-actions' },
            h('button', {
              type: 'button',
              class: 'btn btn-sm btn-ghost',
              disabled: () => saving() || !isDirty(),
              onClick: resetDraft,
            }, 'Reset'),
            h('button', {
              type: 'button',
              class: 'btn btn-primary btn-sm',
              disabled: () => saving() || !isDirty(),
              onClick: save,
            }, () => saving() ? 'Saving...' : 'Save'),
          ),
        ),
      ),
    ),
  );
}

function SurfaceLinkRow(label: string, href: string, actionLabel: string) {
  return h('div', { class: 'surface-link-row' },
    h('div', { class: 'surface-link-copy' },
      h('span', { class: 'detail-label' }, label),
      h('code', { class: 'surface-link-value' }, href),
    ),
    h('div', { class: 'surface-link-actions' },
      h('a', {
        href,
        target: '_blank',
        class: 'btn btn-sm btn-ghost',
      }, actionLabel),
      CopyButton({ text: href, iconOnly: true, title: 'Copy ' + label.toLowerCase() }),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
