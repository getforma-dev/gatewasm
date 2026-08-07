/**
 * Sanitize operator-provided CSS declarations for the branding preview.
 * Input is a declaration list only — no selectors, no at-rules.
 *
 * This MUST mirror `sanitize_custom_css` in
 * crates/server/src/handlers/management.rs. If it drifts, the operator sees
 * one thing in the console preview and their users see another in production.
 *
 * Allowlist, not denylist. The previous denylist could not see through CSS
 * escapes — `background:u\72 l(evil)` unescapes to `url(evil)` inside the CSS
 * parser, after any substring check has run. Rejecting `\` outright is what
 * makes the name allowlist below sound: with no escapes available, an
 * identifier can only be spelled literally.
 */

const ALLOWED_PROPERTIES = new Set([
  'background', 'background-color', 'border', 'border-bottom', 'border-color',
  'border-left', 'border-radius', 'border-right', 'border-style', 'border-top',
  'border-width', 'box-shadow', 'color', 'font-family', 'font-size',
  'font-style', 'font-weight', 'gap', 'height', 'letter-spacing', 'line-height',
  'margin', 'margin-bottom', 'margin-left', 'margin-right', 'margin-top',
  'max-height', 'max-width', 'min-height', 'min-width', 'opacity', 'outline',
  'outline-color', 'padding', 'padding-bottom', 'padding-left', 'padding-right',
  'padding-top', 'text-align', 'text-decoration', 'text-shadow',
  'text-transform', 'width',
]);

const ALLOWED_FUNCTIONS = new Set([
  'calc', 'clamp', 'color-mix', 'conic-gradient', 'hsl', 'hsla', 'hwb', 'lab',
  'lch', 'linear-gradient', 'max', 'min', 'oklab', 'oklch', 'radial-gradient',
  'repeating-conic-gradient', 'repeating-linear-gradient',
  'repeating-radial-gradient', 'rgb', 'rgba', 'var',
]);

const MAX_CHARS = 2000;

/** Characters that cannot terminate a declaration, open a rule or comment, or fetch. */
const SAFE_VALUE_CHARS = /^[A-Za-z0-9 #%.,\-+/()!_"']*$/;

export function isSafeCSSValue(value: string): boolean {
  if (value.includes('/*') || value.includes('*/')) return false;
  if (!SAFE_VALUE_CHARS.test(value)) return false;
  // Unbalanced quotes would swallow the declarations emitted after this one.
  if ((value.match(/"/g)?.length ?? 0) % 2 !== 0) return false;
  if ((value.match(/'/g)?.length ?? 0) % 2 !== 0) return false;

  let depth = 0;
  for (let i = 0; i < value.length; i++) {
    const ch = value[i];
    if (ch === '(') {
      depth++;
      // Function name is the identifier run ending at i. A vendor spelling
      // such as -webkit-linear-gradient keeps its leading `-` and so fails.
      let start = i;
      while (start > 0 && /[A-Za-z0-9\-_]/.test(value[start - 1]!)) start--;
      if (!ALLOWED_FUNCTIONS.has(value.slice(start, i).toLowerCase())) return false;
    } else if (ch === ')') {
      depth--;
      if (depth < 0) return false;
    }
  }
  return depth === 0;
}

export function sanitizeCSS(input: string): string {
  const truncated = [...input].slice(0, MAX_CHARS).join('');
  const kept: string[] = [];

  for (const declaration of truncated.split(';')) {
    const colon = declaration.indexOf(':');
    if (colon < 0) continue;
    const property = declaration.slice(0, colon).trim().toLowerCase();
    const value = declaration.slice(colon + 1).trim();

    if (!ALLOWED_PROPERTIES.has(property)) continue;
    if (!value || [...value].length > 200 || !isSafeCSSValue(value)) continue;
    kept.push(`${property}:${value}`);
  }

  return kept.join(';');
}

/**
 * Mirrors `is_safe_css_color`. Wider than a hex check because the branding
 * wizard pairs its colour picker with a free-text field, so gradients and the
 * modern colour spaces are already in use.
 */
export function isSafeCSSColor(value: string): boolean {
  const v = value.trim();
  return v.length > 0 && [...v].length <= 200 && isSafeCSSValue(v);
}
