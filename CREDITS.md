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
| **Bravura** 1.482, © 2015 Steinberg Media Technologies GmbH, <https://github.com/steinbergmedia/bravura> (Reserved Font Name "Bravura") | `assets/fonts/Bravura.otf` | Music symbols in the palettes (SMuFL glyphs) | SIL Open Font License 1.1 (`assets/fonts/Bravura-OFL.txt`) |
| **Bravura metadata** (`Bravura.json`, the font's SMuFL metadata: glyph boxes, advance widths), Steinberg | `assets/fonts/Bravura.json` | Ink boxes that fit each palette symbol inside its cell | Distributed with the font in the same OFL release (`OFL.txt`) |
| **Ubuntu Light**, © Canonical Ltd. (bundled by egui) | inside the `epaint_default_fonts` crate | Text on the score page; fallback for interface text | Ubuntu Font Licence 1.0 |
| **Hack**, © Source Foundry Authors (bundled by egui) | inside the `epaint_default_fonts` crate | Monospaced text | MIT (DejaVu parts public domain) |
| **egui-icons**, a subset of emoji-icon-font by John Slegers (bundled by egui) | inside the `epaint_default_fonts` crate | A few UI symbols egui itself draws | MIT |

Bravura and its metadata were downloaded from the official release `bravura-1.482` of
`steinbergmedia/bravura` (assets `Bravura.otf`, `Bravura.json`, `OFL.txt`), unchanged. The SMuFL glyph names used
to read the metadata come from the SMuFL standard (w3c/smufl). The score page itself still uses Fermata's own
drawings (`src/glyphs.rs`); moving the page to Bravura is task F-03.

Inter was downloaded from the project's official release (`Inter-4.1.zip`, GitHub release v4.1); the three TTF
files are from its `extras/ttf/` folder, unchanged.

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

Their own dependencies carry their licences in `Cargo.lock`'s packages (`cargo metadata` lists them).

## Compatibility

Fermata reads and writes Encore-compatible (`.enc`) files to work alongside that program. Fermata's icons, images,
fonts and sounds are not taken from Encore.
