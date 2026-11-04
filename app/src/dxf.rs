//! Import and export of part drawings as ASCII DXF.
//!
//! Rules taken from the original (see MECHANICS.md, "DXF"):
//! - joining tolerance 0.01 mm and arc smoothness 0.1 mm
//!   (D4);
//! - every drawing object is looked up by its **layer** and given a machining
//!   role; the default role (code 13) is a normal cut contour, other layers can
//!   be marking or left out (D5).
//!
//! Reads LINE, ARC, CIRCLE, LWPOLYLINE (with bulges) and old-style POLYLINE /
//! VERTEX entities from the ENTITIES section. Cut pieces are joined end-to-end
//! into closed loops; the largest loop is the outer contour and the loops inside
//! it are holes. Units are millimetres.

use crate::nest::{area, P};
use std::collections::BTreeMap;

/// Joining tolerance in mm (original default 0.01).
pub const JOIN_TOL: f64 = 0.01;
/// Largest gap between an arc and its straight segments, in mm (original default 0.1).
pub const ARC_CHORD_ERR: f64 = 0.1;

/// A part shape read from a drawing, moved so its bounding box starts at 0,0.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomGeo {
    pub outline: Vec<P>,
    pub holes: Vec<Vec<P>>,
    /// Triangles filling the outline (for drawing concave shapes).
    pub fills: Vec<Vec<P>>,
    /// Triangles filling each hole.
    pub hole_fills: Vec<Vec<P>>,
    /// Marking lines (from layers set to "Mark"), not cut through.
    pub marks: Vec<Vec<P>>,
    /// Texts from the drawing (TEXT/MTEXT), in part coordinates.
    pub texts: Vec<crate::nest::PartText>,
    pub w: f64,
    pub h: f64,
}

/// What a drawing layer becomes (original: layer → machining code; 13 = cut).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerRole {
    Cut,
    Mark,
    Ignore,
}

impl LayerRole {
    pub const ALL: [LayerRole; 3] = [LayerRole::Cut, LayerRole::Mark, LayerRole::Ignore];
    pub fn label(self) -> &'static str {
        match self {
            LayerRole::Cut => "Cut",
            LayerRole::Mark => "Mark",
            LayerRole::Ignore => "Leave out",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ImportReport {
    pub entities: usize,
    pub skipped: Vec<String>,
    pub open_pieces: usize,
    pub outside_loops: usize,
}

/// One drawn object, already turned into points.
#[derive(Debug, Clone)]
pub struct Piece {
    pub layer: String,
    pub pts: Vec<P>,
    pub closed: bool,
}

/// A drawing as read, before layer roles are applied.
#[derive(Debug, Clone, Default)]
pub struct Drawing {
    pub pieces: Vec<Piece>,
    /// Texts: (layer, text, insertion point, height, angle in degrees).
    pub texts: Vec<(String, String, P, f64, f64)>,
    pub report: ImportReport,
}

impl Drawing {
    /// Layer names with how many objects each has, in name order.
    pub fn layers(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for p in &self.pieces {
            *m.entry(p.layer.clone()).or_insert(0) += 1;
        }
        for t in &self.texts {
            *m.entry(t.0.clone()).or_insert(0) += 1;
        }
        m
    }
}

/// Number of straight segments for an arc so it stays within ARC_CHORD_ERR.
fn arc_segments(r: f64, sweep: f64) -> usize {
    let step = if r > ARC_CHORD_ERR { 2.0 * (1.0 - ARC_CHORD_ERR / r).acos() } else { std::f64::consts::FRAC_PI_2 };
    ((sweep.abs() / step).ceil() as usize).clamp(2, 720)
}

fn arc_points(c: P, r: f64, a0: f64, sweep: f64) -> Vec<P> {
    let n = arc_segments(r, sweep);
    (0..=n)
        .map(|i| {
            let a = a0 + sweep * i as f64 / n as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Points of a polyline segment with a DXF bulge (tan of a quarter of the arc angle).
fn bulge_points(p1: P, p2: P, bulge: f64) -> Vec<P> {
    if bulge.abs() < 1e-9 {
        return vec![p1, p2];
    }
    let theta = 4.0 * bulge.atan();
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        return vec![p1, p2];
    }
    let mid = [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0];
    let left = [-dy / len, dx / len];
    let d = (len / 2.0) / (theta / 2.0).tan();
    let c = [mid[0] + left[0] * d, mid[1] + left[1] * d];
    let r = ((p1[0] - c[0]).powi(2) + (p1[1] - c[1]).powi(2)).sqrt();
    let a0 = (p1[1] - c[1]).atan2(p1[0] - c[0]);
    let mut pts = arc_points(c, r, a0, theta);
    pts[0] = p1;
    let last = pts.len() - 1;
    pts[last] = p2;
    pts
}

fn close(a: P, b: P, eps: f64) -> bool {
    (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps
}

fn point_in(poly: &[P], p: P) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn signed_area(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn cleanup(mut pts: Vec<P>, eps: f64) -> Vec<P> {
    pts.dedup_by(|a, b| close(*a, *b, eps));
    if pts.len() > 1 && close(pts[0], pts[pts.len() - 1], eps) {
        pts.pop();
    }
    pts
}

/// Ear-clipping triangulation of a simple polygon.
pub fn triangulate(poly: &[P]) -> Vec<Vec<P>> {
    let mut pts: Vec<P> = poly.to_vec();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    let mut tris = Vec::new();
    let cross = |a: P, b: P, c: P| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let mut guard = 0;
    while idx.len() > 3 && guard < 100_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 1e-12 {
                continue;
            }
            let has_inside = idx.iter().any(|&k| {
                if k == ia || k == ib || k == ic {
                    return false;
                }
                let p = pts[k];
                cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
            });
            if has_inside {
                continue;
            }
            tris.push(vec![a, b, c]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break; // degenerate input: stop rather than loop forever
        }
    }
    if idx.len() == 3 {
        tris.push(vec![pts[idx[0]], pts[idx[1]], pts[idx[2]]]);
    }
    tris
}

fn push_poly(layer: &str, verts: &[(P, f64)], closed: bool, out: &mut Vec<Piece>) {
    if verts.len() < 2 {
        return;
    }
    let n = verts.len();
    let segs = if closed { n } else { n - 1 };
    let mut pts: Vec<P> = vec![verts[0].0];
    for s in 0..segs {
        let (p1, b) = verts[s];
        let p2 = verts[(s + 1) % n].0;
        pts.extend(bulge_points(p1, p2, b).into_iter().skip(1));
    }
    if closed {
        pts.pop();
    }
    out.push(Piece { layer: layer.to_string(), pts, closed });
}

/// Read every drawable object of a DXF, keeping its layer.
pub fn read(text: &str) -> Result<Drawing, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut pairs: Vec<(i32, &str)> = Vec::with_capacity(lines.len() / 2);
    let mut i = 0;
    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].parse::<i32>() {
            pairs.push((code, lines[i + 1]));
        }
        i += 2;
    }
    let start = pairs
        .windows(2)
        .position(|w| w[0] == (0, "SECTION") && w[1] == (2, "ENTITIES"))
        .ok_or("This file has no drawing (ENTITIES section) — is it an ASCII DXF?")?
        + 2;

    let mut d = Drawing::default();
    // Old-style POLYLINE state: (layer, closed, vertices).
    let mut poly: Option<(String, bool, Vec<(P, f64)>)> = None;
    let f = |v: &str| v.parse::<f64>().unwrap_or(0.0);
    let mut k = start;
    while k < pairs.len() {
        let (code, name) = pairs[k];
        if code != 0 {
            k += 1;
            continue;
        }
        if name == "ENDSEC" {
            break;
        }
        let mut j = k + 1;
        while j < pairs.len() && pairs[j].0 != 0 {
            j += 1;
        }
        let data = &pairs[k + 1..j];
        let get = |c: i32| data.iter().find(|x| x.0 == c).map(|x| f(x.1));
        let layer = data.iter().find(|x| x.0 == 8).map_or("0".to_string(), |x| x.1.to_string());
        d.report.entities += 1;
        match name {
            "LINE" => d.pieces.push(Piece {
                layer,
                pts: vec![[get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)], [get(11).unwrap_or(0.0), get(21).unwrap_or(0.0)]],
                closed: false,
            }),
            "CIRCLE" => {
                let c = [get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)];
                let r = get(40).unwrap_or(0.0);
                if r > 0.0 {
                    let mut pts = arc_points(c, r, 0.0, std::f64::consts::TAU);
                    pts.pop();
                    d.pieces.push(Piece { layer, pts, closed: true });
                }
            }
            "ARC" => {
                let c = [get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)];
                let r = get(40).unwrap_or(0.0);
                let a0 = get(50).unwrap_or(0.0).to_radians();
                let mut a1 = get(51).unwrap_or(360.0).to_radians();
                if a1 <= a0 {
                    a1 += std::f64::consts::TAU;
                }
                if r > 0.0 {
                    d.pieces.push(Piece { layer, pts: arc_points(c, r, a0, a1 - a0), closed: false });
                }
            }
            "LWPOLYLINE" => {
                let closed = get(70).is_some_and(|v| (v as i64) & 1 == 1);
                let mut verts: Vec<(P, f64)> = vec![];
                let mut cur: Option<[Option<f64>; 2]> = None;
                for &(c, v) in data {
                    match c {
                        10 => {
                            if let Some([Some(x), Some(y)]) = cur.take() {
                                verts.push(([x, y], 0.0));
                            }
                            cur = Some([Some(f(v)), None]);
                        }
                        20 => {
                            if let Some(ref mut p) = cur {
                                p[1] = Some(f(v));
                            }
                        }
                        42 => {
                            if let Some([Some(x), Some(y)]) = cur.take() {
                                verts.push(([x, y], f(v)));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some([Some(x), Some(y)]) = cur.take() {
                    verts.push(([x, y], 0.0));
                }
                push_poly(&layer, &verts, closed, &mut d.pieces);
            }
            "POLYLINE" => {
                let closed = get(70).is_some_and(|v| (v as i64) & 1 == 1);
                poly = Some((layer, closed, vec![]));
            }
            "VERTEX" => {
                if let Some((_, _, ref mut v)) = poly {
                    v.push(([get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)], get(42).unwrap_or(0.0)));
                }
            }
            "SEQEND" => {
                if let Some((l, closed, v)) = poly.take() {
                    push_poly(&l, &v, closed, &mut d.pieces);
                }
            }
            "TEXT" | "MTEXT" => {
                // Text becomes a text element (as in the original, T6);
                // MTEXT formatting codes are dropped.
                let raw = data.iter().filter(|x| x.0 == 1 || x.0 == 3).map(|x| x.1).collect::<Vec<_>>().join("");
                let mut txt = String::new();
                let mut skip = false;
                for ch in raw.chars() {
                    match ch {
                        '\\' => skip = true,
                        ';' if skip => skip = false,
                        '{' | '}' => {}
                        c if !skip => txt.push(c),
                        _ => {}
                    }
                }
                let txt = txt.trim().to_string();
                if !txt.is_empty() {
                    let h = get(40).unwrap_or(5.0).max(0.1);
                    d.texts.push((layer, txt, [get(10).unwrap_or(0.0), get(20).unwrap_or(0.0)], h, get(50).unwrap_or(0.0)));
                }
            }
            "DIMENSION" | "POINT" | "HATCH" | "INSERT" | "ATTRIB" | "ATTDEF" => {
                d.report.entities -= 1; // annotations, not geometry
            }
            other => {
                if !d.report.skipped.iter().any(|s| s == other) {
                    d.report.skipped.push(other.to_string());
                }
            }
        }
        k = j;
    }
    if d.pieces.is_empty() {
        return Err("No lines, arcs, circles or polylines were found in the drawing.".into());
    }
    Ok(d)
}

/// Turn a read drawing into one part, using a role per layer (missing = Cut).
pub fn build(d: &Drawing, roles: &BTreeMap<String, LayerRole>) -> Result<(CustomGeo, ImportReport), String> {
    let role = |l: &str| roles.get(l).copied().unwrap_or(LayerRole::Cut);
    let mut report = d.report.clone();
    let mut loops: Vec<Vec<P>> = vec![];
    let mut open: Vec<Vec<P>> = vec![];
    let mut marks: Vec<Vec<P>> = vec![];
    for p in &d.pieces {
        match role(&p.layer) {
            LayerRole::Ignore => {}
            LayerRole::Mark => {
                let mut pts = p.pts.clone();
                if p.closed {
                    pts.push(pts[0]);
                }
                marks.push(pts);
            }
            LayerRole::Cut => {
                if p.closed {
                    loops.push(p.pts.clone());
                } else {
                    open.push(p.pts.clone());
                }
            }
        }
    }
    let eps = JOIN_TOL;
    // Join open cut pieces into loops.
    while let Some(mut chain) = open.pop() {
        loop {
            let end = *chain.last().unwrap();
            if chain.len() > 2 && close(end, chain[0], eps) {
                loops.push(chain);
                break;
            }
            let next = open.iter().position(|c| close(c[0], end, eps) || close(*c.last().unwrap(), end, eps));
            match next {
                Some(n) => {
                    let mut c = open.remove(n);
                    if !close(c[0], end, eps) {
                        c.reverse();
                    }
                    chain.extend(c.into_iter().skip(1));
                }
                None => {
                    report.open_pieces += 1;
                    break;
                }
            }
        }
    }
    let loops: Vec<Vec<P>> = loops.into_iter().map(|l| cleanup(l, eps)).filter(|l| l.len() >= 3).collect();
    if loops.is_empty() {
        return Err("No closed outline on the cut layers (pieces don't join within 0.01 mm).".into());
    }
    let oi = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).partial_cmp(&area(&loops[b])).unwrap()).unwrap();
    let mut outline = loops[oi].clone();
    let mut holes = vec![];
    for (k, l) in loops.iter().enumerate() {
        if k == oi {
            continue;
        }
        if point_in(&outline, l[0]) {
            holes.push(l.clone());
        } else {
            report.outside_loops += 1;
        }
    }
    let (mut ox, mut oy, mut mx, mut my) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &outline {
        ox = ox.min(p[0]);
        oy = oy.min(p[1]);
        mx = mx.max(p[0]);
        my = my.max(p[1]);
    }
    let shift = |l: &mut Vec<P>| {
        for p in l.iter_mut() {
            p[0] -= ox;
            p[1] -= oy;
        }
    };
    shift(&mut outline);
    for h in &mut holes {
        shift(h);
    }
    for m in &mut marks {
        shift(m);
    }
    // Texts on a Mark layer are marked; texts on a cut layer stay drawing notes
    // (text can't be cut through); left-out layers drop theirs.
    let texts = d
        .texts
        .iter()
        .filter_map(|(layer, text, at, height, angle)| {
            let kind = match role(layer) {
                LayerRole::Ignore => return None,
                LayerRole::Mark => crate::nest::TextKind::Mark,
                LayerRole::Cut => crate::nest::TextKind::Note,
            };
            Some(crate::nest::PartText { text: text.clone(), at: [at[0] - ox, at[1] - oy], height: *height, angle: *angle, curve: 0.0, kind })
        })
        .collect();
    let fills = triangulate(&outline);
    let hole_fills = holes.iter().flat_map(|h| triangulate(h)).collect();
    Ok((CustomGeo { outline, holes, fills, hole_fills, marks, texts, w: mx - ox, h: my - oy }, report))
}

/// Read a DXF and build it with every layer cut.
#[allow(dead_code)]
pub fn parse(text: &str) -> Result<(CustomGeo, ImportReport), String> {
    build(&read(text)?, &BTreeMap::new())
}

/// Write a part as DXF: outer contour and holes on layer CUT, marking on MARK.
pub fn write(outline: &[P], holes: &[Vec<P>], marks: &[Vec<P>]) -> String {
    use std::fmt::Write;
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let mut poly = |pts: &[P], layer: &str, closed: bool| {
        let _ = write!(s, "0\nLWPOLYLINE\n8\n{layer}\n90\n{}\n70\n{}\n", pts.len(), if closed { 1 } else { 0 });
        for p in pts {
            let _ = write!(s, "10\n{:.4}\n20\n{:.4}\n", p[0], p[1]);
        }
    };
    poly(outline, "CUT", true);
    for h in holes {
        poly(h, "CUT", true);
    }
    for m in marks {
        poly(m, "MARK", false);
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// A small drawing used for the sample job: a plate with a rounded end, two
/// holes, and a part number to be marked (layer MARK).
pub const SAMPLE_DXF: &str = "0\nSECTION\n2\nENTITIES\n\
0\nLWPOLYLINE\n8\nCUT\n90\n6\n70\n1\n10\n0\n20\n0\n10\n180\n20\n0\n42\n1\n10\n180\n20\n120\n10\n110\n20\n120\n10\n90\n20\n80\n10\n0\n20\n80\n\
0\nCIRCLE\n8\nCUT\n10\n180\n20\n60\n40\n18\n\
0\nCIRCLE\n8\nCUT\n10\n30\n20\n40\n40\n8\n\
0\nLWPOLYLINE\n8\nMARK\n90\n4\n70\n0\n10\n60\n20\n20\n10\n60\n20\n50\n10\n75\n20\n50\n10\n75\n20\n20\n\
0\nENDSEC\n0\nEOF\n";

/// Roles for the sample drawing's layers.
pub fn sample_roles() -> BTreeMap<String, LayerRole> {
    BTreeMap::from([("MARK".to_string(), LayerRole::Mark)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_circle_make_a_plate_with_a_hole() {
        let dxf = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n10\n10\n20\n10\n11\n110\n21\n10\n\
0\nLINE\n10\n110\n20\n60\n11\n110\n21\n10\n\
0\nLINE\n10\n110\n20\n60\n11\n10\n21\n60\n\
0\nLINE\n10\n10\n20\n60\n11\n10\n21\n10\n\
0\nCIRCLE\n10\n60\n20\n35\n40\n10\n\
0\nTEXT\n1\nlabel\n\
0\nENDSEC\n0\nEOF\n";
        let (g, rep) = parse(dxf).unwrap();
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 50.0).abs() < 1e-6);
        assert_eq!(g.holes.len(), 1);
        assert!((area(&g.outline) - 5000.0).abs() < 1.0);
        assert_eq!(rep.open_pieces, 0);
        let tri: f64 = g.fills.iter().map(|t| area(t)).sum();
        assert!((tri - 5000.0).abs() < 1.0);
    }

    #[test]
    fn gaps_wider_than_the_tolerance_stay_open() {
        // Second line starts 0.05 mm away from where the first ends: > 0.01 mm.
        let dxf = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n10\n0\n20\n0\n11\n100\n21\n0\n\
0\nLINE\n10\n100.05\n20\n0\n11\n100\n21\n50\n\
0\nLINE\n10\n100\n20\n50\n11\n0\n21\n0\n\
0\nENDSEC\n0\nEOF\n";
        assert!(parse(dxf).is_err());
    }

    #[test]
    fn arcs_stay_within_the_chord_error() {
        let pts = arc_points([0.0, 0.0], 50.0, 0.0, std::f64::consts::TAU);
        for w in pts.windows(2) {
            let mid = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
            let dev = 50.0 - (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
            assert!(dev <= ARC_CHORD_ERR + 1e-9, "deviation {dev}");
        }
    }

    #[test]
    fn sample_with_bulge_and_mark_layer() {
        let d = read(SAMPLE_DXF).unwrap();
        assert_eq!(d.layers().len(), 2);
        let (g, _) = build(&d, &sample_roles()).unwrap();
        assert_eq!(g.holes.len(), 2);
        assert_eq!(g.marks.len(), 1);
        assert!((g.w - 240.0).abs() < 0.5, "w = {}", g.w);
        // Leaving the marking layer out drops the mark.
        let (g2, _) = build(&d, &BTreeMap::from([("MARK".to_string(), LayerRole::Ignore)])).unwrap();
        assert!(g2.marks.is_empty());
    }

    #[test]
    fn export_reads_back_the_same_part() {
        let (g, _) = build(&read(SAMPLE_DXF).unwrap(), &sample_roles()).unwrap();
        let text = write(&g.outline, &g.holes, &g.marks);
        let d = read(&text).unwrap();
        let (g2, _) = build(&d, &BTreeMap::from([("MARK".to_string(), LayerRole::Mark)])).unwrap();
        assert_eq!(g2.holes.len(), g.holes.len());
        assert!((area(&g2.outline) - area(&g.outline)).abs() < 0.5);
        assert_eq!(g2.marks.len(), 1);
    }

    #[test]
    fn not_a_dxf_is_refused() {
        assert!(parse("hello").is_err());
    }
}
