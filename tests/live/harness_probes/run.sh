#!/bin/sh
# Opt-in probe (AH-201): runs a real harness turn in a private HOME and prints the events its
# extension saw. Usage: run.sh pi [ok|error|abort|ui]   run.sh opencode [ok|error|permission|interrupt]
# Pi 1.5 (needs Node >= 22.19) and OpenCode 2.2 talk to a local fake OpenAI endpoint
# (fake_openai.py), so no account, no model network traffic and no billing. Exceptions:
#   opencode ok       sends one trivial prompt to OpenCode's free hosted model (network)
#   opencode (others) lets OpenCode install @ai-sdk/openai-compatible from npm on first use
# `error` takes a while: OpenCode retries ten times (about 100 s) before it gives up.
# Everything lives in a temporary directory that is removed at the end.
set -eu
for node22 in /opt/homebrew/opt/node@22/bin /usr/local/opt/node@22/bin; do
  [ -x "$node22/node" ] && PATH="$node22:$PATH" && break
done
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
out="$tmp/events.jsonl"
server=
cleanup() { [ -z "$server" ] || kill "$server" 2>/dev/null || true; rm -rf "$tmp"; }
trap cleanup EXIT INT TERM
mkdir -p "$tmp/work"
harness=${1:-}
scenario=${2:-ok}
port=18765
fake() { python3 "$here/fake_openai.py" "$port" "$1" & server=$!; sleep 1; }
case "$harness" in
pi)
  mkdir -p "$tmp/agent/extensions"
  cp "$here/pi_probe.ts" "$tmp/agent/extensions/probe.ts"
  printf '%s' '{"providers":{"fake":{"baseUrl":"http://127.0.0.1:18765/v1","api":"openai-completions","apiKey":"unused","models":[{"id":"fake-model"}]}}}' > "$tmp/agent/models.json"
  case "$scenario" in
  ok) fake ok; mode=-p ;;
  error) fake error; mode=-p ;;
  abort) fake slow; mode=-p; export PROBE_ABORT=1 ;;
  ui) fake ok; mode=rpc; export PROBE_UI=1 ;;
  *) echo "unknown pi scenario" >&2; exit 2 ;;
  esac
  export HOME="$tmp/home" PI_CODING_AGENT_DIR="$tmp/agent" PROBE_OUT="$out" PI_OFFLINE=1
  cd "$tmp/work"
  if [ "$mode" = rpc ]; then
    python3 "$here/pi_rpc_ui.py" > /dev/null
  else
    timeout 600 pi --provider fake --model fake-model -p "say ok" < /dev/null > /dev/null 2>&1 || true
  fi
  ;;
opencode)
  h="$tmp/home"
  mkdir -p "$h/.config/opencode/plugins" "$h/.local/share" "$h/.local/state" "$h/.cache"
  cp "$here/opencode_probe.ts" "$h/.config/opencode/plugins/probe.ts"
  model=opencode/big-pickle
  if [ "$scenario" != ok ]; then
    model=fake/fake-model
    printf '%s' '{"provider":{"fake":{"npm":"@ai-sdk/openai-compatible","name":"fake","options":{"baseURL":"http://127.0.0.1:18765/v1","apiKey":"unused"},"models":{"fake-model":{"name":"fake-model"}}}},"permission":{"shell":"ask"}}' > "$h/.config/opencode/opencode.json"
  fi
  prompt="Reply with the single word ok"
  case "$scenario" in
  ok) ;;
  error) fake error ;;
  permission) export FAKE_TOOL=shell; fake tool ;;
  subagent) export FAKE_TOOL=task FAKE_ARGS='{"description":"probe","prompt":"say ok","subagent_type":"general"}'; fake once ;;
  interrupt) export PROBE_INTERRUPT=1; fake slow ;;
  *) echo "unknown opencode scenario" >&2; exit 2 ;;
  esac
  # stdin must not be inherited: with it the standalone server never starts.
  (cd "$tmp/work" && HOME="$h" XDG_CONFIG_HOME="$h/.config" XDG_DATA_HOME="$h/.local/share" \
    XDG_STATE_HOME="$h/.local/state" XDG_CACHE_HOME="$h/.cache" PROBE_OUT="$out" \
    timeout 600 opencode run --standalone --print-logs --log-level warn \
    -m "$model" "$prompt" > /dev/null 2>&1 < /dev/null) || true
  pkill -f "opencode serve --stdio" 2>/dev/null || true
  ;;
*) echo "usage: $0 pi|opencode [scenario]" >&2; exit 2 ;;
esac
cat "$out"
