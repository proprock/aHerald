#!/bin/sh
# Package a previously built static aherald-ping for one Linux architecture and check it.
# Usage: scripts/package-ping.sh x86_64|aarch64 [tag]   (build first, see below)
#   cargo build --locked --profile ping -p aherald-ping --target <arch>-unknown-linux-musl
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
. "$here/version.sh"
arch=${1:?usage: scripts/package-ping.sh x86_64|aarch64 [tag]}
case "$arch" in
  x86_64) machine='Advanced Micro Devices X86-64' ;;
  aarch64) machine='AArch64' ;;
  *) echo "Unknown architecture: $arch" >&2; exit 1 ;;
esac
version=$(crate_version)
tag=${2:-v$version}
require_tag_matches "$tag" "$version"
binary=target/$arch-unknown-linux-musl/ping/aherald-ping
[ -x "$binary" ] || { echo "Build first: $binary" >&2; exit 1; }
readelf -h "$binary" | grep -q "$machine" || { echo "Not a $arch ELF: $binary" >&2; exit 1; }
# Static: no program interpreter and no shared library, so it runs in any guest.
if readelf -l "$binary" | grep -q 'INTERP'; then echo 'The binary needs a loader' >&2; exit 1; fi
if readelf -d "$binary" 2>/dev/null | grep -q 'NEEDED'; then echo 'The binary needs shared libraries' >&2; exit 1; fi
name=aherald-ping-$tag-linux-$arch-musl
mkdir -p dist
[ ! -e "dist/$name" ] && [ ! -e "dist/$name.sha256" ] || { echo "Already exists: dist/$name" >&2; exit 1; }
cp "$binary" "dist/$name"
chmod 755 "dist/$name"
(cd dist && sha256sum "$name" > "$name.sha256")
# Run it where the host allows: no settings must be a quiet, explained failure of --self-test.
if [ "$(uname -m)" = "$arch" ]; then
  out=$(env -i "dist/$name" --self-test 2>&1 || true)
  case "$out" in *'no endpoint'*) ;; *) echo "Unexpected self-test output: $out" >&2; exit 1 ;; esac
fi
echo "Built and verified dist/$name"
