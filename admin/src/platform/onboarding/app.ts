import { activateIslands, mount } from 'formajs';
import { OnboardingPage } from './OnboardingPage';
import { RegistrationForm } from './RegistrationForm';
import { init } from './store';

init();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ RegistrationForm });
} else {
  mount(() => OnboardingPage(), '#app');
}
