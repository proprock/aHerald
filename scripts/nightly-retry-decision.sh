#!/bin/sh
# Decides whether a failed Nightly attempt is worth rerunning. Used by
# .github/workflows/nightly-retry.yml; safe to run by hand.
#
#   scripts/nightly-retry-decision.sh JOBS_JSON ATTEMPT
#
# JOBS_JSON holds the answer of `GET /repos/{repo}/actions/runs/{id}/attempts/{n}/jobs`
# (an object with a `jobs` array) and ATTEMPT is the number of that attempt. The first line of
# the output is `rerun` or `keep`, followed by the reason; the exit status is 0 for both.
#
# A rerun is chosen only when every failed job never got a runner: a job that waited for a
# macOS runner until its time limit and was cancelled has `runner_id` 0 and no steps, while a real
# failure always ran on a runner and has steps. The final `CI gate` job fails because of the
# jobs before it and is not counted. At most two reruns: an attempt number of 3 or more is kept.
# Bad input is an error with a message (exit status 1), never a rerun. Needs jq.
set -eu

# Reruns allowed after the first attempt.
max_attempt=2

[ "$#" -eq 2 ] || { echo "Usage: $0 JOBS_JSON ATTEMPT" >&2; exit 2; }
jobs_file=$1
attempt=$2

case $attempt in
  '' | *[!0-9]*) echo "The attempt '$attempt' is not a number." >&2; exit 1 ;;
esac
[ -f "$jobs_file" ] || { echo "The jobs file '$jobs_file' does not exist." >&2; exit 1; }
command -v jq > /dev/null || { echo "jq is required." >&2; exit 1; }
jq -e '(.jobs | type) == "array" and (.jobs | length) > 0' "$jobs_file" > /dev/null 2>&1 ||
  { echo "'$jobs_file' is not a jobs answer with at least one job." >&2; exit 1; }

# Failed jobs other than the final gate, as "runner_id steps name" lines.
failed=$(jq -r '
  .jobs[]
  | select(.conclusion == "failure" or .conclusion == "cancelled" or .conclusion == "timed_out")
  | select(.name | test("CI gate$") | not)
  | [(.runner_id // 0), (.steps // [] | length), .name]
  | @tsv' "$jobs_file")

if [ -z "$failed" ]; then
  echo "keep: no failed job other than the gate"
  exit 0
fi

# Jobs that did get a runner or ran steps are real failures.
ran=$(printf '%s\n' "$failed" | awk -F '\t' '$1 != 0 || $2 != 0 { print $3 }')
if [ -n "$ran" ]; then
  echo "keep: failed after getting a runner: $(printf '%s' "$ran" | tr '\n' ',' | sed 's/,$//')"
  exit 0
fi

if [ "$attempt" -gt "$max_attempt" ]; then
  echo "keep: attempt $attempt already had $((attempt - 1)) reruns; the shortage lasts too long"
  exit 0
fi

echo "rerun: no runner for $(printf '%s' "$failed" | cut -f3 | tr '\n' ',' | sed 's/,$//')"
