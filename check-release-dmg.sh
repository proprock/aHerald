#!/bin/sh
# Release check for the macOS disk image: layout, bundle metadata, signature, and the isolated
# app lifecycle run against the program inside the mounted image.
# Usage: ./check-release-dmg.sh <image.dmg> <version>
set -eu
cd "$(dirname "$0")"
image=$1
version=$2
fail() { echo "FAIL: $*" >&2; exit 1; }

mount=$(mktemp -d "${TMPDIR:-/tmp}/aherald-dmg-check.XXXXXX")
cleanup() { hdiutil detach -quiet "$mount" 2>/dev/null || true; rmdir "$mount" 2>/dev/null || true; }
trap cleanup EXIT HUP INT TERM
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount" "$image"

app=$mount/aHerald.app
exe=$app/Contents/MacOS/aherald
[ -x "$exe" ] || fail "image lacks the program"
[ -L "$mount/Applications" ] && [ "$(readlink "$mount/Applications")" = /Applications ] ||
  fail "image lacks the Applications link"
[ -f "$mount/READ ME FIRST.txt" ] || fail "image lacks the instructions"
[ -e "$app/Contents/Resources/assets/turn_complete" ] || fail "image lacks the default assets"
lipo -archs "$exe" | grep -qx arm64 || fail "program is not arm64-only"
plist=$app/Contents/Info.plist
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")" = "$version" ] ||
  fail "bundle version differs"
codesign --verify --strict "$app" || fail "signature does not verify"
[ "$("$exe" --version)" = "aHerald $version" ] || fail "program reports another version"
tests/macos_app_lifecycle.sh "$exe" >/dev/null || fail "app lifecycle failed on the mounted program"
echo "OK: $image"
