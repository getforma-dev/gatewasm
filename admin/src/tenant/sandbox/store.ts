import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';

const config = (window as any).__TENANT__ || {};
const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const slug: string = config.slug || '';

const auth = createAuth({ serverUrl, tenant: slug });

export interface IntrospectData {
  user: { id: string; email: string; name: string | null; roles: string[]; created_at: string };
  session: { id: string; created_at: string; expires_at: string } | null;
  tenant: { id: string; slug: string };
  token: { issuer: string; audience: string; issued_at: number; expires_at: number; algorithm: string };
}

export const [data, setData] = createSignal<IntrospectData | null>(null);
export const [error, setError] = createSignal<string | null>(null);
export const [rawToken, setRawToken] = createSignal<string | null>(null);
export const tenantName: string = config.name || slug;
export { slug, serverUrl };

export async function init(): Promise<void> {
  try {
    const token = await auth.silentRefresh();
    setRawToken(token);

    const res = await fetch(serverUrl + '/t/' + slug + '/auth/introspect', {
      headers: { 'Authorization': 'Bearer ' + token },
      credentials: 'include',
    });
    if (!res.ok) throw new Error('introspect_failed');
    setData(await res.json());
  } catch (e: any) {
    setError(e.message || 'Failed to load auth data');
  }
}

export function logout(): void {
  auth.logout();
  window.location.href = '/t/' + slug + '/login?redirect=/t/' + slug + '/sandbox';
}
