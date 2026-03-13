import { activateIslands } from 'formajs';
import { OnboardingPage } from './OnboardingPage';
import { RegistrationForm } from './RegistrationForm';
import { init } from './store';

// OnboardingPage import: SSR compiler traces this for IR generation.
// At runtime, esbuild tree-shakes it — only activateIslands() runs.
void OnboardingPage;

init();

activateIslands({
  RegistrationForm,
});
