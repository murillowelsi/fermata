#!/bin/sh
# F-41: downloads the HOMUS dataset (Handwritten Online Musical Symbols, Calvo-Zaragoza & Oncina, ICPR 2014) for
# training the pen model. The dataset declares no licence: it is used for training with the permission of
# Fermata's maintainer and is NOT redistributed. It goes to tools/train-pen/data/ (ignored by git); only the trained
# weights (assets/pen_model.bin) ship with the app. See CREDITS.md.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
data="$here/data"
url="https://grfia.dlsi.ua.es/homus/HOMUS.zip"
sha="3e213b32f91fcf01aeb809d729b55a2fcd75b90ff49590eb005cdc0b492fc442"
mkdir -p "$data"
if [ ! -f "$data/HOMUS.zip" ]; then
  curl -fL --retry 3 -o "$data/HOMUS.zip.part" "$url"
  mv "$data/HOMUS.zip.part" "$data/HOMUS.zip"
fi
got="$(shasum -a 256 "$data/HOMUS.zip" | cut -d' ' -f1)"
if [ "$got" != "$sha" ]; then
  echo "HOMUS.zip: SHA-256 $got, expected $sha. Not using it." >&2
  exit 1
fi
rm -rf "$data/HOMUS"
unzip -q "$data/HOMUS.zip" -d "$data"
echo "HOMUS ready in $data/HOMUS"
