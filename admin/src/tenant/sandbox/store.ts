import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import type { IntrospectData } from '../../core/types';

const config = (window as any).__TENANT__ || {};
const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const slug: string = config.slug || '';

const auth = createAuth({ serverUrl, tenant: slug });
const api = createApi(auth);

export const [data, setData] = createSignal<IntrospectData | null>(null);
export const [error, setError] = createSignal<string | null>(null);
export const [rawToken, setRawToken] = createSignal<string | null>(null);
export const tenantName: string = config.name || slug;
export { slug, serverUrl };

export async function init(): Promise<void> {
  try {
    const token = await auth.silentRefresh();
    setRawToken(token);

    const result = await api.get<IntrospectData>('/auth/introspect');
    setData(result);
  } catch (e: any) {
    setError(e.message || 'Failed to load auth data');
  }
}

export function logout(): void {
  auth.logout();
  window.location.href = '/t/' + slug + '/login?redirect=/t/' + slug + '/sandbox';
}
