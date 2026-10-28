# FORMA-247 hosted authentication handoff

This directory is a reviewed delivery artifact for GenUI to incorporate beside its demo. It does not modify the application repository. `config.example.json` is explicitly synthetic and loopback-only. GenUI must supply the real app origin and exact callback before any hosted provisioning. No production hostname has been guessed.

## Contract

Register a tenant-scoped client with `POST /manage/oauth-clients` authenticated by its management key. The body is `{client_id, name, redirect_uri}`. One exact HTTPS redirect is permitted per identifier (loopback HTTP is allowed only when the issuer itself runs over local HTTP). Identical requests return 200 after the first 201; differing configuration or another tenant's identifier returns 409. Client identifiers ending in `-hosted` are reserved for fAuth's existing hosted login. `GET /manage/oauth-clients` lists only the authenticated tenant's clients.

Start the password code flow by navigating the browser to `/t/<slug>/auth/authorize?provider=email&client_id=...&redirect_uri=...&response_type=code&code_challenge=...&code_challenge_method=S256&state=...`. The platform must generate and bind a fresh high-entropy state and verifier to its own secure pre-login session. fAuth redirects to its existing branded sign-in page with a browser-bound, five-minute request. Credentials are checked by the existing password handler, including its rate limiter and account lockout. Success returns only a callback URL containing a one-use, 60-second code and the unchanged state; no access or refresh token is returned to browser JavaScript for this flow.

The platform validates state, then posts JSON to `/t/<slug>/auth/token` from its backend with `grant_type: authorization_code`, `code`, `code_verifier`, `client_id` and the exact `redirect_uri`. The backend stores the returned refresh token securely and keeps browser sessions opaque. Verify `alg=EdDSA`, `kid` against `/auth/jwks`, exact configured `iss` and `aud`, expiry, and `tenant_id`/`sub` before using roles. The existing default issuer/audience are deployment settings (`gatewasm` / `gatewasm-app`), not inferred URL values. Use the deployed receipt. Canary tokens must never authorize a Northstar resource.

This first password code adapter refuses tenants with an enabled MFA policy, individual MFA-enabled users, and compliance-required MFA. The demo selects `mfa_policy: off`; disabling a real account's MFA to make this flow work is not supported. Provider federation retains its existing flow. No passkey UI is added. Browser-bound requests use one cookie per tenant, so starting another sign-in in the same browser/tenant supersedes the previous pending request. Requests are rate limited and capped at 1,000 pending records per tenant.

## Provisioning

Use Python 3 with standard-library modules. `FAUTH_PLATFORM_KEY` and password variables named in the config must be supplied through the approved secret environment. The script prints a public receipt only. Hosted runs read/write tenant management keys through AWS Secrets Manager using the current CLI identity and selected region; use a dedicated least-privilege secret prefix. Secret values never appear in process arguments, tickets or receipts.

```
python3 provision.py --config reviewed-hosted-config.json --region us-east-1 --secret-prefix forma-demo/management
```

Run twice and retain both public receipts. A tenant that exists without its recorded secret, a mismatched subject/role, wrong password, changed client redirect, or missing environment input stops the run; there is no silent key rotation, password reset, or fallback. Existing users' passwords are not changed. Each synthetic login probe is immediately logged out. Both tenants are invite-only, email-only, and use explicitly selected branding, origins and callbacks. OAuth provider buttons remain hidden without credentials for that tenant.

For disposable local evidence only, pass `--synthetic-secret-file /absolute/private/path.json` (0600; loopback issuer required). Treat this file as a secret, keep it outside Git, and remove it with the disposable environment. The checked-in example contains no passwords or tokens.

## Deployment and acceptance

The previous GateWasm AWS application was torn down at the user's request on 2026-10-28. Source changes do not establish hosted availability. Before rebuilding, agree on the test/cost window, recover or reissue the external provider/app secrets, recreate the image repository, build the reviewed source, record the immutable image digest and deploy the isolated environment. `build_and_push_image.sh` embeds the clean source revision; `/health` exposes `source_revision` alongside database health. An `unrecorded` value is not deployment-identity evidence.

FORMA-248 KMS custody remains required before real-organization sign-in. A local synthetic success does not satisfy that gate. GenUI owns its platform adapter, browser session handling, tenant authorization and Chrome/Safari/Firefox integrated acceptance. Return exact source/PR, image digest, endpoint, issuer, audience, public JWKS, tenant/client identifiers, redirect configuration, two provisioning receipts, browser results, CI/review and rollback identity. Keep tickets open for any unverified hosted or consumer checks.

Rollback to the previously reviewed source/image is additive except for migration 005 (pending requests only); expire/delete those temporary requests if rolling back. Retain registered tenants/client rows and managed secrets. A file-key rollback is not acceptable custody for real organizations. Restore ordinary rate-limit settings after rehearsal; this artifact does not alter them automatically.
