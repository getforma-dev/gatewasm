/**
 * PLANE 3 — Tenant Sandbox
 * USER: Authenticated Member testing their Tenant's auth flow
 * SERVER DATA: window.__TENANT__ { slug, name }, window.__FORMA_SERVER_URL__
 * ENTRY POINT: tenant-sandbox.js
 * ROUTE: /t/:slug/sandbox
 */
import { mount } from 'formajs';
import { SandboxPage } from './SandboxPage';
import { init } from './store';

// Dev mode: clear stale service workers
if (location.hostname === 'localhost') {
  navigator.serviceWorker?.getRegistrations().then(rs => rs.forEach(r => r.unregister()));
}

init()
  .then(() => mount(() => SandboxPage(), '#app'))
  .catch(() => {});
