#!/bin/bash
# Two-step rebuild: admin assets (SSR) → Rust binary
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Step 1/2: Rebuild admin assets (SSR + IR) ==="
cd "$REPO_ROOT/admin"
npm ci
npx tsx build.ts --ssr

echo "=== Step 2/2: Rebuild Rust binary ==="
cd "$REPO_ROOT"
touch crates/server/build.rs
cargo build -p gatewasm-server

echo "=== Rebuild complete ==="
