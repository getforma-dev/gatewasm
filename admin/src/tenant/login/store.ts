/**
 * Tenant Login Store — shared reactive state for TenantLoginCard island
 *
 * All signals, API helpers, and auth flow logic live here.
 * Reads tenant config from window.__TENANT__ (injected by Rust handler).
 */
import { createSignal } from 'formajs';
import type { TenantConfig } from '../../core/types';

// ── Tenant config (injected by Rust handler into window.__TENANT__) ──────
const config: TenantConfig & { hosted_redirect_uri?: string; authorization_request?: string } =
  (window as any).__TENANT__ || {};

export const slug = config.slug || '';
export const serverUrl = config.server_url || `/t/${slug}`;
export const providers: string[] = config.providers || [];
export const appName = config.name || slug;
export const primaryColor = config.primary_color || '#c4a265';
export const logoUrl = config.logo_url || null;
export const signupMode = config.signup_mode || 'public';
export const loginRedirectUrl = config.login_redirect_url || '/';
const hostedRedirectUri = config.hosted_redirect_uri || '';
const authorizationRequest = config.authorization_request || '';
const clientId = config.client_id || `${slug}-hosted`;

// ── Provider flags ──────────────────────────────────────────────────────
export const hasGoogle = providers.indexOf('google') >= 0;
export const hasGithub = providers.indexOf('github') >= 0;
export const hasOAuth = hasGoogle || hasGithub;
export const hasEmail = providers.indexOf('email') >= 0;

// ── Invite token ────────────────────────────────────────────────────────
const inviteStorageKey = 'tenant_invite_token_' + slug;
const urlParams = new URLSearchParams(window.location.search);
let inviteToken = urlParams.get('invite_token') || '';
if (!inviteToken) {
  try { inviteToken = sessionStorage.getItem(inviteStorageKey) || ''; } catch { /* noop */ }
}
if (inviteToken) {
  try { sessionStorage.setItem(inviteStorageKey, inviteToken); } catch { /* noop */ }
}

const resetToken = urlParams.get('reset_token') || '';

// ── Redirect query param (for post-auth navigation) ────────────────────
const redirectParam = urlParams.get('redirect') || '';
// Security: only allow same-tenant redirects (starts with /t/{slug}/)
const safeRedirect = redirectParam.startsWith('/t/' + slug + '/') ? redirectParam : '';

export const inviteOnly = signupMode === 'invite_only';
export const allowPublicSignup = !authorizationRequest && (!inviteOnly || !!inviteToken);

// ── Reactive state ──────────────────────────────────────────────────────
export type LoginMode = 'login' | 'register' | 'forgot' | 'forgot-sent' | 'reset' | 'reset-done';

export const [email, setEmail] = createSignal('');
export const [password, setPassword] = createSignal('');
export const [name, setName] = createSignal('');
export const [error, setError] = createSignal<string | null>(null);
export const [success, setSuccess] = createSignal<string | null>(null);
export const [loading, setLoading] = createSignal(false);
const modeParam = urlParams.get('mode');
const initialMode: LoginMode = resetToken ? 'reset' : (modeParam === 'register' ? 'register' : 'login');
export const [mode, setMode] = createSignal<LoginMode>(initialMode);
export const [showPassword, setShowPassword] = createSignal(false);
export const [rememberEmail, setRememberEmail] = createSignal(false);

// ── Helpers ─────────────────────────────────────────────────────────────

import { randomState, generatePKCE } from '../../core/utils';

// ── Remember email ──────────────────────────────────────────────────────

function loadRememberedEmail() {
  try {
    const saved = localStorage.getItem('tenant_login_email_' + slug);
    if (saved) {
      setEmail(saved);
      setRememberEmail(true);
    }
  } catch { /* noop */ }
}

export function persistEmail() {
  try {
    if (rememberEmail()) {
      localStorage.setItem('tenant_login_email_' + slug, email());
    } else {
      localStorage.removeItem('tenant_login_email_' + slug);
    }
  } catch { /* noop */ }
}

// ── Apply primary color ─────────────────────────────────────────────────

function applyBranding() {
  document.documentElement.style.setProperty('--accent', primaryColor);
  document.documentElement.style.setProperty('--primary', primaryColor);
  document.documentElement.style.setProperty(
    '--primary-hover',
    `color-mix(in srgb, ${primaryColor} 85%, black)`,
  );
}

// ── OAuth start ─────────────────────────────────────────────────────────

export function startOAuth(provider: string) {
  setError(null);
  setSuccess(null);
  generatePKCE().then((pkce) => {
    const state = randomState();
    try { sessionStorage.setItem('gatewasm_code_verifier', pkce.verifier); } catch { /* noop */ }
    window.location.href =
      serverUrl + '/auth/authorize' +
      '?provider=' + encodeURIComponent(provider) +
      '&client_id=' + encodeURIComponent(clientId) +
      '&redirect_uri=' + encodeURIComponent(hostedRedirectUri) +
      '&response_type=code' +
      '&code_challenge=' + encodeURIComponent(pkce.challenge) +
      '&code_challenge_method=S256' +
      '&state=' + encodeURIComponent(state) +
      (inviteToken ? '&invite_token=' + encodeURIComponent(inviteToken) : '');
  });
}

// ── OAuth callback ──────────────────────────────────────────────────────

function handleOAuthCallback() {
  const cbCode = urlParams.get('code');
  const cbState = urlParams.get('state');
  if (!cbCode || !cbState) return false;

  let verifier = '';
  try { verifier = sessionStorage.getItem('gatewasm_code_verifier') || ''; } catch { /* noop */ }
  try { sessionStorage.removeItem('gatewasm_code_verifier'); } catch { /* noop */ }

  if (!verifier) {
    window.history.replaceState(null, '', window.location.pathname);
    return false;
  }

  setLoading(true);
  // Pre-auth: OAuth authorization_code exchange — no token available yet
  fetch(serverUrl + '/auth/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({
      grant_type: 'authorization_code',
      code: cbCode,
      code_verifier: verifier,
      client_id: clientId,
      redirect_uri: hostedRedirectUri,
    }),
  })
    .then((res) => {
      if (res.ok) {
        window.location.href = safeRedirect || loginRedirectUrl;
      } else {
        window.history.replaceState(null, '', window.location.pathname);
        setLoading(false);
        setError('OAuth sign-in failed. Please try again.');
      }
    })
    .catch(() => {
      window.history.replaceState(null, '', window.location.pathname);
      setLoading(false);
      setError('Network error during OAuth. Please try again.');
    });

  return true;
}

// ── Form submit ─────────────────────────────────────────────────────────

export function submitForm(e: Event) {
  e.preventDefault();
  setError(null);
  setSuccess(null);
  setLoading(true);
  persistEmail();

  const currentMode = mode();
  const isRegister = currentMode === 'register';
  const isReset = currentMode === 'reset';

  if (isRegister && !allowPublicSignup) {
    setLoading(false);
    setError('This tenant is invite-only. Use an invitation link to sign up.');
    return;
  }

  const url = serverUrl + (
    isReset ? '/auth/reset-password' :
    isRegister ? '/auth/register' :
    '/auth/login'
  );

  const body: Record<string, string> = isReset
    ? { token: resetToken, new_password: password() }
    : isRegister
      ? { email: email(), password: password(), name: name() }
      : { email: email(), password: password() };

  if (!isReset && !isRegister && authorizationRequest) body.authorization_request = authorizationRequest;

  if (!isReset && inviteToken) {
    body.invite_token = inviteToken;
  }

  // Pre-auth: login, register, or reset-password — no token available yet
  fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    credentials: 'include',
  })
    .then((res) =>
      res.json().then((data: any) => ({ ok: res.ok, status: res.status, data })),
    )
    .then((result) => {
      setLoading(false);
      if (!result.ok) {
        setError(result.data?.message || 'Authentication failed');
        return;
      }
      if (isReset) {
        setPassword('');
        setMode('reset-done');
        clearResetTokenFromUrl();
        return;
      }
      if (inviteToken) {
        try { sessionStorage.removeItem(inviteStorageKey); } catch { /* noop */ }
      }
      if (authorizationRequest) {
        if (!result.data?.redirect_uri) { setError('Sign-in request expired. Start sign-in again.'); return; }
        window.location.href = result.data.redirect_uri;
        return;
      }
      window.location.href = safeRedirect || loginRedirectUrl;
    })
    .catch((err: any) => {
      setLoading(false);
      setError(err?.message || 'Network error');
    });
}

function clearResetTokenFromUrl() {
  if (!window.history?.replaceState) return;
  const params = new URLSearchParams(window.location.search);
  params.delete('reset_token');
  const next = window.location.pathname + (params.toString() ? ('?' + params.toString()) : '');
  window.history.replaceState(null, '', next);
}

// ── Forgot password ─────────────────────────────────────────────────────

export function sendForgotPassword() {
  if (!email()) {
    setError('Enter your email first');
    return;
  }
  setLoading(true);
  setError(null);
  // Pre-auth: forgot-password request — no token available yet
  fetch(serverUrl + '/auth/forgot-password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email: email() }),
  })
    .then(() => { setLoading(false); setError(null); setMode('forgot-sent'); })
    .catch(() => { setLoading(false); setError(null); setMode('forgot-sent'); });
}

// ── bfcache fallback ────────────────────────────────────────────────────

function setupBfcacheListener() {
  window.addEventListener('pageshow', (evt: PageTransitionEvent) => {
    if (evt.persisted) {
      // Pre-auth: bfcache refresh via httpOnly cookie — no token available
      fetch(serverUrl + '/auth/token', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({ grant_type: 'refresh_token' }),
      })
        .then((r) => { if (r.ok) window.location.href = safeRedirect || loginRedirectUrl; })
        .catch(() => { /* noop */ });
    }
  });
}

// ── Init (called from app.ts) ───────────────────────────────────────────

export function init() {
  // Clean up legacy sessionStorage tokens
  try {
    sessionStorage.removeItem('gatewasm_access_token');
    sessionStorage.removeItem('gatewasm_refresh_token');
  } catch { /* noop */ }

  applyBranding();
  loadRememberedEmail();
  if (!authorizationRequest) { setupBfcacheListener(); handleOAuthCallback(); }
}
