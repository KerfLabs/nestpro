//! GEO drawings (JOB-1): two unrelated formats share the extension.
//!
//! - **TRUMPF GEO** (TruTops): sections opened by `#~<number>` and closed by
//!   `##~~`. Points are defined once (`P`, number, x y z) and elements refer to
//!   them by number: `LIN` (two points), `ARC` (centre, start, end, direction),
//!   `CIR` (centre, radius), `TXT`. Each element's first number says what it is:
//!   cut, marking, not cut, bend line (GE3).
//! - **Pen-code GEO** (plotter style): one item per line; a single number is a
//!   pen code (cut, mark, points, pen up, end), two numbers a point to move to,
//!   five numbers an arc (GE8).
//!
//! The reader tells them apart by content (GE1).


use super::essi::{Pen, arc_between};
use super::{Meta, ReadPart};
use crate::nest::P;
use std::collections::HashMap;

pub const CUT: &str = "Cut";
pub const MARK: &str = "Mark";
pub const NOT_CUT: &str = "Not cut";
pub const BEND: &str = "Bend line";
pub const TEXT: &str = "Text";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Decide from the content (GE1).
    Auto,
    Trumpf,
    PenCode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub kind: Kind,
    /// GE5: take material and thickness from the TRUMPF header.
    pub read_material: bool,
    pub read_thickness: bool,
    /// GE3: also return the "Not cut" and "Bend line" elements (on their own
    /// layers) instead of leaving them out.
    pub keep_not_cut: bool,
    /// Join tolerance for the pen-code format: an arc whose ends are closer is a
    /// full circle (GE8).
    pub tolerance: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { kind: Kind::Auto, read_material: true, read_thickness: true, keep_not_cut: false, tolerance: 0.01 }
    }
}

fn nums(s: &str) -> Vec<f64> {
    s.split_whitespace().map(|t| t.parse::<f64>().unwrap_or(0.0)).collect()
}

/// The first `n` numbers of a line, all of them real numbers; `None` when one
/// is missing or isn't a number (a damaged record).
fn coords(s: &str, n: usize) -> Option<Vec<f64>> {
    let v: Vec<f64> = s.split_whitespace().take(n).map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite())).collect::<Option<_>>()?;
    (v.len() == n).then_some(v)
}

/// The first `n` point numbers of a line, all whole numbers; `None` when one is
/// missing or isn't a number (a damaged record, never point 0 instead).
fn refs(s: &str, n: usize) -> Option<Vec<i64>> {
    let v: Vec<i64> = s.split_whitespace().take(n).map(|t| t.parse::<i64>().ok()).collect::<Option<_>>()?;
    (v.len() == n).then_some(v)
}

fn damaged(line: usize, what: &str) -> String {
    format!("Line {line} of this GEO file is {what} that can't be read; nothing was imported, so no contour goes missing. Export the file again, or save the part as DXF.")
}

fn first_int(s: &str) -> i64 {
    s.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()).map_or(0, |v| v as i64)
}

/// GE1: a TRUMPF GEO has `#~` section lines; a plotter-command file starts with
/// two letters; anything else is read as pen codes.
fn detect(lines: &[&str]) -> Result<Kind, String> {
    if lines.iter().any(|l| l.trim_start().starts_with("#~")) {
        return Ok(Kind::Trumpf);
    }
    let first = lines.iter().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("");
    if first.len() >= 2 && first.chars().take(2).all(|c| c.is_ascii_alphabetic()) {
        return Err("This looks like a plotter-command file (PLT/PLA). That kind isn't read yet; \
                    save the part as DXF in the program that made it."
            .into());
    }
    Ok(Kind::PenCode)
}

fn element_layer(attr: i64) -> &'static str {
    // GE3: 1 cut, 2 not cut, 3 marking, 4 bend line; anything else is cut.
    match attr {
        2 => NOT_CUT,
        3 => MARK,
        4 => BEND,
        _ => CUT,
    }
}

fn read_trumpf(lines: &[&str], o: &Options) -> Result<ReadPart, String> {
    let mut meta = Meta::default();
    // GE4: header line 8 is the unit (2 = inch); GE5: lines 19 and 20 hold the
    // material and the thickness.
    let scale = if lines.get(7).map(|l| l.trim()) == Some("2") { 25.4 } else { 1.0 };
    if o.read_material {
        meta.material = lines.get(18).map(|l| l.trim().to_string()).filter(|m| !m.is_empty());
    }
    if o.read_thickness {
        // GE4: in an inch file the thickness is in inches too.
        meta.thickness = lines.get(19).and_then(|l| l.trim().parse::<f64>().ok()).filter(|t| *t > 0.0 && t.is_finite()).map(|t| t * scale);
    }
    let mut pts: HashMap<i64, P> = HashMap::new();
    let mut pen = Pen::default();
    let point = |pts: &HashMap<i64, P>, n: i64| pts.get(&n).copied().ok_or_else(|| {
        format!("The GEO file uses point {n}, which it never defines. The file is damaged or cut short; export it again.")
    });
    let line = |i: usize| lines.get(i).map(|l| l.trim()).unwrap_or("");
    let mut i = 0;
    while i < lines.len() {
        let key = line(i);
        match key {
            "P" => {
                // A point's number is a whole number; "1.5" isn't point 1.
                let n = refs(line(i + 1), 1).ok_or_else(|| damaged(i + 2, "a point number"))?[0];
                let v = coords(line(i + 2), 2).ok_or_else(|| damaged(i + 3, "a point"))?;
                pts.insert(n, [v[0] * scale, v[1] * scale]);
                i += 2;
            }
            "LIN" | "CHA" => {
                let attr = first_int(line(i + 1));
                let ids = refs(line(i + 2), 2).ok_or_else(|| damaged(i + 3, "a line's point numbers"))?;
                let a = point(&pts, ids[0])?;
                let b = point(&pts, ids[1])?;
                add(&mut pen, attr, vec![a, b], false, o);
                i += 2;
            }
            "ARC" | "FIL" => {
                // GE2: centre, start, end; a direction above 0 is counter-clockwise.
                let attr = first_int(line(i + 1));
                let ids = refs(line(i + 2), 3).ok_or_else(|| damaged(i + 3, "an arc's point numbers"))?;
                let (c, a, b) = (point(&pts, ids[0])?, point(&pts, ids[1])?, point(&pts, ids[2])?);
                let ccw = first_int(line(i + 3)) > 0;
                let arc = arc_between(a, b, c, ccw);
                let full = (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
                add(&mut pen, attr, arc, full, o);
                i += 3;
            }
            "CIR" => {
                let attr = first_int(line(i + 1));
                let c = point(&pts, refs(line(i + 2), 1).ok_or_else(|| damaged(i + 3, "a circle's centre point number"))?[0])?;
                let r = coords(line(i + 3), 1).filter(|v| v[0] > 0.0).ok_or_else(|| damaged(i + 4, "a circle radius"))?[0] * scale;
                {
                    add(&mut pen, attr, crate::dxf::arc_points(c, r, 0.0, std::f64::consts::TAU), true, o);
                }
                i += 3;
            }
            "PKT" => {
                pen.skipped("point (PKT)");
                i += 2;
            }
            "ATT" => {
                i += 2;
            }
            "TXT" => {
                // GE6: kind (3 = marked text), a number, height and width,
                // two numbers the last of which is the angle, the insertion point,
                // the number of text lines, then the lines.
                let marked = first_int(line(i + 1)) == 3;
                let size = nums(line(i + 3));
                let rot = nums(line(i + 4));
                let place = nums(line(i + 5));
                // The text lines end where the record does (`|~`): a count that runs
                // past it would read the drawing after it as text, so the file is
                // refused (GE6).
                let count = place.get(2).copied().unwrap_or(0.0).max(0.0) as usize;
                // Only the claimed lines are looked at (the work stays in step with
                // the file).
                if count > lines.len().saturating_sub(i + 6) || (0..count).any(|k| matches!(line(i + 6 + k), "|~" | "##~~")) {
                    return Err(damaged(i + 6, "a text whose line count runs past its record"));
                }
                let height = size.first().copied().unwrap_or(5.0) * scale;
                let angle = rot.get(1).copied().unwrap_or(0.0);
                let at = pts.get(&(place.first().copied().unwrap_or(0.0) as i64)).copied();
                for k in 0..count {
                    let text = line(i + 6 + k);
                    if let (Some(p), false) = (at, text.is_empty()) {
                        let (s, c) = angle.to_radians().sin_cos();
                        let down = k as f64 * height * 1.5;
                        let ins = [p[0] + s * down, p[1] - c * down];
                        let layer = if marked { MARK } else { TEXT };
                        pen.d.texts.push((layer.into(), text.to_string(), ins, height.max(0.1), angle));
                    }
                }
                i = i.saturating_add(5 + count);
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(e) = pen.refusal() {
        return Err(e);
    }
    if pen.d.pieces.is_empty() {
        return Err("No lines, arcs or circles were found in this GEO file.".into());
    }
    Ok(ReadPart { drawing: pen.d, meta, mark_layers: vec![MARK.into()] })
}

fn add(pen: &mut Pen, attr: i64, pts: Vec<P>, full: bool, o: &Options) {
    let layer = element_layer(attr);
    if (layer == NOT_CUT || layer == BEND) && !o.keep_not_cut {
        pen.skipped(if layer == BEND { "bend line" } else { "not-cut element" });
        return;
    }
    if full { pen.circle(layer, pts) } else { pen.path(layer, pts) }
}

/// GE8: pen-code GEO.
fn read_pen_code(lines: &[&str], o: &Options) -> Result<ReadPart, String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Mode {
        Up,
        Points,
        Mark,
        Cut,
        Text,
    }
    let mut mode = Mode::Up;
    let mut skip_first = false;
    let mut pos: P = [0.0, 0.0];
    let mut pen = Pen::default();
    for (n, raw) in lines.iter().enumerate() {
        let f: Vec<&str> = raw.split_whitespace().collect();
        match f.len() {
            0 => {}
            1 => match f[0].parse::<i64>() {
                Ok(5) => mode = Mode::Points,
                Ok(-5) => (mode, skip_first) = (Mode::Mark, true),
                Ok(7) | Ok(-7) => (mode, skip_first) = (Mode::Cut, true),
                Ok(-6) => mode = Mode::Text,
                Ok(2) | Ok(3) | Ok(4) => {}
                Ok(8) => mode = Mode::Up,
                Ok(9) | Ok(99) => break,
                _ => {
                    return Err(format!(
                        "Line {} of this GEO file has a code ({}) that isn't a known pen code. \
                         It may be another kind of GEO file; save the part as DXF instead.",
                        n + 1,
                        f[0]
                    ));
                }
            },
            2 => {
                if mode == Mode::Text {
                    continue;
                }
                let v = coords(raw, 2).ok_or_else(|| damaged(n + 1, "a move"))?;
                let p = [v[0], v[1]];
                if skip_first {
                    skip_first = false;
                } else {
                    match mode {
                        Mode::Points => pen.skipped("point"),
                        Mode::Mark => pen.path(MARK, vec![pos, p]),
                        Mode::Cut => pen.path(CUT, vec![pos, p]),
                        _ => {}
                    }
                }
                pos = p;
            }
            5 => {
                let v = coords(raw, 5).ok_or_else(|| damaged(n + 1, "an arc"))?;
                let (end, c, ccw) = ([v[0], v[1]], [v[2], v[3]], v[4] >= 0.0);
                let layer = match mode {
                    Mode::Mark => Some(MARK),
                    Mode::Cut => Some(CUT),
                    _ => None,
                };
                if let Some(layer) = layer {
                    let d = ((end[0] - pos[0]).powi(2) + (end[1] - pos[1]).powi(2)).sqrt();
                    if d > o.tolerance {
                        pen.path(layer, arc_between(pos, end, c, ccw));
                    } else {
                        let r = ((pos[0] - c[0]).powi(2) + (pos[1] - c[1]).powi(2)).sqrt();
                        if r > 0.0 {
                            pen.circle(layer, crate::dxf::arc_points(c, r, 0.0, std::f64::consts::TAU));
                        }
                    }
                }
                pos = end;
            }
            // 4 numbers: drawing window; 6: text; others: not drawn.
            _ => {}
        }
    }
    if let Some(e) = pen.refusal() {
        return Err(e);
    }
    if pen.d.pieces.is_empty() {
        return Err("No cut or marking lines were found in this GEO file.".into());
    }
    Ok(ReadPart { drawing: pen.d, meta: Meta::default(), mark_layers: vec![MARK.into()] })
}

/// Read a GEO file: one part.
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let kind = match opts.kind {
        Kind::Auto => detect(&lines)?,
        k => k,
    };
    let part = match kind {
        Kind::Trumpf => read_trumpf(&lines, opts)?,
        _ => read_pen_code(&lines, opts)?,
    };
    Ok(vec![part])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A made-up 100 x 60 plate: rounded top-right corner (r=10), a 20 mm hole,
    /// a marking line, a bend line and a text.
    const TRUMPF: &str = "#~1\n1.03\n1\n04.11.2026\n0 0 0\n100 60 0\n5978.5\n1\n0.001\n0\n1\n##~~\n#~11\n\n1\n\n\n\nS235\n4.000\n##~~\n\
#~31\nP\n1\n0.0 0.0 0.0\n|~\nP\n2\n100.0 0.0 0.0\n|~\nP\n3\n100.0 50.0 0.0\n|~\nP\n4\n90.0 60.0 0.0\n|~\nP\n5\n0.0 60.0 0.0\n|~\n\
P\n6\n90.0 50.0 0.0\n|~\nP\n7\n30.0 30.0 0.0\n|~\nP\n8\n60.0 20.0 0.0\n|~\nP\n9\n80.0 20.0 0.0\n|~\nP\n10\n50.0 0.0 0.0\n|~\nP\n11\n50.0 60.0 0.0\n|~\n##~~\n\
#~33\n#~331\nLIN\n1 0\n1 2\n|~\nLIN\n1 0\n2 3\n|~\nARC\n1 0\n6 3 4\n1\n|~\nLIN\n1 0\n4 5\n|~\nLIN\n1 0\n5 1\n|~\n\
CIR\n1 0\n7\n10.0\n1\n|~\nLIN\n3 0\n8 9\n|~\nLIN\n4 0\n10 11\n|~\nTXT\n1 0\n0\n5.0 1.0\n0 0\n8 0 1\nDEMO-001\n|~\n##~~\n#~END\n";

    fn roles(p: &ReadPart) -> BTreeMap<String, crate::dxf::LayerRole> {
        p.mark_layers.iter().map(|l| (l.clone(), crate::dxf::LayerRole::Mark)).collect()
    }

    #[test]
    fn trumpf_plate_reads_with_header_data() {
        let parts = read(TRUMPF.as_bytes(), &Options::default()).unwrap();
        let p = &parts[0];
        assert_eq!(p.meta.material.as_deref(), Some("S235"));
        assert_eq!(p.meta.thickness, Some(4.0));
        // The bend line is left out by default.
        assert!(p.drawing.layers().get(BEND).is_none());
        let (g, _) = crate::dxf::build(&p.drawing, &roles(p)).unwrap();
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 60.0).abs() < 1e-6);
        assert_eq!(g.holes.len(), 1);
        assert_eq!(g.marks.len(), 1);
        assert_eq!(g.texts.len(), 1);
        let a = crate::nest::area(&g.outline);
        let want = 6000.0 - (100.0 - 25.0 * std::f64::consts::PI);
        assert!((a - want).abs() < 2.0, "area {a}, want {want}");
    }

    #[test]
    fn a_text_claiming_more_lines_than_it_has_is_refused() {
        // The text's line count is a trillion (or just 2), with one real line: the
        // lines after it would be read as text, so the file is refused.
        for n in ["1000000000000", "2"] {
            let damaged = TRUMPF.replace("8 0 1\nDEMO-001", &format!("8 0 {n}\nDEMO-001"));
            assert!(read(damaged.as_bytes(), &Options::default()).unwrap_err().contains("text"));
        }
    }

    #[test]
    fn bend_lines_on_request_and_inch_files() {
        let o = Options { keep_not_cut: true, ..Options::default() };
        let p = &read(TRUMPF.as_bytes(), &o).unwrap()[0];
        assert_eq!(p.drawing.layers().get(BEND), Some(&1));
        // Header line 8 = 2: inch.
        let inch = TRUMPF.replacen("5978.5\n1\n", "5978.5\n2\n", 1);
        let p = &read(inch.as_bytes(), &Options::default()).unwrap()[0];
        let (g, _) = crate::dxf::build(&p.drawing, &BTreeMap::new()).unwrap();
        assert!((g.w - 2540.0).abs() < 1e-6, "w {}", g.w);
    }

    #[test]
    fn damaged_records_and_inch_thickness() {
        // GE4: thickness 4 in an inch file is 101.6 mm.
        let inch = TRUMPF.replacen("5978.5\n1\n", "5978.5\n2\n", 1);
        assert_eq!(read(inch.as_bytes(), &Options::default()).unwrap()[0].meta.thickness, Some(101.6));
        // A circle without its radius is refused, not left out.
        let cut = TRUMPF.replace("CIR\n1 0\n7\n10.0\n", "CIR\n1 0\n7\n\n");
        assert!(read(cut.as_bytes(), &Options::default()).unwrap_err().contains("radius"));
    }

    #[test]
    fn a_point_number_that_isnt_one_is_refused() {
        let bad = TRUMPF.replace("LIN\n1 0\n1 2\n", "LIN\n1 0\n1 BAD\n");
        assert!(read(bad.as_bytes(), &Options::default()).unwrap_err().contains("point numbers"));
        let frac = TRUMPF.replace("P\n2\n100.0 0.0 0.0\n", "P\n1.5\n100.0 0.0 0.0\n");
        assert!(read(frac.as_bytes(), &Options::default()).unwrap_err().contains("point number"));
    }

    #[test]
    fn undefined_point_is_an_error() {
        let bad = TRUMPF.replace("LIN\n1 0\n1 2\n", "LIN\n1 0\n1 42\n");
        assert!(read(bad.as_bytes(), &Options::default()).unwrap_err().contains("42"));
    }

    #[test]
    fn pen_code_geo_square_with_a_circle_and_a_mark() {
        let geo = "0 0 200 200\n7\n10 10\n110 10\n110 60\n10 60\n10 10\n8\n7\n50 35\n50 35 60 35 10\n8\n-5\n20 20\n40 20\n99\n";
        let p = &read(geo.as_bytes(), &Options::default()).unwrap()[0];
        let (g, _) = crate::dxf::build(&p.drawing, &roles(p)).unwrap();
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 50.0).abs() < 1e-6);
        assert_eq!(g.holes.len(), 1);
        assert_eq!(g.marks.len(), 1);
    }

    #[test]
    fn plotter_commands_and_bad_codes_are_refused() {
        assert!(read(b"IN;\nPU0,0;\n", &Options::default()).is_err());
        assert!(read(b"7\n0 0\n10 0\n42\n", &Options::default()).is_err());
    }
}
