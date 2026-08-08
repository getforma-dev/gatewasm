/**
 * PLANE 3 — Tenant Login
 * USER: Unauthenticated End User logging in through a Tenant's branded page
 * SERVER DATA: window.__TENANT__ { slug, server_url, providers, name, primary_color, logo_url, signup_mode, login_redirect_url, client_id }
 * ENTRY POINT: tenant-login.js
 * ROUTE: /t/:slug/login
 */
import { mount } from 'formajs';
import { TenantLoginCard } from './TenantLoginCard';
import { init } from './store';
import { initThemeMode } from '../../core/utils';


initThemeMode();
// Dev mode: clear stale service workers
if (location.hostname === 'localhost') {
  navigator.serviceWorker?.getRegistrations().then(rs => rs.forEach(r => r.unregister()));
}

// Phase1ClientMount (CSR): no SSR content, mount after store init.
// Store is a singleton module — TenantLoginCard imports deps directly.
init();
mount(() => TenantLoginCard(), '#app');
