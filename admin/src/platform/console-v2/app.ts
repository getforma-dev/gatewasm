import { mount } from 'formajs';
import { ConsoleApp } from './ConsoleApp';
import { init } from './store';

// Auth-gated page: no SSR content, CSR mount after auth.
// Store is a singleton module — ConsoleApp imports deps directly.
// init() rejects on auth failure (redirects to login).
init()
  .then(() => mount(() => ConsoleApp(), '#app'))
  .catch(() => {});
