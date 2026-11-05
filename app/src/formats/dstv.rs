//! DSTV NC files (`.nc1`, steel construction NC data, JOB-1): plates only.
//!
//! A DSTV file is a header block `ST` followed by data blocks, each started by a
//! two-letter code on its own line: `AK` outer contour, `IK` inner contour,
//! `BO` holes, `SI` numbering text, `PU` powder marking, `KO` marking, `SC` saw
//! cuts, `KA` bends, `EN` end. Comment lines start with `**`. Data lines hold a
//! face letter (`v` front, `h` rear, `o`/`u` flanges) followed by numbers.
//!
//! Only plates (profile code `B`) are read; beams, angles, channels and tubes are
//! refused (DS2). The part comes in millimetres as drawn on the front face.


use super::essi::{Pen, arc_between};
use super::{Meta, ReadPart};
use crate::nest::P;

pub const AK: &str = "AK outer contour";
pub const IK: &str = "IK inner contour";
pub const BO: &str = "BO holes";
pub const BO_MARK: &str = "BO marked holes";
pub const PU: &str = "PU powder marking";
pub const KO: &str = "KO marking";
pub const SI: &str = "SI numbering";

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// DS9: how to build the part name from header lines, e.g. `4` (piece
    /// mark) or `1+'-'+4` (order, a dash, piece mark). Empty = the file name.
    pub name_from: String,
    /// Character that quotes literal text in `name_from`.
    pub quote: char,
    /// DS8: take material, thickness and quantity from the header.
    pub read_material: bool,
    pub read_thickness: bool,
    pub read_quantity: bool,
    /// DS6: holes of this radius or less become points (not read); 0 = none.
    pub point_radius: f64,
    /// DS7: keep marking drawn on the rear face (as marking).
    pub rear_marking: bool,
    /// Text height for SI numbering; below 0 = the height in the file.
    pub text_height: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            name_from: String::new(),
            quote: '\'',
            read_material: true,
            read_thickness: true,
            read_quantity: true,
            point_radius: 0.0,
            rear_marking: false,
            text_height: -1.0,
        }
    }
}

/// Small tolerance used to tell "no radius" and "same point" (DS4).
const TOL: f64 = 0.01;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Block {
    None,
    Header,
    Holes,
    Text,
    Outer,
    Inner,
    Powder,
    Mark,
    Other,
}

#[derive(Debug, Clone)]
struct Pt {
    face: char,
    x: f64,
    y: f64,
    /// Letter written right after the y value (`t`/`w` notch radius).
    y_note: char,
    r: f64,
}

#[derive(Debug, Clone)]
struct Contour {
    kind: Block,
    pts: Vec<Pt>,
}

#[derive(Debug, Clone)]
struct Hole {
    face: char,
    x: f64,
    y: f64,
    /// Hole kind written right after y: `m` = marked, `g` thread, `s` countersink…
    kind: char,
    dia: f64,
    /// Letter after the depth: `l` = slotted hole.
    slot: char,
    slot_len: f64,
    slot_wid: f64,
    slot_ang: f64,
}

#[derive(Debug, Clone)]
struct Text {
    face: char,
    x: f64,
    y: f64,
    angle: f64,
    height: f64,
    text: String,
}

/// Reads the fields of a data line from column 3 on: single letters and
/// numbers written one after the other (DS3).
struct Cur<'a> {
    s: &'a [char],
    i: usize,
    /// A number on the line couldn't be read (or a needed one is missing).
    bad: bool,
}

impl Cur<'_> {
    fn ch(&mut self) -> char {
        let c = self.s.get(self.i).copied().unwrap_or(' ');
        self.i += 1;
        c
    }
    /// A number that may be left out (0 then); one written but unreadable marks
    /// the line bad.
    fn num(&mut self) -> f64 {
        self.number(false)
    }
    /// A number the record needs: missing or unreadable marks the line bad.
    fn req(&mut self) -> f64 {
        self.number(true)
    }
    fn number(&mut self, needed: bool) -> f64 {
        while self.s.get(self.i) == Some(&' ') {
            self.i += 1;
        }
        let st = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.')) {
            self.i += 1;
        }
        let t: String = self.s[st.min(self.s.len())..self.i.min(self.s.len())].iter().collect();
        if t.is_empty() {
            self.bad |= needed;
            return 0.0;
        }
        match t.parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => {
                self.bad = true;
                0.0
            }
        }
    }
    fn more(&self) -> bool {
        self.s.get(self.i + 1..).is_some_and(|r| r.iter().any(|c| *c != ' '))
    }
    fn rest(&self) -> String {
        self.s.get(self.i..).map(|r| r.iter().collect::<String>()).unwrap_or_default().trim().to_string()
    }
}

#[derive(Default)]
struct File {
    /// Header lines 1..24 (index 0 = line 1).
    head: Vec<String>,
    holes: Vec<Hole>,
    texts: Vec<Text>,
    contours: Vec<Contour>,
    skipped: Vec<String>,
}

/// The blocks of an NC file. A contour point, hole or numbering line whose
/// numbers can't be read refuses the file (a corner or hole would be missing).
fn parse(text: &str) -> Result<File, String> {
    let mut f = File::default();
    let mut block = Block::None;
    // DS3: an empty face letter or x note repeats the one of the line before.
    let mut last_face = ' ';
    for (ln, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.starts_with("**") {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() >= 2 && chars[0] != ' ' && chars[1] != ' ' {
            let code: String = chars[..2].iter().collect();
            block = match code.as_str() {
                "ST" => Block::Header,
                "EN" => Block::None,
                "BO" => Block::Holes,
                "SI" => Block::Text,
                "AK" => Block::Outer,
                "IK" => Block::Inner,
                "PU" => Block::Powder,
                "KO" => Block::Mark,
                _ => {
                    if !f.skipped.contains(&code) {
                        f.skipped.push(code.clone());
                    }
                    Block::Other
                }
            };
            if matches!(block, Block::Outer | Block::Inner | Block::Powder | Block::Mark) {
                f.contours.push(Contour { kind: block, pts: vec![] });
            }
            continue;
        }
        if chars.is_empty() && block != Block::Header {
            continue;
        }
        let mut c = Cur { s: &chars, i: 2, bad: false };
        match block {
            Block::Header => {
                if f.head.len() < 24 {
                    f.head.push(chars.get(2..).map(|r| r.iter().collect::<String>()).unwrap_or_default().trim().to_string());
                }
            }
            Block::Holes => {
                let mut face = c.ch();
                let x = c.req();
                c.ch();
                let y = c.req();
                let kind = c.ch();
                let dia = c.req();
                c.ch();
                let _depth = c.num();
                let slot = c.ch();
                let slot_len = c.num();
                c.ch();
                let slot_wid = c.num();
                c.ch();
                let slot_ang = c.num();
                if face == ' ' {
                    face = last_face;
                }
                last_face = face;
                f.holes.push(Hole { face, x, y, kind, dia, slot, slot_len, slot_wid, slot_ang });
            }
            Block::Text => {
                let mut face = c.ch();
                let x = c.req();
                c.ch();
                let y = c.req();
                c.ch();
                let angle = c.num();
                c.ch();
                let height = c.num();
                c.ch();
                let text = c.rest();
                if face == ' ' {
                    face = last_face;
                }
                last_face = face;
                f.texts.push(Text { face, x, y, angle, height, text });
            }
            Block::Outer | Block::Inner | Block::Powder | Block::Mark => {
                let mut face = c.ch();
                let x = c.req();
                c.ch();
                let y = c.req();
                let y_note = c.ch();
                let r = c.num();
                // Weld preparation (bevel angles and lands) may follow: not used.
                let _bevel = c.more();
                if face == ' ' {
                    face = last_face;
                }
                last_face = face;
                if let Some(k) = f.contours.last_mut() {
                    k.pts.push(Pt { face, x, y, y_note, r });
                }
            }
            Block::None | Block::Other => {}
        }
        if c.bad && !matches!(block, Block::Header | Block::None | Block::Other) {
            return Err(format!(
                "Line {} of this DSTV file has a number that can't be read ({}); a corner or hole would be missing, so nothing was imported. Export the plate again.",
                ln + 1,
                line.trim()
            ));
        }
    }
    Ok(f)
}

fn head(f: &File, line: usize) -> &str {
    f.head.get(line.wrapping_sub(1)).map_or("", |s| s.as_str())
}

fn head_num(f: &File, line: usize) -> f64 {
    head(f, line).trim().parse().unwrap_or(0.0)
}

/// DS9: build a name from header lines: numbers pick a header line, text
/// between quotes is copied, `+` joins.
fn name_from(f: &File, expr: &str, quote: char) -> String {
    let mut out = String::new();
    let mut num = String::new();
    let mut chars = expr.chars();
    let flush = |num: &mut String, out: &mut String| {
        if let Ok(n) = num.trim().parse::<usize>() {
            out.push_str(head(f, n));
        }
        num.clear();
    };
    while let Some(ch) = chars.next() {
        if ch == '+' {
            flush(&mut num, &mut out);
        } else if ch == quote {
            for q in chars.by_ref() {
                if q == quote {
                    break;
                }
                out.push(q);
            }
        } else {
            num.push(ch);
        }
    }
    flush(&mut num, &mut out);
    out.trim().to_string()
}

/// Front or rear face (DS3): the faces a plate is drawn on.
fn plate_face(face: char) -> bool {
    face == 'v' || face == 'h'
}

/// DS4: the arc from `a` to `b` with radius |r|: counter-clockwise when r > 0 on
/// the front face (the other way on the rear face), always the shorter arc.
fn arc_by_radius(a: P, b: P, r: f64, face: char) -> Vec<P> {
    let ccw = (r > 0.0) != (face == 'h');
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let d = (dx * dx + dy * dy).sqrt();
    if d < 1e-9 {
        return vec![a, b];
    }
    let h = d / 2.0;
    let k = (r * r - h * h).abs().sqrt();
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let left = [-dy / d, dx / d];
    let s = if ccw { 1.0 } else { -1.0 };
    let c = [mid[0] + s * k * left[0], mid[1] + s * k * left[1]];
    arc_between(a, b, c, ccw)
}

/// DS5: a slotted hole: a hole of radius `r` at `c` stretched by `len` along x
/// and `wid` along y, then turned by `ang` degrees about its centre.
fn slot(c: P, r: f64, len: f64, wid: f64, ang: f64) -> Vec<P> {
    let (s, co) = ang.to_radians().sin_cos();
    let tf = |p: P| [c[0] + p[0] * co - p[1] * s, c[1] + p[0] * s + p[1] * co];
    // Four sides (left out when shorter than the tolerance), each followed by a
    // quarter arc around one corner of the stretch, counter-clockwise.
    let sides: [(P, P, P, f64); 4] = [
        ([len, -r], [len, 0.0], [len + r, 0.0], len),
        ([len + r, wid], [len, wid], [len, wid + r], wid),
        ([0.0, wid + r], [0.0, wid], [-r, wid], len),
        ([-r, 0.0], [0.0, 0.0], [0.0, -r], wid),
    ];
    let mut pts: Vec<P> = vec![tf([0.0, -r])];
    for (side_end, corner, arc_end, side_len) in sides {
        if side_len > TOL {
            pts.push(tf(side_end));
        }
        let arc = arc_between(*pts.last().unwrap(), tf(arc_end), tf(corner), true);
        pts.extend(arc.into_iter().skip(1));
    }
    pts.pop();
    pts
}

/// Read a DSTV file: one plate.
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let text = String::from_utf8_lossy(bytes);
    let f = parse(&text)?;
    if f.head.is_empty() {
        return Err("This is not a DSTV file: it has no ST header block.".into());
    }
    // DS2: only plates.
    let code = head(&f, 8).trim().to_ascii_uppercase();
    if code != "B" {
        let what = match code.as_str() {
            "I" => "an I-beam",
            "U" => "a channel (U profile)",
            "L" => "an angle (L profile)",
            "M" => "a rectangular tube",
            "RO" => "a round tube",
            "RU" => "a round bar",
            "C" => "a C profile",
            "T" => "a T profile",
            "" => "a part without a profile code",
            _ => "a profile",
        };
        return Err(format!(
            "This DSTV file describes {what} ({} {}), not a plate. Only flat plates (profile code B) \
             can be nested; export the plate parts from the CAD program, or unfold the profile there.",
            head(&f, 7),
            code
        ));
    }
    let mut pen = Pen::default();
    for s in &f.skipped {
        pen.skipped(&format!("{s} block"));
    }

    // DS7: contours drawn on both faces keep the front one; marking on the rear
    // face is left out unless asked for.
    // Which kinds of contour each face has, gathered once (a short list).
    let mut present: Vec<(Block, char)> = vec![];
    for k in &f.contours {
        if let Some(p) = k.pts.first() {
            if !present.contains(&(k.kind, p.face)) {
                present.push((k.kind, p.face));
            }
        }
    }
    let on_face = |kind: Block, face: char| present.contains(&(kind, face));
    let mut have_outer = false;
    for k in &f.contours {
        let Some(first) = k.pts.first() else { continue };
        let face = first.face;
        if !plate_face(face) {
            continue;
        }
        let layer = match k.kind {
            Block::Outer => AK,
            Block::Inner => IK,
            Block::Powder => PU,
            _ => KO,
        };
        if face == 'h' {
            match k.kind {
                Block::Outer | Block::Inner if on_face(k.kind, 'v') => continue,
                Block::Powder | Block::Mark if !opts.rear_marking => {
                    pen.skipped("marking on the rear face");
                    continue;
                }
                _ => {}
            }
        }
        if k.kind == Block::Outer {
            have_outer = true;
        }
        let mut start: Option<P> = None;
        for (i, p) in k.pts.iter().enumerate() {
            if !plate_face(p.face) {
                continue;
            }
            // DS4: a corner written with a `t` or `w` note and a radius is a notch
            // radius, not a point of the contour.
            if matches!(p.y_note, 't' | 'w') && p.r.abs() > TOL {
                continue;
            }
            let here = [p.x, p.y];
            if let Some(a) = start {
                let prev = &k.pts[i - 1];
                let seg = if prev.r.abs() > TOL { arc_by_radius(a, here, prev.r, p.face) } else { vec![a, here] };
                pen.path(layer, seg);
            }
            start = Some(here);
        }
    }
    // DS1: a plate without an outer contour is the rectangle length x width.
    if !have_outer {
        let (l, w) = (head_num(&f, 9), head_num(&f, 10));
        if l > 0.0 && w > 0.0 {
            pen.path(AK, vec![[0.0, 0.0], [l, 0.0], [l, w], [0.0, w], [0.0, 0.0]]);
        }
    }
    // DS5, DS6: holes on the front or rear face.
    for h in &f.holes {
        if !plate_face(h.face) {
            continue;
        }
        let r = h.dia / 2.0;
        let c = [h.x, h.y];
        if h.slot == 'l' {
            let pts = slot(c, r, h.slot_len, h.slot_wid, h.slot_ang);
            pen.d.report.entities += 1;
            pen.closed_piece(BO, pts);
        } else if r <= opts.point_radius + 1e-9 {
            pen.skipped("hole too small (point)");
        } else {
            let layer = if h.kind == 'm' { BO_MARK } else { BO };
            let mut pts = crate::dxf::arc_points(c, r, 0.0, std::f64::consts::TAU);
            pts.pop();
            pen.d.report.entities += 1;
            pen.closed_piece(layer, pts);
        }
    }
    // DS10: numbering texts.
    for t in &f.texts {
        if !plate_face(t.face) || t.text.is_empty() {
            continue;
        }
        let h = if opts.text_height >= 0.0 { opts.text_height } else { t.height };
        pen.d.texts.push((SI.into(), t.text.clone(), [t.x, t.y], h.max(0.1), t.angle));
    }
    if let Some(e) = pen.refusal() {
        return Err(e);
    }
    if pen.d.pieces.is_empty() {
        return Err("This DSTV plate has no contour, holes or size to draw.".into());
    }
    let name = name_from(&f, &opts.name_from, opts.quote);
    let qty = head_num(&f, 6);
    let meta = Meta {
        name: (!name.is_empty()).then_some(name),
        material: opts.read_material.then(|| head(&f, 5).to_string()).filter(|m| !m.is_empty()),
        // DS8: the plate thickness is the web thickness (header line 13).
        thickness: opts.read_thickness.then(|| head_num(&f, 13)).filter(|t| *t > 0.0),
        qty: opts.read_quantity.then_some(qty).filter(|q| *q >= 1.0).map(|q| q as u32),
    };
    let mark_layers = [BO_MARK, PU, KO, SI].iter().map(|s| s.to_string()).collect();
    Ok(vec![ReadPart { drawing: pen.d, meta, mark_layers }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const PLATE: &str = r#"ST
** made-up demo plate
  DEMO-001
  D-10
  1
  P7
  S355J2
  3
  PL10*200
  B
    300.00
    200.00
     10.00
     10.00
     10.00
      0.00
     15.70
      0.000
      0.000
      0.000
      0.000
      0.000
  
  
  
  
AK
  v       0.00u      0.00       0.00
        300.00       0.00       0.00
        300.00     150.00      50.00
        250.00     200.00       0.00
          0.00     200.00       0.00
          0.00       0.00       0.00
IK
  v      40.00u     40.00       0.00
         80.00      40.00       0.00
         80.00      80.00       0.00
         40.00      80.00       0.00
         40.00      40.00       0.00
BO
  v     150.00s    100.00      22.00       0.00
  v     200.00s     50.00      18.00       0.00l     30.00       0.00       0.00
  v     250.00s     50.00m      6.00       0.00
SI
  v      20.00u    180.00   0.00   10r P7
KO
  v     100.00u    150.00       0.00
        150.00     150.00       0.00
EN
"#;

    fn roles(p: &ReadPart) -> BTreeMap<String, crate::dxf::LayerRole> {
        p.mark_layers.iter().map(|l| (l.clone(), crate::dxf::LayerRole::Mark)).collect()
    }

    #[test]
    fn plate_with_round_corner_window_holes_slot_and_marking() {
        let parts = read(PLATE.as_bytes(), &Options::default()).unwrap();
        let p = &parts[0];
        assert_eq!(p.meta.material.as_deref(), Some("S355J2"));
        assert_eq!(p.meta.thickness, Some(10.0));
        assert_eq!(p.meta.qty, Some(3));
        assert_eq!(p.meta.name, None);
        let (g, _) = crate::dxf::build(&p.drawing, &roles(p)).unwrap();
        assert!((g.w - 300.0).abs() < 1e-6 && (g.h - 200.0).abs() < 1e-6);
        // Window, round hole, slot; the marked hole is marking.
        assert_eq!(g.holes.len(), 3);
        assert_eq!(g.marks.len(), 2);
        assert_eq!(g.texts.len(), 1);
        // Corner rounded with r=50 (convex): area = 60000 - (2500 - 2500 pi / 4).
        let a = crate::nest::area(&g.outline);
        let want = 60000.0 - (2500.0 - 2500.0 * std::f64::consts::PI / 4.0);
        assert!((a - want).abs() < 20.0, "area {a}, want {want}");
        // Slot: 18 mm wide, 30 mm stretched: 30*18 + pi*81.
        let slot_area = g.holes.iter().map(|h| crate::nest::area(h)).find(|a| (*a - (540.0 + 81.0 * std::f64::consts::PI)).abs() < 5.0);
        assert!(slot_area.is_some(), "no slot among {:?}", g.holes.iter().map(|h| crate::nest::area(h)).collect::<Vec<_>>());
    }

    #[test]
    fn name_from_header_lines() {
        let o = Options { name_from: "1+'-'+4".into(), ..Options::default() };
        let p = &read(PLATE.as_bytes(), &o).unwrap()[0];
        assert_eq!(p.meta.name.as_deref(), Some("DEMO-001-P7"));
    }

    #[test]
    fn beams_are_refused() {
        let beam = PLATE.replace("  PL10*200\n  B\n", "  HEA200\n  I\n");
        let err = read(beam.as_bytes(), &Options::default()).unwrap_err();
        assert!(err.contains("I-beam") && err.contains("plate"), "{err}");
    }

    #[test]
    fn plate_without_contour_is_its_rectangle() {
        let bare: String = PLATE.split("AK\n").next().unwrap().to_string() + "EN\n";
        let p = &read(bare.as_bytes(), &Options::default()).unwrap()[0];
        let (g, _) = crate::dxf::build(&p.drawing, &BTreeMap::new()).unwrap();
        assert!((g.w - 300.0).abs() < 1e-6 && (g.h - 200.0).abs() < 1e-6);
    }

    #[test]
    fn positive_radius_turns_counter_clockwise() {
        // From (300,150) to (250,200) with r=50, counter-clockwise: centre (250,150).
        let pts = arc_by_radius([300.0, 150.0], [250.0, 200.0], 50.0, 'v');
        let m = pts[pts.len() / 2];
        assert!(((m[0] - 250.0).powi(2) + (m[1] - 150.0).powi(2)).sqrt() - 50.0 < 0.2);
        assert!(m[0] > 275.0 && m[1] > 175.0, "bulges outwards: {m:?}");
        let cw = arc_by_radius([300.0, 150.0], [250.0, 200.0], -50.0, 'v');
        let m = cw[cw.len() / 2];
        assert!(m[0] < 275.0 && m[1] < 175.0, "bulges inwards: {m:?}");
    }

    #[test]
    fn not_dstv_is_refused() {
        assert!(read(b"hello", &Options::default()).is_err());
    }
}
