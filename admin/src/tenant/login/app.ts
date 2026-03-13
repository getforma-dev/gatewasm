import { mount } from 'formajs';
import { TenantLoginCard } from './TenantLoginCard';
import { init } from './store';

// Phase1ClientMount (CSR): no SSR content, mount after store init.
// Store is a singleton module — TenantLoginCard imports deps directly.
init();
mount(() => TenantLoginCard(), '#app');
