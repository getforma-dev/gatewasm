import { mount } from 'formajs';
import { TenantLoginCard } from './TenantLoginCard';
import { init } from './store';

// Dev mode: clear stale service workers
if (location.hostname === 'localhost') {
  navigator.serviceWorker?.getRegistrations().then(rs => rs.forEach(r => r.unregister()));
}

// Phase1ClientMount (CSR): no SSR content, mount after store init.
// Store is a singleton module — TenantLoginCard imports deps directly.
init();
mount(() => TenantLoginCard(), '#app');
