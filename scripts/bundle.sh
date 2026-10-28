#!/bin/sh
# Build a release binary and wrap it into target/Rustman.app.
# UNIVERSAL=1 builds a fat binary for Apple Silicon + Intel (needs rustup targets).
set -e
cd "$(dirname "$0")/.."
APP=target/Rustman.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"

if [ "$UNIVERSAL" = "1" ]; then
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output "$APP/Contents/MacOS/rustman" \
    target/aarch64-apple-darwin/release/rustman \
    target/x86_64-apple-darwin/release/rustman
else
  cargo build --release
  cp target/release/rustman "$APP/Contents/MacOS/rustman"
fi

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Rustman</string>
  <key>CFBundleIdentifier</key><string>local.rustman</string>
  <key>CFBundleExecutable</key><string>rustman</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "Built $APP"
