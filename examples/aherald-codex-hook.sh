#!/bin/sh
# Codex hook for aHerald. Reference this file from ~/.codex/hooks.json (see
# hooks.macos.json); change the path below if aHerald.app is not in /Applications.
exec /Applications/aHerald.app/Contents/MacOS/aherald --hook --resident
