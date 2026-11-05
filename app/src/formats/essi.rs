//! ESSI cutting programs (ISO 6582 style word-address code, JOB-1).
//!
//! An ESSI program is a list of lines. A line holding only a number is a
//! function code (cut on/off, marking on/off, rapid on/off, comment start/end…);
//! a line with two signed numbers is a straight move, one with four signed
//! numbers and a final sign is an arc. Moves are relative to the current point
//! and in program units (ES1, ES2). Which codes switch the torch or the marker
//! depends on the control, so the codes come from a dialect (ES3).
//!
//! Every cutting move becomes a piece on the "Cut" layer and every marking move
//! a piece on the marker's layer, so the normal layer roles (Cut / Mark / Leave
//! out) apply. One file is one part, except the VULCAN dialect, which holds
//! several parts separated by sheet headers (ES9).


use super::{Meta, ReadPart};
use crate::dxf::{Drawing, Piece};
use crate::nest::P;

/// Layer for cutting moves.
pub const CUT: &str = "Cut";
/// Layers for the four marking tools (ES5).
pub const PUNCH_MARK: &str = "Punch mark";
pub const POWDER_MARK: &str = "Powder mark";
pub const PEN_MARK: &str = "Pen mark";
pub const TORCH_MARK: &str = "Torch mark";
/// Layer for label texts (ESAB inkjet labels, ES10).
pub const LABEL: &str = "Label";

/// The function codes one control uses (ES3). `None` = the control has no such code.
#[derive(Debug, Clone, PartialEq)]
pub struct Dialect {
    pub name: String,
    /// File extension the control writes (without the dot).
    pub extension: String,
    pub comment_on: Option<i32>,
    pub comment_off: Option<i32>,
    pub rapid_on: Option<i32>,
    pub rapid_off: Option<i32>,
    /// Oxyfuel torch on/off.
    pub oxy_on: Option<i32>,
    pub oxy_off: Option<i32>,
    pub plasma_on: Option<i32>,
    pub plasma_off: Option<i32>,
    pub punch_on: Option<i32>,
    pub punch_off: Option<i32>,
    pub powder_on: Option<i32>,
    pub powder_off: Option<i32>,
    pub pen_on: Option<i32>,
    pub pen_off: Option<i32>,
    /// Marking with the cutting torch at low power.
    pub torch_mark_on: Option<i32>,
    pub torch_mark_off: Option<i32>,
    /// Rapid moves give the absolute target instead of a relative move.
    pub rapid_absolute: bool,
    /// Code whose next line is a label text, and the codes that set its kind,
    /// height and angle (ES10).
    pub label_text: Option<i32>,
    pub label_kind: Option<i32>,
    pub label_height: Option<i32>,
    pub label_angle: Option<i32>,
    /// Inkjet character heights for label height index 1..6, mm.
    pub inkjet_sizes: [f64; 6],
}

impl Dialect {
    fn base(name: &str, ext: &str) -> Dialect {
        Dialect {
            name: name.into(),
            extension: ext.into(),
            comment_on: Some(3),
            comment_off: Some(4),
            rapid_on: Some(5),
            rapid_off: Some(6),
            oxy_on: Some(7),
            oxy_off: Some(8),
            plasma_on: Some(53),
            plasma_off: Some(54),
            punch_on: Some(9),
            punch_off: Some(10),
            powder_on: Some(110),
            powder_off: Some(111),
            pen_on: None,
            pen_off: None,
            torch_mark_on: None,
            torch_mark_off: None,
            rapid_absolute: false,
            label_text: None,
            label_kind: None,
            label_height: None,
            label_angle: None,
            inkjet_sizes: [0.0; 6],
        }
    }

    /// The dialects the original ships with (ES3). Unknown names give `None`.
    pub fn named(name: &str) -> Option<Dialect> {
        let d = match name.to_ascii_uppercase().as_str() {
            "ESSI" => Dialect::base("ESSI", "CNC"),
            "BURNY" => Dialect {
                rapid_on: None,
                rapid_off: None,
                plasma_on: None,
                plasma_off: None,
                powder_on: Some(13),
                powder_off: Some(14),
                ..Dialect::base("BURNY", "CNC")
            },
            "ESAB" => Dialect {
                label_text: Some(224),
                label_kind: Some(223),
                label_height: Some(225),
                label_angle: Some(226),
                inkjet_sizes: [40.0, 50.0, 60.0, 70.0, 80.0, 90.0],
                ..Dialect::base("ESAB", "MPG")
            },
            "MESSER" => Dialect { plasma_on: None, plasma_off: None, ..Dialect::base("MESSER", "LOS") },
            "VULCAN" => Dialect::base("VULCAN", "DAT"),
            _ => return None,
        };
        Some(d)
    }

    /// Names of the shipped dialects, for a picker.
    pub const NAMES: [&'static str; 5] = ["ESSI", "BURNY", "ESAB", "MESSER", "VULCAN"];

    fn is(code: i32, c: Option<i32>) -> bool {
        c == Some(code)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub dialect: Dialect,
    /// Size of one program unit in mm (ES2: 0.1 mm).
    pub unit: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { dialect: Dialect::named("ESSI").unwrap(), unit: 0.1 }
    }
}

/// One line of the program, classified by how many signs it has (ES1).
enum Line {
    /// A function code (no sign).
    Code(i32),
    /// A function code with a value, e.g. `39+1` (one sign).
    CodeValue(i32, f64),
    /// A straight move (two signs).
    Move(f64, f64),
    /// An arc: end dx, dy, centre i, j from the start, counter-clockwise (five signs).
    Arc(f64, f64, f64, f64, bool),
    /// Three or four signs: not a record the controls use; ignored (ES1) and
    /// reported.
    Odd,
    /// Anything else (text…): ignored.
    Other,
    /// A move or arc whose numbers can't be read: the positions after it would
    /// all be wrong, so the file is refused.
    Bad,
}

/// Signed numbers of a line: each starts at a `+` or `-` and runs to the next
/// sign. An empty number is 0 (`++200` = 0, 200); one that isn't a number
/// makes the whole line unreadable (`None`).
fn signed_numbers(s: &str) -> Option<Vec<(char, f64)>> {
    let mut out = Vec::new();
    let mut cur: Option<(char, String)> = None;
    for ch in s.chars() {
        if ch == '+' || ch == '-' {
            if let Some((sign, num)) = cur.take() {
                out.push((sign, number(sign, &num)?));
            }
            cur = Some((ch, String::new()));
        } else if let Some((_, ref mut num)) = cur {
            if !ch.is_whitespace() {
                num.push(ch);
            }
        }
    }
    if let Some((sign, num)) = cur {
        out.push((sign, number(sign, &num)?));
    }
    Some(out)
}

fn number(sign: char, digits: &str) -> Option<f64> {
    let v = if digits.is_empty() { 0.0 } else { digits.parse::<f64>().ok().filter(|v| v.is_finite())? };
    Some(if sign == '-' { -v } else { v })
}

fn leading_int(s: &str) -> Option<i32> {
    let t = s.trim();
    let end = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    t[..end].parse().ok()
}

fn classify(raw: &str) -> Line {
    let s = raw.trim();
    let signs = s.chars().filter(|&c| c == '+' || c == '-').count();
    match signs {
        0 => leading_int(s).map_or(Line::Other, Line::Code),
        1 => {
            let at = s.find(['+', '-']).unwrap();
            match leading_int(&s[..at]) {
                Some(code) => match signed_numbers(&s[at..]) {
                    Some(n) => Line::CodeValue(code, n.first().map_or(0.0, |x| x.1)),
                    None => Line::Other,
                },
                None => Line::Other,
            }
        }
        2 => match signed_numbers(s) {
            Some(n) => Line::Move(n[0].1, n[1].1),
            None => Line::Bad,
        },
        5 => match signed_numbers(s) {
            // ES4: the fifth sign is the direction: `-` clockwise, anything else
            // counter-clockwise.
            Some(n) => Line::Arc(n[0].1, n[1].1, n[2].1, n[3].1, n[4].0 != '-'),
            None => Line::Bad,
        },
        3 | 4 => Line::Odd,
        _ => Line::Other,
    }
}

/// Points of an arc from `a` to `b` around `c`, within the drawing's chord error.
/// Equal ends make a full circle. The radius is the mean of both end distances.
pub(super) fn arc_between(a: P, b: P, c: P, ccw: bool) -> Vec<P> {
    use std::f64::consts::TAU;
    let ra = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
    let rb = ((b[0] - c[0]).powi(2) + (b[1] - c[1]).powi(2)).sqrt();
    let r = (ra + rb) / 2.0;
    if r < 1e-9 {
        return vec![a, b];
    }
    let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let a1 = (b[1] - c[1]).atan2(b[0] - c[0]);
    let full = (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
    let mut sweep = a1 - a0;
    if ccw {
        while sweep <= 1e-12 {
            sweep += TAU;
        }
        if full {
            sweep = TAU;
        }
    } else {
        while sweep >= -1e-12 {
            sweep -= TAU;
        }
        if full {
            sweep = -TAU;
        }
    }
    let mut pts = crate::dxf::arc_points(c, r, a0, sweep);
    pts[0] = a;
    let last = pts.len() - 1;
    pts[last] = b;
    pts
}

/// Collects drawn moves, chaining a move onto the previous piece of the same
/// layer when it starts where that one ended.
pub(super) struct Pen {
    pub d: Drawing,
    last: Option<usize>,
    /// Points drawn so far and how many the file may have; past that the file
    /// is refused whole (no part comes in short of a contour).
    pub points: usize,
    pub limit: usize,
}

/// Most points one ESSI, DSTV or GEO file may give.
pub(super) const MAX_POINTS: usize = 5_000_000;

impl Default for Pen {
    fn default() -> Self {
        Pen { d: Drawing::default(), last: None, points: 0, limit: MAX_POINTS }
    }
}

impl Pen {
    /// Count `n` more points; false once the file is over its limit.
    fn take(&mut self, n: usize) -> bool {
        self.points = self.points.saturating_add(n);
        self.points <= self.limit
    }

    pub fn over(&self) -> bool {
        self.points > self.limit
    }

    /// The message when the file went over its limit.
    pub fn refusal(&self) -> Option<String> {
        self.over().then(|| {
            "This drawing is bigger than the import can take whole (more than 5 million points). \
             Nothing was imported, so no contour goes missing. Simplify or split it, or export it as DXF."
                .to_string()
        })
    }

    /// A closed piece drawn as it is (holes, circles).
    pub fn closed_piece(&mut self, layer: &str, pts: Vec<P>) {
        if !self.take(pts.len()) {
            return;
        }
        self.d.pieces.push(Piece { layer: layer.to_string(), pts, closed: true });
        self.last = None;
    }

    pub fn path(&mut self, layer: &str, pts: Vec<P>) {
        if !self.take(pts.len()) || pts.len() < 2 {
            return;
        }
        self.d.report.entities += 1;
        if let Some(i) = self.last {
            let p = &mut self.d.pieces[i];
            let end = *p.pts.last().unwrap();
            if !p.closed && p.layer == layer && (end[0] - pts[0][0]).abs() < 1e-9 && (end[1] - pts[0][1]).abs() < 1e-9 {
                p.pts.extend(pts.into_iter().skip(1));
                return;
            }
        }
        self.d.pieces.push(Piece { layer: layer.to_string(), pts, closed: false });
        self.last = Some(self.d.pieces.len() - 1);
    }

    pub fn circle(&mut self, layer: &str, mut pts: Vec<P>) {
        pts.pop();
        if !self.take(pts.len()) || pts.len() < 3 {
            return;
        }
        self.d.report.entities += 1;
        self.d.pieces.push(Piece { layer: layer.to_string(), pts, closed: true });
        self.last = None;
    }

    pub fn skipped(&mut self, what: &str) {
        if !self.d.report.skipped.iter().any(|s| s == what) {
            self.d.report.skipped.push(what.to_string());
        }
    }
}

/// Read one ESSI program (a whole file, or one VULCAN part) with `limit`
/// points left for it; also gives the points it used.
fn read_program(lines: &[&str], o: &Options, limit: usize) -> Result<(Drawing, usize), String> {
    let dl = &o.dialect;
    let mut pen = Pen { limit, ..Pen::default() };
    let mut odd = 0usize;
    // Position in program units (ES2: scaled only when drawn, so long programs
    // don't drift).
    let mut pos = [0.0f64; 2];
    let (mut comment, mut rapid) = (false, false);
    let (mut cutting, mut punch, mut powder, mut pen_mark, mut torch_mark) = (false, false, false, false, false);
    // Whether anything was drawn since the last tool was switched on (ES6).
    let mut moved = false;
    let mut label_next = false;
    let (mut label_kind, mut label_height, mut label_angle) = (0.0f64, 0.0f64, 0.0f64);
    let u = o.unit;
    let at = |p: [f64; 2]| [p[0] * u, p[1] * u];
    for (ln, raw) in lines.iter().enumerate() {
        if pen.over() {
            break;
        }
        if label_next {
            // ES10: the line after the label code is the label text.
            label_next = false;
            let text = raw.trim();
            if label_kind == 2.0 && !text.is_empty() {
                let idx = label_height as usize;
                let h = if (1..=6).contains(&idx) { dl.inkjet_sizes[idx - 1] } else { label_height };
                let a = (label_angle / 100.0).to_radians();
                let p = at(pos);
                let ins = [p[0] + a.sin() * h * 0.5, p[1] - a.cos() * h * 0.5];
                pen.d.texts.push((LABEL.to_string(), text.to_string(), ins, h.max(0.1), label_angle / 100.0));
            }
            continue;
        }
        match classify(raw) {
            Line::Code(c) => {
                if Dialect::is(c, dl.label_text) {
                    label_next = true;
                } else if Dialect::is(c, dl.comment_on) {
                    comment = true;
                } else if Dialect::is(c, dl.comment_off) {
                    comment = false;
                } else if comment {
                    // ES7: inside a comment only the comment-end code counts.
                } else if Dialect::is(c, dl.rapid_on) {
                    rapid = true;
                } else if Dialect::is(c, dl.rapid_off) {
                    rapid = false;
                } else if Dialect::is(c, dl.oxy_on) || Dialect::is(c, dl.plasma_on) {
                    cutting = true;
                    moved = false;
                } else if Dialect::is(c, dl.oxy_off) || Dialect::is(c, dl.plasma_off) {
                    if cutting && !moved {
                        pen.skipped("pierce point (torch on/off without a move)");
                    }
                    cutting = false;
                } else if Dialect::is(c, dl.punch_on) {
                    punch = true;
                    moved = false;
                } else if Dialect::is(c, dl.punch_off) {
                    if punch && !moved {
                        pen.skipped("punch-mark point");
                    }
                    punch = false;
                } else if Dialect::is(c, dl.powder_on) {
                    powder = true;
                    moved = false;
                } else if Dialect::is(c, dl.powder_off) {
                    powder = false;
                } else if Dialect::is(c, dl.pen_on) {
                    pen_mark = true;
                    moved = false;
                } else if Dialect::is(c, dl.pen_off) {
                    pen_mark = false;
                } else if Dialect::is(c, dl.torch_mark_on) {
                    torch_mark = true;
                    moved = false;
                } else if Dialect::is(c, dl.torch_mark_off) {
                    torch_mark = false;
                }
                // Other codes (kerf left/right/off, tool or kerf table, end of
                // program…) don't change the shape (ES8).
            }
            Line::CodeValue(c, v) => {
                if Dialect::is(c, dl.label_kind) {
                    label_kind = v;
                } else if Dialect::is(c, dl.label_height) {
                    label_height = v;
                } else if Dialect::is(c, dl.label_angle) {
                    label_angle = v;
                }
            }
            Line::Move(dx, dy) => {
                if comment {
                    continue;
                }
                let marking = punch || powder || pen_mark || torch_mark;
                // ES6: a straight move is drawn when the torch is on outside a
                // rapid, or when a marker is on (markers draw even in rapid).
                if (!rapid && cutting) || marking {
                    let layer = layer_for(cutting, torch_mark, pen_mark, punch);
                    pen.path(layer, vec![at(pos), at([pos[0] + dx, pos[1] + dy])]);
                    moved = true;
                }
                if rapid && dl.rapid_absolute {
                    pos = [dx, dy];
                } else {
                    pos = [pos[0] + dx, pos[1] + dy];
                }
            }
            Line::Arc(dx, dy, i, j, ccw) => {
                if comment {
                    continue;
                }
                // ES6: arcs are always drawn; on the marker's layer when only a
                // marker is on, otherwise as a cut.
                let marking = punch || powder || pen_mark || torch_mark;
                let layer = if marking { layer_for(cutting, torch_mark, pen_mark, punch) } else { CUT };
                let a = at(pos);
                let b = at([pos[0] + dx, pos[1] + dy]);
                let c = at([pos[0] + i, pos[1] + j]);
                let pts = arc_between(a, b, c, ccw);
                if dx == 0.0 && dy == 0.0 {
                    pen.circle(layer, pts);
                } else {
                    pen.path(layer, pts);
                }
                moved = true;
                pos = [pos[0] + dx, pos[1] + dy];
            }
            Line::Odd if comment => {}
            Line::Odd => odd += 1,
            Line::Other => {}
            // ES7: inside a comment only its start and end codes count.
            Line::Bad if comment => {}
            Line::Bad => {
                return Err(format!(
                    "Line {} of this ESSI program is a move whose numbers can't be read ({}); every position after it would be wrong, so nothing was imported.",
                    ln + 1,
                    raw.trim()
                ));
            }
        }
    }
    if let Some(e) = pen.refusal() {
        return Err(e);
    }
    if odd > 0 {
        pen.d.report.skipped.push(format!("{odd} line(s) with 3 or 4 numbers (no ESSI record)"));
    }
    Ok((pen.d, pen.points))
}

/// ES5: the torch wins over a marker; then torch-marking, pen, punch, powder.
fn layer_for(cutting: bool, torch_mark: bool, pen_mark: bool, punch: bool) -> &'static str {
    if cutting {
        CUT
    } else if torch_mark {
        TORCH_MARK
    } else if pen_mark {
        PEN_MARK
    } else if punch {
        PUNCH_MARK
    } else {
        POWDER_MARK
    }
}

const MARK_LAYERS: [&str; 5] = [PUNCH_MARK, POWDER_MARK, PEN_MARK, TORCH_MARK, LABEL];

fn part(drawing: Drawing, meta: Meta) -> ReadPart {
    ReadPart { drawing, meta, mark_layers: MARK_LAYERS.iter().map(|s| s.to_string()).collect() }
}

/// A line ` KEY: value` of a VULCAN sheet header (ES9).
fn header_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(' ')?.strip_prefix(key).map(|v| v.trim())
}

/// ES9: a VULCAN file holds several parts. Each ends at a line ` 0`; the file
/// ends at ` NO MORE SHEETS`. A part's header gives ` JOB: name`,
/// ` SHEET NO. n`, ` GAUGE: thickness` and ` MATRL: material`; the part is
/// called `name-n`.
fn read_vulcan(lines: &[&str], o: &Options) -> Result<Vec<ReadPart>, String> {
    let mut parts = Vec::new();
    // One point budget for the whole file, shared by its parts.
    let mut total = 0usize;
    let mut start = 0;
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim_end();
        let file_end = l.starts_with(" NO MORE SHEETS");
        let part_end = l.len() >= 2 && l.starts_with(" 0");
        if file_end || part_end || i + 1 == lines.len() {
            let chunk = &lines[start..=i];
            let (mut job, mut sheet, mut gauge, mut matrl) = (None, None, None, None);
            for c in chunk {
                let c = c.trim_end();
                if job.is_none() {
                    job = header_value(c, "JOB:").map(str::to_string);
                }
                if sheet.is_none() {
                    sheet = header_value(c, "SHEET NO.").map(|v| v.replace(' ', "").parse::<i64>().unwrap_or(0));
                }
                if gauge.is_none() {
                    gauge = header_value(c, "GAUGE:").and_then(|v| v.replace(' ', "").parse::<f64>().ok());
                }
                if matrl.is_none() {
                    matrl = header_value(c, "MATRL:").map(str::to_string);
                }
            }
            let (d, used) = read_program(chunk, o, MAX_POINTS.saturating_sub(total))?;
            total += used;
            if !d.pieces.is_empty() {
                let name = job.map(|j| format!("{}-{}", j, sheet.unwrap_or(0)));
                let meta = Meta {
                    name,
                    material: matrl.filter(|m| !m.is_empty()),
                    thickness: gauge.filter(|t| *t > 0.0),
                    qty: None,
                };
                parts.push(part(d, meta));
            }
            start = i + 1;
            if file_end {
                break;
            }
        }
        i += 1;
    }
    Ok(parts)
}

/// Read an ESSI file into one part (or several for the VULCAN dialect).
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    if opts.unit <= 0.0 {
        return Err("The ESSI unit size must be more than 0 mm (usually 0.1).".into());
    }
    let parts = if opts.dialect.name.eq_ignore_ascii_case("VULCAN") {
        read_vulcan(&lines, opts)?
    } else {
        let (d, _) = read_program(&lines, opts, MAX_POINTS)?;
        if d.pieces.is_empty() { vec![] } else { vec![part(d, Meta::default())] }
    };
    if parts.is_empty() {
        return Err(format!(
            "No cutting or marking moves were found in this file with the {} codes. \
             Check that it is an ESSI program and pick the dialect of the control that wrote it.",
            opts.dialect.name
        ));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn build(p: &ReadPart) -> crate::dxf::CustomGeo {
        let roles: BTreeMap<String, crate::dxf::LayerRole> =
            p.mark_layers.iter().map(|l| (l.clone(), crate::dxf::LayerRole::Mark)).collect();
        crate::dxf::build(&p.drawing, &roles).unwrap().0
    }

    #[test]
    fn a_move_that_cant_be_read_refuses_the_file() {
        // ES1: "+100+oops" would move by 0 in y and shift every later position.
        let e = read(b"%\n7\n+1000+0\n+0+oops\n-1000+0\n8\n", &Options::default()).unwrap_err();
        assert!(e.contains("Line 4"), "{e}");
        // Three numbers isn't an ESSI record: ignored, and said so.
        let p = read(b"%\n7\n+1000+0\n+1+2+3\n+0+500\n-1000+0\n+0-500\n8\n", &Options::default()).unwrap();
        assert!(p[0].drawing.report.skipped.iter().any(|s| s.contains("3 or 4")));
        // ES7: inside a comment nothing is read, so odd text there is fine.
        let p = read(b"%\n3\nCHECK +X+Y\n4\n7\n+1000+0\n+0+500\n-1000+0\n+0-500\n8\n", &Options::default()).unwrap();
        assert!(p[0].drawing.report.skipped.is_empty());
    }

    #[test]
    fn plate_with_a_round_corner_and_a_hole() {
        // 100 x 50 mm, top right corner rounded r=10 (clockwise travel), and a
        // 20 mm hole: 1/10 mm units.
        let prog = "%\n3\nDEMO-001 plate\n4\n5\n+200+200\n6\n7\n+0+500\n+900+0\n+100-100+0-100-\n+0-400\n-1000+0\n8\n\
                    5\n+300+250\n6\n7\n+0+0+100+0+\n8\n0\n";
        let parts = read(prog.as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts.len(), 1);
        let g = build(&parts[0]);
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 50.0).abs() < 1e-6, "{} x {}", g.w, g.h);
        assert_eq!(g.holes.len(), 1);
        // Corner rounded: area = 5000 - (100 - 25 pi) = 4978.5
        let a = crate::nest::area(&g.outline);
        assert!((a - (5000.0 - (100.0 - 25.0 * std::f64::consts::PI))).abs() < 1.0, "area {a}");
        let hole = crate::nest::area(&g.holes[0]);
        assert!((hole - 100.0 * std::f64::consts::PI).abs() < 6.0, "hole {hole}");
    }

    #[test]
    fn minus_arc_is_clockwise_plus_is_counter_clockwise() {
        // Quarter arc from (0,0) to (10,-10) around (0,-10).
        let cw = arc_between([0.0, 0.0], [10.0, -10.0], [0.0, -10.0], false);
        let ccw = arc_between([0.0, 0.0], [10.0, -10.0], [0.0, -10.0], true);
        let mid = |p: &Vec<P>| p[p.len() / 2];
        assert!(mid(&cw)[0] > 0.0, "clockwise goes right first: {:?}", mid(&cw));
        assert!(mid(&ccw)[0] < 0.0, "counter-clockwise goes the long way: {:?}", mid(&ccw));
        match classify("+450-450+0-450-") {
            Line::Arc(dx, dy, i, j, ccw) => assert!((dx, dy, i, j, ccw) == (450.0, -450.0, 0.0, -450.0, false)),
            _ => panic!("not an arc"),
        }
    }

    #[test]
    fn marking_goes_on_its_own_layer_and_rapids_are_not_drawn() {
        let prog = "7\n+1000+0\n+0+500\n-1000+0\n+0-500\n8\n5\n+100+100\n6\n110\n+200+0\n111\n5\n+0+100\n6\n0\n";
        let p = &read(prog.as_bytes(), &Options::default()).unwrap()[0];
        let layers = p.drawing.layers();
        assert_eq!(layers.get(CUT), Some(&1));
        assert_eq!(layers.get(POWDER_MARK), Some(&1));
        assert_eq!(layers.len(), 2);
        let g = build(p);
        assert_eq!(g.marks.len(), 1);
    }

    #[test]
    fn comments_are_skipped_and_empty_numbers_are_zero() {
        let prog = "3\n7\n+500+500\n4\n7\n++1000\n+1000+\n+-1000\n-1000+\n8\n";
        let p = &read(prog.as_bytes(), &Options::default()).unwrap()[0];
        let g = build(p);
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 100.0).abs() < 1e-6);
    }

    #[test]
    fn burny_has_no_rapid_codes_and_other_powder_codes() {
        let o = Options { dialect: Dialect::named("burny").unwrap(), ..Options::default() };
        let prog = "+100+100\n7\n+500+0\n+0+500\n-500+0\n+0-500\n8\n13\n+100+0\n14\n";
        let p = &read(prog.as_bytes(), &o).unwrap()[0];
        assert_eq!(p.drawing.layers().get(POWDER_MARK), Some(&1));
    }

    #[test]
    fn vulcan_file_gives_one_part_per_sheet() {
        let o = Options { dialect: Dialect::named("VULCAN").unwrap(), ..Options::default() };
        let one = "7\n+1000+0\n+0+500\n-1000+0\n+0-500\n8\n";
        let prog = format!(
            "3\n JOB: DEMO\n SHEET NO. 1\n GAUGE: 6.0\n MATRL: S235\n4\n{one} 0\n3\n JOB: DEMO\n SHEET NO. 2\n4\n{one} 0\n NO MORE SHEETS\n"
        );
        let parts = read(prog.as_bytes(), &o).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].meta.name.as_deref(), Some("DEMO-1"));
        assert_eq!(parts[0].meta.thickness, Some(6.0));
        assert_eq!(parts[0].meta.material.as_deref(), Some("S235"));
        assert_eq!(parts[1].meta.name.as_deref(), Some("DEMO-2"));
    }

    #[test]
    fn a_file_without_moves_is_refused() {
        assert!(read(b"%\n39+1\n0\n", &Options::default()).is_err());
        assert!(read(b"hello world", &Options::default()).is_err());
    }

    /// Every ESSI sample of the original's install (skipped when it isn't there).
    #[test]
    fn samples_from_the_install_import() {
        let dir = std::path::PathBuf::from(std::env::var("LANTEK_DIR").unwrap_or_else(|_| r"C:\Lantek\Expert".into()))
            .join("Samples")
            .join("ESSI");
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for e in entries.flatten() {
            let bytes = std::fs::read(e.path()).unwrap();
            let parts = read(&bytes, &Options::default()).unwrap_or_else(|err| panic!("{:?}: {err}", e.path()));
            for p in &parts {
                crate::dxf::build(&p.drawing, &BTreeMap::new()).unwrap_or_else(|err| panic!("{:?}: {err}", e.path()));
            }
        }
    }
}
