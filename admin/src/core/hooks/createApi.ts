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
  function asRecord(value: unknown): Record<string, unknown> {
    return value && typeof value === 'object' ? value as Record<string, unknown> : {};
  }

  function getStringField(record: Record<string, unknown>, key: string): string | null {
    const value = record[key];
    return typeof value === 'string' ? value : null;
  }

  async function parseJsonResponse<T>(res: Response): Promise<T> {
    if (res.status === 204) {
      return undefined as T;
    }

    const text = await res.text();
    if (!text) {
      return undefined as T;
    }

    return JSON.parse(text) as T;
  }

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
      try {
        await auth.silentRefresh();
      } catch {
        throw new ApiError('Unauthorized', 401, 'unauthorized');
      }
      headers['Authorization'] = 'Bearer ' + auth.accessToken();
      res = await fetch(auth.baseUrl + path, {
        method,
        headers,
        credentials: 'include',
        body: opts?.body ? JSON.stringify(opts.body) : undefined,
      });
    }

    if (!res.ok) {
      const data = asRecord(await parseJsonResponse<unknown>(res).catch(() => null));
      throw new ApiError(
        getStringField(data, 'message') || getStringField(data, 'error') || 'Request failed',
        res.status,
        getStringField(data, 'error') || 'unknown_error',
      );
    }

    return parseJsonResponse<T>(res);
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
