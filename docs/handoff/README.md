# Start here

You are picking up `gatewasm` on a new machine. Everything needed is in this
repo — clone it, read this page, then work the tasks in order.

Branch: **`fix/s0-restore-workspace-resolve`** → PR #1, all 5 CI jobs green.
All four Forma repos are committed and pushed; nothing is in flight.

| doc | what it is |
|---|---|
| [`MACHINE-SETUP.md`](MACHINE-SETUP.md) | Clone → running stack on a new machine. Where the e2e suite lives, the two traps a fresh clone cannot infer, what to prove by hand. Named to avoid colliding with the repo's own `LOCAL-DEV.md`. |
| `forma-tools/docs/HANDOFF-imported-signal-defaults.md` | The compiler gap blocking SSR fidelity. Diagnosed, specified, not started. Lives in the repo the fix lands in; the travelling copy that used to sit here was deleted once both repos were cloned, per its own instruction. |

### The repo's own docs — read these too

These predate this work and are still accurate. The handoff deliberately does
not duplicate them.

| doc | why you want it |
|---|---|
| `GETTING-STARTED.md` | **The manual test script for task 1.** Four user types, four end-to-end flows (self-serve signup, tenant login, owner invite, admin user management), and a full route reference with credentials. |
| `LOCAL-DEV.md` | The repo's own dev guide — env vars, creating tenants, API usage, troubleshooting. Longer than `MACHINE-SETUP.md`, which only covers what a *new machine* needs. |
| `CONTRIBUTING.md` | Full command reference and when to reach for which reset. |
| `BACKLOG.md` | Tracked debt that is not blocking. Two entries matter to you — see below. |
| `SSR-MIGRATION-PLAN.md` | Corrected. Three of its prerequisites were false and had sent earlier work down a dead end. |
| `RUNBOOK.md` | Deploy procedures. For later — the App Runner target is being sunset. |

### Two backlog entries that will bite on a Mac

`BACKLOG.md` flags that visual-regression baselines are named `*-darwin.png` —
they were generated on macOS. That works in your favour locally and is exactly
why `@visual` tests would fail on Linux CI; do not regenerate them to match a
Linux runner without reading that entry first.

It also flags every route shipping 310–424KB of JS against a 195KB budget,
which is the source of the 16 "Budget exceeded" warnings on every build.

---

## Tasks, in order

### 1. Stand the stack up locally and prove it works

This is yours to do, not something already done for you. The Windows machine it
came from used native Postgres on 5432; **use docker-compose instead** (dev on
5434, test on 5433) — that is the repo's design and what CI matches.

Follow `MACHINE-SETUP.md`, then confirm all four suites pass: 97 rust unit, 274
integration, 44 e2e smoke, and the SSR degradation gate.

Then exercise the product by hand, which no test covers: onboard a tenant, log
in at `/t/{slug}/login`, change branding in the console and confirm the hosted
login page reflects it, add a second tenant and confirm isolation.

`gatewasm.code-workspace` in the repo root is a multi-root VS Code workspace
(root / admin / server / infra) with tasks wired for every command. Open that
rather than the folder. Adjust it to suit your setup and commit the change — it
was written on Windows and has not been opened on macOS.

**Why this comes first:** this repo could not build for four months, its CI was
red for 20 consecutive runs, and its e2e suite had never once executed. All
three are fixed, but only ever demonstrated on one machine.

### 2. The compiler fix

`forma-tools/docs/HANDOFF-imported-signal-defaults.md`. Self-contained: cause,
the five existing APIs to reuse, six tests to write, hazards. Roughly 40–60
lines plus tests.

Lands in `forma-tools`, which is platform infrastructure — `ksx` and `gatewasm`
consume it today and more products are intended to, so keep it strictly
additive.

### 3. SSR migration for `/platform/login` and `/platform/onboarding`

Gated on (2). Both are held on Phase 1 by `SSR_ROUTES` in
`crates/server/src/ir/mod.rs`. 158 IR degradations app-wide, 33 on login.

### 4. Run the full regression suite once

CI only runs `--grep @smoke` (44 tests). The `@regression` tag covers ~147 and
**has never been run to completion on any machine** — the suite could not start
at all until this branch fixed it. Run `npm run test:e2e` once the stack is up;
expect to find genuine breakage, since nothing has been exercising those paths.

### 5. Re-land the warning-budget ratchet

Reverted in `c7651c1` because its clippy collector read 26 locally and 0 in CI
while `|| true` swallowed the reason. The fix is in that commit message: fail
loudly when the command emits nothing, and verify on a Linux runner before
committing a baseline. The SSR gate is unaffected and still running.

---

## One habit to carry over

Four checks in this work passed by doing nothing: a collector reading stdout
while the compiler wrote to stderr; the same one reading a warm cache as "26
warnings fixed"; a test asserting a case-sensitive literal against a mixed-case
payload; and a test whose request never reached the handler for want of a tenant
header.

All four looked green. The only thing that caught them was breaking the fix on
purpose and confirming the check went red. Do that for anything new — a gate
nobody trusts is worse than no gate, because it also removes the suspicion that
something is unchecked. That is how the login page shipped a dead link for
months while the compiler printed the reason on every build.
