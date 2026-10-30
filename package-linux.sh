#!/bin/sh
# Package a previously built x86_64 GNU/Linux binary; verify a clean extraction.
set -eu
cd "$(dirname "$0")"
executable=${1:-target/release/aherald}
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo 'Cannot read version' >&2; exit 1; }
tag=${2:-v$version}
[ "$tag" = "v$version" ] || { echo "Tag $tag does not match version $version" >&2; exit 1; }
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
cp aHerald.example.toml LICENSE README.md CHANGELOG.md "$root/"
cp docs/portable-linux.md "$root/START-HERE.md"
(cd "$stage" && tar -czf "$OLDPWD/$archive" "$name")
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
mkdir "$stage/extracted"
tar -xzf "$archive" -C "$stage/extracted"
test "$("$stage/extracted/$name/aherald" --version)" = "aHerald $version"
[ -f "$stage/extracted/$name/assets/turn_complete/animation.png" ] || [ -d "$stage/extracted/$name/assets/turn_complete" ]
echo "Built and verified $archive"
