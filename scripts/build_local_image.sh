#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <local-image-tag>" >&2
  exit 1
fi
image_tag="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
source_revision="$(git -C "${repo_root}" rev-parse HEAD)"
if [[ -n "$(git -C "${repo_root}" status --porcelain --untracked-files=normal)" ]]; then
  echo "Refusing an image identity for a dirty source tree (including untracked files)." >&2
  exit 1
fi
# An immutable Git archive excludes ignored host assets, credentials and caches.
# The Dockerfile independently builds locked frontend dependencies and records hashes.
git -C "${repo_root}" archive --format=tar "${source_revision}" | \
  docker build --platform linux/amd64 --progress plain \
    --build-arg "GATEWASM_SOURCE_REVISION=${source_revision}" \
    --build-arg "CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-3}" \
    --label "org.opencontainers.image.revision=${source_revision}" \
    -f crates/server/Dockerfile -t "${image_tag}" -
docker image inspect --format '{{.Id}} {{.Os}}/{{.Architecture}} {{index .Config.Labels "org.opencontainers.image.revision"}}' "${image_tag}"
