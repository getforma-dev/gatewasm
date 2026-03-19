import { createSignal } from 'formajs';

export interface AuthOptions {
  serverUrl: string;
  tenant: string;
  autoRefreshMs?: number;
}

export function createAuth(opts: AuthOptions) {
  const { serverUrl, tenant, autoRefreshMs = 8 * 60 * 1000 } = opts;
  const baseUrl = serverUrl + '/t/' + tenant;
  const [accessToken, setAccessToken] = createSignal<string | null>(null);

  async function silentRefresh(): Promise<string> {
    const res = await fetch(baseUrl + '/auth/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ grant_type: 'refresh_token' }),
    });
    if (!res.ok) throw new Error('refresh_failed');
    const data = await res.json();
    setAccessToken(data.access_token);
    return data.access_token;
  }

  async function ensureRole(role: string): Promise<void> {
    const res = await fetch(baseUrl + '/auth/me', {
      headers: { Authorization: 'Bearer ' + accessToken() },
      credentials: 'include',
    });
    if (!res.ok) throw new Error('auth_check_failed');
    const me = await res.json();
    const roles: string[] = me.roles || [];
    if (!roles.includes(role)) throw new Error('missing_role_' + role);
  }

  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  function startAutoRefresh(): void {
    stopAutoRefresh();
    refreshTimer = setInterval(() => {
      silentRefresh().catch(() => {});
    }, autoRefreshMs);
  }

  function stopAutoRefresh(): void {
    if (refreshTimer) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  }

  async function logout(): Promise<void> {
    stopAutoRefresh();
    setAccessToken(null);
    fetch(baseUrl + '/auth/logout', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({}),
    }).catch(() => {});
  }

  return {
    accessToken,
    setAccessToken,
    silentRefresh,
    ensureRole,
    startAutoRefresh,
    stopAutoRefresh,
    logout,
    baseUrl,
  };
}

export type Auth = ReturnType<typeof createAuth>;
