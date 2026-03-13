// AuthCard.ts — Island: Alerts + OAuth buttons + email/password form + forgot + MFA trouble
// All state and logic live in store.ts (shared with MfaPanel island).
import { h, createShow } from 'formajs';
import {
  error, info, busy, showPassword, capsLock, showOauthHint,
  setShowPassword, setCapsLock,
  hasOAuth, hasGoogle, hasGithub,
  callbackGoogle, callbackGithub,
  setEmailInput, setPasswordInput, setRememberCheck,
  passwordInput,
  startOAuth, loginWithEmail, persistRememberedEmail,
  requestPasswordReset, beginMfaRecoveryFlow,
  setInfo, copyText,
} from './store';

export function AuthCard() {
  return h('div', { class: 'auth-card-island' },
    // Error alert
    h('div', {
      class: () => 'alert error' + (error() ? ' show' : ''),
      role: 'alert',
    }, () => error() || ''),

    // Info alert
    h('div', {
      class: () => 'alert info' + (info() ? ' show' : ''),
      role: 'status',
      'aria-live': 'polite',
    }, () => info() || ''),

    // OAuth config hint
    h('div', {
      class: () => 'alert info oauth-config' + (showOauthHint() ? ' show' : ''),
      role: 'note',
    },
      h('strong', null, 'OAuth callback check'),
      h('code', null, 'Google: ' + callbackGoogle),
      h('code', null, 'GitHub: ' + callbackGithub),
      h('button', {
        class: 'btn-link mt-xs',
        type: 'button',
        onClick: () => {
          copyText('Google: ' + callbackGoogle + '\nGitHub: ' + callbackGithub).then(() =>
            setInfo('Callback URLs copied.'),
          );
        },
      }, 'Copy callback URLs'),
    ),

    // OAuth buttons
    createShow(
      () => hasOAuth,
      () =>
        h('div', { class: 'actions' },
          hasGoogle
            ? h('button', {
                class: 'btn btn-ghost',
                type: 'button',
                disabled: () => !!busy(),
                onClick: () => startOAuth('google'),
              }, () => busy() === 'oauth-google' ? 'Redirecting to Google...' : 'Continue with Google')
            : null,
          hasGithub
            ? h('button', {
                class: 'btn btn-ghost',
                type: 'button',
                disabled: () => !!busy(),
                onClick: () => startOAuth('github'),
              }, () => busy() === 'oauth-github' ? 'Redirecting to GitHub...' : 'Continue with GitHub')
            : null,
          h('div', { class: 'divider' }, 'or use email'),
        ),
    ),

    // Email form
    h('form', {
      class: 'form',
      autocomplete: 'on',
      onSubmit: loginWithEmail,
    },
      h('div', { class: 'field' },
        h('label', { for: 'email' }, 'Email'),
        h('input', {
          id: 'email',
          type: 'email',
          placeholder: 'you@company.com',
          autocomplete: 'email',
          ref: (el: HTMLInputElement) => { setEmailInput(el); },
          onBlur: persistRememberedEmail,
        }),
      ),
      h('div', { class: 'field' },
        h('label', { for: 'password' }, 'Password'),
        h('div', { class: 'password-wrap relative' },
          h('input', {
            id: 'password',
            type: 'password',
            placeholder: 'Your password',
            autocomplete: 'current-password',
            class: 'input-pr-lg',
            ref: (el: HTMLInputElement) => { setPasswordInput(el); },
            onKeydown: (e: KeyboardEvent) => {
              setCapsLock(!!(e.getModifierState && e.getModifierState('CapsLock')));
            },
            onKeyup: (e: KeyboardEvent) => {
              setCapsLock(!!(e.getModifierState && e.getModifierState('CapsLock')));
            },
            onBlur: () => setCapsLock(false),
          }),
          h('button', {
            class: 'password-toggle',
            type: 'button',
            'aria-label': () => showPassword() ? 'Hide password' : 'Show password',
            onClick: () => {
              const next = !showPassword();
              setShowPassword(next);
              const el = passwordInput || document.getElementById('password') as HTMLInputElement | null;
              if (el) {
                el.type = next ? 'text' : 'password';
                el.focus();
              }
            },
          }, () => showPassword() ? 'Hide' : 'Show'),
        ),
        h('div', {
          class: () => 'field-hint field-hint--danger' + (capsLock() ? '' : ' hidden'),
        }, 'Caps Lock appears to be on.'),
      ),
      h('button', {
        class: 'btn btn-primary full-width',
        type: 'submit',
        disabled: () => !!busy(),
      }, () => busy() === 'login' ? 'Signing in...' : 'Sign In'),
      h('label', { class: 'remember-email checkbox-label' },
        h('input', {
          type: 'checkbox',
          class: 'checkbox-auto',
          ref: (el: HTMLInputElement) => { setRememberCheck(el); },
          onChange: persistRememberedEmail,
        }),
        'Remember email on this device',
      ),
      h('div', { class: 'mini-row flex justify-between items-center gap-md mt-xs' },
        h('button', {
          class: 'btn-link text-xs no-pad',
          type: 'button',
          onClick: requestPasswordReset,
        }, 'Forgot password'),
        h('button', {
          class: 'btn-link secondary text-xs no-pad text-muted-dim',
          type: 'button',
          onClick: beginMfaRecoveryFlow,
        }, 'Trouble with MFA?'),
      ),
    ),
  );
}
