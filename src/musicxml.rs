//! MusicXML import (F-07): `.musicxml` / `.xml` (score-partwise or score-timewise) and compressed `.mxl`.
//!
//! Written from the public MusicXML 4.0 specification (w3.org/2021/06/musicxml40) and the ZIP file format note
//! (PKWARE APPNOTE). The XML reader and the ZIP reader are our own; only the deflate step uses `miniz_oxide`,
//! which the app already ships through `png`.
//!
//! What the model can't hold is skipped without failing, and counted in `Import::skipped`, which the status bar
//! shows after opening the file.
use crate::model::*;
use std::collections::{BTreeMap, HashMap};

// ---------------------------------------------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------------------------------------------

/// One XML element: name, attributes, child elements and the text directly inside it.
#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == k).map(|a| a.1.as_str())
    }
    pub fn child(&self, n: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == n)
    }
    pub fn all<'a>(&'a self, n: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == n)
    }
    pub fn has(&self, n: &str) -> bool {
        self.child(n).is_some()
    }
    /// Trimmed text of a child element.
    pub fn text_of(&self, n: &str) -> Option<&str> {
        self.child(n).map(|c| c.text.trim())
    }
    pub fn num<T: std::str::FromStr>(&self, n: &str) -> Option<T> {
        self.text_of(n).and_then(|t| t.parse().ok())
    }
    pub fn fnum(&self, n: &str) -> Option<f64> {
        self.text_of(n).and_then(|t| t.parse::<f64>().ok()).filter(|x| x.is_finite())
    }
}

/// Deepest element nesting accepted (MusicXML needs about 10); deeper files are refused instead of risking the stack.
const MAX_DEPTH: usize = 200;

/// Text of an XML file: UTF-8 (with or without BOM) or UTF-16 with a BOM.
fn decode_text(b: &[u8]) -> String {
    let utf16 = |le: bool, b: &[u8]| {
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) }).collect();
        String::from_utf16_lossy(&units)
    };
    if b.starts_with(&[0xff, 0xfe]) {
        utf16(true, &b[2..])
    } else if b.starts_with(&[0xfe, 0xff]) {
        utf16(false, &b[2..])
    } else if b.starts_with(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8_lossy(&b[3..]).into_owned()
    } else {
        String::from_utf8_lossy(b).into_owned()
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse an XML document into its root element.
pub fn parse_xml(src: &str) -> Result<Node, String> {
    let b = src.as_bytes();
    let mut i = 0usize;
    // the open elements; stack[0] is a holder for the root
    let mut stack: Vec<Node> = vec![Node::default()];
    let err = |what: &str, at: usize| Err(format!("Broken XML: {what} (at byte {at})"));
    while i < b.len() {
        if b[i] != b'<' {
            let end = src[i..].find('<').map(|e| i + e).unwrap_or(b.len());
            if stack.len() > 1 {
                let t = unescape(&src[i..end]);
                stack.last_mut().unwrap().text.push_str(&t);
            }
            i = end;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<?") {
            i = src[i..].find("?>").map(|e| i + e + 2).ok_or("Broken XML: unclosed <?")?;
        } else if rest.starts_with("<!--") {
            i = src[i + 4..].find("-->").map(|e| i + 4 + e + 3).ok_or("Broken XML: unclosed comment")?;
        } else if rest.starts_with("<![CDATA[") {
            let e = src[i + 9..].find("]]>").map(|e| i + 9 + e).ok_or("Broken XML: unclosed CDATA")?;
            if stack.len() > 1 {
                stack.last_mut().unwrap().text.push_str(&src[i + 9..e]);
            }
            i = e + 3;
        } else if rest.starts_with("<!") {
            // <!DOCTYPE ...> with an optional [internal subset]
            let mut j = i + 2;
            let mut depth = 0i32;
            while j < b.len() {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'>' if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if j >= b.len() {
                return err("unclosed <!", i);
            }
            i = j + 1;
        } else if rest.starts_with("</") {
            let e = src[i..].find('>').map(|e| i + e).ok_or("Broken XML: unclosed end tag")?;
            let name = src[i + 2..e].trim();
            if stack.len() < 2 {
                return err("end tag without a start", i);
            }
            let node = stack.pop().unwrap();
            if node.name != name {
                return err(&format!("<{}> closed by </{name}>", node.name), i);
            }
            stack.last_mut().unwrap().children.push(node);
            i = e + 1;
        } else {
            // start tag: name, attributes, maybe self-closing
            let mut j = i + 1;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let name = src[i + 1..j].to_string();
            if name.is_empty() {
                return err("empty tag name", i);
            }
            let mut node = Node { name, ..Default::default() };
            let mut closed = false;
            loop {
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j >= b.len() {
                    return err("unclosed tag", i);
                }
                if b[j] == b'>' {
                    j += 1;
                    break;
                }
                if b[j] == b'/' {
                    if b.get(j + 1) != Some(&b'>') {
                        return err("bad '/' in tag", j);
                    }
                    closed = true;
                    j += 2;
                    break;
                }
                let k0 = j;
                while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                    j += 1;
                }
                let key = src[k0..j].to_string();
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if b.get(j) != Some(&b'=') {
                    return err("attribute without a value", j);
                }
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let q = *b.get(j).ok_or("Broken XML: unclosed tag")?;
                if q != b'"' && q != b'\'' {
                    return err("unquoted attribute", j);
                }
                let v0 = j + 1;
                let v1 = src[v0..].find(q as char).map(|e| v0 + e).ok_or("Broken XML: unclosed attribute")?;
                node.attrs.push((key, unescape(&src[v0..v1])));
                j = v1 + 1;
            }
            i = j;
            if closed {
                stack.last_mut().unwrap().children.push(node);
            } else {
                if stack.len() > MAX_DEPTH {
                    return err("elements nested too deeply", i);
                }
                stack.push(node);
            }
        }
    }
    if stack.len() != 1 {
        return Err(format!("Broken XML: <{}> is never closed", stack.last().map(|n| n.name.as_str()).unwrap_or("?")));
    }
    let mut holder = stack.pop().unwrap();
    holder.children.pop().ok_or_else(|| "Broken XML: no root element".to_string())
}

// ---------------------------------------------------------------------------------------------------------------
// .mxl (ZIP)
// ---------------------------------------------------------------------------------------------------------------

/// Largest uncompressed entry read from a .mxl (a guard against "zip bombs").
const MAX_ENTRY: usize = 64 << 20;

fn u16le(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize)
}
fn u32le(b: &[u8], i: usize) -> Option<usize> {
    b.get(i..i + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize)
}

/// The entries of a ZIP file: (name, method, compressed size, local header offset).
pub(crate) fn zip_entries(b: &[u8]) -> Result<Vec<(String, usize, usize, usize)>, String> {
    // the end-of-central-directory record is in the last 64 KiB + 22 bytes
    let from = b.len().saturating_sub(22 + 65535);
    let eocd = (from..b.len().saturating_sub(21)).rev().find(|&i| b[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or("Not a .mxl file (no ZIP directory)")?;
    let count = u16le(b, eocd + 10).ok_or("Damaged .mxl")?;
    let mut p = u32le(b, eocd + 16).ok_or("Damaged .mxl")?;
    let mut out = vec![];
    for _ in 0..count {
        if b.get(p..p + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("Damaged .mxl (bad ZIP directory)".into());
        }
        let method = u16le(b, p + 10).ok_or("Damaged .mxl")?;
        let csize = u32le(b, p + 20).ok_or("Damaged .mxl")?;
        let nlen = u16le(b, p + 28).ok_or("Damaged .mxl")?;
        let xlen = u16le(b, p + 30).ok_or("Damaged .mxl")?;
        let clen = u16le(b, p + 32).ok_or("Damaged .mxl")?;
        let off = u32le(b, p + 42).ok_or("Damaged .mxl")?;
        let name = b.get(p + 46..p + 46 + nlen).ok_or("Damaged .mxl")?;
        out.push((String::from_utf8_lossy(name).into_owned(), method, csize, off));
        p += 46 + nlen + xlen + clen;
    }
    Ok(out)
}

pub(crate) fn zip_read(b: &[u8], e: &(String, usize, usize, usize)) -> Result<Vec<u8>, String> {
    let (_, method, csize, off) = e;
    if b.get(*off..off + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
        return Err("Damaged .mxl (bad entry)".into());
    }
    let nlen = u16le(b, off + 26).ok_or("Damaged .mxl")?;
    let xlen = u16le(b, off + 28).ok_or("Damaged .mxl")?;
    let start = off + 30 + nlen + xlen;
    let data = b.get(start..start + csize).ok_or("Damaged .mxl (entry cut short)")?;
    match method {
        0 => Ok(data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_ENTRY).map_err(|_| "Damaged .mxl (can't decompress)".to_string()),
        m => Err(format!("This .mxl uses a compression Fermata doesn't read (method {m})")),
    }
}

/// The score inside a compressed MusicXML file (.mxl): the file named by META-INF/container.xml, or else the first
/// .xml / .musicxml outside META-INF.
pub fn mxl_score_xml(b: &[u8]) -> Result<Vec<u8>, String> {
    let entries = zip_entries(b)?;
    let mut target: Option<String> = None;
    if let Some(c) = entries.iter().find(|e| e.0 == "META-INF/container.xml") {
        if let Ok(root) = parse_xml(&decode_text(&zip_read(b, c)?)) {
            target = root.child("rootfiles").and_then(|r| r.all("rootfile").find(|f| f.attr("media-type").map(|m| m.contains("musicxml") || m.ends_with("xml")).unwrap_or(true))).and_then(|f| f.attr("full-path")).map(|s| s.to_string());
        }
    }
    let entry = match target {
        Some(t) => entries.iter().find(|e| e.0 == t),
        None => None,
    }
    .or_else(|| entries.iter().find(|e| !e.0.starts_with("META-INF/") && (e.0.ends_with(".xml") || e.0.ends_with(".musicxml"))))
    .ok_or("The .mxl has no score inside")?;
    zip_read(b, entry)
}

// ---------------------------------------------------------------------------------------------------------------
// MusicXML -> Score
// ---------------------------------------------------------------------------------------------------------------

/// Does this file look like MusicXML (by name or content)?
pub fn is_musicxml(name: &str, bytes: &[u8]) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    if matches!(ext.as_str(), "musicxml" | "mxl") || bytes.starts_with(b"PK\x03\x04") && ext != "fmt" && ext != "json" {
        return true;
    }
    let head = decode_text(&bytes[..bytes.len().min(4096)]);
    head.contains("<score-partwise") || head.contains("<score-timewise") || ext == "xml" && head.trim_start().starts_with('<')
}

/// The result of an import: the score and what was left out (description -> how many times).
pub struct Import {
    pub score: Score,
    pub skipped: BTreeMap<String, usize>,
}

impl Import {
    /// One line for the status bar.
    pub fn summary(&self, file: &str) -> String {
        if self.skipped.is_empty() {
            return format!("Opened {file} (MusicXML).");
        }
        let parts: Vec<String> = self.skipped.iter().map(|(k, n)| if *n > 1 { format!("{k} ×{n}") } else { k.clone() }).collect();
        format!("Opened {file} (MusicXML). Not imported: {}.", parts.join(", "))
    }
}

/// Open a MusicXML file (plain or .mxl).
pub fn import(name: &str, bytes: &[u8]) -> Result<Import, String> {
    let xml = if bytes.starts_with(b"PK") { mxl_score_xml(bytes)? } else { bytes.to_vec() };
    let root = parse_xml(&decode_text(&xml))?;
    let root = match root.name.as_str() {
        "score-partwise" => root,
        "score-timewise" => timewise_to_partwise(root),
        other => return Err(format!("Not a MusicXML score (the file is <{other}>)")),
    };
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = stem.rsplit_once('.').map(|x| x.0).unwrap_or(stem);
    Reader::default().read(&root, stem)
}

fn timewise_to_partwise(root: Node) -> Node {
    let mut parts: Vec<Node> = vec![];
    let mut out = Node { name: "score-partwise".into(), attrs: root.attrs.clone(), ..Default::default() };
    for c in root.children {
        if c.name != "measure" {
            out.children.push(c);
            continue;
        }
        for p in c.children.iter().filter(|p| p.name == "part") {
            let id = p.attr("id").unwrap_or("").to_string();
            let idx = match parts.iter().position(|x| x.attr("id") == Some(id.as_str())) {
                Some(i) => i,
                None => {
                    parts.push(Node { name: "part".into(), attrs: vec![("id".into(), id)], ..Default::default() });
                    parts.len() - 1
                }
            };
            parts[idx].children.push(Node { name: "measure".into(), attrs: c.attrs.clone(), children: p.children.clone(), text: String::new() });
        }
    }
    out.children.extend(parts);
    out
}

/// Something placed at a time in a staff that belongs to a note there (or becomes a text graphic).
enum Attach {
    Dynamic(String),
    Words(String),
    Chord(String),
    Mark(Mark),
}

struct Pending {
    staff: usize,
    measure: usize,
    tick: u32,
    what: Attach,
}

/// An open spanner (slur, hairpin, octave line) waiting for its end.
struct OpenSpan {
    kind: GraphicKind,
    staff: usize,
    measure: usize,
    tick: u32,
    /// highest/lowest step of the first note (slurs), for the curve's height
    step: Option<(i32, i32)>,
    below: bool,
}

/// A finished slur: its start, and the measure, tick and step range of its last note.
type SlurEnd = (OpenSpan, usize, u32, Option<(i32, i32)>);

#[derive(Default)]
struct PartInfo {
    name: String,
    abbrev: String,
    channel: Option<u8>,
    program: u8,
    volume: Option<u8>,
    pan: Option<i8>,
}

#[derive(Default)]
struct Reader {
    skipped: BTreeMap<String, usize>,
    /// per measure index: time signature, and key per part
    time: Vec<Option<(u8, u8)>>,
    keys: Vec<Vec<Option<i8>>>,
    tempo: Vec<Option<u32>>,
    measure_len: Vec<u32>,
    implicit_first: bool,
    staves: Vec<Staff>,
    /// events[staff][measure]
    events: Vec<Vec<Vec<Event>>>,
    /// MusicXML voice number per staff -> (number in the file, uses)
    voices: Vec<BTreeMap<u32, usize>>,
    graphics: Vec<Graphic>,
    pending: Vec<Pending>,
    barlines: Vec<Barline>,
    endings: Vec<Option<String>>,
    repeat_times: Vec<u8>,
    coda: Vec<Option<String>>,
    rehearsal: Vec<Option<String>>,
    breaks: Vec<bool>,
    /// slurs: open spans keyed by (staff, number); the notes they join are known at the stop
    slurs: Vec<((usize, String), OpenSpan)>,
    spans: Vec<((usize, String, &'static str), OpenSpan)>,
    slur_ends: Vec<SlurEnd>,
    /// lyric "number" attribute -> verse line (0-based), in order of appearance
    verses: Vec<String>,
}

fn skip(sk: &mut BTreeMap<String, usize>, what: &str) {
    *sk.entry(what.to_string()).or_insert(0) += 1;
}

fn value_of_type(t: &str) -> Option<u8> {
    Some(match t {
        "maxima" | "long" | "breve" => 0,
        "whole" => 1,
        "half" => 2,
        "quarter" => 3,
        "eighth" => 4,
        "16th" => 5,
        "32nd" => 6,
        "64th" => 7,
        "128th" | "256th" | "512th" | "1024th" => 8,
        _ => return None,
    })
}

fn clef_of(sign: &str, line: Option<i32>, oct: i32) -> (Clef, bool) {
    // (clef, exact): an unusual clef becomes the nearest one Fermata draws
    match (sign, line, oct) {
        ("G", None | Some(2), 0) => (Clef::Treble, true),
        ("G", None | Some(2), 1) => (Clef::Treble8va, true),
        ("G", None | Some(2), -1) => (Clef::Treble8vb, true),
        ("G", _, 1) => (Clef::Treble8va, false),
        ("G", _, -1) => (Clef::Treble8vb, false),
        ("G", _, _) => (Clef::Treble, false),
        ("F", None | Some(4), 0) => (Clef::Bass, true),
        ("F", None | Some(4), -1) => (Clef::Bass8vb, true),
        ("F", _, -1) => (Clef::Bass8vb, false),
        ("F", _, _) => (Clef::Bass, false),
        ("C", Some(3), 0) => (Clef::Alto, true),
        ("C", Some(4), 0) => (Clef::Tenor, true),
        ("C", Some(5), _) => (Clef::Tenor, false),
        ("C", _, _) => (Clef::Alto, false),
        ("percussion", _, _) => (Clef::Percussion, true),
        _ => (Clef::Treble, false),
    }
}

/// A chord symbol's text from <harmony>.
fn harmony_text(h: &Node) -> Option<String> {
    let acc = |a: Option<f64>| match a.map(|x| x.round() as i32).unwrap_or(0) {
        1 => "#",
        2 => "##",
        -1 => "b",
        -2 => "bb",
        _ => "",
    };
    let kind_node = h.child("kind");
    let kind = kind_node.map(|k| k.text.trim()).unwrap_or("major");
    let root = match h.child("root") {
        Some(r) => format!("{}{}", r.text_of("root-step").unwrap_or(""), acc(r.fnum("root-alter"))),
        None if kind == "none" => return Some("N.C.".into()),
        None => return None,
    };
    if root.is_empty() {
        return None;
    }
    let suffix = match kind_node.and_then(|k| k.attr("text")) {
        Some(t) => t.to_string(),
        None => match kind {
            "major" => "",
            "minor" => "m",
            "augmented" => "+",
            "diminished" => "dim",
            "dominant" => "7",
            "major-seventh" => "maj7",
            "minor-seventh" => "m7",
            "diminished-seventh" => "dim7",
            "augmented-seventh" => "+7",
            "half-diminished" => "m7(b5)",
            "major-minor" => "m(maj7)",
            "major-sixth" => "6",
            "minor-sixth" => "m6",
            "dominant-ninth" => "9",
            "major-ninth" => "maj9",
            "minor-ninth" => "m9",
            "dominant-11th" => "11",
            "major-11th" => "maj11",
            "minor-11th" => "m11",
            "dominant-13th" => "13",
            "major-13th" => "maj13",
            "minor-13th" => "m13",
            "suspended-second" => "sus2",
            "suspended-fourth" => "sus4",
            "power" => "5",
            "Neapolitan" | "Italian" | "French" | "German" | "Tristan" | "pedal" | "other" => "",
            _ => "",
        }
        .to_string(),
    };
    let mut t = format!("{root}{suffix}");
    for d in h.all("degree") {
        let v = d.text_of("degree-value").unwrap_or("");
        let a = acc(d.fnum("degree-alter"));
        match d.text_of("degree-type").unwrap_or("add") {
            "add" => t.push_str(&format!("(add{a}{v})")),
            "alter" => t.push_str(&format!("({a}{v})")),
            "subtract" => t.push_str(&format!("(no{v})")),
            _ => {}
        }
    }
    if let Some(b) = h.child("bass") {
        let bass = format!("{}{}", b.text_of("bass-step").unwrap_or(""), acc(b.fnum("bass-alter")));
        if !bass.is_empty() {
            t.push('/');
            t.push_str(&bass);
        }
    }
    Some(t)
}

fn dynamic_of(d: &Node) -> Vec<String> {
    d.children.iter().map(|c| if c.name == "other-dynamics" { c.text.trim().to_string() } else { c.name.clone() }).filter(|s| !s.is_empty()).collect()
}

fn is_coda_phrase(w: &str) -> bool {
    let l = w.trim().to_lowercase().replace(' ', "");
    l.starts_with("d.c.") || l.starts_with("d.s.") || l.starts_with("dacapo") || l.starts_with("dalsegno") || l == "fine" || l.starts_with("tocoda")
}

impl Reader {
    fn read(mut self, root: &Node, file_stem: &str) -> Result<Import, String> {
        // part-list: names and instruments, in order; groups give braces and brackets
        let mut info: HashMap<String, PartInfo> = HashMap::new();
        let mut order: Vec<String> = vec![];
        // (part index where a group starts, symbol, barline across)
        let mut groups_open: HashMap<String, (usize, String, bool)> = HashMap::new();
        let mut groups: Vec<(usize, usize, String, bool)> = vec![];
        if let Some(pl) = root.child("part-list") {
            for c in &pl.children {
                match c.name.as_str() {
                    "score-part" => {
                        let id = c.attr("id").unwrap_or("").to_string();
                        let mut pi = PartInfo {
                            name: c.text_of("part-name").unwrap_or("").to_string(),
                            abbrev: c.text_of("part-abbreviation").unwrap_or("").to_string(),
                            ..Default::default()
                        };
                        if let Some(mi) = c.child("midi-instrument") {
                            pi.channel = mi.num::<u32>("midi-channel").filter(|c| (1..=16).contains(c)).map(|c| (c - 1) as u8);
                            pi.program = mi.num::<u32>("midi-program").map(|p| p.clamp(1, 128) - 1).unwrap_or(0) as u8;
                            pi.volume = mi.fnum("volume").map(|v| (v.clamp(0.0, 100.0) * 1.27).round() as u8);
                            pi.pan = mi.fnum("pan").map(|p| (p.clamp(-90.0, 90.0) * 64.0 / 90.0).round().clamp(-64.0, 63.0) as i8);
                        }
                        info.insert(id.clone(), pi);
                        order.push(id);
                    }
                    "part-group" => {
                        let num = c.attr("number").unwrap_or("1").to_string();
                        if c.attr("type") == Some("start") {
                            let sym = c.text_of("group-symbol").unwrap_or("none").to_string();
                            let bar = c.text_of("group-barline") == Some("yes");
                            groups_open.insert(num, (order.len(), sym, bar));
                        } else if let Some((a, sym, bar)) = groups_open.remove(&num) {
                            groups.push((a, order.len(), sym, bar));
                        }
                    }
                    _ => {}
                }
            }
        }
        let parts: Vec<&Node> = root.all("part").collect();
        if parts.is_empty() {
            return Err("The MusicXML file has no parts".into());
        }
        let nmeas = parts.iter().map(|p| p.all("measure").count()).max().unwrap_or(0).max(1);
        self.time = vec![None; nmeas];
        self.keys = vec![vec![None; parts.len()]; nmeas];
        self.tempo = vec![None; nmeas];
        self.measure_len = vec![0; nmeas];
        self.barlines = vec![Barline::Single; nmeas];
        self.endings = vec![None; nmeas];
        self.repeat_times = vec![2; nmeas];
        self.coda = vec![None; nmeas];
        self.rehearsal = vec![None; nmeas];
        self.breaks = vec![false; nmeas];
        // the first staff of each part, for groups
        let mut first_staff_of_part: Vec<(usize, usize)> = vec![];
        let mut part_transpose: Vec<i8> = vec![];
        for (pi, part) in parts.iter().enumerate() {
            let id = part.attr("id").unwrap_or("");
            let p_info = info.remove(id).unwrap_or_default();
            let base = self.staves.len();
            let tr = self.read_part(pi, part, &p_info, nmeas);
            part_transpose.push(tr);
            first_staff_of_part.push((base, self.staves.len()));
        }
        if self.staves.is_empty() {
            return Err("The MusicXML file has no staves".into());
        }
        // groups: bracket or brace from the first to the last staff of the grouped parts
        for (a, b, sym, bar) in groups {
            let (Some(&(s0, _)), Some(&(_, s1))) = (first_staff_of_part.get(a), first_staff_of_part.get(b.saturating_sub(1))) else { continue };
            for s in s0..s1.saturating_sub(1) {
                match sym.as_str() {
                    "brace" => self.staves[s].brace_with_next = true,
                    "bracket" | "square" => self.staves[s].bracket_with_next = true,
                    _ => {}
                }
                if bar {
                    self.staves[s].barline_to_next = true;
                }
            }
        }
        // measures: time signature and key carried forward; the key from a part that isn't transposed
        let mut sc = Score::new(std::mem::take(&mut self.staves), nmeas);
        let (mut ts, mut key) = ((4u8, 4u8), 0i8);
        let mut mixed_keys = false;
        for m in 0..nmeas {
            if let Some(t) = self.time[m] {
                ts = t;
            }
            let ks = &self.keys[m];
            let concert = ks.iter().enumerate().find(|(p, k)| k.is_some() && part_transpose.get(*p) == Some(&0)).and_then(|x| *x.1);
            if let Some(k) = concert.or_else(|| ks.iter().flatten().next().copied()) {
                if ks.iter().flatten().any(|x| *x != k) {
                    mixed_keys = true;
                }
                key = k;
            }
            let mi = &mut sc.measures[m];
            mi.num = ts.0;
            mi.den = ts.1;
            mi.key = key;
            mi.tempo = self.tempo[m];
            mi.barline = self.barlines[m];
            mi.ending = self.endings[m].take();
            mi.repeat_times = self.repeat_times[m];
            mi.coda_phrase = self.coda[m].take();
            mi.rehearsal = self.rehearsal[m].take();
            mi.system_break = self.breaks[m];
        }
        if mixed_keys {
            skip(&mut self.skipped, "different key signatures per part (concert key used)");
        }
        sc.events = std::mem::take(&mut self.events);
        // voices: numbers 1..8 stay; other numbering (e.g. 5-8 on a piano's lower staff) becomes 1, 2, 3...
        for (s, used) in self.voices.iter().enumerate() {
            let keep = used.keys().all(|v| (1..=8).contains(v)) && used.contains_key(&1);
            let map: HashMap<u32, u8> = used.keys().enumerate().map(|(i, v)| (*v, if keep { *v as u8 } else { (i as u8 + 1).min(8) })).collect();
            for m in sc.events[s].iter_mut() {
                for e in m.iter_mut() {
                    e.voice = map.get(&(e.voice as u32)).copied().unwrap_or(1);
                }
            }
        }
        // a pickup (an incomplete first measure): its notes go to the end of the measure, so beats line up
        if self.implicit_first && nmeas > 0 {
            let full = sc.measures[0].ticks();
            let len = self.measure_len[0];
            if len > 0 && len < full {
                for st in sc.events.iter_mut() {
                    for e in st[0].iter_mut() {
                        e.start += full - len;
                    }
                }
                for p in self.pending.iter_mut().filter(|p| p.measure == 0) {
                    p.tick += full - len;
                }
                skip(&mut self.skipped, "pickup measure (filled with a rest before it)");
            }
        }
        for s in 0..sc.staves.len() {
            for m in 0..nmeas {
                for e in sc.events[s][m].iter_mut() {
                    e.sort_pitches();
                }
                sc.sort_measure(s, m);
            }
        }
        // overfull measures (content longer than the time signature)
        let over = (0..nmeas).filter(|&m| self.measure_len[m] > sc.measures[m].ticks()).count();
        for _ in 0..over {
            skip(&mut self.skipped, "measure longer than its time signature");
        }
        self.attach_pending(&mut sc);
        self.finish_slurs(&mut sc);
        sc.graphics.append(&mut self.graphics);
        // texts
        let work = root.child("work").and_then(|w| w.text_of("work-title")).unwrap_or("").to_string();
        let movement = root.text_of("movement-title").unwrap_or("").to_string();
        sc.text.title = if !work.is_empty() { work } else { movement.clone() };
        if sc.text.title != movement {
            sc.text.subtitle = movement;
        }
        if let Some(id) = root.child("identification") {
            for c in id.all("creator") {
                let t = c.text.trim().to_string();
                match c.attr("type") {
                    Some("composer") => sc.text.composer = t,
                    Some("lyricist") | Some("poet") | Some("arranger") if sc.text.lyricist.is_empty() => sc.text.lyricist = t,
                    _ => {}
                }
            }
            if let Some(r) = id.text_of("rights") {
                sc.text.copyright = r.to_string();
            }
        }
        for cr in root.all("credit") {
            let ty = cr.text_of("credit-type").unwrap_or("");
            let words: Vec<&str> = cr.all("credit-words").map(|w| w.text.trim()).filter(|w| !w.is_empty()).collect();
            if words.is_empty() {
                continue;
            }
            let w = words.join(" ");
            let slot = match ty {
                "title" => &mut sc.text.title,
                "subtitle" => &mut sc.text.subtitle,
                "composer" => &mut sc.text.composer,
                "lyricist" | "arranger" => &mut sc.text.lyricist,
                "rights" => &mut sc.text.copyright,
                _ => continue,
            };
            if slot.is_empty() {
                *slot = w;
            }
        }
        if sc.text.title.trim().is_empty() {
            sc.text.title = if file_stem.is_empty() { "Imported MusicXML".into() } else { file_stem.to_string() };
        }
        crate::io::repair(&mut sc);
        Ok(Import { score: sc, skipped: self.skipped })
    }

    /// Read one <part>; returns its transposition.
    fn read_part(&mut self, pi: usize, part: &Node, info: &PartInfo, nmeas: usize) -> i8 {
        // the staves of this part are created when <staves> is known (default 1)
        let base = self.staves.len();
        let mut nst = 0usize;
        let ensure = |rd: &mut Reader, n: usize, nst: &mut usize| {
            while *nst < n.clamp(1, 16) {
                let name = if *nst == 0 { info.name.as_str() } else { "" };
                let mut st = Staff::new(name, Clef::Treble, info.program);
                st.abbrev = if *nst == 0 && !info.abbrev.is_empty() { info.abbrev.clone() } else if *nst == 0 { st.abbrev } else { String::new() };
                st.channel = info.channel.unwrap_or_else(|| {
                    let c = (pi % 15) as u8;
                    if c >= 9 { c + 1 } else { c }
                });
                st.drum = info.channel == Some(9);
                if let Some(v) = info.volume {
                    st.volume = v;
                }
                if let Some(p) = info.pan {
                    st.pan = p;
                }
                if *nst > 0 {
                    rd.staves[base + *nst - 1].brace_with_next = true;
                    rd.staves[base + *nst - 1].barline_to_next = true;
                }
                rd.staves.push(st);
                rd.events.push(vec![vec![]; nmeas]);
                rd.voices.push(BTreeMap::new());
                *nst += 1;
            }
        };
        ensure(self, 1, &mut nst);
        let mut divisions: i64 = 1;
        let mut transpose: i8 = 0;
        // per measure: position in divisions; the last note made (staff, index) for <chord/>
        for (m, meas) in part.all("measure").enumerate() {
            if m >= nmeas {
                break;
            }
            if m == 0 && meas.attr("implicit") == Some("yes") {
                self.implicit_first = true;
            }
            let mut pos: i64 = 0;
            let mut max_pos: i64 = 0;
            let mut last: Option<(usize, usize)> = None;
            let mut last_start: i64 = 0;
            let tick = |pos: i64, div: i64| -> u32 { (pos.max(0) * TPQ as i64 + div / 2).checked_div(div).unwrap_or(0).clamp(0, u32::MAX as i64) as u32 };
            for c in &meas.children {
                match c.name.as_str() {
                    "attributes" => {
                        if let Some(d) = c.num::<i64>("divisions").filter(|d| *d > 0 && *d < 1_000_000) {
                            // keep the position in the new unit
                            pos = pos * d / divisions;
                            last_start = last_start * d / divisions;
                            max_pos = max_pos * d / divisions;
                            divisions = d;
                        }
                        if let Some(n) = c.num::<usize>("staves") {
                            ensure(self, n, &mut nst);
                        }
                        for k in c.all("key") {
                            match k.num::<i32>("fifths") {
                                Some(f) => {
                                    if k.attr("number").map(|n| n == "1").unwrap_or(true) {
                                        self.keys[m][pi] = Some(f.clamp(-7, 7) as i8);
                                    }
                                }
                                None => skip(&mut self.skipped, "non-standard key signature"),
                            }
                        }
                        if let Some(t) = c.child("time") {
                            if t.has("senza-misura") {
                                skip(&mut self.skipped, "unmeasured time (senza misura)");
                            } else {
                                let beats: Option<u32> = t.text_of("beats").map(|b| b.split('+').filter_map(|x| x.trim().parse::<u32>().ok()).sum()).filter(|b| *b > 0);
                                let bt: Option<u32> = t.num("beat-type");
                                match (beats, bt) {
                                    (Some(b), Some(d)) if b <= 99 && [1, 2, 4, 8, 16, 32, 64].contains(&d) => {
                                        if self.time[m].is_none() {
                                            self.time[m] = Some((b as u8, d as u8));
                                        }
                                    }
                                    _ => skip(&mut self.skipped, "unusual time signature"),
                                }
                            }
                        }
                        for cl in c.all("clef") {
                            let sn = cl.attr("number").and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).max(1);
                            ensure(self, sn, &mut nst);
                            let sign = cl.text_of("sign").unwrap_or("G");
                            if sign == "TAB" {
                                self.staves[base + sn - 1].tab = true;
                                self.staves[base + sn - 1].lines = 6;
                                continue;
                            }
                            if sign == "none" {
                                continue;
                            }
                            let (clef, exact) = clef_of(sign, cl.num("line"), cl.num("clef-octave-change").unwrap_or(0));
                            if !exact {
                                skip(&mut self.skipped, &format!("{sign} clef on line {} (nearest clef used)", cl.text_of("line").unwrap_or("?")));
                            }
                            if clef == Clef::Percussion {
                                self.staves[base + sn - 1].drum = true;
                            }
                            // Fermata changes clef at a bar line: a change inside the measure takes effect at the next one
                            let at = if pos <= 0 { m } else { m + 1 };
                            if pos > 0 && tick(pos, divisions) < self.measure_len_guess(m) {
                                skip(&mut self.skipped, "clef change inside a measure (moved to the next bar line)");
                            }
                            if at < nmeas {
                                self.staves[base + sn - 1].set_clef(at, clef);
                            }
                        }
                        if let Some(t) = c.child("transpose") {
                            let semis = t.num::<i32>("chromatic").unwrap_or(0) + 12 * t.num::<i32>("octave-change").unwrap_or(0);
                            transpose = semis.clamp(-48, 48) as i8;
                        }
                        for sd in c.all("staff-details") {
                            let sn = sd.attr("number").and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).max(1);
                            ensure(self, sn, &mut nst);
                            if let Some(l) = sd.num::<u8>("staff-lines").filter(|l| (1..=10).contains(l)) {
                                self.staves[base + sn - 1].lines = l;
                            }
                        }
                    }
                    "backup" => {
                        pos = (pos - c.num::<i64>("duration").unwrap_or(0).max(0)).max(0);
                    }
                    "forward" => {
                        pos += c.num::<i64>("duration").unwrap_or(0).max(0);
                        max_pos = max_pos.max(pos);
                    }
                    "note" => {
                        let sn = c.num::<usize>("staff").unwrap_or(1).clamp(1, 16);
                        ensure(self, sn, &mut nst);
                        let s = base + sn - 1;
                        let grace = c.has("grace");
                        let is_chord = c.has("chord");
                        let dur = if grace { 0 } else { c.num::<i64>("duration").unwrap_or(0).clamp(0, 1 << 30) };
                        let start = if is_chord { last_start } else { pos };
                        if c.has("cue") {
                            skip(&mut self.skipped, "cue note");
                            if !is_chord {
                                pos += dur;
                                max_pos = max_pos.max(pos);
                            }
                            continue;
                        }
                        let pitch = if let Some(p) = c.child("pitch") {
                            let letter = LETTERS.iter().position(|l| p.text_of("step").and_then(|s| s.chars().next()) == Some(*l));
                            let alter = p.fnum("alter").unwrap_or(0.0);
                            if alter.fract() != 0.0 {
                                skip(&mut self.skipped, "microtone (rounded)");
                            }
                            letter.map(|l| Pitch { step: p.num::<i32>("octave").unwrap_or(4).clamp(0, 9) * 7 + l as i32, alter: alter.round().clamp(-2.0, 2.0) as i8 })
                        } else if let Some(u) = c.child("unpitched") {
                            let letter = LETTERS.iter().position(|l| u.text_of("display-step").and_then(|s| s.chars().next()) == Some(*l)).unwrap_or(6);
                            Some(Pitch { step: u.num::<i32>("display-octave").unwrap_or(4).clamp(0, 9) * 7 + letter as i32, alter: 0 })
                        } else {
                            None
                        };
                        let is_rest = c.has("rest");
                        if pitch.is_none() && !is_rest {
                            skip(&mut self.skipped, "note without a pitch");
                        }
                        let mtick = tick(start, divisions);
                        let ticks = tick(dur, divisions);
                        // a whole-measure rest is an empty measure in Fermata
                        let measure_rest = is_rest && c.child("rest").and_then(|r| r.attr("measure")) == Some("yes");
                        let mut made: Option<usize> = None;
                        if is_chord && pitch.is_some() {
                            if let Some((_, li)) = last.filter(|l| l.0 == s) {
                                let e = &mut self.events[s][m][li];
                                if !e.is_rest() {
                                    e.pitches.extend(pitch);
                                    e.normalize();
                                    made = Some(li);
                                }
                            }
                        }
                        if made.is_none() && !(measure_rest || pitch.is_none() && !is_rest) {
                            let tm = c.child("time-modification");
                            let tuplet = tm.and_then(|t| Some((t.num::<u32>("actual-notes")?, t.num::<u32>("normal-notes")?))).filter(|(a, n)| *a > 0 && *n > 0 && *a <= 255 && *n <= 255).map(|(a, n)| (a as u8, n as u8));
                            let dots = c.all("dot").count().min(3) as u8;
                            let value = match c.text_of("type").and_then(value_of_type) {
                                Some(v) => v,
                                None => {
                                    let raw = match tuplet {
                                        Some((a, n)) => ticks * a as u32 / n as u32,
                                        None => ticks,
                                    };
                                    crate::io::guess_value(raw.max(1)).0
                                }
                            };
                            let voice_no: u32 = c.num("voice").unwrap_or(1).clamp(1, 255);
                            *self.voices[s].entry(voice_no).or_insert(0) += 1;
                            let mut e = Event::new(voice_no.min(255) as u8, mtick, value, pitch.into_iter().collect());
                            e.dots = dots;
                            e.tuplet = tuplet;
                            e.grace = grace;
                            if !grace && dur > 0 {
                                let written = dur_ticks(value, dots, tuplet);
                                if written.abs_diff(ticks) > 1 {
                                    // the file's own length plays; the written value still shows
                                    e.play_len = Some(ticks.max(1));
                                }
                            }
                            e.hidden = c.attr("print-object") == Some("no");
                            e.stem = match c.text_of("stem") {
                                Some("up") => Stem::Up,
                                Some("down") => Stem::Down,
                                _ => Stem::Auto,
                            };
                            if let Some(v) = c.attr("dynamics").and_then(|d| d.parse::<f64>().ok()) {
                                e.velocity = (v * 90.0 / 100.0).round().clamp(1.0, 127.0) as u8;
                            }
                            self.events[s][m].push(e);
                            made = Some(self.events[s][m].len() - 1);
                        }
                        if let Some(idx) = made {
                            let k = self.events[s][m][idx].pitches.len().saturating_sub(1);
                            self.note_details(c, s, m, idx, k);
                            last = Some((s, idx));
                        }
                        if !is_chord {
                            last_start = pos;
                            pos += dur;
                            max_pos = max_pos.max(pos);
                        }
                    }
                    "direction" => {
                        let sn = c.num::<usize>("staff").unwrap_or(1).clamp(1, nst.max(1));
                        let s = base + sn - 1;
                        let off = c.num::<i64>("offset").unwrap_or(0);
                        let at = tick((pos + off).max(0), divisions);
                        let below = c.attr("placement") == Some("below");
                        for dt in c.all("direction-type") {
                            self.direction_type(dt, s, m, at, below);
                        }
                        if let Some(snd) = c.child("sound") {
                            self.sound(snd, m);
                        }
                    }
                    "sound" => self.sound(c, m),
                    "harmony" => {
                        let sn = c.num::<usize>("staff").unwrap_or(1).clamp(1, nst.max(1));
                        let off = c.num::<i64>("offset").unwrap_or(0);
                        match harmony_text(c) {
                            Some(t) => self.pending.push(Pending { staff: base + sn - 1, measure: m, tick: tick((pos + off).max(0), divisions), what: Attach::Chord(t) }),
                            None => skip(&mut self.skipped, "chord symbol without a root"),
                        }
                    }
                    "barline" => self.barline(c, m),
                    "print" => {
                        if m > 0 && (c.attr("new-system") == Some("yes") || c.attr("new-page") == Some("yes")) {
                            self.breaks[m - 1] = true;
                        }
                    }
                    "figured-bass" => skip(&mut self.skipped, "figured bass"),
                    _ => {}
                }
            }
            let len = tick(max_pos, divisions);
            self.measure_len[m] = self.measure_len[m].max(len);
        }
        for st in &mut self.staves[base..base + nst] {
            st.transpose = transpose;
        }
        transpose
    }

    fn measure_len_guess(&self, m: usize) -> u32 {
        let (n, d) = self.time[..=m.min(self.time.len().saturating_sub(1))].iter().rev().flatten().next().copied().unwrap_or((4, 4));
        TPQ * 4 * n as u32 / d.max(1) as u32
    }

    /// Ties, notations and lyrics of a <note> (k = index of its pitch in the event).
    fn note_details(&mut self, c: &Node, s: usize, m: usize, idx: usize, k: usize) {
        let e = &mut self.events[s][m][idx];
        let tie_start = c.all("tie").any(|t| t.attr("type") == Some("start")) || c.child("notations").map(|n| n.all("tied").any(|t| t.attr("type") == Some("start"))).unwrap_or(false);
        if tie_start && k < e.ties.len() && !e.is_rest() {
            e.ties[k] = true;
        }
        if let Some(a) = c.child("accidental") {
            if (a.attr("cautionary") == Some("yes") || a.attr("parentheses") == Some("yes") || a.attr("editorial") == Some("yes")) && k < e.courtesy.len() {
                e.courtesy[k] = true;
            }
        }
        let (step_lo, step_hi) = (e.pitches.iter().map(|p| p.step).min(), e.pitches.iter().map(|p| p.step).max());
        let steps = step_lo.zip(step_hi);
        let start = e.start;
        let mut marks: Vec<Mark> = vec![];
        let mut dyns: Vec<String> = vec![];
        for n in c.all("notations") {
            for x in &n.children {
                match x.name.as_str() {
                    "tied" => {}
                    "slur" => {
                        let num = x.attr("number").unwrap_or("1").to_string();
                        match x.attr("type") {
                            Some("start") => {
                                let below = x.attr("placement") == Some("below") || x.attr("orientation") == Some("under");
                                self.slurs.retain(|o| o.0 != (s, num.clone()));
                                self.slurs.push(((s, num), OpenSpan { kind: GraphicKind::Slur, staff: s, measure: m, tick: start, step: steps, below }));
                            }
                            Some("stop") => {
                                if let Some(i) = self.slurs.iter().position(|o| o.0 == (s, num.clone())) {
                                    let (_, open) = self.slurs.remove(i);
                                    self.slur_ends.push((open, m, start, steps));
                                } else if let Some(i) = self.slurs.iter().position(|o| o.0 .1 == num) {
                                    // a slur that crosses to another staff of the part ends where it started
                                    let (_, open) = self.slurs.remove(i);
                                    self.slur_ends.push((open, m, start, steps));
                                }
                            }
                            _ => {}
                        }
                    }
                    "tuplet" | "beam" => {}
                    "fermata" => marks.push(if x.attr("type") == Some("inverted") { Mark::FermataBelow } else { Mark::Fermata }),
                    "arpeggiate" => marks.push(Mark::Arpeggio),
                    "dynamics" => dyns.extend(dynamic_of(x)),
                    "articulations" => {
                        for a in &x.children {
                            match a.name.as_str() {
                                "staccato" => marks.push(Mark::Staccato),
                                "staccatissimo" | "spiccato" => marks.push(Mark::Staccatissimo),
                                "tenuto" => marks.push(Mark::Tenuto),
                                "accent" => marks.push(Mark::Accent),
                                "strong-accent" => marks.push(Mark::Marcato),
                                "detached-legato" => marks.extend([Mark::Tenuto, Mark::Staccato]),
                                "breath-mark" => marks.push(Mark::Breath),
                                "caesura" => marks.push(Mark::Caesura),
                                other => skip(&mut self.skipped, &format!("articulation {other}")),
                            }
                        }
                    }
                    "ornaments" => {
                        for o in &x.children {
                            match o.name.as_str() {
                                "trill-mark" => marks.push(Mark::Trill),
                                "turn" => marks.push(Mark::Turn),
                                "mordent" => marks.push(Mark::Mordent),
                                "inverted-mordent" => marks.push(Mark::InvMordent),
                                "tremolo" => match o.text.trim().parse::<u8>().unwrap_or(3) {
                                    1 => marks.push(Mark::Tremolo1),
                                    2 => marks.push(Mark::Tremolo2),
                                    _ => marks.push(Mark::Tremolo3),
                                },
                                "accidental-mark" | "wavy-line" => {}
                                other => skip(&mut self.skipped, &format!("ornament {other}")),
                            }
                        }
                    }
                    "technical" => {
                        for t in &x.children {
                            match t.name.as_str() {
                                "up-bow" => marks.push(Mark::UpBow),
                                "down-bow" => marks.push(Mark::DownBow),
                                "harmonic" => marks.push(Mark::Harmonic),
                                "stopped" => marks.push(Mark::Plus),
                                "fingering" => match t.text.trim().parse::<u8>() {
                                    Ok(f) if f <= 9 => marks.push(Mark::Finger(f)),
                                    _ => skip(&mut self.skipped, "fingering"),
                                },
                                "string" | "fret" => {}
                                other => skip(&mut self.skipped, &format!("technique {other}")),
                            }
                        }
                    }
                    "glissando" | "slide" => skip(&mut self.skipped, "glissando"),
                    "accidental-mark" | "other-notation" | "non-arpeggiate" => skip(&mut self.skipped, &x.name.replace('-', " ")),
                    _ => {}
                }
            }
        }
        let e = &mut self.events[s][m][idx];
        for mk in marks {
            if !e.marks.contains(&mk) {
                e.marks.push(mk);
            }
        }
        if let Some(d) = dyns.first() {
            if e.dynamic.is_none() {
                e.dynamic = Some(d.clone());
            }
        }
        // lyrics (a chord's lyric belongs to the whole chord)
        for l in c.all("lyric") {
            // number="1", "2"... is the verse; any other name gets the next free line in order of appearance
            let num = l.attr("number").unwrap_or("1").to_string();
            let line = match num.trim().parse::<usize>() {
                Ok(n) if n >= 1 => n - 1,
                _ => match self.verses.iter().position(|v| *v == num) {
                    Some(i) => i,
                    None => {
                        self.verses.push(num);
                        self.verses.len() - 1
                    }
                },
            };
            if line >= 8 {
                skip(&mut self.skipped, "lyric verse beyond the 8th");
                continue;
            }
            let texts: Vec<String> = l.all("text").map(|t| t.text.clone()).collect();
            let mut t = texts.join(" ");
            if t.trim().is_empty() {
                // only an extender or an empty syllable
                continue;
            }
            match l.text_of("syllabic") {
                Some("begin") | Some("middle") => t.push('-'),
                _ => {
                    if l.has("extend") {
                        t.push('_');
                    }
                }
            }
            let e = &mut self.events[s][m][idx];
            if e.lyric.len() <= line {
                e.lyric.resize(line + 1, String::new());
            }
            e.lyric[line] = t;
        }
    }

    fn direction_type(&mut self, dt: &Node, s: usize, m: usize, at: u32, below: bool) {
        for x in &dt.children {
            match x.name.as_str() {
                "dynamics" => {
                    for d in dynamic_of(x) {
                        self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Dynamic(d) });
                    }
                }
                "words" => {
                    let w = x.text.trim().to_string();
                    if w.is_empty() {
                        continue;
                    }
                    if is_coda_phrase(&w) {
                        self.coda[m] = Some(w);
                    } else {
                        self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Words(w) });
                    }
                }
                "rehearsal" => {
                    let r = x.text.trim();
                    if !r.is_empty() {
                        self.rehearsal[m] = Some(r.to_string());
                    }
                }
                "segno" => self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::Segno) }),
                "coda" => self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::Coda) }),
                "pedal" => match x.attr("type") {
                    Some("start") | Some("sostenuto") => self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::PedalDown) }),
                    Some("stop") => self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::PedalUp) }),
                    Some("change") => {
                        self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::PedalUp) });
                        self.pending.push(Pending { staff: s, measure: m, tick: at, what: Attach::Mark(Mark::PedalDown) });
                    }
                    _ => {}
                },
                "metronome" => {
                    // tempo in quarter notes per minute; <sound tempo> wins when both are there
                    let unit = x.text_of("beat-unit").and_then(value_of_type);
                    let pm = x.fnum("per-minute");
                    if let (Some(u), Some(pm)) = (unit, pm) {
                        let dots = x.all("beat-unit-dot").count() as u8;
                        let q = pm * dur_ticks(u, dots, None) as f64 / TPQ as f64;
                        if self.tempo[m].is_none() && q >= 1.0 {
                            self.tempo[m] = Some(q.round().clamp(10.0, 400.0) as u32);
                        }
                    } else if x.has("beat-unit") && !x.has("per-minute") {
                        skip(&mut self.skipped, "metric modulation");
                    }
                }
                "wedge" | "octave-shift" => {
                    let num = x.attr("number").unwrap_or("1").to_string();
                    let ty = x.attr("type").unwrap_or("");
                    let key = (s, num, if x.name == "wedge" { "wedge" } else { "octave" });
                    let kind = match (x.name.as_str(), ty) {
                        ("wedge", "crescendo") => Some(GraphicKind::Crescendo),
                        ("wedge", "diminuendo") => Some(GraphicKind::Decrescendo),
                        ("octave-shift", "down") => Some(GraphicKind::Ottava),
                        ("octave-shift", "up") => Some(GraphicKind::OttavaBassa),
                        _ => None,
                    };
                    if x.name == "octave-shift" && x.attr("size").map(|s| s != "8").unwrap_or(false) && kind.is_some() {
                        skip(&mut self.skipped, "15ma/15mb line (shown as 8va/8vb)");
                    }
                    if let Some(kind) = kind {
                        self.spans.retain(|o| o.0 != key);
                        self.spans.push((key, OpenSpan { kind, staff: s, measure: m, tick: at, step: None, below }));
                    } else if ty == "stop" {
                        if let Some(i) = self.spans.iter().position(|o| o.0 == key) {
                            let (_, o) = self.spans.remove(i);
                            let dy = match o.kind {
                                GraphicKind::Crescendo | GraphicKind::Decrescendo => if o.below { 6.5 } else { -2.5 },
                                GraphicKind::Ottava => -3.0,
                                _ => 7.0,
                            };
                            self.graphics.push(Graphic { kind: o.kind, staff: o.staff, measure: o.measure, tick: o.tick, end_measure: m, end_tick: at, dx: 0.0, dy, end_dy: dy, text: String::new(), color: BLACK, flip: false, size: 0.0, bold: false, italic: false });
                        }
                    }
                }
                "bracket" | "dashes" => skip(&mut self.skipped, "line or bracket"),
                "other-direction" | "image" | "accordion-registration" | "harp-pedals" | "damp" | "damp-all" | "eyeglasses" | "string-mute" | "scordatura" | "principal-voice" | "percussion" | "staff-divide" | "symbol" => {
                    skip(&mut self.skipped, &x.name.replace('-', " "))
                }
                _ => {}
            }
        }
    }

    fn sound(&mut self, snd: &Node, m: usize) {
        if let Some(t) = snd.attr("tempo").and_then(|t| t.parse::<f64>().ok()).filter(|t| t.is_finite() && *t >= 1.0) {
            self.tempo[m] = Some(t.round().clamp(10.0, 400.0) as u32);
        }
    }

    fn barline(&mut self, b: &Node, m: usize) {
        let right = b.attr("location").unwrap_or("right") == "right";
        let style = b.text_of("bar-style").unwrap_or("");
        let repeat = b.child("repeat").and_then(|r| r.attr("direction"));
        if let Some(t) = b.child("repeat").and_then(|r| r.attr("times")).and_then(|t| t.parse::<u8>().ok()) {
            self.repeat_times[m] = t.clamp(2, 9);
        }
        if let Some(e) = b.child("ending") {
            if e.attr("type") == Some("start") {
                let nums = e.attr("number").unwrap_or("").trim();
                let txt = e.text.trim();
                let label = if !txt.is_empty() { txt.to_string() } else if nums.is_empty() { "1.".into() } else { format!("{}.", nums.replace(' ', "")) };
                // the label must carry the numbers for playback
                let label = if nums.is_empty() || label.chars().any(|c| c.is_ascii_digit()) { label } else { format!("{}.", nums) };
                self.endings[m] = Some(label);
            }
        }
        if right {
            let bl = match (repeat, style) {
                (Some("backward"), _) => Some(if self.barlines[m] == Barline::RepeatStart { Barline::RepeatBoth } else { Barline::RepeatEnd }),
                (_, "light-heavy") => Some(Barline::Final),
                (_, "light-light" | "heavy-heavy" | "heavy" | "heavy-light") => Some(Barline::Double),
                (_, "dashed" | "dotted") => Some(Barline::Dashed),
                (_, "none") => Some(Barline::Invisible),
                _ => None,
            };
            if let Some(bl) = bl {
                self.barlines[m] = bl;
            }
        } else {
            // a left bar line is the right one of the previous measure
            if repeat == Some("forward") {
                if m > 0 {
                    let p = &mut self.barlines[m - 1];
                    *p = if *p == Barline::RepeatEnd { Barline::RepeatBoth } else { Barline::RepeatStart };
                }
            } else if m > 0 && self.barlines[m - 1] == Barline::Single {
                match style {
                    "light-light" | "heavy-heavy" | "heavy-light" => self.barlines[m - 1] = Barline::Double,
                    "dashed" | "dotted" => self.barlines[m - 1] = Barline::Dashed,
                    _ => {}
                }
            }
        }
    }

    /// Dynamics, words, chord symbols and marks go on the note at their time (or a note close to it); without a
    /// note there they become text above the staff.
    fn attach_pending(&mut self, sc: &mut Score) {
        for p in std::mem::take(&mut self.pending) {
            let Some(evs) = sc.events.get_mut(p.staff).and_then(|st| st.get_mut(p.measure)) else { continue };
            let notes_first = !matches!(p.what, Attach::Dynamic(_));
            let target = evs
                .iter_mut()
                .filter(|e| !e.grace && (!notes_first || !e.is_rest()))
                .filter(|e| e.start.abs_diff(p.tick) <= TPQ / 4)
                .min_by_key(|e| (e.start.abs_diff(p.tick), e.voice));
            let target = target.filter(|e| match &p.what {
                Attach::Dynamic(_) => e.dynamic.is_none(),
                Attach::Words(_) => e.expression.is_none(),
                Attach::Chord(_) => e.chord_symbol.is_none(),
                Attach::Mark(_) => true,
            });
            match (target, p.what) {
                (Some(e), what) => match what {
                    Attach::Dynamic(d) => e.dynamic = Some(d),
                    Attach::Words(w) => e.expression = Some(w),
                    Attach::Chord(c) => e.chord_symbol = Some(c),
                    Attach::Mark(mk) => {
                        if !e.marks.contains(&mk) {
                            e.marks.push(mk);
                        }
                    }
                },
                (_, Attach::Mark(_)) => skip(&mut self.skipped, "segno, coda or pedal mark away from a note"),
                (_, what) => {
                    let (text, dy, italic, bold) = match what {
                        Attach::Dynamic(d) => (d, 7.0, true, true),
                        Attach::Words(w) => (w, -3.0, true, false),
                        Attach::Chord(c) => (c, -3.0, false, false),
                        Attach::Mark(_) => unreachable!(),
                    };
                    sc.graphics.push(Graphic { kind: GraphicKind::Text, staff: p.staff, measure: p.measure, tick: p.tick, end_measure: p.measure, end_tick: p.tick, dx: 0.0, dy, end_dy: dy, text, color: BLACK, flip: false, size: 1.8, bold, italic });
                }
            }
        }
        for _ in std::mem::take(&mut self.spans) {
            skip(&mut self.skipped, "hairpin or 8va line without an end");
        }
    }

    fn finish_slurs(&mut self, sc: &mut Score) {
        for (o, em, et, esteps) in std::mem::take(&mut self.slur_ends) {
            let clef = sc.staves[o.staff].clef_at(o.measure);
            let mid = clef.middle_step();
            let pos = |st: Option<(i32, i32)>, below: bool| -> f32 {
                let step = st.map(|(lo, hi)| if below { lo } else { hi }).unwrap_or(mid);
                (mid + 4 - step) as f32 * 0.5 + if below { 1.2 } else { -1.2 }
            };
            sc.graphics.push(Graphic {
                kind: GraphicKind::Slur,
                staff: o.staff,
                measure: o.measure,
                tick: o.tick,
                end_measure: em,
                end_tick: et,
                dx: 0.0,
                dy: pos(o.step, o.below),
                end_dy: pos(esteps, o.below),
                text: String::new(),
                color: BLACK,
                flip: o.below,
                size: 0.0,
                bold: false,
                italic: false,
            });
        }
        for _ in std::mem::take(&mut self.slurs) {
            skip(&mut self.skipped, "slur without an end");
        }
    }
}

#[cfg(test)]
#[path = "musicxml_tests.rs"]
mod tests;
