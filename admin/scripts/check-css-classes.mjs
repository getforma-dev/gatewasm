#!/usr/bin/env node
/**
 * CSS class ratchet.
 *
 * Fails when a component emits a class token that no stylesheet loaded on
 * that component's route ever defines. This is the class of bug that made
 * the tenant admin page render ghost buttons and an unstyled modal for
 * months: the markup spoke a vocabulary that only a different route's
 * bundle knew.
 *
 * Mirrors check-ir-degradations.mjs: a per-route baseline that may only go
 * down, `--update` to re-record, and a loud abort when the inputs are
 * missing rather than a silent pass.
 *
 *   node scripts/check-css-classes.mjs            # verify against baseline
 *   node scripts/check-css-classes.mjs --update   # re-record the baseline
 */
import { readFileSync, writeFileSync, existsSync, readdirSync, statSync } from 'fs';
import { join, dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const ADMIN = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIST = join(ADMIN, 'dist');
const BASELINE = join(ADMIN, 'scripts', 'css-class-baseline.json');
const UPDATE = process.argv.includes('--update');

/** Abort loudly: an unreadable input must never read as "zero misses". */
function die(msg) {
  console.error(`check-css-classes: ${msg}`);
  process.exit(1);
}

const manifestPath = join(DIST, 'manifest.json');
if (!existsSync(manifestPath)) die('dist/manifest.json missing — run `npm run build` first.');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
if (!manifest.routes || !Object.keys(manifest.routes).length) die('manifest has no routes.');

// Entry js file -> source entry path, from build.ts's entry list.
const buildSrc = readFileSync(join(ADMIN, 'build.ts'), 'utf8');
const entryMap = new Map();
for (const m of buildSrc.matchAll(/entry:\s*'([^']+)',\s*outfile:\s*'([^']+)\.js'/g)) {
  entryMap.set(m[2], m[1]);
}
if (!entryMap.size) die('could not parse entrypoints out of build.ts.');

/** Follow relative imports from an entry file; return every reachable source file. */
function reachableSources(entryRel) {
  const seen = new Set();
  const stack = [join(ADMIN, entryRel)];
  while (stack.length) {
    const file = stack.pop();
    const candidates = [file, `${file}.ts`, join(file, 'index.ts')];
    const real = candidates.find((c) => existsSync(c) && statSync(c).isFile());
    if (!real || seen.has(real)) continue;
    seen.add(real);
    const text = readFileSync(real, 'utf8');
    for (const m of text.matchAll(/from\s+'(\.[^']+)'/g)) {
      stack.push(resolve(dirname(real), m[1]));
    }
  }
  return seen;
}

/** Class tokens from `class: '...'` literals, including inside thunks. */
function classTokens(files) {
  const tokens = new Map(); // token -> Set(file)
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    for (const m of text.matchAll(/class:\s*(['"])([^'"]+)\1/g)) {
      for (const tok of m[2].trim().split(/\s+/)) {
        if (!tok || tok.includes('$') || tok.includes('{')) continue;
        if (!tokens.has(tok)) tokens.set(tok, new Set());
        tokens.get(tok).add(file.slice(ADMIN.length + 1));
      }
    }
  }
  return tokens;
}

const cssCache = new Map();
function cssFor(name) {
  if (!cssCache.has(name)) {
    const p = join(DIST, name);
    if (!existsSync(p)) die(`route stylesheet ${name} listed in the manifest is missing from dist.`);
    cssCache.set(name, readFileSync(p, 'utf8'));
  }
  return cssCache.get(name);
}

const results = {};
for (const [route, assets] of Object.entries(manifest.routes)) {
  const jsList = assets.js || [];
  const cssList = assets.css || [];
  if (!cssList.length) continue;
  const haystack = cssList.map(cssFor).join('\n');

  const sources = new Set();
  for (const js of jsList) {
    const base = js.replace(/\.[a-f0-9]+\.js$/, '').replace(/\.js$/, '');
    const entry = entryMap.get(base);
    if (!entry) die(`no build.ts entry matches emitted bundle "${js}" on route ${route}.`);
    for (const f of reachableSources(entry)) sources.add(f);
  }
  if (!sources.size) die(`resolved zero source files for route ${route}.`);

  const misses = [];
  for (const [tok, files] of classTokens(sources)) {
    const re = new RegExp('\\.' + tok.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '(?![\\w-])');
    if (!re.test(haystack)) misses.push({ class: tok, sources: [...files].sort() });
  }
  results[route] = misses.sort((a, b) => a.class.localeCompare(b.class));
}

const total = Object.values(results).reduce((n, m) => n + m.length, 0);

if (UPDATE) {
  const counts = Object.fromEntries(Object.entries(results).map(([r, m]) => [r, m.length]));
  writeFileSync(BASELINE, JSON.stringify({ total, counts, detail: results }, null, 2) + '\n');
  console.log(`check-css-classes: baseline recorded — ${total} undefined class references.`);
  process.exit(0);
}

if (!existsSync(BASELINE)) die('no baseline — run with --update once to record one.');
const baseline = JSON.parse(readFileSync(BASELINE, 'utf8'));

let failed = false;
for (const [route, misses] of Object.entries(results)) {
  const was = baseline.counts?.[route] ?? 0;
  if (misses.length > was) {
    failed = true;
    console.error(`\n${route}: ${misses.length} undefined class references (baseline ${was})`);
    const known = new Set((baseline.detail?.[route] || []).map((m) => m.class));
    for (const m of misses.filter((m) => !known.has(m.class))) {
      console.error(`  .${m.class} — emitted by ${m.sources.join(', ')}, defined in no stylesheet on this route`);
    }
  }
}

if (failed) {
  console.error('\nEither define the class in a stylesheet this route loads, or use the');
  console.error('existing class that already carries the styling you want.');
  process.exit(1);
}

console.log(`CSS class references: ${total} undefined (baseline ${baseline.total}) — no regression.`);
if (total < baseline.total) {
  console.log('Improved — re-record with `npm run check:css -- --update`.');
}
