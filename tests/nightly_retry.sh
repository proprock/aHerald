#!/bin/sh
# Contract test for scripts/nightly-retry-decision.sh: the jobs of a failed Nightly attempt in,
# "rerun" or "keep" out. A rerun is right only when every failed job (the final CI gate aside)
# never got a runner, and only for the first two attempts.
# Run from the repository root: sh tests/nightly_retry.sh
set -eu

script=scripts/nightly-retry-decision.sh
# Jobs of a real failed attempt (Nightly 2026-11-09): two macOS jobs waited 15 minutes for a runner
# and were cancelled (runner_id 0, no steps), and the CI gate failed because of them.
shortage=tests/fixtures/nightly-retry/runner-shortage.json
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0

# decide <name> <expected first word> <jobs file> <attempt>
decide() {
  name=$1
  expected=$2
  actual=$(sh "$script" "$3" "$4" 2> "$work/err.txt" | head -n 1 | cut -d: -f1) || actual=error
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $name: expected $expected, got $actual" >&2
    sed 's/^/  /' "$work/err.txt" >&2
    failures=$((failures + 1))
  fi
}

# Variants of the real fixture, built with jq.
# A job that ran and failed: it has a runner and steps.
jq '.jobs |= map(if .conclusion == "cancelled" then .conclusion = "success" else . end)
  | (.jobs[] | select(.name == "Full CI / Build and test (ubuntu-22.04)")) |= (.conclusion = "failure")' \
  "$shortage" > "$work/ubuntu-failed.json"
# The two shortage jobs plus a real failure.
jq '(.jobs[] | select(.name == "Full CI / Build and test (ubuntu-22.04)")) |= (.conclusion = "failure")' \
  "$shortage" > "$work/mixed.json"
# A job that did get a runner and then was cancelled (timeout while running): not a shortage.
jq '(.jobs[] | select(.name == "Full CI / Build and test (macos-15)")) |= (.runner_id = 1000009999 | .runner_name = "GitHub Actions 1000009999" | .steps = [{"name": "Set up job", "conclusion": "success"}])' \
  "$shortage" > "$work/ran-then-cancelled.json"
# Only the gate failed.
jq '.jobs |= map(if .conclusion == "cancelled" then .conclusion = "success" else . end)' \
  "$shortage" > "$work/gate-only.json"
# Nothing failed.
jq '.jobs |= map(.conclusion = "success")' "$shortage" > "$work/green.json"
# A timed-out job that never started also counts.
jq '(.jobs[] | select(.name == "Full CI / MSRV build (macos-15)")) |= (.conclusion = "timed_out")' \
  "$shortage" > "$work/timed-out.json"

decide "a runner shortage on the first attempt is rerun" rerun "$shortage" 1
decide "a runner shortage on the second attempt is rerun once more" rerun "$shortage" 2
decide "a runner shortage on the third attempt is not rerun again" keep "$shortage" 3
decide "a failed test is not rerun" keep "$work/ubuntu-failed.json" 1
decide "a real failure next to a shortage is not rerun" keep "$work/mixed.json" 1
decide "a job that had a runner and was cancelled is not rerun" keep "$work/ran-then-cancelled.json" 1
decide "a failed gate alone is not rerun" keep "$work/gate-only.json" 1
decide "a green attempt is not rerun" keep "$work/green.json" 1
decide "a queue timeout counts as a shortage" rerun "$work/timed-out.json" 1

# Bad input is an error with a message, never a rerun.
refuse() {
  name=$1
  shift
  if out=$(sh "$script" "$@" 2> "$work/err.txt"); then
    echo "FAIL: $name: expected a non-zero exit, got: $out" >&2
    failures=$((failures + 1))
  elif [ ! -s "$work/err.txt" ]; then
    echo "FAIL: $name: expected a message on stderr" >&2
    failures=$((failures + 1))
  fi
}
printf '{ not json' > "$work/broken.json"
printf '{"jobs": []}' > "$work/empty.json"
refuse "a missing file" "$work/absent.json" 1
refuse "broken JSON" "$work/broken.json" 1
refuse "no jobs at all" "$work/empty.json" 1
refuse "a non-numeric attempt" "$shortage" first
refuse "a missing attempt" "$shortage"

if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "nightly_retry: all checks passed"
