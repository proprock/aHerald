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
export AH101_SCOPE="ah101-smoke-$$"
config="$root/config.toml"
printf '[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n[claude.usage]\nenabled=false\n[codex.warnings]\ncontext_tokens=1\n[claude.warnings]\ncontext_tokens=1\n[events.context_warning]\nduration_sec=0\n' > "$config"
exe="$PWD/target/debug/aherald"
bridge="$PWD/target/debug/examples/session_warning_bridge"
"$exe" --resident-child "$AH101_SCOPE" --config "$config" >"$root/out" 2>"$root/err" &
pid=$!
cleanup() { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; rm -rf "$root"; }
trap cleanup EXIT
for i in {1..100}; do
  status=$("$bridge" --status)
  if [[ "$status" == *'"result":"status"'* ]]; then break; fi
  sleep .1
done
printf '%s\n' "$status"
STATUS="$status" EXPECTED_PID="$pid" python3 -c 'import json,os; s=json.loads(os.environ["STATUS"]); assert s["result"]=="status" and s["pid"]==int(os.environ["EXPECTED_PID"])'
printf '%s' '{"hook_event_name":"PostModelRequest","session_id":"codex-smoke","status":"completed","token_usage":{"input_tokens":101}}' | "$bridge" --hook --resident
printf '%s' '{"session_id":"claude-smoke","context_window":{"current_usage":{"input_tokens":201,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}' | "$bridge" --statusline
status=$("$bridge" --status)
printf '%s\n' "$status"
STATUS="$status" python3 -c 'import json,os; assert json.loads(os.environ["STATUS"])["queued"]==2'
"$bridge" --quit
wait "$pid"
