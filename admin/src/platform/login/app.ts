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
import { init } from './store';

initThemeMode();
init();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ AuthCard, MfaPanel });
} else {
  mount(() => LoginPage(), '#app');
}
