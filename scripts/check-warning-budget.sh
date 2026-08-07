#!/usr/bin/env bash
# Ratchet on compiler and linter warnings.
#
# Every warning stream in this repo has a committed baseline that can only go
# down. That is the general form of the fix for a specific failure: the SSR
# compiler had been printing "the attribute is OMITTED server-side" for
# /platform/login's onboarding link on every build for months, and the page
# shipped a dead link the whole time because nobody reads 158 lines of warnings.
#
# A warning nobody is forced to read is not a warning. This makes each stream
# fail the build when it grows, and asks you to re-record when it shrinks, so
# the number is always honest.
#
# This is the second landing. The first (reverted in c7651c1) read 26 locally
# and 0 in CI because `|| true` swallowed whatever the collector's command did
# wrong. The rule now: a collector whose command fails, or whose output carries
# no evidence the tool actually ran, ABORTS the gate — it never reports zero.
# Empty output is an infrastructure failure, not a clean bill.
#
# Usage:
#   scripts/check-warning-budget.sh           # gate against the baseline
#   scripts/check-warning-budget.sh --update  # re-record after fixing some
set -uo pipefail
cd "$(dirname "$0")/.."

BASELINE_FILE="scripts/warning-budget.txt"
FAILED=0
# Plain variables, not `declare -A`: macOS ships bash 3.2, and the first
# landing of this script only ever ran on runners with bash 4+. A gate the
# dev machine cannot run is a gate nobody rehearses.
CURRENT_clippy=""
CURRENT_bundle=""

current_of() {
  case "$1" in
    clippy) printf '%s' "$CURRENT_clippy" ;;
    bundle) printf '%s' "$CURRENT_bundle" ;;
    *) printf '' ;;
  esac
}

# Run one collector command; abort the gate unless the output proves the tool
# ran to completion. $1=stream name, $2=regex the output must contain for the
# run to count as evidence, rest=command.
collect() {
  local key="$1" evidence="$2"
  shift 2
  local out status
  out=$("$@" 2>&1)
  status=$?
  if (( status != 0 )); then
    echo "::error::$key collector exited $status — refusing to report a count" >&2
    printf '%s\n' "$out" | tail -20 >&2
    exit 1
  fi
  if ! grep -qE "$evidence" <<<"$out"; then
    echo "::error::$key collector produced no evidence of a completed run (expected /$evidence/) — that is an infrastructure failure, not zero warnings" >&2
    printf '%s\n' "$out" | tail -20 >&2
    exit 1
  fi
  printf '%s\n' "$out"
}

# ── clippy ────────────────────────────────────────────────────────────────
# Counts every lint, not just string_slice. clippy::string_slice is what found
# the unauthenticated process abort in the OAuth token endpoint, and it was
# only visible because the count was zero — a new warning stood out. Keeping
# the total ratcheting down preserves that property for every other lint.
#
# `Finished` is the evidence: clippy that could not run (missing component,
# broken workspace) never prints it, and that used to read as 0 warnings.
#
# `cargo clean -p` first: a warm cache (rust-cache on CI, a prior local run)
# leaves the crate fingerprint-fresh and clippy prints NOTHING for it — 25
# real warnings read as 0 with full completion evidence. Cleaning only the
# primary crate forces the re-lint while every dependency stays cached.
# CARGO_TERM_COLOR=never: CI exports `always`, which threads ANSI escapes
# through the middle of every ': warning:' — the count read 0 against 25 real
# warnings while the 'Finished' evidence (an intact word) still passed. The
# counter and the evidence check must see the same bytes a plain pipe sees.
cargo clean -p gatewasm-server 2>/dev/null || true
CLIPPY_OUT=$(collect clippy 'Finished|error\[' env CARGO_TERM_COLOR=never cargo clippy -p gatewasm-server --lib --message-format=short)
CURRENT_clippy=$(grep -c "^.*: warning:" <<<"$CLIPPY_OUT" || true)

# ── bundle size budgets ───────────────────────────────────────────────────
# Emitted per route by @getforma/build. Over budget means a slower first paint
# for every visitor to that route. `Manifest written` is the build's own
# completion line — no manifest, no build, no count.
BUNDLE_OUT=$(collect bundle 'Manifest written' bash -c 'cd admin && npx tsx build.ts --ssr')
CURRENT_bundle=$(grep -c "Budget exceeded" <<<"$BUNDLE_OUT" || true)

# `admin/scripts/check-ir-degradations.mjs` owns the SSR stream — it needs
# per-file granularity rather than a single count, so it is not folded in here.

if [[ "${1:-}" == "--update" ]]; then
  : > "$BASELINE_FILE"
  for k in bundle clippy; do echo "$k=$(current_of "$k")" >> "$BASELINE_FILE"; done
  echo "Recorded warning budget:"
  cat "$BASELINE_FILE"
  exit 0
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "No baseline at $BASELINE_FILE — run with --update to record one." >&2
  exit 1
fi

while IFS='=' read -r key was; do
  [[ -z "$key" ]] && continue
  now="$(current_of "$key")"
  now="${now:-0}"
  if (( now > was )); then
    echo "::error::$key warnings rose from $was to $now" >&2
    echo "  These are not cosmetic. The SSR stream shipped a dead link for months" >&2
    echo "  precisely because nobody was forced to read it." >&2
    FAILED=1
  elif (( now < was )); then
    echo "$key warnings improved: $was -> $now — re-record with --update" >&2
    FAILED=1
  else
    echo "$key: $now (baseline $was)"
  fi
done < "$BASELINE_FILE"

exit "$FAILED"
