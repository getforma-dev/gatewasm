import { h, createSignal } from 'formajs';
import { Field } from '../../../core/components/ui/Field';
import { updateTenant, type Tenant } from '../store';

interface BrandingStepProps {
  tenant: Tenant;
  onNext: () => void;
}

export function BrandingStep({ tenant, onNext }: BrandingStepProps) {
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const loginUrl = serverUrl + '/t/' + tenant.slug + '/login';

  const [appName, setAppName] = createSignal(tenant.name || '');
  const [color, setColor] = createSignal(tenant.primary_color || '#6366f1');
  const [saving, setSaving] = createSignal(false);

  async function handleNext() {
    setSaving(true);
    await updateTenant(tenant.id, {
      name: appName(),
      primary_color: color(),
      onboarding_step: 2,
    });
    setSaving(false);
    onNext();
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 1 of 3'),
      h('h2', { class: 'wizard-step-title' }, 'Make it yours'),
      h('p', { class: 'wizard-step-subtitle' },
        'Give your login page a name and color. Takes five seconds.',
      ),
    ),
    h('div', { class: 'wizard-step-body' },
      // App Name field — uses shared Field component
      Field({
        label: 'App Name',
        name: 'app-name',
        value: appName,
        onChange: setAppName,
        placeholder: 'My SaaS App',
        required: true,
      }),
      // Login URL preview (read-only slug display)
      h('div', { class: 'field' },
        h('label', null, 'Login URL'),
        h('code', {
          class: 'code-block block text-sm break-all ws-normal',
        }, loginUrl),
      ),
      // Color field — custom layout for color picker + text input
      h('div', { class: 'field' },
        h('label', null, 'Primary Color'),
        h('div', { class: 'flex items-center gap-sm' },
          h('input', {
            type: 'color',
            value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            class: 'color-picker-btn',
          }),
          h('input', {
            type: 'text',
            value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            class: 'flex-1',
          }),
        ),
      ),
      // Live preview — mini mockup of login page header
      h('div', { class: 'wizard-preview' },
        h('div', {
          class: 'wizard-preview-card',
          style: () => `border-top: 3px solid ${color()};`,
        },
          h('div', {
            class: 'wizard-preview-name',
            style: () => `color: ${color()};`,
          }, () => appName() || 'Your App'),
          h('div', { class: 'wizard-preview-subtitle' }, 'Sign in to your account'),
          h('div', { class: 'wizard-preview-field' }),
          h('div', { class: 'wizard-preview-field' }),
          h('div', {
            class: 'wizard-preview-btn',
            style: () => `background: ${color()};`,
          }, 'Sign in'),
        ),
      ),
    ),
    h('div', { class: 'wizard-step-footer' },
      h('button', {
        class: 'btn btn-primary',
        disabled: () => saving() || !appName().trim(),
        onClick: handleNext,
      }, () => saving() ? 'Saving...' : 'Next \u2192'),
    ),
  );
}
