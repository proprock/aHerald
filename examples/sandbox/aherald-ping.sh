#!/bin/sh
# aHerald sender for an agent inside a container or virtual machine.
# Usage: aherald-ping.sh claude|codex   (the hook payload arrives on stdin)
# Needs AHERALD_ENDPOINT (http://host:port) and AHERALD_TOKEN from `aherald --remote-info` on the
# host, and curl. It posts the payload to the host's [remote] listener, prints the host's reply
# only when it is the Codex Stop acknowledgement `{}`, and always exits 0: a missing host, a wrong
# token or a slow network never fails or delays the agent for more than about a second.
# The same logic is built into plugins/aherald/hooks/aherald-hook.sh; keep the two in step.
agent=$1
case "$agent" in
  claude | codex) ;;
  *) cat >/dev/null; exit 0 ;;
esac
if [ -z "${AHERALD_ENDPOINT:-}" ] || [ -z "${AHERALD_TOKEN:-}" ] || ! command -v curl >/dev/null 2>&1; then
  cat >/dev/null
  exit 0
fi
# At most 64 KiB, the limit of the listener; curl gives up after one second whatever happens.
response=$(head -c 65536 | curl --silent --fail --max-time 1 --connect-timeout 0.5 \
  --request POST --header "Authorization: Bearer $AHERALD_TOKEN" --data-binary @- \
  "${AHERALD_ENDPOINT%/}/v1/hook?agent=$agent" 2>/dev/null) || response=
if [ "$agent" = codex ] && [ "$response" = '{}' ]; then
  printf '%s\n' '{}'
fi
exit 0
