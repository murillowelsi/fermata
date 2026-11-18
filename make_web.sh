#!/bin/sh
# Builds the browser version of Fermata (F-23 spike) into dist/web/: index.html, fermata.js, fermata_bg.wasm, icon.png.
# A static folder: serve it from a site root or a subfolder (all paths are relative). Needs the wasm32 target and the
# wasm-bindgen CLI at the same version as the wasm-bindgen crate in Cargo.lock (0.2.130).
set -e
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --profile web --target wasm32-unknown-unknown
OUT=dist/web
rm -rf "$OUT"; mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name fermata target/wasm32-unknown-unknown/web/fermata.wasm
cp web/index.html "$OUT/index.html"
cp assets/AppIcon.iconset/icon_32x32@2x.png "$OUT/icon.png"
ls -l "$OUT"
echo "Built $OUT (try: cd $OUT && python3 -m http.server)"
