/**
 * Platform Login Store — shared reactive state for login islands
 *
 * All signals, API helpers, and auth flow logic live here.
 * AuthCard and MfaPanel islands import from this module.
 */
import { createSignal } from 'formajs';

// ── Server config (injected by Rust handler) ──────────────────────────
export const serverUrl: string =
  (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const platformConfig: { google_enabled?: boolean; github_enabled?: boolean } =
  (window as any).__PLATFORM_CONFIG__ || {};
const oauthClientId = 'platform-login';
const oauthRedirectUri = serverUrl + '/platform/login';

// ── OAuth config ──────────────────────────────────────────────────────
export const callbackGoogle = serverUrl + '/auth/callback/google';
export const callbackGithub = serverUrl + '/auth/callback/github';
export const hasGoogle = !!platformConfig.google_enabled;
export const hasGithub = !!platformConfig.github_enabled;
export const hasOAuth = hasGoogle || hasGithub;

// ── Signals ───────────────────────────────────────────────────────────
export const [error, setError] = createSignal(null as string | null);
export const [info, setInfo] = createSignal(null as string | null);
export const [busy, setBusy] = createSignal(null as string | null);
export const [showPassword, setShowPassword] = createSignal(false);
export const [capsLock, setCapsLock] = createSignal(false);
export const [showMfa, setShowMfa] = createSignal(false);
export const [mfaError, setMfaError] = createSignal(null as string | null);
export const [mfaSetup, setMfaSetup] = createSignal(null as {
  secret?: string;
  uri?: string;
  qrDataUrl?: string;
} | null);
export const [showMfaRecoveryPassword, setShowMfaRecoveryPassword] = createSignal(false);
export const [showMfaResetPasswordToggle, setShowMfaResetPasswordToggle] = createSignal(false);
export const [showOauthHint, setShowOauthHint] = createSignal(false);

// ── Form refs ─────────────────────────────────────────────────────────
export let emailInput: HTMLInputElement | null = null;
export let passwordInput: HTMLInputElement | null = null;
export let rememberCheck: HTMLInputElement | null = null;
export let mfaCodeInput: HTMLInputElement | null = null;
export let mfaRecoveryPasswordInput: HTMLInputElement | null = null;

export function setEmailInput(el: HTMLInputElement) { emailInput = el; }
export function setPasswordInput(el: HTMLInputElement) { passwordInput = el; }
export function setRememberCheck(el: HTMLInputElement) { rememberCheck = el; }
export function setMfaCodeInput(el: HTMLInputElement) { mfaCodeInput = el; }
export function setMfaRecoveryPasswordInput(el: HTMLInputElement) { mfaRecoveryPasswordInput = el; }

// ── MFA promise state ─────────────────────────────────────────────────
let pendingMfa: { resolve: (code: string) => void; reject: (err: Error) => void } | null = null;
let mfaSetupUri = '';

// ── Access token ──────────────────────────────────────────────────────
export let accessToken: string | null = null;

// ── Query params ──────────────────────────────────────────────────────
const params = new URLSearchParams(window.location.search);
const code = params.get('code');
let ownerInviteToken = params.get('owner_invite') || '';
if (code) {
  try {
    if (!ownerInviteToken)
      ownerInviteToken = sessionStorage.getItem('platform_owner_invite_token') || '';
  } catch { /* noop */ }
}

// ── Invite resolution ─────────────────────────────────────────────────
export const [inviteInfo, setInviteInfo] = createSignal<{
  invite_email: string;
  inviter_email: string | null;
  role: string;
  tenant_names: string[];
  user_exists: boolean;
} | null>(null);
export const [inviteLoading, setInviteLoading] = createSignal(false);

if (ownerInviteToken) {
  setInviteLoading(true);
  fetch(serverUrl + '/platform/admin/invites/resolve?token=' + encodeURIComponent(ownerInviteToken))
    .then((r) => r.ok ? r.json() : null)
    .then((data: any) => {
      if (data) {
        setInviteInfo(data);
        // Auto-fill email after DOM is ready
        requestAnimationFrame(() => {
          if (emailInput) emailInput.value = data.invite_email;
        });
      }
    })
    .catch(() => { /* resolve failed — show normal login */ })
    .finally(() => setInviteLoading(false));
}

export { ownerInviteToken };

// ── Helpers ───────────────────────────────────────────────────────────

import { randomState, generatePKCE, decodeJwtClaims, copyText } from '../../core/utils';

function formatRetryAfter(seconds: number): string {
  const sec = Number(seconds || 0);
  if (!sec || sec < 1) return 'Try again in a moment.';
  const minutes = Math.ceil(sec / 60);
  if (minutes <= 1) return 'Too many attempts. Try again in about 1 minute.';
  return 'Too many attempts. Try again in about ' + minutes + ' minutes.';
}

function isOauthConfigError(detail: string): boolean {
  const text = String(detail || '').toLowerCase();
  return (
    text.indexOf('redirect_uri') !== -1 ||
    text.indexOf('redirect uri') !== -1 ||
    text.indexOf('redirect_uri_mismatch') !== -1 ||
    text.indexOf('tenant_not_found') !== -1 ||
    text.indexOf("slug 'auth'") !== -1
  );
}

// Re-export for consumers that import copyText from this module
export { copyText };

// ── API helper ────────────────────────────────────────────────────────
// Mid-auth helper: this store manages its own token lifecycle during
// login/MFA flows. Cannot use createApi because (a) pre-auth calls have
// no token, (b) MFA calls use a partially-issued token, and (c) there
// is no createAuth instance here — the store IS the auth orchestrator.

function apiJson(
  path: string,
  opts: { method?: string; body?: Record<string, any>; headers?: Record<string, string> } = {},
): Promise<{ ok: boolean; status: number; data: Record<string, any> }> {
  const headers: Record<string, string> = { ...(opts.headers || {}) };
  if (accessToken) headers['Authorization'] = 'Bearer ' + accessToken;
  if (!headers['Content-Type'] && opts.body) headers['Content-Type'] = 'application/json';

  return fetch(serverUrl + path, {
    method: opts.method || 'GET',
    headers,
    credentials: 'include',
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  })
    .then((response) =>
      response
        .json()
        .then((data: any) => ({ ok: response.ok, status: response.status, data: data || {} }))
        .catch(() => ({ ok: response.ok, status: response.status, data: {} as any })),
    );
}

// ── Remember email ────────────────────────────────────────────────────

function loadRememberedEmail() {
  try {
    const remembered = localStorage.getItem('platform_login_email') || '';
    if (remembered && emailInput) {
      emailInput.value = remembered;
      if (rememberCheck) rememberCheck.checked = true;
    }
  } catch { /* noop */ }
}

export function persistRememberedEmail() {
  if (!emailInput || !rememberCheck) return;
  try {
    if (rememberCheck.checked) {
      localStorage.setItem('platform_login_email', (emailInput.value || '').trim());
    } else {
      localStorage.removeItem('platform_login_email');
    }
  } catch { /* noop */ }
}

// ── MFA ───────────────────────────────────────────────────────────────

export function closeMfaPanel() {
  setShowMfa(false);
  if (mfaCodeInput) mfaCodeInput.value = '';
  setShowMfaRecoveryPassword(false);
  setMfaError(null);
  setMfaSetup(null);
}

function openMfaPanel(step: Record<string, any>): Promise<string> {
  if (pendingMfa && pendingMfa.reject) {
    pendingMfa.reject(new Error('mfa_cancelled'));
  }

  mfaSetupUri = step.uri ? String(step.uri) : '';

  if (step.secret) {
    setMfaSetup({
      secret: step.secret,
      uri: step.uri || '',
      qrDataUrl: step.qrDataUrl || '',
    });
  } else {
    setMfaSetup(null);
  }

  setMfaError(null);
  setShowMfaRecoveryPassword(false);
  setShowMfa(true);

  setTimeout(() => {
    if (mfaCodeInput) {
      mfaCodeInput.focus();
      mfaCodeInput.select();
    }
  }, 0);

  return new Promise<string>((resolve, reject) => {
    pendingMfa = { resolve, reject };
  });
}

function verifyMfaCodeWithApi(codeValue: string) {
  return apiJson('/t/platform/auth/mfa/verify', {
    method: 'POST',
    body: { code: String(codeValue).trim() },
  }).then((verifyRes) => {
    if (!verifyRes.ok) {
      const err: any = new Error(
        verifyRes.data?.error || 'mfa_verify_failed',
      );
      err.retryAfter = verifyRes.data?.retry_after
        ? Number(verifyRes.data.retry_after)
        : 0;
      throw err;
    }
    accessToken = verifyRes.data.access_token;
    return true;
  });
}

function startMfaResetSetup(setupPayload: Record<string, any>) {
  return apiJson('/t/platform/auth/mfa/setup', {
    method: 'POST',
    body: setupPayload,
  })
    .then((res) => {
      if (!res.ok) throw new Error(res.data?.error || 'mfa_setup_failed');
      if (!res.data?.secret) throw new Error('mfa_setup_failed');
      setInfo('MFA reset started. Add the new key in Duo/Authenticator and enter a fresh code.');
      setShowMfaRecoveryPassword(false);
      return openMfaPanel({
        needsCode: true,
        secret: res.data.secret,
        uri: res.data.otpauth_uri || '',
        qrDataUrl: res.data.otpauth_qr_data_url || '',
        recoveryRequired: true,
      })
        .then((codeValue) => verifyMfaCodeWithApi(codeValue))
        .then(() => {
          closeMfaPanel();
          routeAfterAuth();
        });
    })
    .catch((err: any) => {
      if (!err?.message) {
        setMfaError('Could not reset MFA. Please retry.');
        return;
      }
      const msg = err.message;
      if (msg === 'invalid_credentials') {
        setMfaError('Incorrect account password.');
      } else if (msg === 'mfa_recovery_password_unavailable') {
        setMfaError('This account does not support password-based MFA recovery.');
      } else if (msg === 'mfa_sso_reauth_required') {
        setMfaError('Your SSO session is too old. Sign in again with Google/GitHub, then retry reset.');
      } else if (msg === 'mfa_sso_recovery_not_allowed') {
        setMfaError('SSO recovery is only available for owner SSO sessions.');
      } else if (msg === 'mfa_locked') {
        setMfaError(formatRetryAfter(err.retryAfter));
      } else if (msg === 'mfa_cancelled' || msg === 'mfa_code_required') {
        setMfaError('MFA reset cancelled.');
      } else {
        setMfaError('Could not reset MFA. Please retry.');
      }
    });
}

function ensureOwnerMfaVerified(): Promise<boolean> {
  return apiJson('/t/platform/auth/mfa/status')
    .then((statusRes) => {
      if (!statusRes.ok) throw new Error('mfa_status_failed');
      const status = statusRes.data || {};

      if (status.mfa_verified) return { verified: true } as any;

      const authMethod = String(status.auth_method || '').toLowerCase();
      if (authMethod !== 'sso') throw new Error('owner_sso_required');

      if (status.mfa_enabled && !status.recovery_required) {
        return { needsCode: true };
      }

      return apiJson('/t/platform/auth/mfa/setup', {
        method: 'POST',
        body: { rotate: false },
      }).then((setupRes) => {
        if (!setupRes.ok) throw new Error(setupRes.data?.error || 'mfa_setup_failed');
        return {
          needsCode: true,
          secret: setupRes.data?.secret || '',
          uri: setupRes.data?.otpauth_uri || '',
          qrDataUrl: setupRes.data?.otpauth_qr_data_url || '',
          recoveryRequired: !!(setupRes.data?.recovered),
        };
      });
    })
    .then((step: any) => {
      if (step?.verified) return true;
      return openMfaPanel(step)
        .then((codeValue) => verifyMfaCodeWithApi(codeValue))
        .then(() => {
          closeMfaPanel();
          return true;
        });
    });
}

export function completeMfaFromInline() {
  setMfaError(null);
  const codeValue = String(mfaCodeInput?.value || '').trim();

  if (!/^[0-9]{6}$/.test(codeValue)) {
    setMfaError('Enter the 6-digit code from your authenticator app.');
    return;
  }

  if (pendingMfa?.resolve) {
    const resolver = pendingMfa.resolve;
    pendingMfa = null;
    resolver(codeValue);
    return;
  }

  verifyMfaCodeWithApi(codeValue)
    .then(() => {
      closeMfaPanel();
      routeAfterAuth();
    })
    .catch((err: any) => {
      if (err?.message === 'mfa_locked') {
        setMfaError(formatRetryAfter(err.retryAfter));
        return;
      }
      setMfaError('Invalid MFA code. Check your authenticator and retry.');
    });
}

export function cancelInlineMfa() {
  if (pendingMfa?.reject) {
    const rejecter = pendingMfa.reject;
    pendingMfa = null;
    rejecter(new Error('mfa_code_required'));
  }
  closeMfaPanel();
}

export function beginMfaRecoveryFlow() {
  setError(null);
  setInfo(null);
  setMfaError(null);

  if (!accessToken) {
    setError('Sign in first, then use MFA recovery.');
    return;
  }
  const claims = decodeJwtClaims(accessToken) || {};
  const authMethod = String(claims.auth_method || '').toLowerCase();
  setShowMfa(true);

  if (authMethod === 'sso') {
    setShowMfaRecoveryPassword(false);
    setInfo('Resetting MFA using your current SSO session (Google/GitHub OAuth).');
    startMfaResetSetup({ rotate: true, recovery_via_sso: true });
    return;
  }

  setShowMfaRecoveryPassword(true);
  setInfo('If you lost your authenticator device, enter your account password to reset MFA setup.');
  if (mfaRecoveryPasswordInput) mfaRecoveryPasswordInput.focus();
}

export function resetMfaUsingPassword() {
  setError(null);
  setInfo(null);
  setMfaError(null);
  if (!accessToken) {
    setMfaError('Sign in first, then retry MFA reset.');
    return;
  }
  const recoveryPassword = String(mfaRecoveryPasswordInput?.value || '').trim();
  if (!recoveryPassword) {
    setMfaError('Enter your account password to reset MFA.');
    if (mfaRecoveryPasswordInput) mfaRecoveryPasswordInput.focus();
    return;
  }
  startMfaResetSetup({ rotate: true, recovery_password: recoveryPassword });
}

export function getMfaSetupUri(): string { return mfaSetupUri; }

// ── Route after auth ──────────────────────────────────────────────────

function routeAfterAuth() {
  if (ownerInviteToken) {
    // Post-auth: uses accessToken set during login flow.
    // Uses raw fetch (not createApi) because this store has no createAuth
    // instance — it manages its own token as the auth orchestrator.
    fetch(serverUrl + '/platform/admin/invites/accept', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer ' + accessToken,
      },
      credentials: 'include',
      body: JSON.stringify({ token: ownerInviteToken }),
    })
      .then((response) =>
        response.json().then((data: any) => ({ ok: response.ok, data: data || {} })),
      )
      .then((result) => {
        if (!result.ok) {
          throw new Error(result.data?.message || 'invite_accept_failed');
        }
        ownerInviteToken = '';
        try { sessionStorage.removeItem('platform_owner_invite_token'); } catch { /* noop */ }
        // Pre-auth: refresh token via httpOnly cookie to get updated claims
        return fetch(serverUrl + '/t/platform/auth/token', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          credentials: 'include',
          body: JSON.stringify({ grant_type: 'refresh_token' }),
        })
          .then((r) => (r.ok ? r.json() : null))
          .then((payload: any) => {
            if (payload?.access_token) accessToken = payload.access_token;
            routeAfterAuth();
          });
      })
      .catch((error: any) => {
        setError(error?.message || 'Could not accept owner invite.');
      });
    return;
  }

  const claims = decodeJwtClaims(accessToken) || {};
  const roles: string[] = Array.isArray(claims.roles) ? claims.roles : [];

  if (roles.indexOf('platform_owner') !== -1) {
    const isLocal =
      window.location.hostname === 'localhost' ||
      window.location.hostname === '127.0.0.1';
    if (!isLocal && String(claims.auth_method || '') !== 'sso') {
      setError('Owner access requires Google/GitHub OAuth sign-in.');
      return;
    }
    if (isLocal) {
      window.location.href = '/platform/admin/dashboard';
      return;
    }
    ensureOwnerMfaVerified()
      .then(() => {
        window.location.href = '/platform/admin/dashboard';
      })
      .catch((error: any) => {
        const message = error?.message || 'mfa_verify_failed';
        if (message === 'owner_auth_method_required' || message === 'owner_sso_required') {
          setError('Owner access requires Google/GitHub OAuth sign-in before MFA.');
        } else if (message === 'mfa_code_required' || message === 'mfa_cancelled') {
          setError('MFA verification is required to continue.');
        } else if (message === 'mfa_locked') {
          setError(formatRetryAfter(error.retryAfter));
        } else if (message === 'mfa_recovery_required') {
          setError('MFA secret changed after deploy. Use Reset Setup to enroll a new authenticator key.');
          setShowMfa(true);
        } else if (message === 'invalid_mfa_code' || message === 'mfa_verify_failed') {
          setError('Invalid MFA code. Check your authenticator time and try again.');
          setShowMfa(true);
        } else {
          setError('Could not verify owner MFA. Please try again.');
        }
      });
    return;
  }

  // Regular platform users always land in the console now.
  // Tenant creation happens inside the console instead of routing
  // signed-in users back through the onboarding/register surface.
  window.location.href = '/platform/console';
}

// ── OAuth ─────────────────────────────────────────────────────────────

export function startOAuth(provider: 'google' | 'github') {
  setError(null);
  setInfo(null);
  setBusy(provider === 'google' ? 'oauth-google' : 'oauth-github');

  generatePKCE()
    .then((pkce) => {
      try {
        sessionStorage.setItem('platform_pkce_verifier', pkce.verifier);
        sessionStorage.setItem('platform_owner_invite_token', ownerInviteToken || '');
      } catch { /* noop */ }

      const state = randomState();
      const url =
        serverUrl +
        '/t/platform/auth/authorize' +
        '?provider=' + encodeURIComponent(provider) +
        '&client_id=' + encodeURIComponent(oauthClientId) +
        '&redirect_uri=' + encodeURIComponent(oauthRedirectUri) +
        '&response_type=code' +
        '&code_challenge=' + encodeURIComponent(pkce.challenge) +
        '&code_challenge_method=S256' +
        '&state=' + encodeURIComponent(state);

      window.location.href = url;
    })
    .catch(() => {
      setBusy(null);
      setError('Failed to start OAuth flow. Please refresh and retry.');
    });
}

// ── Email login ───────────────────────────────────────────────────────

export function loginWithEmail(e?: Event) {
  if (e) e.preventDefault();
  setError(null);
  setInfo(null);
  persistRememberedEmail();
  setBusy('login');

  // Pre-auth: email/password login — no token available yet
  fetch(serverUrl + '/t/platform/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({
      email: emailInput?.value || '',
      password: passwordInput?.value || '',
    }),
  })
    .then((response) =>
      response.json().then((data: any) => ({ ok: response.ok, data: data || {} })),
    )
    .then((result) => {
      setBusy(null);
      if (!result.ok) {
        setError(result.data.message || 'Login failed');
        return;
      }
      accessToken = result.data.access_token;
      routeAfterAuth();
    })
    .catch(() => {
      setBusy(null);
      setError('Network error. Please try again.');
    });
}

// ── Forgot password ───────────────────────────────────────────────────

export function requestPasswordReset() {
  setError(null);
  setInfo(null);
  const email = (emailInput?.value || '').trim();
  if (!email) {
    setError('Enter your email first, then click Forgot password.');
    emailInput?.focus();
    return;
  }
  // Pre-auth: password reset request — no token available yet
  fetch(serverUrl + '/t/platform/auth/forgot-password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({ email }),
  })
    .then(() => setInfo('If that email exists, a password reset link has been sent.'))
    .catch(() => setInfo('If that email exists, a password reset link has been sent.'));
}

// ── Silent refresh ────────────────────────────────────────────────────

function trySilentRefresh() {
  // Pre-auth: silent refresh via httpOnly cookie — no token available yet
  return fetch(serverUrl + '/t/platform/auth/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({ grant_type: 'refresh_token' }),
  })
    .then((response) => {
      if (!response.ok) throw new Error('not_authenticated');
      return response.json();
    })
    .then((payload: any) => {
      accessToken = payload.access_token;
      routeAfterAuth();
    })
    .catch(() => null);
}

// ── OAuth callback ────────────────────────────────────────────────────

function handleOAuthCallback() {
  setBusy('oauth-google');
  let verifier: string | null = null;
  try { verifier = sessionStorage.getItem('platform_pkce_verifier'); } catch { /* noop */ }
  try { sessionStorage.removeItem('platform_pkce_verifier'); } catch { /* noop */ }

  // Pre-auth: OAuth authorization_code exchange — no token available yet
  fetch(serverUrl + '/t/platform/auth/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({
      grant_type: 'authorization_code',
      code,
      code_verifier: verifier || '',
      redirect_uri: oauthRedirectUri,
      client_id: oauthClientId,
    }),
  })
    .then((response) =>
      response.json().then((data: any) => ({ ok: response.ok, data: data || {} })),
    )
    .then((result) => {
      if (!result.ok) {
        const detail = result.data?.message || result.data?.error || 'oauth_failed';
        throw new Error(detail);
      }
      accessToken = result.data.access_token;
      history.replaceState(null, '', '/platform/login');
      routeAfterAuth();
    })
    .catch((error: any) => {
      const detail = error?.message || 'unknown_error';
      setError('OAuth sign-in failed: ' + detail);
      if (isOauthConfigError(detail)) {
        setShowOauthHint(true);
        setInfo('OAuth callback mismatch detected. Confirm provider callback URLs.');
      }
    })
    .finally(() => {
      setBusy(null);
      try { sessionStorage.removeItem('platform_owner_invite_token'); } catch { /* noop */ }
    });
}

// ── Init (called from app.ts) ─────────────────────────────────────────

export function init() {
  setTimeout(() => {
    loadRememberedEmail();

    const providerErr = params.get('error') || params.get('error_description');
    if (providerErr) {
      setError('OAuth provider error: ' + providerErr);
      if (isOauthConfigError(providerErr)) {
        setShowOauthHint(true);
        setInfo('OAuth callback mismatch detected. Confirm provider callback URLs.');
      }
    }

    if (code) {
      handleOAuthCallback();
    } else {
      trySilentRefresh();
    }
  }, 0);
}
