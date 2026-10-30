#!/bin/sh
# Set AHERALD_PATH to the extracted aherald binary, or use this default location.
binary=${AHERALD_PATH:-"$HOME/.local/share/aHerald/aherald"}
[ -x "$binary" ] || exit 0
exec "$binary" --hook --agent claude --resident
