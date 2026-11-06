#!/bin/sh
# Repeats test binaries under CPU load and stops at the first failure, keeping that run's output.
# Born from the fork-window flakes AH-279 and AH-284: a test runs a freshly written executable, or
# expects a dropped listener to look dead, while another test thread forks.
#
#   scripts/manual/stress.sh [runs] [full-runs] [test-binary ...]
#
# Defaults: 200 runs, 5 full runs afterwards, and the four binaries below. Name other integration
# tests to stress those instead; a full run is `cargo test --workspace` without the timing budget.
set -eu
runs=${1:-200}
full=${2:-5}
if [ "$#" -ge 2 ]; then shift 2; else shift "$#"; fi
[ "$#" -gt 0 ] || set -- plugin_hook_script setup sandbox_sender unix_ipc
cd "$(dirname "$0")/../.."
tests=""
for name in "$@"; do tests="$tests --test $name"; done
count=$#

# shellcheck disable=SC2086 # `$tests` is a list of `--test NAME` words
binaries=$(cargo test --locked --no-run --message-format=json-render-diagnostics $tests |
    sed -n 's/.*"executable":"\([^"]*\/deps\/[^"]*\)".*/\1/p')
[ "$(printf '%s\n' "$binaries" | wc -l)" -eq "$count" ] || { echo "expected $count test binaries" >&2; exit 1; }

log="${TMPDIR:-/tmp}/stress-run.log"
hogs=""
cleanup() { [ -z "$hogs" ] || kill $hogs 2>/dev/null || true; }
trap cleanup EXIT INT TERM
for _ in $(seq "$(nproc)"); do
    (while :; do :; done) &
    hogs="$hogs $!"
done

# "run 120/400, 0:31:00/1:40:00, 1:09:00 left": done/total, elapsed/forecast total, forecast rest.
# The forecast is the average time of the finished iterations times the total.
clock() { printf '%d:%02d:%02d' $(($1 / 3600)) $(($1 % 3600 / 60)) $(($1 % 60)); }
progress() {
    label=$1 done_count=$2 total=$3 began=$4
    elapsed=$(($(date +%s) - began))
    forecast=$((elapsed * total / done_count))
    echo "$label $done_count/$total, $(clock "$elapsed")/$(clock "$forecast"), $(clock $((forecast - elapsed))) left"
}

started=$(date +%s)
i=1
while [ "$i" -le "$runs" ]; do
    for binary in $binaries; do
        # Keep the output of the failing run itself: a rerun usually passes.
        "$binary" --test-threads=64 >"$log" 2>&1 || {
            echo "FAILED: run $i of $runs: $binary" >&2
            grep -E '^test .* FAILED|panicked at|^---- ' -A6 "$log" | head -60 >&2 || true
            exit 1
        }
    done
    if [ "$i" -eq 1 ] || [ $((i % 10)) -eq 0 ] || [ "$i" -eq "$runs" ]; then
        progress run "$i" "$runs" "$started"
    fi
    i=$((i + 1))
done
echo "ok: $runs runs of $count test binaries under load"

started=$(date +%s)
i=1
while [ "$i" -le "$full" ]; do
    # The timing budgets measure an idle machine; the load makes them fail by design.
    cargo test --locked --workspace --all-targets --no-fail-fast -- --skip the_wrapper_costs \
        >"${TMPDIR:-/tmp}/stress-full-$i.log" 2>&1 || {
        echo "FAILED: full run $i of $full, see ${TMPDIR:-/tmp}/stress-full-$i.log" >&2
        grep -E '^test .* FAILED|panicked at' "${TMPDIR:-/tmp}/stress-full-$i.log" >&2 || true
        exit 1
    }
    progress full "$i" "$full" "$started"
    i=$((i + 1))
done
echo "ok: $full full runs under load"
