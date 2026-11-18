#!/bin/sh
# F-30: download the General MIDI SoundFont that the installers ship (it is not kept in git: 32 MB).
# GeneralUser GS 2.0.3 by S. Christian Collins, from its official repository (meant for automated packaging),
# pinned to one commit and checked by SHA-256. Licence: licenses/GeneralUser-GS-LICENSE.txt.
set -e
cd "$(dirname "$0")/.."
OUT=assets/soundfont/GeneralUser-GS.sf2
SHA=9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe
URL=https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/684543d5e5efaef08d02be50dcda8d552478fa60/GeneralUser-GS.sf2
sum() { shasum -a 256 "$1" 2>/dev/null | cut -d' ' -f1 || sha256sum "$1" | cut -d' ' -f1; }
if [ -f "$OUT" ] && [ "$(sum "$OUT")" = "$SHA" ]; then
  echo "SoundFont present: $OUT"
  exit 0
fi
mkdir -p assets/soundfont
curl -fsSL -o "$OUT.part" "$URL"
if [ "$(sum "$OUT.part")" != "$SHA" ]; then
  rm -f "$OUT.part"
  echo "SoundFont download failed its checksum" >&2
  exit 1
fi
mv "$OUT.part" "$OUT"
echo "SoundFont downloaded: $OUT ($(wc -c < "$OUT") bytes)"
