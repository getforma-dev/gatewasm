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

export function isTrialExpiring(iso: string | null | undefined): boolean {
  if (!iso) return false;
  const diff = new Date(iso).getTime() - Date.now();
  return diff > 0 && diff < 7 * 24 * 3600 * 1000;
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
