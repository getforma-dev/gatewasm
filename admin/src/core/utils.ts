// admin/src/core/utils.ts

export function money(cents: number | null | undefined): string {
  if (cents == null) return '$0.00';
  return '$' + (cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return '-';
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return '-';
  }
}

export function normalizePillStatus(status: unknown): string {
  return String(status || 'unknown').toLowerCase().replace(/[^a-z]/g, '');
}

export function toCsv(rows: Record<string, unknown>[], columns: { key: string; label: string }[]): string {
  const header = columns.map((c) => c.label).join(',');
  const body = rows
    .map((r) =>
      columns
        .map((c) => {
          const v = r[c.key];
          const s = v == null ? '' : String(v);
          return s.includes(',') || s.includes('"') || s.includes('\n')
            ? '"' + s.replace(/"/g, '""') + '"'
            : s;
        })
        .join(','),
    )
    .join('\n');
  return header + '\n' + body;
}

export function downloadCsv(filename: string, csvText: string): void {
  const blob = new Blob([csvText], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    document.execCommand('copy');
    document.body.removeChild(ta);
  }
}

// ── Auth / crypto helpers ───────────────────────────────────────────────────

export function randomState(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

export async function generatePKCE(): Promise<{ verifier: string; challenge: string }> {
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

export function decodeJwtClaims(token: string | null): Record<string, unknown> | null {
  try {
    const parts = String(token || '').split('.');
    if (parts.length < 2) return null;
    let payload = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    while (payload.length % 4) payload += '=';
    return JSON.parse(atob(payload));
  } catch {
    return null;
  }
}

export function detectEnv(): string {
  const host = typeof window !== 'undefined' ? window.location.hostname : '';
  return (host === 'localhost' || host === '127.0.0.1') ? 'dev' : 'prod';
}

export type ThemeMode = 'dark' | 'light';

const THEME_STORAGE_KEY = 'forma-theme-mode';

function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'dark' || value === 'light';
}

export function getStoredThemeMode(): ThemeMode | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeMode(stored) ? stored : null;
  } catch {
    return null;
  }
}

export function getThemeMode(): ThemeMode {
  if (typeof document !== 'undefined') {
    const applied = document.documentElement.getAttribute('data-theme');
    if (isThemeMode(applied)) {
      return applied;
    }
  }

  return getStoredThemeMode() || 'dark';
}

export function applyThemeMode(theme: ThemeMode, persist = true): ThemeMode {
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', theme);
    document.documentElement.style.colorScheme = theme;
    document.documentElement.style.backgroundColor = 'var(--bg)';

    if (document.body) {
      document.body.style.backgroundColor = 'var(--bg)';
      document.body.style.color = 'var(--text)';
    }
  }

  if (persist && typeof window !== 'undefined') {
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // Ignore storage failures.
    }
  }

  return theme;
}

export function initThemeMode(): ThemeMode {
  return applyThemeMode(getStoredThemeMode() || 'dark', false);
}

export function toggleThemeMode(currentTheme?: ThemeMode): ThemeMode {
  const nextTheme = (currentTheme || getThemeMode()) === 'dark' ? 'light' : 'dark';
  return applyThemeMode(nextTheme);
}
