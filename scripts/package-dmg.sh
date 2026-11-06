#!/bin/sh
# Builds the disk image of an already packaged aHerald.app (see scripts/package-macos.sh) and checks it.
# Usage: ./scripts/package-dmg.sh [tag]   (run ./scripts/package-macos.sh first)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
. "$here/version.sh"

version=$(crate_version)
tag=${1:-v$version}
require_tag_matches "$tag" "$version"

name=aherald-$tag-macos-arm64
out=dist
app=$out/$name/aHerald.app
[ -d "$app" ] || { echo "Run ./scripts/package-macos.sh first: $app is missing" >&2; exit 1; }

dmg=$out/$name.dmg
stage=$(mktemp -d "${TMPDIR:-/tmp}/aherald-dmg.XXXXXX")
trap 'rm -rf "$stage"' EXIT
rm -f "$dmg" "$dmg.sha256"

# ditto keeps the ad-hoc signature and extended attributes intact.
ditto "$app" "$stage/aHerald.app"
ln -s /Applications "$stage/Applications"
cp docs/dmg-readme.txt "$stage/READ ME FIRST.txt"
hdiutil create -quiet -volname aHerald -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg"
(cd "$out" && shasum -a 256 "$name.dmg" > "$name.dmg.sha256")
echo "Built $dmg"
"$here/check-release-dmg.sh" "$dmg" "$version"
