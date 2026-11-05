#!/bin/sh
# Contract test for .github/platform-flags.sh: changed paths in, CI flags out.
# Run from the repository root: sh tests/ci_platform_flags.sh
set -eu

script=.github/platform-flags.sh
failures=0

# expect <name> <expected key=value lines, space separated> <changed paths...>
expect() {
  name=$1
  expected=$2
  shift 2
  actual="$(printf '%s\n' "$@" | sh "$script")"
  for pair in $expected; do
    if ! printf '%s\n' "$actual" | grep -qxF -- "$pair"; then
      echo "FAIL: $name: expected $pair, got:" >&2
      printf '%s\n' "$actual" | sed 's/^/  /' >&2
      failures=$((failures + 1))
    fi
  done
}

expect "empty diff selects nothing" 'code=false os=[]'
expect "markdown only selects nothing" 'code=false os=[]' README.md docs/guide.md
expect "shared source selects every platform" \
  'code=true windows=true linux=true macos=true os=["windows-latest","macos-15","ubuntu-22.04"]' \
  src/lib.rs
expect "windows-only source" 'windows=true linux=false macos=false os=["windows-latest"]' src/tray.rs
expect "a windows-gated test selects only windows" \
  'windows=true linux=false macos=false' tests/statusline_windows.rs
expect "linux-only source" 'windows=false linux=true macos=false os=["ubuntu-22.04"]' src/linux/mod.rs
expect "macos-only source" 'windows=false linux=false macos=true os=["macos-15"]' src/mac/window.rs
expect "unix tests select linux and macos" \
  'windows=false linux=true macos=true os=["macos-15","ubuntu-22.04"]' tests/unix_ipc.rs
expect "shared mac input also builds on linux" 'linux=true macos=true' src/mac/input.rs
expect "powershell scripts select windows" 'windows=true linux=false macos=false' scripts/check-release.ps1
expect "a docs change beside code does not widen the selection" \
  'windows=true linux=false macos=false' README.md src/tray.rs
expect "an unknown path falls back to every platform" 'windows=true linux=true macos=true' brand-new-file.txt
expect "a manifest change flags the dependency audit" 'deps=true' Cargo.lock
expect "a source change does not flag the dependency audit" 'deps=false' src/lib.rs
expect "a workflow change flags the workflow lint and every platform" \
  'workflows=true windows=true linux=true macos=true' .github/workflows/ci.yml
expect "a source change does not flag the workflow lint" 'workflows=false' src/lib.rs

# Packaging and installer checks are slow, so a pull request runs one only when its change can
# affect that platform's package; the full run (--all) always runs every one.
expect "a core source change leaves the packaging checks to the full run" \
  'pkg_windows=false pkg_linux=false pkg_macos=false' src/lib.rs
expect "the Windows installer sources select the Windows package check" \
  'pkg_windows=true pkg_linux=false pkg_macos=false' installer/aherald.iss
expect "a Debian script selects the Linux package check" \
  'pkg_windows=false pkg_linux=true pkg_macos=false' scripts/package-deb.sh
expect "a disk image script selects the macOS package check" \
  'pkg_windows=false pkg_linux=false pkg_macos=true' scripts/package-dmg.sh
expect "the shared shell version reader selects the Linux and macOS package checks" \
  'pkg_windows=false pkg_linux=true pkg_macos=true windows=false linux=true macos=true' scripts/version.sh
expect "the Ping packaging script selects every platform's build but no package check" \
  'pkg_windows=false pkg_linux=false pkg_macos=false windows=true linux=true macos=true' scripts/package-ping.sh
expect "a Windows packaging script selects only the Windows package check" \
  'pkg_windows=true pkg_linux=false pkg_macos=false' scripts/package-release.ps1
expect "the manifest lock selects every package check" \
  'pkg_windows=true pkg_linux=true pkg_macos=true' Cargo.lock
expect "hook setup code selects every package check" \
  'pkg_windows=true pkg_linux=true pkg_macos=true' src/hook_setup.rs
expect "a workflow change selects every package check" \
  'pkg_windows=true pkg_linux=true pkg_macos=true' .github/workflows/ci.yml

all="$(sh "$script" --all < /dev/null)"
for pair in code=true deps=true workflows=true windows=true linux=true macos=true \
  pkg_windows=true pkg_linux=true pkg_macos=true \
  'os=["windows-latest","macos-15","ubuntu-22.04"]'; do
  if ! printf '%s\n' "$all" | grep -qxF -- "$pair"; then
    echo "FAIL: --all: expected $pair, got:" >&2
    printf '%s\n' "$all" | sed 's/^/  /' >&2
    failures=$((failures + 1))
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "$failures platform-flags check(s) failed" >&2
  exit 1
fi
echo "platform-flags checks passed"
