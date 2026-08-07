# Handoff — prove it runs locally, then think about the cloud

**Written:** 2026-08-07, moving this work from a Windows machine to a Mac.
**Branch:** `fix/s0-restore-workspace-resolve` → PR #1. All 5 CI jobs green.

Everything is committed and pushed across all four repos. A fresh clone plus
this document is the whole context.

---

## 1. Yes, this is the right next step

Proving the stack runs end to end on a developer machine before returning to
distribution is the correct order, and this repo has earned the caution: it
could not build at all for four months, its CI was red for 20 consecutive runs,
and its e2e suite had *never once executed* — a missing `mailpit` binary killed
the run before the first test. All three are fixed, but none of it had been
demonstrated on a second machine, which is exactly what the Mac is for.

There is no deploy target to rush back to. App Runner is being sunset and the
old image predates every fix here, so nothing is waiting on this.

---

## 2. Where things are

| | |
|---|---|
| **e2e suite** | `admin/e2e/` — 21 spec files, Playwright. `global-setup.ts` spawns the server and waits on `/health`; `global-teardown.ts` kills the process tree. Tagged `@smoke` (44 tests, what CI runs) and `@regression` (~147). |
| **Frontend** | `admin/` — FormaJS + Tailwind, built by `admin/build.ts` via `@getforma/build`. |
| **Backend** | `crates/server/` — Rust/Axum. 97 unit tests, 274 integration tests in one `tests/integration.rs`. |
| **Database** | `docker-compose.yml` — `db` on **5434**, `db-test` on **5433**, plus pgweb on 8081/8082. |
| **Workspace** | `gatewasm.code-workspace` — multi-root (root / admin / server / infra) with tasks for every command below. Open this, not the folder. |

### One trap worth knowing

The Windows machine ran **native Postgres on 5432** and pointed
`TEST_DATABASE_URL` at it. That is *not* the repo's design. On the Mac use
docker-compose, which is what CI matches:

- dev DB → `postgres://gatewasm:gatewasm@localhost:5434/gatewasm`
- test DB → `postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test`

If a port looks wrong, trust `docker-compose.yml` over any `.env` you inherit.

---

## 3. Getting it running on the Mac

`.env` is gitignored, so it does not travel. Create it from `.env.example` and
generate real secrets — the app refuses to start without `CSRF_SECRET` and
`MFA_ENCRYPTION_KEY`:

```bash
git clone git@github.com:getforma-dev/gatewasm.git && cd gatewasm
git checkout fix/s0-restore-workspace-resolve

cp .env.example .env
# then set, with real values:
#   DATABASE_URL=postgres://gatewasm:gatewasm@localhost:5434/gatewasm
#   CSRF_SECRET=$(openssl rand -hex 32)
#   MFA_ENCRYPTION_KEY=$(openssl rand -hex 32)
#   PLATFORM_ADMIN_KEY=plat_$(openssl rand -hex 16)
#   SERVER_URL=http://localhost:3001
#   SUPPRESS_EMAIL=true

npm install && (cd admin && npm ci)
npm run db:test          # dev + test Postgres
npm run build:ssr        # MUST run before any cargo command — see below
npm run dev              # server + frontend watcher + mailpit
npm run db:seed          # test tenants and users
```

**`npm run build:ssr` before `cargo` is not optional.** `AdminAssets` embeds
`admin/dist/`, so with no `dist` the Rust build fails with a confusing
`AdminAssets: Embed is not satisfied`. This is why every CI job has a "Build
admin assets" step first.

Requirements: Docker Desktop, Rust ≥1.85 (Docker image pins 1.93), Node ≥20.19
(core 2.0.0's floor; CI uses 22). Mailpit is optional — the suite warns and
continues without it.

### What to prove works

```bash
npm test                              # 97 rust unit
npm run test:integration              # 274 integration (needs db-test)
npm run test:e2e:smoke                # 44 e2e
cd admin && npm run check:ssr         # SSR degradation gate
```

Then exercise it by hand, because the tests do not cover the *product*: sign up
a tenant through `/platform/onboarding`, log in at `/t/{slug}/login`, change
branding in the console and confirm the hosted login page reflects it, and add
a second tenant to confirm isolation. That is the part nobody has done on a
clean machine.

---

## 4. Outstanding, in priority order

1. **The compiler fix — `forma-tools/docs/HANDOFF-imported-signal-defaults.md`.**
   Fully specified, not started. Blocks SSR fidelity for every store-based app,
   and forma-tools is platform infrastructure — many products will build on it,
   so this is not a gatewasm-local concern.
2. **SSR migration for `/platform/login` and `/platform/onboarding`.** Held on
   Phase 1 behind `SSR_ROUTES` in `crates/server/src/ir/mod.rs`. Gated on (1).
   158 IR degradations app-wide, 33 on the login route. `SSR-MIGRATION-PLAN.md`
   has the corrected prerequisites.
3. **Re-land the warning-budget ratchet.** Reverted in `c7651c1`: the clippy
   collector read 26 locally and 0 in CI, and `|| true` swallowed the reason.
   The commit message carries the fix — fail loudly when the command emits
   nothing, and verify on a Linux runner before committing a baseline. The SSR
   gate is unaffected and still runs.
4. **Then, and only then, distribution.** The migration hazard in
   `crates/server/src/db/mod.rs` only bites a database that already carries
   `schema_migrations` rows 1–3; a fresh database applies them cleanly, which
   the local run proves. Wherever this lands next, start fresh or audit first.

Nothing here is blocking. The branch is green and self-consistent.

---

## 5. The habit worth keeping

Four separate checks in this work passed by doing nothing: a collector reading
stdout while the compiler wrote to stderr; the same collector reading a warm
cache as "26 warnings fixed"; a test asserting a case-sensitive literal against
a mixed-case payload; and a test whose request never reached the handler
because it lacked a tenant header.

Every one looked green. The only thing that caught them was breaking the fix on
purpose and confirming the check went red. Do that for anything new — a gate
nobody trusts is worse than no gate, because it also removes the suspicion that
something is unchecked. That is how a login page shipped a dead link for months
while the compiler printed the reason on every single build.
