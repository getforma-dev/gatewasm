import { readFileSync, unlinkSync } from 'fs';
import { execSync } from 'child_process';

const PID_FILE = '/tmp/gatewasm-e2e.pid';
const MAILPIT_PID_FILE = '/tmp/gatewasm-e2e-mailpit.pid';

export default async function globalTeardown() {
  // Kill the test server
  try {
    const pid = readFileSync(PID_FILE, 'utf-8').trim();
    // Kill process group (negative PID kills the group, catches cargo + binary)
    try { process.kill(-Number(pid), 'SIGTERM'); } catch {}
    // Also kill anything still listening on the port
    execSync(`lsof -ti :3102 | xargs kill 2>/dev/null`);
  } catch {}
  try { unlinkSync(PID_FILE); } catch {}

  // Kill Mailpit
  try {
    const pid = readFileSync(MAILPIT_PID_FILE, 'utf-8').trim();
    try { process.kill(-Number(pid), 'SIGTERM'); } catch {}
    execSync(`lsof -ti :1025 | xargs kill 2>/dev/null`);
  } catch {}
  try { unlinkSync(MAILPIT_PID_FILE); } catch {}
}
