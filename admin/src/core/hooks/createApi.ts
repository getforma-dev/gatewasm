import type { Auth } from './createAuth';

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(message: string, status: number, code: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export function createApi(auth: Auth) {
  async function request<T = Record<string, unknown>>(
    path: string,
    opts?: { method?: string; body?: unknown; headers?: Record<string, string> },
  ): Promise<T> {
    const method = opts?.method ?? 'GET';
    const headers: Record<string, string> = {
      ...(opts?.headers || {}),
    };
    if (auth.accessToken()) {
      headers['Authorization'] = 'Bearer ' + auth.accessToken();
    }
    if (opts?.body) {
      headers['Content-Type'] = 'application/json';
    }

    let res = await fetch(auth.baseUrl + path, {
      method,
      headers,
      credentials: 'include',
      body: opts?.body ? JSON.stringify(opts.body) : undefined,
    });

    // Auto-retry on 401
    if (res.status === 401) {
      await auth.silentRefresh();
      headers['Authorization'] = 'Bearer ' + auth.accessToken();
      res = await fetch(auth.baseUrl + path, {
        method,
        headers,
        credentials: 'include',
        body: opts?.body ? JSON.stringify(opts.body) : undefined,
      });
    }

    if (!res.ok) {
      const data = await res.json().catch(() => ({}));
      throw new ApiError(
        data.message || data.error || 'Request failed',
        res.status,
        data.error || 'unknown_error',
      );
    }

    return res.json();
  }

  function get<T = Record<string, unknown>>(path: string, params?: Record<string, unknown>): Promise<T> {
    const qs = queryFromObject(params || {});
    return request<T>(path + (qs ? '?' + qs : ''));
  }

  function post<T = Record<string, unknown>>(path: string, body?: unknown): Promise<T> {
    return request<T>(path, { method: 'POST', body });
  }

  function patch<T = Record<string, unknown>>(path: string, body?: unknown): Promise<T> {
    return request<T>(path, { method: 'PATCH', body });
  }

  function put<T = Record<string, unknown>>(path: string, body?: unknown): Promise<T> {
    return request<T>(path, { method: 'PUT', body });
  }

  function del<T = Record<string, unknown>>(path: string): Promise<T> {
    return request<T>(path, { method: 'DELETE' });
  }

  return { request, get, post, patch, put, del };
}

export function queryFromObject(obj: Record<string, unknown>): string {
  const parts: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    if (v == null || v === '') continue;
    if (v instanceof Date) {
      parts.push(k + '=' + encodeURIComponent(v.toISOString()));
    } else {
      parts.push(k + '=' + encodeURIComponent(String(v)));
    }
  }
  return parts.join('&');
}

export type Api = ReturnType<typeof createApi>;
