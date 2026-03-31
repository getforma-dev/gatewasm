import { mount } from 'formajs';
import { SandboxPage } from './SandboxPage';
import { init } from './store';

init()
  .then(() => mount(() => SandboxPage(), '#app'))
  .catch(() => {});
