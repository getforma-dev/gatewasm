/**
 * Onboarding Store — shared reactive state for onboarding islands
 *
 * All signals, helpers, and auth flow logic live here.
 * RegistrationForm island imports from this module.
 */
import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';


// ── Server-injected config ──────────────────────────────────────────────

export const serverUrl: string =
  (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const platformConfig: { google_enabled?: boolean; github_enabled?: boolean; client_id?: string } =
  (window as any).__PLATFORM_CONFIG__ || {};

const oauthClientId = platformConfig.client_id || 'platform-onboarding';
const oauthRedirectUri = serverUrl + '/platform/onboarding';
export const hasGoogle = !!platformConfig.google_enabled;
export const hasGithub = !!platformConfig.github_enabled;
export const hasOAuth = hasGoogle || hasGithub;

// ── Auth hook ───────────────────────────────────────────────────────────

const auth = createAuth({ serverUrl, tenant: 'platform' });
// ── Signals ─────────────────────────────────────────────────────────────

export const [email, setEmail] = createSignal('');
export const [password, setPassword] = createSignal('');
export const [confirm, setConfirm] = createSignal('');
export const [error, setError] = createSignal<string | null>(null);
export const [info, setInfo] = createSignal<string | null>(null);
export const [submitting, setSubmitting] = createSignal(false);
export const [showPassword, setShowPassword] = createSignal(false);
export const [showConfirm, setShowConfirm] = createSignal(false);

// ── Shared helpers ──────────────────────────────────────────────────────

import { randomState, generatePKCE } from '../../core/utils';

// ── Post-auth routing ───────────────────────────────────────────────────

async function routeAfterAuth(): Promise<void> {
  // Preserve owner_invite param so the console can auto-accept after registration
  const params = new URLSearchParams(window.location.search);
  const invite = params.get('owner_invite');
  const target = invite
    ? '/platform/console?owner_invite=' + encodeURIComponent(invite)
    : '/platform/console';
  window.location.href = target;
}

// ── OAuth ───────────────────────────────────────────────────────────────

export async function startOAuth(provider: string): Promise<void> {
  const pkce = await generatePKCE();
  try {
    sessionStorage.setItem('platform_pkce_verifier', pkce.verifier);
  } catch {
    // sessionStorage unavailable — continue anyway
  }
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
}

// ── Registration ────────────────────────────────────────────────────────

export async function handleRegister(e: Event) {
  e.preventDefault();
  setError(null);
  setInfo(null);

  const emailVal = email().trim();
  const passwordVal = password();
  const confirmVal = confirm();

  if (!emailVal) { setError('Email is required.'); return; }
  if (!passwordVal) { setError('Password is required.'); return; }
  if (passwordVal.length < 8) { setError('Password must be at least 8 characters.'); return; }
  if (passwordVal !== confirmVal) { setError('Passwords do not match.'); return; }

  setSubmitting(true);

  try {
    // Pre-auth: registration endpoint — no token available yet
    const res = await fetch(serverUrl + '/t/platform/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ email: emailVal, password: passwordVal }),
    });
    const data = await res.json();
    setSubmitting(false);

    if (!res.ok) {
      setError(data.message || 'Registration failed.');
      return;
    }

    auth.setAccessToken(data.access_token);
    await routeAfterAuth();
  } catch {
    setSubmitting(false);
    setError('Network error. Please try again.');
  }
}

// ── Init (called from app.ts) ───────────────────────────────────────────

export function init() {
  const urlParams = new URLSearchParams(window.location.search);
  const code = urlParams.get('code');

  if (code) {
    let verifier: string | null = null;
    try { verifier = sessionStorage.getItem('platform_pkce_verifier'); } catch { /* */ }
    try { sessionStorage.removeItem('platform_pkce_verifier'); } catch { /* */ }

    setInfo('Completing sign-in...');

    // Pre-auth: OAuth code-for-token exchange — no token available yet
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
      .then((r) => r.json().then((d: any) => ({ ok: r.ok, data: d })))
      .then((res) => {
        setInfo(null);
        if (!res.ok) throw new Error(res.data?.message || 'oauth_failed');
        auth.setAccessToken(res.data.access_token);
        history.replaceState(null, '', '/platform/onboarding');
        routeAfterAuth();
      })
      .catch(() => {
        setInfo(null);
        setError('OAuth sign-in failed. Please try again.');
      });
  } else {
    // Pre-auth: silent refresh via httpOnly cookie — no token available yet
    fetch(serverUrl + '/t/platform/auth/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ grant_type: 'refresh_token' }),
    })
      .then((r) => {
        if (r.ok) return r.json();
        throw new Error('not authenticated');
      })
      .then((d: any) => {
        auth.setAccessToken(d.access_token);
        routeAfterAuth();
      })
      .catch(() => {
        // Not authenticated — stay on registration form
      });
  }
}
