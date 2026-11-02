#!/bin/sh
# Isolated dpkg lifecycle; requires unprivileged user and mount namespaces.
set -eu
if [ "${1:-}" != --inside ]; then
    package=$(realpath "$1")
    root=$(mktemp -d /tmp/aherald-deb-test.XXXXXXXX)
    trap 'rm -rf "$root"' EXIT HUP INT TERM
    TASK_DEB_ROOT="$root" TASK_DEB_FILE="$package" \
        unshare --user --map-root-user --mount --pid --fork sh "$0" --inside
    exit
fi
root=$TASK_DEB_ROOT
mkdir -p "$root/var/lib/dpkg" "$root/home/test" "$root/lib" "$root/lib64" "$root/proc"
# Only the private mount namespace sees these read-only runtime libraries.
mount --bind /lib "$root/lib"
mount -o remount,bind,ro "$root/lib"
mount --bind /lib64 "$root/lib64"
mount -o remount,bind,ro "$root/lib64"
mount -t proc proc "$root/proc"
# Host needrestart's dpkg status logger would otherwise write to host /run.
if [ -f /etc/dpkg/dpkg.cfg.d/needrestart ]; then
    : > "$root/empty-dpkg-config"
    mount --bind "$root/empty-dpkg-config" /etc/dpkg/dpkg.cfg.d/needrestart
fi
dpkg_test() {
    dpkg --status-logger='cat >/dev/null' --log="$root/dpkg.log" --root="$root" --force-depends "$@"
}
app() {
    user_home=${TASK_USER_HOME:-/home/test}
    HOME="$user_home" XDG_CONFIG_HOME="$user_home/config" XDG_DATA_HOME="$user_home/data" \
      XDG_STATE_HOME="$user_home/state" CODEX_HOME="${CODEX_HOME_TEST:-$user_home/codex}" \
      CLAUDE_CONFIG_DIR="$user_home/claude" chroot "$root" /usr/bin/aherald "$@"
}
dpkg_test --install "$TASK_DEB_FILE"
test -x "$root/usr/lib/aherald/aherald"
test -L "$root/usr/bin/aherald"
mkdir -p "$root/usr/lib/x86_64-linux-gnu"
mount --bind /usr/lib/x86_64-linux-gnu "$root/usr/lib/x86_64-linux-gnu"
mount -o remount,bind,ro "$root/usr/lib/x86_64-linux-gnu"
app --setup --harness codex --harness claude --codex-usage true --claude-context true --codex-context true --apply
TASK_USER_HOME=/home/second app --setup --harness codex --apply
# A second user's own status line is wrapped and restored exactly (the test root has no sh, so
# running the wrapped command is left to tests/statusline.rs).
mkdir -p "$root/home/second/claude"
printf '%s' '{"statusLine":{"type":"command","command":"echo mine"}}' > "$root/home/second/claude/settings.json"
TASK_USER_HOME=/home/second app --setup --harness codex --harness claude --claude-context true --apply
grep -q "/usr/lib/aherald/aherald' --statusline --then 'echo mine'" "$root/home/second/claude/settings.json"
test -f "$root/home/second/data/aherald/aherald-setup.json"
receipt="$root/home/test/data/aherald/aherald-setup.json"
receipt_config="$root/home/test/config/aherald/config.toml"
test -f "$receipt"
test -f "$receipt_config"
test -f "$root/home/second/codex/hooks.json"
grep -q '/usr/lib/aherald/aherald' "$root/home/second/codex/hooks.json"
# Context warnings: thresholds are set and the packaged wrapper replaces no user line.
grep -q "'/usr/lib/aherald/aherald' --statusline\"" "$root/home/test/claude/settings.json"
grep -q 400k "$receipt_config" && grep -q 250k "$receipt_config"
# A configured format is printed by the packaged wrapper; without a configuration nothing is
# printed and nothing is created.
sed -i 's/^format = ""$/format = "{model.display_name}"/; s/^color = true$/color = false/' "$receipt_config"
grep -q '^format = "{model.display_name}"$' "$receipt_config"
payload='{"session_id":"s","model":{"display_name":"Opus"}}'
test "$(printf '%s' "$payload" | app --statusline)" = Opus
test -z "$(printf '%s' "$payload" | TASK_USER_HOME=/home/nobody app --statusline)"
test ! -e "$root/home/nobody/config"
cp "$root/home/test/claude/settings.json" "$root/claude.valid"
cp "$root/home/test/codex/hooks.json" "$root/codex.before"
printf '{invalid' > "$root/home/test/claude/settings.json"
if app --unsetup --apply; then echo 'invalid cleanup unexpectedly succeeded' >&2; exit 1; fi
cmp "$root/codex.before" "$root/home/test/codex/hooks.json"
test -f "$receipt"
cp "$root/claude.valid" "$root/home/test/claude/settings.json"
dpkg_test --install "$TASK_DEB_FILE"
# An upgrade keeps the wrapper and the thresholds.
grep -q -- '--statusline' "$root/home/test/claude/settings.json"
grep -q 400k "$receipt_config"
CODEX_HOME_TEST=/home/test/other app --unsetup --apply
test ! -e "$root/home/test/other/hooks.json"
! grep -q -- '--statusline' "$root/home/test/claude/settings.json"
TASK_USER_HOME=/home/second app --unsetup --apply
grep -q '"command": *"echo mine"' "$root/home/second/claude/settings.json"
! grep -q -- '--statusline' "$root/home/second/claude/settings.json"
mkdir -p "$root/home/test/data/aherald/assets"
printf 'custom pet' > "$root/home/test/data/aherald/assets/pet.txt"
dpkg_test --purge aherald
test ! -e "$root/usr/lib/aherald/aherald"
test "$(cat "$root/home/test/data/aherald/assets/pet.txt")" = 'custom pet'
test -f "$receipt"
test -f "$receipt_config"
echo 'Isolated dpkg install, upgrade, cleanup failure, cleanup and purge passed'
