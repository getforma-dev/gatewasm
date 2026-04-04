import { readFileSync } from 'node:fs';

const manifestPath = new URL('../admin/dist/manifest.json', import.meta.url);
const deadline = Date.now() + 30_000;

function readBuildHash() {
  try {
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    return manifest?.build_hash ?? null;
  } catch {
    return null;
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

let logged = false;
while (Date.now() < deadline) {
  if (readBuildHash() === 'dev') {
    process.exit(0);
  }
  if (!logged) {
    console.log('Waiting for admin/dist/manifest.json (build_hash="dev")...');
    logged = true;
  }
  await sleep(200);
}

console.error('Timed out waiting for admin/dist/manifest.json with build_hash=\"dev\"');
process.exit(1);
