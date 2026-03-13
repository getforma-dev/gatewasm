import { readFileSync, unlinkSync } from 'fs';
import { execSync } from 'child_process';

const PID_FILE = '/tmp/gatewasm-e2e.pid';

export default async function globalTeardown() {
  try {
    const pid = readFileSync(PID_FILE, 'utf-8').trim();
    // Kill process group (negative PID kills the group, catches cargo + binary)
    try { process.kill(-Number(pid), 'SIGTERM'); } catch {}
    // Also kill anything still listening on the port
    execSync(`lsof -ti :3102 | xargs kill 2>/dev/null`);
  } catch {}
  try { unlinkSync(PID_FILE); } catch {}
}
