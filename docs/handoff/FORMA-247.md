# FORMA-247 source qualification — 2026-10-28

Historical existing-user qualification. The later invitation-first extension is recorded in `FORMA-247-INVITED-SIGNUP.md`; its new source/image and application acceptance are separate.

Base: `b750ca534ae89066714f100eb2b76ee71edf1ab1`; branch `codex/forma-247-hosted-auth`. This changes fAuth only; its published Forma 2/0.2 dependencies remain pinned. No GenUI checkout was modified.

Implemented: idempotent tenant-scoped OAuth client registration, exact redirect validation, branded email/password authorization-code flow with S256 and browser binding, bounded pending requests, one-use 60-second codes, atomic expiry/binding check at exchange, token-free browser login response, and explicit MFA refusal. Tenant provider buttons require actual tenant credentials. Health includes build-time source identity. The delivery directory `handoff/forma-247/` contains idempotent Northstar/canary provisioning and the exact app contract.

Validation on an isolated local PostgreSQL16 container at port5547, auth server3247, Node22.21.0, Rust stable, at most3 build jobs/2test threads:

- `npm ci --ignore-scripts --prefix admin`, `npm run typecheck --prefix admin`, `npm run build:ssr --prefix admin`: pass. Existing tracked SSR degradations remain; no frontend framework migration.
- `cargo test --locked -p gatewasm-server --lib --test integration -- --test-threads=2`:97unit pass;275integration pass,1ignored,1old assertion failed because it expected an unconfigured Google provider. That assertion was corrected to the ticket's required hidden-provider behavior.
- Subsequent `cargo test --locked -p gatewasm-server --test integration hosted -- --test-threads=2`:10/10pass, including new MFA/expiry/cap test and corrected provider test. Focused code-flow tests cover missing/wrong browser cookie, verifier/client/redirect mismatch, canary denial, concurrent replay, expired code, refresh and offline signature/claim verification.
- Provisioning executed twice against the disposable issuer. Both public receipts are identical; all four synthetic users authenticate and are logged out. Managed secret file was0600 and exists only outside Git for this synthetic run.
- `scripts/verify-hosted-code.mjs`:Chromium,Firefox,WebKit pass branded login, provider hiding, password code flow, Node backend exchange and independent public-JWKS verification. Injecting a synthetic access token into the browser login response makes the detector fail. This harness is issuer evidence, not the GenUI application's acceptance.
- Independent source review by Codex root found no outstanding production issue after redirect-forwarding prevention, pagination, request caps and replay hardening. Review is not hosted approval.

Raw logs/public receipts are retained in the ticket work area `/Users/victorvillacis/dev/Forma/work/FORMA-247/`. The first focused test invocation omitted the isolated database URL and failed to connect; the correctly configured run passed. The first browser attempt lacked pinned browser binaries; installed matching binaries. An early asynchronous browser response observer raced with navigation; replaced it with awaited interception and proved its negative control.

Hosted acceptance remains open. GateWasm AWS was torn down under the user's cost instruction. No source/image has been deployed, no public origin/callback has been selected for GenUI, no hosted test/spending window is authorized, and no real-organization sign-in is enabled. No passkey UI added. FORMA-248 remains mandatory. Rollback and migration limits are documented in the delivery README.
