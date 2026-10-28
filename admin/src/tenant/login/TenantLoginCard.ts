/**
 * TenantLoginCard — island component for tenant login
 *
 * Renders: error/success alerts, OAuth buttons, email/password form,
 * mode switching (login, register, forgot, reset), invite-only notice.
 * All state imported from store.ts singleton.
 */
import { h, createShow } from 'formajs';
import { Alert } from '../../core/components/ui/Alert';
import { GoogleIcon, GitHubIcon, EyeIcon, EyeOffIcon } from '../../core/components/ui/BrandIcons';
import {
  error, success, loading, mode, setMode, showPassword, setShowPassword,
  email, setEmail, password, setPassword, name, setName,
  rememberEmail, setRememberEmail,
  hasGoogle, hasGithub, hasOAuth, hasEmail,
  inviteRequired, allowSignup,
  startOAuth, submitForm, sendForgotPassword, persistEmail,
  appName, logoUrl,
  setError, setSuccess,
  type LoginMode,
} from './store';

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
  const inputId = opts.id || 'tl-password';
  return h('div', { class: 'tl-field' },
    h('label', { for: inputId }, opts.label),
    h('div', { class: 'tl-password-wrap' },
      h('input', {
        id: inputId,
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
          const el = document.getElementById(inputId);
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
    inviteRequired
      ? h('div', { class: 'tl-notice' },
          allowSignup
            ? 'Invite link detected. You can sign in or create your invited account.'
            : 'This tenant is invite-only. New account sign-up requires an invite link.',
        )
      : null,
    hasEmail
      ? h('form', { onSubmit: submitForm },
          createShow(() => mode() === 'register', () =>
            h('div', { class: 'tl-field' },
              h('label', { for: 'tl-name' }, 'Name'),
              h('input', {
                id: 'tl-name',
                type: 'text', placeholder: 'Your name', value: name,
                onInput: (e: Event) => setName((e.target as HTMLInputElement).value),
              }),
            ),
          ),
          h('div', { class: 'tl-field' },
            h('label', { for: 'tl-email' }, 'Email'),
            h('input', {
              id: 'tl-email',
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
          h('label', { class: 'tl-remember', for: 'tl-remember-email' },
            h('input', {
              id: 'tl-remember-email',
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
      ? allowSignup
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
      h('div', { class: 'secured-badge' },
        'Secured by ',
        h('a', { href: 'https://getforma.dev', target: '_blank', rel: 'noopener', class: 'secured-badge-link' }, 'fAuth'),
      ),
    ),
  );
}
