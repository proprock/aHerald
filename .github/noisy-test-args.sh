#!/bin/sh
# Prints the `--skip <name>` arguments for the tests in .github/noisy-tests.txt (or the list named
# by the first argument), separated by spaces, for `cargo test -- $(sh .github/noisy-test-args.sh)`.
# Names are plain identifiers (tests/ci_noisy_tests.sh checks that), so the unquoted expansion in a
# workflow cannot split or glob them. A missing list is an error: skipping nothing silently would
# bring the noisy tests back into every pull request.
set -eu
list=${1:-.github/noisy-tests.txt}
[ -f "$list" ] || { echo "noisy-test-args: no list at $list" >&2; exit 1; }
args=''
while IFS= read -r line || [ -n "$line" ]; do
  name=$(printf '%s' "$line" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
  case "$name" in
    '' | '#'*) continue ;;
  esac
  args="$args --skip $name"
done <"$list"
printf '%s\n' "${args# }"
