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

  // Sync form fields when data loads
  const syncFields = () => {
    const p = hostedPage();
    if (p && !initialized()) {
      setHeadline(p.headline || 'Welcome');
      setBody(p.body || '');
      setCtaText(p.cta_text || '');
      setCtaLink(p.cta_link || '');
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
    // Trigger sync on render
    h('span', { class: 'hidden' }, syncFields),
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Hosted Page'),
      h('p', { class: 'view-subtitle text-muted' }, 'Your free, auth-gated landing page'),
    ),

    // Live URL
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Page URL'),
      h('div', { class: 'flex items-center gap-sm' },
        h('code', { class: 'card-value flex-1' }, siteUrl),
        h('a', { href: siteUrl, target: '_blank', class: 'btn btn-sm btn-ghost' }, 'Preview'),
        h('button', {
          class: 'btn btn-sm btn-ghost',
          onClick: () => { navigator.clipboard.writeText(siteUrl); toast.show('Copied', 'success'); },
        }, 'Copy'),
      ),
    ),

    // Editor
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Page Content'),
      Field({ label: 'Headline', name: 'headline', value: headline, onChange: setHeadline, placeholder: 'Welcome to My App' }),
      h('div', { class: 'field' },
        h('label', null, 'Body Text'),
        h('textarea', {
          value: body,
          onInput: (e: Event) => setBody((e.target as HTMLTextAreaElement).value),
          placeholder: 'Tell your visitors what this is about...',
          rows: 4,
          class: 'full-width-resize',
        }),
      ),
      Field({ label: 'CTA Button Text', name: 'cta-text', value: ctaText, onChange: setCtaText, placeholder: 'Get Started' }),
      Field({ label: 'CTA Button Link', name: 'cta-link', value: ctaLink, onChange: setCtaLink, placeholder: 'https://...' }),
      h('button', {
        class: 'btn btn-primary btn-sm mt-md',
        disabled: saving,
        onClick: save,
      }, () => saving() ? 'Saving...' : 'Save Page'),
    ),

    // Upsell
    h('p', { class: 'text-muted text-xs mt-md text-center' },
      'Need more pages or a full app? ',
      h('a', { href: 'https://getforma.dev', target: '_blank', class: 'btn-link' }, 'Check out Forma'),
    ),
  );
}
