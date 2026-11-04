//! True-shape strip ("band") nesting, modelled on Lantek Expert's own engine
//! (MECHANICS.md "True-shape strip nesting").
//!
//! In shop terms: every part, at every orientation it may take, is cut into thin
//! strips side by side along the sheet's X. Each strip keeps the stretches of Y the
//! part covers, worked out from the outline grown by half the part gap, so two grown
//! outlines that touch are one gap apart. Holes stay open. The sheet is cut into
//! strips too; each sheet strip keeps the stretches of Y that are still free. A part
//! is placed by lining its first strip up with a sheet strip and dropping it down in
//! Y to the lowest height where every stretch it covers sits in free space. When it
//! is placed, its stretches are taken out of the sheet's free lists, so the holes of
//! a placed part stay free and later, smaller parts can drop into them.
//!
//! All strip coordinates are integers of 0.1 mm, like the original (which
//! truncates v·10). The rebuild rounds outward (floor/ceil) instead of
//! truncating, so a profile always covers its grown outline.

use crate::nest::P;

/// Strip units per mm (0.1 mm).
pub const UNITS: f64 = 10.0;
/// Narrowest strip, in units: 0.5 mm (B091 "minimum strip width", built-in
/// value 5). Strips are never cut narrower than this.
pub const MIN_STRIP: i32 = 5;
/// A straight edge running along Y and longer than this (mm) puts a strip
/// boundary where it lies, so straight sides fall on strip edges.
pub const SPLIT_EDGE: f64 = 1.0;
/// Slabs are opened by this much (mm) so an edge lying exactly one grown distance
/// away doesn't count as inside the strip (rebuild's numeric guard).
const EPS: f64 = 1e-4;

/// One strip: X from `x0` to `x1` and its sorted, disjoint Y stretches `[lo, hi]`.
/// In a part's profile the stretches are material; on a sheet they are free space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strip {
    pub x0: i32,
    pub x1: i32,
    pub iv: Vec<(i32, i32)>,
}

/// A part (or a module of parts) at one orientation, cut into strips (the
/// original's strip structure). Local coordinates: the grown outline's box starts at
/// (0, 0); the part's own footprint starts at (`grow`, `grow`) mm.
#[derive(Clone, Debug)]
pub struct Profile {
    pub strips: Vec<Strip>,
    /// Size of the grown box along X, and the top of the highest stretch, in units.
    pub sx: i32,
    pub sy: i32,
    /// Centre of gravity of the strip profile, local mm (area-weighted mean over
    /// all stretches).
    pub cg: (f64, f64),
    /// How far (mm) the outline was grown: half the part gap plus the rebuild's guard.
    pub grow: f64,
}

impl Profile {
    /// Same strips (orientations that look alike are tried once).
    pub fn same(&self, o: &Profile) -> bool {
        self.sx == o.sx && self.sy == o.sy && self.strips == o.strips
    }
}

/// Merge overlapping or touching stretches (the original keeps a strip's
/// stretches as a union).
fn merge<T: Copy + PartialOrd>(mut v: Vec<(T, T)>) -> Vec<(T, T)> {
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(T, T)> = Vec::with_capacity(v.len());
    for (lo, hi) in v {
        match out.last_mut() {
            Some(l) if lo <= l.1 => {
                if hi > l.1 {
                    l.1 = hi;
                }
            }
            _ => out.push((lo, hi)),
        }
    }
    out
}

/// Even-odd point test over a set of edges.
fn inside(edges: &[(P, P)], p: P) -> bool {
    let mut c = false;
    for &(a, b) in edges {
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

/// The Y stretches the material covers anywhere in the slab `a ≤ x ≤ b`: the
/// projection onto Y of (shape ∩ slab). It is the union of the Y extents of the
/// edges clipped to the slab and of the cuts along the slab's two side lines
/// (paired odd/even, the way the original pairs its cut points).
/// Concave notches and holes that cross the whole slab give separate stretches.
fn slab_cover(edges: &[(P, P)], ids: &[u32], a: f64, b: f64) -> Vec<(f64, f64)> {
    let mut iv: Vec<(f64, f64)> = vec![];
    let (mut ca, mut cb): (Vec<f64>, Vec<f64>) = (vec![], vec![]);
    for &i in ids {
        let (p, q) = edges[i as usize];
        let dx = q[0] - p[0];
        for (c, cut) in [(a, &mut ca), (b, &mut cb)] {
            if (p[0] <= c) != (q[0] <= c) {
                cut.push(p[1] + (c - p[0]) / dx * (q[1] - p[1]));
            }
        }
        let (lo, hi) = (p[0].min(q[0]), p[0].max(q[0]));
        if hi < a || lo > b {
            continue;
        }
        if dx.abs() < 1e-12 {
            iv.push((p[1].min(q[1]), p[1].max(q[1])));
            continue;
        }
        let (ta, tb) = ((a - p[0]) / dx, (b - p[0]) / dx);
        let (t0, t1) = (ta.min(tb).max(0.0), ta.max(tb).min(1.0));
        if t0 > t1 {
            continue;
        }
        let (y0, y1) = (p[1] + t0 * (q[1] - p[1]), p[1] + t1 * (q[1] - p[1]));
        iv.push((y0.min(y1), y0.max(y1)));
    }
    for mut cut in [ca, cb] {
        cut.sort_by(f64::total_cmp);
        for w in cut.chunks_exact(2) {
            iv.push((w[0], w[1]));
        }
    }
    merge(iv)
}

/// Build the strip profile of a shape.
///
/// `polys` are the shape's contours in footprint coordinates (box `0..w` × `0..h`),
/// taken even-odd: outlines plus the holes that stay open. The outline is grown by
/// `grow` mm (the original offsets by half the gap). The rebuild grows it as a
/// square around every point (the slab is widened by `grow` and every stretch
/// lengthened by `grow` at both ends) — a little more than a round offset on
/// slanted edges, never less, so the gap always holds (design choice).
/// `band` is the strip width in units (B057).
pub fn profile(polys: &[Vec<P>], w: f64, h: f64, grow: f64, band: i32) -> Profile {
    let band = band.max(MIN_STRIP);
    let sx = ((w + 2.0 * grow) * UNITS).ceil().max(1.0) as i32;
    let box_top = ((h + 2.0 * grow) * UNITS).ceil().max(1.0) as i32;
    let edges: Vec<(P, P)> = polys.iter().filter(|p| p.len() >= 2).flat_map(|p| (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()]))).filter(|(a, b)| a != b).collect();
    // A regular grid of width B057 from local X = 0; the last strip is cut back
    // to the end (a sliver under 0.5 mm joins the strip before it).
    let mut bx: Vec<i32> = (0..).map(|k| k * band).take_while(|&x| x < sx).collect();
    while bx.len() > 1 && sx - bx[bx.len() - 1] < MIN_STRIP {
        bx.pop();
    }
    // A boundary where a straight edge runs along Y, if both pieces stay ≥ 0.5 mm.
    for &(p, q) in &edges {
        if (p[0] - q[0]).abs() > 1e-6 || (p[1] - q[1]).abs() <= SPLIT_EDGE {
            continue;
        }
        let e = (p[0] + q[0]) / 2.0;
        let my = (p[1] + q[1]) / 2.0;
        // Material on the left: the grown edge is at e + grow (local e + 2·grow);
        // on the right: at e − grow (local e).
        let s = if inside(&edges, [e - 1e-3, my]) { ((e + 2.0 * grow) * UNITS).ceil() as i32 } else { (e * UNITS).floor() as i32 };
        let i = bx.partition_point(|&x| x < s);
        let prev = if i > 0 { bx[i - 1] } else { i32::MIN / 2 };
        let next = bx.get(i).copied().unwrap_or(sx);
        if s - prev >= MIN_STRIP && next - s >= MIN_STRIP {
            bx.insert(i, s);
        }
    }
    bx.push(sx);
    let n = bx.len() - 1;
    // Edges by the strips they reach (slab of strip k, footprint coordinates:
    // [bx[k]/10 − 2·grow, bx[k+1]/10]).
    let mut buckets: Vec<Vec<u32>> = vec![vec![]; n];
    for (ei, &(p, q)) in edges.iter().enumerate() {
        let (lo, hi) = (p[0].min(q[0]), p[0].max(q[0]));
        let k0 = bx[1..].partition_point(|&x| (x as f64) / UNITS < lo - 1e-6);
        let k1 = bx[..n].partition_point(|&x| (x as f64) / UNITS - 2.0 * grow <= hi + 1e-6);
        for b in buckets.iter_mut().take(k1).skip(k0) {
            b.push(ei as u32);
        }
    }
    let mut strips: Vec<Strip> = Vec::with_capacity(n);
    for k in 0..n {
        let (a, b) = (bx[k] as f64 / UNITS - 2.0 * grow + EPS, bx[k + 1] as f64 / UNITS - EPS);
        let cover = slab_cover(&edges, &buckets[k], a, b);
        // Local Y = footprint Y + grow; each stretch lengthened by grow at both ends.
        let iv: Vec<(i32, i32)> = cover.iter().map(|&(lo, hi)| (((lo * UNITS).floor() as i32).max(0), (((hi + 2.0 * grow) * UNITS).ceil() as i32).min(box_top))).collect();
        strips.push(Strip { x0: bx[k], x1: bx[k + 1], iv: merge(iv) });
    }
    // Nothing found → the part's box.
    if strips.iter().all(|s| s.iv.is_empty()) {
        for s in &mut strips {
            s.iv = vec![(0, box_top)];
        }
    }
    // Neighbouring strips with the same stretches become one wider strip.
    let mut merged: Vec<Strip> = Vec::with_capacity(strips.len());
    for s in strips {
        match merged.last_mut() {
            Some(l) if l.iv == s.iv => l.x1 = s.x1,
            _ => merged.push(s),
        }
    }
    // Centre of gravity of the stretches.
    let (mut m, mut mx, mut my) = (0.0, 0.0, 0.0);
    for s in &merged {
        let wdt = (s.x1 - s.x0) as f64;
        for &(lo, hi) in &s.iv {
            let a = wdt * (hi - lo) as f64;
            m += a;
            mx += a * (s.x0 + s.x1) as f64 / 2.0;
            my += a * (lo + hi) as f64 / 2.0;
        }
    }
    let cg = if m > 0.0 { (mx / m / UNITS, my / m / UNITS) } else { (sx as f64 / 2.0 / UNITS, box_top as f64 / 2.0 / UNITS) };
    let sy = merged.iter().flat_map(|s| s.iv.iter().map(|v| v.1)).max().unwrap_or(box_top);
    Profile { strips: merged, sx, sy, cg, grow }
}

/// A spot found for a profile: its local origin at (`px`, `py`) units on the sheet,
/// the method's cost there and the cost's second key.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub px: i32,
    pub py: i32,
    /// X of the first strip where the profile fitted (where the next search for
    /// it may start).
    pub first: i32,
    pub cost: f64,
    pub key2: Option<f64>,
}

/// Drops already worked out for one profile on one sheet: X → lowest Y (or no
/// fit). A result stays true until a part is placed over that X range, because
/// placing only ever takes free space away (rebuild's speed-up; the original
/// re-drops).
#[derive(Clone, Debug, Default)]
pub struct Memo {
    seen: usize,
    drops: std::collections::HashMap<i32, Option<i32>>,
}

/// The sheet's strips with their free stretches.
#[derive(Clone, Debug)]
pub struct Sheet {
    pub strips: Vec<Strip>,
    /// X range of every part placed so far, in order (for `Memo`).
    placed: Vec<(i32, i32)>,
    /// Usable area, units.
    pub x0: i32,
    pub x1: i32,
    pub y0: i32,
    pub y1: i32,
}

impl Sheet {
    /// A sheet whose usable area (mm) is `x0..x1` × `y0..y1`
    /// is cut into a grid of width `band` units; the leftover width (span mod width)
    /// becomes the first strip when it is at least 0.5 mm (B596 static 1), else it
    /// widens the first strip. Every strip starts with one free stretch.
    pub fn new(x0: f64, x1: f64, y0: f64, y1: f64, band: i32) -> Sheet {
        let band = band.max(MIN_STRIP);
        let (x0, x1) = ((x0 * UNITS).ceil() as i32, (x1 * UNITS).floor() as i32);
        let (y0, y1) = ((y0 * UNITS).ceil() as i32, (y1 * UNITS).floor() as i32);
        let mut strips = vec![];
        if x1 - x0 >= MIN_STRIP && y1 > y0 {
            let rem = (x1 - x0) % band;
            let mut x = x0;
            if rem >= MIN_STRIP {
                strips.push(Strip { x0, x1: x0 + rem, iv: vec![(y0, y1)] });
                x += rem;
            } else if x1 - x0 > band {
                strips.push(Strip { x0, x1: x0 + band + rem, iv: vec![(y0, y1)] });
                x += band + rem;
            }
            while x < x1 {
                let e = (x + band).min(x1);
                strips.push(Strip { x0: x, x1: e, iv: vec![(y0, y1)] });
                x = e;
            }
        }
        Sheet { strips, placed: vec![], x0, x1, y0, y1 }
    }

    /// Keep only the free space inside `region` (mm, even-odd: outer outlines and
    /// their holes), for a sheet that isn't a rectangle — a remnant cut to its
    /// real shape (RE26). Each strip keeps the Y stretches where its whole width
    /// lies inside the region; everything else counts as taken, so a part's strips
    /// can never reach outside the shape. Call before placing anything.
    pub fn keep_inside(&mut self, region: &[Vec<P>]) {
        let (fx0, fx1) = (self.x0 as f64 / UNITS - 1.0, self.x1 as f64 / UNITS + 1.0);
        let (fy0, fy1) = (self.y0 as f64 / UNITS - 1.0, self.y1 as f64 / UNITS + 1.0);
        // The material is the frame around the sheet less the region (even-odd).
        let frame = vec![[fx0, fy0], [fx1, fy0], [fx1, fy1], [fx0, fy1]];
        let polys: Vec<&Vec<P>> = std::iter::once(&frame).chain(region.iter()).filter(|p| p.len() >= 3).collect();
        let edges: Vec<(P, P)> = polys.iter().flat_map(|p| (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()]))).filter(|(a, b)| a != b).collect();
        let ids: Vec<u32> = (0..edges.len() as u32).collect();
        for s in &mut self.strips {
            let (a, b) = (s.x0 as f64 / UNITS, s.x1 as f64 / UNITS);
            for (lo, hi) in slab_cover(&edges, &ids, a, b) {
                subtract(&mut s.iv, (lo * UNITS).floor() as i32, (hi * UNITS).ceil() as i32);
            }
        }
    }

    /// Lowest Y (units) where `prof` fits with its first strip lined up with sheet
    /// strip `si`, or None (the original's drop): every
    /// part stretch must lie inside one free stretch of every sheet strip it
    /// overlaps. Y only goes up: each stretch asks for the smallest Y ≥ the current
    /// one where it fits; the largest of those becomes the new Y, until all agree.
    /// Exact to 0.1 mm.
    pub fn drop_at(&self, prof: &Profile, si: usize) -> Option<i32> {
        let px = self.strips.get(si)?.x0;
        if px + prof.sx > self.x1 || prof.sy > self.y1 - self.y0 {
            return None;
        }
        let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(prof.strips.len() + 8);
        let (mut j, mut k) = (0usize, si);
        while j < prof.strips.len() && k < self.strips.len() {
            let (pj, s) = (&prof.strips[j], &self.strips[k]);
            let (a, b) = (px + pj.x0, px + pj.x1);
            if s.x1 <= a {
                k += 1;
                continue;
            }
            if !pj.iv.is_empty() {
                pairs.push((j, k));
            }
            match s.x1.cmp(&b) {
                std::cmp::Ordering::Less => k += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    j += 1;
                    k += 1;
                }
            }
        }
        if j < prof.strips.len() {
            return None; // runs past the last sheet strip
        }
        let mut y = self.y0;
        loop {
            let mut ny = y;
            for &(j, k) in &pairs {
                let free = &self.strips[k].iv;
                for &(lo, hi) in &prof.strips[j].iv {
                    let len = hi - lo;
                    let mut i = free.partition_point(|f| f.1 - hi < y);
                    while i < free.len() && free[i].1 - free[i].0 < len {
                        i += 1;
                    }
                    let f = free.get(i)?;
                    ny = ny.max(f.0 - lo);
                }
            }
            if ny == y {
                return Some(y);
            }
            y = ny;
            if y + prof.sy > self.y1 {
                return None;
            }
        }
    }

    /// Is everything under the profile at strip `si` still untouched sheet
    /// (the original's "strip free" test)?
    fn free_from(&self, si: usize, sx: i32) -> bool {
        let end = self.strips[si].x0 + sx;
        self.strips[si..].iter().take_while(|s| s.x0 < end).all(|s| s.iv.len() == 1 && s.iv[0] == (self.y0, self.y1))
    }

    /// The best spot for a profile:
    /// sheet strips are tried from `from_x` onward. With `full` false the first
    /// strip where the part fits wins at once; with `full` (methods 8–11) every
    /// strip is tried until the rest of the sheet is untouched, and a spot replaces
    /// the best so far only when its cost is lower and either lower by at least
    /// `tie` or its second key is lower too. `cost` must never fall as Y rises or
    /// X grows (true of every rule in `nest::spot_cost`).
    pub fn search(&self, prof: &Profile, from_x: i32, full: bool, tie: f64, memo: &mut Memo, mut cost: impl FnMut(i32, i32) -> (f64, Option<f64>)) -> Option<Spot> {
        if memo.seen > self.placed.len() {
            memo.drops.clear();
        } else if memo.seen < self.placed.len() {
            let new = &self.placed[memo.seen..];
            memo.drops.retain(|&px, _| !new.iter().any(|&(a, b)| px < b && px + prof.sx > a));
        }
        memo.seen = self.placed.len();
        let start = self.strips.partition_point(|s| s.x0 < from_x);
        let mut best: Option<Spot> = None;
        let mut first = None;
        for si in start..self.strips.len() {
            let px = self.strips[si].x0;
            if px + prof.sx > self.x1 {
                break;
            }
            // Every cost rule only grows as Y rises or X moves on, so once the
            // cost at the bottom of this strip can't beat the best, no later strip
            // can (rebuild's exact shortcut; the original scans on).
            if let Some(b) = &best {
                if cost(px, self.y0).0 >= b.cost {
                    break;
                }
            }
            let dropped = match memo.drops.get(&px) {
                Some(&r) => r,
                None => {
                    let r = self.drop_at(prof, si);
                    memo.drops.insert(px, r);
                    r
                }
            };
            let Some(py) = dropped else { continue };
            let (c, k2) = cost(px, py);
            let first = *first.get_or_insert(px);
            let sp = Spot { px, py, first, cost: c, key2: k2 };
            if !full {
                return Some(sp);
            }
            let better = match &best {
                None => true,
                Some(b) => c < b.cost && (b.cost - c >= tie || matches!((k2, b.key2), (Some(x), Some(y)) if x < y)),
            };
            if better {
                best = Some(Spot { first, ..sp });
            }
            if self.free_from(si, prof.sx) {
                break;
            }
        }
        best
    }

    /// Put a profile on the sheet with its origin at (`px`, `py`)
    /// (as the original updates the sheet's strips): the sheet strips are split
    /// where the part's strips start and end, so the next part can go flush
    /// against it, then each part stretch is cut out of the free stretches under it.
    pub fn place(&mut self, prof: &Profile, px: i32, py: i32) {
        self.placed.push((px, px + prof.sx));
        let mut cuts: Vec<i32> = prof.strips.iter().map(|s| px + s.x0).collect();
        cuts.push(px + prof.sx);
        let mut out: Vec<Strip> = Vec::with_capacity(self.strips.len() + cuts.len());
        let mut ci = 0;
        for s in self.strips.drain(..) {
            while ci < cuts.len() && cuts[ci] <= s.x0 {
                ci += 1;
            }
            let mut x = s.x0;
            while ci < cuts.len() && cuts[ci] < s.x1 {
                out.push(Strip { x0: x, x1: cuts[ci], iv: s.iv.clone() });
                x = cuts[ci];
                ci += 1;
            }
            out.push(Strip { x0: x, x1: s.x1, iv: s.iv });
        }
        self.strips = out;
        let mut j = 0;
        for s in self.strips.iter_mut() {
            if s.x0 < px || s.x1 > px + prof.sx {
                continue;
            }
            while j < prof.strips.len() && px + prof.strips[j].x1 <= s.x0 {
                j += 1;
            }
            let Some(pj) = prof.strips.get(j) else { break };
            for &(lo, hi) in &pj.iv {
                subtract(&mut s.iv, lo + py, hi + py);
            }
        }
    }
}

/// Take `[a, b]` out of a sorted free list:
/// a free stretch may split in two. Touching ends stay free.
fn subtract(free: &mut Vec<(i32, i32)>, a: i32, b: i32) {
    if !free.iter().any(|&(f0, f1)| f1 > a && f0 < b) {
        return;
    }
    let mut out = Vec::with_capacity(free.len() + 1);
    for &(f0, f1) in free.iter() {
        if f1 <= a || f0 >= b {
            out.push((f0, f1));
            continue;
        }
        if f0 < a {
            out.push((f0, a));
        }
        if b < f1 {
            out.push((b, f1));
        }
    }
    *free = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    #[test]
    fn a_rectangle_is_one_strip_grown_by_half_the_gap() {
        let p = profile(&[rect(0.0, 0.0, 100.0, 50.0)], 100.0, 50.0, 5.0, 139);
        assert_eq!(p.sx, 1100);
        assert_eq!(p.strips, vec![Strip { x0: 0, x1: 1100, iv: vec![(0, 600)] }]);
        assert!((p.cg.0 - 55.0).abs() < 1e-9 && (p.cg.1 - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_hole_across_the_strip_stays_free_and_a_notch_is_open() {
        // 100 × 100 plate with a 40 × 40 hole in the middle.
        let mut hole = rect(30.0, 30.0, 70.0, 70.0);
        hole.reverse();
        let p = profile(&[rect(0.0, 0.0, 100.0, 100.0), hole], 100.0, 100.0, 2.0, 50);
        // Strips fully across the hole keep it open, shrunk by the grow at each
        // end: material local [0, 34] and [70, 104] mm (hole 30..70 + 2 mm).
        let s = p.strips.iter().find(|s| s.x0 <= 400 && s.x1 >= 640).expect("a strip across the hole");
        assert_eq!(s.iv, vec![(0, 340), (700, 1040)]);
        assert_eq!((s.x0, s.x1), (340, 700), "the hole's straight sides are strip edges");
        // An L opening up and right: past the upright the strips only hold the foot.
        let l = profile(&[vec![[0.0, 0.0], [100.0, 0.0], [100.0, 40.0], [40.0, 40.0], [40.0, 100.0], [0.0, 100.0]]], 100.0, 100.0, 2.5, 50);
        assert!(l.strips.iter().any(|s| s.x0 == 450), "the upright's straight side is a strip edge: {:?}", l.strips);
        assert_eq!(l.strips.last().unwrap().iv, vec![(0, 450)]);
        assert_eq!(l.strips[0].iv, vec![(0, 1050)]);
    }

    #[test]
    fn placing_splits_the_sheet_strips_and_takes_the_stretches_out() {
        let mut sh = Sheet::new(0.0, 100.0, 0.0, 50.0, 100);
        assert_eq!(sh.strips.len(), 10);
        let p = profile(&[rect(0.0, 0.0, 15.0, 10.0)], 15.0, 10.0, 0.0, 100);
        assert_eq!(sh.drop_at(&p, 0), Some(0));
        sh.place(&p, 0, 0);
        // Split at X 15 so the next part can start flush.
        assert!(sh.strips.iter().any(|s| s.x0 == 150));
        assert_eq!(sh.strips[0].iv, vec![(100, 500)]);
        assert_eq!(sh.drop_at(&p, 0), Some(100));
        let at15 = sh.strips.iter().position(|s| s.x0 == 150).unwrap();
        assert_eq!(sh.drop_at(&p, at15), Some(0));
        let mut v = vec![(0, 100)];
        subtract(&mut v, 20, 30);
        assert_eq!(v, vec![(0, 20), (30, 100)]);
    }
}
