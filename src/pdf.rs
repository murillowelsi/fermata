//! F-06: the score's pages as a vector PDF.
//!
//! The page is drawn by the same code as the screen (`score_view::draw`, without cursor, selection or hidden items)
//! into an offscreen egui context; its shapes become PDF paths and text. Text stays text: each glyph is written with
//! the font egui chose for it (Bravura for music, the page font for words, with egui's own fallbacks), embedded as a
//! subset of the glyphs used, with a ToUnicode map so it can be selected and searched. One PDF page per score page, at
//! the paper size and the print scale of Page Setup.
use crate::model::Score;
use crate::score_view::{self, DrawState};
use egui::epaint::{ColorMode, Mesh, PathShape, RectShape, Shape, TextShape};
use egui::{Color32, FontFamily, Pos2, Rect, Stroke};
use pdf_writer::types::{CidFontType, FontFlags, LineCapStyle, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Date, Filter, Finish, Name, Pdf, Rect as PdfRect, Ref, Str, TextStr};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Points to millimetres, to a tenth (Page Setup shows margins in mm).
pub fn pt_to_mm(pt: f32) -> f32 {
    (pt * 25.4 / 72.0 * 10.0).round() / 10.0
}

pub fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

/// Page Setup's values into a layout (F-06). A margin whose millimetres didn't change keeps its exact points, so OK
/// without changes moves nothing; four equal margins stay one value, as older files have it.
pub fn apply_page_setup(l: &mut crate::model::Layout, paper: (f32, f32), margins_mm: [f32; 4], scale: u16, spacing: (f32, f32, f32)) {
    l.page_w = paper.0;
    l.page_h = paper.1;
    let old = l.margins();
    let pt: [f32; 4] = std::array::from_fn(|k| if (pt_to_mm(old[k]) - margins_mm[k]).abs() < 0.05 { old[k] } else { mm_to_pt(margins_mm[k]) });
    if pt.iter().all(|m| (m - pt[0]).abs() < 1e-3) {
        l.margin = pt[0];
        l.margins = None;
    } else {
        l.margins = Some(pt);
    }
    l.print_scale = scale.clamp(50, 200);
    (l.staff_space, l.staff_gap, l.system_gap) = spacing;
}

/// Page Setup's "Fit width": the largest print scale at which every system keeps its natural spacing between the
/// margins.
pub fn fit_width_scale(score: &Score) -> u16 {
    let lin = score_view::layout(score, Pos2::ZERO, 1.0, true);
    let Some(line) = lin.systems.first() else { return 100 };
    let width: HashMap<usize, f32> = line.measures.iter().map(|m| (m.m, m.x1 - m.x0)).collect();
    let header = line.measures.first().map(|m| m.content_x - m.x0).unwrap_or(0.0);
    let geo = score_view::layout(score, Pos2::ZERO, 1.0, false);
    let [_, _, ml, mr] = score.layout.margins();
    let mut need = 1.0f32;
    for s in &geo.systems {
        let mut w: f32 = (s.m0..s.m1).filter_map(|m| width.get(&m)).sum();
        if s.m0 > 0 {
            // the clef and key that start every system (the line's first measure already has them)
            w += header;
        }
        let indent = s.left - geo.pages[s.page].left() - ml;
        need = need.max(w + indent);
    }
    let f = (score.layout.page_w - ml - mr) / need;
    ((f * 100.0).floor() as u16).clamp(50, 200)
}

/// The score as it is printed: the page laid out for the paper divided by the print scale, so a scale of 75% fits
/// more on a page and 125% less; margins stay their real size on the paper.
pub fn print_score(score: &Score) -> Score {
    let mut sc = score.clone();
    let f = sc.layout.print_factor();
    let m = sc.layout.margins();
    sc.layout.page_w /= f;
    sc.layout.page_h /= f;
    sc.layout.margin /= f;
    sc.layout.margins = Some(m.map(|x| x / f));
    sc
}

/// The shapes of every printed page, in the layout's coordinates, and the page rectangles.
pub fn page_shapes(score: &Score) -> (Vec<Rect>, Vec<Vec<Shape>>) {
    let sc = print_score(score);
    let geo = score_view::layout(&sc, Pos2::ZERO, 1.0, false);
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts());
    // text is laid out on a fine grid (a quarter of a point), as a printer needs
    ctx.set_pixels_per_point(4.0);
    let (sel, selg) = (HashSet::new(), HashSet::new());
    let st = DrawState {
        selected: &sel,
        sel_graphics: &selg,
        cursor: None,
        play_tick: None,
        show_voice: None,
        show_control_points: false,
        show_hidden: false,
        sel_measures: None,
        hover_ghost: None,
        flash: None,
    };
    let mut all = vec![];
    for pass in 0..2 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            if pass == 1 {
                let p = egui::Painter::new(ui.ctx().clone(), egui::LayerId::background(), Rect::EVERYTHING);
                score_view::draw(&p, &sc, &geo, &st, &mut vec![], &mut vec![]);
            }
        });
        out.textures_delta.clear();
        if pass == 1 {
            all = out.shapes.into_iter().map(|c| c.shape).collect::<Vec<_>>();
        }
    }
    let mut flat = vec![];
    for s in all {
        flatten(s, &mut flat);
    }
    let pages: Vec<Vec<Shape>> = geo
        .pages
        .iter()
        .map(|pr| {
            flat.iter()
                .filter(|s| !is_sheet(s, pr) && pr.intersects(s.visual_bounding_rect()))
                .cloned()
                .collect()
        })
        .collect();
    (geo.pages.clone(), pages)
}

fn flatten(s: Shape, out: &mut Vec<Shape>) {
    match s {
        Shape::Vec(v) => v.into_iter().for_each(|x| flatten(x, out)),
        Shape::Noop | Shape::Callback(_) => {}
        s => out.push(s),
    }
}

/// The sheet itself and its shadow: the screen's, not the paper's.
fn is_sheet(s: &Shape, page: &Rect) -> bool {
    match s {
        Shape::Rect(r) => r.blur_width > 0.0 || (r.fill == crate::theme::PAGE && r.rect.area() >= page.area() * 0.9),
        _ => false,
    }
}

/// The score's pages as a PDF document.
pub fn score_pdf(score: &Score) -> Result<Vec<u8>, String> {
    let (rects, pages) = page_shapes(score);
    let f = score.layout.print_factor();
    let (paper_w, paper_h) = (score.layout.page_w, score.layout.page_h);
    let mut fonts = Fonts::new();
    let mut contents = vec![];
    for (pr, shapes) in rects.iter().zip(&pages) {
        let mut c = Content::new();
        // the layout's points (y down, from the page's corner) to the paper's (y up), at the print scale
        c.transform([f, 0.0, 0.0, -f, -pr.left() * f, paper_h + pr.top() * f]);
        c.set_line_cap(LineCapStyle::ButtCap);
        for s in shapes {
            emit(&mut c, s, &mut fonts);
        }
        contents.push(c.finish());
    }

    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let catalog = alloc();
    let tree = alloc();
    let info = alloc();
    let page_refs: Vec<(Ref, Ref)> = pages.iter().map(|_| (alloc(), alloc())).collect();
    let font_refs: Vec<Ref> = fonts.used.iter().map(|_| alloc()).collect();

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids(page_refs.iter().map(|p| p.0)).count(page_refs.len() as i32);
    let names: Vec<String> = (0..fonts.used.len()).map(|i| format!("F{i}")).collect();
    for ((page, content), data) in page_refs.iter().zip(&contents) {
        let mut pg = pdf.page(*page);
        pg.parent(tree).media_box(PdfRect::new(0.0, 0.0, paper_w, paper_h)).contents(*content);
        let mut res = pg.resources();
        let mut fd = res.fonts();
        for (i, r) in font_refs.iter().enumerate() {
            fd.pair(Name(names[i].as_bytes()), *r);
        }
        fd.finish();
        res.finish();
        pg.finish();
        pdf.stream(*content, &deflate(data)).filter(Filter::FlateDecode);
    }
    for (i, r) in font_refs.iter().enumerate() {
        fonts.write(&mut pdf, i, *r, &mut alloc)?;
    }

    let t = &score.text;
    let version = format!("Fermata {}", env!("CARGO_PKG_VERSION"));
    let mut di = pdf.document_info(info);
    if !t.title.trim().is_empty() {
        di.title(TextStr(t.title.trim()));
    }
    let composer: Vec<&str> = t.composer.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if !composer.is_empty() {
        let a = composer.join(", ");
        di.author(TextStr(&a));
    }
    if let Some(sub) = t.subtitle.lines().map(str::trim).find(|l| !l.is_empty()) {
        di.subject(TextStr(sub));
    }
    di.creator(TextStr(&version));
    di.producer(TextStr(&version));
    di.creation_date(pdf_date(crate::clock::unix_secs()));
    di.finish();
    Ok(pdf.finish())
}

fn pdf_date(secs: u64) -> Date {
    // days since 1970 to a civil date (Howard Hinnant's algorithm)
    let days = (secs / 86400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let s = secs % 86400;
    Date::new(y.clamp(0, 9999) as u16)
        .month(m as u8)
        .day(d as u8)
        .hour((s / 3600) as u8)
        .minute((s / 60 % 60) as u8)
        .second((s % 60) as u8)
        .utc_offset_hour(0)
        .utc_offset_minute(0)
}

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

/// A colour on white paper (egui's colours are premultiplied by their alpha).
fn rgb(c: Color32) -> Option<(f32, f32, f32)> {
    if c.a() == 0 {
        return None;
    }
    let k = 1.0 - c.a() as f32 / 255.0;
    Some(((c.r() as f32 / 255.0 + k).min(1.0), (c.g() as f32 / 255.0 + k).min(1.0), (c.b() as f32 / 255.0 + k).min(1.0)))
}

fn stroke_color(c: &mut Content, s: Stroke) -> bool {
    if s.width <= 0.0 {
        return false;
    }
    match rgb(s.color) {
        Some((r, g, b)) => {
            c.set_stroke_rgb(r, g, b);
            c.set_line_width(s.width);
            true
        }
        None => false,
    }
}

fn fill_color(c: &mut Content, col: Color32) -> bool {
    match rgb(col) {
        Some((r, g, b)) => {
            c.set_fill_rgb(r, g, b);
            true
        }
        None => false,
    }
}

fn path_stroke(p: &PathShape) -> Stroke {
    let col = match &p.stroke.color {
        ColorMode::Solid(c) => *c,
        _ => Color32::BLACK,
    };
    Stroke::new(p.stroke.width, col)
}

/// Fills and strokes the current path as asked.
fn paint(c: &mut Content, fill: Color32, stroke: Stroke) {
    let f = fill_color(c, fill);
    let s = stroke_color(c, stroke);
    match (f, s) {
        (true, true) => {
            c.fill_nonzero_and_stroke();
        }
        (true, false) => {
            c.fill_nonzero();
        }
        (false, true) => {
            c.stroke();
        }
        (false, false) => {
            c.end_path();
        }
    }
}

fn ellipse(c: &mut Content, ctr: Pos2, rx: f32, ry: f32) {
    let k = 0.552_284_8;
    let (x, y) = (ctr.x, ctr.y);
    c.move_to(x + rx, y);
    c.cubic_to(x + rx, y + ry * k, x + rx * k, y + ry, x, y + ry);
    c.cubic_to(x - rx * k, y + ry, x - rx, y + ry * k, x - rx, y);
    c.cubic_to(x - rx, y - ry * k, x - rx * k, y - ry, x, y - ry);
    c.cubic_to(x + rx * k, y - ry, x + rx, y - ry * k, x + rx, y);
    c.close_path();
}

fn emit(c: &mut Content, s: &Shape, fonts: &mut Fonts) {
    match s {
        Shape::LineSegment { points, stroke } => {
            if stroke_color(c, *stroke) {
                c.move_to(points[0].x, points[0].y);
                c.line_to(points[1].x, points[1].y);
                c.stroke();
            }
        }
        Shape::Path(p) => {
            let Some(first) = p.points.first() else { return };
            c.move_to(first.x, first.y);
            for q in &p.points[1..] {
                c.line_to(q.x, q.y);
            }
            if p.closed {
                c.close_path();
                paint(c, p.fill, path_stroke(p));
            } else if stroke_color(c, path_stroke(p)) {
                c.stroke();
            } else {
                c.end_path();
            }
        }
        Shape::Circle(ci) => {
            ellipse(c, ci.center, ci.radius, ci.radius);
            paint(c, ci.fill, ci.stroke);
        }
        Shape::Ellipse(e) => {
            ellipse(c, e.center, e.radius.x, e.radius.y);
            paint(c, e.fill, e.stroke);
        }
        Shape::Rect(r) => emit_rect(c, r),
        Shape::CubicBezier(b) => {
            let p = b.points;
            c.move_to(p[0].x, p[0].y);
            c.cubic_to(p[1].x, p[1].y, p[2].x, p[2].y, p[3].x, p[3].y);
            let st = match &b.stroke.color {
                ColorMode::Solid(col) => Stroke::new(b.stroke.width, *col),
                _ => Stroke::new(b.stroke.width, Color32::BLACK),
            };
            if b.closed {
                c.close_path();
            }
            paint(c, if b.closed { b.fill } else { Color32::TRANSPARENT }, st);
        }
        Shape::QuadraticBezier(b) => {
            let p = b.points;
            let c1 = p[0] + (p[1] - p[0]) * (2.0 / 3.0);
            let c2 = p[2] + (p[1] - p[2]) * (2.0 / 3.0);
            c.move_to(p[0].x, p[0].y);
            c.cubic_to(c1.x, c1.y, c2.x, c2.y, p[2].x, p[2].y);
            let st = match &b.stroke.color {
                ColorMode::Solid(col) => Stroke::new(b.stroke.width, *col),
                _ => Stroke::new(b.stroke.width, Color32::BLACK),
            };
            if b.closed {
                c.close_path();
            }
            paint(c, if b.closed { b.fill } else { Color32::TRANSPARENT }, st);
        }
        Shape::Mesh(m) => emit_mesh(c, m),
        Shape::Text(t) => emit_text(c, t, fonts),
        _ => {}
    }
}

fn emit_rect(c: &mut Content, r: &RectShape) {
    let mut rect = r.rect;
    // where the stroke runs: inside, on, or outside the rectangle's edge
    let w = r.stroke.width;
    match r.stroke_kind {
        egui::StrokeKind::Inside => rect = rect.shrink(w / 2.0),
        egui::StrokeKind::Outside => rect = rect.expand(w / 2.0),
        egui::StrokeKind::Middle => {}
    }
    if fill_color(c, r.fill) {
        c.rect(r.rect.left(), r.rect.top(), r.rect.width(), r.rect.height());
        c.fill_nonzero();
    }
    if stroke_color(c, r.stroke) {
        c.rect(rect.left(), rect.top(), rect.width(), rect.height());
        c.stroke();
    }
}

/// A filled mesh (a slur or tie drawn as a band of triangles) as its outline: the edges only one triangle has.
fn emit_mesh(c: &mut Content, m: &Mesh) {
    let Some(v0) = m.vertices.first() else { return };
    let mut count: HashMap<(u32, u32), i32> = HashMap::new();
    let mut directed = vec![];
    for t in m.indices.chunks(3) {
        if t.len() < 3 {
            continue;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
            directed.push((a, b));
        }
    }
    let mut nexts: HashMap<u32, Vec<u32>> = HashMap::new();
    for (a, b) in directed {
        if count[&(a.min(b), a.max(b))] == 1 {
            nexts.entry(a).or_default().push(b);
        }
    }
    if !fill_color(c, v0.color) {
        return;
    }
    let mut any = false;
    while let Some(&start) = nexts.keys().next() {
        let mut cur = start;
        let p = m.vertices[cur as usize].pos;
        c.move_to(p.x, p.y);
        while let Some(list) = nexts.get_mut(&cur) {
            let nx = list.pop().unwrap_or(start);
            if list.is_empty() {
                nexts.remove(&cur);
            }
            if nx == start {
                break;
            }
            let q = m.vertices[nx as usize].pos;
            c.line_to(q.x, q.y);
            cur = nx;
        }
        c.close_path();
        any = true;
    }
    if any {
        c.fill_nonzero();
    } else {
        c.end_path();
    }
}

fn emit_text(c: &mut Content, t: &TextShape, fonts: &mut Fonts) {
    let galley = &t.galley;
    let Some(section) = galley.job.sections.first() else { return };
    let fmt = &section.format;
    let size = fmt.font_id.size;
    let family = fmt.font_id.family.clone();
    let mut col = t.override_text_color.unwrap_or(fmt.color);
    if col == Color32::PLACEHOLDER {
        col = t.fallback_color;
    }
    if !fill_color(c, col) {
        return;
    }
    let (sin, cos) = t.angle.sin_cos();
    let at = |q: Pos2| -> Pos2 {
        let d = q.to_vec2();
        Pos2::new(t.pos.x + d.x * cos - d.y * sin, t.pos.y + d.x * sin + d.y * cos)
    };
    for row in &galley.rows {
        // runs of glyphs from one font, written with the font's own advances and the gaps egui left between them
        let mut run: Option<(usize, f32, f32, f32)> = None; // font, size, x of the pen, y
        let mut items: Vec<(u16, f32)> = vec![]; // new glyph id, adjustment before it
        let mut prev_end: Option<(f32, usize)> = None; // x where the last glyph ended with its own advance
        let flush = |c: &mut Content, run: &Option<(usize, f32, f32, f32)>, items: &mut Vec<(u16, f32)>, fonts: &Fonts| {
            if let Some((fi, sz, x, y)) = *run {
                if !items.is_empty() {
                    let p = at(Pos2::new(x, y));
                    c.begin_text();
                    c.set_font(Name(fonts.res_name(fi).as_bytes()), sz);
                    c.set_text_matrix([cos, sin, sin, -cos, p.x, p.y]);
                    let mut sp = c.show_positioned();
                    let mut it = sp.items();
                    for (g, adj) in items.iter() {
                        if adj.abs() > 0.01 {
                            it.adjust(*adj);
                        }
                        it.show(Str(&g.to_be_bytes()));
                    }
                    it.finish();
                    sp.finish();
                    c.end_text();
                }
            }
            items.clear();
        };
        let mut pen_x: Option<f32> = None;
        for g in &row.row.glyphs {
            let ch = g.chr;
            let base = row.pos + g.pos.to_vec2();
            // a zero-width continuation glyph (the 2nd letter of a ligature): it goes after the previous letter
            let x = if g.advance_width == 0.0 && !ch.is_whitespace() { pen_x.unwrap_or(base.x) } else { base.x };
            let Some((fi, gid, k, dy, dy_pt)) = fonts.resolve(&family, ch) else {
                pen_x = Some(x);
                continue;
            };
            let sz = size * k;
            let y = base.y + dy * size + dy_pt;
            let adv = fonts.advance(fi, gid) * sz;
            pen_x = Some(x + adv);
            let same = matches!(run, Some((f0, s0, _, y0)) if f0 == fi && (s0 - sz).abs() < 1e-3 && (y0 - y).abs() < 1e-3);
            if !same {
                flush(c, &run, &mut items, fonts);
                run = Some((fi, sz, x, y));
                prev_end = None;
            }
            let new = fonts.use_glyph(fi, gid, ch);
            let adj = match prev_end {
                Some((end, _)) => -(x - end) * 1000.0 / sz,
                None => 0.0,
            };
            items.push((new, adj));
            prev_end = Some((x + adv, fi));
        }
        flush(c, &run, &mut items, fonts);
    }
}

// ---------------- fonts ----------------

/// The tables of an OpenType font that a PDF needs.
struct FontFile {
    key: String,
    data: Arc<egui::FontData>,
    upem: f32,
    ascent: f32,
    descent: f32,
    cap: f32,
    bbox: [f32; 4],
    cmap: HashMap<u32, u16>,
    advances: Vec<u16>,
    cff: bool,
    /// egui's tweak of this font: size factor, and how far down its glyphs go (fraction of the size, points)
    scale: f32,
    y_factor: f32,
    y_points: f32,
}

#[derive(Default)]
struct Used {
    remap: subsetter::GlyphRemapper,
    /// new glyph id -> (old glyph id, character)
    glyphs: BTreeMap<u16, (u16, char)>,
}

struct Fonts {
    defs: egui::FontDefinitions,
    files: Vec<FontFile>,
    by_key: HashMap<String, Option<usize>>,
    resolved: HashMap<(FontFamily, char), Option<(usize, u16)>>,
    /// fonts used in the document, by file index, in the order they were first used
    used: Vec<(usize, Used)>,
    names: Vec<String>,
}

fn be16(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}
fn be32(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The offset and length of each table of the font at `index` (a font collection holds several).
fn tables(d: &[u8], index: u32) -> Option<HashMap<[u8; 4], (usize, usize)>> {
    let off = if d.get(0..4)? == b"ttcf" { be32(d, 12 + 4 * index as usize)? as usize } else { 0 };
    let n = be16(d, off + 4)? as usize;
    let mut t = HashMap::new();
    for i in 0..n {
        let r = off + 12 + 16 * i;
        let tag: [u8; 4] = d.get(r..r + 4)?.try_into().ok()?;
        t.insert(tag, (be32(d, r + 8)? as usize, be32(d, r + 12)? as usize));
    }
    Some(t)
}

fn parse_cmap(d: &[u8], at: usize) -> HashMap<u32, u16> {
    let mut map = HashMap::new();
    let n = be16(d, at + 2).unwrap_or(0) as usize;
    let mut best: Option<(u32, usize)> = None; // rank, subtable offset
    for i in 0..n {
        let r = at + 4 + 8 * i;
        let (Some(pid), Some(eid), Some(off)) = (be16(d, r), be16(d, r + 2), be32(d, r + 4)) else { continue };
        let st = at + off as usize;
        let fmt = be16(d, st).unwrap_or(0);
        let rank = match (pid, eid, fmt) {
            (3, 10, 12) | (0, 4, 12) | (0, 6, 12) => 3,
            (3, 1, 4) | (0, 3, 4) | (0, _, 4) => 2,
            (3, 0, 4) => 1,
            _ => 0,
        };
        if rank > 0 && best.map(|b| rank > b.0).unwrap_or(true) {
            best = Some((rank, st));
        }
    }
    let Some((_, st)) = best else { return map };
    match be16(d, st) {
        Some(12) => {
            let groups = be32(d, st + 12).unwrap_or(0) as usize;
            for g in 0..groups {
                let r = st + 16 + 12 * g;
                let (Some(a), Some(b), Some(gid)) = (be32(d, r), be32(d, r + 4), be32(d, r + 8)) else { break };
                for (k, cp) in (a..=b.min(a + 0xFFFF)).enumerate() {
                    map.insert(cp, (gid + k as u32) as u16);
                }
            }
        }
        Some(4) => {
            let seg = be16(d, st + 6).unwrap_or(0) as usize / 2;
            let ends = st + 14;
            let starts = ends + 2 * seg + 2;
            let deltas = starts + 2 * seg;
            let ranges = deltas + 2 * seg;
            for s in 0..seg {
                let (Some(e), Some(b), Some(delta), Some(ro)) = (be16(d, ends + 2 * s), be16(d, starts + 2 * s), be16(d, deltas + 2 * s), be16(d, ranges + 2 * s)) else { break };
                if b == 0xFFFF {
                    continue;
                }
                for cp in b..=e {
                    let gid = if ro == 0 {
                        cp.wrapping_add(delta)
                    } else {
                        let o = ranges + 2 * s + ro as usize + 2 * (cp - b) as usize;
                        match be16(d, o) {
                            Some(0) | None => 0,
                            Some(g) => g.wrapping_add(delta),
                        }
                    };
                    if gid != 0 {
                        map.insert(cp as u32, gid);
                    }
                    if cp == 0xFFFF {
                        break;
                    }
                }
            }
        }
        _ => {}
    }
    map
}

impl FontFile {
    fn parse(key: &str, data: Arc<egui::FontData>) -> Option<FontFile> {
        let d: &[u8] = (*data.font).as_ref();
        let t = tables(d, data.index)?;
        let head = t.get(b"head")?.0;
        let hhea = t.get(b"hhea")?.0;
        let upem = be16(d, head + 18)? as f32;
        let i16at = |o: usize| be16(d, o).map(|v| v as i16 as f32);
        let bbox = [i16at(head + 36)?, i16at(head + 38)?, i16at(head + 40)?, i16at(head + 42)?];
        let ascent = i16at(hhea + 4)?;
        let descent = i16at(hhea + 6)?;
        let n_hm = be16(d, hhea + 34)? as usize;
        let n_glyphs = be16(d, t.get(b"maxp")?.0 + 4)? as usize;
        let hmtx = t.get(b"hmtx")?.0;
        let mut advances = Vec::with_capacity(n_glyphs);
        let mut last = 0;
        for i in 0..n_glyphs {
            if i < n_hm {
                last = be16(d, hmtx + 4 * i).unwrap_or(last);
            }
            advances.push(last);
        }
        let cap = t
            .get(b"OS/2")
            .and_then(|&(o, _)| if be16(d, o)? >= 2 { i16at(o + 88) } else { None })
            .unwrap_or(ascent * 0.7);
        let cmap = parse_cmap(d, t.get(b"cmap")?.0);
        let tw = data.tweak.clone();
        Some(FontFile {
            key: key.to_owned(),
            upem,
            ascent,
            descent,
            cap,
            bbox,
            cmap,
            advances,
            cff: t.contains_key(b"CFF "),
            scale: tw.scale,
            y_factor: tw.y_offset_factor,
            y_points: tw.y_offset,
            data,
        })
    }
}

const IDENTITY: SystemInfo = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };

impl Fonts {
    fn new() -> Fonts {
        Fonts { defs: crate::theme::fonts(), files: vec![], by_key: HashMap::new(), resolved: HashMap::new(), used: vec![], names: vec![] }
    }

    fn file(&mut self, key: &str) -> Option<usize> {
        if let Some(i) = self.by_key.get(key) {
            return *i;
        }
        let i = self.defs.font_data.get(key).cloned().and_then(|d| FontFile::parse(key, d)).map(|f| {
            self.files.push(f);
            self.files.len() - 1
        });
        self.by_key.insert(key.to_owned(), i);
        i
    }

    /// The font egui draws `ch` with in `family` (the first in the family's list that has it), the glyph, the size
    /// factor, and the downward shift of that font (a fraction of the size, and points).
    fn resolve(&mut self, family: &FontFamily, ch: char) -> Option<(usize, u16, f32, f32, f32)> {
        let key = (family.clone(), ch);
        let r = if let Some(r) = self.resolved.get(&key) {
            *r
        } else {
            let list = self.defs.families.get(family).cloned().unwrap_or_default();
            let mut found = None;
            for name in list {
                if let Some(fi) = self.file(&name) {
                    if let Some(&g) = self.files[fi].cmap.get(&(ch as u32)) {
                        found = Some((fi, g));
                        break;
                    }
                }
            }
            self.resolved.insert(key, found);
            found
        };
        let (fi, g) = r?;
        let f = &self.files[fi];
        // egui moves the glyphs of a tweaked font down by size × scale × y_offset_factor + y_offset points
        Some((fi, g, f.scale, f.scale * f.y_factor, f.y_points))
    }

    /// Advance of a glyph, as a fraction of the font size.
    fn advance(&self, fi: usize, gid: u16) -> f32 {
        let f = &self.files[fi];
        f.advances.get(gid as usize).copied().unwrap_or(0) as f32 / f.upem
    }

    /// Marks a glyph as used and returns its id in the subset.
    fn use_glyph(&mut self, fi: usize, gid: u16, ch: char) -> u16 {
        let k = match self.used.iter().position(|u| u.0 == fi) {
            Some(k) => k,
            None => {
                let mut u = Used::default();
                u.remap.remap(0); // .notdef stays glyph 0
                self.used.push((fi, u));
                self.names.push(format!("F{}", self.used.len() - 1));
                self.used.len() - 1
            }
        };
        let u = &mut self.used[k].1;
        let new = u.remap.remap(gid);
        u.glyphs.entry(new).or_insert((gid, ch));
        new
    }

    fn res_name(&self, fi: usize) -> &str {
        let k = self.used.iter().position(|u| u.0 == fi).unwrap_or(0);
        &self.names[k]
    }

    /// Writes used font `k` (Type 0 font, CID font, descriptor, the subset and the ToUnicode map).
    fn write(&self, pdf: &mut Pdf, k: usize, type0: Ref, alloc: &mut impl FnMut() -> Ref) -> Result<(), String> {
        let (fi, used) = &self.used[k];
        let f = &self.files[*fi];
        let (cid, desc, file, cmap) = (alloc(), alloc(), alloc(), alloc());
        let tag: String = (0..6).map(|i| (b'A' + ((k / 26usize.pow(i)) % 26) as u8) as char).collect();
        let ps: String = f.key.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        let base = format!("{tag}+{ps}");
        let to1000 = |v: f32| v * 1000.0 / f.upem;

        let mut t0 = pdf.type0_font(type0);
        t0.base_font(Name(base.as_bytes())).encoding_predefined(Name(b"Identity-H")).descendant_font(cid).to_unicode(cmap);
        t0.finish();

        let mut cf = pdf.cid_font(cid);
        cf.subtype(if f.cff { CidFontType::Type0 } else { CidFontType::Type2 })
            .base_font(Name(base.as_bytes()))
            .system_info(IDENTITY)
            .font_descriptor(desc)
            .default_width(0.0);
        if !f.cff {
            cf.cid_to_gid_map_predefined(Name(b"Identity"));
        }
        let mut w = cf.widths();
        for (new, (old, _)) in &used.glyphs {
            w.consecutive(*new, [to1000(f.advances.get(*old as usize).copied().unwrap_or(0) as f32)]);
        }
        w.finish();
        cf.finish();

        let mut fd = pdf.font_descriptor(desc);
        fd.name(Name(base.as_bytes()))
            .flags(FontFlags::SYMBOLIC)
            .bbox(PdfRect::new(to1000(f.bbox[0]), to1000(f.bbox[1]), to1000(f.bbox[2]), to1000(f.bbox[3])))
            .italic_angle(0.0)
            .ascent(to1000(f.ascent))
            .descent(to1000(f.descent))
            .cap_height(to1000(f.cap))
            .stem_v(80.0);
        if f.cff {
            fd.font_file3(file);
        } else {
            fd.font_file2(file);
        }
        fd.finish();

        let sub = subsetter::subset((*f.data.font).as_ref(), f.data.index, &used.remap).map_err(|e| format!("font {}: {e:?}", f.key))?;
        if f.cff {
            // a CFF font goes in as its bare CFF table (CID-keyed after subsetting)
            let t = tables(&sub, 0).ok_or("subset font has no tables")?;
            let &(o, l) = t.get(b"CFF ").ok_or("subset font has no CFF table")?;
            let cff = sub.get(o..o + l).ok_or("subset CFF table out of range")?;
            pdf.stream(file, &deflate(cff)).filter(Filter::FlateDecode).pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
        } else {
            pdf.stream(file, &deflate(&sub)).filter(Filter::FlateDecode).pair(Name(b"Length1"), sub.len() as i32);
        }

        let mut uc = UnicodeCmap::new(Name(b"Fermata-UCS"), IDENTITY);
        for (new, (_, ch)) in &used.glyphs {
            if *new != 0 {
                uc.pair(*new, *ch);
            }
        }
        let data = uc.finish();
        pdf.cmap(cmap, &data);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, Pitch};

    fn estudo() -> Score {
        serde_json::from_slice(include_bytes!("../tests/corpus/estudo-em-sol.fmt")).unwrap()
    }

    fn count(hay: &[u8], needle: &[u8]) -> usize {
        hay.windows(needle.len()).filter(|w| *w == needle).count()
    }

    /// Pages of the document (`/Type /Page` objects, not `/Pages`).
    fn pages(pdf: &[u8]) -> usize {
        count(pdf, b"/Type /Page\n")
    }

    #[test]
    fn one_pdf_page_per_score_page_on_letter_and_a4() {
        for (w, h) in [(612.0, 792.0), (595.0, 842.0)] {
            let mut sc = crate::score_view::page_demo::piano_page();
            sc.layout.page_w = w;
            sc.layout.page_h = h;
            let n = crate::score_view::layout(&print_score(&sc), Pos2::ZERO, 1.0, false).pages.len();
            let pdf = score_pdf(&sc).unwrap();
            assert!(pdf.starts_with(b"%PDF-"));
            assert_eq!(pages(&pdf), n);
            let mb = format!("/MediaBox [0 0 {w} {h}]");
            assert_eq!(count(&pdf, mb.as_bytes()), n, "every page is the paper of Page Setup");
        }
    }

    #[test]
    fn fonts_are_embedded_subsets_with_text_maps() {
        let pdf = score_pdf(&estudo()).unwrap();
        // Bravura (music) and the page's text font, each a subset (tagged name), with a ToUnicode map
        assert_eq!(count(&pdf, b"/Subtype /Type0"), 2);
        assert!(count(&pdf, b"+Bravura") >= 1);
        assert!(count(&pdf, b"+Ubuntu-Light") >= 1);
        assert_eq!(count(&pdf, b"/ToUnicode"), 2);
        assert_eq!(count(&pdf, b"/FontFile3") + count(&pdf, b"/FontFile2"), 2);
        // a subset, not the whole fonts (Bravura alone is about 500 KB)
        assert!(pdf.len() < 60_000, "the PDF has {} bytes", pdf.len());
    }

    #[test]
    fn the_title_and_composer_are_in_the_metadata() {
        let pdf = score_pdf(&estudo()).unwrap();
        assert_eq!(count(&pdf, b"/Title (Estudo em Sol)"), 1);
        assert_eq!(count(&pdf, b"/Author (Fermata Design)"), 1);
        assert_eq!(count(&pdf, b"/Creator (Fermata "), 1);
        assert_eq!(count(&pdf, b"/CreationDate (D:"), 1);
    }

    #[test]
    fn the_print_scale_changes_how_much_fits_on_the_paper() {
        let mut sc = crate::score_view::page_demo::piano_page();
        let n = |sc: &Score| pages(&score_pdf(sc).unwrap());
        let at100 = n(&sc);
        sc.layout.print_scale = 200;
        let at200 = n(&sc);
        sc.layout.print_scale = 50;
        let at50 = n(&sc);
        assert!(at200 > at100, "200%: {at200} pages, 100%: {at100}");
        assert!(at50 <= at100);
        // the paper stays the paper
        assert_eq!(count(&score_pdf(&sc).unwrap(), b"/MediaBox [0 0 612 792]"), at50);
        // and the screen doesn't change
        let screen = |sc: &Score| crate::score_view::layout(sc, Pos2::ZERO, 1.0, false).pages.len();
        let mut a = crate::score_view::page_demo::piano_page();
        let before = screen(&a);
        a.layout.print_scale = 200;
        assert_eq!(screen(&a), before);
    }

    #[test]
    fn hidden_notes_cursor_and_selection_are_not_printed() {
        let sc = estudo();
        let (_, base) = page_shapes(&sc);
        let mut hid = sc.clone();
        let mut e = Event::new(2, 0, 3, vec![Pitch::from_midi(60, false)]);
        e.hidden = true;
        hid.events[0][0].push(e);
        hid.sort_measure(0, 0);
        let (_, with_hidden) = page_shapes(&hid);
        assert_eq!(base[0].len(), with_hidden[0].len(), "a hidden note draws nothing on the printed page");
        // nothing in the page's colour of the screen's cursor or selection (blue, red)
        for s in &base[0] {
            let c = match s {
                Shape::Rect(r) => r.fill,
                Shape::LineSegment { stroke, .. } => stroke.color,
                _ => continue,
            };
            assert!(c.r() == c.g() && c.g() == c.b() || c.a() == 0, "a coloured shape {c:?} on the printed page");
        }
    }

    #[test]
    fn page_setup_margins_in_mm_and_ok_without_changes_moves_nothing() {
        let mut l = estudo().layout;
        let before = l.clone();
        let mm = l.margins().map(pt_to_mm);
        let (paper, scale, spacing) = ((l.page_w, l.page_h), l.print_scale, (l.staff_space, l.staff_gap, l.system_gap));
        apply_page_setup(&mut l, paper, mm, scale, spacing);
        assert_eq!(l, before, "OK without changes");
        apply_page_setup(&mut l, (595.0, 842.0), [20.0, 25.0, 15.0, 15.0], 75, (7.0, 9.0, 10.0));
        let m = l.margins();
        assert!((m[0] - mm_to_pt(20.0)).abs() < 1e-3 && (m[1] - mm_to_pt(25.0)).abs() < 1e-3);
        assert_eq!(l.print_scale, 75);
        // four equal margins are one value again, as older files keep it
        apply_page_setup(&mut l, (595.0, 842.0), [12.7; 4], 100, (7.0, 9.0, 10.0));
        assert!(l.margins.is_none() && (l.margin - 36.0).abs() < 0.01);
    }

    #[test]
    fn fit_width_fills_the_line_without_crowding() {
        let mut sc = estudo();
        let f = fit_width_scale(&sc);
        assert!((50..=200).contains(&f));
        sc.layout.print_scale = f;
        let ps = print_score(&sc);
        let geo = crate::score_view::layout(&ps, Pos2::ZERO, 1.0, false);
        let right = geo.pages[0].right() - ps.layout.margins()[3];
        assert!(geo.systems.iter().all(|s| s.right <= right + 0.5));
    }
}
