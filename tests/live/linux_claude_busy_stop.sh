#!/bin/bash
set -euo pipefail
# Run only under a disposable display and D-Bus session, as documented in README.md.
if [[ ${AH115_ISOLATED:-} != 1 ]]; then
  printf '%s\n' 'Set AH115_ISOLATED=1 inside a private Xvfb and D-Bus session.' >&2
  exit 2
fi
cd "$(dirname "$0")/../.."
root=$(mktemp -d)
export XDG_RUNTIME_DIR="$root/runtime"
mkdir -m 700 "$XDG_RUNTIME_DIR"
config="$root/config.toml"
printf '[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n[claude.usage]\nenabled=false\n[events.turn_complete]\nduration_sec=0\n' > "$config"
exe="$PWD/target/debug/aherald"
# The production hook and status paths use the scope named "resident"; the private runtime
# directory keeps this resident apart from the user's own.
"$exe" --resident-child resident --config "$config" >"$root/out" 2>"$root/err" &
pid=$!
cleanup() { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; rm -rf "$root"; }
trap cleanup EXIT
for i in {1..100}; do
  status=$("$exe" --status --config "$config")
  if [[ "$status" == *'"result":"status"'* ]]; then break; fi
  sleep .1
done
queued() { "$exe" --status --config "$config" | python3 -c 'import json,sys; print(json.load(sys.stdin)["queued"])'; }
stop() { printf '%s' "$1" | "$exe" --hook --agent claude --resident --config "$config"; }
expect() { [[ $(queued) == "$2" ]] || { printf 'FAIL %s: queued=%s, expected %s\n' "$1" "$(queued)" "$2" >&2; exit 1; }; }
expect start 0
# Background work (a shell task, a one-shot wakeup) means the turn is not finished: no card.
stop '{"hook_event_name":"Stop","session_id":"busy-1","background_tasks":[{"id":"1","type":"shell","status":"running","description":"DO_NOT_KEEP","command":"DO_NOT_KEEP"}],"session_crons":[]}'
stop '{"hook_event_name":"Stop","session_id":"busy-2","background_tasks":[],"session_crons":[{"id":"c","schedule":"0 9 * * *","recurring":false,"prompt":"DO_NOT_KEEP"}]}'
expect "busy stops" 0
# Only monitors and recurring wakeups, or nothing at all, leave the turn finished: a card.
stop '{"hook_event_name":"Stop","session_id":"idle-1","background_tasks":[{"id":"2","type":"monitor","status":"running"}],"session_crons":[{"id":"c","schedule":"* * * * *","recurring":true}]}'
expect "monitor and recurring wakeup" 1
stop '{"hook_event_name":"Stop","session_id":"idle-2","background_tasks":[],"session_crons":[]}'
expect "empty arrays" 2
stop '{"hook_event_name":"Stop","session_id":"idle-3"}'
expect "no fields" 3
"$exe" --quit --config "$config" >/dev/null
wait "$pid"
