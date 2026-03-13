#!/bin/bash
# Start gatewasm-server on port 3102 with test database.
# Waits for /health, then prints PID.
# Usage: source scripts/serve-test.sh  (sets GATEWASM_PID)
#        kill $GATEWASM_PID            (cleanup)
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Kill existing server on port if any
lsof -ti :3102 2>/dev/null | xargs kill 2>/dev/null || true
sleep 1

export PORT=3102
export SERVER_URL=http://127.0.0.1:3102
export DATABASE_URL=postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test
export JWT_SECRET=test-secret-key-for-development
export MASTER_MGMT_KEY=mgmt_test_key
export MASTER_PLATFORM_KEY=plat_local_dev_key
export COOKIE_DOMAIN=localhost

cd "$REPO_ROOT"
cargo run -p gatewasm-server &
GATEWASM_PID=$!

# Wait for health check (up to 30s)
for i in $(seq 1 30); do
  if curl -sf http://127.0.0.1:3102/health > /dev/null 2>&1; then
    echo "Server ready (PID=$GATEWASM_PID)"
    export GATEWASM_PID
    exit 0
  fi
  sleep 1
done

echo "Server failed to start within 30s"
kill $GATEWASM_PID 2>/dev/null
exit 1
