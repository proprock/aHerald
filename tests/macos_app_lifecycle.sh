#!/bin/sh
# Isolated lifecycle of an installed aHerald.app: first setup, repeat, selection change,
# status line, malformed files, failed writes, upgrade and removal after the environment
# overrides change. Uses a private HOME and a copy of the executable inside a bundle in a
# temporary Applications folder; it never touches the real user's hooks or settings.
# Usage: tests/macos_app_lifecycle.sh [path/to/aherald]
set -eu
cd "$(dirname "$0")/.."
binary=${1:-target/debug/aherald}
[ -x "$binary" ] || { echo "Build the executable first: $binary" >&2; exit 1; }
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")

root=$(mktemp -d "${TMPDIR:-/tmp}/aherald-app-test.XXXXXXXX")
trap 'chmod -R u+w "$root" 2>/dev/null || true; rm -rf "$root"' EXIT HUP INT TERM
home=$root/home
apps=$root/Applications
exe=$apps/aHerald.app/Contents/MacOS/aherald
mkdir -p "$home" "$apps/aHerald.app/Contents/MacOS" "$apps/aHerald.app/Contents/Resources"
cp "$binary" "$exe"
printf 'asset' > "$apps/aHerald.app/Contents/Resources/marker"
support=$home/Library/Application\ Support/aHerald
receipt=$support/aherald-setup.json
config=$support/config.toml
codex=$home/codex
claude=$home/claude

app() {
    HOME="$home" CODEX_HOME="${CODEX_HOME_TEST:-$codex}" CLAUDE_CONFIG_DIR="$claude" "$exe" "$@"
}
tree_sum() { (cd "$apps/aHerald.app" && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256); }
fail() { echo "FAIL: $*" >&2; exit 1; }

bundle_before=$(tree_sum)

# A dry run changes nothing.
app --setup --harness codex --harness claude >/dev/null
[ ! -e "$codex/hooks.json" ] && [ ! -e "$claude/settings.json" ] && [ ! -e "$receipt" ] ||
    fail "dry run wrote files"

# First setup writes hooks for the fixed bundle path; the receipt and config stay outside it.
app --setup --harness codex --harness claude --codex-usage true --codex-context true \
    --claude-context true --apply >/dev/null
[ -f "$receipt" ] && [ -f "$config" ] || fail "receipt or config missing under Application Support"
grep -q "$exe" "$codex/hooks.json" || fail "codex hooks do not use the bundle executable"
grep -q "$exe" "$claude/settings.json" || fail "claude hooks do not use the bundle executable"
grep -q 250k "$config" && grep -q 400k "$config" || fail "default thresholds not set"
[ "$(tree_sum)" = "$bundle_before" ] || fail "setup modified the app bundle"

# Repeated setup is idempotent.
cp "$codex/hooks.json" "$root/codex.before"
cp "$claude/settings.json" "$root/claude.before"
app --setup --harness codex --harness claude --codex-usage true --codex-context true \
    --claude-context true --apply >/dev/null
cmp "$root/codex.before" "$codex/hooks.json" || fail "repeated setup changed codex hooks"
cmp "$root/claude.before" "$claude/settings.json" || fail "repeated setup changed claude settings"

# Deselecting a harness removes only its bindings.
app --setup --harness claude --apply >/dev/null
if [ -f "$codex/hooks.json" ] && grep -q "$exe" "$codex/hooks.json"; then
    fail "codex hooks remained after deselection"
fi
grep -q "$exe" "$claude/settings.json" || fail "claude hooks lost on deselection of codex"
app --setup --harness codex --harness claude --apply >/dev/null

# A user's own status line is wrapped and restored exactly.
app --unsetup --apply >/dev/null
printf '%s' '{"statusLine":{"type":"command","command":"echo mine"}}' > "$claude/settings.json"
app --setup --harness claude --claude-context true --apply >/dev/null
grep -q "'$exe' --statusline --then 'echo mine'" "$claude/settings.json" ||
    fail "status line was not wrapped"
app --unsetup --apply >/dev/null
grep -q '"command": *"echo mine"' "$claude/settings.json" || fail "own status line not restored"
! grep -q -- '--statusline' "$claude/settings.json" || fail "wrapper remained after removal"

# Malformed settings stop cleanup and every other file stays as it was.
app --setup --harness codex --harness claude --apply >/dev/null
cp "$claude/settings.json" "$root/claude.valid"
cp "$codex/hooks.json" "$root/codex.before"
printf '{invalid' > "$claude/settings.json"
if app --unsetup --apply >/dev/null 2>&1; then fail "cleanup of invalid settings succeeded"; fi
cmp "$root/codex.before" "$codex/hooks.json" || fail "failed cleanup changed codex hooks"
[ -f "$receipt" ] || fail "receipt lost after failed cleanup"
cp "$root/claude.valid" "$claude/settings.json"

# A write that fails rolls back earlier writes and keeps the receipt unchanged.
app --unsetup --apply >/dev/null
rm -f "$receipt"
chmod 555 "$codex"
if app --setup --harness claude --harness codex --apply >/dev/null 2>&1; then
    chmod 755 "$codex"; fail "setup into a read-only directory succeeded"
fi
chmod 755 "$codex"
[ ! -e "$receipt" ] || fail "receipt written after a failed setup"
if [ -f "$claude/settings.json" ]; then
    ! grep -q "$exe" "$claude/settings.json" || fail "failed setup left claude hooks"
fi

# An upgrade replaces the executable at the same path; setup stays idempotent and keeps settings.
app --setup --harness codex --harness claude --claude-context true --apply >/dev/null
cp "$binary" "$exe"
cp "$codex/hooks.json" "$root/codex.before"
app --setup --harness codex --harness claude --apply >/dev/null
cmp "$root/codex.before" "$codex/hooks.json" || fail "upgrade changed codex hooks"
grep -q 400k "$config" || fail "upgrade lost the Claude threshold"
mkdir -p "$support/assets"
printf 'custom pet' > "$support/assets/pet.txt"

# Removal after the environment overrides changed still uses the saved file paths.
mkdir -p "$home/other"
CODEX_HOME_TEST=$home/other app --unsetup --apply >/dev/null
if [ -f "$codex/hooks.json" ]; then ! grep -q "$exe" "$codex/hooks.json" || fail "codex hooks remained"; fi
! grep -q -- "$exe" "$claude/settings.json" || fail "claude hooks remained"
[ ! -e "$home/other/hooks.json" ] || fail "removal wrote to the new override"
[ "$(cat "$support/assets/pet.txt")" = 'custom pet' ] || fail "customized asset lost"
[ -f "$config" ] || fail "configuration lost on removal"
[ "$(tree_sum)" = "$bundle_before" ] || fail "lifecycle modified the app bundle"

echo 'Isolated macOS app lifecycle passed'
