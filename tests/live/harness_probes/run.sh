#!/bin/sh
# Opt-in probe (AH-201): runs a real harness turn in a private HOME and prints the events its
# extension saw. Usage: run.sh pi|opencode
#   pi       needs Pi >= 1.5 and Node >= 22.19 on PATH; uses a local fake OpenAI endpoint,
#            so no account, network or billing.
#   opencode needs OpenCode 2.x; sends one trivial prompt ("Reply with the single word ok")
#            to its free hosted model: network access, no account. Always run with
#            --print-logs: without it `--standalone` hangs on this version.
# Everything is created under a temporary directory and removed at the end.
set -eu
# Pi 1.5 fails on Node 23.7 (bundled undici) and needs >= 22.19: prefer Homebrew's node@22.
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
case "${1:-}" in
pi)
  mkdir -p "$tmp/agent/extensions"
  cp "$here/pi_probe.ts" "$tmp/agent/extensions/probe.ts"
  printf '%s' '{"providers":{"fake":{"baseUrl":"http://127.0.0.1:18765/v1","api":"openai-completions","apiKey":"unused","models":[{"id":"fake-model"}]}}}' > "$tmp/agent/models.json"
  python3 "$here/fake_openai.py" 18765 & server=$!
  sleep 1
  (cd "$tmp/work" && HOME="$tmp/home" PI_CODING_AGENT_DIR="$tmp/agent" PROBE_OUT="$out" PI_OFFLINE=1 \
    timeout 90 pi --provider fake --model fake-model -p "say ok" < /dev/null > /dev/null)
  ;;
opencode)
  h="$tmp/home"
  mkdir -p "$h/.config/opencode/plugins" "$h/.local/share" "$h/.local/state" "$h/.cache"
  cp "$here/opencode_probe.ts" "$h/.config/opencode/plugins/probe.ts"
  (cd "$tmp/work" && HOME="$h" XDG_CONFIG_HOME="$h/.config" XDG_DATA_HOME="$h/.local/share" \
    XDG_STATE_HOME="$h/.local/state" XDG_CACHE_HOME="$h/.cache" PROBE_OUT="$out" \
    timeout 90 opencode run --standalone --print-logs --log-level warn \
    -m opencode/big-pickle "Reply with the single word ok" > /dev/null 2>&1)
  pkill -f "opencode serve --stdio" 2>/dev/null || true
  ;;
*) echo "usage: $0 pi|opencode" >&2; exit 2 ;;
esac
cat "$out"
