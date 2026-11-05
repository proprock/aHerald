#!/bin/sh
# Package a previously built static aherald-ping for one Linux architecture and check it.
# Usage: package-ping.sh x86_64|aarch64 [tag]   (build first, see below)
#   cargo build --locked --profile ping -p aherald-ping --target <arch>-unknown-linux-musl
set -eu
cd "$(dirname "$0")"
arch=${1:?usage: package-ping.sh x86_64|aarch64 [tag]}
case "$arch" in
  x86_64) machine='Advanced Micro Devices X86-64' ;;
  aarch64) machine='AArch64' ;;
  *) echo "Unknown architecture: $arch" >&2; exit 1 ;;
esac
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo 'Cannot read version' >&2; exit 1; }
tag=${2:-v$version}
[ "$tag" = "v$version" ] || { echo "Tag $tag does not match version $version" >&2; exit 1; }
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
