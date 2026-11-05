#!/bin/sh
# Build a system package without running package scripts or changing user homes.
set -eu
umask 022
cd "$(dirname "$0")"
executable=${1:-target/release/aherald}
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo 'Cannot read version' >&2; exit 1; }
tag=${2:-v$version}
[ "$tag" = "v$version" ] || { echo "Tag $tag does not match version $version" >&2; exit 1; }
[ -x "$executable" ] || { echo "Build executable first: $executable" >&2; exit 1; }
readelf -h "$executable" | grep -q 'Advanced Micro Devices X86-64' || { echo 'Executable is not x86_64 ELF' >&2; exit 1; }
readelf -l "$executable" | grep -q 'ld-linux-x86-64.so.2' || { echo 'Executable must use the GNU libc loader' >&2; exit 1; }
deb_version=$(printf '%s' "$version" | sed 's/-/~/')
name=aherald-${tag}-linux-x86_64-gnu.deb
mkdir -p dist
[ ! -e "dist/$name" ] && [ ! -e "dist/$name.sha256" ] || { echo "Package already exists: dist/$name" >&2; exit 1; }
stage=$(mktemp -d "${TMPDIR:-/tmp}/aherald-deb.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
root=$stage/root
mkdir -p "$root/DEBIAN" "$root/usr/lib/aherald" "$root/usr/bin" "$root/usr/share/doc/aherald" "$root/usr/share/applications" "$root/usr/share/man/man1"
cp "$executable" "$root/usr/lib/aherald/aherald"
cp -R assets "$root/usr/lib/aherald/assets"
cp LICENSE README.md docs/installation.md "$root/usr/share/doc/aherald/"
cp LICENSE "$root/usr/share/doc/aherald/copyright"
gzip -9n -c CHANGELOG.md > "$root/usr/share/doc/aherald/changelog.gz"
gzip -9n -c docs/aherald.1 > "$root/usr/share/man/man1/aherald.1.gz"
ln -s ../lib/aherald/aherald "$root/usr/bin/aherald"
cat > "$root/DEBIAN/control" <<EOF
Package: aherald
Version: $deb_version
Architecture: amd64
Maintainer: aHerald contributors <proprock@gmail.com>
Depends: libc6 (>= 2.35), libgcc-s1, libcairo2, libpango-1.0-0, libpangocairo-1.0-0, libglib2.0-0, libpulse0
Section: utils
Priority: optional
Description: Desktop notifications for coding agents
 Native notification companion for AI coding agents: Codex, Claude Code, OpenCode, Pi, Goose and Crush.
EOF
cat > "$root/usr/share/applications/aherald.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=aHerald
Comment=Notifications for coding agents
Exec=/usr/bin/aherald --resident
Terminal=false
Categories=Utility;
EOF
cat > "$root/usr/share/applications/aherald-setup.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Set up aHerald
Comment=Configure your coding-agent hooks
Exec=/usr/bin/aherald --console-setup
Terminal=true
Categories=Utility;
EOF
chmod -R go-w "$root"
dpkg-deb --root-owner-group --build "$root" "dist/$name"
(cd dist && sha256sum "$name" > "$name.sha256")
dpkg-deb --info "dist/$name" >/dev/null
dpkg-deb --extract "dist/$name" "$stage/extracted"
test -x "$stage/extracted/usr/lib/aherald/aherald"
test -f "$stage/extracted/usr/lib/aherald/assets/turn_complete/animation.toml"
case "$("$stage/extracted/usr/lib/aherald/aherald" --version)" in
  "aHerald $version (built 20"*" UTC)") ;;
  *) echo "The packaged program reports another version" >&2; exit 1 ;;
esac
echo "Built and verified dist/$name"
