import { execSync, spawn } from 'child_process';
import { writeFileSync } from 'fs';

const PORT = 3102;
const PID_FILE = '/tmp/gatewasm-e2e.pid';

export default async function globalSetup() {
  // Kill any existing server
  try { execSync(`lsof -ti :${PORT} | xargs kill 2>/dev/null`); } catch {}
  await new Promise(r => setTimeout(r, 1000));

  const server = spawn('cargo', ['run', '-p', 'gatewasm-server'], {
    cwd: process.cwd().replace('/admin', ''),
    env: {
      ...process.env,
      PORT: String(PORT),
      SERVER_URL: `http://127.0.0.1:${PORT}`,
      DATABASE_URL: 'postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test',
      JWT_SECRET: 'test-secret-key-for-development',
      MASTER_MGMT_KEY: 'mgmt_test_key',
      MASTER_PLATFORM_KEY: 'plat_local_dev_key',
      COOKIE_DOMAIN: 'localhost',
      JWT_ACCESS_TOKEN_EXPIRY: '60',
      REGISTER_RATE_LIMIT_BURST: '100',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: true,
  });

  // Drain stdout/stderr to prevent pipe buffer from filling and blocking server
  server.stdout?.resume();
  server.stderr?.resume();
  server.unref();

  writeFileSync(PID_FILE, String(server.pid));

  // Wait for health
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/health`);
      if (res.ok) return;
    } catch {}
    await new Promise(r => setTimeout(r, 1000));
  }
  throw new Error('Server failed to start within 30s');
}
