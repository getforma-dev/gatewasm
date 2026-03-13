// GateWASM build script — uses @getforma/compiler for SSR IR emission.
// TODO: Migrate to @getforma/build's parameterized build() when ready.
import * as esbuild from 'esbuild';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, rmSync, readdirSync, readFileSync, writeFileSync, renameSync, statSync, existsSync } from 'node:fs';
import { join, extname, basename } from 'node:path';
import { brotliCompressSync, gzipSync, constants } from 'node:zlib';
import { execSync } from 'node:child_process';

const isWatch = process.argv.includes('--watch');
const isSsr = process.argv.includes('--ssr');

let formaSsrPlugin: ((opts: { page: string; outDir: string; entryPoint?: string }) => esbuild.Plugin) | undefined;
if (isSsr) {
  console.log('SSR mode enabled — emitting IR files');
  const { formaSsrPlugin: plugin } = await import('@getforma/compiler');
  formaSsrPlugin = plugin;
}

const entries: { entryPoints: string[]; outfile: string }[] = [
  // Platform pages
  { entryPoints: ['src/platform/login/app.ts'], outfile: 'dist/platform-login.js' },
  { entryPoints: ['src/platform/onboarding/app.ts'], outfile: 'dist/platform-onboarding.js' },
  { entryPoints: ['src/platform/console/app.ts'], outfile: 'dist/platform-console.js' },
  { entryPoints: ['src/platform/benchmark/app.ts'], outfile: 'dist/platform-benchmark.js' },
  // Admin pages
  { entryPoints: ['src/admin/dashboard/app.ts'], outfile: 'dist/admin-dashboard.js' },
  // Tenant pages
  { entryPoints: ['src/tenant/login/app.ts'], outfile: 'dist/tenant-login.js' },
  { entryPoints: ['src/tenant/dashboard/app.ts'], outfile: 'dist/tenant-dashboard.js' },
  { entryPoints: ['src/tenant/admin/app.ts'], outfile: 'dist/tenant-admin.js' },
];

const shared: Partial<esbuild.BuildOptions> = {
  bundle: true,
  format: 'esm',
  target: 'es2022',
  // FormaJS is resolved at build time via alias — no runtime import map needed.
  // Uses the published @getforma/core package instead of monorepo path.
  alias: { 'formajs': '@getforma/core' },
  minify: !isWatch,
  sourcemap: isWatch ? 'inline' : false,
  logLevel: 'info',
};

// Clean dist/ to avoid double-hashing leftover files from previous builds
rmSync('dist', { recursive: true, force: true });
mkdirSync('dist', { recursive: true });

// ── CSS generation (moved here so it runs after dist/ clean) ─────────
execSync('npx @tailwindcss/cli -i src/styles/main.css -o dist/owner.css --minify', { stdio: 'inherit' });
const cssConcat = ['src/core/theme/tokens.css', 'src/core/theme/base.css', 'src/core/theme/components.css', 'src/core/theme/pages.css']
  .map((f) => readFileSync(f, 'utf8'))
  .join('\n');
writeFileSync('dist/forma-platform.css', cssConcat);

// FormaJS is resolved at build time via the alias above — no separate
// bundle needed. Each page module inlines FormaJS automatically.


// Copy font files to dist/ (fonts keep stable names — not hashed)
const FONT_DIR = 'src/core/theme/fonts';
if (existsSync(FONT_DIR)) {
  for (const fontFile of readdirSync(FONT_DIR)) {
    if (fontFile.endsWith('.woff2')) {
      cpSync(join(FONT_DIR, fontFile), join('dist', fontFile));
    }
  }
}

if (isWatch) {
  for (const entry of entries) {
    const ctx = await esbuild.context({ ...shared, ...entry });
    await ctx.watch();
  }
} else {
  // Parallel builds — with optional SSR IR emission plugin
  await Promise.all(
    entries.map((entry) => {
      const buildOptions: esbuild.BuildOptions = { ...shared, ...entry };
      if (isSsr && formaSsrPlugin) {
        const pageName = basename(entry.outfile, '.js');
        buildOptions.plugins = [
          ...(buildOptions.plugins || []),
          formaSsrPlugin({
            page: pageName,
            outDir: 'dist',
            entryPoint: entry.entryPoints[0],
          }),
        ];
      }
      return esbuild.build(buildOptions);
    })
  );

  // ── Island registry entry point generation ──────────────────────────
  // For pages with islands, generate a client entry point that imports
  // activateIslands and the top-level component.
  if (isSsr) {
    const generatedRegistries: string[] = [];

    for (const entry of entries) {
      const pageName = basename(entry.outfile, '.js');
      const islandMetaPath = join('dist', `${pageName}.islands.json`);

      if (!existsSync(islandMetaPath)) continue;

      const islands = JSON.parse(readFileSync(islandMetaPath, 'utf8')) as Array<{ id: number; name: string }>;
      if (islands.length === 0) continue;

      // The page's main JS module already bundles the component + FormaJS.
      // Generate a registry entry point that re-exports the activation call.
      // The entry point imports the page module (which includes activateIslands)
      // and maps island names to the page's root component.
      // TODO: This registry is scaffolding — it will need a separate esbuild
      // pass to bundle into a real client entry point. For now, import paths
      // reference the source entry relative to the admin/ directory.
      const registrySource = [
        `// Auto-generated island registry for ${pageName}`,
        `// Islands discovered: ${islands.map(i => i.name).join(', ')}`,
        `// TODO: Bundle this file with a second esbuild pass for production use`,
        `import { activateIslands } from 'formajs';`,
        `import PageComponent from '../${entry.entryPoints[0]}';`,
        ``,
        `// Map all micro-islands to the page's root component`,
        `const registry = {`,
        ...islands.map(i => `  '${i.name}': PageComponent,`),
        `};`,
        ``,
        `activateIslands(registry);`,
        ``,
      ].join('\n');

      const registryPath = join('dist', `${pageName}.islands.js`);
      writeFileSync(registryPath, registrySource);
      generatedRegistries.push(pageName);

      console.log(`   Island registry: ${pageName}.islands.js (${islands.length} islands)`);
    }

    if (generatedRegistries.length > 0) {
      console.log(`   Generated ${generatedRegistries.length} island registry entry points`);
    }
  }

  // ── Content-hashing post-processor + manifest generation ──────────────
  const distDir = 'dist';
  const BUDGET_THRESHOLD = 204800; // 200KB

  // Route → asset mapping (original base names, no extensions).
  // FormaJS is bundled into each page module — no separate entry needed.
  const routeMap: Record<string, { js: string[]; css: string[] }> = {
    '/platform/login':           { js: ['platform-login'],      css: ['forma-platform'] },
    '/platform/onboarding':      { js: ['platform-onboarding'], css: ['forma-platform'] },
    '/platform/console':         { js: ['platform-console'],    css: ['forma-platform'] },
    '/platform/benchmark':       { js: ['platform-benchmark'],  css: ['forma-platform'] },
    '/platform/admin/dashboard': { js: ['admin-dashboard'],     css: ['owner', 'forma-platform'] },
    '/t/:slug/login':            { js: ['tenant-login'],        css: ['forma-platform'] },
    '/t/:slug/dashboard':        { js: ['tenant-dashboard'],    css: ['forma-platform'] },
    '/t/:slug/admin':            { js: ['tenant-admin'],        css: ['forma-platform'] },
  };

  // Hash a file and return 8-char hex prefix
  function contentHash(filePath: string): string {
    const content = readFileSync(filePath);
    return createHash('sha256').update(content).digest('hex').slice(0, 8);
  }

  // Collect all JS and CSS files in dist/ to hash
  const files = readdirSync(distDir);
  const assets: Record<string, string> = {}; // original name → hashed name

  for (const file of files) {
    const ext = extname(file);
    // Only hash .js, .css, .wasm, .ir files; skip manifest.json, .woff2, etc.
    if (ext !== '.js' && ext !== '.css' && ext !== '.wasm' && ext !== '.ir') continue;
    // Skip files inlined by the server (not served as assets via /_assets/)
    if (file.startsWith('formajs-runtime')) continue;

    const filePath = join(distDir, file);
    const hash = contentHash(filePath);
    const base = basename(file, ext);
    const hashedName = `${base}.${hash}${ext}`;

    // Keep unhashed copy for files inlined by the server via asset() calls
    // (hosted_login.rs inlines forma-platform.css and tenant-login.js)
    const serverInlined = new Set(['forma-platform.css', 'tenant-login.js', 'sw.js']);
    if (serverInlined.has(file)) {
      cpSync(filePath, join(distDir, hashedName));
    } else {
      renameSync(filePath, join(distDir, hashedName));
    }
    assets[file] = hashedName;
  }

  // ── Build-time compression ───────────────────────────────────────
  const COMPRESSIBLE_EXTS = new Set(['.js', '.css']);
  let compressCount = 0;

  for (const file of readdirSync(distDir)) {
    const ext = extname(file);
    if (!COMPRESSIBLE_EXTS.has(ext)) continue;

    const filePath = join(distDir, file);
    if (!statSync(filePath).isFile()) continue;

    const content = readFileSync(filePath);

    // Brotli (level 11 — max compression)
    const br = brotliCompressSync(content, {
      params: {
        [constants.BROTLI_PARAM_QUALITY]: 11,
      },
    });
    if (br.length < content.length) {
      writeFileSync(`${filePath}.br`, br);
    }

    // Gzip (level 9 — max compression)
    const gz = gzipSync(content, { level: 9 });
    if (gz.length < content.length) {
      writeFileSync(`${filePath}.gz`, gz);
    }

    compressCount++;
  }

  console.log(`   ${compressCount} files compressed (brotli 11 + gzip 9)`);

  // Build hash: SHA-256 of all hashed filenames sorted and joined
  const buildHash = createHash('sha256')
    .update(Object.values(assets).sort().join(','))
    .digest('hex');

  // Collect font files (stable names, not hashed)
  const fonts = readdirSync(distDir).filter(f => f.endsWith('.woff2')).sort();

  // Build route manifest
  const routes: Record<string, {
    js: string[];
    css: string[];
    fonts: string[];
    total_size_br: number;
    budget_warn_threshold: number;
    ir?: string;
  }> = {};

  for (const [route, mapping] of Object.entries(routeMap)) {
    const jsFiles = mapping.js.map((name) => assets[`${name}.js`] ?? `${name}.js`);
    const cssFiles = mapping.css.map((name) => assets[`${name}.css`] ?? `${name}.css`);

    // Calculate total size using brotli sizes when available
    let totalSize = 0;
    for (const f of [...jsFiles, ...cssFiles]) {
      const brPath = join(distDir, `${f}.br`);
      const origPath = join(distDir, f);
      if (existsSync(brPath)) {
        totalSize += statSync(brPath).size;
      } else if (existsSync(origPath)) {
        totalSize += statSync(origPath).size;
      }
    }
    // Add font sizes (uncompressed — woff2 is already compressed)
    for (const f of fonts) {
      const fp = join(distDir, f);
      if (existsSync(fp)) {
        totalSize += statSync(fp).size;
      }
    }

    const routeEntry: typeof routes[string] = {
      js: jsFiles,
      css: cssFiles,
      fonts,
      total_size_br: totalSize,
      budget_warn_threshold: BUDGET_THRESHOLD,
    };

    // If SSR mode, check for .ir files and add to route manifest
    if (isSsr) {
      // Derive page name from the first JS base name (e.g., 'platform-login')
      const pageName = mapping.js[0];
      if (pageName) {
        const irFile = `${pageName}.ir`;
        if (assets[irFile]) {
          routeEntry.ir = assets[irFile];
        }
      }
    }

    routes[route] = routeEntry;
  }

  // Write manifest
  const manifest: Record<string, any> = {
    version: 1,
    build_hash: buildHash,
    assets,
    routes,
  };

  writeFileSync(join(distDir, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`\n✅ Manifest written: dist/manifest.json (build_hash: ${buildHash.slice(0, 12)}…)`);
  console.log(`   ${Object.keys(assets).length} assets hashed`);

  // ── Service Worker generation ──────────────────────────────────────
  const swCacheName = `forma-${buildHash.slice(0, 12)}`;

  // Precache: all CSS assets + all font files
  const precacheUrls = [
    ...Object.entries(assets)
      .filter(([orig]) => orig.endsWith('.css'))
      .map(([, hashed]) => `/_assets/${hashed}`),
    ...fonts.map(f => `/_assets/${f}`),
  ];

  const swContent = `// Generated by build.ts — do not edit
const CACHE_NAME = '${swCacheName}';
const PRECACHE_URLS = ${JSON.stringify(precacheUrls, null, 2)};

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME)
      .then((cache) => cache.addAll(PRECACHE_URLS))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((names) => Promise.all(
        names.filter((n) => n !== CACHE_NAME).map((n) => caches.delete(n))
      ))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  if (!url.pathname.startsWith('/_assets/')) return;

  event.respondWith(
    caches.match(event.request).then((cached) => {
      if (cached) return cached;
      return fetch(event.request).then((response) => {
        if (response.ok) {
          const clone = response.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
        }
        return response;
      });
    })
  );
});
`;

  writeFileSync(join(distDir, 'sw.js'), swContent);
  console.log(`   SW generated: dist/sw.js (cache: ${swCacheName})`);

  // Budget warnings
  for (const [route, info] of Object.entries(routes)) {
    if (info.total_size_br > BUDGET_THRESHOLD) {
      const kb = (info.total_size_br / 1024).toFixed(1);
      console.warn(`⚠️  Budget exceeded: ${route} → ${kb}KB (threshold: ${BUDGET_THRESHOLD / 1024}KB)`);
    }
  }
}
