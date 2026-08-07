import { execSync, spawn } from 'child_process';
import { writeFileSync } from 'fs';

const PORT = 3102;
const MAILPIT_SMTP_PORT = 1025;
const MAILPIT_UI_PORT = 8025;
const PID_FILE = '/tmp/gatewasm-e2e.pid';
const MAILPIT_PID_FILE = '/tmp/gatewasm-e2e-mailpit.pid';

export default async function globalSetup() {
  // Kill any existing server and mailpit
  try { execSync(`lsof -ti :${PORT} | xargs kill 2>/dev/null`); } catch {}
  try { execSync(`lsof -ti :${MAILPIT_SMTP_PORT} | xargs kill 2>/dev/null`); } catch {}
  await new Promise(r => setTimeout(r, 1000));

  // Start Mailpit — local SMTP trap for capturing test emails.
  //
  // Optional. CI does not install it, and the server suppresses delivery in
  // test environments anyway, so its absence must not stop the suite. Without
  // the 'error' handler below, a missing binary raises ENOENT as an unhandled
  // 'error' event, which killed the whole run before a single test executed —
  // the CI job could never pass on any commit.
  let mailpitReady = false;
  const mailpit = spawn('mailpit', [
    '--smtp', `0.0.0.0:${MAILPIT_SMTP_PORT}`,
    '--listen', `0.0.0.0:${MAILPIT_UI_PORT}`,
  ], {
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: true,
  });
  mailpit.on('error', (err: NodeJS.ErrnoException) => {
    if (err.code === 'ENOENT') {
      console.warn('[e2e] mailpit not installed — email-inspecting tests will be skipped');
    } else {
      console.warn(`[e2e] mailpit failed to start: ${err.message}`);
    }
  });
  mailpit.stdout?.resume();
  mailpit.stderr?.resume();
  mailpit.unref();
  if (mailpit.pid) writeFileSync(MAILPIT_PID_FILE, String(mailpit.pid));

  // Wait for Mailpit to be ready
  for (let i = 0; i < 10; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${MAILPIT_UI_PORT}/api/v1/messages?limit=1`);
      if (res.ok) { mailpitReady = true; break; }
    } catch {}
    await new Promise(r => setTimeout(r, 500));
  }
  // Surfaced so a test that needs the inbox can skip rather than fail obscurely.
  process.env.MAILPIT_AVAILABLE = mailpitReady ? '1' : '';

  // Test-only defaults — override via environment for CI or shared envs.
  const server = spawn('cargo', ['run', '-p', 'gatewasm-server'], {
    cwd: process.cwd().replace('/admin', ''),
    env: {
      ...process.env,
      PORT: String(PORT),
      SERVER_URL: `http://127.0.0.1:${PORT}`,
      DATABASE_URL: process.env.DATABASE_URL || 'postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test',
      JWT_SECRET: process.env.JWT_SECRET || 'test-secret-key-for-development',
      MASTER_MGMT_KEY: process.env.MASTER_MGMT_KEY || 'mgmt_test_key',
      MASTER_PLATFORM_KEY: process.env.MASTER_PLATFORM_KEY || 'plat_local_dev_key',
      COOKIE_DOMAIN: process.env.COOKIE_DOMAIN || 'localhost',
      JWT_ACCESS_TOKEN_EXPIRY: process.env.JWT_ACCESS_TOKEN_EXPIRY || '60',
      REGISTER_RATE_LIMIT_BURST: process.env.REGISTER_RATE_LIMIT_BURST || '500',
      LOGIN_RATE_LIMIT_BURST: process.env.LOGIN_RATE_LIMIT_BURST || '500',
      TOKEN_RATE_LIMIT_BURST: process.env.TOKEN_RATE_LIMIT_BURST || '500',
      WS_TICKET_TTL_SECS: process.env.WS_TICKET_TTL_SECS || '5',
      // Route emails to local Mailpit instead of Resend/external SMTP
      SMTP_HOST: `127.0.0.1`,
      SMTP_PORT: String(MAILPIT_SMTP_PORT),
      SMTP_FROM_EMAIL: 'test@getforma.dev',
      SMTP_STARTTLS: 'false',
      // Unset Resend so the server uses SMTP (Mailpit) instead
      RESEND_API_KEY: '',
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
