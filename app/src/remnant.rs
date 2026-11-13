//! Turning what's left of a nested sheet into remnants for the stock — the
//! original's "Generate remnants when you save" (manual p. 556).
//!
//! - The region is the sheet (or the remnant it already was), cut along the
//!   remnant line(s) by a strip as wide as the cut.
//! - Pieces holding a part are the used side; the others are remnants (RE24
//!   keeps "the piece that contains no part").
//! - Pieces smaller than the minimum area — net of holes — are scrap (RE20).
//! - "Generate remnants with the completed sheets" keeps the whole sheet less
//!   the parts instead, for nesting more parts on it later.

use crate::cutoff::{CutKind, CutLine};
use crate::nest::{self, Part, Placed, SheetFormat, P};
use crate::stock::Outline;

/// Smallest remnant kept by default, in m² (the dialog on manual p. 556 shows
/// 0.100 m²; the stored unit is unconfirmed).
pub const DEFAULT_MIN_AREA_M2: f64 = 0.1;

/// The sheet's shape in sheet coordinates.
pub fn sheet_outline(f: &SheetFormat) -> Outline {
    match &f.outline {
        Some(o) if o.outer.len() >= 3 => o.clone(),
        _ => Outline { outer: vec![[0.0, 0.0], [f.w, 0.0], [f.w, f.h], [0.0, f.h]], holes: vec![] },
    }
}

/// A thin strip along a polyline (the material the cut removes), stretched past
/// both ends so it crosses the sheet edge cleanly (RE7: lines run 2 × tolerance
/// past the edges).
fn cut_strip(pts: &[P], kerf: f64) -> Vec<Outline> {
    let mut out = vec![];
    let half = (kerf / 2.0).max(0.05);
    let n = pts.len();
    for i in 0..n.saturating_sub(1) {
        let (mut a, mut b) = (pts[i], pts[i + 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        // Overlap joints a little, and run the ends past the sheet.
        let (ea, eb) = (if i == 0 { 5.0 } else { half }, if i + 2 == n { 5.0 } else { half });
        a = [a[0] - ux * ea, a[1] - uy * ea];
        b = [b[0] + ux * eb, b[1] + uy * eb];
        let (nx, ny) = (-uy * half, ux * half);
        out.push(Outline { outer: vec![[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]], holes: vec![] });
    }
    out
}

/// The parts' outer outlines on the sheet.
fn part_outlines(parts: &[Part], sheet: &[Placed]) -> Vec<Outline> {
    sheet.iter().map(|pl| Outline { outer: nest::placed_shape(parts, pl).0, holes: vec![] }).filter(|o| o.outer.len() >= 3).collect()
}

/// The remnants a nested sheet leaves, in sheet coordinates, largest first.
/// Without a remnant line there are none, unless `keep_completed` is on.
pub fn sheet_remnants(parts: &[Part], sheet: &[Placed], f: &SheetFormat, lines: &[CutLine], kerf: f64, min_area_m2: f64, keep_completed: bool) -> Vec<Outline> {
    let s = sheet_outline(f);
    let used = part_outlines(parts, sheet);
    let min_mm2 = (min_area_m2 * 1e6).max(1e-6);
    if keep_completed {
        return crate::region::difference(&s, &used).into_iter().filter(|o| o.area_mm2() >= min_mm2).collect();
    }
    let strips: Vec<Outline> = lines.iter().filter(|l| l.kind == CutKind::Remnant).flat_map(|l| cut_strip(&l.pts, kerf)).collect();
    if strips.is_empty() {
        return vec![];
    }
    crate::region::difference(&s, &strips)
        .into_iter()
        .filter(|piece| !used.iter().any(|u| crate::region::intersection(piece, u).iter().any(|o| o.area_mm2() > 1.0)))
        .filter(|o| o.area_mm2() >= min_mm2)
        .collect()
}

/// `sheet_remnants` for drawing: the last few answers are kept, since the
/// sheet is redrawn every frame.
pub fn sheet_remnants_cached(parts: &[Part], sheet: &[Placed], f: &SheetFormat, lines: &[CutLine], kerf: f64) -> Vec<Outline> {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: std::cell::RefCell<Vec<(u64, Vec<Outline>)>> = const { std::cell::RefCell::new(vec![]) };
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in [f.w, f.h, kerf] {
        v.to_bits().hash(&mut h);
    }
    if let Some(o) = &f.outline {
        for p in o.outer.iter().chain(o.holes.iter().flatten()) {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    for p in sheet {
        (p.part, p.turns, p.mirror).hash(&mut h);
        if !p.spin.is_none() {
            p.spin.deg.to_bits().hash(&mut h);
        }
        for v in [p.x, p.y, p.w, p.h] {
            v.to_bits().hash(&mut h);
        }
    }
    for l in lines {
        (l.kind == CutKind::Remnant).hash(&mut h);
        for p in &l.pts {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    let key = h.finish();
    if let Some(v) = CACHE.with(|c| c.borrow().iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())) {
        return v;
    }
    let v = sheet_remnants(parts, sheet, f, lines, kerf, 0.0, false);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.insert(0, (key, v.clone()));
        c.truncate(6);
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    fn part(w: f64, h: f64) -> Part {
        Part { name: "A".into(), shape: Shape::Rectangle, w, h, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn a_straight_line_leaves_the_far_side() {
        let parts = vec![part(1000.0, 800.0)];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 1000.0, h: 800.0, turns: 0, mirror: false, spin: Default::default() }];
        let f = SheetFormat::rect(3000.0, 1500.0);
        let line = CutLine { pts: vec![[1100.0, 0.0], [1100.0, 1500.0]], kind: CutKind::Remnant };
        let r = sheet_remnants(&parts, &sheet, &f, &[line], 0.3, 0.1, false);
        assert_eq!(r.len(), 1);
        let (l, w) = r[0].size();
        assert!((l - 1899.85).abs() < 0.01 && (w - 1500.0).abs() < 1e-6, "{l} × {w}");
    }

    #[test]
    fn a_stepped_line_gives_an_l_shaped_remnant() {
        let parts = vec![part(1000.0, 800.0), part(400.0, 1400.0)];
        let sheet = vec![
            Placed { part: 0, x: 10.0, y: 10.0, w: 1000.0, h: 800.0, turns: 0, mirror: false, spin: Default::default() },
            Placed { part: 1, x: 10.0, y: 10.0 + 800.0 + 10.0, w: 400.0, h: 600.0, turns: 0, mirror: false, spin: Default::default() },
        ];
        let f = SheetFormat::rect(3000.0, 1500.0);
        // Up at x = 1100 to y = 820, across to 500, up to the top.
        let line = CutLine { pts: vec![[1100.0, 0.0], [1100.0, 820.0], [500.0, 820.0], [500.0, 1500.0]], kind: CutKind::Remnant };
        let r = sheet_remnants(&parts, &sheet, &f, &[line], 0.3, 0.1, false);
        assert_eq!(r.len(), 1);
        let a = r[0].area_mm2();
        let expect = 3000.0 * 1500.0 - 1100.0 * 820.0 - 500.0 * 680.0;
        assert!((a - expect).abs() < expect * 0.001, "{a} vs {expect}");
        assert!(r[0].outer.len() >= 6, "L-shaped");
    }

    #[test]
    fn small_pieces_are_scrap_and_no_line_means_no_remnant() {
        let parts = vec![part(2800.0, 1400.0)];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 2800.0, h: 1400.0, turns: 0, mirror: false, spin: Default::default() }];
        let f = SheetFormat::rect(3000.0, 1500.0);
        let line = CutLine { pts: vec![[2850.0, 0.0], [2850.0, 1500.0]], kind: CutKind::Remnant };
        // 150 × 1500 = 0.225 m²: kept at 0.1 m², scrap at 0.3 m².
        assert_eq!(sheet_remnants(&parts, &sheet, &f, &[line.clone()], 0.3, 0.1, false).len(), 1);
        assert!(sheet_remnants(&parts, &sheet, &f, &[line], 0.3, 0.3, false).is_empty());
        assert!(sheet_remnants(&parts, &sheet, &f, &[], 0.3, 0.1, false).is_empty());
    }

    #[test]
    fn a_completed_sheet_keeps_everything_but_the_parts() {
        let parts = vec![part(1000.0, 800.0)];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 1000.0, h: 800.0, turns: 0, mirror: false, spin: Default::default() }];
        let f = SheetFormat::rect(3000.0, 1500.0);
        let r = sheet_remnants(&parts, &sheet, &f, &[], 0.3, 0.1, true);
        assert_eq!(r.len(), 1);
        assert!((r[0].area_mm2() - (3000.0 * 1500.0 - 1000.0 * 800.0)).abs() < 1.0);
    }

    #[test]
    fn a_remnant_of_a_remnant_follows_its_shape() {
        let parts = vec![part(300.0, 300.0)];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 300.0, h: 300.0, turns: 0, mirror: false, spin: Default::default() }];
        // L-shaped remnant sheet; cut off the right part at x = 400.
        let l = Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [600.0, 1000.0], [600.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let f = SheetFormat { w: 1000.0, h: 1000.0, outline: Some(l), stock_ref: "2-16".into(), remnant: true, made: vec![], made_from: 0 };
        let line = CutLine { pts: vec![[400.0, 0.0], [400.0, 1000.0]], kind: CutKind::Remnant };
        let r = sheet_remnants(&parts, &sheet, &f, &[line], 0.3, 0.1, false);
        assert_eq!(r.len(), 1);
        let expect = 1000.0 * 1000.0 - 600.0 * 600.0 - 400.0 * 400.0;
        assert!((r[0].area_mm2() - expect).abs() < expect * 0.002, "{}", r[0].area_mm2());
    }
}
