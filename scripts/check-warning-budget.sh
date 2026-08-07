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
# Usage:
#   scripts/check-warning-budget.sh           # gate against the baseline
#   scripts/check-warning-budget.sh --update  # re-record after fixing some
set -uo pipefail
cd "$(dirname "$0")/.."

BASELINE_FILE="scripts/warning-budget.txt"
FAILED=0
declare -A CURRENT

# ── clippy ────────────────────────────────────────────────────────────────
# Counts every lint, not just string_slice. clippy::string_slice is what found
# the unauthenticated process abort in the OAuth token endpoint, and it was
# only visible because the count was zero — a new warning stood out. Keeping
# the total ratcheting down preserves that property for every other lint.
CURRENT[clippy]=$(cargo clippy -p gatewasm-server --lib --message-format=short 2>&1 \
  | grep -c "^.*: warning:" || true)

# ── bundle size budgets ───────────────────────────────────────────────────
# Emitted per route by @getforma/build. Over budget means a slower first paint
# for every visitor to that route.
CURRENT[bundle]=$( (cd admin && npx tsx build.ts --ssr 2>&1) \
  | grep -c "Budget exceeded" || true)

# `admin/scripts/check-ir-degradations.mjs` owns the SSR stream — it needs
# per-file granularity rather than a single count, so it is not folded in here.

if [[ "${1:-}" == "--update" ]]; then
  : > "$BASELINE_FILE"
  for k in "${!CURRENT[@]}"; do echo "$k=${CURRENT[$k]}" >> "$BASELINE_FILE"; done
  sort -o "$BASELINE_FILE" "$BASELINE_FILE"
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
  now="${CURRENT[$key]:-0}"
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
