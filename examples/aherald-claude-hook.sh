#!/bin/sh
# Claude Code hook for aHerald. Reference this file from ~/.claude/settings.json (see
# claude-settings.macos.json); change the path below if aHerald.app is not in /Applications.
exec /Applications/aHerald.app/Contents/MacOS/aherald --hook --agent claude --resident
