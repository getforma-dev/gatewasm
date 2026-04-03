/**
 * Format a timestamp as relative time ("2 minutes ago", "Yesterday").
 */
export function relativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return 'Just now';
  if (diffMin < 60) return diffMin + ' minute' + (diffMin === 1 ? '' : 's') + ' ago';
  if (diffHr < 24) return diffHr + ' hour' + (diffHr === 1 ? '' : 's') + ' ago';
  if (diffDay === 1) return 'Yesterday';
  if (diffDay < 30) return diffDay + ' days ago';
  const months = Math.floor(diffDay / 30);
  if (diffDay < 365) return months + ' month' + (months === 1 ? '' : 's') + ' ago';
  const years = Math.floor(diffDay / 365);
  return years + ' year' + (years === 1 ? '' : 's') + ' ago';
}

/**
 * Format a date as ISO-like string for tooltip display.
 */
export function formatExact(dateStr: string): string {
  const d = new Date(dateStr);
  return d.toISOString().replace('T', ' ').replace(/\.\d{3}Z$/, ' UTC');
}
