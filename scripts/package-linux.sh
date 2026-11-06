#!/bin/sh
# Package a previously built x86_64 GNU/Linux binary; verify a clean extraction.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
. "$here/version.sh"
executable=${1:-target/release/aherald}
version=$(crate_version)
tag=${2:-v$version}
require_tag_matches "$tag" "$version"
[ -x "$executable" ] || { echo "Build executable first: $executable" >&2; exit 1; }
[ "$(uname -m)" = x86_64 ] || { echo 'Packaging requires an x86_64 host' >&2; exit 1; }
readelf -h "$executable" | grep -q 'Advanced Micro Devices X86-64' || { echo 'Executable is not x86_64 ELF' >&2; exit 1; }
readelf -l "$executable" | grep -q 'ld-linux-x86-64.so.2' || { echo 'Executable must use the GNU libc loader' >&2; exit 1; }
name=aherald-$tag-linux-x86_64-gnu
mkdir -p dist
archive=dist/$name.tar.gz
[ ! -e "$archive" ] && [ ! -e "$archive.sha256" ] || { echo "Archive already exists: $archive" >&2; exit 1; }
stage=$(mktemp -d "${TMPDIR:-/tmp}/aherald-package.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
root=$stage/$name
mkdir -p "$root"
cp "$executable" "$root/aherald"
cp -R assets docs examples "$root/"
rm -rf "$root/docs/notes" # engineering notes are project history, not user documentation
cp config.example.toml LICENSE README.md CHANGELOG.md "$root/"
cp docs/portable-linux.md "$root/START-HERE.md"
(cd "$stage" && tar -czf "$OLDPWD/$archive" "$name")
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
mkdir "$stage/extracted"
tar -xzf "$archive" -C "$stage/extracted"
case "$("$stage/extracted/$name/aherald" --version)" in
  "aHerald $version (built 20"*" UTC)") ;;
  *) echo "The packaged program reports another version" >&2; exit 1 ;;
esac
[ -f "$stage/extracted/$name/assets/turn_complete/animation.png" ] || [ -d "$stage/extracted/$name/assets/turn_complete" ]
echo "Built and verified $archive"
