#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${BASE_URL:-}" ]]; then
  echo "BASE_URL is required (example: https://auth-staging.getforma.dev)"
  exit 1
fi

tenant_slug="${TENANT_SLUG:-platform}"
password="${SMOKE_PASSWORD:-Sm0keTest!Passw0rd}"
email="smoke+$(date +%s)@getforma.dev"

base_url="${BASE_URL%/}"
auth_base="${base_url}/t/${tenant_slug}/auth"

echo "Running smoke auth flow against ${auth_base} with ${email}"

register_payload="$(jq -nc \
  --arg email "${email}" \
  --arg password "${password}" \
  '{email:$email,password:$password,name:"Smoke Test"}')"

register_resp="$(curl -sS -f -X POST "${auth_base}/register" \
  -H 'content-type: application/json' \
  --data "${register_payload}")"

access_token="$(echo "${register_resp}" | jq -r '.access_token')"
refresh_token="$(echo "${register_resp}" | jq -r '.refresh_token')"

if [[ -z "${access_token}" || "${access_token}" == "null" ]]; then
  echo "Register response missing access_token"
  exit 1
fi
if [[ -z "${refresh_token}" || "${refresh_token}" == "null" ]]; then
  echo "Register response missing refresh_token"
  exit 1
fi

curl -sS -f "${auth_base}/me" \
  -H "authorization: Bearer ${access_token}" > /dev/null

mfa_setup_resp="$(curl -sS -f -X POST "${auth_base}/mfa/setup" \
  -H 'content-type: application/json' \
  -H "authorization: Bearer ${access_token}" \
  --data '{"rotate":false}')"

mfa_secret="$(echo "${mfa_setup_resp}" | jq -r '.secret')"
if [[ -z "${mfa_secret}" || "${mfa_secret}" == "null" ]]; then
  echo "MFA setup did not return secret"
  exit 1
fi

mfa_code="$(python3 - "${mfa_secret}" <<'PY'
import base64
import hashlib
import hmac
import struct
import sys
import time

secret = sys.argv[1].strip().upper().replace(" ", "")
padding = "=" * ((8 - len(secret) % 8) % 8)
key = base64.b32decode(secret + padding)
counter = int(time.time() // 30)
msg = struct.pack(">Q", counter)
digest = hmac.new(key, msg, hashlib.sha1).digest()
offset = digest[-1] & 0x0F
binary = ((digest[offset] & 0x7F) << 24) | ((digest[offset + 1] & 0xFF) << 16) | ((digest[offset + 2] & 0xFF) << 8) | (digest[offset + 3] & 0xFF)
print(str(binary % 1000000).zfill(6))
PY
)"

mfa_verify_resp="$(curl -sS -f -X POST "${auth_base}/mfa/verify" \
  -H 'content-type: application/json' \
  -H "authorization: Bearer ${access_token}" \
  --data "$(jq -nc --arg code "${mfa_code}" '{code:$code}')")"

mfa_access_token="$(echo "${mfa_verify_resp}" | jq -r '.access_token')"
if [[ -z "${mfa_access_token}" || "${mfa_access_token}" == "null" ]]; then
  echo "MFA verify response missing access_token"
  exit 1
fi

refresh_resp="$(curl -sS -f -X POST "${auth_base}/token" \
  -H 'content-type: application/json' \
  --data "$(jq -nc --arg rt "${refresh_token}" '{grant_type:"refresh_token",refresh_token:$rt}')")"

new_refresh_token="$(echo "${refresh_resp}" | jq -r '.refresh_token')"
if [[ -z "${new_refresh_token}" || "${new_refresh_token}" == "null" ]]; then
  echo "Refresh flow failed to return rotated refresh token"
  exit 1
fi

me_resp="$(curl -sS -f "${auth_base}/me" \
  -H "authorization: Bearer ${mfa_access_token}")"
mfa_verified="$(echo "${me_resp}" | jq -r '.mfa_verified')"

if [[ "${mfa_verified}" != "true" ]]; then
  echo "Expected mfa_verified=true, got ${mfa_verified}"
  exit 1
fi

echo "Smoke auth flow passed for tenant ${tenant_slug}"

