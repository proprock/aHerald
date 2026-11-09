#!/bin/sh
# Pins the aherald plugin in a Claude Code marketplace catalog (proprock/marketplace) to a release.
# Used by the update_marketplace job of .github/workflows/release.yml; safe to run by hand.
#
#   scripts/update-marketplace.sh CATALOG TAG SHA
#
# CATALOG is the path of .claude-plugin/marketplace.json, TAG a stable release tag (vX.Y.Z) and SHA
# the 40-character commit of that tag in proprock/aHerald (`git rev-parse "$TAG^{commit}"`).
# The aherald entry is added at the end of `plugins` or replaced in place; other entries are not
# touched. Running it twice with the same arguments leaves the file byte-identical. On bad input
# or an unreadable catalog it prints the reason and leaves the file as it was. Needs jq.
set -eu

[ "$#" -eq 3 ] || { echo "Usage: $0 CATALOG TAG SHA" >&2; exit 2; }
catalog=$1
tag=$2
sha=$3

printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' ||
  { echo "Tag '$tag' is not a stable release tag (vX.Y.Z); pre-releases are not listed." >&2; exit 1; }
printf '%s\n' "$sha" | grep -Eq '^[0-9a-f]{40}$' ||
  { echo "'$sha' is not a 40-character lowercase commit hash." >&2; exit 1; }
[ -f "$catalog" ] || { echo "Catalog '$catalog' does not exist." >&2; exit 1; }
command -v jq > /dev/null || { echo "jq is required." >&2; exit 1; }

# Written next to the catalog and moved over it only on success, so a failure never truncates it.
tmp=$(mktemp "$catalog.XXXXXX")
trap 'rm -f "$tmp"' EXIT
jq --arg tag "$tag" --arg sha "$sha" '
  {
    name: "aherald",
    source: {
      source: "git-subdir",
      url: "https://github.com/proprock/aHerald.git",
      path: "plugins/aherald",
      ref: $tag,
      sha: $sha
    },
    description: "Hooks that show an aHerald card when Claude Code finishes, needs permission or fails"
  } as $entry
  | if (.plugins | type) != "array" then error("the catalog has no plugins array") else . end
  | if any(.plugins[]; .name == "aherald")
    then .plugins |= map(if .name == "aherald" then $entry else . end)
    else .plugins += [$entry]
    end
' "$catalog" > "$tmp" || { echo "Cannot update '$catalog': it is not a valid catalog." >&2; exit 1; }
mv "$tmp" "$catalog"
trap - EXIT
