# Mechanics sheet

## How the original was read
* The download is a MindVision **Installer VISE** package. Stub `Encore5_Setup.exe`, `sub_401b50`: reads the last
  8 bytes, checks `0x56495345` ("VISE") and takes the payload offset (`0x00010000`).
* `sub_4016a0` lists the first-stage files (count u16; per file: name length u8, name, DOS date/time, size) and
  `sub_402080` decompresses each one. Mode 3 → `sub_402410` → `sub_404760` (inflate). The bit reader works in 16-bit
  units (`bk >= 16`, `n = bk >> 4` in `sub_404760`): it is **raw deflate stored as byte-swapped 16-bit words**.
  The level table at `0x40e2a8` is gzip's good/lazy/nice/chain table.
* The main file table (name, size, compressed size, flag, offset) uses offsets from base `0x10000`. Encore.exe
  (19,685,376 bytes, sha256 049c1166…) was decompressed this way and analysed.

## Values from Encore.exe
| Value | Where | Used in the rebuild |
|---|---|---|
| Ticks per quarter note = **240** | score header field `+0x2c` (= 0xF0 in every template and example file); written as `<divisions>` by `sub_463850` @ `0x463ba5` | MIDI and MusicXML export use 240 (internal 480, converted) |
| Score files start with `SCOW` (Windows, little-endian) / `SCO5` (Mac, big-endian) | template `.enc` headers | detection message when opening `.enc` |
| Dynamic marks, in Encore's order 0–22: p pp ppp pppp ppppp pppppp f ff fff ffff fffff ffffff mp mf sf sfp sfpp fp rf rfz sfz sffz fz | string table `0x4eb8a0`, read by `sub_491ef0` | Dynamics palette, playback, MusicXML |
| Dynamic → velocity: middle of the dynamic's range; random inside it with "Humanize" | help text at rsrc `0x1716530` (Dynamics velocity ranges) | `dyn_velocity_in` |
| Beam stem length = **7 half spaces** (allowed ≤ 16) | Spacing Defaults `sub_41ca60`, default at `0x4f7080` | stem length |
| Beam thickness = **100 % of a half space** (allowed 25–400) | `0x4f7082`, validated in `sub_41ca60` | beam thickness |
| Horizontal tie offset = **0** quarter spaces; vertical tie offset = **3** quarter spaces (≤ 8) | `0x4f7084`, `0x4f7086` | tie placement |
| Measure spacing = **2, 2, 2, 2** half spaces | `0x4f708c`–`0x4f7092` (dialog ids 0x848/0x849/0x84a/0x855) | space before the first note of a bar |
| Swing: 50.0–75.0 %, note type Whole / Half / Quarter / Eighth / 16th / 32nd | dialog resource 2010 "Swing Playback" | Swing dialog and playback |
| Change Velocity: set all / change to % / change smoothly from–to / add; note-on or note-off | dialog 1151 | Change Velocity dialog |
| Change Tempo: from measure–to; set all / % / smoothly / add | dialog 1111 | Tempo dialog |
| Spacing Defaults fields | dialog 2110 | View ▸ Spacing Defaults |
| New Score: staves per system, systems per page, measures per system, "C" score, Handwritten, template list, Create / Create then open Wizard | dialog 1330 | New Score dialog |
| Menu tree (File, Edit, Notes, Measures, Score, View, Windows, Setup, Help) | menu resource `THEMENU` | menus |
| Keyboard shortcuts | accelerator resource `THEACCEL` (e.g. Beam Group Ctrl+M, Add Mark Ctrl+9, Simplify Accidentals Ctrl+Shift+E, Percussion Staff Ctrl+Alt+1, Connect Staves Alt+[, Open/Close all palettes Alt+O/Alt+P, Undo Alt+Backspace, Cut Shift+Del, Copy Ctrl+Ins, Paste Shift+Ins) | `commands::shortcut`; on macOS Ctrl→Cmd and Ctrl+Alt→Ctrl as in the Mac chart |
| Instrument transpositions list | menu resource 1101 | (Staff Sheet transposition values) |

## Still estimated
* The default velocity range of each dynamic: no constant table in the program's data (two scans of `.data`/`.rdata`
  found none); values in `default_dynamic_ranges` are estimated.
* The default swing note type (eighth), the default tempo of a new score (120), the default note velocity (80).
* QWERTY pitch keys A W S E D F T G Y H U J K (the Quick Start lists the mode but not the keys).
* The `.enc` note data layout beyond the header (time resolution and header fields only).

## Encore score files (.enc) — read by the rebuild
Worked out from the shipped example and template files, checked against Encore.exe's MusicXML writer (`sub_463850`).
* Header: magic `SCOW` (Windows, little-endian) or `SCO5` (Mac, big-endian); ticks per quarter at 0x2c (240);
  number of tracks at 0x32; number of shown staves at 0x33 (a split/grand staff shows one track on two staves).
* `TITL`: UTF-16 strings — title, then subtitle.
* `MEAS` (one per measure): 62-byte header — +8 tempo in beats per minute (beat = +12 ticks), +12 beat length,
  +14 measure length (= 240·4·num/den), +16 numerator, +17 denominator; then events
  `[tick u16][kind u8][size u8][body]` until `FF FF`.
* Event kind high nibble: 9 note, 8 rest, 3 tie/slur (tie when both ends share a staff position), 6 text,
  4 beam group, 5 graphics; low nibble = voice − 1.
* Note/rest body: [0] track, bit 0x40 = lower staff of a split pair; [1] low nibble = value (1 whole … 7 64th);
  [6..8] x position; [8] staff position (steps above two steps below the bottom line); [10] dot flags (0x18 = dotted);
  [11] MIDI pitch; [12..14] played length (normally 80 % of the written length); [14] velocity.
* Not stored as plain fields (derived on import, so **inferred**): clef per staff (the clef whose positions match
  the MIDI pitches), key signature (the key that explains the most alterations), triplets (two-thirds spacing).
  Staff names and instruments are not read yet.

### .enc layout used for writing (Export Encore File)
Checked against all 20 example files and a user file:
* header 0xC2 bytes: 0x2E line count, 0x30 page count, 0x32 tracks, 0x33 shown staves, 0x34 measure count;
* one 0x86E-byte instrument block per track, then PAGE chunks (body 0x1A: +0 first line, +2 lines on the page,
  +12 staff rows on the page), LINE chunks (body 26 + 30 per shown staff: +0 page, +4 top y, +10 first measure,
  +12 measures in the line, +16 first line of the page; per staff +3 running staff row, +14/+18/+20 y values),
  MEAS chunks (header 0x3E incl. tag and length; +24 measure width, +28 x in the line, +40 width without the
  clef/key/time lead), then the rest (PREC, TITL, TEXT, FONT, COLR, DRUM, WINI) unchanged;
* note body byte 9 = tuplet as two hex digits (0x32 = 3 in the time of 2, 0x54 = 5:4); byte 16 bit 0x80 = stem down;
* rest body (14 bytes): +8 vertical position, +12..13 length in ticks;
* chord symbol body: +1 type, +2 bit 0x02 bass, +6 x, +8 root, +9 bass, +10 y.
Writing keeps the model file's instrument blocks and settings; it is verified by reading the written file back
(round trip of every example: same notes, rests, tuplets, chord symbols and title). Not checked in the original program.

### Keyboard layout (changed at the person's request)
Windows Encore's keys (Quick Start p.10-11 / THEACCEL) are the main shortcuts on every platform, also on the Mac:
Ctrl = main modifier, Ctrl+Alt = Measures/Score commands, Alt+Shift+letter = palettes. On the Mac the Mac chart's
Cmd keys also work (Cmd+S, Cmd+Z, Cmd+T...); its Ctrl keys are dropped because they clash with the Windows layout.
Quit stays Cmd+Q on the Mac. Lyric keys (Space, -, Shift+-, Shift+Space, Tab, Shift+Tab, Ctrl+Space, Ctrl+-) and the
Choose Chord dialog keys (a-g, A-G, 3/4, #/$, M, m, +, o, 7, &, 9, 1, 5) follow the Quick Start's charts.
