/**
 * PLANE 2 — Tenant Management Console
 * USER: Authenticated Member managing their Organization's Tenants
 * SERVER DATA: window.__FORMA_SERVER_URL__ (via store)
 * ENTRY POINT: platform-console.js
 * ROUTE: /platform/console
 */
import { mount } from 'formajs';
import { initThemeMode } from '../../core/utils';
import { ConsoleApp } from './ConsoleApp';
import { init } from './store';

initThemeMode();

// Dev mode: clear stale service workers
if (location.hostname === 'localhost') {
  navigator.serviceWorker?.getRegistrations().then(rs => rs.forEach(r => r.unregister()));
}

// Auth-gated page: no SSR content, CSR mount after auth.
// Store is a singleton module — ConsoleApp imports deps directly.
// init() rejects on auth failure (redirects to login).
init()
  .then(() => mount(() => ConsoleApp(), '#app'))
  .catch(() => {});
