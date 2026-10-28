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

# Finish and qualify the committed image before authenticating to the registry.
"${script_dir}/build_local_image.sh" "${repo_uri}:${tag}"

echo "Logging in to ECR (${repo_uri})..."
aws ecr get-login-password --region "${region}" | \
  docker login --username AWS --password-stdin "$(dirname "${repo_uri}")"

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

