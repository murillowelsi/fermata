#!/bin/sh
# Builds the portable Windows version from this Mac (target x86_64-pc-windows-gnu, mingw-w64 from Homebrew):
# dist/Fermata-Windows-x64.zip with fermata.exe, the General MIDI SoundFont beside it (F-30), the SoundFont's
# licence and CREDITS.md. Unzip anywhere and run fermata.exe; nothing is installed.
set -e
cd "$(dirname "$0")"
rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true
cargo build --release --target x86_64-pc-windows-gnu
sh tools/fetch_soundfont.sh

OUT=dist/windows
ZIP=dist/Fermata-Windows-x64.zip
rm -rf "$OUT" "$ZIP"
mkdir -p "$OUT/licenses"
cp target/x86_64-pc-windows-gnu/release/fermata.exe "$OUT/"
x86_64-w64-mingw32-strip "$OUT/fermata.exe" 2>/dev/null || echo "warning: mingw strip not found; fermata.exe is not stripped"
cp assets/soundfont/GeneralUser-GS.sf2 "$OUT/"
cp licenses/GeneralUser-GS-LICENSE.txt "$OUT/licenses/"
cp CREDITS.md "$OUT/"
(cd "$OUT" && zip -q -r -X "../Fermata-Windows-x64.zip" .)
echo "Built $ZIP ($(wc -c < "$ZIP" | tr -d ' ') bytes)"
unzip -l "$ZIP"
