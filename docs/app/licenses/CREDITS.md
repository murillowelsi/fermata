# Credits

Fermata's own code, icons and colours are original work of the Fermata team. The toolbar icons are drawn in code
(`src/ui.rs`, `paint_icon`); no icon font is used.

## App icon

The app icon **"Traço"** (a white fermata drawn as a single-weight arch over a dot, on blue #007AFF) is original
artwork by the Fermata team (Design, F-14). Source: `assets/icone-traco-light.svg`; the sizes in
`assets/AppIcon.iconset/` are made from it. It is not taken from Encore or any other program.

## Fonts embedded in the app

| Font | Where | Used for | Licence |
|---|---|---|---|
| **Inter** 4.1 (Regular, Medium, SemiBold), © 2016 The Inter Project Authors, <https://github.com/rsms/inter> | `assets/fonts/` | Interface text: menus, toolbar, status bar, dialogs, tips | SIL Open Font License 1.1 (`assets/fonts/Inter-OFL.txt`) |
| **Bravura** 1.482, © 2015 Steinberg Media Technologies GmbH, <https://github.com/steinbergmedia/bravura> (Reserved Font Name "Bravura") | `assets/fonts/Bravura.otf` | Music symbols in the palettes and on the score page (SMuFL glyphs) | SIL Open Font License 1.1 (`assets/fonts/Bravura-OFL.txt`) |
| **Bravura metadata** (`Bravura.json`, the font's SMuFL metadata: glyph boxes, advance widths), Steinberg | `assets/fonts/Bravura.json` | Ink boxes that fit each palette symbol inside its cell | Distributed with the font in the same OFL release (`OFL.txt`) |
| **Browser version only:** Inter subsets (Latin, punctuation, arrows and key symbols) | `assets/fonts/web/Inter-*.ttf` | Interface text in the browser | SIL Open Font License 1.1 (`assets/fonts/Inter-OFL.txt`) |
| **Browser version only:** **Fermata Palette Glyphs**, a subset of Bravura 1.482 with only the palettes' glyphs (© 2015 Steinberg Media Technologies GmbH); renamed because "Bravura" is a Reserved Font Name and a subset is a modified version under the OFL. Its trimmed metadata: `FermataPaletteGlyphs.json` | `assets/fonts/web/` | Music symbols in the palettes, in the browser | SIL Open Font License 1.1 (`assets/fonts/Bravura-OFL.txt`; the web build carries it in `licenses/`) |
| **Ubuntu Light**, © Canonical Ltd. (bundled by egui) | inside the `epaint_default_fonts` crate | Text on the score page (titles, credits, chord symbols, lyrics, numbers); fallback for interface text | Ubuntu Font Licence 1.0 |
| **Hack**, © Source Foundry Authors (bundled by egui) | inside the `epaint_default_fonts` crate | Monospaced text | MIT (DejaVu parts public domain) |
| **egui-icons**, a subset of emoji-icon-font by John Slegers (bundled by egui) | inside the `epaint_default_fonts` crate | A few UI symbols egui itself draws | MIT |

Bravura and its metadata were downloaded from the official release `bravura-1.482` of
`steinbergmedia/bravura` (assets `Bravura.otf`, `Bravura.json`, `OFL.txt`), unchanged. The SMuFL glyph names used
to read the metadata come from the SMuFL standard (w3c/smufl). The score page uses Fermata's own
drawings for lines, beams, ties and braces (`src/glyphs.rs`); its symbols come from Bravura (F-03).

Inter was downloaded from the project's official release (`Inter-4.1.zip`, GitHub release v4.1); the three TTF
files are from its `extras/ttf/` folder, unchanged.

## Fonts inside exported and printed PDFs (F-06)

File ▸ Export PDF... and File ▸ Print... write a PDF that embeds **subsets** of the fonts the page uses (only the
glyphs that appear), so the PDF looks the same everywhere and its text can be selected:

- **Bravura** (music symbols), SIL Open Font License 1.1, which allows embedding fonts in documents;
- **Ubuntu Light** (the page's text), Ubuntu Font Licence 1.0, which allows embedding in documents (a document made
  with the font is not a derivative of it);
- a character neither font has (rare) comes from egui's own fallback fonts listed above, under their licences.

The fonts' names in the PDF carry a subset tag (`ABCDEF+Bravura`), as the PDF standard asks; the fonts' copyright
notices are kept in the subsets.

## Sound (F-30)

Playback, auditioning and the metronome use the **GeneralUser GS 2.0.3** General MIDI SoundFont by S. Christian
Collins (schristiancollins.com, github.com/mrbumpy409/GeneralUser-GS), shipped unchanged beside the app
(`GeneralUser-GS.sf2`; Mac: inside Fermata.app/Contents/Resources, Windows: beside fermata.exe). Its licence
(`licenses/GeneralUser-GS-LICENSE.txt`, "License v2.0") allows using it without restriction for music, private or
commercial, and in software projects. The author notes that he can't be 100% sure where every sample came from,
though none came from commercial packages. The file isn't kept in git: `tools/fetch_soundfont.sh` downloads it from
the official repository at a fixed commit and checks its SHA-256. Without it (or if it is damaged) the built-in
synthesizer plays, as before.

## Libraries

| Crate | Licence |
|---|---|
| egui, eframe, epaint 0.37 | MIT OR Apache-2.0 |
| serde, serde_json | MIT OR Apache-2.0 |
| image 0.25 | MIT OR Apache-2.0 |
| midir 0.11 | MIT |
| rfd 0.15 | MIT |
| cpal 0.18 | Apache-2.0 |
| winit 0.30 | Apache-2.0 |
| rustysynth 1.3 (SoundFont synthesizer, F-30) | MIT |
| miniz_oxide 0.8 (decompresses `.mxl` files and compresses PDF streams; already in the app through `png`) | MIT OR Zlib OR Apache-2.0 |
| pdf-writer 0.15.0 (writes the PDF, F-06), from the Typst project | MIT OR Apache-2.0 |
| subsetter 0.2.6 (font subsets for the PDF, F-06; default features off), from the Typst project | MIT OR Apache-2.0 |
| ryu 1.0 (number formatting, a dependency of pdf-writer) | Apache-2.0 OR BSL-1.0 |
| ed25519-dalek 2.2.0 and curve25519-dalek 4 (checks the signature of update files, F-34; Mac and Windows only) | BSD-3-Clause |
| sha2 0.10.9 (SHA-256 of update files, F-34) and its RustCrypto helpers (digest, block-buffer, crypto-common, generic-array, typenum, cpufeatures, subtle, zeroize, signature, ed25519) | MIT OR Apache-2.0 (generic-array: MIT; subtle: BSD-3-Clause) |

Updates (F-34) are downloaded with the operating system's own `curl` (built into macOS and Windows 10 and later), so
no HTTP or TLS library is built into Fermata. `tools/sign-update` (the release's signing tool, not shipped in the app)
also uses `getrandom` (MIT OR Apache-2.0) for new keys.

Their own dependencies carry their licences in `Cargo.lock`'s packages (`cargo metadata` lists them).

## Training data for the pen recognizer (F-40, F-41)

The pen model in `assets/pen_model.bin` (weights only) was trained on synthetic ink drawn by Fermata's own code and
on the **HOMUS** dataset (Handwritten Online Musical Symbols): Jorge Calvo-Zaragoza and Jose Oncina, "Recognition of
Pen-Based Music Notation: The HOMUS Dataset", 22nd International Conference on Pattern Recognition (ICPR), Stockholm,
2014, pp. 3038–3043, doi:10.1109/ICPR.2014.524. Source: https://grfia.dlsi.ua.es/homus/ (revised version:
https://github.com/apacha/Homus).

The dataset declares no licence; it is used for training with the authorization of Fermata's maintainer; the data
are not redistributed (Fermata ships only the trained weights; `tools/train-pen/fetch-homus.sh` downloads the data
for training, outside the repository). `assets/pen_model_synthetic.bin` is trained on synthetic ink only and can be
shipped instead (`--features pen-synthetic-only`).

## File formats

The MusicXML reader (`src/musicxml.rs`), its XML parser and its ZIP reader are Fermata's own code, written from the
public MusicXML 4.0 specification (W3C Music Notation Community Group) and the ZIP format note (PKWARE APPNOTE).
The test files in `tests/musicxml/` were written by the Fermata team; `tests/corpus/` holds scores made by the team.

## Compatibility

Fermata reads and writes Encore-compatible (`.enc`) files to work alongside that program. Fermata's icons, images,
fonts and sounds are not taken from Encore.
