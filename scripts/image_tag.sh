#!/usr/bin/env bash
set -euo pipefail

git_sha="$(git rev-parse --short=12 HEAD)"
timestamp="$(date -u +%Y%m%d-%H%M%S)"

echo "release-${timestamp}-${git_sha}"

