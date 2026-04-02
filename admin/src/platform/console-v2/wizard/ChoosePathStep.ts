import { h, createSignal } from 'formajs';
import { updateTenant, setView, loadTenantDetail, type Tenant } from '../store';

interface ChoosePathStepProps {
  tenant: Tenant;
}

export function ChoosePathStep({ tenant }: ChoosePathStepProps) {
  const [selected, setSelected] = createSignal<'hosted-page' | 'integrate' | null>(null);
  const [saving, setSaving] = createSignal(false);

  async function handleFinish() {
    const choice = selected();
    if (!choice) return;
    setSaving(true);
    await updateTenant(tenant.id, {
      onboarding_completed: true,
      preferred_tab: choice,
    });
    setSaving(false);
    await loadTenantDetail(tenant.id);
    setView(choice);
  }

  function cardClass(option: string): string {
    return 'wizard-path-card' + (selected() === option ? ' selected' : '');
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 3 of 3'),
      h('h2', { class: 'wizard-step-title' }, 'What do you want to do next?'),
      h('p', { class: 'wizard-step-subtitle' },
        'Both options are always available in your console. Pick where to start.',
      ),
    ),
    h('div', { class: 'wizard-step-body' },
      h('div', { class: 'wizard-paths' },
        h('div', {
          class: () => cardClass('hosted-page'),
          onClick: () => setSelected('hosted-page'),
        },
          h('h3', null, 'Host it here'),
          h('p', null, "Don't have an app yet? We'll host an auth-gated page for you. Free forever."),
        ),
        h('div', {
          class: () => cardClass('integrate'),
          onClick: () => setSelected('integrate'),
        },
          h('h3', null, 'Add it to your app'),
          h('p', null, 'Already have an app? Integrate auth in three steps with bridge.js.'),
        ),
      ),
      h('p', { class: 'text-muted-dim text-xs text-center' },
        'You can switch anytime from your console.',
      ),
    ),
    h('div', { class: 'wizard-step-footer' },
      h('button', {
        class: 'btn btn-primary',
        disabled: () => !selected() || saving(),
        onClick: handleFinish,
      }, () => saving() ? 'Saving...' : 'Go to Console →'),
    ),
  );
}
