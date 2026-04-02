import { h, createSignal, createSwitch } from 'formajs';
import { selectedTenant } from '../store';
import { BrandingStep } from './BrandingStep';
import { TestAuthStep } from './TestAuthStep';
import { ChoosePathStep } from './ChoosePathStep';

export function WizardFlow() {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  // Resume from persisted step
  const [step, setStep] = createSignal<1 | 2 | 3>(
    (tenant.onboarding_step as 1 | 2 | 3) || 1
  );

  function advance() {
    const next = (step() + 1) as 1 | 2 | 3;
    if (next <= 3) setStep(next);
  }

  return h('div', { class: 'wizard' },
    createSwitch(
      () => String(step()),
      [
        { match: '1', render: () => BrandingStep({ tenant, onNext: advance }) },
        { match: '2', render: () => TestAuthStep({ tenant, onNext: advance }) },
        { match: '3', render: () => ChoosePathStep({ tenant }) },
      ],
    ),
  );
}
