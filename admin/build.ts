// fAuth build script — config-driven via @getforma/build
import { build } from '@getforma/build';
import { cpSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

// Font files by type role (see --font-* in core/theme/tokens.css). The
// numeral face is only preloaded where stat values or plane pips render.
const TEXT_FONTS = [
  'instrument-serif-regular.woff2',
  'instrument-serif-italic.woff2',
  'dm-mono-300.woff2',
  'dm-mono-400.woff2',
  'dm-mono-500.woff2',
];
const NUMERAL_FONTS = [...TEXT_FONTS, 'orbitron-500.woff2', 'orbitron-700.woff2'];

const result = await build({
  entryPoints: [
    { entry: 'src/platform/login/app.ts', outfile: 'platform-login.js' },
    { entry: 'src/platform/onboarding/app.ts', outfile: 'platform-onboarding.js' },
    { entry: 'src/platform/console/app.ts', outfile: 'platform-console.js' },
    { entry: 'src/platform/benchmark/app.ts', outfile: 'platform-benchmark.js' },
    { entry: 'src/admin/dashboard/app.ts', outfile: 'admin-dashboard.js' },
    { entry: 'src/tenant/login/app.ts', outfile: 'tenant-login.js' },
    { entry: 'src/tenant/dashboard/app.ts', outfile: 'tenant-dashboard.js' },
    { entry: 'src/tenant/admin/app.ts', outfile: 'tenant-admin.js' },
  ],
  // Every route used to preload all 11 font files, which is most of why all
  // 8 blew the 195KB budget. Fonts are declared per route by the type roles
  // the route's markup actually reaches: TEXT_FONTS is display + data (every
  // route), and only surfaces rendering stat values or plane pips also pull
  // the numeral face.
  routes: {
    '/platform/login':           { js: ['platform-login'],      css: ['forma-platform'], fonts: TEXT_FONTS },
    '/platform/onboarding':      { js: ['platform-onboarding'], css: ['forma-platform'], fonts: TEXT_FONTS },
    '/platform/console':         { js: ['platform-console'],    css: ['forma-platform'], fonts: NUMERAL_FONTS },
    '/platform/benchmark':       { js: ['platform-benchmark'],  css: ['forma-platform'], fonts: NUMERAL_FONTS },
    '/platform/admin/dashboard': { js: ['admin-dashboard'],     css: ['forma-platform'], fonts: NUMERAL_FONTS },
    '/t/:slug/login':            { js: ['tenant-login'],        css: ['forma-platform'], fonts: TEXT_FONTS },
    '/t/:slug/dashboard':        { js: ['tenant-dashboard'],    css: ['forma-platform'], fonts: NUMERAL_FONTS },
    '/t/:slug/admin':            { js: ['tenant-admin'],        css: ['forma-platform'], fonts: NUMERAL_FONTS },
  },
  cssEntries: [
    { input: 'src/core/theme/platform.css', outfile: 'forma-platform.css', tailwind: true },
  ],
  outputDir: 'dist',
  formaAlias: '@getforma/core',
  fontDir: 'src/core/theme/fonts',
  ssr: process.argv.includes('--ssr'),
  watch: process.argv.includes('--watch'),
  serverInlined: ['forma-platform.css', 'tenant-login.js', 'sw.js'],
});

// Copy brand assets to dist/ so rust_embed can serve them at /_assets/
const assetDir = 'src/core/assets';
for (const f of readdirSync(assetDir)) {
  cpSync(`${assetDir}/${f}`, `dist/${f}`);
}

// @getforma/build already printed each warning as it was found \u2014 re-printing
// result.warnings here doubled every line, which put the bundle warning
// baseline at 2x reality (16 recorded for 8 over-budget routes).

// Record the build's warnings as a STRUCTURED artifact next to the output
// they describe. The warning-budget gate reads this instead of re-running
// the build and scraping console text \u2014 the count is taken once, at the
// source, and manifest_sha256 ties the report to the exact dist it measured
// so a torn or stale report/manifest pairing is detectable.
const manifestSha = createHash('sha256')
  .update(readFileSync('dist/manifest.json'))
  .digest('hex')
  .slice(0, 16);
writeFileSync('dist/build-report.json', JSON.stringify({
  manifest_sha256: manifestSha,
  budget_exceeded: result.warnings.filter((w: string) => w.includes('Budget exceeded')).length,
  warnings: result.warnings,
}, null, 2) + '\n');
