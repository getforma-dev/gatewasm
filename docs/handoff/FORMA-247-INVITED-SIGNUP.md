# FORMA-247 invitation-first hosted signup

Base: `737c73f88f2b89e8712e4cd1f5dbf40163ab6081`; branch `codex/forma-247-hosted-signup`. This is an identity-only extension in GateWasm. GenUI owns its adapter, organizations, invite/trial entitlements, sessions and activation policy. No GenUI files or cloud resources are changed.

## Change record

Previously the hosted PKCE page could sign in existing users but suppressed registration. Ordinary registration issued access/refresh material to the browser and could not resume an application authorization request. The new path carries a backend-pushed fAuth invitation into the browser-bound request, creates or admits the invited identity, and returns only the one-use code callback. It never creates refresh material on a hosted browser submission.

- Added optional `RegisterRequest.authorization_request`; preserves the legacy registration response when that field is absent. Empty/present invalid values cannot fall through.
- Added server-derived signup eligibility and an invitation-first form. Hosted submissions send no raw invitation token and cannot choose an authoritative role or tenant.
- Email PAR stores an invitation hash, validates current pending invitation scope, uses bounded storage/rate limiting, and consumes atomically with hosted request creation. No PAR handle logging. Existing federation PAR remains supported.
- Registration/invitation/code changes are transactional; existing-member role acceptance occurs only with a validated code transaction. Duplicate users retain their pending continuation for sign-in. Current provider/MFA policy, registered callback and actual-clock expiry are rechecked; no MFA fallback.
- Added `delivery_mode: server_only` to authenticated invitation creation. It requires an explicit tenant role, suppresses legacy email, omits the URL and returns the raw invitation token only to the authorized backend. Legacy email mode remains default. Invalid mode/role is refused before invitation writes.
- Added migration 007. Before rollback, stop hosted signup admissions, retain `invite_only`, restore matching frontend/backend assets and invalidate pending PAR/hosted requests. Old binaries do not enforce the new invitation binding; clearing rows alone is insufficient for an open tenant and cached new UI. The previously qualified image remains an existing-user artifact and does not qualify this new source.

The precise payload/status, browser/backend boundary, rollback and deployment conditions are in `handoff/forma-247/README.md`.

## Qualification

Evidence is retained under `/Users/victorvillacis/dev/Forma/work/FORMA-247/evidence/` and the scoped browser evidence directory. Local checks use the owned PostgreSQL16 container `forma247-signup-pg` on loopback5547, no host data mount, maximum3 Rust build jobs and2 test threads. SMTP evidence uses an ephemeral loopback server and a positive legacy-delivery control, with no external mail.

Initial focused hosted suite:17passed. Initial full run:106unit passed,288integration passed,1ignored,1 historical PAR error-message assertion failed after the lookup became client/tenant-bound. Updated that assertion to generic refusal and proved the legitimate PAR request remains unconsumed. A subsequent source review preserved post-success platform bookkeeping and locked the callback through transaction commit. Final focused checks:18 hosted integration tests and7 PAR tests pass, including the fixed legacy assertion. All106 unit tests passed in the full run. Repository `scripts/check-warning-budget.sh` passes with25 inherited Clippy warnings and0 bundle warnings, exactly the committed baseline. A broad all-warnings-as-errors trial failed on inherited warnings plus one newly introduced boolean style warning; only the new warning was fixed. The production local binary builds successfully.

Three actual source mutations were detected by tests: disabling invitation email equality, returning an extra access-token field, and replacing post-lock/actual-clock expiry with transaction-start validity. Each produced an assertion failure, not a build failure. The original source was restored and the final focused suite rerun. The subsequent equivalent `is_none_or` cleanup was reviewed independently. All12 local browser cases pass (four each in Chromium, Firefox and WebKit), including invited signup, existing login, empty-state refusal and legacy invitation UI. Injecting an access token into a real successful signup response fails the response-shape assertion. Independent backend/UI review approved this source. Final PR CI and immutable image qualification remain separate gates.

Hosted availability, actual KMS custody/rotation, external mailbox activation and the GenUI application integration are separate gates. The human-approved rehearsal is limited to six hours/$5 within the combined $100 monthly cap; no paid resources may be created until the agreed callback and application test environment are ready. This local source work does not start that window.
