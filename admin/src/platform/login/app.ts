/**
 * PLANE 2 — Platform Login
 * USER: Unauthenticated Member signing in
 * SERVER DATA: window.__FORMA_SERVER_URL__, window.__PLATFORM_CONFIG__ { google_enabled, github_enabled }
 * ENTRY POINT: platform-login.js
 * ROUTE: /platform/login
 */
import { activateIslands, mount } from 'formajs';
import { initThemeMode } from '../../core/utils';
import { LoginPage } from './LoginPage';
import { AuthCard } from './AuthCard';
import { MfaPanel } from './MfaPanel';
import { InviteBanner, LoginCardHead, OnboardingCta } from './InviteIslands';
import { init } from './store';

initThemeMode();
init();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
// The invite islands MUST be registered here: only registered islands
// hydrate on the SSR route, and these three are what make a resolved
// ?owner_invite token update the page after load.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ AuthCard, MfaPanel, InviteBanner, LoginCardHead, OnboardingCta });
} else {
  mount(() => LoginPage(), '#app');
}
