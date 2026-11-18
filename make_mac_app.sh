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
  <key>CFBundleShortVersionString</key><string>0.3.10</string>
  <key>CFBundleVersion</key><string>13</string>
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

# F-30: the General MIDI SoundFont (GeneralUser GS, ~32 MB) and its licence, in Resources where the app looks first
sh tools/fetch_soundfont.sh
cp assets/soundfont/GeneralUser-GS.sf2 "$APP/Contents/Resources/"
mkdir -p "$APP/Contents/Resources/licenses"
cp licenses/GeneralUser-GS-LICENSE.txt "$APP/Contents/Resources/licenses/"

# ad-hoc signature so macOS will run it (first launch: right-click > Open)
codesign --force --deep -s - "$APP"
echo "Built $APP"
lipo -info "$APP/Contents/MacOS/Fermata"

# installer disk image (F-14c): a 720x460 window with the Traço background, Fermata on the left and
# Applications on the right, and the Fermata icon on the disk. Laying out the window needs the Finder (a graphical
# session and, the first time, Automation permission); without it a plain .dmg is built and a warning printed.
# FERMATA_DMG_SIMPLE=1 skips the layout on purpose.
DMG=dist/Fermata-Installer.dmg
STAGE=dist/dmg-stage
RW=dist/dmg-rw.dmg
# During the layout the writable disk has a name of its own: the Finder finds disks by name, and another disk called
# "Fermata" (an installer someone left open) would get the layout instead. It's renamed "Fermata" at the end.
BUILDNAME="FermataBuild$$"
MNT="/Volumes/$BUILDNAME"
rm -rf "$STAGE" "$DMG" "$RW"
mkdir -p "$STAGE/.background"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp assets/dmg/fundo.tiff "$STAGE/.background/fundo.tiff"

# runs a command, stopping it after $1 seconds (a Finder waiting for a permission nobody answers mustn't hang the build)
with_timeout() {
  secs=$1; shift
  "$@" & cmd=$!
  ( sleep "$secs"; kill "$cmd" 2>/dev/null ) & guard=$!
  wait "$cmd"; rc=$?
  kill "$guard" 2>/dev/null; wait "$guard" 2>/dev/null
  return $rc
}

finder_layout() {
  osascript <<APPLESCRIPT
tell application "Finder"
  set v to disk "$BUILDNAME"
  open v
  delay 1
  set w to container window of v
  set current view of w to icon view
  set toolbar visible of w to false
  set statusbar visible of w to false
  set bounds of w to {200, 120, 920, 608}
  set opts to the icon view options of w
  set arrangement of opts to not arranged
  set icon size of opts to 128
  set text size of opts to 13
  set shows item info of opts to false
  set background picture of opts to file ".background:fundo.tiff" of v
  set position of item "Fermata.app" of v to {190, 250}
  set position of item "Applications" of v to {530, 250}
  update v without registering applications
  delay 2
  close w
end tell
APPLESCRIPT
}

# the binary .DS_Store holds the layout records as 4-letter codes
has_layout() {
  [ -f "$MNT/.DS_Store" ] && LC_ALL=C grep -aq Iloc "$MNT/.DS_Store" && LC_ALL=C grep -aq icvp "$MNT/.DS_Store" && LC_ALL=C grep -aq pBBk "$MNT/.DS_Store"
}

# the laid-out disk image; returns non-zero (after cleaning up) if any step fails
layout_dmg() {
  hdiutil create -volname "$BUILDNAME" -srcfolder "$STAGE" -ov -format UDRW -fs HFS+ "$RW" >/dev/null || return 1
  DEV=$(hdiutil attach -readwrite -noverify -noautoopen "$RW" | awk '/Apple_HFS/{print $1; exit}')
  [ -n "$DEV" ] && [ -d "$MNT" ] || { [ -n "$DEV" ] && hdiutil detach "$DEV" -force >/dev/null 2>&1; return 1; }
  if ! with_timeout 90 finder_layout; then
    hdiutil detach "$DEV" -force >/dev/null 2>&1
    return 1
  fi
  # the Finder writes the layout to .DS_Store a little after closing the window: wait for the icon positions,
  # window options and background, so a layout that didn't stick is caught instead of shipped
  n=0
  until has_layout; do
    n=$((n + 1))
    if [ $n -gt 30 ]; then
      echo "warning: the Finder didn't save the window layout to .DS_Store" >&2
      hdiutil detach "$DEV" -force >/dev/null 2>&1
      return 1
    fi
    sleep 0.5
  done
  diskutil rename "$DEV" "Fermata" >/dev/null || { hdiutil detach "$DEV" -force >/dev/null 2>&1; return 1; }
  MNT=$(diskutil info "$DEV" | sed -n 's/^ *Mount Point: *//p')
  # after the Finder, which rewrites the disk's information when it saves the layout
  cp assets/dmg/VolumeIcon.icns "$MNT/.VolumeIcon.icns"
  if command -v SetFile >/dev/null 2>&1; then
    SetFile -a C "$MNT"
    SetFile -a V "$MNT/.VolumeIcon.icns" "$MNT/.background"
  else
    echo "warning: SetFile not found (Xcode Command Line Tools); the disk keeps the default icon" >&2
  fi
  rm -rf "$MNT/.fseventsd"; sync
  hdiutil detach "$DEV" >/dev/null || hdiutil detach "$DEV" -force >/dev/null || return 1
  hdiutil convert "$RW" -format UDZO -imagekey zlib-level=9 -o "$DMG" >/dev/null || return 1
}

simple_dmg() {
  rm -f "$DMG"
  hdiutil create -volname "Fermata" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
}

if [ "${FERMATA_DMG_SIMPLE:-0}" = 1 ]; then
  echo "note: FERMATA_DMG_SIMPLE=1, building a plain .dmg without the window layout" >&2
  simple_dmg
elif ! layout_dmg; then
  echo "warning: couldn't lay out the .dmg window with the Finder (no graphical session, or no Automation permission);" >&2
  echo "warning: building a plain .dmg instead (it works, without the background and icon positions)" >&2
  simple_dmg
fi
rm -rf "$STAGE" "$RW"
echo "Built $DMG"
