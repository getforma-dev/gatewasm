/**
 * TenantLoginCard — island component for tenant login
 *
 * Renders: error/success alerts, OAuth buttons, email/password form,
 * mode switching (login, register, forgot, reset), invite-only notice.
 * All state imported from store.ts singleton.
 */
import { h, createShow } from 'formajs';
import { Alert } from '../../core/components/ui/Alert';
import {
  error, success, loading, mode, setMode, showPassword, setShowPassword,
  email, setEmail, password, setPassword, name, setName,
  rememberEmail, setRememberEmail,
  hasGoogle, hasGithub, hasOAuth, hasEmail,
  inviteOnly, allowPublicSignup,
  startOAuth, submitForm, sendForgotPassword, persistEmail,
  appName, logoUrl,
  setError, setSuccess,
  type LoginMode,
} from './store';

// ── SVG icons ───────────────────────────────────────────────────────────

function GoogleIcon() {
  return h('svg', { width: '18', height: '18', viewBox: '0 0 18 18' },
    h('path', { fill: '#4285F4', d: 'M17.64 9.2c0-.637-.057-1.251-.164-1.84H9v3.481h4.844a4.14 4.14 0 0 1-1.796 2.716v2.259h2.908c1.702-1.567 2.684-3.875 2.684-6.615z' }),
    h('path', { fill: '#34A853', d: 'M9 18c2.43 0 4.467-.806 5.956-2.18l-2.908-2.259c-.806.54-1.837.86-3.048.86-2.344 0-4.328-1.584-5.036-3.711H.957v2.332A8.997 8.997 0 0 0 9 18z' }),
    h('path', { fill: '#FBBC05', d: 'M3.964 10.71A5.41 5.41 0 0 1 3.682 9c0-.593.102-1.17.282-1.71V4.958H.957A8.997 8.997 0 0 0 0 9c0 1.452.348 2.827.957 4.042l3.007-2.332z' }),
    h('path', { fill: '#EA4335', d: 'M9 3.58c1.321 0 2.508.454 3.44 1.345l2.582-2.58C13.463.891 11.426 0 9 0A8.997 8.997 0 0 0 .957 4.958L3.964 6.29C4.672 4.163 6.656 2.58 9 3.58z' }),
  );
}

function GitHubIcon() {
  return h('svg', { width: '18', height: '18', viewBox: '0 0 16 16', fill: 'currentColor' },
    h('path', { d: 'M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z' }),
  );
}

function EyeIcon() {
  return h('svg', { width: '16', height: '16', viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '2' },
    h('path', { d: 'M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z' }),
    h('circle', { cx: '12', cy: '12', r: '3' }),
  );
}

function EyeOffIcon() {
  return h('svg', { width: '16', height: '16', viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '2' },
    h('path', { d: 'M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24' }),
    h('line', { x1: '1', y1: '1', x2: '23', y2: '23' }),
  );
}

// ── Sub-components ──────────────────────────────────────────────────────

function OAuthButtons() {
  return h('div', { class: 'tl-oauth-buttons' },
    hasGoogle
      ? h('button', {
          class: 'tl-btn tl-btn-oauth', type: 'button',
          disabled: loading, onClick: () => startOAuth('google'),
        }, GoogleIcon(), 'Continue with Google')
      : null,
    hasGithub
      ? h('button', {
          class: 'tl-btn tl-btn-oauth', type: 'button',
          disabled: loading, onClick: () => startOAuth('github'),
        }, GitHubIcon(), 'Continue with GitHub')
      : null,
  );
}

function PasswordField(opts: {
  label: string; placeholder: string; autocomplete: string;
  value: () => string; onInput: (v: string) => void;
  id?: string;
}) {
  return h('div', { class: 'tl-field' },
    h('label', null, opts.label),
    h('div', { class: 'tl-password-wrap' },
      h('input', {
        id: opts.id,
        type: 'password',
        placeholder: opts.placeholder,
        required: true,
        autocomplete: opts.autocomplete,
        value: opts.value,
        onInput: (e: Event) => opts.onInput((e.target as HTMLInputElement).value),
      }),
      h('button', {
        class: 'tl-password-toggle', type: 'button',
        'aria-label': () => showPassword() ? 'Hide password' : 'Show password',
        onClick: () => {
          setShowPassword(!showPassword());
          const el = document.getElementById(opts.id || 'tl-password');
          if (el) (el as HTMLInputElement).type = showPassword() ? 'text' : 'password';
        },
      }, () => showPassword() ? EyeOffIcon() : EyeIcon()),
    ),
  );
}

function ForgotSentView() {
  return h('div', { class: 'tl-message-view' },
    h('p', null, 'If that email is registered, you will receive a password reset link.'),
    h('a', { class: 'tl-link', onClick: () => { setMode('login'); setError(null); setSuccess(null); } }, 'Back to sign in'),
  );
}

function ResetDoneView() {
  return h('div', { class: 'tl-message-view' },
    h('p', null, 'Password updated successfully.'),
    h('a', { class: 'tl-link', onClick: () => { setMode('login'); setError(null); setSuccess(null); } }, 'Back to sign in'),
  );
}

function ResetView() {
  return h('div', null,
    h('div', { class: 'tl-subtitle' }, 'Set a new password'),
    h('form', { onSubmit: submitForm },
      PasswordField({
        label: 'New password', placeholder: '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022',
        autocomplete: 'new-password', value: password, onInput: setPassword,
        id: 'tl-password',
      }),
      h('button', { type: 'submit', class: 'tl-btn tl-btn-primary', disabled: loading },
        () => loading() ? 'Please wait\u2026' : 'Set new password'),
    ),
    h('div', { class: 'tl-toggle' },
      h('a', { class: 'tl-link', onClick: () => { setMode('login'); setError(null); } }, 'Back to sign in'),
    ),
  );
}

function AuthFormView() {
  return h('div', null,
    h('div', { class: 'tl-subtitle' }, () =>
      mode() === 'login' ? 'Sign in to your account' : 'Create your account',
    ),
    hasOAuth ? OAuthButtons() : null,
    hasOAuth && hasEmail ? h('div', { class: 'tl-divider' }, 'or') : null,
    inviteOnly
      ? h('div', { class: 'tl-notice' },
          allowPublicSignup
            ? 'Invite link detected. You can sign in or create your invited account.'
            : 'This tenant is invite-only. New account sign-up requires an invite link.',
        )
      : null,
    hasEmail
      ? h('form', { onSubmit: submitForm },
          createShow(() => mode() === 'register', () =>
            h('div', { class: 'tl-field' },
              h('label', null, 'Name'),
              h('input', {
                type: 'text', placeholder: 'Your name', value: name,
                onInput: (e: Event) => setName((e.target as HTMLInputElement).value),
              }),
            ),
          ),
          h('div', { class: 'tl-field' },
            h('label', null, 'Email'),
            h('input', {
              type: 'email', placeholder: 'you@example.com', required: true,
              autocomplete: 'email', value: email,
              onInput: (e: Event) => setEmail((e.target as HTMLInputElement).value),
            }),
          ),
          PasswordField({
            label: 'Password', placeholder: '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022',
            autocomplete: mode() === 'login' ? 'current-password' : 'new-password',
            value: password, onInput: setPassword,
            id: 'tl-password',
          }),
          createShow(() => mode() === 'login', () =>
            h('div', { class: 'tl-forgot' },
              h('a', { class: 'tl-link', onClick: sendForgotPassword }, 'Forgot password?'),
            ),
          ),
          h('label', { class: 'tl-remember' },
            h('input', {
              type: 'checkbox', checked: rememberEmail,
              onChange: (e: Event) => {
                setRememberEmail((e.target as HTMLInputElement).checked);
                persistEmail();
              },
            }),
            'Remember email',
          ),
          h('button', { type: 'submit', class: 'tl-btn tl-btn-primary', disabled: loading },
            () => loading()
              ? 'Please wait\u2026'
              : mode() === 'login' ? 'Sign in' : 'Create account',
          ),
        )
      : null,
    hasEmail
      ? allowPublicSignup
        ? h('div', { class: 'tl-toggle' },
            () => mode() === 'login' ? "Don\u2019t have an account? " : 'Already have an account? ',
            h('a', {
              class: 'tl-link',
              onClick: () => {
                setMode((m: LoginMode) => m === 'login' ? 'register' as LoginMode : 'login' as LoginMode);
                setError(null);
                setSuccess(null);
              },
            }, () => mode() === 'login' ? 'Sign up' : 'Sign in'),
          )
        : h('div', { class: 'tl-toggle' }, 'Need access? Ask your tenant admin for an invite link.')
      : null,
  );
}

// ── Main island render ──────────────────────────────────────────────────

export function TenantLoginCard() {
  return h('div', { class: 'tl-page' },
    h('div', { class: 'tl-card' },
      h('div', { class: 'tl-logo-area' },
        logoUrl ? h('img', { src: logoUrl, alt: appName, class: 'tl-logo' }) : null,
        h('div', { class: 'tl-app-name' }, appName),
      ),
      Alert({ message: error, variant: 'error' }),
      Alert({ message: success, variant: 'success' }),
      createShow(() => mode() === 'forgot-sent', ForgotSentView),
      createShow(() => mode() === 'reset-done', ResetDoneView),
      createShow(() => mode() === 'reset', ResetView),
      createShow(() => mode() === 'login' || mode() === 'register', AuthFormView),
      h('div', { class: 'tl-powered' },
        'Secured by ',
        h('a', { href: 'https://gatewasm.dev', target: '_blank', rel: 'noopener' }, 'GateWASM'),
      ),
    ),
  );
}
