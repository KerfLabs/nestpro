//! Sheet shapes that aren't plain rectangles — remnants — and the polygon
//! operations around them: inward offsets (margins), boolean operations (sheet
//! less what was cut), and splitting a shape into the free rectangles the
//! nester starts from.

use crate::nest::P;
use crate::stock::Outline;
use geo::algorithm::buffer::{BufferStyle, LineJoin};
use geo::{Area, BooleanOps, Buffer, Coord, LineString, MultiPolygon, Polygon};

/// A free rectangle (x, y, w, h).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, o: &Rect) -> bool {
        let e = 1e-6;
        o.x >= self.x - e && o.y >= self.y - e && o.x + o.w <= self.x + self.w + e && o.y + o.h <= self.y + self.h + e
    }
}

fn ring(pts: &[P]) -> LineString<f64> {
    let mut v: Vec<Coord<f64>> = pts.iter().map(|p| Coord { x: p[0], y: p[1] }).collect();
    if let (Some(a), Some(b)) = (v.first().copied(), v.last().copied()) {
        if a != b {
            v.push(a);
        }
    }
    LineString::new(v)
}

pub fn to_polygon(o: &Outline) -> Polygon<f64> {
    Polygon::new(ring(&o.outer), o.holes.iter().map(|h| ring(h)).collect())
}

fn ring_pts(ls: &LineString<f64>) -> Vec<P> {
    let mut v: Vec<P> = ls.coords().map(|c| [c.x, c.y]).collect();
    if v.len() > 1 && v.first() == v.last() {
        v.pop();
    }
    v
}

/// Each polygon of a multipolygon as an outline, largest first.
pub fn outlines(mp: &MultiPolygon<f64>) -> Vec<Outline> {
    let mut v: Vec<(f64, Outline)> = mp
        .0
        .iter()
        .map(|p| (p.unsigned_area(), Outline { outer: ring_pts(p.exterior()), holes: p.interiors().iter().map(ring_pts).collect() }))
        .filter(|(a, o)| *a > 1e-6 && o.outer.len() >= 3)
        .collect();
    v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    v.into_iter().map(|(_, o)| o).collect()
}

/// The shape moved in by `d` (out for negative `d`), keeping square corners
/// square (mitred joins).
pub fn offset(o: &Outline, d: f64) -> Vec<Outline> {
    if d.abs() < 1e-9 {
        return vec![o.clone()];
    }
    let style = BufferStyle::new(-d).line_join(LineJoin::Miter(0.1));
    outlines(&to_polygon(o).buffer_with_style(style))
}

/// The shape moved in by `d` (out for negative `d`) at a constant distance:
/// corners become arcs (a true offset, like the original's compensation).
pub fn offset_round(o: &Outline, d: f64) -> Vec<Outline> {
    if d.abs() < 1e-9 {
        return vec![o.clone()];
    }
    let style = BufferStyle::new(-d).line_join(LineJoin::Round(0.1));
    outlines(&to_polygon(o).buffer_with_style(style))
}

/// `a` less `b`.
pub fn difference(a: &Outline, b: &[Outline]) -> Vec<Outline> {
    let mut cur = MultiPolygon::new(vec![to_polygon(a)]);
    for o in b {
        cur = cur.difference(&to_polygon(o));
    }
    outlines(&cur)
}

/// Everything any of the shapes covers.
pub fn union_all(v: &[Outline]) -> Vec<Outline> {
    let mut mp = MultiPolygon::<f64>::new(vec![]);
    for o in v {
        mp = mp.union(&MultiPolygon::new(vec![to_polygon(o)]));
    }
    outlines(&mp)
}

/// `a` within `b`.
pub fn intersection(a: &Outline, b: &Outline) -> Vec<Outline> {
    outlines(&to_polygon(a).intersection(&to_polygon(b)))
}

/// The x-ranges inside the shape along a horizontal line (even–odd rule).
fn spans_at(o: &Outline, y: f64) -> Vec<(f64, f64)> {
    let mut xs = vec![];
    for r in std::iter::once(&o.outer).chain(o.holes.iter()) {
        for i in 0..r.len() {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            if (a[1] <= y && b[1] > y) || (b[1] <= y && a[1] > y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.chunks(2).filter(|c| c.len() == 2 && c[1] - c[0] > 1e-6).map(|c| (c[0], c[1])).collect()
}

/// Ranges inside both lists.
fn meet(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = vec![];
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let (l, r) = (a0.max(b0), a1.min(b1));
            if r - l > 1e-6 {
                out.push((l, r));
            }
        }
    }
    out
}

/// Most bands a shape is cut into before neighbouring bands are merged (a
/// shape with many curved vertices would otherwise give thousands).
const MAX_BANDS: usize = 120;
/// Largest sideways drift of an edge within one band (mm).
const SLANT_STEP: f64 = 10.0;

/// The largest rectangles that fit inside the shape, overlapping each other —
/// the free space the nester (MaxRects) starts a remnant sheet with. Built from
/// horizontal bands between vertex heights; each band keeps only what is
/// inside along its whole height, so slanted and curved edges are treated
/// conservatively (nothing is placed outside the shape).
pub fn free_rects(o: &Outline) -> Vec<Rect> {
    let mut ys: Vec<f64> = o.outer.iter().chain(o.holes.iter().flatten()).map(|p| p[1]).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    // Split bands crossed by slanted or curved edges so each piece slants by at
    // most SLANT_STEP; otherwise a slanted band would only keep a sliver.
    let mut fine = vec![];
    for w in ys.windows(2) {
        let e = (w[1] - w[0]) * 1e-6;
        let (a, b) = (spans_at(o, w[0] + e), spans_at(o, w[1] - e));
        let drift = a.iter().zip(&b).map(|(p, q)| (p.0 - q.0).abs().max((p.1 - q.1).abs())).fold(0.0, f64::max);
        let k = ((drift / SLANT_STEP).ceil() as usize).clamp(1, 40);
        fine.extend((0..k).map(|i| w[0] + (w[1] - w[0]) * i as f64 / k as f64));
    }
    fine.extend(ys.last().copied());
    ys = fine;
    if ys.len() > MAX_BANDS + 1 {
        // Keep the extreme heights and evenly spaced ones between them.
        let (lo, hi) = (ys[0], ys[ys.len() - 1]);
        ys = (0..=MAX_BANDS).map(|i| lo + (hi - lo) * i as f64 / MAX_BANDS as f64).collect();
    }
    // Inside ranges of each band, checked just inside its bottom, middle and top.
    let bands: Vec<(f64, f64, Vec<(f64, f64)>)> = ys
        .windows(2)
        .filter(|w| w[1] - w[0] > 1e-6)
        .map(|w| {
            let e = (w[1] - w[0]) * 1e-6;
            let s = meet(&meet(&spans_at(o, w[0] + e), &spans_at(o, (w[0] + w[1]) / 2.0)), &spans_at(o, w[1] - e));
            (w[0], w[1], s)
        })
        .collect();
    let mut rects: Vec<Rect> = vec![];
    for i in 0..bands.len() {
        for &(x0, x1) in &bands[i].2 {
            // Grow upward through the bands while the range still overlaps.
            let mut cur = vec![(x0, x1)];
            for k in i..bands.len() {
                if k > i {
                    cur = meet(&cur, &bands[k].2);
                }
                if cur.is_empty() {
                    break;
                }
                for &(l, r) in &cur {
                    rects.push(Rect { x: l, y: bands[i].0, w: r - l, h: bands[k].1 - bands[i].0 });
                }
            }
        }
    }
    // Keep only rectangles that no other one contains.
    let mut keep = vec![true; rects.len()];
    for i in 0..rects.len() {
        for j in 0..rects.len() {
            if i != j && keep[j] && rects[j].contains(&rects[i]) && (!rects[i].contains(&rects[j]) || j < i) {
                keep[i] = false;
                break;
            }
        }
    }
    rects.into_iter().zip(keep).filter(|(_, k)| *k).map(|(r, _)| r).collect()
}

/// The free rectangles inside a shape moved in by `margin` — the room parts
/// have on a remnant sheet. Kept for the last few shapes asked about, because
/// dragging a part asks many times per mouse move.
pub fn inner_rects(o: &Outline, margin: f64) -> Vec<Rect> {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: std::cell::RefCell<Vec<(u64, Vec<Rect>)>> = const { std::cell::RefCell::new(vec![]) };
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    margin.to_bits().hash(&mut h);
    for p in o.outer.iter().chain(o.holes.iter().flatten()) {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    o.holes.len().hash(&mut h);
    let key = h.finish();
    if let Some(r) = CACHE.with(|c| c.borrow().iter().find(|(k, _)| *k == key).map(|(_, r)| r.clone())) {
        return r;
    }
    let rects: Vec<Rect> = offset(o, margin).iter().flat_map(free_rects).collect();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.insert(0, (key, rects.clone()));
        c.truncate(4);
    });
    rects
}

/// Does a polygon (a part's real outline, sheet mm) lie inside the shape, at
/// least `margin` from its edges? Within 0.01 mm² of overlap (rounding).
pub fn poly_inside(o: &Outline, margin: f64, pts: &[P]) -> bool {
    if pts.len() < 3 {
        return true;
    }
    let inner: Vec<Polygon<f64>> = offset(o, margin).iter().map(to_polygon).collect();
    let part = Polygon::new(ring(pts), vec![]);
    part.difference(&MultiPolygon::new(inner)).unsigned_area() < 0.01
}

/// Does a box lie inside the shape, at least `margin` from its edges?
pub fn box_inside(o: &Outline, margin: f64, x: f64, y: f64, w: f64, h: f64) -> bool {
    let e = 1e-6;
    inner_rects(o, margin).iter().any(|r| x >= r.x - e && y >= r.y - e && x + w <= r.x + r.w + e && y + h <= r.y + r.h + e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Outline {
        Outline { outer: vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]], holes: vec![] }
    }

    #[test]
    fn a_rectangle_is_one_free_rectangle() {
        assert_eq!(free_rects(&rect(0.0, 0.0, 300.0, 200.0)), vec![Rect { x: 0.0, y: 0.0, w: 300.0, h: 200.0 }]);
    }

    #[test]
    fn an_l_shape_gives_its_two_maximal_rectangles() {
        // 1000 × 1500 with the top-left 500 × 1000 missing.
        let l = Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1500.0], [500.0, 1500.0], [500.0, 500.0], [0.0, 500.0]], holes: vec![] };
        let mut r = free_rects(&l);
        r.sort_by(|a, b| a.w.partial_cmp(&b.w).unwrap());
        assert_eq!(r, vec![Rect { x: 500.0, y: 0.0, w: 500.0, h: 1500.0 }, Rect { x: 0.0, y: 0.0, w: 1000.0, h: 500.0 }]);
    }

    #[test]
    fn holes_are_left_out() {
        let mut o = rect(0.0, 0.0, 300.0, 300.0);
        o.holes.push(vec![[100.0, 100.0], [200.0, 100.0], [200.0, 200.0], [100.0, 200.0]]);
        let r = free_rects(&o);
        assert!(r.iter().all(|r| !(r.x < 200.0 && r.x + r.w > 100.0 && r.y < 200.0 && r.y + r.h > 100.0)), "{r:?}");
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn slanted_edges_stay_inside() {
        let tri = Outline { outer: vec![[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]], holes: vec![] };
        let rs = free_rects(&tri);
        assert!(!rs.is_empty());
        for r in rs {
            // The far corner of every rectangle must be inside the triangle.
            assert!(r.x >= -1e-6 && r.y >= -1e-6 && r.x + r.w + r.y + r.h <= 100.0 + 1e-3, "{r:?}");
        }
        // Usable: the best rectangle is close to the ideal 50 × 50.
        let best = free_rects(&tri).iter().map(|r| r.w * r.h).fold(0.0, f64::max);
        assert!(best > 2000.0, "{best}");
    }

    #[test]
    fn offsets_and_boolean_operations() {
        let sq = rect(0.0, 0.0, 100.0, 100.0);
        // Growing keeps square corners square.
        let big = offset(&sq, -10.0);
        assert!((big[0].area_mm2() - 14400.0).abs() < 1.0, "{}", big[0].area_mm2());
        let small = offset(&sq, 10.0);
        assert_eq!(small.len(), 1);
        assert!((small[0].area_mm2() - 6400.0).abs() < 1.0, "{}", small[0].area_mm2());
        let left = difference(&sq, &[rect(50.0, -10.0, 100.0, 200.0)]);
        assert!((left[0].area_mm2() - 5000.0).abs() < 1e-6);
        let both = intersection(&sq, &rect(50.0, 50.0, 100.0, 100.0));
        assert!((both[0].area_mm2() - 2500.0).abs() < 1e-6);
    }
}
