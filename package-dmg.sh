#!/bin/sh
# Builds the disk image of an already packaged aHerald.app (see package-macos.sh) and checks it.
# Usage: ./package-dmg.sh [tag]   (run ./package-macos.sh first)
set -eu
cd "$(dirname "$0")"

version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo "Cannot read the crate version from Cargo.toml" >&2; exit 1; }
tag=${1:-v$version}
[ "$tag" = "v$version" ] || { echo "Tag $tag does not match Cargo.toml version $version" >&2; exit 1; }

name=aherald-$tag-macos-arm64
out=dist
app=$out/$name/aHerald.app
[ -d "$app" ] || { echo "Run ./package-macos.sh first: $app is missing" >&2; exit 1; }

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
./check-release-dmg.sh "$dmg" "$version"
