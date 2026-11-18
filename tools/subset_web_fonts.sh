#!/bin/sh
# Smaller fonts for the browser build (F-23): the desktop app keeps the full files.
# Inter: Latin, punctuation, arrows and the key symbols the menus use; anything else falls back to egui's fonts.
# Bravura: only the SMuFL glyphs the palettes draw (the score page still uses glyphs.rs), and their boxes.
# Needs fonttools: python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools; then FT=/tmp/ft/bin sh tools/subset_web_fonts.sh
set -e
cd "$(dirname "$0")/.."
FT=${FT:-}
OUT=assets/fonts/web
mkdir -p "$OUT"
U="U+0020-007E,U+00A0-017F,U+0192,U+02C6-02DC,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2212,U+2300-23FF,U+25A0-25FF,U+2600-26FF,U+2713-2714,U+2715,U+2717"
for w in Regular Medium SemiBold; do
  "${FT:+$FT/}pyftsubset" assets/fonts/Inter-$w.ttf --unicodes="$U" --layout-features='kern,liga,calt,tnum' --output-file="$OUT/Inter-$w.ttf"
done
CPS=$(grep -o '\\u{[0-9A-Fa-f]*}' src/palettes.rs | sort -u | sed 's/\\u{\(.*\)}/U+\1/' | paste -sd, -)
# Bravura's licence reserves its name ("Reserved Font Name"), and a subset is a modified version under the OFL, so
# the subset gets its own name, "Fermata Palette Glyphs"; the copyright and licence entries stay in the font, and
# the OFL text goes with the web build (make_web.sh copies it to licenses/).
"${FT:+$FT/}pyftsubset" assets/fonts/Bravura.otf --unicodes="$CPS" --name-IDs='*' --name-languages='*' --name-legacy \
  --output-file="$OUT/FermataPaletteGlyphs.otf"
rm -f "$OUT/Bravura.otf" "$OUT/Bravura.json"
"${FT:+$FT/}python3" - <<'PY'
import json, re
from fontTools.ttLib import TTFont
f = TTFont("assets/fonts/web/FermataPaletteGlyphs.otf")
fam, ps = "Fermata Palette Glyphs", "FermataPaletteGlyphs-Regular"
for rec in list(f["name"].names):
    if rec.nameID in (16, 17, 21, 22):
        f["name"].removeNames(nameID=rec.nameID)
for nid, val in ((1, fam), (2, "Regular"), (3, ps + ";subset of Bravura"), (4, fam), (6, ps)):
    f["name"].setName(val, nid, 3, 1, 0x409)
    f["name"].setName(val, nid, 1, 0, 0)
cff = f["CFF "].cff
cff.fontNames = [ps]
top = cff.topDictIndex[0]
top.FamilyName, top.FullName = fam, fam
f.save("assets/fonts/web/FermataPaletteGlyphs.otf")
names = re.findall(r"'\\u\{[0-9A-Fa-f]+\}', \"([A-Za-z0-9]+)\"", open("src/palettes.rs").read())
meta = json.load(open("assets/fonts/Bravura.json"))
small = {"fontName": fam, "basedOn": meta["fontName"] + " " + str(meta["fontVersion"]), "glyphBBoxes": {n: meta["glyphBBoxes"][n] for n in names}}
json.dump(small, open("assets/fonts/web/FermataPaletteGlyphs.json", "w"), separators=(",", ":"))
print(len(names), "glyph boxes")
PY
ls -l "$OUT"
