#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <image-tag> [--also-latest]"
  exit 1
fi

tag="$1"
also_latest="${2:-}"

repo_uri="${ECR_REPOSITORY_URI:-877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server}"
region="${AWS_REGION:-us-east-1}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

echo "Logging in to ECR (${repo_uri})..."
aws ecr get-login-password --region "${region}" | \
  docker login --username AWS --password-stdin "$(dirname "${repo_uri}")"

source_revision="$(git -C "${repo_root}" rev-parse HEAD)"
if [[ -n "$(git -C "${repo_root}" status --porcelain --untracked-files=no)" ]]; then
  echo "Refusing an image identity for a dirty source tree." >&2
  exit 1
fi
echo "Building image ${repo_uri}:${tag}..."
docker build \
  --platform linux/amd64 \
  --build-arg "GATEWASM_SOURCE_REVISION=${source_revision}" \
  --label "org.opencontainers.image.revision=${source_revision}" \
  -f "${repo_root}/crates/server/Dockerfile" \
  -t "${repo_uri}:${tag}" \
  "${repo_root}"

echo "Pushing immutable image tag ${tag}..."
docker push "${repo_uri}:${tag}"

if [[ "${also_latest}" == "--also-latest" ]]; then
  echo "Tagging and pushing latest alias..."
  docker tag "${repo_uri}:${tag}" "${repo_uri}:latest"
  docker push "${repo_uri}:latest"
fi

digest="$(aws ecr describe-images \
  --repository-name "$(basename "${repo_uri}")" \
  --image-ids imageTag="${tag}" \
  --region "${region}" \
  --query 'imageDetails[0].imageDigest' \
  --output text)"

echo "Pushed ${repo_uri}:${tag} (${digest})"

