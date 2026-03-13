import { activateIslands } from 'formajs';
import { LoginPage } from './LoginPage';
import { AuthCard } from './AuthCard';
import { MfaPanel } from './MfaPanel';
import { init } from './store';

// LoginPage import: SSR compiler traces this for IR generation.
// At runtime, esbuild tree-shakes it — only activateIslands() runs.
void LoginPage;

init();

activateIslands({
  AuthCard,
  MfaPanel,
});
