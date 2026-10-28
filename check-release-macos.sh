#!/bin/sh
# Release check for the macOS archive: structure, architecture, bundle metadata, signature
# and a clean-extraction run. Usage: ./check-release-macos.sh <archive.zip> <version>
set -eu
archive=$1
version=$2
fail() { echo "FAIL: $*" >&2; exit 1; }

check=$(mktemp -d "${TMPDIR:-/tmp}/aherald-extract.XXXXXX")
trap 'rm -rf "$check"' EXIT
ditto -x -k "$archive" "$check"
root=$(echo "$check"/aherald-*)
app=$root/aHerald.app
exe=$app/Contents/MacOS/aherald

for required in aHerald.example.toml LICENSE README.md START-HERE.md MANIFEST.txt \
  examples/aherald-codex-hook.sh examples/aherald-claude-hook.sh docs/configuration.md \
  aHerald.app/Contents/Info.plist aHerald.app/Contents/Resources/assets/turn_complete; do
  [ -e "$root/$required" ] || fail "archive lacks $required"
done
[ -x "$exe" ] || fail "main program is not executable"
[ -x "$root/examples/aherald-codex-hook.sh" ] || fail "hook example is not executable"

lipo -archs "$exe" | grep -qx arm64 || fail "program is not arm64-only"
plist=$app/Contents/Info.plist
[ "$(/usr/libexec/PlistBuddy -c 'Print :LSUIElement' "$plist")" = true ] || fail "LSUIElement is not set"
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")" = "$version" ] || fail "bundle version differs"
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")" = aherald ] || fail "bundle executable differs"
codesign --verify --strict "$app" || fail "signature does not verify"
# Only system libraries and frameworks may be linked.
if otool -L "$exe" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/usr/lib/|/System/Library/)' | grep -q .; then
  fail "links a library outside the system"
fi

[ "$("$exe" --version)" = "aHerald $version" ] || fail "extracted program reports another version"
# A fresh home must not make the program touch the signed bundle.
home=$check/home
mkdir "$home"
HOME=$home "$exe" --status >/dev/null || fail "--status failed"
codesign --verify --strict "$app" || fail "the bundle changed while running"
echo "OK: $archive"
