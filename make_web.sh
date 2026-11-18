#!/bin/sh
# Builds Fermata for the browser into dist/web/: a static folder to copy to docs/app/ (served at /fermata/app/).
# index.html, fermata.js (+ snippets/), fermata_bg.wasm, manifest.webmanifest, sw.js, icons/. All paths are relative,
# so it works from a site root or a subfolder. Needs the wasm32 target and the wasm-bindgen CLI at the version in
# Cargo.lock (0.2.130); wasm-opt (binaryen) is used when installed and makes the download smaller.
set -e
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --profile web --target wasm32-unknown-unknown
OUT=dist/web
rm -rf "$OUT"; mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name fermata target/wasm32-unknown-unknown/web/fermata.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    -o "$OUT/fermata_bg.opt.wasm" "$OUT/fermata_bg.wasm"
  mv "$OUT/fermata_bg.opt.wasm" "$OUT/fermata_bg.wasm"
else
  echo "note: wasm-opt not found (brew install binaryen): the .wasm stays larger" >&2
fi
cp -R web/icons "$OUT/icons"
cp web/manifest.webmanifest "$OUT/"
# the fonts inside the app are under the SIL Open Font License: its text goes with them
mkdir -p "$OUT/licenses"
cp assets/fonts/Inter-OFL.txt "$OUT/licenses/Inter-OFL.txt"
cp assets/fonts/Bravura-OFL.txt "$OUT/licenses/Fermata-Palette-Glyphs-OFL.txt"
cp CREDITS.md "$OUT/licenses/CREDITS.md"
# the version: the app's version plus a fingerprint of the files, so every build gets its own cache
APPV=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
FP=$(cat "$OUT/fermata_bg.wasm" "$OUT/fermata.js" $(find "$OUT/snippets" -type f | sort) web/index.html web/sw.js \
  web/manifest.webmanifest | shasum | cut -c1-8)
VERSION="$APPV-$FP"
sed "s/__VERSION__/$VERSION/g" web/index.html > "$OUT/index.html"
# the helper scripts get the version too (their folder name doesn't change between builds), so a browser's cache
# never pairs a new fermata.js with an old helper; only the import lines change, not the wasm's module names
sed -e "s#^\(import .* from [\"']\./snippets/[^\"']*\.js\)\([\"']\)#\1?v=$VERSION\2#" "$OUT/fermata.js" > "$OUT/fermata.v.js"
mv "$OUT/fermata.v.js" "$OUT/fermata.js"
FILES=$(cd "$OUT" && { echo "./"; echo "./index.html"; echo "./fermata.js?v=$VERSION"; echo "./fermata_bg.wasm?v=$VERSION"; \
  find snippets -type f | sort | sed "s#^\(.*\)\$#./\1?v=$VERSION#"; \
  find icons manifest.webmanifest -type f | sort | sed 's#^#./#'; } | sed 's/.*/"&"/' | paste -sd, -)
sed -e "s/__VERSION__/$VERSION/g" -e "s#__FILES__#[$FILES]#" web/sw.js > "$OUT/sw.js"
ls -l "$OUT"
echo "Built $OUT, version $VERSION (try: cd $OUT && python3 -m http.server)"
