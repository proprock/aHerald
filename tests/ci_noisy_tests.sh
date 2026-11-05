#!/bin/sh
# Contract test for .github/noisy-test-args.sh: the list of noisy tests in, `--skip` arguments out.
# Run from the repository root: sh tests/ci_noisy_tests.sh
set -eu

script=.github/noisy-test-args.sh
list=.github/noisy-tests.txt
tmp="${TMPDIR:-/tmp}/aherald-noisy-$$"
trap 'rm -f "$tmp"' EXIT
failures=0

check() {
  name=$1 expected=$2 actual=$3
  if [ "$expected" != "$actual" ]; then
    echo "FAIL: $name: expected '$expected', got '$actual'" >&2
    failures=$((failures + 1))
  fi
}

printf '# a comment\n\nfirst_test\n  second_test  \n# another\nthird::test\n' >"$tmp"
check "names become --skip pairs; comments and blanks are dropped" \
  '--skip first_test --skip second_test --skip third::test' "$(sh "$script" "$tmp")"

: >"$tmp"
check "an empty list skips nothing" '' "$(sh "$script" "$tmp")"

if sh "$script" "$tmp.missing" >/dev/null 2>&1; then
  echo "FAIL: a missing list must fail, not skip nothing silently" >&2
  failures=$((failures + 1))
fi

# The real list: every entry is one plain test name, so the unquoted expansion in the workflows
# cannot split or glob it.
real="$(sh "$script")"
check "the real list is not empty" 1 "$([ -n "$real" ] && echo 1 || echo 0)"
for word in $real; do
  case "$word" in
    --skip) ;;
    *[!A-Za-z0-9_:]*) echo "FAIL: unsafe test name in $list: $word" >&2; failures=$((failures + 1)) ;;
  esac
done

[ "$failures" -eq 0 ] || exit 1
echo "ok"
