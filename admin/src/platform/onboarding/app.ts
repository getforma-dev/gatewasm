/**
 * PLANE 2 — Platform Onboarding
 * USER: New Member creating an Organization
 * SERVER DATA: window.__FORMA_SERVER_URL__, window.__PLATFORM_CONFIG__ { google_enabled, github_enabled, client_id }
 * ENTRY POINT: platform-onboarding.js
 * ROUTE: /platform/onboarding
 */
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
