#!/bin/sh
# Prints the `--skip <name>` arguments for the tests in the lists named as arguments, separated by
# spaces, for `cargo test -- $(sh .github/noisy-test-args.sh)`. With no argument it reads
# .github/noisy-tests.txt (noisy checks the nightly full run keeps) and .github/solo-tests.txt
# (checks every run executes alone, in their own step), so no whole-suite run sees either.
# Names are plain identifiers (tests/ci_noisy_tests.sh checks that), so the unquoted expansion in a
# workflow cannot split or glob them. A missing list is an error: skipping nothing silently would
# bring the noisy tests back into every pull request.
set -eu
[ "$#" -gt 0 ] || set -- .github/noisy-tests.txt .github/solo-tests.txt
args=''
for list in "$@"; do
  [ -f "$list" ] || { echo "noisy-test-args: no list at $list" >&2; exit 1; }
  while IFS= read -r line || [ -n "$line" ]; do
    name=$(printf '%s' "$line" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
    case "$name" in
      '' | '#'*) continue ;;
    esac
    args="$args --skip $name"
  done <"$list"
done
printf '%s\n' "${args# }"
