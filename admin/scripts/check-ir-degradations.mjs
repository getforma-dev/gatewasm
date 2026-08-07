#!/usr/bin/env node
/**
 * Gate on the compiler's SSR degradation diagnostics.
 *
 * `@getforma/compiler` prints one `IR:` line per construct it cannot evaluate
 * at build time. Each one means the server-rendered HTML is missing something
 * the client renders — an attribute omitted, a conditional resolved to the
 * wrong branch, a text node left empty. The page still looks right in a browser
 * because hydration fills it in, so these are invisible until someone disables
 * JavaScript, reads the first paint, or writes an e2e assertion.
 *
 * That is how `/platform/login` came to server-render its "New here?" link with
 * no href: the diagnostic had been printing on every build for months.
 *
 * This does not fix them. It stops the count going up, and makes going down
 * the only direction available.
 *
 * Usage:
 *   node scripts/check-ir-degradations.mjs            # gate against the baseline
 *   node scripts/check-ir-degradations.mjs --update   # re-record after fixing some
 */
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const BASELINE = join(here, 'ir-degradations.json');

/** Per-route degradation counts, keyed by source file. */
function collect() {
  // The compiler writes diagnostics to stderr, so both streams are needed —
  // reading only stdout silently reports zero, which is how this script first
  // "passed" against 158 real degradations.
  const res = spawnSync(
    process.execPath,
    [join(here, '..', 'node_modules', 'tsx', 'dist', 'cli.mjs'), 'build.ts', '--ssr'],
    { cwd: join(here, '..'), encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
  if (res.status !== 0) {
    console.error(res.stdout ?? '');
    console.error(res.stderr ?? '');
    throw new Error(`build.ts --ssr exited ${res.status}`);
  }
  const out = `${res.stdout ?? ''}
${res.stderr ?? ''}`;
  const counts = {};
  for (const line of out.split('\n')) {
    if (!line.includes('IR: ')) continue;
    // Diagnostics name an absolute source path; key on the repo-relative part
    // so the baseline is stable across machines.
    const m = line.match(/src[\\/](.+?\.ts)/);
    const key = m ? m[1].replace(/\\/g, '/') : '(unattributed)';
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}

const current = collect();
const total = Object.values(current).reduce((a, b) => a + b, 0);

if (process.argv.includes('--update')) {
  writeFileSync(BASELINE, `${JSON.stringify({ total, files: current }, null, 2)}\n`);
  console.log(`Recorded ${total} IR degradations across ${Object.keys(current).length} files.`);
  process.exit(0);
}

if (!existsSync(BASELINE)) {
  console.error(`No baseline at ${BASELINE}. Run with --update to record one.`);
  process.exit(1);
}

const baseline = JSON.parse(readFileSync(BASELINE, 'utf8'));
const regressions = [];
for (const [file, count] of Object.entries(current)) {
  const was = baseline.files[file] ?? 0;
  if (count > was) regressions.push(`  ${file}: ${was} -> ${count}`);
}
const improvements = [];
for (const [file, was] of Object.entries(baseline.files)) {
  const now = current[file] ?? 0;
  if (now < was) improvements.push(`  ${file}: ${was} -> ${now}`);
}

if (regressions.length) {
  console.error(
    `\nSSR degraded further: ${total} IR diagnostics, baseline ${baseline.total}.\n` +
      `Each one means the server-rendered HTML differs from what the client renders.\n\n` +
      `${regressions.join('\n')}\n\n` +
      `Run \`npm run build:ssr\` and read the IR: lines — they name the fix.\n` +
      `Usually: give the signal a literal default, fold the value into a\n` +
      `module-level const, or wrap the expression in a function for a real\n` +
      `reactive binding.\n`,
  );
  process.exit(1);
}

if (improvements.length) {
  console.log(
    `\nSSR degradations improved: ${total}, baseline ${baseline.total}.\n${improvements.join('\n')}\n\n` +
      `Run \`node scripts/check-ir-degradations.mjs --update\` to lock it in.\n`,
  );
  process.exit(1);
}

console.log(`IR degradations: ${total} (baseline ${baseline.total}) — no regression.`);
