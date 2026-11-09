#!/bin/sh
# Builds "MIDI Spyglass.app" (Apple silicon + Intel in one) and installs it in /Applications.
set -e
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="MIDI Spyglass"
APP="dist/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp icon/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"   # (tools/make_icon.py draws it)
lipo -create -output "$APP/Contents/MacOS/midi-spyglass" \
    target/aarch64-apple-darwin/release/midi-spyglass target/x86_64-apple-darwin/release/midi-spyglass
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>com.pfkellogg.midi-spyglass</string>
  <key>CFBundleExecutable</key><string>midi-spyglass</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleVersion</key><string>$VER</string>
  <key>CFBundleShortVersionString</key><string>$VER</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --deep -s - "$APP"
rm -rf "/Applications/$NAME.app"
cp -R "$APP" /Applications/
echo "Installed /Applications/$NAME.app ($VER)"
