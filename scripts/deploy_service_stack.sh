#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <prod|staging> <image-tag>"
  exit 1
fi

target_env="$1"
image_tag="$2"
extra_context=()

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd "${script_dir}/.." && pwd)"
infra_dir="${project_root}/infra"

case "${target_env}" in
  prod)
    stack_name="GatewasmServiceStack"
    ;;
  staging)
    stack_name="GatewasmStagingServiceStack"
    extra_context=(-c gatewasm:staging:enabled=true -c gatewasm:staging:imageTag="${image_tag}")
    if [[ -n "${STAGING_SECRET_ARNS_JSON:-}" ]]; then
      extra_context+=(-c "gatewasm:staging:secretArns=${STAGING_SECRET_ARNS_JSON}")
    fi
    if [[ -n "${STAGING_ALERTS_EMAIL:-}" ]]; then
      extra_context+=(-c "gatewasm:staging:alertsEmail=${STAGING_ALERTS_EMAIL}")
    fi
    if [[ -n "${STAGING_OWNER_SESSION_SECRET_ARN:-}" ]]; then
      extra_context+=(-c "gatewasm:staging:ownerSessionSecretArn=${STAGING_OWNER_SESSION_SECRET_ARN}")
    fi
    ;;
  *)
    echo "Invalid environment: ${target_env}"
    exit 1
    ;;
esac

cd "${infra_dir}"

if [[ ! -d node_modules ]]; then
  npm install
fi

echo "Deploying ${stack_name} with image tag ${image_tag}..."
deploy_cmd=(
  npx cdk deploy "${stack_name}"
  --require-approval never
  -c "gatewasm:imageTag=${image_tag}"
)

if [[ ${#extra_context[@]} -gt 0 ]]; then
  deploy_cmd+=("${extra_context[@]}")
fi

"${deploy_cmd[@]}"

rollback_file="${project_root}/deploy/rollback-targets.log"
mkdir -p "$(dirname "${rollback_file}")"
printf '%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${target_env}" "${image_tag}" >> "${rollback_file}"

echo "Recorded rollback target in ${rollback_file}"
