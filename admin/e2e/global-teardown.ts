import { tmpdir } from 'os';
import { join } from 'path';
import { readFileSync, unlinkSync } from 'fs';
import { execSync } from 'child_process';

const PID_FILE = join(tmpdir(), 'gatewasm-e2e.pid');
const MAILPIT_PID_FILE = join(tmpdir(), 'gatewasm-e2e-mailpit.pid');

export default async function globalTeardown() {
  // Kill the test server
  try {
    const pid = readFileSync(PID_FILE, 'utf-8').trim();
    // A negative PID kills the process group, which catches cargo and the
    // binary it spawned — but Windows has no process groups, so an orphaned
    // server survived teardown and the next run failed to bind the port.
    try { killTree(pid); } catch {}
    // Also kill anything still listening on the port
    if (process.platform !== 'win32') execSync(`lsof -ti :3102 | xargs kill 2>/dev/null`);
  } catch {}
  try { unlinkSync(PID_FILE); } catch {}

  // Kill Mailpit
  try {
    const pid = readFileSync(MAILPIT_PID_FILE, 'utf-8').trim();
    try { killTree(pid); } catch {}
    if (process.platform !== 'win32') execSync(`lsof -ti :1025 | xargs kill 2>/dev/null`);
  } catch {}
  try { unlinkSync(MAILPIT_PID_FILE); } catch {}
}

/** Kill a process and its children on either platform. */
function killTree(pid: string) {
  if (process.platform === 'win32') {
    execSync(`taskkill /PID ${pid} /T /F`, { stdio: 'ignore' });
  } else {
    process.kill(-Number(pid), 'SIGTERM');
  }
}
