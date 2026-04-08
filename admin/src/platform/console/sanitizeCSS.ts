/**
 * Sanitize operator-provided CSS declarations.
 * Strips dangerous patterns; input is declarations-only (no selectors).
 * Mirrors the server-side sanitizer in onboarding.rs.
 */

const BLOCKED_PATTERNS = [
  'url(', '@import', 'expression(', 'javascript:', '-moz-binding',
  'behavior:', '</style', '<script', '<!--',
];

export function sanitizeCSS(input: string): string {
  let result = input.slice(0, 2000);

  // Strip blocked patterns (case-insensitive)
  for (const pat of BLOCKED_PATTERNS) {
    const regex = new RegExp(pat.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
    result = result.replace(regex, '');
  }

  // Strip angle brackets, backslash escapes, and selector braces
  result = result.replace(/[<>\\{}]/g, '');

  return result.trim();
}
