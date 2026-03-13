#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <prod|staging> <image-tag>"
  exit 1
fi

target_env="$1"
image_tag="$2"
repo_name="${ECR_REPOSITORY_NAME:-gatewasm-server}"
region="${AWS_REGION:-us-east-1}"

echo "Validating image tag ${image_tag} exists in ECR..."
aws ecr describe-images \
  --repository-name "${repo_name}" \
  --region "${region}" \
  --image-ids "imageTag=${image_tag}" \
  --query 'imageDetails[0].imageDigest' \
  --output text > /dev/null

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"${script_dir}/deploy_service_stack.sh" "${target_env}" "${image_tag}"

echo "Rollback complete: ${target_env} now points to ${image_tag}"

