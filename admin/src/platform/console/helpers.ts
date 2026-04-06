export function userDisplayName(name: string | null | undefined, email: string): string {
  const trimmed = (name || '').trim();
  if (trimmed) return trimmed;
  const local = email.split('@')[0] || email;
  return local.replace(/[._-]+/g, ' ');
}

export function userInitials(name: string | null | undefined, email: string): string {
  const parts = userDisplayName(name, email).split(/\s+/).filter(Boolean);
  if (parts.length === 0) return email.slice(0, 2).toUpperCase();
  return parts.slice(0, 2).map((p) => p[0] || '').join('').toUpperCase() || email.slice(0, 2).toUpperCase();
}
