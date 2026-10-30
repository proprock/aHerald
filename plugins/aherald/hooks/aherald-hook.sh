#!/bin/sh
# Claude Code plugin hook launcher for macOS and Linux.
# Usage: aherald-hook.sh claude|codex (hook payload arrives on stdin).
# AHERALD_PATH explicitly selects an executable; otherwise try the macOS app locations,
# the portable Linux location and then PATH. Missing or failing programs stay silent.
agent=$1
case "$agent" in
  claude) set -- --hook --agent claude --resident ;;
  codex) set -- --hook --resident ;;
  *) cat >/dev/null; exit 0 ;;
esac

run_hook() {
  executable=$1
  shift
  if [ "$agent" = codex ]; then
    # Preserve the documented Codex Stop acknowledgement only; ignore other output.
    if response=$("$executable" "$@" 2>/dev/null) && [ "$response" = '{}' ]; then
      printf '%s\n' '{}'
    fi
  else
    "$executable" "$@" >/dev/null 2>&1 || :
  fi
  exit 0
}

if [ -n "${AHERALD_PATH:-}" ]; then
  if [ -x "$AHERALD_PATH" ]; then
    run_hook "$AHERALD_PATH" "$@"
  fi
  cat >/dev/null
  exit 0
fi

for app in "${AHERALD_APP:-}" "${AHERALD_SYSTEM_APPLICATIONS:-/Applications}/aHerald.app" "${HOME:-/nonexistent}/Applications/aHerald.app"; do
  if [ -n "$app" ] && [ -x "$app/Contents/MacOS/aherald" ]; then
    run_hook "$app/Contents/MacOS/aherald" "$@"
  fi
done

if [ "$(uname -s 2>/dev/null)" = Darwin ]; then
  found=$("${AHERALD_MDFIND:-mdfind}" "kMDItemCFBundleIdentifier == 'io.github.proprock.aherald'" 2>/dev/null | head -n 1)
  if [ -n "$found" ] && [ -x "$found/Contents/MacOS/aherald" ]; then
    run_hook "$found/Contents/MacOS/aherald" "$@"
  fi
fi

portable=${HOME:-/nonexistent}/.local/share/aHerald/aherald
if [ -x "$portable" ]; then
  run_hook "$portable" "$@"
fi
if exe=$(command -v aherald 2>/dev/null) && [ -x "$exe" ]; then
  run_hook "$exe" "$@"
fi

cat >/dev/null
exit 0
