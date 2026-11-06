#!/bin/sh
# Shared by the packaging scripts: `. "$here/version.sh"`. Reads the program's version from
# `cargo metadata` instead of grepping Cargo.toml, so the workspace table can change freely.
# Needs cargo and python3. Run from the repository root.

# Prints the version of the `aherald` package.
crate_version() {
  version=$(cargo metadata --no-deps --format-version 1 | python3 -c \
    'import json, sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "aherald"))') ||
    version=
  [ -n "$version" ] || { echo 'Cannot read the crate version with cargo metadata' >&2; return 1; }
  printf '%s\n' "$version"
}

# require_tag_matches TAG VERSION: exits unless TAG is `v` followed by VERSION.
require_tag_matches() {
  [ "$1" = "v$2" ] || { echo "Tag '$1' does not match the crate version $2" >&2; exit 1; }
}
