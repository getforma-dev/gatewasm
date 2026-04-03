import { h, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { selectedTenant, hostedPage, loadHostedPage, saveHostedPage, toast } from '../store';

export function HostedPageView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const siteUrl = serverUrl + '/t/' + tenant.slug + '/site';

  loadHostedPage(tenant.id);

  const [headline, setHeadline] = createSignal('');
  const [body, setBody] = createSignal('');
  const [ctaText, setCtaText] = createSignal('');
  const [ctaLink, setCtaLink] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [initialized, setInitialized] = createSignal(false);

  const syncFields = () => {
    const page = hostedPage();
    if (page && !initialized()) {
      setHeadline(page.headline || 'Welcome');
      setBody(page.body || '');
      setCtaText(page.cta_text || '');
      setCtaLink(page.cta_link || '');
      setInitialized(true);
    }
  };

  async function save() {
    setSaving(true);
    await saveHostedPage(tenant.id, {
      headline: headline(),
      body: body(),
      cta_text: ctaText(),
      cta_link: ctaLink(),
    });
    setSaving(false);
  }

  return h('div', { class: 'console-view' },
    h('span', { class: 'hidden' }, syncFields),
    h('div', { class: 'view-header view-header-split' },
      h('div', { class: 'view-header-body' },
        h('div', { class: 'view-eyebrow' }, 'Hosted Experience'),
        h('h2', { class: 'view-title' }, 'Hosted Page'),
        h('p', { class: 'view-subtitle text-muted' }, 'Shape the first page people see after authenticating into /t/' + tenant.slug),
      ),
      h('p', { class: 'view-header-note' },
        'Keep the public-facing copy on the right track while preserving a direct preview link for quick QA.',
      ),
    ),
    h('div', { class: 'hosted-layout' },
      h('aside', { class: 'console-card hosted-summary-card' },
        h('div', { class: 'card-label' }, 'Published URL'),
        h('code', { class: 'code-block code-inline-block break-all' }, siteUrl),
        h('div', { class: 'card-actions card-actions-vertical' },
          h('a', { href: siteUrl, target: '_blank', class: 'btn btn-sm btn-ghost' }, 'Preview page'),
          h('button', {
            class: 'btn btn-sm btn-ghost',
            onClick: () => { navigator.clipboard.writeText(siteUrl); toast.show('Copied', 'success'); },
          }, 'Copy URL'),
        ),
        h('div', { class: 'detail-grid hosted-summary-list' },
          DetailRow('Headline', hostedPage()?.headline || 'Welcome'),
          DetailRow('CTA', hostedPage()?.cta_text || '(not set)'),
          DetailRow('Updated', hostedPage()?.updated_at ? new Date(hostedPage()!.updated_at!).toLocaleString() : 'Draft'),
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
        h('div', { class: 'card-actions' },
          h('button', {
            class: 'btn btn-primary btn-sm',
            disabled: saving,
            onClick: save,
          }, () => saving() ? 'Saving...' : 'Save Page'),
        ),
      ),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'detail-row' },
    h('span', { class: 'detail-label' }, label),
    h('span', { class: 'detail-value' }, value),
  );
}
