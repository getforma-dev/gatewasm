#!/usr/bin/env python3
"""Rotate one tenant management key; secret values never enter arguments or receipts.

Requires a single named operator, AWS Secrets Manager permissions, and the platform
key supplied via FAUTH_PLATFORM_KEY. Use the SAME --operation-id when retrying.
"""
import argparse
import json
import os
import secrets
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid


class Failure(Exception):
    pass


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class AwsStore:
    def __init__(self, secret_id, region):
        self.secret_id, self.region = secret_id, region

    def call(self, action, **values):
        # stdin, never command-line arguments, carries any secret value.
        payload = {"SecretId": self.secret_id, **values}
        result = subprocess.run(["aws", "secretsmanager", action, "--cli-input-json",
                                 "file:///dev/stdin", "--region", self.region, "--output", "json"],
                                input=json.dumps(payload), text=True, capture_output=True)
        if result.returncode:
            raise Failure("Managed secret operation failed; retry with the same operation ID after resolving access")
        return json.loads(result.stdout)

    def versions(self):
        return self.call("describe-secret")["VersionIdsToStages"]

    def get(self, version=None):
        value = self.call("get-secret-value", **({"VersionId": version} if version else {"VersionStage": "AWSCURRENT"}))
        return value["VersionId"], json.loads(value["SecretString"])

    def stage(self, operation, value):
        self.call("put-secret-value", ClientRequestToken=operation,
                  VersionStages=["AWSPENDING"], SecretString=json.dumps(value))

    def promote(self, operation, previous):
        self.call("update-secret-version-stage", VersionStage="AWSCURRENT",
                  MoveToVersionId=operation, RemoveFromVersionId=previous)

    def finish(self, operation):
        if "AWSPENDING" in self.versions().get(operation, []):
            self.call("update-secret-version-stage", VersionStage="AWSPENDING", RemoveFromVersionId=operation)


def http_request(origin, method, path, key, body=None):
    req = urllib.request.Request(origin + path, method=method,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        data=json.dumps(body).encode() if body is not None else None)
    try:
        with urllib.request.build_opener(NoRedirect()).open(req, timeout=30) as res:
            data = res.read()
            return res.status, json.loads(data) if data else {}
    except urllib.error.HTTPError as error:
        return error.code, {}  # Do not copy server bodies into operational logs.
    except Exception:
        raise Failure("Auth request failed; retry with the same operation ID") from None


def rotate(store, origin, platform_key, operation, request=http_request):
    versions = store.versions()
    if any(version != operation and "AWSPENDING" in stages for version, stages in versions.items()):
        raise Failure("Another pending rotation exists; resume its operation ID first")
    current_id, current = store.get()
    if operation in versions:
        _, pending = store.get(operation)
        previous_id = pending.get("_rotation", {}).get("previous_version")
        if not previous_id or current_id not in (previous_id, operation):
            raise Failure("Operation is obsolete or lacks recovery metadata; refusing to overwrite a later rotation")
        _, previous = store.get(previous_id)
    else:
        previous_id, previous = current_id, current
        pending = {**current, "management_key": "mgmt_" + secrets.token_urlsafe(32),
                   "_rotation": {"previous_version": previous_id}}
        store.stage(operation, pending)
    tenant = pending["tenant_id"]
    if tenant != previous["tenant_id"]:
        raise Failure("Secret tenant identity changed")
    status, response = request(origin, "POST", "/platform/tenants/" + urllib.parse.quote(tenant, safe="") + "/rotate-key",
                               platform_key, {"next_key": pending["management_key"]})
    if status != 200 or response.get("rotated") is not True or "api_key" in response:
        raise Failure(f"Auth key switch failed (HTTP {status}); pending version retained for recovery")
    if current_id != operation:
        store.promote(operation, previous_id)
    version, value = store.get()
    if version != operation or value["management_key"] != pending["management_key"]:
        raise Failure("Current managed secret does not match the staged credential")
    new_status, _ = request(origin, "GET", "/manage/config", pending["management_key"])
    old_status, _ = request(origin, "GET", "/manage/config", previous["management_key"])
    if new_status != 200 or old_status != 401:
        raise Failure("Rotation verification failed; retain both versions and investigate without starting another rotation")
    store.finish(operation)
    return {"operation_id": operation, "tenant_id": tenant, "secret_version": version,
            "new_key_http": new_status, "revoked_key_http": old_status, "secret_values_logged": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--auth-origin", required=True)
    parser.add_argument("--secret-id", required=True)
    parser.add_argument("--region", default="us-east-1")
    parser.add_argument("--operation-id", required=True, help="Fresh UUID for a new rotation; reuse exactly on retry")
    args = parser.parse_args()
    if str(uuid.UUID(args.operation_id)) != args.operation_id:
        raise Failure("Use a canonical UUID operation ID")
    origin = urllib.parse.urlsplit(args.auth_origin)
    if origin.scheme != "https" or not origin.netloc or origin.username or origin.password or origin.path not in ("", "/") or origin.query or origin.fragment:
        raise Failure("auth-origin must be an HTTPS origin without credentials, path, query or fragment")
    key = os.environ.get("FAUTH_PLATFORM_KEY")
    if not key:
        raise Failure("Supply FAUTH_PLATFORM_KEY through the approved secret environment")
    receipt = rotate(AwsStore(args.secret_id, args.region), args.auth_origin.rstrip("/"), key, args.operation_id)
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (Failure, ValueError, KeyError) as exc:
        # Failure descriptions are fixed or contain only status codes, never credentials.
        print(str(exc) if isinstance(exc, Failure) else "Invalid rotation configuration", file=sys.stderr)
        sys.exit(1)
