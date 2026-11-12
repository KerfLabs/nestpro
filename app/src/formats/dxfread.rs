//! The DXF reader's objects (JOB-20): every drawing object of the ENTITIES and
//! BLOCKS sections becomes the same object the DWG reader makes, and both go
//! through one geometry step — object planes (DW7, a −Z extrusion mirrors),
//! block inserts (DW8), ellipses (DW9), splines (DW10), texts (DW11), smoothed
//! and 3D polylines (DW12). Only model space is read (DW6, DX1).

use super::dwg::geom::{Ctx, Db, Mat};
use super::dwg::objects::{Kind, Obj, Spline, V3};
use crate::dxf::Drawing;
use std::collections::HashMap;

/// Reading choices (the DWG reader's, DW6 / DW8 / DW13).
#[derive(Debug, Clone)]
pub struct Options {
    /// Also read paper space (title blocks, sheet borders).
    pub paper_space: bool,
    pub explode_blocks: bool,
    pub skip_invisible: bool,
    /// mm per drawing unit for a file that doesn't say (`$INSUNITS`); 1 = mm.
    pub unit_mm: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { paper_space: false, explode_blocks: true, skip_invisible: false, unit_mm: 1.0 }
    }
}

/// Group codes of one object: (code, value) in file order.
struct Rec<'a> {
    name: &'a str,
    data: &'a [(i32, &'a str)],
}

impl<'a> Rec<'a> {
    fn f(&self, c: i32) -> Option<f64> {
        self.data.iter().find(|x| x.0 == c).and_then(|x| x.1.parse::<f64>().ok()).filter(|v| v.is_finite())
    }
    /// A coordinate, size or angle (codes 10–59, 210–239) written but not a
    /// number: the object is damaged (left out and named, DW5), never drawn
    /// with a 0 in its place. A value left out keeps its default.
    fn has_bad_number(&self) -> bool {
        self.data.iter().any(|&(c, v)| ((10..=59).contains(&c) || (210..=239).contains(&c)) && !v.parse::<f64>().is_ok_and(f64::is_finite))
    }
    /// Whether a code the object needs is left out (a LINE without its end's
    /// y, a CIRCLE without its radius…): the object is damaged (DW5).
    fn lacks(&self, codes: &[i32]) -> bool {
        codes.iter().any(|&c| !self.data.iter().any(|x| x.0 == c))
    }
    fn n(&self, c: i32, def: f64) -> f64 {
        self.f(c).unwrap_or(def)
    }
    fn i(&self, c: i32) -> i64 {
        self.f(c).map_or(0, |v| v as i64)
    }
    fn s(&self, c: i32) -> Option<&'a str> {
        self.data.iter().find(|x| x.0 == c).map(|x| x.1)
    }
    fn p(&self, c: i32) -> V3 {
        [self.n(c, 0.0), self.n(c + 10, 0.0), self.n(c + 20, 0.0)]
    }
    /// Extrusion direction (210/220/230), default +Z.
    fn ext(&self) -> V3 {
        [self.n(210, 0.0), self.n(220, 0.0), self.n(230, 1.0)]
    }
    /// Every value of a code, in order (spline knots, weights).
    fn all(&self, c: i32) -> Vec<f64> {
        self.data.iter().filter(|x| x.0 == c).filter_map(|x| x.1.parse::<f64>().ok()).collect()
    }
    /// Every point of a code (spline control / fit points): x from `c`, y from
    /// c+10, z from c+20 (z may be left out). `None` when a point has no y, or
    /// a y with no x before it: the object is damaged (DW5), never drawn with a
    /// 0 in place of a missing coordinate.
    fn points(&self, c: i32) -> Option<Vec<V3>> {
        let mut out: Vec<V3> = vec![];
        let mut has_y = true;
        for &(code, v) in self.data {
            let v = v.parse::<f64>().unwrap_or(0.0);
            if code == c {
                if !has_y {
                    return None;
                }
                out.push([v, 0.0, 0.0]);
                has_y = false;
            } else if code == c + 10 {
                let p = out.last_mut().filter(|_| !has_y)?;
                p[1] = v;
                has_y = true;
            } else if code == c + 20 {
                if let Some(p) = out.last_mut() {
                    p[2] = v;
                }
            }
        }
        has_y.then_some(out)
    }
}

/// Split the group-code pairs from `start` into objects (each starts at code 0)
/// until ENDSEC.
fn records<'a>(pairs: &'a [(i32, &'a str)], start: usize) -> Vec<Rec<'a>> {
    let mut out = vec![];
    let mut k = start;
    while k < pairs.len() {
        if pairs[k].0 != 0 {
            k += 1;
            continue;
        }
        if pairs[k].1 == "ENDSEC" {
            break;
        }
        let mut j = k + 1;
        while j < pairs.len() && pairs[j].0 != 0 {
            j += 1;
        }
        out.push(Rec { name: pairs[k].1, data: &pairs[k + 1..j] });
        k = j;
    }
    out
}

fn section(pairs: &[(i32, &str)], name: &str) -> Option<usize> {
    pairs.windows(2).position(|w| w[0] == (0, "SECTION") && w[1] == (2, name)).map(|i| i + 2)
}

struct Builder {
    db: Db,
    next: u64,
    layer_ids: HashMap<String, u64>,
    block_ids: HashMap<String, u64>,
}

impl Builder {
    fn handle(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn layer(&mut self, r: &Rec) -> u64 {
        let name = r.s(8).unwrap_or("0").to_string();
        if let Some(&h) = self.layer_ids.get(&name) {
            return h;
        }
        let h = self.handle();
        self.db.layers.insert(h, name.clone());
        self.layer_ids.insert(name, h);
        h
    }

    fn block(&mut self, name: &str) -> u64 {
        let key = name.to_uppercase();
        if let Some(&h) = self.block_ids.get(&key) {
            return h;
        }
        let h = self.handle();
        self.block_ids.insert(key, h);
        h
    }

    fn add(&mut self, kind: Kind, r: &Rec, owner: Option<u64>, entmode: u8) -> u64 {
        let handle = self.handle();
        let layer = Some(self.layer(r));
        let invisible = r.i(60) == 1;
        if let Some(o) = owner {
            self.db.owned.entry(o).or_default().push(handle);
        }
        // DX3: the object's own line type (6), when it isn't its layer's.
        if let Some(t) = r.s(6).map(str::trim).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("BYLAYER")) {
            self.db.ltypes.insert(handle, t.to_string());
        }
        self.db.order.push(handle);
        self.db.objs.insert(handle, Obj { handle, kind, entmode, owner, layer, invisible, next: None });
        handle
    }

    /// The drawing's layer and line type tables (DX3): which layers are frozen
    /// or off, each layer's line type, and which line types have dashes.
    fn tables(&mut self, recs: &[Rec]) {
        let mut broken = std::collections::HashSet::new();
        let mut any_ltype = false;
        for r in recs {
            match r.name {
                "LTYPE" => {
                    any_ltype = true;
                    // 73 = number of dash elements; 0 is a solid line.
                    if let Some(n) = r.s(2).filter(|_| r.i(73) > 0) {
                        broken.insert(n.trim().to_ascii_uppercase());
                    }
                }
                "LAYER" => {
                    let Some(name) = r.s(2) else { continue };
                    let info = super::dwg::geom::LayerInfo {
                        frozen: r.i(70) & 1 != 0,
                        // A negative colour number is how a layer is turned off.
                        off: r.i(62) < 0,
                        ltype: r.s(6).unwrap_or("").trim().to_string(),
                    };
                    self.db.layer_info.insert(name.chars().take(255).collect::<String>().to_uppercase(), info);
                }
                _ => {}
            }
        }
        if any_ltype {
            self.db.broken = Some(broken);
        }
    }

    /// The objects of one section; `owner` = the block they belong to, if any.
    fn entities(&mut self, recs: &[Rec], mut owner: Option<u64>, in_blocks: bool) {
        // An old-style POLYLINE collects its VERTEX objects until SEQEND.
        let mut poly: Option<(usize, Vec<u64>, bool)> = None;
        for (idx, r) in recs.iter().enumerate() {
            // DX1: paper-space objects (code 67 = 1) are marked so they're left out.
            let entmode = if owner.is_some() { 0 } else if r.i(67) == 1 { 1 } else { 2 };
            match r.name {
                "BLOCK" if in_blocks => {
                    let name = r.s(2).or(r.s(3)).unwrap_or("");
                    let h = self.block(name);
                    // External references (flag 4) bring no geometry of their own.
                    if r.i(70) & 4 == 0 {
                        self.db.blocks.insert(h, (name.to_string(), r.p(10)));
                    }
                    owner = Some(h);
                }
                "ENDBLK" if in_blocks => owner = None,
                "VERTEX" => {
                    if let Some((_, v, bad)) = poly.as_mut() {
                        *bad |= r.has_bad_number() || r.lacks(&[10, 20]);
                        let kind = Kind::Vertex { flags: r.i(70) as u8, pt: r.p(10), bulge: r.n(42, 0.0) };
                        let h = self.add(kind, r, None, 0);
                        v.push(h);
                    }
                }
                "SEQEND" => {
                    if let Some((pi, verts, bad)) = poly.take() {
                        self.polyline(&recs[pi], verts, bad, owner, entmode_of(owner, &recs[pi]));
                    }
                }
                _ => {
                    if let Some((pi, verts, bad)) = poly.take() {
                        // A POLYLINE without SEQEND ends at the next object.
                        self.polyline(&recs[pi], verts, bad, owner, entmode_of(owner, &recs[pi]));
                    }
                    if r.name == "POLYLINE" {
                        poly = Some((idx, vec![], r.has_bad_number()));
                        continue;
                    }
                    if let Some(kind) = self.kind(r) {
                        self.add(kind, r, owner, entmode);
                    }
                }
            }
        }
        if let Some((pi, verts, bad)) = poly.take() {
            self.polyline(&recs[pi], verts, bad, owner, entmode_of(owner, &recs[pi]));
        }
    }

    fn polyline(&mut self, r: &Rec, verts: Vec<u64>, bad: bool, owner: Option<u64>, entmode: u8) {
        let flags = r.i(70);
        // DW12: polygon meshes (16) and polyface meshes (64) aren't outlines.
        let kind = if bad {
            Kind::Skip { name: "damaged POLYLINE".into(), annotation: false }
        } else if flags & (16 | 64) != 0 {
            Kind::Skip { name: "POLYLINE mesh".into(), annotation: false }
        } else {
            Kind::Poly { three_d: flags & 8 != 0, closed: flags & 1 != 0, spline_fit: flags & 4 != 0, elev: r.n(30, 0.0), ext: r.ext(), owned: verts, first: None, last: None }
        };
        self.add(kind, r, owner, entmode);
    }

    fn kind(&mut self, r: &Rec) -> Option<Kind> {
        let deg = |v: f64| v.to_radians();
        let drawn = matches!(r.name, "LINE" | "CIRCLE" | "ARC" | "LWPOLYLINE" | "ELLIPSE" | "SPLINE" | "TEXT" | "MTEXT" | "INSERT");
        // The codes each kind of object can't do without (z values, extrusion
        // and angles of text may be left out and take their defaults).
        let needs: &[i32] = match r.name {
            "LINE" => &[10, 20, 11, 21],
            "CIRCLE" => &[10, 20, 40],
            "ARC" => &[10, 20, 40, 50, 51],
            "ELLIPSE" => &[10, 20, 11, 21, 40],
            "TEXT" | "MTEXT" | "INSERT" => &[10, 20],
            _ => &[],
        };
        if drawn && (r.has_bad_number() || r.lacks(needs)) {
            return Some(Kind::Skip { name: format!("damaged {}", r.name), annotation: false });
        }
        Some(match r.name {
            "LINE" => Kind::Line { a: r.p(10), b: r.p(11) },
            "CIRCLE" => Kind::Circle { c: r.p(10), r: r.n(40, 0.0), ext: r.ext() },
            "ARC" => Kind::Arc { c: r.p(10), r: r.n(40, 0.0), a0: deg(r.n(50, 0.0)), a1: deg(r.n(51, 360.0)), ext: r.ext() },
            "LWPOLYLINE" => {
                // Each corner is 10 (x) then 20 (y), with an optional bulge (42). A
                // corner without its y, a number that isn't one, or fewer corners than
                // the count (90) says makes the polyline damaged: it is left out and
                // named in the notes (DW5), never drawn with a corner missing.
                let mut verts: Vec<([f64; 2], f64)> = vec![];
                let mut cur: Option<([f64; 2], bool)> = None;
                let mut bad = false;
                for &(c, v) in r.data {
                    let x = v.parse::<f64>().ok().filter(|x| x.is_finite());
                    match c {
                        10 => {
                            if let Some((p, has_y)) = cur.take() {
                                bad |= !has_y;
                                verts.push((p, 0.0));
                            }
                            bad |= x.is_none();
                            cur = Some(([x.unwrap_or(0.0), 0.0], false));
                        }
                        20 => match cur.as_mut() {
                            Some((p, has_y)) if !*has_y => {
                                bad |= x.is_none();
                                p[1] = x.unwrap_or(0.0);
                                *has_y = true;
                            }
                            _ => bad = true,
                        },
                        42 => match cur.take() {
                            Some((p, true)) => {
                                bad |= x.is_none();
                                verts.push((p, x.unwrap_or(0.0)));
                            }
                            _ => bad = true,
                        },
                        _ => {}
                    }
                }
                if let Some((p, has_y)) = cur.take() {
                    bad |= !has_y;
                    verts.push((p, 0.0));
                }
                if r.f(90).is_some_and(|n| n as usize != verts.len()) {
                    bad = true;
                }
                if bad {
                    Kind::Skip { name: "damaged LWPOLYLINE".into(), annotation: false }
                } else {
                    Kind::LwPoly { closed: r.i(70) & 1 == 1, elev: r.n(38, 0.0), ext: r.ext(), verts }
                }
            }
            "ELLIPSE" => Kind::Ellipse { c: r.p(10), major: r.p(11), ext: r.ext(), ratio: r.n(40, 1.0), t0: r.n(41, 0.0), t1: r.n(42, std::f64::consts::TAU) },
            "SPLINE" => {
                let (Some(ctrl), Some(fit)) = (r.points(10), r.points(11)) else {
                    return Some(Kind::Skip { name: "damaged SPLINE".into(), annotation: false });
                };
                let knots = r.all(40);
                let mut weights = r.all(41);
                let flags = r.i(70);
                // DW10: counts the spline declares (72 knots, 73 control points,
                // 74 fit points) must match what it holds, and weights, when
                // given, one per control point (left out they are all 1, which
                // DXF allows even for a rational spline); otherwise it is damaged
                // (left out and named), never drawn some other way.
                let declared = |c: i32, have: usize| r.f(c).is_none_or(|n| n as usize == have);
                if !declared(72, knots.len()) || !declared(73, ctrl.len()) || !declared(74, fit.len()) || (!weights.is_empty() && weights.len() != ctrl.len()) {
                    return Some(Kind::Skip { name: "damaged SPLINE".into(), annotation: false });
                }
                if weights.is_empty() {
                    weights = vec![1.0; ctrl.len()];
                }
                Kind::Spline(Spline {
                    degree: r.i(71).clamp(1, 25) as usize,
                    closed: flags & 1 != 0,
                    knots,
                    ctrl,
                    weights,
                    fit,
                    beg_tan: r.p(12),
                    end_tan: r.p(13),
                })
            }
            "TEXT" => Kind::Text { pt: r.p(10), ext: r.ext(), rot: deg(r.n(50, 0.0)), h: r.n(40, 5.0), s: r.s(1).unwrap_or("").to_string() },
            "MTEXT" => {
                // The text comes in pieces: 3 (any number), then 1.
                let s: String = r.data.iter().filter(|x| x.0 == 3 || x.0 == 1).map(|x| x.1).collect();
                // Direction from 11/21/31, else the rotation (50, in radians for MTEXT).
                let xdir = if r.f(11).is_some() || r.f(21).is_some() {
                    r.p(11)
                } else {
                    let a = r.n(50, 0.0);
                    [a.cos(), a.sin(), 0.0]
                };
                Kind::MText { pt: r.p(10), xdir, h: r.n(40, 5.0), s }
            }
            "INSERT" => {
                let block = self.block(r.s(2).unwrap_or(""));
                Kind::Insert {
                    block,
                    pt: r.p(10),
                    scale: [r.n(41, 1.0), r.n(42, 1.0), r.n(43, 1.0)],
                    rot: deg(r.n(50, 0.0)),
                    ext: r.ext(),
                    cols: r.i(70).clamp(1, u16::MAX as i64) as u16,
                    rows: r.i(71).clamp(1, u16::MAX as i64) as u16,
                    dc: r.n(44, 0.0),
                    dr: r.n(45, 0.0),
                }
            }
            // Annotations: not part geometry, not reported (as before, DF5).
            "DIMENSION" | "POINT" | "HATCH" | "ATTRIB" | "ATTDEF" | "VIEWPORT" | "LEADER" | "MLEADER" | "MULTILEADER" | "TOLERANCE" | "WIPEOUT" | "IMAGE" => {
                Kind::Skip { name: r.name.to_string(), annotation: true }
            }
            other => Kind::Skip { name: other.to_string(), annotation: false },
        })
    }
}

fn entmode_of(owner: Option<u64>, r: &Rec) -> u8 {
    if owner.is_some() {
        0
    } else if r.i(67) == 1 {
        1
    } else {
        2
    }
}

/// Read the drawing objects of a DXF's group-code pairs.
/// mm per unit from the HEADER's `$INSUNITS` (9 $INSUNITS, then 70 <value>).
fn header_units(pairs: &[(i32, &str)]) -> Option<f64> {
    let i = pairs.iter().position(|p| *p == (9, "$INSUNITS"))?;
    let v = pairs.get(i + 1).filter(|p| p.0 == 70)?.1.parse().ok()?;
    crate::integrations::insunits_mm(v)
}

pub fn read(pairs: &[(i32, &str)], opts: &Options) -> Result<Drawing, String> {
    let start = section(pairs, "ENTITIES").ok_or("This file has no drawing (ENTITIES section) — is it an ASCII DXF?")?;
    let mut b = Builder { db: Db::default(), next: 0, layer_ids: HashMap::new(), block_ids: HashMap::new() };
    if let Some(ts) = section(pairs, "TABLES") {
        b.tables(&records(pairs, ts));
    }
    if let Some(bs) = section(pairs, "BLOCKS") {
        b.entities(&records(pairs, bs), None, true);
    }
    b.entities(&records(pairs, start), None, false);

    let db = b.db;
    // The drawing's own unit ($INSUNITS), else the caller's: everything is
    // placed in mm, so arcs are split within ARC_CHORD_ERR mm and an inch
    // drawing comes in at size with round holes (IU1).
    let u = header_units(pairs).unwrap_or(opts.unit_mm);
    let top = Mat { x: [u, 0.0, 0.0], y: [0.0, u, 0.0], z: [0.0, 0.0, u], o: [0.0; 3] };
    let mut ctx = Ctx::new(&db, opts.explode_blocks, opts.skip_invisible);
    let mut paper = 0usize;
    for h in &db.order {
        if ctx.over {
            break;
        }
        let o = &db.objs[h];
        if o.entmode == 2 || (opts.paper_space && o.entmode == 1) {
            ctx.entity(o, &top, None, 0);
        } else if o.entmode == 1 && !matches!(o.kind, Kind::Skip { annotation: true, .. }) {
            paper += 1;
        }
    }
    // Over a limit, part of the drawing is missing: refuse it whole.
    if let Some(e) = ctx.refusal() {
        return Err(e);
    }
    let Ctx { mut d, depth_hit, .. } = ctx;
    if paper > 0 {
        d.report.skipped.push(format!("{paper} paper-space objects"));
    }
    if depth_hit {
        d.report.skipped.push("blocks nested more than 16 deep".into());
    }
    if d.pieces.is_empty() {
        return Err("No lines, arcs, circles or polylines were found in the drawing.".into());
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use crate::dxf::{build, read, read_with};
    use std::collections::BTreeMap;

    fn dxf(blocks: &str, entities: &str) -> String {
        let b = if blocks.is_empty() { String::new() } else { format!("0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n") };
        format!("{b}0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n")
    }

    fn bbox(d: &crate::dxf::Drawing) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in d.pieces.iter().flat_map(|p| &p.pts) {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        b
    }

    #[test]
    fn the_header_gives_the_drawing_unit() {
        let width = |header: &str| {
            let text = format!("0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n{}", dxf("", "0\nLINE\n8\nCUT\n10\n0\n20\n0\n11\n1\n21\n0\n"));
            let b = bbox(&read(&text).unwrap());
            b[2] - b[0]
        };
        assert!((width("9\n$INSUNITS\n70\n1\n") - 25.4).abs() < 1e-9, "inch");
        assert!((width("  9\n$INSUNITS\n 70\n     4\n") - 1.0).abs() < 1e-9, "mm, padded as CAD programs write it");
        assert!((width("  9\n$INSUNITS\n 70\n     1\n") - 25.4).abs() < 1e-9, "inch, padded");
        assert!((width("9\n$INSUNITS\n70\n0\n") - 1.0).abs() < 1e-9, "unitless: the caller's unit");
    }

    #[test]
    fn a_mirrored_arc_lands_on_the_mirrored_side() {
        // DW7: centre (50,0) in a plane seen from below (extrusion 0,0,-1) is at x = -50.
        let d = read(&dxf("", "0\nARC\n8\nCUT\n10\n50\n20\n0\n30\n0\n40\n10\n50\n0\n51\n180\n210\n0\n220\n0\n230\n-1\n")).unwrap();
        let b = bbox(&d);
        assert!((b[0] + 60.0).abs() < 1e-6 && (b[2] + 40.0).abs() < 1e-6, "{b:?}");
        assert!((b[3] - 10.0).abs() < 1e-6, "the arc still bulges up: {b:?}");
    }

    #[test]
    fn an_ellipse_is_a_closed_piece() {
        // DW9: 40 × 20 mm ellipse round the origin.
        let d = read(&dxf("", "0\nELLIPSE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n20\n21\n0\n31\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n")).unwrap();
        assert!(d.pieces[0].closed);
        let b = bbox(&d);
        assert!((b[2] - 20.0).abs() < 1e-6 && (b[3] - 10.0).abs() < 0.1, "{b:?}");
        let (g, _) = build(&d, &BTreeMap::new()).unwrap();
        assert!((g.w - 40.0).abs() < 1e-6);
    }

    #[test]
    fn a_spline_follows_its_control_points() {
        // DW10: clamped quadratic (0,0) (10,20) (20,0) peaks at (10,10).
        let s = "0\nSPLINE\n8\n0\n70\n8\n71\n2\n72\n6\n73\n3\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n\
10\n0\n20\n0\n30\n0\n10\n10\n20\n20\n30\n0\n10\n20\n20\n0\n30\n0\n";
        let d = read(&dxf("", s)).unwrap();
        let b = bbox(&d);
        assert!((b[2] - 20.0).abs() < 1e-9 && (b[3] - 10.0).abs() < 0.1, "{b:?}");
    }

    #[test]
    fn blocks_are_exploded_with_their_insert() {
        // DW8: a 10 mm hole block (on layer 0) inserted twice into a plate, the second
        // time through another block, scaled ×2 and turned 90°.
        let blocks = "0\nBLOCK\n2\nHOLE\n70\n0\n10\n0\n20\n0\n30\n0\n0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n0\nENDBLK\n\
0\nBLOCK\n2\nWRAP\n70\n0\n10\n0\n20\n0\n30\n0\n0\nINSERT\n8\n0\n2\nHOLE\n10\n0\n20\n0\n30\n0\n0\nENDBLK\n";
        let plate = "0\nLWPOLYLINE\n8\nCUT\n90\n4\n70\n1\n10\n0\n20\n0\n10\n200\n20\n0\n10\n200\n20\n100\n10\n0\n20\n100\n";
        let ents = format!("{plate}0\nINSERT\n8\nHOLES\n2\nHOLE\n10\n50\n20\n50\n30\n0\n0\nINSERT\n8\nHOLES\n2\nwrap\n10\n150\n20\n50\n30\n0\n41\n2\n42\n2\n43\n1\n50\n90\n");
        let d = read(&dxf(blocks, &ents)).unwrap();
        // The circles on layer 0 take the insert's layer.
        assert_eq!(d.layers().get("HOLES"), Some(&2));
        let (g, rep) = build(&d, &BTreeMap::new()).unwrap();
        assert_eq!((g.holes.len(), rep.open_pieces), (2, 0));
        let wide = g.holes.iter().map(|h| h.iter().map(|p| p[0]).fold(f64::MIN, f64::max) - h.iter().map(|p| p[0]).fold(f64::MAX, f64::min)).fold(0.0, f64::max);
        assert!((wide - 20.0).abs() < 0.1, "the doubled hole is 20 mm across: {wide}");
    }

    #[test]
    fn paper_space_is_left_out_unless_asked() {
        // DX1: a title-block frame in paper space (67 = 1) isn't part of the part.
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n0\nLINE\n8\nFRAME\n67\n1\n10\n-500\n20\n-500\n30\n0\n11\n500\n21\n-500\n31\n0\n";
        let d = read(&dxf("", ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s.contains("paper-space")), "{:?}", d.report.skipped);
        let all = read_with(&dxf("", ents), &super::Options { paper_space: true, ..Default::default() }).unwrap();
        assert_eq!(all.pieces.len(), 2);
    }

    #[test]
    fn a_drawing_over_the_limits_is_refused_not_cut_short() {
        // 100 000 large bulged segments would make tens of millions of points;
        // a block grid of 200 × 200 copies is over the 10 000-copy limit.
        let mut lw = String::from("0\nLWPOLYLINE\n8\n0\n70\n1\n");
        for i in 0..100_000 {
            lw.push_str(&format!("10\n{}\n20\n0\n42\n0.9\n", i as f64 * 1000.0));
        }
        let e = read(&dxf("", &lw)).unwrap_err();
        assert!(e.contains("bigger than the import"), "{e}");
        let blocks = "0\nBLOCK\n2\nB\n70\n0\n10\n0\n20\n0\n30\n0\n0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n1\n0\nENDBLK\n";
        let grid = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5000\n0\nINSERT\n8\n0\n2\nB\n10\n0\n20\n0\n30\n0\n70\n200\n71\n200\n44\n5\n45\n5\n";
        assert!(read(&dxf(blocks, grid)).unwrap_err().contains("bigger than the import"));
    }

    #[test]
    fn a_polyline_with_a_corner_missing_is_left_out_and_named() {
        // Four corners declared, one without its y: not drawn as a triangle.
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n10\n100\n20\n50\n10\n0\n20\n50\n";
        let d = read(&dxf("", ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s.contains("damaged LWPOLYLINE")), "{:?}", d.report.skipped);
    }

    #[test]
    fn objects_with_unreadable_numbers_are_left_out_and_named() {
        // A LINE end written as "bad" and a CIRCLE radius "bad": not drawn at 0.
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\nbad\n21\n5\n0\nCIRCLE\n8\n0\n10\n50\n20\n0\n40\nbad\n";
        let d = read(&dxf("", ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s == "damaged LINE"), "{:?}", d.report.skipped);
        assert!(d.report.skipped.iter().any(|s| s == "damaged CIRCLE"), "{:?}", d.report.skipped);
    }

    #[test]
    fn an_insert_of_a_missing_block_is_named() {
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n50\n0\nINSERT\n8\n0\n2\nNOHOLE\n10\n0\n20\n0\n30\n0\n";
        let d = read(&dxf("", ents)).unwrap();
        assert!(d.report.skipped.iter().any(|s| s.contains("isn't in the file")), "{:?}", d.report.skipped);
    }

    #[test]
    fn damaged_splines_and_radii_are_named_not_drawn() {
        // A quadratic spline with 5 knots for 3 control points (needs 6), and a
        // circle of radius -5, next to a good outline.
        let spline = "0\nSPLINE\n8\n0\n70\n9\n71\n2\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n10\n0\n20\n0\n30\n0\n10\n10\n20\n20\n30\n0\n10\n20\n20\n0\n30\n0\n";
        let ents = format!("0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n100\n{spline}0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n-5\n");
        let d = read(&dxf("", &ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s == "damaged SPLINE"), "{:?}", d.report.skipped);
        assert!(d.report.skipped.iter().any(|s| s == "damaged CIRCLE"), "{:?}", d.report.skipped);
    }

    #[test]
    fn texts_copied_through_a_block_grid_have_a_budget() {
        // A block with a 1 MB text, inserted as a 100 × 100 grid: refused, not
        // 10 GB of text.
        let big = "A".repeat(1 << 20);
        let blocks = format!("0\nBLOCK\n2\nT\n70\n0\n10\n0\n20\n0\n30\n0\n0\nMTEXT\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n1\n{big}\n0\nENDBLK\n");
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n50\n0\nINSERT\n8\n0\n2\nT\n10\n0\n20\n0\n30\n0\n70\n100\n71\n100\n44\n1\n45\n1\n";
        assert!(read(&dxf(&blocks, ents)).unwrap_err().contains("bigger than the import"));
    }

    #[test]
    fn a_spline_point_missing_a_coordinate_is_named_not_drawn() {
        // A closed degree-1 spline square whose third control point has no y.
        let s = "0\nSPLINE\n8\n0\n70\n9\n71\n1\n40\n0\n40\n0\n40\n1\n40\n2\n40\n3\n40\n4\n40\n4\n\
10\n0\n20\n0\n30\n0\n10\n20\n20\n0\n30\n0\n10\n20\n30\n0\n10\n0\n20\n20\n30\n0\n10\n0\n20\n0\n30\n0\n";
        let ents = format!("0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n100\n{s}");
        let d = read(&dxf("", &ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s == "damaged SPLINE"), "{:?}", d.report.skipped);
    }

    #[test]
    fn objects_missing_a_coordinate_are_named_not_drawn_at_zero() {
        // A LINE without its end's y and a CIRCLE without a radius.
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n100\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n50\n0\nCIRCLE\n8\n0\n10\n5\n20\n5\n";
        let d = read(&dxf("", ents)).unwrap();
        assert_eq!(d.pieces.len(), 1);
        assert!(d.report.skipped.iter().any(|s| s == "damaged LINE"), "{:?}", d.report.skipped);
        assert!(d.report.skipped.iter().any(|s| s == "damaged CIRCLE"), "{:?}", d.report.skipped);
    }

    #[test]
    fn rational_splines_may_leave_out_weights_and_flat_ellipses_are_named() {
        // A closed degree-1 rational spline square (flags 13) with no weights:
        // the weights are 1, as DXF allows; it imports.
        let sq = "0\nSPLINE\n8\n0\n70\n13\n71\n1\n40\n0\n40\n0\n40\n1\n40\n2\n40\n3\n40\n4\n40\n4\n\
10\n0\n20\n0\n30\n0\n10\n20\n20\n0\n30\n0\n10\n20\n20\n20\n30\n0\n10\n0\n20\n20\n30\n0\n10\n0\n20\n0\n30\n0\n";
        let flat = "0\nELLIPSE\n8\n0\n10\n50\n20\n50\n30\n0\n11\n0\n21\n0\n31\n0\n40\n0.5\n";
        let ents = format!("0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n100\n{sq}{flat}");
        let d = read(&dxf("", &ents)).unwrap();
        assert_eq!(d.pieces.len(), 2, "{:?}", d.report.skipped);
        assert!(d.report.skipped.iter().any(|s| s == "damaged ELLIPSE"), "{:?}", d.report.skipped);
    }

    #[test]
    fn texts_and_annotations() {
        // DW11: MTEXT codes removed; a dimension is neither drawn nor reported.
        let ents = "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n30\n0\n40\n5\n0\nMTEXT\n8\nMARK\n10\n1\n20\n2\n30\n0\n40\n3\n1\n{\\fArial;DEMO-001}\n0\nDIMENSION\n8\n0\n";
        let d = read(&dxf("", ents)).unwrap();
        assert_eq!(d.texts.len(), 1);
        assert_eq!(d.texts[0].1, "DEMO-001");
        assert!(d.report.skipped.is_empty(), "{:?}", d.report.skipped);
    }

    /// A 100 × 100 square on layer 0 and a Ø10 circle at x, with extra codes.
    fn square() -> String {
        "0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n100\n10\n0\n20\n100\n".to_string()
    }
    fn circle(x: f64, layer: &str, extra: &str) -> String {
        format!("0\nCIRCLE\n8\n{layer}\n{extra}10\n{x}\n20\n50\n30\n0\n40\n5\n")
    }

    #[test]
    fn what_the_drawing_hides_or_draws_as_centre_lines_starts_left_out() {
        // DX3: line types (CENTER has dashes, SOLIDX none) and layers (CL drawn in
        // CENTER, OFFL turned off, FRZ frozen) from the drawing's tables.
        let tables = "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLTYPE\n\
0\nLTYPE\n2\nCONTINUOUS\n70\n0\n73\n0\n0\nLTYPE\n2\nCENTER\n70\n0\n73\n4\n0\nLTYPE\n2\nSOLIDX\n70\n0\n73\n0\n0\nENDTAB\n\
0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n0\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nLAYER\n2\nCL\n70\n0\n62\n1\n6\nCENTER\n\
0\nLAYER\n2\nOFFL\n70\n0\n62\n-7\n6\nCONTINUOUS\n0\nLAYER\n2\nFRZ\n70\n1\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n";
        // A bolt circle as a block: its circle is BYBLOCK, the insert is CENTER.
        let blocks = "0\nBLOCK\n8\n0\n2\nBOLT\n70\n0\n10\n0\n20\n0\n30\n0\n0\nCIRCLE\n8\n0\n6\nBYBLOCK\n10\n0\n20\n0\n30\n0\n40\n8\n0\nENDBLK\n";
        let ents = [
            square(),
            circle(15.0, "0", "6\nCENTER\n"),
            circle(30.0, "CL", ""),
            circle(45.0, "OFFL", ""),
            circle(60.0, "FRZ", ""),
            circle(75.0, "0", "60\n1\n"),
            circle(90.0, "0", "6\nSOLIDX\n"),
            "0\nINSERT\n8\n0\n6\nCENTER\n2\nBOLT\n10\n50\n20\n20\n30\n0\n".to_string(),
        ]
        .concat();
        let text = format!("{tables}{}", dxf(blocks, &ents));
        let d = read(&text).unwrap();
        let rows: Vec<&String> = d.hidden.keys().collect();
        assert_eq!(rows, ["0 [CENTER]", "0 [invisible]", "CL [CENTER]", "FRZ [frozen]", "OFFL [off]"], "{:?}", d.layers());
        assert_eq!(d.layers().get("0 [CENTER]"), Some(&2), "the CENTER circle and the bolt circle");
        assert!(d.hidden["FRZ [frozen]"].contains("frozen"));
        // Left out unless asked: the square and the solid circle are cut.
        let (g, rep) = build(&d, &BTreeMap::new()).unwrap();
        assert_eq!(g.holes.len(), 1, "only the SOLIDX circle is a hole");
        let out: usize = rep.hidden_left_out.iter().map(|x| x.1).sum();
        assert_eq!(out, 6, "{:?}", rep.hidden_left_out);
        let notes = crate::engine::import_notes("DEMO-001", &rep);
        assert!(notes.iter().any(|n| n.contains("centre / dashed") && n.contains("'0 [CENTER]'")), "{notes:?}");
        // Set to Cut, a row is cut like any layer.
        let roles: BTreeMap<String, crate::dxf::LayerRole> = [("0 [CENTER]".to_string(), crate::dxf::LayerRole::Cut)].into();
        assert_eq!(build(&d, &roles).unwrap().0.holes.len(), 3, "the solid circle, the CENTER circle and the bolt circle");
        // The same file without tables: any line type but CONTINUOUS is broken.
        let bare = read(&dxf(blocks, &ents)).unwrap();
        assert!(bare.hidden.contains_key("0 [SOLIDX]") && bare.hidden.contains_key("0 [CENTER]"), "{:?}", bare.hidden);
        assert!(!bare.hidden.keys().any(|k| k.starts_with("FRZ") || k.starts_with("OFFL")));
    }

    #[test]
    fn an_object_drawn_twice_is_cut_once() {
        // DX4: the same hole twice (once as a circle, once as the same circle
        // drawn the other way round on another layer) and one edge drawn twice.
        let line = |a: [f64; 2], b: [f64; 2]| format!("0\nLINE\n8\n0\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
        let ents = [
            line([0.0, 0.0], [100.0, 0.0]),
            line([100.0, 0.0], [100.0, 100.0]),
            line([100.0, 100.0], [0.0, 100.0]),
            line([0.0, 100.0], [0.0, 0.0]),
            line([100.0, 0.0], [0.0, 0.0]),
            circle(50.0, "0", ""),
            circle(50.0, "HOLES", ""),
        ]
        .concat();
        let d = read(&dxf("", &ents)).unwrap();
        let (g, rep) = build(&d, &BTreeMap::new()).unwrap();
        assert_eq!((g.holes.len(), rep.duplicates, rep.open_pieces), (1, 2, 0), "{rep:?}");
        assert!((g.w - 100.0).abs() < 1e-9 && (g.h - 100.0).abs() < 1e-9);
        assert!(crate::engine::import_notes("DEMO-001", &rep).iter().any(|n| n.contains("2 object(s) drawn twice")));
    }
}
