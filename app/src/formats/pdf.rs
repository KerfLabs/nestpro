//! Vector PDF drawings (JOB-1).
//!
//! Rules (see MECHANICS.md, "PDF import"):
//! - PD1: page units are points; 1 pt = 25.4/72 mm, at a scale of 1:1 unless the
//!   user sets another scale (PD2).
//! - PD3: all pages go side by side, left to right, into one drawing; each page
//!   starts where the previous page's media box ends.
//! - PD4: layers come from the line colour by default; PDF layers (optional
//!   content) or a single layer can be chosen instead.
//! - PD5: stroked paths are the geometry; shapes that are only filled are left
//!   out (white fills always), clipping paths and images are not drawn.
//! - PD6: curves become straight segments within the arc smoothness (0.1 mm).
//! - PD7: text is kept as text elements (simple fonts only).
//! - PD8: protected (encrypted) and damaged files are refused in plain words.
//!
//! The file is read by our own parser: classic and compressed cross-reference
//! tables, object streams, Flate/LZW/ASCII85/ASCIIHex/RunLength filters with PNG
//! predictors; when the cross-reference data is broken the objects are found by
//! scanning the file.

use super::{Meta, ReadPart};
use crate::dxf::{ARC_CHORD_ERR, Drawing, Piece};
use crate::nest::P;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// How several pages become parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pages {
    /// PD3: pages side by side, left to right, in one drawing (original default).
    SideBySide,
    /// One part per page (pages with no lines are skipped).
    Separate,
}

/// What the drawing's layers are made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layers {
    /// PD4: one layer per line colour (original default).
    Colour,
    /// The PDF's own layers (optional content groups), by name.
    PdfLayers,
    /// Everything on one layer "0".
    None,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// PD2: real size = drawing size × scale (original default 1, i.e. 1:1).
    pub scale: f64,
    pub pages: Pages,
    pub layers: Layers,
    /// PD5: also read shapes that are only filled (not white). Off by default.
    pub fills: bool,
    /// PD7: keep the drawing's text as text elements.
    pub text: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { scale: 1.0, pages: Pages::SideBySide, layers: Layers::Colour, fills: false, text: true }
    }
}

/// PD1: millimetres per PDF point.
pub const MM_PER_PT: f64 = 25.4 / 72.0;

/// Read a PDF's vector drawing.
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let start = find(bytes, b"%PDF-", 0).filter(|&p| p < 1024).ok_or("This is not a PDF file (it has no PDF header).")?;
    let doc = Doc::open(bytes, start)?;
    if doc.trailer.contains_key("Encrypt") {
        // PD8
        return Err("This PDF is protected (encrypted), so its drawing can't be read. \
                    Save or print it again without protection, or ask for a DXF."
            .into());
    }
    let pages = doc.pages()?;
    doc.refusal()?;
    if pages.is_empty() {
        return Err("No pages were found in this PDF — the file may be damaged. Try saving it again from a PDF viewer.".into());
    }
    let k = MM_PER_PT * if opts.scale > 0.0 && opts.scale.is_finite() { opts.scale } else { 1.0 };
    let mut parts: Vec<ReadPart> = vec![];
    let mut whole = Drawing::default();
    let mut skipped = Skipped::default();
    let mut x_off = 0.0;
    // One budget of operations and points for the whole file, not per page
    // (PD8): many pages sharing one drawing can't add up past the limits.
    let (mut budget, mut points, mut read) = (None, 0usize, 0usize);
    for (n, page) in pages.iter().enumerate() {
        // PD1: a page may set its own unit (UserUnit, in points; default 1),
        // which scales everything on it, its size included.
        let unit = match page.dict.get("UserUnit").map(|o| doc.resolve(o)) {
            None => 1.0,
            Some(o) => match o.num().filter(|u| u.is_finite() && *u > 0.0) {
                Some(u) => u,
                None => return Err("A page of this PDF gives a unit size that can't be read, so its drawing can't be measured; nothing was imported. Save it again from a PDF viewer, or export DXF.".into()),
            },
        };
        let k = k * unit;
        let mut p = Painter::new(&doc, opts, k, page.media, x_off);
        if let Some(b) = budget {
            p.budget = b;
        }
        p.points = points;
        p.read_bytes = read;
        let content = doc.page_content(&page.dict)?;
        if content.len() > p.budget {
            p.over = true;
        } else {
            p.run(&content, &page.resources, 0);
        }
        budget = Some(p.budget);
        points = p.points;
        read = p.read_bytes;
        if p.bad_transform {
            return Err("A position change in this PDF can't be read, so the drawing after it can't be placed; nothing was imported. Save it again from a PDF viewer, or export DXF.".into());
        }
        if p.damaged {
            return Err("A drawing inside this PDF (a form) can't be read; nothing was imported, so no contour goes missing. Save it again from a PDF viewer, or export DXF.".into());
        }
        if p.over {
            return Err("This PDF drawing is bigger or deeper than the import can take whole (too many drawing operations, points or nested forms). \
                        Nothing was imported, so no contour goes missing. Export the part as DXF, or simplify the PDF."
                .into());
        }
        x_off += (page.media[2] - page.media[0]).abs() * k;
        skipped.add(&p.skipped);
        match opts.pages {
            Pages::SideBySide => {
                whole.pieces.append(&mut p.out.pieces);
                whole.texts.append(&mut p.out.texts);
            }
            Pages::Separate => {
                if p.out.pieces.is_empty() {
                    continue;
                }
                let mut d = p.out;
                d.report.entities = d.pieces.len() + d.texts.len();
                d.report.skipped = p.skipped.list();
                // Shift the page back to its own origin.
                let dx = x_off - (page.media[2] - page.media[0]).abs() * k;
                for pc in &mut d.pieces {
                    for q in &mut pc.pts {
                        q[0] -= dx;
                    }
                }
                for t in &mut d.texts {
                    t.2[0] -= dx;
                }
                let name = if pages.len() > 1 { Some(format!("page {}", n + 1)) } else { None };
                parts.push(ReadPart { drawing: d, meta: Meta { name, ..Meta::default() }, mark_layers: vec![] });
            }
        }
    }
    doc.refusal()?;
    let none = "No lines or curves were found in this PDF. If it is a scanned drawing (a picture), \
                it can't be imported — ask for a DXF or a PDF exported from CAD.";
    match opts.pages {
        Pages::SideBySide => {
            if whole.pieces.is_empty() {
                return Err(none.into());
            }
            whole.report.entities = whole.pieces.len() + whole.texts.len();
            whole.report.skipped = skipped.list();
            Ok(vec![ReadPart { drawing: whole, meta: Meta::default(), mark_layers: vec![] }])
        }
        Pages::Separate => {
            if parts.is_empty() {
                return Err(none.into());
            }
            Ok(parts)
        }
    }
}

// ---------------------------------------------------------------- objects

type Dict = HashMap<String, Obj>;

#[derive(Debug, Clone)]
enum Obj {
    Null,
    Bool(#[allow(dead_code)] bool),
    Num(f64),
    Str(Vec<u8>),
    Name(String),
    Arr(Vec<Obj>),
    Dict(Dict),
    Ref(u32),
    Stream(Rc<Dict>, Rc<Vec<u8>>),
    /// A bare keyword (content-stream operator, `obj`, `R`…).
    Kw(Vec<u8>),
}

impl Obj {
    fn num(&self) -> Option<f64> {
        match self {
            Obj::Num(v) => Some(*v),
            _ => None,
        }
    }
    fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(s) => Some(s),
            _ => None,
        }
    }
    fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) => Some(d),
            Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() || needle.is_empty() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}
fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
    /// Objects nested deeper than the lexer reads: what follows was not read.
    too_deep: bool,
}

impl<'a> Lexer<'a> {
    fn new(b: &'a [u8], i: usize) -> Self {
        Lexer { b, i, too_deep: false }
    }
    fn skip_ws(&mut self) {
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if is_ws(c) {
                self.i += 1;
            } else if c == b'%' {
                while self.i < self.b.len() && self.b[self.i] != b'\n' && self.b[self.i] != b'\r' {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }
    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }
    fn peek(&self) -> u8 {
        self.b.get(self.i).copied().unwrap_or(0)
    }
    fn regular(&mut self) -> &'a [u8] {
        let s = self.i;
        while self.i < self.b.len() && !is_ws(self.b[self.i]) && !is_delim(self.b[self.i]) {
            self.i += 1;
        }
        &self.b[s..self.i]
    }
    fn number(tok: &[u8]) -> Option<f64> {
        if tok.is_empty() || !tok.iter().all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')) {
            return None;
        }
        // Tolerate doubled signs ("--5") some writers produce.
        let t = std::str::from_utf8(tok).ok()?;
        let t = t.trim_start_matches('+');
        let neg = t.starts_with('-');
        let t = t.trim_start_matches('-');
        let v: f64 = if t.is_empty() || t == "." {
            0.0
        } else {
            // "1.2.3" and similar junk is no number (never its leading part).
            t.parse().ok()?
        };
        Some(if neg { -v } else { v })
    }
    /// One object; `refs` turns "n g R" into references (file syntax, not content).
    fn obj(&mut self, refs: bool) -> Option<Obj> {
        self.obj_depth(refs, 0)
    }
    fn obj_depth(&mut self, refs: bool, depth: u32) -> Option<Obj> {
        self.skip_ws();
        if self.eof() {
            return None;
        }
        if depth > 64 {
            self.i = self.b.len();
            self.too_deep = true;
            return None;
        }
        let c = self.peek();
        match c {
            b'[' => {
                self.i += 1;
                let mut v = vec![];
                loop {
                    self.skip_ws();
                    if self.eof() {
                        break;
                    }
                    if self.peek() == b']' {
                        self.i += 1;
                        break;
                    }
                    // No real file has an array this long: stop reading (the
                    // content is then refused like one nested too deep, PD8).
                    if v.len() >= MAX_ITEMS {
                        self.i = self.b.len();
                        self.too_deep = true;
                        return None;
                    }
                    match self.obj_depth(refs, depth + 1) {
                        Some(o) => v.push(o),
                        None => break,
                    }
                }
                Some(Obj::Arr(v))
            }
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = Dict::new();
                loop {
                    self.skip_ws();
                    if self.eof() {
                        break;
                    }
                    if self.peek() == b'>' {
                        self.i += if self.b.get(self.i + 1) == Some(&b'>') { 2 } else { 1 };
                        break;
                    }
                    if d.len() >= MAX_ITEMS {
                        self.i = self.b.len();
                        self.too_deep = true;
                        return None;
                    }
                    let key = match self.obj_depth(refs, depth + 1) {
                        Some(Obj::Name(k)) => k,
                        Some(_) => continue,
                        None => break,
                    };
                    self.skip_ws();
                    if self.peek() == b'>' {
                        d.insert(key, Obj::Null);
                        continue;
                    }
                    match self.obj_depth(refs, depth + 1) {
                        Some(v) => {
                            d.insert(key, v);
                        }
                        None => break,
                    }
                }
                Some(Obj::Dict(d))
            }
            b'<' => {
                self.i += 1;
                let mut out = vec![];
                let mut hi: Option<u8> = None;
                while self.i < self.b.len() && self.b[self.i] != b'>' {
                    let h = match self.b[self.i] {
                        x @ b'0'..=b'9' => Some(x - b'0'),
                        x @ b'a'..=b'f' => Some(x - b'a' + 10),
                        x @ b'A'..=b'F' => Some(x - b'A' + 10),
                        _ => None,
                    };
                    if let Some(h) = h {
                        match hi.take() {
                            Some(x) => out.push(x << 4 | h),
                            None => hi = Some(h),
                        }
                    }
                    self.i += 1;
                }
                if let Some(x) = hi {
                    out.push(x << 4);
                }
                self.i += 1;
                Some(Obj::Str(out))
            }
            b'(' => {
                self.i += 1;
                let mut out = vec![];
                let mut nest = 0;
                while self.i < self.b.len() {
                    let ch = self.b[self.i];
                    self.i += 1;
                    match ch {
                        b'\\' => {
                            let Some(&e) = self.b.get(self.i) else { break };
                            self.i += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b'r' => out.push(b'\r'),
                                b't' => out.push(b'\t'),
                                b'b' => out.push(8),
                                b'f' => out.push(12),
                                b'0'..=b'7' => {
                                    let mut v = (e - b'0') as u32;
                                    for _ in 0..2 {
                                        match self.b.get(self.i) {
                                            Some(&d @ b'0'..=b'7') => {
                                                v = v * 8 + (d - b'0') as u32;
                                                self.i += 1;
                                            }
                                            _ => break,
                                        }
                                    }
                                    out.push(v as u8);
                                }
                                b'\r' => {
                                    if self.b.get(self.i) == Some(&b'\n') {
                                        self.i += 1;
                                    }
                                }
                                b'\n' => {}
                                other => out.push(other),
                            }
                        }
                        b'(' => {
                            nest += 1;
                            out.push(ch);
                        }
                        b')' => {
                            if nest == 0 {
                                break;
                            }
                            nest -= 1;
                            out.push(ch);
                        }
                        _ => out.push(ch),
                    }
                }
                Some(Obj::Str(out))
            }
            b'/' => {
                self.i += 1;
                let raw = self.regular();
                let mut s = Vec::with_capacity(raw.len());
                let mut k = 0;
                while k < raw.len() {
                    if raw[k] == b'#'
                        && k + 2 < raw.len()
                        && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&raw[k + 1..k + 3]).unwrap_or("x"), 16)
                    {
                        s.push(v);
                        k += 3;
                        continue;
                    }
                    s.push(raw[k]);
                    k += 1;
                }
                Some(Obj::Name(String::from_utf8_lossy(&s).into_owned()))
            }
            b']' | b'>' | b')' | b'{' | b'}' => {
                self.i += 1;
                Some(Obj::Kw(vec![c]))
            }
            _ => {
                let tok = self.regular();
                if tok.is_empty() {
                    self.i += 1;
                    return Some(Obj::Kw(vec![c]));
                }
                // Looks like a number but isn't one ("200.0.5"): an unreadable
                // operand, so the command it belongs to sees a damaged number.
                if Self::number(tok).is_none() && tok.iter().all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')) {
                    return Some(Obj::Null);
                }
                if let Some(v) = Self::number(tok) {
                    if refs && tok.iter().all(|c| c.is_ascii_digit()) {
                        let save = self.i;
                        self.skip_ws();
                        let g = self.regular();
                        if !g.is_empty() && g.iter().all(|c| c.is_ascii_digit()) {
                            self.skip_ws();
                            if self.peek() == b'R' && self.b.get(self.i + 1).is_none_or(|&n| is_ws(n) || is_delim(n)) {
                                self.i += 1;
                                return Some(Obj::Ref(v as u32));
                            }
                        }
                        self.i = save;
                    }
                    return Some(Obj::Num(v));
                }
                Some(match tok {
                    b"true" => Obj::Bool(true),
                    b"false" => Obj::Bool(false),
                    b"null" => Obj::Null,
                    _ => Obj::Kw(tok.to_vec()),
                })
            }
        }
    }
}

// ---------------------------------------------------------------- filters

fn decode_stream(dict: &Dict, raw: &[u8], doc: &Doc) -> Result<Vec<u8>, String> {
    let filters: Vec<String> = match doc.resolve(dict.get("Filter").unwrap_or(&Obj::Null)) {
        Obj::Name(n) => vec![n],
        Obj::Arr(a) => a.iter().filter_map(|o| doc.resolve(o).name().map(str::to_string)).collect(),
        _ => vec![],
    };
    let parms: Vec<Obj> = match doc.resolve(dict.get("DecodeParms").or(dict.get("DP")).unwrap_or(&Obj::Null)) {
        Obj::Arr(a) => a.iter().map(|o| doc.resolve(o)).collect(),
        o => vec![o],
    };
    let mut data = raw.to_vec();
    for (k, f) in filters.iter().enumerate() {
        let p = parms.get(k).cloned().unwrap_or(Obj::Null);
        data = match f.as_str() {
            "FlateDecode" | "Fl" => predict(inflate(&data)?, &p, doc),
            "LZWDecode" | "LZW" => {
                let early = p.dict().and_then(|d| d.get("EarlyChange")).and_then(Obj::num).unwrap_or(1.0) != 0.0;
                predict(lzw(&data, early)?, &p, doc)
            }
            "ASCIIHexDecode" | "AHx" => ascii_hex(&data)?,
            "ASCII85Decode" | "A85" => ascii85(&data)?,
            "RunLengthDecode" | "RL" => run_length(&data)?,
            other => return Err(format!("unsupported filter {other}")),
        };
        // Every byte unpacked anywhere in the file counts toward one budget
        // (PD8): a small file can't keep the import unpacking for minutes.
        let total = doc.decoded.get().saturating_add(data.len());
        doc.decoded.set(total);
        if total > MAX_DECODED {
            doc.too_big.set(true);
            return Err("the file unpacks to more than the import reads".into());
        }
    }
    Ok(data)
}

/// Largest a decoded stream may grow (PD8): a small damaged or hostile file
/// can't make the import claim gigabytes.
const STREAM_MAX: usize = 256 << 20;

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    use miniz_oxide::inflate::{decompress_to_vec_with_limit as decompress_to_vec_l, decompress_to_vec_zlib_with_limit as decompress_to_vec_zlib_l};
    let decompress_to_vec_zlib = |d: &[u8]| decompress_to_vec_zlib_l(d, STREAM_MAX);
    let decompress_to_vec = |d: &[u8]| decompress_to_vec_l(d, STREAM_MAX);
    if let Ok(v) = decompress_to_vec_zlib(data) {
        return Ok(v);
    }
    // Damaged checksum or a missing zlib header: read the raw deflate data.
    if data.len() > 2
        && let Ok(v) = decompress_to_vec(&data[2..])
    {
        return Ok(v);
    }
    decompress_to_vec(data).map_err(|_| "a compressed stream is damaged".to_string())
}

fn predict(data: Vec<u8>, parms: &Obj, doc: &Doc) -> Vec<u8> {
    let Some(d) = parms.dict() else { return data };
    let get = |k: &str, def: f64| d.get(k).map(|o| doc.resolve(o)).and_then(|o| o.num()).unwrap_or(def);
    let pred = get("Predictor", 1.0) as u32;
    if pred < 2 {
        return data;
    }
    let colors = get("Colors", 1.0).clamp(1.0, 64.0) as usize;
    let bpc = get("BitsPerComponent", 8.0).clamp(1.0, 16.0) as usize;
    let cols = get("Columns", 1.0).clamp(1.0, 1e9) as usize;
    let bpp = (colors * bpc).div_ceil(8).max(1);
    // A row can't be longer than the data it is decoded from; a damaged size
    // leaves the data as it is instead of asking for a huge buffer.
    let Some(row) = (colors * bpc).checked_mul(cols).map(|b| b.div_ceil(8)).filter(|&r| r > 0 && r <= data.len()) else { return data };
    if pred == 2 {
        // TIFF predictor (8-bit components only).
        let mut out = data;
        if bpc == 8 {
            for r in out.chunks_mut(row) {
                for i in bpp..r.len() {
                    r[i] = r[i].wrapping_add(r[i - bpp]);
                }
            }
        }
        return out;
    }
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        if chunk.is_empty() {
            break;
        }
        let ft = chunk[0];
        let mut cur: Vec<u8> = chunk[1..].to_vec();
        cur.resize(row, 0);
        for i in 0..row {
            let a = if i >= bpp { cur[i - bpp] as i32 } else { 0 };
            let b = prev[i] as i32;
            let c = if i >= bpp { prev[i - bpp] as i32 } else { 0 };
            let x = cur[i] as i32;
            cur[i] = match ft {
                1 => x + a,
                2 => x + b,
                3 => x + (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    x + if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => x,
            } as u8;
        }
        out.extend_from_slice(&cur);
        prev = cur;
    }
    out
}

/// LZW decoding; a code the table can't have means the stream is damaged
/// (an error, so the page is refused rather than read in part, PD8).
fn lzw(data: &[u8], early: bool) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    let mut table: Vec<Vec<u8>> = (0..256).map(|i| vec![i as u8]).collect();
    table.push(vec![]);
    table.push(vec![]);
    let mut width = 9;
    let (mut buf, mut bits) = (0u32, 0u32);
    let mut prev: Option<Vec<u8>> = None;
    for &byte in data {
        buf = (buf << 8) | byte as u32;
        bits += 8;
        while bits >= width {
            let code = ((buf >> (bits - width)) & ((1 << width) - 1)) as usize;
            bits -= width;
            if code == 256 {
                table.truncate(258);
                width = 9;
                prev = None;
                continue;
            }
            if code == 257 {
                return Ok(out);
            }
            let entry = if code < table.len() && !(prev.is_none() && code > 257) {
                table[code].clone()
            } else if let (Some(p), true) = (&prev, code == table.len()) {
                let mut e = p.clone();
                e.push(p[0]);
                e
            } else {
                return Err("a compressed (LZW) stream is damaged".into());
            };
            out.extend_from_slice(&entry);
            if out.len() > STREAM_MAX {
                return Err("a compressed stream is bigger than the import reads".into());
            }
            // The code table holds at most 4096 entries (12-bit codes).
            if let Some(p) = prev.take().filter(|_| table.len() < 4096) {
                let mut n = p;
                n.push(entry[0]);
                table.push(n);
            }
            prev = Some(entry);
            let lim = table.len() + if early { 1 } else { 0 };
            if lim >= (1 << width) && width < 12 {
                width += 1;
            }
        }
    }
    // No end code: the stream was cut off and what followed it is missing.
    Err("a compressed (LZW) stream is cut short".into())
}

/// ASCIIHex decoding; the data must end with `>` (else it was cut short).
fn ascii_hex(data: &[u8]) -> Result<Vec<u8>, String> {
    let end = data.iter().position(|&c| c == b'>').ok_or("a hex-coded stream is cut short")?;
    // Only hex digits and white space before the end: anything else is damage.
    if data[..end].iter().any(|&c| !c.is_ascii_hexdigit() && !is_ws(c)) {
        return Err("a hex-coded stream is damaged".into());
    }
    let mut s = vec![b'<'];
    s.extend_from_slice(&data[..end]);
    s.push(b'>');
    match Lexer::new(&s, 0).obj(false) {
        Some(Obj::Str(v)) => Ok(v),
        _ => Err("a hex-coded stream is damaged".into()),
    }
}

/// ASCII85 decoding; the data must end with `~>` (else it was cut short).
fn ascii85(data: &[u8]) -> Result<Vec<u8>, String> {
    // Strictly: data characters `!`–`u`, `z` for four zero bytes between
    // groups, white space, and the end `~>` — anything else (a stray `~`, a
    // `z` inside a group, a lone last character) is damage, never an early end.
    let damaged = || "an ASCII85-coded stream is damaged".to_string();
    let mut out = vec![];
    let mut group: Vec<u32> = vec![];
    let mut i = if data.starts_with(b"<~") { 2 } else { 0 };
    let mut ended = false;
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            b'~' => {
                if data.get(i) != Some(&b'>') {
                    return Err(damaged());
                }
                ended = true;
                break;
            }
            b'z' if group.is_empty() => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group.push((c - b'!') as u32);
                if group.len() == 5 {
                    let v = group.iter().fold(0u64, |a, &d| a * 85 + d as u64);
                    if v > u32::MAX as u64 {
                        return Err(damaged());
                    }
                    out.extend_from_slice(&(v as u32).to_be_bytes());
                    group.clear();
                }
            }
            c if is_ws(c) => {}
            _ => return Err(damaged()),
        }
        if out.len() > STREAM_MAX {
            return Err("a compressed stream is bigger than the import reads".into());
        }
    }
    if !ended {
        return Err("an ASCII85-coded stream is cut short".into());
    }
    match group.len() {
        0 => {}
        1 => return Err(damaged()),
        n => {
            while group.len() < 5 {
                group.push(84);
            }
            let v = group.iter().fold(0u64, |a, &d| a * 85 + d as u64);
            if v > u32::MAX as u64 {
                return Err(damaged());
            }
            out.extend_from_slice(&(v as u32).to_be_bytes()[..n - 1]);
            if out.len() > STREAM_MAX {
                return Err("a compressed stream is bigger than the import reads".into());
            }
        }
    }
    Ok(out)
}

/// RunLength decoding; a run cut short means the stream is damaged, and the
/// output is held to `STREAM_MAX` (errors refuse the page, PD8).
fn run_length(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    let mut i = 0;
    while i < data.len() {
        let n = data[i] as usize;
        i += 1;
        if n == 128 {
            return Ok(out);
        }
        let grow = if n < 128 { n + 1 } else { 257 - n };
        if out.len().saturating_add(grow) > STREAM_MAX {
            return Err("a compressed stream is bigger than the import reads".into());
        }
        if n < 128 {
            let run = data.get(i..i + n + 1).ok_or("a compressed (RunLength) stream is cut short")?;
            out.extend_from_slice(run);
            i += n + 1;
        } else {
            let b = *data.get(i).ok_or("a compressed (RunLength) stream is cut short")?;
            out.extend(std::iter::repeat_n(b, grow));
            i += 1;
        }
    }
    // No end marker: the stream was cut off and what followed it is missing.
    Err("a compressed (RunLength) stream is cut short".into())
}

// ---------------------------------------------------------------- document

#[derive(Clone, Copy, Debug)]
enum Loc {
    At(usize),
    InStream(u32, usize),
}

/// An object stream once decoded: its bytes, its index (object number, offset)
/// in file order, and each object number's first offset for direct look-up.
struct ObjStm(Vec<u8>, Vec<(u32, usize)>, HashMap<u32, usize>);

struct Doc<'a> {
    data: &'a [u8],
    /// Where `%PDF-` starts; offsets in the file are counted from here.
    base: usize,
    xref: HashMap<u32, Loc>,
    scanned: RefCell<Option<Rc<HashMap<u32, Loc>>>>,
    cache: RefCell<HashMap<u32, Obj>>,
    objstm: RefCell<HashMap<u32, Rc<ObjStm>>>,
    busy: RefCell<HashSet<u32>>,
    trailer: Dict,
    /// Forms decoded once and kept (a form drawn many times isn't unpacked again).
    forms: RefCell<HashMap<u32, Rc<Vec<u8>>>>,
    /// Dictionaries looked things up in, resolved once and shared (a resource
    /// table used by every form call isn't copied each time).
    dicts: RefCell<HashMap<u32, Rc<Dict>>>,
    /// Each form's own resources, resolved once.
    form_res: RefCell<HashMap<u32, Rc<Dict>>>,
    /// Fonts built once (a font used by many texts isn't read again).
    fonts: RefCell<HashMap<u32, Font>>,
    /// PDF layer names worked out once per referenced layer.
    oc_names: RefCell<HashMap<u32, Option<String>>>,
    /// Bytes of decoded object streams kept, and whether that went over budget.
    objstm_bytes: std::cell::Cell<usize>,
    too_big: std::cell::Cell<bool>,
    /// An object of the file itself (not drawing content) held more entries or
    /// was nested deeper than the lexer reads: the file is refused (PD8).
    too_complex: std::cell::Cell<bool>,
    /// Bytes unpacked so far from any stream.
    decoded: std::cell::Cell<usize>,
}

/// Most bytes of decoded object streams one PDF may keep (PD8).
const MAX_OBJSTM_BYTES: usize = 256 << 20;

/// Most bytes one PDF may unpack altogether (PD8).
const MAX_DECODED: usize = 1 << 30;

/// Most entries one array or dictionary may hold (PD8).
const MAX_ITEMS: usize = 1_000_000;

struct Page {
    dict: Dict,
    /// Shared by the pages that inherit them (not a copy per page).
    resources: Rc<Dict>,
    media: [f64; 4],
}

/// Roughly how many bytes an object holds (strings, names and their nesting).
fn obj_size(o: &Obj, depth: u32) -> usize {
    if depth > 32 {
        return 0;
    }
    8 + match o {
        Obj::Str(v) | Obj::Kw(v) => v.len(),
        Obj::Name(n) => n.len(),
        Obj::Arr(a) => a.iter().map(|x| obj_size(x, depth + 1)).sum(),
        Obj::Dict(d) => d.iter().map(|(k, v)| k.len() + obj_size(v, depth + 1)).sum(),
        Obj::Stream(d, _) => d.iter().map(|(k, v)| k.len() + obj_size(v, depth + 1)).sum(),
        _ => 0,
    }
}

/// Most bytes of page and resource dictionaries one PDF may keep (PD8).
const MAX_PAGE_BYTES: usize = 256 << 20;

impl<'a> Doc<'a> {
    fn open(data: &'a [u8], base: usize) -> Result<Doc<'a>, String> {
        let mut doc = Doc {
            data,
            base,
            xref: HashMap::new(),
            scanned: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
            objstm: RefCell::new(HashMap::new()),
            busy: RefCell::new(HashSet::new()),
            trailer: Dict::new(),
            forms: RefCell::new(HashMap::new()),
            dicts: RefCell::new(HashMap::new()),
            form_res: RefCell::new(HashMap::new()),
            fonts: RefCell::new(HashMap::new()),
            oc_names: RefCell::new(HashMap::new()),
            objstm_bytes: std::cell::Cell::new(0),
            too_big: std::cell::Cell::new(false),
            too_complex: std::cell::Cell::new(false),
            decoded: std::cell::Cell::new(0),
        };
        let ok = doc.load_xref();
        if !ok || !doc.trailer.contains_key("Root") {
            // Broken cross-reference data: find the objects by scanning.
            doc.xref.clear();
            let scan = doc.scan();
            doc.trailer = doc.scan_trailer(&scan);
        }
        doc.refusal()?;
        if !doc.trailer.contains_key("Root") {
            return Err("This PDF is damaged: its page list can't be found. Try saving it again from a PDF viewer.".into());
        }
        Ok(doc)
    }

    /// Refuse the whole file once any of its limits was passed (PD8): no page
    /// comes in quietly missing what was left unread.
    fn refusal(&self) -> Result<(), String> {
        if self.too_big.get() {
            return Err("This PDF packs more data than the import reads (over 256 MB of object streams or 1 GB unpacked); nothing was imported. Save it again from a PDF viewer, or export DXF.".into());
        }
        if self.too_complex.get() {
            return Err("This PDF holds a list longer than the import reads (over a million entries) or nested deeper than 64 levels; nothing was imported. Save it again from a PDF viewer, or export DXF.".into());
        }
        Ok(())
    }

    /// Note a lexer that stopped at a limit; true when it did.
    fn stopped(&self, lx: &Lexer) -> bool {
        if lx.too_deep {
            self.too_complex.set(true);
        }
        lx.too_deep
    }

    fn load_xref(&mut self) -> bool {
        let tail_from = self.data.len().saturating_sub(4096);
        let Some(sx) = rfind(&self.data[tail_from..], b"startxref").map(|p| p + tail_from) else { return false };
        let mut lx = Lexer::new(self.data, sx + 9);
        let Some(Obj::Num(off)) = lx.obj(false) else { return false };
        let mut todo = vec![off as usize];
        let mut seen = HashSet::new();
        let mut first = true;
        while let Some(off) = todo.pop() {
            if !seen.insert(off) || seen.len() > 64 {
                continue;
            }
            let pos = self.base + off;
            if pos >= self.data.len() {
                return false;
            }
            let mut lx = Lexer::new(self.data, pos);
            lx.skip_ws();
            let trailer: Dict;
            if self.data[lx.i..].starts_with(b"xref") {
                lx.i += 4;
                loop {
                    lx.skip_ws();
                    if lx.eof() || self.data[lx.i..].starts_with(b"trailer") {
                        break;
                    }
                    let (Some(Obj::Num(s)), Some(Obj::Num(n))) = (lx.obj(false), lx.obj(false)) else { return false };
                    for e in 0..n as u32 {
                        let (Some(Obj::Num(o)), Some(Obj::Num(_g)), Some(Obj::Kw(t))) = (lx.obj(false), lx.obj(false), lx.obj(false)) else {
                            return false;
                        };
                        let num = s as u32 + e;
                        if t == b"n" && o > 0.0 {
                            self.xref.entry(num).or_insert(Loc::At(self.base + o as usize));
                        }
                    }
                }
                lx.i += 7;
                let t = lx.obj(true);
                if self.stopped(&lx) {
                    return false;
                }
                let Some(Obj::Dict(t)) = t else { return false };
                trailer = t;
                if let Some(Obj::Num(x)) = trailer.get("XRefStm") {
                    if !self.read_xref_stream(*x as usize) {
                        return false;
                    }
                }
            } else {
                let Some((_, Obj::Stream(d, _))) = self.parse_at(pos) else { return false };
                if !self.read_xref_stream(off) {
                    return false;
                }
                trailer = (*d).clone();
            }
            for (k, v) in &trailer {
                if first || !self.trailer.contains_key(k) {
                    self.trailer.entry(k.clone()).or_insert(v.clone());
                }
            }
            first = false;
            if let Some(Obj::Num(p)) = trailer.get("Prev") {
                todo.push(*p as usize);
            }
        }
        !self.xref.is_empty()
    }

    fn read_xref_stream(&mut self, off: usize) -> bool {
        let Some((_, Obj::Stream(d, raw))) = self.parse_at(self.base + off) else { return false };
        let Ok(data) = decode_stream(&d, &raw, self) else { return false };
        let w: Vec<usize> = match d.get("W") {
            Some(Obj::Arr(a)) => a.iter().map(|o| o.num().unwrap_or(0.0).max(0.0) as usize).collect(),
            _ => return false,
        };
        if w.len() < 3 || w.iter().any(|&x| x > 8) {
            return false;
        }
        let size = d.get("Size").and_then(Obj::num).unwrap_or(0.0) as u32;
        let index: Vec<u32> = match d.get("Index") {
            Some(Obj::Arr(a)) => a.iter().map(|o| o.num().unwrap_or(0.0) as u32).collect(),
            _ => vec![0, size],
        };
        let rec = w[0] + w[1] + w[2];
        if rec == 0 {
            return false;
        }
        let field = |b: &[u8]| b.iter().fold(0u64, |a, &x| (a << 8) | x as u64);
        let mut p = 0;
        for pair in index.chunks(2) {
            let (s, n) = (pair[0], *pair.get(1).unwrap_or(&0));
            for e in 0..n {
                if p + rec > data.len() {
                    return true;
                }
                let r = &data[p..p + rec];
                p += rec;
                let t = if w[0] == 0 { 1 } else { field(&r[..w[0]]) };
                let f2 = field(&r[w[0]..w[0] + w[1]]);
                let f3 = field(&r[w[0] + w[1]..]);
                let num = s + e;
                match t {
                    1 => {
                        self.xref.entry(num).or_insert(Loc::At(self.base + f2 as usize));
                    }
                    2 => {
                        self.xref.entry(num).or_insert(Loc::InStream(f2 as u32, f3 as usize));
                    }
                    _ => {}
                }
            }
        }
        true
    }

    /// Parse "n g obj … endobj" at `pos`; returns the object number and object.
    fn parse_at(&self, pos: usize) -> Option<(u32, Obj)> {
        let mut lx = Lexer::new(self.data, pos);
        let r = self.parse_lx(&mut lx);
        if self.stopped(&lx) {
            return None;
        }
        r
    }

    fn parse_lx(&self, lx: &mut Lexer<'a>) -> Option<(u32, Obj)> {
        let Some(Obj::Num(n)) = lx.obj(false) else { return None };
        let Some(Obj::Num(_)) = lx.obj(false) else { return None };
        match lx.obj(false) {
            Some(Obj::Kw(k)) if k == b"obj" => {}
            _ => return None,
        }
        let o = lx.obj(true)?;
        lx.skip_ws();
        if let Obj::Dict(d) = &o
            && self.data[lx.i..].starts_with(b"stream")
        {
            let mut s = lx.i + 6;
            if self.data.get(s) == Some(&b'\r') {
                s += 1;
            }
            if self.data.get(s) == Some(&b'\n') {
                s += 1;
            }
            let len = match d.get("Length") {
                Some(Obj::Num(l)) => Some(*l as usize),
                Some(Obj::Ref(r)) if *r != n as u32 => self.get(*r).num().map(|l| l as usize),
                _ => None,
            };
            let ok_len = len.filter(|&l| {
                let e = s.saturating_add(l);
                e <= self.data.len() && {
                    let mut t = Lexer::new(self.data, e);
                    t.skip_ws();
                    self.data[t.i..].starts_with(b"endstream")
                }
            });
            let end = match ok_len {
                Some(l) => s + l,
                None => {
                    let mut e = find(self.data, b"endstream", s).unwrap_or(self.data.len());
                    if e > s && self.data[e - 1] == b'\n' {
                        e -= 1;
                    }
                    if e > s && self.data[e - 1] == b'\r' {
                        e -= 1;
                    }
                    e
                }
            };
            let raw = self.data[s..end.max(s)].to_vec();
            return Some((n as u32, Obj::Stream(Rc::new(d.clone()), Rc::new(raw))));
        }
        Some((n as u32, o))
    }

    fn scan(&self) -> Rc<HashMap<u32, Loc>> {
        if let Some(s) = self.scanned.borrow().as_ref() {
            return s.clone();
        }
        let mut map = HashMap::new();
        let d = self.data;
        let mut i = 0;
        while let Some(p) = find(d, b"obj", i) {
            i = p + 3;
            if d.get(p + 3).is_some_and(|&c| !is_ws(c) && !is_delim(c)) || p == 0 || !is_ws(d[p - 1]) {
                continue;
            }
            // Walk back over "num gen ".
            let mut j = p;
            while j > 0 && is_ws(d[j - 1]) {
                j -= 1;
            }
            let ge = j;
            while j > 0 && d[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == ge || j == 0 || !is_ws(d[j - 1]) {
                continue;
            }
            while j > 0 && is_ws(d[j - 1]) {
                j -= 1;
            }
            let ne = j;
            while j > 0 && d[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j == ne || (j > 0 && !is_ws(d[j - 1]) && !is_delim(d[j - 1])) {
                continue;
            }
            if let Ok(n) = std::str::from_utf8(&d[j..ne]).unwrap_or("x").parse::<u32>() {
                map.insert(n, Loc::At(j));
            }
        }
        // Publish the direct objects first: reading an object stream may need them.
        *self.scanned.borrow_mut() = Some(Rc::new(map.clone()));
        // Objects kept inside object streams.
        let direct: Vec<(u32, usize)> = map.iter().filter_map(|(n, l)| if let Loc::At(p) = l { Some((*n, *p)) } else { None }).collect();
        for (n, p) in direct {
            let head = &d[p..(p + 400).min(d.len())];
            if find(head, b"/ObjStm", 0).is_some()
                && let Some(st) = self.objstm_of(n, Some(p))
            {
                // Keep each object's place in the index, so finding it later
                // is a direct look-up, not a search of the whole index.
                for (i, (m, _)) in st.1.iter().enumerate() {
                    map.entry(*m).or_insert(Loc::InStream(n, i));
                }
            }
        }
        let rc = Rc::new(map);
        *self.scanned.borrow_mut() = Some(rc.clone());
        rc
    }

    fn scan_trailer(&self, scan: &HashMap<u32, Loc>) -> Dict {
        let mut t = Dict::new();
        let mut i = 0;
        while let Some(p) = find(self.data, b"trailer", i) {
            i = p + 7;
            let mut lx = Lexer::new(self.data, p + 7);
            let o = lx.obj(true);
            if self.stopped(&lx) {
                break;
            }
            if let Some(Obj::Dict(d)) = o {
                for (k, v) in d {
                    t.insert(k, v);
                }
            }
        }
        if !t.contains_key("Root") {
            for &n in scan.keys() {
                if let Some(d) = self.get(n).dict()
                    && d.get("Type").and_then(Obj::name) == Some("Catalog")
                {
                    t.insert("Root".into(), Obj::Ref(n));
                    break;
                }
            }
        }
        t
    }

    fn objstm_of(&self, n: u32, at: Option<usize>) -> Option<Rc<ObjStm>> {
        if let Some(s) = self.objstm.borrow().get(&n) {
            return Some(s.clone());
        }
        let o = match at {
            Some(p) => self.parse_at(p).map(|x| x.1)?,
            None => self.get(n),
        };
        let Obj::Stream(d, raw) = o else { return None };
        let data = decode_stream(&d, &raw, self).ok()?;
        // Object streams are kept once decoded: their bytes count toward a
        // file-wide budget; past it the file is refused (PD8).
        let kept = self.objstm_bytes.get().saturating_add(data.len());
        self.objstm_bytes.set(kept);
        if kept > MAX_OBJSTM_BYTES {
            self.too_big.set(true);
            return None;
        }
        let count = d.get("N").and_then(Obj::num).unwrap_or(0.0) as usize;
        let first = d.get("First").and_then(Obj::num).unwrap_or(0.0) as usize;
        // An index longer than the import reads refuses the file (PD8): cutting
        // it short would quietly lose the objects past the cut.
        if count > MAX_ITEMS {
            self.too_complex.set(true);
            return None;
        }
        let mut lx = Lexer::new(&data, 0);
        let mut list = vec![];
        for _ in 0..count {
            let (Some(Obj::Num(m)), Some(Obj::Num(off))) = (lx.obj(false), lx.obj(false)) else { break };
            list.push((m as u32, first + off as usize));
        }
        if self.stopped(&lx) {
            return None;
        }
        let mut first_at = HashMap::with_capacity(list.len());
        for &(m, off) in &list {
            first_at.entry(m).or_insert(off);
        }
        let rc = Rc::new(ObjStm(data, list, first_at));
        self.objstm.borrow_mut().insert(n, rc.clone());
        Some(rc)
    }

    fn load(&self, n: u32, loc: Loc) -> Option<Obj> {
        match loc {
            Loc::At(p) => match self.parse_at(p) {
                Some((m, o)) if m == n => Some(o),
                _ => None,
            },
            Loc::InStream(s, idx) => {
                let st = self.objstm_of(s, None)?;
                let off = if idx < st.1.len() && st.1[idx].0 == n {
                    st.1[idx].1
                } else {
                    *st.2.get(&n)?
                };
                let mut lx = Lexer::new(&st.0, off);
                let o = lx.obj(true);
                if self.stopped(&lx) {
                    return None;
                }
                o
            }
        }
    }

    fn get(&self, n: u32) -> Obj {
        if let Some(o) = self.cache.borrow().get(&n) {
            return o.clone();
        }
        // Guard against reference loops (an object whose length is itself…).
        {
            let mut busy = self.busy.borrow_mut();
            if busy.len() >= 64 || !busy.insert(n) {
                return Obj::Null;
            }
        }
        let mut o = self.xref.get(&n).and_then(|&l| self.load(n, l));
        if o.is_none() {
            let scan = self.scan();
            o = scan.get(&n).and_then(|&l| self.load(n, l));
        }
        self.busy.borrow_mut().remove(&n);
        let o = o.unwrap_or(Obj::Null);
        self.cache.borrow_mut().insert(n, o.clone());
        o
    }

    fn resolve(&self, o: &Obj) -> Obj {
        let mut cur = o.clone();
        for _ in 0..16 {
            match cur {
                Obj::Ref(n) => cur = self.get(n),
                _ => return cur,
            }
        }
        Obj::Null
    }

    /// A dictionary that is looked things up in many times: one behind a
    /// reference is resolved once and shared.
    fn rdict_rc(&self, o: Option<&Obj>) -> Rc<Dict> {
        if let Some(Obj::Ref(n)) = o {
            if let Some(d) = self.dicts.borrow().get(n) {
                return d.clone();
            }
            let d = Rc::new(self.rdict(o));
            self.dicts.borrow_mut().insert(*n, d.clone());
            return d;
        }
        Rc::new(self.rdict(o))
    }

    /// One entry of a dictionary (given in place or behind a reference),
    /// without copying the whole dictionary.
    fn entry(&self, holder: Option<&Obj>, key: &str) -> Option<Obj> {
        match holder {
            Some(Obj::Dict(d)) => d.get(key).cloned(),
            Some(o @ Obj::Ref(_)) => self.rdict_rc(Some(o)).get(key).cloned(),
            _ => None,
        }
    }

    fn rdict(&self, o: Option<&Obj>) -> Dict {
        o.map(|o| self.resolve(o)).and_then(|o| o.dict().cloned()).unwrap_or_default()
    }

    fn rect(&self, o: Option<&Obj>) -> Option<[f64; 4]> {
        let Obj::Arr(a) = self.resolve(o?) else { return None };
        if a.len() < 4 {
            return None;
        }
        let v: Vec<f64> = a.iter().map(|x| self.resolve(x).num().unwrap_or(0.0)).collect();
        Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
    }

    /// The pages in order; a page tree deeper than 32 levels or with more than
    /// 10 000 pages is refused rather than read in part (PD8).
    fn pages(&self) -> Result<Vec<Page>, String> {
        let root = self.rdict(self.trailer.get("Root"));
        let mut out = vec![];
        let mut seen = HashSet::new();
        let mut over = false;
        let mut missing = false;
        let mut kept = 0usize;
        if let Some(p) = root.get("Pages") {
            self.walk(p, &Rc::new(Dict::new()), [0.0, 0.0, 612.0, 792.0], 0, &mut seen, &mut out, &mut (&mut over, &mut missing, &mut kept));
        }
        if over {
            return Err("This PDF has more pages (a deeper page tree, or bigger page resources) than the import reads; nothing was imported. Save the drawing's pages as a smaller PDF, or export DXF.".into());
        }
        if missing {
            return Err("This PDF lists a page that isn't in the file; nothing was imported, so no page goes missing. Save it again from a PDF viewer, or export DXF.".into());
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    /// `flags`: (over a limit, a listed page missing, bytes of dictionaries
    /// kept) — a limit or a missing page refuses the file (PD8).
    fn walk(&self, node: &Obj, res: &Rc<Dict>, media: [f64; 4], depth: u32, seen: &mut HashSet<u32>, out: &mut Vec<Page>, flags: &mut (&mut bool, &mut bool, &mut usize)) {
        if *flags.0 || *flags.1 {
            return;
        }
        if depth > 32 || out.len() >= 10_000 {
            *flags.0 = true;
            return;
        }
        if let Obj::Ref(n) = node
            && !seen.insert(*n)
        {
            return;
        }
        let Obj::Dict(d) = self.resolve(node) else {
            *flags.1 = true;
            return;
        };
        // Pages without resources of their own share their parent's.
        let res = if d.contains_key("Resources") {
            let own = self.rdict(d.get("Resources"));
            *flags.2 = flags.2.saturating_add(own.iter().map(|(k, v)| k.len() + obj_size(v, 0)).sum::<usize>());
            Rc::new(own)
        } else {
            res.clone()
        };
        *flags.2 = flags.2.saturating_add(d.iter().map(|(k, v)| k.len() + obj_size(v, 0)).sum::<usize>());
        if *flags.2 > MAX_PAGE_BYTES {
            *flags.0 = true;
            return;
        }
        let media = self.rect(d.get("MediaBox")).unwrap_or(media);
        match self.resolve(d.get("Kids").unwrap_or(&Obj::Null)) {
            Obj::Arr(kids) => {
                for k in &kids {
                    self.walk(k, &res, media, depth + 1, seen, out, flags);
                }
            }
            _ => {
                if d.contains_key("Contents") || d.get("Type").and_then(Obj::name) == Some("Page") {
                    out.push(Page { dict: d, resources: res, media });
                }
            }
        }
    }

    /// A page's content, its streams joined. A stream that can't be decoded, or
    /// content larger than the import reads, refuses the file (PD8): its
    /// contours would otherwise be missing.
    fn page_content(&self, page: &Dict) -> Result<Vec<u8>, String> {
        let mut out = vec![];
        let parts = match page.get("Contents") {
            // A page without content is an empty page.
            None => vec![],
            // Content the page names but the file doesn't hold is refused below.
            Some(c) => match self.resolve(c) {
                Obj::Arr(a) => a,
                o => vec![o],
            },
        };
        for p in parts {
            let Obj::Stream(d, raw) = self.resolve(&p) else {
                return Err("A page of this PDF points to content that isn't in the file; nothing was imported, so no contour goes missing. Save it again from a PDF viewer, or export DXF.".into());
            };
            {
                let v = decode_stream(&d, &raw, self).map_err(|e| format!("A page of this PDF can't be read ({e}); nothing was imported. Save it again from a PDF viewer, or export DXF."))?;
                if out.len().saturating_add(v.len()) > STREAM_MAX {
                    return Err("A page of this PDF is bigger than the import reads; nothing was imported. Export the part as DXF, or simplify the PDF.".into());
                }
                out.extend_from_slice(&v);
                out.push(b'\n');
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- painting

type Mat = [f64; 6];
const ID: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` then `b` (PDF row-vector order: a × b).
fn mul(a: &Mat, b: &Mat) -> Mat {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: &Mat, x: f64, y: f64) -> P {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

#[derive(Clone, Debug)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    /// Indexed colours: base space and the colour table.
    Indexed(Box<Space>, Vec<u8>),
    /// Separation / DeviceN: the tint is shown as a grey level.
    Tint,
    Pattern,
}

#[derive(Clone)]
struct Gs {
    ctm: Mat,
    stroke: [f64; 3],
    fill: [f64; 3],
    stroke_cs: Space,
    fill_cs: Space,
    stroke_pattern: bool,
    fill_pattern: bool,
}

#[derive(Default, Clone)]
struct Skipped {
    fills: usize,
    images: usize,
    cid_text: usize,
    /// Paths with a drawing command whose numbers can't be read (left out whole).
    damaged_paths: usize,
}

impl Skipped {
    fn add(&mut self, o: &Skipped) {
        self.fills += o.fills;
        self.images += o.images;
        self.cid_text += o.cid_text;
        self.damaged_paths += o.damaged_paths;
    }
    fn list(&self) -> Vec<String> {
        let mut v = vec![];
        if self.fills > 0 {
            v.push(format!("filled areas ({})", self.fills));
        }
        if self.images > 0 {
            v.push(format!("pictures ({})", self.images));
        }
        if self.cid_text > 0 {
            v.push(format!("text in embedded fonts ({})", self.cid_text));
        }
        if self.damaged_paths > 0 {
            v.push(format!("damaged paths ({})", self.damaged_paths));
        }
        v
    }
}

struct Sub {
    pts: Vec<P>,
    closed: bool,
}

#[derive(Clone)]
struct Font {
    cid: bool,
    first: i64,
    /// One width per byte code at most (256), shared by every use of the font.
    widths: Rc<Vec<f64>>,
}

#[derive(Clone)]
struct Text {
    tm: Mat,
    tlm: Mat,
    size: f64,
    leading: f64,
    char_sp: f64,
    word_sp: f64,
    hscale: f64,
    rise: f64,
    mode: i64,
    font: Option<Font>,
}

struct Painter<'d, 'a> {
    doc: &'d Doc<'a>,
    opts: &'d Options,
    k: f64,
    origin: [f64; 2],
    x_off: f64,
    gs: Gs,
    /// Saved graphics states with their text settings (font, size, spacing,
    /// rise, mode — the text matrices are not part of a saved state).
    stack: Vec<(Gs, Text)>,
    path: Vec<Sub>,
    /// Current point (mm) and start of the current subpath.
    cur: Option<P>,
    start: P,
    text: Text,
    oc: Vec<Option<String>>,
    out: Drawing,
    skipped: Skipped,
    budget: usize,
    /// Path points made so far.
    points: usize,
    /// Content bytes read so far (a form drawn many times is read each time).
    read_bytes: usize,
    /// A limit was reached (operations, nesting, points): part of the drawing
    /// is missing, so the import is refused whole (PD8).
    over: bool,
    /// A form's stream couldn't be decoded: its contours would be missing, so
    /// the import is refused whole (PD8).
    damaged: bool,
    /// The path being built has a command whose numbers can't be read: it is
    /// left out whole when painted, and named (never drawn a corner short).
    path_bad: bool,
    /// A position change (`cm`) whose numbers can't be read: the file is refused.
    bad_transform: bool,
    /// Fonts found by (resource table, name) while drawing this page; the
    /// tables live as long as the page is drawn.
    font_cache: RefCell<HashMap<(usize, String), Option<Font>>>,
    /// Layer names found by (resource table, name) while drawing this page.
    oc_cache: HashMap<(usize, String), Option<String>>,
    /// Colour spaces found by (resource table, name) while drawing this page.
    space_cache: RefCell<HashMap<(usize, String), Space>>,
    /// Form resource tables used on this page, kept until the page is done so
    /// none is freed and its address reused by another (the font cache's key).
    kept: HashMap<usize, Rc<Dict>>,
}

/// Most path points one PDF may give (PD8).
const MAX_POINTS: usize = 5_000_000;

/// Most content bytes one PDF may have read, its forms counted each time
/// they are drawn (PD8).
const MAX_READ: usize = 512 << 20;

fn white(c: &[f64; 3]) -> bool {
    c.iter().all(|&v| v > 0.95)
}

/// PD4: a readable layer name for a colour.
pub(crate) fn colour_name(c: [f64; 3]) -> String {
    let v = c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as i32);
    const NAMED: [(&str, [i32; 3]); 9] = [
        ("Black", [0, 0, 0]),
        ("White", [255, 255, 255]),
        ("Red", [255, 0, 0]),
        ("Yellow", [255, 255, 0]),
        ("Green", [0, 255, 0]),
        ("Cyan", [0, 255, 255]),
        ("Blue", [0, 0, 255]),
        ("Magenta", [255, 0, 255]),
        ("Grey", [128, 128, 128]),
    ];
    for (n, rgb) in NAMED {
        if (0..3).all(|i| (v[i] - rgb[i]).abs() <= 16) {
            return n.to_string();
        }
    }
    format!("Colour #{:02X}{:02X}{:02X}", v[0], v[1], v[2])
}

impl<'d, 'a> Painter<'d, 'a> {
    fn new(doc: &'d Doc<'a>, opts: &'d Options, k: f64, media: [f64; 4], x_off: f64) -> Self {
        Painter {
            doc,
            opts,
            k,
            origin: [media[0], media[1]],
            x_off,
            gs: Gs {
                ctm: ID,
                stroke: [0.0; 3],
                fill: [0.0; 3],
                stroke_cs: Space::Gray,
                fill_cs: Space::Gray,
                stroke_pattern: false,
                fill_pattern: false,
            },
            stack: vec![],
            path: vec![],
            cur: None,
            start: [0.0, 0.0],
            text: Text { tm: ID, tlm: ID, size: 0.0, leading: 0.0, char_sp: 0.0, word_sp: 0.0, hscale: 1.0, rise: 0.0, mode: 0, font: None },
            oc: vec![],
            out: Drawing::default(),
            skipped: Skipped::default(),
            budget: 20_000_000,
            points: 0,
            read_bytes: 0,
            over: false,
            damaged: false,
            path_bad: false,
            bad_transform: false,
            font_cache: RefCell::new(HashMap::new()),
            oc_cache: HashMap::new(),
            space_cache: RefCell::new(HashMap::new()),
            kept: HashMap::new(),
        }
    }

    /// User space → drawing millimetres (PD1, PD3).
    fn tp(&self, x: f64, y: f64) -> P {
        let p = apply(&self.gs.ctm, x, y);
        [(p[0] - self.origin[0]) * self.k + self.x_off, (p[1] - self.origin[1]) * self.k]
    }

    fn layer(&mut self, stroke: bool) -> String {
        let name: String = match self.opts.layers {
            Layers::None => "0".into(),
            // A PDF layer's name, at most 255 characters as in CAD programs.
            Layers::PdfLayers => self.oc.iter().rev().flatten().next().map(|n| n.chars().take(255).collect()).unwrap_or_else(|| "0".into()),
            Layers::Colour => {
                let pattern = if stroke { self.gs.stroke_pattern } else { self.gs.fill_pattern };
                if pattern {
                    "Pattern".into()
                } else {
                    colour_name(if stroke { self.gs.stroke } else { self.gs.fill })
                }
            }
        };
        // (Each piece's copy of the name is charged where the piece is made.)
        name
    }

    /// A colour space; one selected by name is worked out once per (resource
    /// table, name) for the page, so selecting it again unpacks nothing.
    fn space(&self, o: &Obj, res: &Dict) -> Space {
        let Obj::Name(n) = o else { return self.space_at(o, res, 0) };
        let ck = (res as *const Dict as usize, n.clone());
        if let Some(sp) = self.space_cache.borrow().get(&ck) {
            return sp.clone();
        }
        let sp = self.space_at(o, res, 0);
        self.space_cache.borrow_mut().insert(ck, sp.clone());
        sp
    }

    /// A colour space; one built on another (Indexed on its base, a named one
    /// from the resources) stops after a few steps, so a file whose colour space
    /// is its own base can't loop (PD8).
    fn space_at(&self, o: &Obj, res: &Dict, depth: u32) -> Space {
        if depth > 8 {
            return Space::Gray;
        }
        let o = self.doc.resolve(o);
        let (name, arr) = match &o {
            Obj::Name(n) => (n.clone(), vec![]),
            Obj::Arr(a) if !a.is_empty() => (self.doc.resolve(&a[0]).name().unwrap_or("").to_string(), a.clone()),
            _ => return Space::Gray,
        };
        match name.as_str() {
            "DeviceGray" | "G" | "CalGray" => Space::Gray,
            "DeviceRGB" | "RGB" | "CalRGB" | "Lab" => Space::Rgb,
            "DeviceCMYK" | "CMYK" => Space::Cmyk,
            "Pattern" => Space::Pattern,
            "Separation" | "DeviceN" => Space::Tint,
            "ICCBased" => {
                let n = arr.get(1).map(|s| self.doc.resolve(s)).and_then(|s| s.dict().and_then(|d| d.get("N")).and_then(Obj::num)).unwrap_or(3.0);
                match n as i32 {
                    1 => Space::Gray,
                    4 => Space::Cmyk,
                    _ => Space::Rgb,
                }
            }
            "Indexed" | "I" => {
                let base = arr.get(1).map(|b| self.space_at(b, res, depth + 1)).unwrap_or(Space::Rgb);
                let table = match arr.get(3).map(|t| self.doc.resolve(t)) {
                    Some(Obj::Str(s)) => s,
                    Some(Obj::Stream(d, raw)) => decode_stream(&d, &raw, self.doc).unwrap_or_default(),
                    _ => vec![],
                };
                // At most 256 entries of at most 4 components: the rest is never
                // looked up, so it isn't kept (and copied with every saved state).
                let mut table = table;
                table.truncate(256 * 4);
                Space::Indexed(Box::new(base), table)
            }
            other => {
                // A named colour space from the resources.
                match self.doc.entry(res.get("ColorSpace"), other) {
                    Some(def) if !matches!(self.doc.resolve(&def), Obj::Name(ref n) if n == other) => self.space_at(&def, &Dict::new(), depth + 1),
                    _ => Space::Gray,
                }
            }
        }
    }

    fn to_rgb(space: &Space, v: &[f64]) -> [f64; 3] {
        let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
        match space {
            Space::Gray => [g(0); 3],
            Space::Rgb => [g(0), g(1), g(2)],
            Space::Cmyk => {
                let k = g(3);
                [(1.0 - g(0)) * (1.0 - k), (1.0 - g(1)) * (1.0 - k), (1.0 - g(2)) * (1.0 - k)]
            }
            Space::Tint => [1.0 - g(0); 3],
            Space::Pattern => [0.0; 3],
            Space::Indexed(base, table) => {
                let n = match **base {
                    Space::Gray => 1,
                    Space::Cmyk => 4,
                    _ => 3,
                };
                let i = g(0).max(0.0) as usize * n;
                if i + n <= table.len() {
                    let c: Vec<f64> = table[i..i + n].iter().map(|&b| b as f64 / 255.0).collect();
                    Self::to_rgb(base, &c)
                } else {
                    [0.0; 3]
                }
            }
        }
    }

    /// A font, built once: per file for fonts behind a reference, and per
    /// resource table and name while drawing (inline fonts too), so a font
    /// selected many times isn't read again.
    fn font(&self, name: &str, res: &Dict) -> Option<Font> {
        let ck = (res as *const Dict as usize, name.to_string());
        if let Some(f) = self.font_cache.borrow().get(&ck) {
            return f.clone();
        }
        let f = self.font_uncached(name, res);
        self.font_cache.borrow_mut().insert(ck, f.clone());
        f
    }

    fn font_uncached(&self, name: &str, res: &Dict) -> Option<Font> {
        let fref = self.doc.entry(res.get("Font"), name)?;
        let key = if let Obj::Ref(n) = fref { Some(n) } else { None };
        if let Some(f) = key.and_then(|n| self.doc.fonts.borrow().get(&n).cloned()) {
            return Some(f);
        }
        let f = self.doc.rdict(Some(&fref));
        if f.is_empty() {
            return None;
        }
        let cid = f.get("Subtype").and_then(Obj::name) == Some("Type0");
        let first = f.get("FirstChar").map(|o| self.doc.resolve(o)).and_then(|o| o.num()).unwrap_or(0.0) as i64;
        // A simple font's codes are single bytes: no more than 256 widths are
        // ever looked up, so no more are kept.
        let widths = match f.get("Widths").map(|o| self.doc.resolve(o)) {
            Some(Obj::Arr(a)) => a.iter().take(256).map(|w| self.doc.resolve(w).num().unwrap_or(0.0)).collect(),
            _ => vec![],
        };
        let font = Font { cid, first, widths: Rc::new(widths) };
        if let Some(n) = key {
            self.doc.fonts.borrow_mut().insert(n, font.clone());
        }
        Some(font)
    }

    fn move_to(&mut self, p: P) {
        self.path.push(Sub { pts: vec![p], closed: false });
        self.cur = Some(p);
        self.start = p;
    }

    fn line_to(&mut self, p: P) {
        self.points += 1;
        if self.points > MAX_POINTS {
            self.over = true;
            return;
        }
        match self.path.last_mut() {
            Some(s) if !s.closed && self.cur.is_some() => s.pts.push(p),
            _ => {
                let st = self.cur.unwrap_or(p);
                self.path.push(Sub { pts: vec![st, p], closed: false });
                self.start = st;
            }
        }
        self.cur = Some(p);
    }

    fn curve_to(&mut self, c1: P, c2: P, p3: P) {
        let Some(p0) = self.cur else {
            self.move_to(p3);
            return;
        };
        let mut pts = vec![];
        flatten_bezier(p0, c1, c2, p3, 0, &mut pts);
        for p in pts {
            self.line_to(p);
        }
    }

    fn close_path(&mut self) {
        if let Some(s) = self.path.last_mut() {
            s.closed = true;
        }
        self.cur = Some(self.start);
    }

    /// PD5: stroked paths are geometry; fill-only shapes are left out unless asked.
    fn paint(&mut self, stroke: bool, fill: bool, close: bool) {
        let path = std::mem::take(&mut self.path);
        self.cur = None;
        if std::mem::take(&mut self.path_bad) {
            self.skipped.damaged_paths += 1;
            return;
        }
        let (layer, keep) = if stroke {
            (self.layer(true), true)
        } else if fill {
            let keep = self.opts.fills && !white(&self.gs.fill);
            (self.layer(false), keep)
        } else {
            (String::new(), false)
        };
        if !keep {
            if fill && !stroke && path.iter().any(|s| s.pts.len() > 1) {
                self.skipped.fills += 1;
            }
            return;
        }
        for s in path {
            let mut pts = s.pts;
            pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7);
            let mut closed = s.closed || close || (fill && !stroke);
            if pts.len() > 2 {
                let (a, b) = (pts[0], pts[pts.len() - 1]);
                if (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7 {
                    pts.pop();
                    closed = true;
                }
            }
            if pts.len() < 2 {
                continue;
            }
            if pts.len() == 2 {
                closed = false;
            }
            // Each piece keeps its own copy of the layer name: charged per piece.
            self.points = self.points.saturating_add(layer.len() / 16);
            if self.points > MAX_POINTS {
                self.over = true;
                return;
            }
            self.out.pieces.push(Piece { layer: layer.clone(), pts, closed });
        }
    }

    fn show_text(&mut self, s: &[u8], adjust: &[(usize, f64)]) {
        let Some(font) = self.text.font.clone() else { return };
        if font.cid {
            self.skipped.cid_text += 1;
            return;
        }
        let t = &self.text;
        let trm = mul(&mul(&[t.size * t.hscale, 0.0, 0.0, t.size, 0.0, t.rise], &t.tm), &self.gs.ctm);
        let at = [(trm[4] - self.origin[0]) * self.k + self.x_off, (trm[5] - self.origin[1]) * self.k];
        let height = (trm[2] * trm[2] + trm[3] * trm[3]).sqrt() * self.k;
        let angle = trm[1].atan2(trm[0]).to_degrees();
        let mut txt = String::new();
        let mut adv = 0.0;
        let mut ai = 0;
        for (i, &b) in s.iter().enumerate() {
            while ai < adjust.len() && adjust[ai].0 == i {
                let a = adjust[ai].1;
                adv -= a / 1000.0 * t.size * t.hscale;
                if a < -250.0 && !txt.ends_with(' ') {
                    txt.push(' ');
                }
                ai += 1;
            }
            let w = font.widths.get((b as i64 - font.first).max(0) as usize).copied().filter(|_| b as i64 >= font.first).unwrap_or(500.0);
            adv += (w / 1000.0 * t.size + t.char_sp + if b == 32 { t.word_sp } else { 0.0 }) * t.hscale;
            if b >= 32 && b != 127 {
                txt.push(b as char);
            }
        }
        while ai < adjust.len() {
            adv -= adjust[ai].1 / 1000.0 * t.size * t.hscale;
            ai += 1;
        }
        self.text.tm = mul(&[1.0, 0.0, 0.0, 1.0, adv, 0.0], &self.text.tm);
        let txt = txt.trim().to_string();
        // Text render mode 3 is invisible (scanned-page text layers).
        if self.opts.text && !txt.is_empty() && self.text.mode != 3 && height > 1e-6 {
            // Text drawn through a form is made once per drawing of it: its bytes
            // count toward the file's budget (PD8), the same budget as points.
            self.points = self.points.saturating_add(txt.len() / 16 + 1);
            if self.points > MAX_POINTS {
                self.over = true;
                return;
            }
            let layer = self.layer(false);
            self.out.texts.push((layer, txt, at, height, angle));
        }
    }

    fn run(&mut self, content: &[u8], res: &Dict, depth: u32) {
        if depth > 12 {
            self.over = true;
            return;
        }
        self.read_bytes = self.read_bytes.saturating_add(content.len());
        if self.read_bytes > MAX_READ {
            self.over = true;
            return;
        }
        let mut lx = Lexer::new(content, 0);
        let mut ops: Vec<Obj> = vec![];
        let nums = |ops: &Vec<Obj>, n: usize| -> Option<Vec<f64>> {
            if ops.len() < n {
                return None;
            }
            ops[ops.len() - n..].iter().map(Obj::num).collect()
        };
        while let Some(tok) = lx.obj(false) {
            if self.budget == 0 || self.over {
                self.over = true;
                return;
            }
            self.budget -= 1;
            let Obj::Kw(op) = tok else {
                ops.push(tok);
                if ops.len() > 10_000 {
                    ops.clear();
                }
                continue;
            };
            match op.as_slice() {
                b"q" => {
                    self.stack.push((self.gs.clone(), self.text.clone()));
                    if self.stack.len() > 4096 {
                        // Deeper saving than that can't be restored right: refuse.
                        self.over = true;
                        return;
                    }
                }
                b"Q" => {
                    if let Some((g, t)) = self.stack.pop() {
                        self.gs = g;
                        self.restore_text(t);
                    }
                }
                b"cm" => {
                    if let Some(v) = nums(&ops, 6) {
                        self.gs.ctm = mul(&[v[0], v[1], v[2], v[3], v[4], v[5]], &self.gs.ctm);
                    } else {
                        // Everything after an unreadable position change would land
                        // in the wrong place: refuse (PD8).
                        self.bad_transform = true;
                        return;
                    }
                }
                // A drawing command must have all its numbers.
                b"m" | b"l" | b"c" | b"v" | b"y" | b"re" if nums(&ops, match op.as_slice() { b"m" | b"l" => 2, b"c" => 6, _ => 4 }).is_none() => {
                    self.path_bad = true;
                }
                b"m" => {
                    if let Some(v) = nums(&ops, 2) {
                        let p = self.tp(v[0], v[1]);
                        self.move_to(p);
                    }
                }
                b"l" => {
                    if let Some(v) = nums(&ops, 2) {
                        let p = self.tp(v[0], v[1]);
                        self.line_to(p);
                    }
                }
                b"c" => {
                    if let Some(v) = nums(&ops, 6) {
                        let (a, b, c) = (self.tp(v[0], v[1]), self.tp(v[2], v[3]), self.tp(v[4], v[5]));
                        self.curve_to(a, b, c);
                    }
                }
                b"v" => {
                    if let Some(v) = nums(&ops, 4) {
                        let (b, c) = (self.tp(v[0], v[1]), self.tp(v[2], v[3]));
                        let a = self.cur.unwrap_or(b);
                        self.curve_to(a, b, c);
                    }
                }
                b"y" => {
                    if let Some(v) = nums(&ops, 4) {
                        let (a, c) = (self.tp(v[0], v[1]), self.tp(v[2], v[3]));
                        self.curve_to(a, c, c);
                    }
                }
                b"re" => {
                    if let Some(v) = nums(&ops, 4) {
                        let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                        // A rectangle's four corners count like any other points.
                        self.points = self.points.saturating_add(4);
                        if self.points > MAX_POINTS {
                            self.over = true;
                            return;
                        }
                        let pts = vec![self.tp(x, y), self.tp(x + w, y), self.tp(x + w, y + h), self.tp(x, y + h)];
                        let p0 = pts[0];
                        self.path.push(Sub { pts, closed: true });
                        self.cur = Some(p0);
                        self.start = p0;
                    }
                }
                b"h" => self.close_path(),
                b"S" => self.paint(true, false, false),
                // `s`, `b`, `b*` close the current subpath only, then paint
                // (earlier open subpaths stay open); a fill closes every one.
                b"s" => {
                    self.close_path();
                    self.paint(true, false, false);
                }
                b"f" | b"F" | b"f*" => self.paint(false, true, true),
                b"B" | b"B*" => self.paint(true, true, false),
                b"b" | b"b*" => {
                    self.close_path();
                    self.paint(true, true, false);
                }
                b"n" => {
                    self.path.clear();
                    self.path_bad = false;
                    self.cur = None;
                }
                b"G" | b"g" | b"RG" | b"rg" | b"K" | b"k" => {
                    let n = match op.as_slice() {
                        b"G" | b"g" => 1,
                        b"RG" | b"rg" => 3,
                        _ => 4,
                    };
                    if let Some(v) = nums(&ops, n) {
                        let sp = match n {
                            1 => Space::Gray,
                            3 => Space::Rgb,
                            _ => Space::Cmyk,
                        };
                        let c = Self::to_rgb(&sp, &v);
                        if op[0].is_ascii_uppercase() {
                            self.gs.stroke = c;
                            self.gs.stroke_cs = sp;
                            self.gs.stroke_pattern = false;
                        } else {
                            self.gs.fill = c;
                            self.gs.fill_cs = sp;
                            self.gs.fill_pattern = false;
                        }
                    }
                }
                b"CS" | b"cs" => {
                    if let Some(o) = ops.last() {
                        let sp = self.space(o, res);
                        let pat = matches!(sp, Space::Pattern);
                        let c = Self::to_rgb(&sp, &[0.0, 0.0, 0.0, 1.0]);
                        let c = if matches!(sp, Space::Cmyk) { [0.0; 3] } else { c };
                        if op[0] == b'C' {
                            self.gs.stroke_cs = sp;
                            self.gs.stroke = c;
                            self.gs.stroke_pattern = pat;
                        } else {
                            self.gs.fill_cs = sp;
                            self.gs.fill = c;
                            self.gs.fill_pattern = pat;
                        }
                    }
                }
                b"SC" | b"SCN" | b"sc" | b"scn" => {
                    let v: Vec<f64> = ops.iter().rev().take_while(|o| o.num().is_some()).filter_map(Obj::num).collect::<Vec<_>>().into_iter().rev().collect();
                    let named = ops.last().is_some_and(|o| o.name().is_some());
                    if op[0] == b'S' {
                        if !named || !v.is_empty() {
                            self.gs.stroke = Self::to_rgb(&self.gs.stroke_cs, &v);
                        }
                        self.gs.stroke_pattern = named;
                    } else {
                        if !named || !v.is_empty() {
                            self.gs.fill = Self::to_rgb(&self.gs.fill_cs, &v);
                        }
                        self.gs.fill_pattern = named;
                    }
                }
                b"BDC" | b"BMC" => {
                    let mut name = None;
                    if op.as_slice() == b"BDC" && ops.len() >= 2 && ops[ops.len() - 2].name() == Some("OC") {
                        name = match &ops[ops.len() - 1] {
                            // A named layer: worked out once per (resource table,
                            // name) for the page — inline properties too.
                            Obj::Name(n) => {
                                let ck = (res as *const Dict as usize, n.clone());
                                match self.oc_cache.get(&ck) {
                                    Some(v) => v.clone(),
                                    None => {
                                        let v = self.oc_name_of(self.doc.entry(res.get("Properties"), n));
                                        self.oc_cache.insert(ck, v.clone());
                                        v
                                    }
                                }
                            }
                            o => self.oc_name_of(Some(o.clone())),
                        };
                    }
                    // Marked sections nest no deeper than saved states (PD8).
                    if self.oc.len() >= 4096 {
                        self.over = true;
                        return;
                    }
                    self.oc.push(name);
                }
                b"EMC" => {
                    self.oc.pop();
                }
                b"Do" => {
                    if let Some(Obj::Name(n)) = ops.last() {
                        self.xobject(n, res, depth);
                    }
                }
                b"BI" => {
                    // Inline picture: skip its data up to "EI".
                    let mut i = lx.i;
                    while let Some(p) = find(content, b"EI", i) {
                        let before = p == 0 || is_ws(content[p - 1]);
                        let after = content.get(p + 2).is_none_or(|&c| is_ws(c) || is_delim(c));
                        i = p + 2;
                        if before && after {
                            break;
                        }
                    }
                    lx.i = i.min(content.len());
                    self.skipped.images += 1;
                }
                b"BT" => {
                    self.text.tm = ID;
                    self.text.tlm = ID;
                }
                b"Tf" => {
                    if ops.len() >= 2 {
                        if let (Some(n), Some(s)) = (ops[ops.len() - 2].name(), ops[ops.len() - 1].num()) {
                            self.text.font = self.font(n, res);
                            self.text.size = s;
                        }
                    }
                }
                b"Tm" => {
                    if let Some(v) = nums(&ops, 6) {
                        self.text.tm = [v[0], v[1], v[2], v[3], v[4], v[5]];
                        self.text.tlm = self.text.tm;
                    }
                }
                b"Td" | b"TD" => {
                    if let Some(v) = nums(&ops, 2) {
                        if op.as_slice() == b"TD" {
                            self.text.leading = -v[1];
                        }
                        self.text.tlm = mul(&[1.0, 0.0, 0.0, 1.0, v[0], v[1]], &self.text.tlm);
                        self.text.tm = self.text.tlm;
                    }
                }
                b"T*" => self.next_line(),
                b"TL" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.leading = v[0];
                    }
                }
                b"Tc" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.char_sp = v[0];
                    }
                }
                b"Tw" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.word_sp = v[0];
                    }
                }
                b"Tz" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.hscale = v[0] / 100.0;
                    }
                }
                b"Ts" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.rise = v[0];
                    }
                }
                b"Tr" => {
                    if let Some(v) = nums(&ops, 1) {
                        self.text.mode = v[0] as i64;
                    }
                }
                b"Tj" => {
                    if let Some(Obj::Str(s)) = ops.last().cloned() {
                        self.show_text(&s, &[]);
                    }
                }
                b"'" | b"\"" => {
                    if op.as_slice() == b"\"" && ops.len() >= 3 {
                        self.text.word_sp = ops[ops.len() - 3].num().unwrap_or(0.0);
                        self.text.char_sp = ops[ops.len() - 2].num().unwrap_or(0.0);
                    }
                    self.next_line();
                    if let Some(Obj::Str(s)) = ops.last().cloned() {
                        self.show_text(&s, &[]);
                    }
                }
                b"TJ" => {
                    if let Some(Obj::Arr(a)) = ops.last().cloned() {
                        let mut s = vec![];
                        let mut adj = vec![];
                        for o in a {
                            match o {
                                Obj::Str(x) => s.extend(x),
                                Obj::Num(n) => adj.push((s.len(), n)),
                                _ => {}
                            }
                        }
                        self.show_text(&s, &adj);
                    }
                }
                _ => {}
            }
            ops.clear();
        }
        // Content nested deeper than the lexer reads ends the stream early: the
        // rest would be missing (PD8).
        if lx.too_deep {
            self.over = true;
        }
    }

    /// Put back saved text settings, keeping the current text matrices.
    fn restore_text(&mut self, t: Text) {
        let (tm, tlm) = (self.text.tm, self.text.tlm);
        self.text = Text { tm, tlm, ..t };
    }

    fn next_line(&mut self) {
        self.text.tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -self.text.leading], &self.text.tlm);
        self.text.tm = self.text.tlm;
    }

    fn ocg_name(&self, prop: &Dict) -> Option<String> {
        self.ocg_name_at(prop, 0)
    }

    /// A PDF layer's name from its properties (in place or behind a
    /// reference), at most 255 characters, worked out once per referenced
    /// layer (a layer marked many times isn't read again).
    fn oc_name_of(&self, o: Option<Obj>) -> Option<String> {
        let o = o?;
        let key = if let Obj::Ref(n) = o { Some(n) } else { None };
        if let Some(n) = key {
            if let Some(v) = self.doc.oc_names.borrow().get(&n) {
                return v.clone();
            }
        }
        let v = self.ocg_name(&self.doc.rdict(Some(&o))).map(|s| s.chars().take(255).collect::<String>());
        if let Some(n) = key {
            self.doc.oc_names.borrow_mut().insert(n, v.clone());
        }
        v
    }

    /// A membership dictionary is named after its first group; a file whose
    /// groups point back at themselves stops after a few steps (PD8).
    fn ocg_name_at(&self, prop: &Dict, depth: u32) -> Option<String> {
        if depth > 8 {
            return None;
        }
        if let Some(Obj::Str(s)) = prop.get("Name").map(|o| self.doc.resolve(o)) {
            return Some(pdf_text(&s));
        }
        match prop.get("OCGs").map(|o| self.doc.resolve(o)) {
            Some(Obj::Arr(a)) => a.first().and_then(|g| self.ocg_name_at(&self.doc.rdict(Some(g)), depth + 1)),
            Some(o @ Obj::Dict(_)) => self.ocg_name_at(o.dict()?, depth + 1),
            _ => None,
        }
    }

    fn xobject(&mut self, name: &str, res: &Dict, depth: u32) {
        // A drawing the page calls but the file doesn't hold: its contours would
        // be missing, so the import is refused (PD8).
        let Some(r) = self.doc.entry(res.get("XObject"), name) else {
            self.damaged = true;
            return;
        };
        let Obj::Stream(d, raw) = self.doc.resolve(&r) else {
            self.damaged = true;
            return;
        };
        match d.get("Subtype").and_then(Obj::name) {
            Some("Form") => {
                // Decoded once per form, kept for its later drawings.
                let key = if let Obj::Ref(n) = r { Some(n) } else { None };
                let cached = key.and_then(|n| self.doc.forms.borrow().get(&n).cloned());
                let data = match cached {
                    Some(d) => d,
                    None => {
                        let Ok(v) = decode_stream(&d, &raw, self.doc) else {
                            self.damaged = true;
                            return;
                        };
                        let v = Rc::new(v);
                        if let Some(n) = key {
                            self.doc.forms.borrow_mut().insert(n, v.clone());
                        }
                        v
                    }
                };
                // No matrix means none; one given must be six readable numbers,
                // or the form's drawing can't be placed and the file is refused.
                let m = match d.get("Matrix").map(|o| self.doc.resolve(o)) {
                    None => ID,
                    Some(Obj::Arr(a)) if a.len() == 6 => {
                        let v: Option<Vec<f64>> = a.iter().map(|x| self.doc.resolve(x).num().filter(|v| v.is_finite())).collect();
                        match v {
                            Some(v) => [v[0], v[1], v[2], v[3], v[4], v[5]],
                            None => {
                                self.bad_transform = true;
                                return;
                            }
                        }
                    }
                    Some(_) => {
                        self.bad_transform = true;
                        return;
                    }
                };
                // The form's own resources, resolved once per form; without its
                // own it uses the caller's (shared, not copied).
                let own: Option<Rc<Dict>> = match (d.get("Resources"), key) {
                    (None, _) => None,
                    (Some(o), Some(n)) => {
                        let cached = self.doc.form_res.borrow().get(&n).cloned();
                        Some(cached.unwrap_or_else(|| {
                            let r = self.doc.rdict_rc(Some(o));
                            self.doc.form_res.borrow_mut().insert(n, r.clone());
                            r
                        }))
                    }
                    (Some(o), None) => Some(self.doc.rdict_rc(Some(o))),
                };
                let oc = self.oc_name_of(d.get("OC").cloned());
                let saved = self.gs.clone();
                let saved_text = self.text.clone();
                // The caller's path, pen position and subpath start wait while the
                // form draws, and come back after it.
                let saved_path = std::mem::take(&mut self.path);
                let (saved_cur, saved_start, saved_bad) = (self.cur.take(), self.start, std::mem::take(&mut self.path_bad));
                let n_oc = self.oc.len();
                self.oc.push(oc);
                self.gs.ctm = mul(&m, &self.gs.ctm);
                if let Some(o) = &own {
                    self.kept.entry(Rc::as_ptr(o) as usize).or_insert_with(|| o.clone());
                }
                let sub_res: &Dict = own.as_deref().unwrap_or(res);
                self.run(&data, sub_res, depth + 1);
                self.oc.truncate(n_oc);
                self.gs = saved;
                self.restore_text(saved_text);
                self.path = saved_path;
                (self.cur, self.start, self.path_bad) = (saved_cur, saved_start, saved_bad);
            }
            Some("Image") => self.skipped.images += 1,
            _ => {}
        }
    }
}

/// A PDF text string (UTF-16 with a byte-order mark, or single bytes).
fn pdf_text(s: &[u8]) -> String {
    if s.len() >= 2 && s[0] == 0xFE && s[1] == 0xFF {
        let u: Vec<u16> = s[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        String::from_utf16_lossy(&u)
    } else {
        s.iter().map(|&b| b as char).collect()
    }
}

/// PD6: flatten a cubic Bézier (already in mm) within the arc smoothness.
fn flatten_bezier(p0: P, p1: P, p2: P, p3: P, depth: u32, out: &mut Vec<P>) {
    // Distance to the chord as a segment (not the endless line through it), so
    // a curve that loops out past its ends is never taken for a straight piece.
    let d = |p: P| {
        let (dx, dy) = (p3[0] - p0[0], p3[1] - p0[1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 1e-24 { 0.0 } else { (((p[0] - p0[0]) * dx + (p[1] - p0[1]) * dy) / l2).clamp(0.0, 1.0) };
        ((p[0] - p0[0] - t * dx).powi(2) + (p[1] - p0[1] - t * dy).powi(2)).sqrt()
    };
    // The curve lies within its control points' hull, so no farther from the
    // chord than its farthest control point.
    if depth >= 16 || d(p1).max(d(p2)) <= ARC_CHORD_ERR {
        out.push(p3);
        return;
    }
    let mid = |a: P, b: P| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (p01, p12, p23) = (mid(p0, p1), mid(p1, p2), mid(p2, p3));
    let (p012, p123) = (mid(p01, p12), mid(p12, p23));
    let m = mid(p012, p123);
    flatten_bezier(p0, p01, p012, m, depth + 1, out);
    flatten_bezier(m, p123, p23, p3, depth + 1, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Build a small PDF with a correct cross-reference table.
    fn make_pdf(pages: &[&str], compress: bool) -> Vec<u8> {
        let mut objs: Vec<Vec<u8>> = vec![];
        let n = pages.len();
        // 1 catalog, 2 pages, then (page, content) pairs.
        objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + 2 * i)).collect();
        objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 595 842] >>", kids.join(" ")).into_bytes());
        for (i, c) in pages.iter().enumerate() {
            objs.push(
                format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>", 4 + 2 * i)
                    .into_bytes(),
            );
            let body = if compress { miniz_oxide::deflate::compress_to_vec_zlib(c.as_bytes(), 6) } else { c.as_bytes().to_vec() };
            let mut s = format!("<< /Length {}{} >>\nstream\n", body.len(), if compress { " /Filter /FlateDecode" } else { "" }).into_bytes();
            s.extend(body);
            s.extend(b"\nendstream");
            objs.push(s);
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n", i + 1).bytes());
            out.extend(o);
            out.extend(b"\nendobj\n");
        }
        let x = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }

    #[test]
    fn a_file_object_past_the_lexer_limits_refuses_the_whole_file() {
        // Page 1 gets a list over a million entries, or one nested 70 deep; the
        // cross-reference table is dropped so the objects are found by scanning.
        // Page 2 alone must not come in as if it were the whole drawing (PD8).
        let page = "0 0 m 72 0 l 72 72 l 0 72 l h S";
        let long = format!("/Junk [{}]", "0 ".repeat(MAX_ITEMS + 1));
        let deep = format!("/Junk {}{}", "[".repeat(70), "]".repeat(70));
        for junk in [long, deep] {
            let pdf = make_pdf(&[page, page], false);
            let x = find(&pdf, b"xref", 0).unwrap();
            let p3 = find(&pdf, b"3 0 obj\n<<", 0).unwrap() + 10;
            let mut bad = pdf[..p3].to_vec();
            bad.extend(junk.bytes());
            bad.extend(&pdf[p3..x]);
            bad.extend(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
            let e = read(&bad, &Options::default()).unwrap_err();
            assert!(e.contains("nothing was imported"), "{e}");
        }
    }

    fn build(part: &ReadPart) -> crate::dxf::CustomGeo {
        crate::dxf::build(&part.drawing, &BTreeMap::new()).unwrap().0
    }

    #[test]
    fn a_stroked_plate_with_a_round_hole_is_read_in_millimetres() {
        // 72 pt = 25.4 mm. A 144 × 72 pt plate and a circle made of 4 Béziers.
        let k = 0.5523 * 18.0;
        let circle = format!(
            "108 36 m 108 {a} {b} 54 90 54 c {c} 54 72 {a} 72 36 c 72 {d} {c} 18 90 18 c {b} 18 108 {d} 108 36 c h S",
            a = 36.0 + k,
            b = 90.0 + k,
            c = 90.0 - k,
            d = 36.0 - k
        );
        let content = format!("1 1 1 rg 0 0 595 842 re f 0 0 0 RG 0 0 m 144 0 l 144 72 l 0 72 l h S {circle}");
        let pdf = make_pdf(&[&content], true);
        let parts = read(&pdf, &Options::default()).unwrap();
        assert_eq!(parts.len(), 1);
        let d = &parts[0].drawing;
        // The white page background is a fill only: left out (PD5).
        assert_eq!(d.pieces.len(), 2);
        assert_eq!(d.layers().keys().cloned().collect::<Vec<_>>(), vec!["Black".to_string()]);
        let g = build(&parts[0]);
        assert!((g.w - 50.8).abs() < 1e-6 && (g.h - 25.4).abs() < 1e-6, "{} x {}", g.w, g.h);
        assert_eq!(g.holes.len(), 1);
        let r = 18.0 * MM_PER_PT;
        let hole_area = crate::nest::area(&g.holes[0]);
        assert!((hole_area - std::f64::consts::PI * r * r).abs() < 0.02 * hole_area, "hole area {hole_area}");
    }

    #[test]
    fn a_colour_space_built_on_itself_and_a_huge_predictor_row_dont_crash() {
        // PD8: an Indexed colour space whose base is itself.
        let pdf = make_pdf(&["/CS1 CS 0 SC 0 0 m 72 0 l 72 72 l 0 72 l h S"], false);
        let text = String::from_utf8_lossy(&pdf)
            .replace("/Resources << /Font", "/Resources << /ColorSpace << /CS1 9 0 R >> /Font")
            .replace("trailer", "9 0 obj\n[/Indexed 9 0 R 1 <000000>]\nendobj\ntrailer");
        assert_eq!(read(text.as_bytes(), &Options::default()).unwrap()[0].drawing.pieces.len(), 1);
        // A predictor row of 10^18 columns over a few bytes: left as it is.
        let mut parms = Dict::new();
        parms.insert("Predictor".into(), Obj::Num(12.0));
        parms.insert("Columns".into(), Obj::Num(1e18));
        let doc = Doc::open(&pdf, 0).unwrap();
        assert_eq!(predict(vec![2, 1, 2, 3], &Obj::Dict(parms), &doc), vec![2, 1, 2, 3]);
    }

    #[test]
    fn a_big_form_drawn_many_times_is_refused() {
        // An 8 MB form of blank space drawn 100 times: 800 MB to read, over the
        // file's read budget, so refused rather than holding up the window.
        let calls = "/F1 Do ".repeat(100);
        let pdf = make_pdf(&[&format!("0 0 m 72 0 l 72 72 l 0 72 l h S {calls}")], false);
        let blank = " ".repeat(8 << 20);
        let form = format!("9 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length {} >>\nstream\n{blank}\nendstream\nendobj\n", blank.len());
        let text = String::from_utf8_lossy(&pdf)
            .replace("/Resources << /Font", "/Resources << /XObject << /F1 9 0 R >> /Font")
            .replace("trailer", &format!("{form}trailer"));
        assert!(read(text.as_bytes(), &Options::default()).unwrap_err().contains("bigger or deeper"));
    }

    #[test]
    fn the_limits_hold_for_the_whole_file_not_each_page() {
        // Two pages, each well under the point limit; with the limit set just
        // above one page's points, the second page goes over: refused.
        let page = "0 0 m 72 0 l 72 72 l 0 72 l h S";
        let pdf = make_pdf(&[page, page], false);
        let doc = Doc::open(&pdf, 0).unwrap();
        let o = Options::default();
        let mut p = Painter::new(&doc, &o, MM_PER_PT, [0.0, 0.0, 595.0, 842.0], 0.0);
        p.run(page.as_bytes(), &Dict::new(), 0);
        let one = p.points;
        assert!(one > 0 && !p.over);
        p.points = MAX_POINTS - one + 1;
        p.run(page.as_bytes(), &Dict::new(), 0);
        assert!(p.over);
    }

    #[test]
    fn a_pdf_past_the_limits_is_refused_not_cut_short() {
        // PD8: 5000 nested saves before the outline: refused whole.
        let content = format!("{} 0 0 m 72 0 l 72 72 l 0 72 l h S", "q ".repeat(5000));
        let e = read(&make_pdf(&[&content], false), &Options::default()).unwrap_err();
        assert!(e.contains("bigger or deeper"), "{e}");
    }

    #[test]
    fn a_listed_page_that_isnt_there_is_refused() {
        let pdf = String::from_utf8_lossy(&make_pdf(&["0 0 m 72 0 l S"], false)).replace("/Kids [3 0 R]", "/Kids [3 0 R 999 0 R]");
        assert!(read(pdf.as_bytes(), &Options::default()).unwrap_err().contains("page that isn't"));
    }

    #[test]
    fn an_unreadable_position_change_refuses_the_file() {
        let pdf = make_pdf(&["0 0 m 72 0 l 72 72 l 0 72 l h S 1 0 0 1 50 cm 0 0 m 10 0 l 10 10 l h S"], false);
        assert!(read(&pdf, &Options::default()).unwrap_err().contains("position change"));
        // A form whose matrix has five numbers, or one that isn't a number.
        for m in ["[1 0 0 1 50]", "[1 0 0 1 /bad 0]"] {
            let pdf = make_pdf(&["0 0 m 72 0 l 72 72 l 0 72 l h S /F1 Do"], false);
            let body = "0 0 m 10 0 l 10 10 l h S";
            let form = format!("9 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Matrix {m} /Length {} >>\nstream\n{body}\nendstream\nendobj\n", body.len());
            let text = String::from_utf8_lossy(&pdf)
                .replace("/Resources << /Font", "/Resources << /XObject << /F1 9 0 R >> /Font")
                .replace("trailer", &format!("{form}trailer"));
            assert!(read(text.as_bytes(), &Options::default()).unwrap_err().contains("position change"), "{m}");
        }
    }

    #[test]
    fn a_path_with_an_unreadable_number_is_left_out_and_named() {
        // The third corner's y is a name, not a number: the path isn't drawn as
        // a triangle; it's left out and named. The good square still comes in.
        let pdf = make_pdf(&["0 0 m 100 0 l 100 /bad l 0 100 l h S 200 0 m 300 0 l 300 100 l 200 100 l h S"], false);
        let p = &read(&pdf, &Options::default()).unwrap()[0];
        assert_eq!(p.drawing.pieces.len(), 1);
        assert!(p.drawing.report.skipped.iter().any(|s| s.contains("damaged paths (1)")), "{:?}", p.drawing.report.skipped);
    }

    #[test]
    fn a_form_in_the_middle_of_a_path_leaves_the_path_whole() {
        // The caller's square goes on after the form is drawn.
        let pdf = make_pdf(&["0 0 m 100 0 l /F1 Do 100 100 l 0 100 l h S"], false);
        let body = "10 10 m 20 10 l 20 20 l h S";
        let form = format!("9 0 obj\n<< /Type /XObject /Subtype /Form /BBox [0 0 30 30] /Length {} >>\nstream\n{body}\nendstream\nendobj\n", body.len());
        let text = String::from_utf8_lossy(&pdf)
            .replace("/Resources << /Font", "/Resources << /XObject << /F1 9 0 R >> /Font")
            .replace("trailer", &format!("{form}trailer"));
        let g = build(&read(text.as_bytes(), &Options::default()).unwrap()[0]);
        assert!((g.w - 100.0 * MM_PER_PT).abs() < 1e-6, "{}", g.w);
    }

    #[test]
    fn coded_streams_need_their_ends_and_numbers_must_be_whole() {
        assert_eq!(ascii_hex(b"414 2>").unwrap(), b"AB");
        assert!(ascii_hex(b"4142").is_err());
        assert_eq!(ascii85(b"87cURD]i,\"Ebo80~>").unwrap(), b"Hello World!");
        assert!(ascii85(b"87cURD]i").is_err());
        // A stray `~` before the real end is damage, not an early end.
        assert!(ascii85(b"87cU~xRD]i,\"Ebo80~>").is_err());
        // A last group too big for 32 bits, and a hex digit that isn't one.
        assert!(ascii85(b"uu~>").is_err());
        assert!(ascii_hex(b"414g>").is_err());
        assert_eq!(ascii85(b"<~z87cURD]i,\"Ebo80~>").unwrap(), b"\0\0\0\0Hello World!");
        // "200.0.5" is no number: the rectangle is a damaged path, named.
        let pdf = make_pdf(&["0 0 m 300 0 l 300 300 l h S 0 0 200.0.5 200 re S"], false);
        let p = &read(&pdf, &Options::default()).unwrap()[0];
        assert!(p.drawing.report.skipped.iter().any(|s| s.contains("damaged paths (1)")), "{:?}", p.drawing.report.skipped);
    }

    #[test]
    fn closing_strokes_close_the_current_subpath_only() {
        // Two open three-point lines, then `s`: only the second closes.
        let pdf = make_pdf(&["0 0 m 72 0 l 72 72 l 100 0 m 172 0 l 172 72 l s"], false);
        let d = &read(&pdf, &Options::default()).unwrap()[0].drawing;
        let closed: Vec<bool> = d.pieces.iter().map(|p| p.closed).collect();
        assert_eq!(closed, vec![false, true]);
        // A page whose content reference points nowhere is refused.
        let gone = String::from_utf8_lossy(&make_pdf(&["0 0 m 72 0 l S"], false)).replace("/Contents 4 0 R", "/Contents 99 0 R");
        assert!(read(gone.as_bytes(), &Options::default()).is_err());
    }

    #[test]
    fn run_length_streams_cut_short_or_too_big_are_errors() {
        assert_eq!(run_length(&[2, b'a', b'b', b'c', 0xFE, b'x', 128]).unwrap(), b"abcxxx");
        // No end marker: cut short.
        assert!(run_length(&[2, b'a', b'b', b'c']).is_err());
        // A literal run claiming 128 bytes with 2 there.
        assert!(run_length(&[127, b'1', b'0']).is_err());
        // A repeat with nothing to repeat.
        assert!(run_length(&[0xFE]).is_err());
    }

    #[test]
    fn a_damaged_lzw_stream_is_an_error() {
        // 9-bit codes: 'A' (65), then 511, which the table can't have yet.
        let mut bits: u64 = 0;
        for c in [65u64, 511] {
            bits = (bits << 9) | c;
        }
        let data = (bits << 6).to_be_bytes()[5..].to_vec();
        assert!(lzw(&data, true).is_err());
        // A clear code, 'A', end: fine.
        let mut ok: u64 = 0;
        for c in [256u64, 65, 257] {
            ok = (ok << 9) | c;
        }
        let data = (ok << 5).to_be_bytes()[4..].to_vec();
        assert_eq!(lzw(&data, true).unwrap(), b"A");
    }

    #[test]
    fn a_page_that_cant_be_decoded_refuses_the_file() {
        // PD8: a damaged compressed content stream: refused, not imported empty.
        let mut pdf = make_pdf(&["0 0 m 72 0 l 72 72 l 0 72 l h S"], true);
        let at = pdf.windows(6).position(|w| w == b"stream").unwrap() + 7;
        for b in &mut pdf[at..at + 6] {
            *b = 0xFF;
        }
        assert!(read(&pdf, &Options::default()).unwrap_err().contains("can't be read"));
    }

    #[test]
    fn layers_that_point_back_at_themselves_dont_hang() {
        // PD8: an optional-content group whose member list holds itself.
        let pdf = make_pdf(&["/OC /P BDC 0 0 m 72 0 l 72 72 l 0 72 l h S EMC"], false);
        let mut text = String::from_utf8_lossy(&pdf).replace("/Resources << /Font", "/Resources << /Properties << /P 9 0 R >> /Font");
        // Object 9 isn't in the cross-reference table: the reader finds it by scanning.
        text = text.replace("trailer", "9 0 obj\n<< /OCGs [9 0 R] >>\nendobj\ntrailer");
        for layers in [Layers::Colour, Layers::PdfLayers] {
            let parts = read(text.as_bytes(), &Options { layers, ..Options::default() }).unwrap();
            assert_eq!(parts[0].drawing.pieces.len(), 1);
        }
    }

    #[test]
    fn a_page_unit_scales_the_drawing() {
        // UserUnit 10: one unit is 10 points, so 72 × 144 units is 254 × 508 mm.
        let pdf = String::from_utf8_lossy(&make_pdf(&["0 0 m 72 0 l 72 144 l 0 144 l h S"], false)).replace("/Type /Page /Parent", "/Type /Page /UserUnit 10 /Parent");
        let g = build(&read(pdf.as_bytes(), &Options::default()).unwrap()[0]);
        assert!((g.w - 254.0).abs() < 1e-6 && (g.h - 508.0).abs() < 1e-6, "{} x {}", g.w, g.h);
        let bad = pdf.replace("/UserUnit 10", "/UserUnit /ten");
        assert!(read(bad.as_bytes(), &Options::default()).unwrap_err().contains("unit size"));
    }

    #[test]
    fn curves_past_their_ends_and_saved_text_settings() {
        // A curve whose control points lie far beyond its ends along the chord.
        let pdf = make_pdf(&["0 0 m 300 0 -300 0 3 0 c S"], false);
        let d = &read(&pdf, &Options::default()).unwrap()[0].drawing;
        let span = d.pieces[0].pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max) - d.pieces[0].pts.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        assert!(span > 30.0, "{span}");
        // A bigger font inside q … Q doesn't carry over to the label after it.
        let pdf = make_pdf(&["0 0 m 72 0 l S BT /F1 12 Tf 10 10 Td (A) Tj ET q BT /F1 48 Tf (B) Tj ET Q BT 20 20 Td (C) Tj ET"], false);
        let d = &read(&pdf, &Options::default()).unwrap()[0].drawing;
        let c = d.texts.iter().find(|t| t.1 == "C").unwrap();
        assert!((c.3 - 12.0 * MM_PER_PT).abs() < 1e-6, "{}", c.3);
    }

    #[test]
    fn scale_option_multiplies_the_size() {
        let pdf = make_pdf(&["0 0 m 72 0 l 72 72 l 0 72 l h S"], false);
        let parts = read(&pdf, &Options { scale: 4.0, ..Options::default() }).unwrap();
        let g = build(&parts[0]);
        assert!((g.w - 101.6).abs() < 1e-6);
    }

    #[test]
    fn colours_become_layers_and_pdf_layers_can_be_used_instead() {
        let c = "0 0 0 RG 0 0 m 100 0 l 100 100 l 0 100 l h S 1 0 0 RG 10 10 m 20 10 l S 0 0 1 RG 30 30 m 40 30 l S";
        let pdf = make_pdf(&[c], false);
        let d = &read(&pdf, &Options::default()).unwrap()[0].drawing;
        let names: Vec<String> = d.layers().keys().cloned().collect();
        assert_eq!(names, vec!["Black", "Blue", "Red"]);
        let one = &read(&pdf, &Options { layers: Layers::None, ..Options::default() }).unwrap()[0].drawing;
        assert_eq!(one.layers().len(), 1);
        // Optional content: marked with an inline layer dictionary.
        let oc = "/OC << /Type /OCG /Name (ENGRAVE) >> BDC 0 0 m 10 0 l S EMC 0 0 m 0 10 l S";
        let pdf = make_pdf(&[oc], false);
        let d = &read(&pdf, &Options { layers: Layers::PdfLayers, ..Options::default() }).unwrap()[0].drawing;
        assert_eq!(d.layers().keys().cloned().collect::<Vec<_>>(), vec!["0", "ENGRAVE"]);
    }

    #[test]
    fn pages_go_side_by_side_or_become_separate_parts() {
        let a = "0 0 m 72 0 l 72 72 l 0 72 l h S";
        let pdf = make_pdf(&[a, a], false);
        let one = read(&pdf, &Options::default()).unwrap();
        assert_eq!(one.len(), 1);
        let xs: Vec<f64> = one[0].drawing.pieces.iter().map(|p| p.pts[0][0]).collect();
        // The second page starts one page width (595 pt) to the right.
        assert!((xs[1] - xs[0] - 595.0 * MM_PER_PT).abs() < 1e-6, "{xs:?}");
        let two = read(&pdf, &Options { pages: Pages::Separate, ..Options::default() }).unwrap();
        assert_eq!(two.len(), 2);
        assert!((two[1].drawing.pieces[0].pts[0][0] - two[0].drawing.pieces[0].pts[0][0]).abs() < 1e-9);
        assert_eq!(two[1].meta.name.as_deref(), Some("page 2"));
    }

    #[test]
    fn transforms_forms_and_text_are_followed() {
        // A form drawn twice, scaled ×2 by cm; text with a 12 pt font.
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let content = "q 2 0 0 2 0 0 cm /X1 Do Q q 1 0 0 1 300 0 cm /X1 Do Q BT /F1 12 Tf 10 200 Td (DEMO-001) Tj ET";
        let form = "0 0 m 10 0 l 10 10 l h S";
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /XObject << /X1 5 0 R >> /Font << /F1 6 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            format!("<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >>\nstream\n{form}\nendstream", form.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        // No cross-reference table at all: the objects are found by scanning.
        for (i, o) in objs.iter().enumerate() {
            pdf.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        pdf.extend(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
        let parts = read(&pdf, &Options::default()).unwrap();
        let d = &parts[0].drawing;
        assert_eq!(d.pieces.len(), 2);
        let big = &d.pieces[0].pts;
        assert!((big[1][0] - 20.0 * MM_PER_PT).abs() < 1e-9);
        assert!((d.pieces[1].pts[0][0] - 300.0 * MM_PER_PT).abs() < 1e-9);
        assert_eq!(d.texts.len(), 1);
        assert_eq!(d.texts[0].1, "DEMO-001");
        assert!((d.texts[0].3 - 12.0 * MM_PER_PT).abs() < 1e-9);
        assert!((d.texts[0].2[1] - 200.0 * MM_PER_PT).abs() < 1e-9);
    }

    /// PDF 1.5 layout: page objects inside an object stream that says it
    /// holds `n` objects, xref as a stream.
    fn objstm_pdf(n: usize) -> Vec<u8> {
        let content = "0 0 m 50 0 l 50 50 l 0 50 l h S";
        let inner = ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>", "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>"];
        let mut header = String::new();
        let mut body = String::new();
        for (i, o) in inner.iter().enumerate() {
            header.push_str(&format!("{} {} ", i + 1, body.len()));
            body.push_str(o);
            body.push(' ');
        }
        let stm = miniz_oxide::deflate::compress_to_vec_zlib(format!("{header}{body}").as_bytes(), 6);
        let mut pdf = b"%PDF-1.5\n".to_vec();
        let o4 = pdf.len();
        pdf.extend(format!("4 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n", content.len()).bytes());
        let o5 = pdf.len();
        pdf.extend(format!("5 0 obj\n<< /Type /ObjStm /N {n} /First {} /Filter /FlateDecode /Length {} >>\nstream\n", header.len(), stm.len()).bytes());
        pdf.extend(&stm);
        pdf.extend(b"\nendstream\nendobj\n");
        let o6 = pdf.len();
        // Rows: type(1) field2(4) field3(2), PNG "Up" predictor.
        let rows: Vec<[u8; 7]> = vec![
            [0, 0, 0, 0, 0, 0xFF, 0xFF],
            [2, 0, 0, 0, 5, 0, 0],
            [2, 0, 0, 0, 5, 0, 1],
            [2, 0, 0, 0, 5, 0, 2],
            [1, (o4 >> 24) as u8, (o4 >> 16) as u8, (o4 >> 8) as u8, o4 as u8, 0, 0],
            [1, (o5 >> 24) as u8, (o5 >> 16) as u8, (o5 >> 8) as u8, o5 as u8, 0, 0],
            [1, (o6 >> 24) as u8, (o6 >> 16) as u8, (o6 >> 8) as u8, o6 as u8, 0, 0],
        ];
        let mut raw = vec![];
        let mut prev = [0u8; 7];
        for r in &rows {
            raw.push(2);
            for i in 0..7 {
                raw.push(r[i].wrapping_sub(prev[i]));
            }
            prev = *r;
        }
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        pdf.extend(
            format!(
                "6 0 obj\n<< /Type /XRef /Size 7 /W [1 4 2] /Root 1 0 R /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 7 >> /Length {} >>\nstream\n",
                z.len()
            )
            .bytes(),
        );
        pdf.extend(&z);
        pdf.extend(format!("\nendstream\nendobj\nstartxref\n{o6}\n%%EOF\n").bytes());
        pdf
    }

    #[test]
    fn compressed_object_and_xref_streams_are_read() {
        let parts = read(&objstm_pdf(3), &Options::default()).unwrap();
        let g = build(&parts[0]);
        assert!((g.w - 50.0 * MM_PER_PT).abs() < 1e-9);
    }

    #[test]
    fn objects_found_by_scanning_keep_their_place_in_an_object_stream() {
        // No cross-reference data: the objects are found by scanning, and each
        // compressed one keeps its index position, so it is looked up directly.
        let pdf = objstm_pdf(3);
        let x = find(&pdf, b"6 0 obj", 0).unwrap();
        let mut bad = pdf[..x].to_vec();
        bad.extend(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
        let doc = Doc::open(&bad, 0).unwrap();
        let scan = doc.scan();
        for (n, i) in [(1, 0), (2, 1), (3, 2)] {
            assert!(matches!(scan.get(&n), Some(Loc::InStream(5, j)) if *j == i), "object {n}");
        }
        let parts = read(&bad, &Options::default()).unwrap();
        assert!((build(&parts[0]).w - 50.0 * MM_PER_PT).abs() < 1e-9);
    }

    #[test]
    fn an_object_stream_index_past_the_limit_refuses_the_file() {
        // Cutting the index short would lose the objects past the cut (PD8).
        let e = read(&objstm_pdf(MAX_ITEMS + 1), &Options::default()).unwrap_err();
        assert!(e.contains("nothing was imported"), "{e}");
    }

    #[test]
    fn fills_are_left_out_unless_asked() {
        let pdf = make_pdf(&["0 0 0 rg 0 0 m 72 0 l 72 72 l h f"], false);
        assert!(read(&pdf, &Options::default()).is_err());
        let parts = read(&pdf, &Options { fills: true, ..Options::default() }).unwrap();
        assert_eq!(parts[0].drawing.pieces.len(), 1);
        assert!(parts[0].drawing.pieces[0].closed);
    }

    #[test]
    fn bad_and_protected_files_are_refused_in_plain_words() {
        assert!(read(b"hello", &Options::default()).unwrap_err().contains("not a PDF"));
        let mut pdf = make_pdf(&["0 0 m 1 1 l S"], false);
        let t = rfind(&pdf, b"/Root").unwrap();
        pdf.splice(t..t, b"/Encrypt 9 0 R ".iter().copied());
        assert!(read(&pdf, &Options::default()).unwrap_err().contains("protected"));
        // Truncated garbage after the header must not panic.
        for cut in [10, 40, 120, 200] {
            let p = make_pdf(&["0 0 m 1 1 l S"], true);
            let _ = read(&p[..cut.min(p.len())], &Options::default());
        }
    }

    #[test]
    fn colour_names_are_readable() {
        assert_eq!(colour_name([0.05882, 0.05882, 0.05882]), "Black");
        assert_eq!(colour_name([1.0, 0.0, 0.0]), "Red");
        assert_eq!(colour_name([1.0, 0.5, 0.0]), "Colour #FF8000");
    }

    /// Reads every sample PDF that ships with the original program, if installed.
    #[test]
    fn samples_from_the_original_install_are_read() {
        let dir = std::env::var("LANTEK_DIR").unwrap_or_else(|_| r"C:\Lantek\Expert".to_string());
        let dir = std::path::Path::new(&dir).join("Samples").join("PDF");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
                let bytes = std::fs::read(&p).unwrap();
                let parts = read(&bytes, &Options::default()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                assert!(!parts[0].drawing.pieces.is_empty());
            }
        }
    }
}
