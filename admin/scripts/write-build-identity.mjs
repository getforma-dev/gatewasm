import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const revision = process.env.GATEWASM_SOURCE_REVISION;
if (!/^[a-f0-9]{40}$/.test(revision ?? '')) throw new Error('A full committed source revision is required');
const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
const files = {};
function visit(directory, relative = '') {
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
    const name = relative ? `${relative}/${entry.name}` : entry.name;
    if (entry.isDirectory()) visit(join(directory, entry.name), name);
    else if (name !== 'build-identity.json') files[name] = sha256(join(directory, entry.name));
  }
}
visit('dist');
const manifestHash = sha256('dist/manifest.json');
const report = JSON.parse(readFileSync('dist/build-report.json', 'utf8'));
if (report.manifest_sha256 !== manifestHash.slice(0, 16)) throw new Error('Build report and manifest differ');
writeFileSync('dist/build-identity.json', JSON.stringify({
  source_revision: revision,
  npm_lock_sha256: sha256('package-lock.json'),
  manifest_sha256: manifestHash,
  files,
}, null, 2) + '\n');
console.log(`Frontend identity: ${revision}, ${Object.keys(files).length} assets, manifest ${manifestHash}`);
