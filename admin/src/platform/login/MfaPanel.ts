// MfaPanel.ts — Island: MFA verification, setup, and recovery
// All state and logic live in store.ts (shared with AuthCard island).
import { h, createShow } from 'formajs';
import {
  showMfa, mfaError, mfaSetup,
  showMfaRecoveryPassword, showMfaResetPasswordToggle,
  setShowMfaResetPasswordToggle,
  setMfaCodeInput, setMfaRecoveryPasswordInput,
  mfaRecoveryPasswordInput,
  completeMfaFromInline, cancelInlineMfa,
  beginMfaRecoveryFlow, resetMfaUsingPassword,
  setInfo, copyText, getMfaSetupUri,
} from './store';

export function MfaPanel() {
  return createShow(
    () => showMfa(),
    () => h('section', {
      class: 'mfa-panel',
      'aria-live': 'polite',
      'aria-label': 'Owner MFA verification',
    },
      h('div', { class: 'mfa-head' },
        h('h3', { class: 'no-margin mfa-title' }, 'Owner MFA Verification'),
        h('p', { class: 'mfa-subtitle no-margin text-muted-dim text-sm' },
          () => mfaSetup()?.secret
            ? 'Set up your authenticator app, then enter the 6-digit code.'
            : 'Enter the 6-digit code from your authenticator app.',
        ),
      ),

      // MFA secret (setup key + QR)
      createShow(
        () => !!mfaSetup()?.secret,
        () => {
          return h('div', { class: 'mfa-secret-box' },
            h('div', { class: 'mfa-secret-label' }, 'Authenticator setup key'),
            h('code', { class: 'mfa-secret-code' }, () => mfaSetup()?.secret || ''),
            createShow(
              () => !!(mfaSetup()?.qrDataUrl),
              () =>
                h('div', { class: 'flex justify-center mfa-qr-wrap' },
                  h('img', {
                    src: () => mfaSetup()?.qrDataUrl || '',
                    alt: 'MFA setup QR code',
                    class: 'mfa-qr-img',
                  }),
                ),
            ),
            h('div', { class: 'flex gap-sm flex-wrap' },
              h('button', {
                class: 'btn-link text-xxs',
                type: 'button',
                onClick: () => {
                  copyText(mfaSetup()?.secret || '').then(() => setInfo('Setup key copied.'));
                },
              }, 'Copy setup key'),
              h('button', {
                class: 'btn-link text-xxs',
                type: 'button',
                onClick: () => {
                  copyText(getMfaSetupUri() || '').then(() => setInfo('Setup URI copied.'));
                },
              }, 'Copy setup URI'),
            ),
            h('div', { class: 'mfa-hint text-xxs text-muted-dim' },
              'Use Duo Mobile, Google Authenticator, 1Password, or any TOTP app. In Duo, choose ',
              h('strong', { class: 'mfa-hint-strong' }, 'Use setup key'),
              '.',
            ),
          );
        },
      ),

      // MFA code input
      h('div', { class: 'field no-margin-bottom' },
        h('label', { for: 'mfa-code-input' }, 'Authenticator code'),
        h('input', {
          id: 'mfa-code-input',
          type: 'text',
          inputmode: 'numeric',
          maxlength: '6',
          placeholder: '123456',
          autocomplete: 'one-time-code',
          ref: (el: HTMLInputElement) => { setMfaCodeInput(el); },
          onKeydown: (e: KeyboardEvent) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              completeMfaFromInline();
            }
          },
          onInput: (e: Event) => {
            const input = e.target as HTMLInputElement;
            input.value = String(input.value || '').replace(/\D+/g, '').slice(0, 6);
          },
        }),
      ),

      // MFA recovery password field
      createShow(
        () => showMfaRecoveryPassword(),
        () =>
          h('form', {
            class: 'field no-margin-bottom',
            autocomplete: 'on',
            onSubmit: (e: Event) => {
              e.preventDefault();
              resetMfaUsingPassword();
            },
          },
            h('label', { for: 'mfa-reset-password-input' },
              'Account password (lost authenticator recovery)',
            ),
            h('div', { class: 'password-wrap relative' },
              h('input', {
                id: 'mfa-reset-password-input',
                type: () => showMfaResetPasswordToggle() ? 'text' : 'password',
                autocomplete: 'current-password',
                placeholder: 'Enter your account password',
                class: 'input-pr-lg',
                ref: (el: HTMLInputElement) => { setMfaRecoveryPasswordInput(el); },
              }),
              h('button', {
                class: 'password-toggle',
                type: 'button',
                'aria-label': () =>
                  showMfaResetPasswordToggle() ? 'Hide password' : 'Show password',
                onClick: () => {
                  setShowMfaResetPasswordToggle(!showMfaResetPasswordToggle());
                  mfaRecoveryPasswordInput?.focus();
                },
              }, () => showMfaResetPasswordToggle() ? 'Hide' : 'Show'),
            ),
            h('button', {
              class: 'btn btn-ghost full-width mt-sm',
              type: 'submit',
            }, 'Reset MFA Using Password'),
          ),
      ),

      // MFA error alert
      h('div', {
        class: () => 'alert error no-margin' + (mfaError() ? ' show' : ''),
      }, () => mfaError() || ''),

      // MFA footer buttons
      h('div', { class: 'flex gap-sm flex-wrap' },
        h('button', {
          class: 'btn btn-ghost mfa-action-btn',
          type: 'button',
          onClick: cancelInlineMfa,
        }, 'Cancel'),
        h('button', {
          class: 'btn btn-ghost mfa-action-btn',
          type: 'button',
          onClick: beginMfaRecoveryFlow,
        }, 'Reset Setup'),
        h('button', {
          class: 'btn btn-primary mfa-action-btn',
          type: 'button',
          onClick: completeMfaFromInline,
        }, 'Verify MFA'),
      ),
    ),
  );
}
