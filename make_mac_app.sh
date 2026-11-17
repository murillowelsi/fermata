#!/bin/sh
# Builds Fermata.app for macOS: one universal program that runs natively on Intel and Apple-chip Macs
# (macOS 10.13 or newer), with its icon, in dist/.
set -e
cd "$(dirname "$0")"
export MACOSX_DEPLOYMENT_TARGET=10.13
rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null 2>&1 || true
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

APP=dist/Fermata.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/Fermata" \
  target/x86_64-apple-darwin/release/fermata target/aarch64-apple-darwin/release/fermata

# icon "Traço" (F-14): the approved PNGs, made from assets/icone-traco-light.svg
iconutil -c icns assets/AppIcon.iconset -o "$APP/Contents/Resources/Fermata.icns"

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
  <key>CFBundleShortVersionString</key><string>0.2.0</string>
  <key>CFBundleVersion</key><string>2</string>
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
2. Open Applications and double-click Fermata. macOS says it can't check the app, because it isn't notarized by Apple. Click Done (or Cancel).
3. Open System Settings > Privacy & Security, scroll down and click "Open Anyway" next to the Fermata message. Confirm with Open and your password.
   On macOS 14 or older you can instead right-click Fermata, choose Open, then click Open.
4. From then on Fermata opens normally.
Full guide: https://murillowelsi.github.io/fermata/
Works on Macs with Intel or Apple chips, macOS 10.13 or newer.
TXT
hdiutil create -volname "Fermata" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "Built $DMG"
