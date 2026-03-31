import { activateIslands, mount } from 'formajs';
import { LoginPage } from './LoginPage';
import { AuthCard } from './AuthCard';
import { MfaPanel } from './MfaPanel';
import { init } from './store';

init();

// Phase 2 SSR: hydrate island markers. Phase 1 (dev mode): full CSR mount.
if (document.querySelector('[data-forma-island]')) {
  activateIslands({ AuthCard, MfaPanel });
} else {
  mount(() => LoginPage(), '#app');
}
