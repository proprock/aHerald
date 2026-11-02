#!/bin/sh
# Run under xvfb-run and dbus-run-session with old and new binaries as arguments.
set -eu
old=$(realpath "$1")
new=$(realpath "$2")
root=$(mktemp -d /tmp/aherald-upgrade.XXXXXXXX)
pid=
cleanup() {
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || :
        wait "$pid" 2>/dev/null || :
    fi
    rm -rf "$root"
}
trap cleanup EXIT HUP INT TERM
mkdir -m 700 "$root/runtime" "$root/home"
export XDG_RUNTIME_DIR="$root/runtime" HOME="$root/home"
printf '[general]\nmode = "resident"\n' > "$root/config.toml"
"$old" --resident --config "$root/config.toml" >"$root/resident.log" 2>&1 &
pid=$!
ready=0
for ignored in $(seq 1 100); do
    status=$("$new" --status)
    if printf '%s' "$status" | grep -q '"pid"'; then ready=1; break; fi
    kill -0 "$pid" || { cat "$root/resident.log" >&2; exit 1; }
    sleep 0.1
done
[ "$ready" = 1 ] || { echo 'old resident did not start' >&2; exit 1; }
"$new" --event turn_complete --resident --config "$root/config.toml"
"$new" --quit | grep -q '"bye"'
wait "$pid"
pid=
echo 'New client exchanged status, event and quit with the old resident'
