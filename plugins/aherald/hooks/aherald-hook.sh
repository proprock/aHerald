#!/bin/sh
# aHerald hook launcher for the Claude Code plugin (macOS).
# Usage: aherald-hook.sh claude|codex   (the hook payload arrives on stdin)
#
# Finds aHerald.app and hands the payload to it. Whatever happens this exits 0 and prints nothing,
# so a missing app or a failure can never change the agent's behaviour. Overrides, in order:
# AHERALD_APP (the .app folder), then /Applications (AHERALD_SYSTEM_APPLICATIONS replaces it), ~/Applications, Spotlight (AHERALD_MDFIND
# replaces the `mdfind` command), then an `aherald` on PATH.
case "$1" in
  claude) set -- --hook --agent claude --resident ;;
  codex) set -- --hook --resident ;;
  *) cat >/dev/null; exit 0 ;;
esac

for app in "${AHERALD_APP:-}" "${AHERALD_SYSTEM_APPLICATIONS:-/Applications}/aHerald.app" "${HOME:-/nonexistent}/Applications/aHerald.app"; do
  if [ -n "$app" ] && [ -x "$app/Contents/MacOS/aherald" ]; then
    exec "$app/Contents/MacOS/aherald" "$@"
  fi
done

found=$("${AHERALD_MDFIND:-mdfind}" "kMDItemCFBundleIdentifier == 'io.github.proprock.aherald'" 2>/dev/null | head -n 1)
if [ -n "$found" ] && [ -x "$found/Contents/MacOS/aherald" ]; then
  exec "$found/Contents/MacOS/aherald" "$@"
fi

if exe=$(command -v aherald 2>/dev/null) && [ -x "$exe" ]; then
  exec "$exe" "$@"
fi

cat >/dev/null
exit 0
