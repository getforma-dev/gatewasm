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
