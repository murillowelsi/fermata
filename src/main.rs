mod app;
mod audio;
mod commands;
mod devshots;
mod enc;
mod exec;
mod glyphs;
mod icon;
mod input;
mod io;
mod kit;
mod midi_in;
mod midi_out;
mod prefs;
mod model;
mod palettes;
mod platform;
mod rec;
mod score_view;
mod theme;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Shell {
    app: app::App,
    frames: u32,
    shot_path: Option<std::path::PathBuf>,
    shot_requested: bool,
    shot_done: Arc<AtomicBool>,
    /// `--palette-sheet`: all 11 palettes side by side on a plain background, then a pixel check of every cell.
    sheet: bool,
    /// `--clicks` / `--hover`: scripted mouse steps for screenshots of menus, submenus and tips.
    steps: Vec<devshots::Step>,
}

/// Writes the pixel check of the palettes next to a screenshot (`<name>.txt`) and prints its summary.
fn write_palette_check(img: &egui::ColorImage, path: &std::path::Path, ppp: f32, cells: &[palettes::CellLog], behind: egui::Color32) {
    let (checked, bad) = palettes::verify_pixels(img, ppp, cells, behind);
    let mut txt = format!(
        "Palette pixel check: {} of {checked} cells with ink outside the inner area (margin max(3 pt, 12% of the shorter side), 1 physical px tolerance, {:.0} px/pt).\n",
        bad.len(),
        ppp
    );
    for b in &bad {
        txt.push_str(&format!("  {b}\n"));
    }
    print!("{txt}");
    let _ = std::fs::write(path.with_extension("txt"), txt);
}

fn save_png(img: &egui::ColorImage, path: &std::path::Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, platform::png_bytes(img)).map_err(|e| e.to_string())
}

impl eframe::App for Shell {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        devshots::inject(&self.steps, self.frames, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        let a = &mut self.app;

        // MIDI keyboard: look for devices now and then, take its notes, grow notes being recorded
        if !a.screenshot_mode {
            if a.midi.is_none() {
                a.midi = Some(midi_in::MidiIn::new());
                a.midi_out = Some(midi_out::MidiOut::new());
                a.prefs = prefs::load();
                // the saved View ▸ Appearance, unless --theme was given
                if !a.theme_forced {
                    ctx.set_theme(a.prefs.appearance.preference());
                }
                a.apply_prefs();
            }
            // MIDI out: find the chosen ports (a piano switched on later is picked up)
            let out_scan = a.midi_out.as_ref().map(|m| m.last_scan.map(|t| t.elapsed().as_secs_f32() > 2.0).unwrap_or(true)).unwrap_or(false);
            if out_scan {
                if let Some(m) = a.midi_out.as_mut() {
                    m.scan();
                }
            }
            let outs = a.midi_out.as_ref().map(|m| m.conns.clone()).unwrap_or_else(|| [std::sync::Arc::new(std::sync::Mutex::new(None)), std::sync::Arc::new(std::sync::Mutex::new(None))]);
            // inputs are checked every 2 seconds too, so a piano switched off and on is picked up again
            let need_scan = a.midi.as_ref().map(|m| m.last_scan.map(|t| t.elapsed().as_secs_f32() > 2.0).unwrap_or(true)).unwrap_or(false);
            if need_scan {
                let synth = Some(a.audio_mut().handle());
                if let Some(m) = a.midi.as_mut() {
                    m.scan(&ctx, synth, outs.clone());
                }
            }
            let prog = a.score.staves.get(a.cursor.0).map(|s| s.program).unwrap_or(0);
            let msgs: Vec<midi_in::MidiMsg> = match a.midi.as_ref() {
                Some(m) => {
                    m.thru_program.store(prog, std::sync::atomic::Ordering::Relaxed);
                    m.thru_on.store(a.audition, std::sync::atomic::Ordering::Relaxed);
                    let (tp, tc) = a.thru_target();
                    let routed = a.midi_out.as_ref().map(|o| o.connected[tp as usize].is_some()).unwrap_or(false);
                    m.thru_port.store(if routed { tp } else { 255 }, std::sync::atomic::Ordering::Relaxed);
                    m.thru_channel.store(tc, std::sync::atomic::Ordering::Relaxed);
                    m.rx.try_iter().collect()
                }
                None => vec![],
            };
            for msg in msgs {
                a.handle_midi(msg);
            }
            if a.rec.is_some() {
                a.rec_tick();
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        a.poll_opened();
        a.handle_shortcuts(&ctx);
        if let Some(c) = a.pending_cmd.take() {
            a.exec(c);
        }

        let k = theme::ctx_tokens(&ctx);
        if self.sheet {
            // contact sheet: the 11 palettes in a row on a plain background, nothing else
            let mut x = 20.0;
            let mut pos = vec![];
            for i in 0..11 {
                pos.push(egui::pos2(x, 24.0));
                let (_, size) = palettes::grid_layout(&palettes::palette_def(i, a.tuplet_ratio));
                x += size.x.max(76.0) + 10.0 + 14.0;
            }
            a.palette_sheet_pos = Some(pos);
            a.palettes = [true; 11];
            a.palette_cell_log = Some(vec![]);
            egui::CentralPanel::default().frame(egui::Frame::new().fill(k.canvas)).show(ui, |_| {});
            a.palettes_ui(&ctx);
        } else {
            if a.palette_cell_log.is_some() {
                a.palette_cell_log = Some(vec![]);
            }
            egui::Panel::top("menus").frame(egui::Frame::new().fill(k.win).inner_margin(egui::Margin::symmetric(8, 2))).show(ui, |ui| a.menu_bar(ui));
            // the toolbar (Windows ▸ Toolbar shows or hides it): voice, tools, transport, zoom and the measure
            if a.toolbar_open {
                egui::Panel::top("score_toolbar")
                    .exact_size(44.0)
                    .frame(egui::Frame::new().fill(k.win).inner_margin(egui::Margin::symmetric(10, 0)))
                    .show(ui, |ui| a.score_toolbar(ui));
            }
            egui::Panel::bottom("status").exact_size(24.0).frame(egui::Frame::new().fill(k.win).inner_margin(egui::Margin::symmetric(6, 0))).show(ui, |ui| a.status_bar(ui));
            egui::CentralPanel::default().frame(egui::Frame::new().fill(k.canvas)).show(ui, |ui| a.score_area(ui));
            // red warning over the score when something didn't fit in the bar
            if let Some((msg, t, _)) = a.warn.clone() {
                let age = t.elapsed().as_secs_f32();
                if age < 4.0 {
                    egui::Area::new(egui::Id::new("fit_warning"))
                        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 92.0))
                        .order(egui::Order::Foreground)
                        .interactable(false)
                        .show(&ctx, |ui| {
                            egui::Frame::new()
                                .fill(k.danger)
                                .corner_radius(9.0)
                                .shadow(ctx.global_style().visuals.popup_shadow)
                                .inner_margin(egui::Margin::symmetric(14, 8))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(format!("⚠  {msg}")).font(theme::font_semibold(13.0)).color(k.danger_fg));
                                });
                        });
                    ctx.request_repaint();
                } else {
                    a.warn = None;
                }
            }
            a.palettes_ui(&ctx);
            a.side_windows(&ctx);
            a.dialogs(&ctx);
        }
        if let Some(c) = a.pending_cmd.take() {
            a.exec(c);
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(a.title()));
        if a.is_playing() || self.shot_path.is_some() {
            ctx.request_repaint();
        }

        // Print / Export Page Image: save what the window shows.
        if a.export_png_requested {
            a.export_png_requested = false;
            if let Some(p) = platform::pick_save_path(&a.file_base_name(), "png", "PNG image") {
                ctx.request_screenshot(move |img| {
                    let _ = save_png(&img, &p);
                });
                a.status = "Page image saved. (Paper printing: open the PNG or a MusicXML export in your system's print dialog.)".into();
            }

        }

        if a.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if let Some(path) = self.shot_path.clone() {
            ctx.request_repaint();
            // after the last scripted mouse step, give menus and tips (0.5 s) time to appear
            let shot_frame = self.steps.iter().map(|s| s.frame + 50).max().unwrap_or(0).max(60);
            if self.frames >= shot_frame && !self.shot_requested {
                self.shot_requested = true;
                let done = self.shot_done.clone();
                let cells = self.app.palette_cell_log.clone();
                let ppp = ctx.pixels_per_point();
                // the cells sit on the palette panel, which is slightly see-through over the canvas
                let tk = theme::ctx_tokens(&ctx);
                let behind = theme::blend(tk.canvas, tk.pal_bg);
                ctx.request_screenshot(move |img| {
                    if let Err(e) = save_png(&img, &path) {
                        eprintln!("screenshot failed: {e}");
                    }
                    if let Some(cells) = cells {
                        write_palette_check(&img, &path, ppp, &cells, behind);
                    }
                    done.store(true, Ordering::SeqCst);
                });
            }
            if self.shot_done.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frames > 600 {
                std::process::exit(1);
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut shot = None;
    let mut open = None;
    let mut force_theme = None;
    let mut open_palettes: Option<Vec<usize>> = None;
    let mut open_dialog: Option<String> = None;
    let mut steps: Vec<devshots::Step> = vec![];
    let mut sheet = false;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--icon-png" && i + 1 < args.len() {
            let size: u32 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(256);
            let _ = image::save_buffer(&args[i + 1], &icon::rgba(size), size, size, image::ColorType::Rgba8);
            return Ok(());
        }
        if args[i] == "--click-selftest" {
            click_selftest();
            return Ok(());
        }
        if args[i] == "--keys-selftest" {
            let all = commands::all_shortcuts();
            let name = |s: &egui::KeyboardShortcut| {
                let m = s.modifiers;
                format!("{}{}{}{}{:?}", if m.mac_cmd || m.command && !m.ctrl { "Cmd+" } else { "" }, if m.ctrl { "Ctrl+" } else { "" }, if m.alt { "Alt+" } else { "" }, if m.shift { "Shift+" } else { "" }, s.logical_key)
            };
            let mut seen: std::collections::HashMap<String, commands::Cmd> = Default::default();
            let mut conflicts = 0;
            for (s, c) in &all {
                let k = name(s);
                if let Some(prev) = seen.insert(k.clone(), *c) {
                    if prev != *c {
                        conflicts += 1;
                        println!("CONFLICT {k}: {prev:?} vs {c:?}");
                    }
                }
            }
            println!("{} key combinations, {conflicts} conflicts", all.len());
            for (s, c) in &all {
                if matches!(c, commands::Cmd::BeamGroup | commands::Cmd::AddMeasure | commands::Cmd::BeamOnBeat | commands::Cmd::Copy) {
                    println!("{:?}: {}", c, name(s));
                }
            }
            // two eighths on beat 4 after a dotted-half chord, saved for a screenshot
            let mut app = app::App::new(true);
            for p in [60, 64, 67] {
                app.enter(0, 0, 0, Some(model::Pitch::from_midi(p, false)), 2, 1, false);
            }
            app.enter(0, 0, 3 * model::TPQ, Some(model::Pitch::from_midi(64, false)), 4, 0, false);
            app.enter(0, 0, 3 * model::TPQ + model::TPQ / 2, Some(model::Pitch::from_midi(65, false)), 4, 0, false);
            let _ = std::fs::write("shots/beam.fmt", io::score_bytes(&app.score).unwrap());
            println!("events: {:?}", app.score.events[0][0].iter().map(|e| (e.start, e.value, e.dots, e.beam)).collect::<Vec<_>>());
            return Ok(());
        }
        if args[i] == "--fit-selftest" {
            let mut app = app::App::new(true);
            for k in 0..3u32 {
                app.enter(0, 0, k * model::TPQ, Some(model::Pitch::from_midi(60 + k as i32 * 2, false)), 3, 0, false);
            }
            // a half note on beat 4 of a 4/4 bar needs 5 beats
            app.enter(0, 0, 3 * model::TPQ, Some(model::Pitch::from_midi(67, false)), 2, 0, false);
            println!("after half on beat 4: {} notes; warning: {:?}", app.score.events[0][0].len(), app.warn.as_ref().map(|w| w.0.clone()));
            // a quarter on beat 4 fits
            app.warn = None;
            app.enter(0, 0, 3 * model::TPQ, Some(model::Pitch::from_midi(67, false)), 3, 0, false);
            println!("after quarter on beat 4: {} notes; warning: {:?}", app.score.events[0][0].len(), app.warn.as_ref().map(|w| w.0.clone()));
            // lengthen the last quarter into a half: must be undone
            app.selected.clear();
            app.selected.insert(score_view::NoteRef { staff: 0, measure: 0, idx: 3 });
            let n = app.modify(|e| e.value = 2);
            println!("lengthen to half: changed {n}, last note value {}, warning: {}", app.score.events[0][0][3].value, app.warn.is_some());
            return Ok(());
        }
        if args[i] == "--tuplet-selftest" {
            // pencil entry with 3:2: six quarter-note triplets fill a 4/4 bar; a seventh is refused
            let mut app = app::App::new(true);
            app.cur_tuplet = true;
            let g = app.grid();
            let mut ok = 0;
            for k in 0..7u32 {
                let r = app.enter(0, 0, k * g, Some(model::Pitch::from_midi(60 + k as i32, false)), 3, 0, false);
                if r.idx != usize::MAX {
                    ok += 1;
                }
            }
            println!("grid {g}, entered {ok} of 7 triplet quarters (want 320 and 6)");
            {
                let mut b = app::App::new(true);
                b.pick_item(app::Item::Note(2));
                b.pick_item(app::Item::Dot(1));
                let r = b.enter(0, 0, 0, Some(model::Pitch::from_midi(64, false)), b.cur_value, b.cur_dots, false);
                println!("figure + dot: item {:?}, dots {}, entered value {} dots {}", b.item, b.cur_dots, b.score.events[0][0][r.idx].value, b.score.events[0][0][r.idx].dots);
            }
            // 4/4: three quarters, then a click near the bar line (snapped to 1680) still adds the fourth on beat 4
            {
                let mut b = app::App::new(true);
                for t in [0u32, 480, 960, 1680] {
                    b.enter(0, 0, t, Some(model::Pitch::from_midi(64, false)), 3, 0, false);
                }
                b.enter(0, 1, 0, Some(model::Pitch::from_midi(64, false)), 2, 0, false);
                b.enter(0, 1, 1200, Some(model::Pitch::from_midi(65, false)), 2, 0, false);
                // bar 3: three quarters left on off-beats (0, 480, 1200), then a fourth must still go in
                b.score.push_measure();
                for t in [0u32, 480, 1200] {
                    b.score.events[0][2].push(model::Event::new(1, t, 3, vec![model::Pitch::from_midi(67, false)]));
                }
                let r = b.enter(0, 2, 1680, Some(model::Pitch::from_midi(60, false)), 3, 0, false);
                let o: Vec<u32> = b.score.events[0][2].iter().map(|e| e.start).collect();
                println!("off-beat bar: {o:?} accepted={} (want 0,480,960,1440)", r.idx != usize::MAX);
                let q: Vec<u32> = b.score.events[0][0].iter().map(|e| e.start).collect();
                let h: Vec<u32> = b.score.events[0][1].iter().map(|e| e.start).collect();
                println!("4/4 quarters at {q:?} (want 0,480,960,1440); halves at {h:?} (want 0,960)");
            }
            // Ctrl+M on three plain quarters in bar 2
            app.cur_tuplet = false;
            app.score.push_measure();
            let mut refs = vec![];
            for k in 0..3u32 {
                refs.push(app.enter(0, 1, k * model::TPQ, Some(model::Pitch::from_midi(64, false)), 3, 0, false));
            }
            app.selected = refs.into_iter().collect();
            app.exec(commands::Cmd::BeamGroup);
            let st: Vec<(u32, Option<(u8, u8)>)> = app.score.events[0][1].iter().map(|e| (e.start, e.tuplet)).collect();
            println!("after Ctrl+M: {st:?} / {}", app.status);
            // eighths stay beamed
            app.score.push_measure();
            let mut refs = vec![];
            for k in 0..4u32 {
                refs.push(app.enter(0, 2, k * model::TPQ / 2, Some(model::Pitch::from_midi(67, false)), 4, 0, false));
            }
            app.selected = refs.into_iter().collect();
            app.exec(commands::Cmd::BeamGroup);
            println!("4 eighths: {}", app.status);
            // triplet quarters left on plain-quarter spacing (0, 240, 480...) go back to 0, 320, 640...
            app.score.push_measure();
            for k in 0..6u32 {
                let mut e = model::Event::new(1, k * 240, 3, vec![model::Pitch::from_midi(48, false)]);
                e.tuplet = Some((3, 2));
                e.dx = 1.5;
                app.score.events[0][3].push(e);
            }
            app.selected.clear();
            app.sel_measures = Some((3, 3));
            app.exec(commands::Cmd::AlignSpacing);
            let st: Vec<(u32, f32)> = app.score.events[0][3].iter().map(|e| (e.start, e.dx)).collect();
            println!("align spacing: {st:?} / {}", app.status);
            return Ok(());
        }
        if args[i] == "--tie-selftest" {
            // whole-note F major chords in bars 1 and 2 (and the last bar of page 1 to the first of page 2): Ctrl+T
            let mut app = app::App::new(true);
            while app.score.measures.len() < 30 {
                app.score.push_measure();
            }
            let chord = |app: &mut app::App, m: usize| {
                let mut r = app.enter(0, m, 0, Some(model::Pitch::from_midi(65, false)), 1, 0, false);
                for p in [69, 72] {
                    r = app.enter(0, m, 0, Some(model::Pitch::from_midi(p, false)), 1, 0, false);
                }
                r
            };
            let a = chord(&mut app, 0);
            let b = chord(&mut app, 1);
            app.selected = [a, b].into_iter().collect();
            app.exec(commands::Cmd::Tie);
            println!("same page: {} ties {:?}", app.status, app.score.events[0][0][0].ties);
            let geo = score_view::layout(&app.score, egui::Pos2::ZERO, 1.0, false);
            let last_p1 = geo.systems.iter().filter(|s| s.page == 0).map(|s| s.measures.last().unwrap().m).max().unwrap();
            let c = chord(&mut app, last_p1);
            let d = chord(&mut app, last_p1 + 1);
            app.selected = [c, d].into_iter().collect();
            app.exec(commands::Cmd::Tie);
            let pg = |m: usize| geo.systems.iter().find(|s| s.measures.iter().any(|x| x.m == m)).map(|s| s.page).unwrap();
            println!("across pages (bar {} on page {} -> bar {} on page {}): {} ", last_p1 + 1, pg(last_p1) + 1, last_p1 + 2, pg(last_p1 + 1) + 1, app.status);
            if let Some(e) = app.ev_mut(a) {
                e.chord_symbol = Some(ui::chord_text("F", "", "", ""));
            }
            if let Some(e) = app.ev_mut(b) {
                e.chord_symbol = Some(ui::chord_text("D", "m7", "C", ""));
            }
            println!("split {:?} {:?}", app::split_chord("Dm7/C"), app::split_chord("F#maj7(#11)"));
            if let Some(out) = args.get(i + 1) {
                std::fs::write(out, io::score_bytes(&app.score).unwrap()).unwrap();
            }
            return Ok(());
        }
        if args[i] == "--enc-roundtrip" {
            // --enc-roundtrip <score.enc> <model.enc> <out.enc>
            let m = std::fs::read(&args[i + 2]).expect("read model");
            let sc = if args[i + 1] == "new" {
                // a fresh score with quarter-note triplets, a chord symbol and a tie
                let mut app = app::App::new(true);
                app.cur_tuplet = true;
                for k in 0..6u32 {
                    app.enter(0, 0, k * 320, Some(model::Pitch::from_midi(60 + k as i32, false)), 3, 0, false);
                }
                app.cur_tuplet = false;
                let r = app.enter(0, 1, 0, Some(model::Pitch::from_midi(67, false)), 1, 0, false);
                if let Some(e) = app.ev_mut(r) {
                    e.chord_symbol = Some("G7".into());
                }
                // more tuplet cases: high notes (stems down), beamed triplet eighths, low notes, bass staff
                let cases: [(usize, u8, i32); 3] = [(2, 3, 79), (3, 4, 72), (4, 3, 57)];
                for (m, v, base) in cases {
                    while app.score.measures.len() <= m {
                        app.score.push_measure();
                    }
                    app.cur_tuplet = true;
                    app.cur_value = v;
                    let g = app.grid();
                    let n = app.score.measures[m].ticks() / g;
                    for k in 0..n {
                        app.enter(0, m, k * g, Some(model::Pitch::from_midi(base + (k as i32 % 3) * 2, false)), v, 0, false);
                    }
                    app.cur_tuplet = false;
                }
                app.score.text.title = "Teste".into();
                app.score
            } else {
                enc::import(&std::fs::read(&args[i + 1]).expect("read score")).expect("import")
            };
            let out = enc::export(&sc, &m);
            match out {
                Err(e) => println!("export refused: {e}"),
                Ok(b) => {
                    std::fs::write(&args[i + 3], &b).expect("write");
                    let back = enc::import(&b).expect("re-import");
                    let sum = |s: &model::Score| {
                        let mut v = vec![];
                        for (si, st) in s.events.iter().enumerate() {
                            for (mi, me) in st.iter().enumerate() {
                                for e in me {
                                    v.push((si, mi, e.start, e.value, e.dots, e.tuplet, e.pitches.iter().map(|p| p.midi()).collect::<Vec<_>>(), e.chord_symbol.clone()));
                                }
                            }
                        }
                        v
                    };
                    let (x, y) = (sum(&sc), sum(&back));
                    let same = x.iter().zip(y.iter()).filter(|(p, q)| p == q).count();
                    println!("measures {} -> {}, events {} -> {}, identical {}, title {:?} -> {:?}", sc.measures.len(), back.measures.len(), x.len(), y.len(), same, sc.text.title, back.text.title);
                    for (p, q) in x.iter().zip(y.iter()).filter(|(p, q)| p != q).take(5) {
                        println!("  {p:?}\n  {q:?}");
                    }
                }
            }
            return Ok(());
        }
        if args[i] == "--long-selftest" {
            long_selftest();
            return Ok(());
        }
        if args[i] == "--rec-selftest" {
            rec_selftest();
            return Ok(());
        }
        if args[i] == "--midi-selftest" {
            midi_selftest();
            return Ok(());
        }
        // pins light or dark mode instead of following the system (for screenshots)
        if args[i] == "--theme" && i + 1 < args.len() {
            force_theme = match args[i + 1].as_str() {
                "dark" => Some(egui::Theme::Dark),
                "light" => Some(egui::Theme::Light),
                _ => None,
            };
            i += 2;
            continue;
        }
        // which palettes are open (indices, e.g. 0,1,3,4,6), with a pixel check of their cells in the screenshot
        // development: open a dialog or floating window by name, then click / move the mouse (points)
        if args[i] == "--dialog" && i + 1 < args.len() {
            open_dialog = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if (args[i] == "--clicks" || args[i] == "--hover") && i + 1 < args.len() {
            let first = steps.iter().map(|s: &devshots::Step| s.frame + 20).max().unwrap_or(20);
            steps.extend(devshots::parse_steps(&args[i + 1], args[i] == "--clicks", first));
            i += 2;
            continue;
        }
        if args[i] == "--palettes" && i + 1 < args.len() {
            open_palettes = Some(args[i + 1].split(',').filter_map(|v| v.trim().parse().ok()).filter(|&v: &usize| v < 11).collect());
            i += 2;
            continue;
        }
        // the 11 palettes side by side, saved as a picture with the pixel check of every cell
        if args[i] == "--palette-sheet" && i + 1 < args.len() {
            shot = Some(std::path::PathBuf::from(&args[i + 1]));
            sheet = true;
            i += 2;
            continue;
        }
        if args[i] == "--screenshot" && i + 1 < args.len() {
            shot = Some(std::path::PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }
        open = Some(std::path::PathBuf::from(&args[i]));
        i += 1;
    }
    let mut vp = egui::ViewportBuilder::default().with_icon(icon::icon_data()).with_inner_size([1440.0, 900.0]).with_min_inner_size([800.0, 500.0]).with_title("Fermata");
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(target_os = "windows")]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        // macOS stops drawing a window that other windows cover completely; floating it (without focus) for the
        // second or two the picture takes keeps it drawing
        #[cfg(target_os = "macos")]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnTop);
        }
    }
    let mut options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    if shot.is_some() {
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
    }
    let screenshot_mode = shot.is_some();
    eframe::run_native(
        "Fermata",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx, force_theme);
            // the saved View ▸ Appearance, from the first frame (screenshots keep the system look or --theme)
            if force_theme.is_none() && !screenshot_mode {
                cc.egui_ctx.set_theme(prefs::load().appearance.preference());
            }
            let mut app = app::App::new(screenshot_mode);
            app.theme_forced = force_theme.is_some();
            if let Some(p) = open {
                app.open_path(p);
            }
            if let Some(list) = open_palettes {
                app.palettes = [false; 11];
                for i in list {
                    app.palettes[i] = true;
                }
                app.palette_cell_log = Some(vec![]);
            }
            if let Some(d) = open_dialog.as_deref() {
                if !devshots::open(&mut app, d) {
                    eprintln!("unknown dialog {d}; known: {:?}", devshots::DIALOGS);
                }
            }
            Ok(Box::new(Shell { app, frames: 0, shot_path: shot, shot_requested: false, shot_done: Arc::new(AtomicBool::new(false)), sheet, steps }))
        }),
    )
}

/// Plays a short score through the MIDI output into a virtual device and reports what arrived (macOS/Linux).
#[cfg(unix)]
fn midi_selftest() {
    use midir::os::unix::VirtualInput;
    use std::sync::{Arc, Mutex};
    let got: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let g2 = got.clone();
    let vin = midir::MidiInput::new("Fermata selftest").expect("midi");
    let _conn = vin.create_virtual("Fermata Test Piano", move |_, m, _| g2.lock().unwrap().push(m.to_vec()), ()).expect("virtual");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut app = app::App::new(false);
    app.midi_out = Some(midi_out::MidiOut::new());
    app.midi_out.as_mut().unwrap().choose(0, Some("Fermata Test Piano".into()));
    println!("output connected: {:?}", app.midi_out.as_ref().unwrap().connected[0]);
    // three quarter notes at the cursor
    for (k, p) in [60, 64, 67].iter().enumerate() {
        app.enter(0, 0, (k as u32) * model::TPQ, Some(model::Pitch::from_midi(*p, false)), 3, 0, false);
    }
    app.score.measures[0].tempo = Some(240);
    app.play_from(0);
    println!("{}", app.status);
    std::thread::sleep(std::time::Duration::from_millis(900));
    app.stop();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let msgs = got.lock().unwrap().clone();
    let on = msgs.iter().filter(|m| m[0] & 0xf0 == 0x90 && m.get(2).copied().unwrap_or(0) > 0).count();
    let off = msgs.iter().filter(|m| m[0] & 0xf0 == 0x80).count();
    let pc = msgs.iter().filter(|m| m[0] & 0xf0 == 0xc0).count();
    let all_off = msgs.iter().filter(|m| m[0] & 0xf0 == 0xb0 && m.get(1) == Some(&123)).count();
    println!("received {} messages: note-on {on}, note-off {off}, program changes {pc}, all-notes-off {all_off}", msgs.len());
    for m in msgs.iter().filter(|m| m[0] & 0xf0 == 0x90).take(4) {
        println!("  {:02x?}", m);
    }
    // stop in the middle of a long note: everything must be released
    got.lock().unwrap().clear();
    app.score.measures[0].tempo = Some(30);
    app.play_from(0);
    std::thread::sleep(std::time::Duration::from_millis(400));
    app.stop();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let msgs = got.lock().unwrap().clone();
    let on = msgs.iter().filter(|m| m[0] & 0xf0 == 0x90 && m.get(2).copied().unwrap_or(0) > 0).count();
    let off = msgs.iter().filter(|m| m[0] & 0xf0 == 0x80).count();
    let all_off = msgs.iter().filter(|m| m[0] & 0xf0 == 0xb0 && m.get(1) == Some(&123)).count();
    let pedal_up = msgs.iter().filter(|m| m[0] & 0xf0 == 0xb0 && m.get(1) == Some(&64) && m.get(2) == Some(&0)).count();
    println!("stopped early: note-on {on}, note-off {off}, all-notes-off {all_off}, pedal-up {pedal_up}");
    // the piano is switched off and on again: same name, new device
    drop(_conn);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let got2: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let g3 = got2.clone();
    let vin2 = midir::MidiInput::new("Fermata selftest 2").expect("midi");
    let _conn2 = vin2.create_virtual("Fermata Test Piano", move |_, m, _| g3.lock().unwrap().push(m.to_vec()), ()).expect("virtual");
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.midi_out.as_mut().unwrap().scan();
    app.score.measures[0].tempo = Some(240);
    app.play_from(0);
    std::thread::sleep(std::time::Duration::from_millis(900));
    app.stop();
    let ok = app.midi_out.as_ref().unwrap().test(0);
    std::thread::sleep(std::time::Duration::from_millis(1600));
    let msgs = got2.lock().unwrap().clone();
    let on = msgs.iter().filter(|m| m[0] & 0xf0 == 0x90 && m.get(2).copied().unwrap_or(0) > 0).count();
    println!("after reconnect: received {} messages, note-on {on}; test button sent: {ok}", msgs.len());
    print!("{}", app.midi_out.as_ref().unwrap().diagnostics());
}

#[cfg(not(unix))]
fn midi_selftest() {
    println!("self-test runs on macOS/Linux only");
}


/// Plays notes from a virtual MIDI keyboard into the app while it records, then lists what was written.
#[cfg(unix)]
fn rec_selftest() {
    use midir::os::unix::VirtualOutput;
    let vout = midir::MidiOutput::new("Fermata keys test").expect("midi");
    let mut keys = vout.create_virtual("Fermata Test Keys").expect("virtual out");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let ctx = egui::Context::default();
    let mut app = app::App::new(false);
    app.midi = Some(midi_in::MidiIn::new());
    let outs = [std::sync::Arc::new(std::sync::Mutex::new(None)), std::sync::Arc::new(std::sync::Mutex::new(None))];
    app.midi.as_mut().unwrap().scan(&ctx, None, outs);
    println!("inputs open: {:?}", app.midi.as_ref().unwrap().names);
    app.rec_settings.count_in = 0;
    app.rec_settings.click = false;
    app.score.measures[0].tempo = Some(120);
    app.exec(commands::Cmd::Record);
    println!("recording: {}", app.rec.is_some());
    let pump = |app: &mut app::App, ms: u64| {
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            let msgs: Vec<midi_in::MidiMsg> = app.midi.as_ref().unwrap().rx.try_iter().collect();
            for m in msgs {
                app.handle_midi(m);
            }
            app.rec_tick();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    // quarter notes C D E, then a C-E-G chord, at 120 per minute
    for k in [60u8, 62, 64] {
        let _ = keys.send(&[0x90, k, 90]);
        pump(&mut app, 450);
        let _ = keys.send(&[0x80, k, 0]);
        pump(&mut app, 50);
    }
    for k in [60u8, 64, 67] {
        let _ = keys.send(&[0x90, k, 90]);
    }
    pump(&mut app, 900);
    for k in [60u8, 64, 67] {
        let _ = keys.send(&[0x80, k, 0]);
    }
    pump(&mut app, 100);
    app.exec(commands::Cmd::Record);
    for m in 0..2 {
        for e in &app.score.events[0][m] {
            println!("measure {} tick {} value {} dots {} pitches {:?}", m + 1, e.start, e.value, e.dots, e.pitches.iter().map(|p| p.name()).collect::<Vec<_>>());
        }
    }
    println!("{}", app.status);
    // record again from the start over the same bar: the old notes must be replaced, not added to
    app.cursor = (0, 0, 0);
    app.exec(commands::Cmd::Record);
    let _ = keys.send(&[0x90, 67, 90]);
    pump(&mut app, 900);
    let _ = keys.send(&[0x80, 67, 0]);
    pump(&mut app, 150);
    app.exec(commands::Cmd::Record);
    let second: Vec<String> = app.score.events[0][0].iter().map(|e| format!("tick {} {:?}", e.start, e.pitches.iter().map(|p| p.name()).collect::<Vec<_>>())).collect();
    println!("second take over measure 1: {second:?}");
    // the keyboard is switched off and on again: the app must find it and record from it
    drop(keys);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let vout2 = midir::MidiOutput::new("Fermata keys test 2").expect("midi");
    let mut keys2 = vout2.create_virtual("Fermata Test Keys").expect("virtual out");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let outs = [std::sync::Arc::new(std::sync::Mutex::new(None)), std::sync::Arc::new(std::sync::Mutex::new(None))];
    app.midi.as_mut().unwrap().scan(&ctx, None, outs);
    app.cursor = (0, 2, 0);
    app.exec(commands::Cmd::Record);
    for k in [67u8, 65] {
        let _ = keys2.send(&[0x90, k, 90]);
        pump(&mut app, 450);
        let _ = keys2.send(&[0x80, k, 0]);
        pump(&mut app, 50);
    }
    app.exec(commands::Cmd::Record);
    let after: Vec<String> = app.score.events[0][2].iter().map(|e| format!("tick {} {:?}", e.start, e.pitches.iter().map(|p| p.name()).collect::<Vec<_>>())).collect();
    println!("after the keyboard restarted, measure 3: {after:?}");
    println!("notes heard in total: {}", app.midi.as_ref().unwrap().received.load(std::sync::atomic::Ordering::Relaxed));
}

#[cfg(not(unix))]
fn rec_selftest() {}

/// Records for two seconds with the click on, into a virtual piano, and lists the click messages it received.
#[cfg(unix)]
fn click_selftest() {
    use midir::os::unix::VirtualInput;
    use std::sync::{Arc, Mutex};
    let got: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(vec![]));
    let g2 = got.clone();
    let vin = midir::MidiInput::new("Fermata click test").expect("midi");
    let _c = vin.create_virtual("Fermata Test Piano", move |_, m, _| g2.lock().unwrap().push(m.to_vec()), ()).expect("virtual");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut app = app::App::new(false);
    app.midi_out = Some(midi_out::MidiOut::new());
    app.midi_out.as_mut().unwrap().choose(0, Some("Fermata Test Piano".into()));
    app.rec_settings.click = true;
    app.rec_settings.count_in = 1;
    app.score.measures[0].tempo = Some(120);
    app.exec(commands::Cmd::Record);
    std::thread::sleep(std::time::Duration::from_millis(2600));
    app.exec(commands::Cmd::Record);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let msgs = got.lock().unwrap().clone();
    let clicks: Vec<String> = msgs.iter().filter(|m| m[0] & 0xf0 == 0x90 && m[2] > 0).map(|m| format!("{:02x?}", m)).collect();
    let setup: Vec<String> = msgs.iter().filter(|m| m[0] & 0xf0 == 0xc0 || (m[0] & 0xf0 == 0xb0 && m[1] != 64 && m[1] != 123)).map(|m| format!("{:02x?}", m)).collect();
    println!("recording: messages {}, click notes {}: {:?}", msgs.len(), clicks.len(), clicks.iter().take(5).collect::<Vec<_>>());
    println!("program/volume changes sent (should be none): {:?}", setup);
    // playback with Click On
    got.lock().unwrap().clear();
    app.score.playback.click = true;
    app.play_from(0);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    app.stop();
    let msgs = got.lock().unwrap().clone();
    println!("playback with Click On: notes {:?}", msgs.iter().filter(|m| m[0] & 0xf0 == 0x90 && m[2] > 0).map(|m| format!("{:02x?}", m)).collect::<Vec<_>>());
}

#[cfg(not(unix))]
fn click_selftest() {}

/// Long notes: a chord held ~3.75 beats must be a whole note; one held 3 beats a dotted half plus a quarter rest.
#[cfg(unix)]
fn long_selftest() {
    use midir::os::unix::VirtualOutput;
    let vout = midir::MidiOutput::new("Fermata long test").expect("midi");
    let mut keys = vout.create_virtual("Fermata Test Keys").expect("virtual out");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let ctx = egui::Context::default();
    let mut app = app::App::new(false);
    app.midi = Some(midi_in::MidiIn::new());
    let outs = [std::sync::Arc::new(std::sync::Mutex::new(None)), std::sync::Arc::new(std::sync::Mutex::new(None))];
    app.midi.as_mut().unwrap().scan(&ctx, None, outs);
    app.rec_settings.count_in = 0;
    app.rec_settings.click = false;
    app.score.measures[0].tempo = Some(120);
    let pump = |app: &mut app::App, ms: u64| {
        let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            let msgs: Vec<midi_in::MidiMsg> = app.midi.as_ref().unwrap().rx.try_iter().collect();
            for m in msgs {
                app.handle_midi(m);
            }
            app.rec_tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    app.exec(commands::Cmd::Record);
    // measure 1: held 3.75 beats (1.875 s at 120), then silence to the bar line
    for k in [60u8, 64, 67] { let _ = keys.send(&[0x90, k, 90]); }
    pump(&mut app, 1875);
    for k in [60u8, 64, 67] { let _ = keys.send(&[0x80, k, 0]); }
    pump(&mut app, 125);
    // measure 2: held exactly 3 beats, then a beat of silence
    for k in [60u8, 64, 67] { let _ = keys.send(&[0x90, k, 90]); }
    pump(&mut app, 1500);
    for k in [60u8, 64, 67] { let _ = keys.send(&[0x80, k, 0]); }
    pump(&mut app, 450);
    app.exec(commands::Cmd::Record);
    let names = ["double whole", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th", "128th"];
    for m in 0..2 {
        let row: Vec<String> = app.score.events[0][m]
            .iter()
            .map(|e| format!("{}{}{} at beat {}", if e.dots > 0 { "dotted " } else { "" }, names[e.value as usize], if e.is_rest() { " rest" } else { "" }, e.start / model::TPQ + 1))
            .collect();
        println!("measure {}: {:?}", m + 1, row);
    }
}

#[cfg(not(unix))]
fn long_selftest() {}
