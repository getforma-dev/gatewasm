// RegistrationForm.ts — Island: Alerts + OAuth buttons + registration form
// All state and logic live in store.ts.
import { h, createShow } from 'formajs';
import {
  error, info, submitting,
  showPassword, setShowPassword,
  showConfirm, setShowConfirm,
  email, setEmail,
  password, setPassword,
  confirm, setConfirm,
  hasOAuth, hasGoogle, hasGithub,
  startOAuth, handleRegister, setError,
} from './store';

export function RegistrationForm() {
  return h('div', { class: 'registration-form-island' },
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

    // OAuth buttons
    createShow(
      () => hasOAuth,
      () => h('div', { class: 'oauth-actions' },
        // createShow, not a bare ternary: the ternary degraded to an empty
        // island shell server-side, and a SHOW_IF slot is what lets the
        // server inject the branch it knows (config names the providers).
        createShow(
          () => hasGoogle,
          () => h('button', {
            class: 'btn btn-ghost',
            type: 'button',
            onClick: () => startOAuth('google').catch(() => setError('Failed to start OAuth flow.')),
          }, 'Continue with Google'),
        ),
        createShow(
          () => hasGithub,
          () => h('button', {
            class: 'btn btn-ghost',
            type: 'button',
            onClick: () => startOAuth('github').catch(() => setError('Failed to start OAuth flow.')),
          }, 'Continue with GitHub'),
        ),
        h('div', { class: 'divider' }, 'or use email'),
      ),
    ),

    // Registration form
    h('form', {
      class: 'auth-form',
      autocomplete: 'on',
      onSubmit: handleRegister,
    },
      h('div', { class: 'field' },
        h('label', { for: 'reg-email' }, 'Email'),
        h('input', {
          id: 'reg-email',
          type: 'email',
          placeholder: 'you@company.com',
          autocomplete: 'email',
          value: () => email(),
          onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
        }),
      ),
      h('div', { class: 'field' },
        h('label', { for: 'reg-password' }, 'Password'),
        h('div', { class: 'password-wrap relative' },
          h('input', {
            id: 'reg-password',
            type: 'password',
            placeholder: 'At least 8 characters',
            autocomplete: 'new-password',
            class: 'input-pr',
            value: () => password(),
            onInput: (e: Event) => setPassword((e.target as HTMLInputElement).value),
          }),
          h('button', {
            class: 'password-toggle',
            type: 'button',
            'aria-label': () => showPassword() ? 'Hide password' : 'Show password',
            onClick: () => {
              const next = !showPassword();
              setShowPassword(next);
              const el = document.getElementById('reg-password') as HTMLInputElement | null;
              if (el) el.type = next ? 'text' : 'password';
            },
          }, () => showPassword() ? 'Hide' : 'Show'),
        ),
      ),
      h('div', { class: 'field' },
        h('label', { for: 'reg-confirm' }, 'Confirm password'),
        h('div', { class: 'password-wrap relative' },
          h('input', {
            id: 'reg-confirm',
            type: 'password',
            placeholder: 'Re-enter password',
            autocomplete: 'new-password',
            class: 'input-pr',
            value: () => confirm(),
            onInput: (e: Event) => setConfirm((e.target as HTMLInputElement).value),
          }),
          h('button', {
            class: 'password-toggle',
            type: 'button',
            'aria-label': () => showConfirm() ? 'Hide password' : 'Show password',
            onClick: () => {
              const next = !showConfirm();
              setShowConfirm(next);
              const el = document.getElementById('reg-confirm') as HTMLInputElement | null;
              if (el) el.type = next ? 'text' : 'password';
            },
          }, () => showConfirm() ? 'Hide' : 'Show'),
        ),
      ),
      h('button', {
        class: 'btn btn-primary full-width mt-xs',
        type: 'submit',
        disabled: () => submitting(),
      }, () => submitting() ? 'Creating account...' : 'Create Account'),
    ),
  );
}
