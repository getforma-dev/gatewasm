#!/bin/bash
# Three-step rebuild: formajs dist → admin assets (SSR) → Rust binary
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT_ROOT="$(cd "$REPO_ROOT/.." && pwd)"

echo "=== Step 1/3: Rebuild FormaJS dist ==="
cd "$PROJECT_ROOT/npm-modules/formajs"
npm run build

echo "=== Step 2/3: Rebuild admin assets (SSR + IR) ==="
cd "$REPO_ROOT/admin"
npx tsx build.ts --ssr

echo "=== Step 3/3: Rebuild Rust binary ==="
cd "$REPO_ROOT"
touch crates/gatewasm-server/build.rs
cargo build -p gatewasm-server

echo "=== Rebuild complete ==="
