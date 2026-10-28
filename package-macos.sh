#!/bin/sh
# Builds aHerald.app (Apple Silicon) and the release archive, then proves that a clean
# extraction runs. Usage: ./package-macos.sh [path/to/aherald] [tag]
# Build first: cargo build --release --target aarch64-apple-darwin
set -eu
cd "$(dirname "$0")"

executable=${1:-target/aarch64-apple-darwin/release/aherald}
version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || { echo "Cannot read the crate version from Cargo.toml" >&2; exit 1; }
tag=${2:-v$version}
[ "$tag" = "v$version" ] || { echo "Tag $tag does not match Cargo.toml version $version" >&2; exit 1; }
[ -f "$executable" ] || { echo "Build the release executable first: $executable" >&2; exit 1; }

name=aherald-$tag-macos-arm64
out=dist
root=$out/$name
rm -rf "$root" "$out/$name.zip" "$out/$name.zip.sha256"
app=$root/aHerald.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$executable" "$app/Contents/MacOS/aherald"
cp -R assets "$app/Contents/Resources/assets"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>aHerald</string>
  <key>CFBundleDisplayName</key><string>aHerald</string>
  <key>CFBundleIdentifier</key><string>io.github.proprock.aherald</string>
  <key>CFBundleExecutable</key><string>aherald</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# Ad-hoc signature only: it makes the bundle internally consistent on Apple Silicon, which
# refuses to run unsigned code. Developer ID signing and notarization are not done.
codesign --force --sign - "$app"

for file in aHerald.example.toml LICENSE README.md CHANGELOG.md; do cp "$file" "$root/"; done
cp -R docs examples "$root/"
cp docs/portable-macos.md "$root/START-HERE.md"

{
  echo "aHerald $version"
  echo "Platform: macOS 11+ on Apple Silicon (arm64)"
  echo "Archive: $name.zip"
  echo
  echo "SHA-256  file"
  (cd "$root" && find . -type f ! -name MANIFEST.txt | sed 's|^\./||' | LC_ALL=C sort |
    while IFS= read -r f; do printf '%s  %s\n' "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$f"; done)
} > "$root/MANIFEST.txt"

(cd "$out" && ditto -c -k --keepParent "$name" "$name.zip" && shasum -a 256 "$name.zip" > "$name.zip.sha256")
echo "Built $out/$name.zip"
./check-release-macos.sh "$out/$name.zip" "$version"
