#!/bin/sh
# Builds Fermata.app for macOS: one universal program that runs natively on Intel and Apple-chip Macs
# (macOS 10.13 or newer), with its icon, in dist/.
set -e
cd "$(dirname "$0")"
export MACOSX_DEPLOYMENT_TARGET=10.13
rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1 || true
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build

APP=dist/Fermata.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/Fermata" \
  target/x86_64-apple-darwin/release/fermata target/aarch64-apple-darwin/release/fermata

# icon
ICONSET=dist/Fermata.iconset
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  ./target/debug/fermata --icon-png "$ICONSET/icon_${s}x${s}.png" $s
  ./target/debug/fermata --icon-png "$ICONSET/icon_${s}x${s}@2x.png" $((s * 2))
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Fermata.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Fermata</string>
  <key>CFBundleDisplayName</key><string>Fermata</string>
  <key>CFBundleIdentifier</key><string>app.fermata.notation</string>
  <key>CFBundleExecutable</key><string>Fermata</string>
  <key>CFBundleIconFile</key><string>Fermata</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>10.13</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Score</string>
      <key>CFBundleTypeExtensions</key><array><string>fmt</string><string>enc</string><string>mid</string><string>midi</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string>
    </dict>
  </array>
</dict>
</plist>
PLIST

# ad-hoc signature so macOS will run it (first launch: right-click > Open)
codesign --force --deep -s - "$APP"
echo "Built $APP"
lipo -info "$APP/Contents/MacOS/Fermata"

# installer disk image: open it and drag Fermata into Applications
DMG=dist/Fermata-Installer.dmg
STAGE=dist/dmg-stage
rm -rf "$STAGE" "$DMG"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/How to install.txt" <<TXT
1. Drag Fermata onto the Applications folder.
2. Open Applications, right-click Fermata and choose Open (only the first time).
3. Click Open in the window that asks for confirmation.
Works on Macs with Intel or Apple chips, macOS 10.13 or newer.
TXT
hdiutil create -volname "Fermata" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "Built $DMG"
