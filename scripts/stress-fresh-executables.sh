#!/bin/sh
# Hunts the AH-279 flake: a test runs a freshly written executable while another test thread
# forks, and Linux answers ETXTBSY. Runs the three test binaries that start such programs many
# times with many threads under CPU load, then a few full runs, and stops at the first failure.
#
#   scripts/stress-fresh-executables.sh [runs] [full-runs]     (defaults: 200 and 5)
set -eu
runs=${1:-200}
full=${2:-5}
cd "$(dirname "$0")/.."

binaries=$(cargo test --locked --no-run --message-format=json-render-diagnostics \
    --test plugin_hook_script --test setup --test sandbox_sender |
    sed -n 's/.*"executable":"\([^"]*\/deps\/[^"]*\)".*/\1/p')
[ "$(printf '%s\n' "$binaries" | wc -l)" -eq 3 ] || { echo "expected three test binaries" >&2; exit 1; }

log="${TMPDIR:-/tmp}/stress-run.log"
hogs=""
cleanup() { [ -z "$hogs" ] || kill $hogs 2>/dev/null || true; }
trap cleanup EXIT INT TERM
for _ in $(seq "$(nproc)"); do
    (while :; do :; done) &
    hogs="$hogs $!"
done

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
    i=$((i + 1))
done
echo "ok: $runs runs of the three test binaries under load"

i=1
while [ "$i" -le "$full" ]; do
    # The timing budgets measure an idle machine; the load makes them fail by design.
    cargo test --locked --workspace --all-targets --no-fail-fast -- --skip the_wrapper_costs \
        >"${TMPDIR:-/tmp}/stress-full-$i.log" 2>&1 || {
        echo "FAILED: full run $i of $full, see ${TMPDIR:-/tmp}/stress-full-$i.log" >&2
        grep -E '^test .* FAILED|panicked at' "${TMPDIR:-/tmp}/stress-full-$i.log" >&2 || true
        exit 1
    }
    i=$((i + 1))
done
echo "ok: $full full runs under load"
