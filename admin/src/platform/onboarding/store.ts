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

// ── PKCE helpers ────────────────────────────────────────────────────────

function randomState(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

async function generatePKCE(): Promise<{ verifier: string; challenge: string }> {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  const verifier = btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
  const hash = await crypto.subtle.digest(
    'SHA-256',
    new TextEncoder().encode(verifier),
  );
  const challenge = btoa(String.fromCharCode(...new Uint8Array(hash)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
  return { verifier, challenge };
}

// ── JWT decode ──────────────────────────────────────────────────────────

function decodeJwtClaims(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split('.');
    if (parts.length < 2) return null;
    let payload = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    while (payload.length % 4 !== 0) payload += '=';
    return JSON.parse(atob(payload));
  } catch {
    return null;
  }
}

// ── Post-auth routing ───────────────────────────────────────────────────

async function autoCreateTenant(accessToken: string): Promise<void> {
  const claims = decodeJwtClaims(accessToken) || {};
  const emailAddr = (claims.email as string) || (claims.sub as string) || '';
  const userPart = emailAddr.split('@')[0] || 'my-app';
  let slug = userPart
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 26);
  const rand = Math.random().toString(36).slice(2, 6);
  slug = slug + '-' + rand;
  const name = userPart.charAt(0).toUpperCase() + userPart.slice(1) + ' App';

  try {
    await fetch(serverUrl + '/platform/self-serve/tenants', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: 'Bearer ' + accessToken,
      },
      body: JSON.stringify({ name, slug }),
    });
  } catch {
    // Best-effort — redirect to console regardless
  }
  window.location.href = '/platform/console';
}

async function routeAfterAuth(accessToken: string): Promise<void> {
  try {
    const res = await fetch(serverUrl + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
    });
    if (!res.ok) throw new Error('tenant_list_failed');
    const data = await res.json();
    if (data && Array.isArray(data.tenants) && data.tenants.length > 0) {
      window.location.href = '/platform/console';
    } else {
      await autoCreateTenant(accessToken);
    }
  } catch {
    await autoCreateTenant(accessToken);
  }
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
    await routeAfterAuth(data.access_token);
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
        routeAfterAuth(res.data.access_token);
      })
      .catch(() => {
        setInfo(null);
        setError('OAuth sign-in failed. Please try again.');
      });
  } else {
    // Silent refresh — check if already authenticated
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
        routeAfterAuth(d.access_token);
      })
      .catch(() => {
        // Not authenticated — stay on registration form
      });
  }
}
