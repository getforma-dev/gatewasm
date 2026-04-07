// fAuth build script — config-driven via @getforma/build
import { build } from '@getforma/build';
import { cpSync, readdirSync } from 'node:fs';

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
  routes: {
    '/platform/login':           { js: ['platform-login'],      css: ['forma-platform'] },
    '/platform/onboarding':      { js: ['platform-onboarding'], css: ['forma-platform'] },
    '/platform/console':         { js: ['platform-console'],    css: ['forma-platform'] },
    '/platform/benchmark':       { js: ['platform-benchmark'],  css: ['forma-platform'] },
    '/platform/admin/dashboard': { js: ['admin-dashboard'],     css: ['owner', 'forma-platform'] },
    '/t/:slug/login':            { js: ['tenant-login'],        css: ['forma-platform'] },
    '/t/:slug/dashboard':        { js: ['tenant-dashboard'],    css: ['forma-platform'] },
    '/t/:slug/admin':            { js: ['tenant-admin'],        css: ['forma-platform'] },
  },
  cssEntries: [
    { input: 'src/styles/main.css', outfile: 'owner.css', tailwind: true },
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

if (result.warnings.length > 0) {
  for (const w of result.warnings) console.warn(`\u26a0\ufe0f  ${w}`);
}
