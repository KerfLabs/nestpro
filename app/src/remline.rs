//! The automatic remnant line, all nine shapes of the original's Sheet
//! Remnants dialog (manual p. 556; MECHANICS.md rules ST8 and ST11).
//!
//! - Shapes 1–5 work from the parts' **boxes**, each grown by the parts↔line
//!   offset and stretched in X to the edge nesting started from. Min X /
//!   Min Y keep the parts side at least that long; a strip or leg
//!   narrower than the minimum width gives no line.
//! - Shape 4 picks the L whose two remnants are the most square.
//! - Shape 6 is an orthogonal staircase hugging the boxes.
//! - Shapes 8/9 follow the parts' real outer outlines (holes and parts in
//!   holes left out), grown by the offset; if that fails, a staircase.
//! - Straight cuts run 2 × tolerance past the sheet edges. Rounding puts
//!   a fillet at the joints, not on shape 8.

use crate::cutoff::{CutKind, CutLine};
use crate::nest::{Part, Placed, SheetSpec, P};
use crate::stock::Outline;
use serde::{Deserialize, Serialize};

/// The original's linear tolerance (mm; its DXF settings' default 0.01).
pub const TOL: f64 = 0.01;

/// The shapes in the dialog's order (icons of Bitmaps/LINRETAL.gif). The
/// number is the original's internal shape code.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum LineShape {
    /// 1: one vertical cut — a strip at the far side in X.
    #[default]
    VerticalStrip,
    /// 7: one horizontal cut — a strip at the far side in Y.
    HorizontalStrip,
    /// 2: a full-height vertical cut, then a horizontal cut over the parts.
    LVerticalFirst,
    /// 3: a full-width horizontal cut, then a vertical cut over the parts.
    LHorizontalFirst,
    /// 4: the L whose two remnants are the most square.
    LAuto,
    /// 5: one step — horizontal, vertical, horizontal.
    OneStep,
    /// 6: an orthogonal staircase following the parts.
    Staircase,
    /// 9: following the parts' outlines at the offset ("optimized irregular").
    Optimized,
    /// 8: following the parts' outlines at the cut ("exact irregular").
    Exact,
}

impl LineShape {
    pub const ALL: [LineShape; 9] = [
        LineShape::VerticalStrip,
        LineShape::HorizontalStrip,
        LineShape::LVerticalFirst,
        LineShape::LHorizontalFirst,
        LineShape::LAuto,
        LineShape::OneStep,
        LineShape::Staircase,
        LineShape::Optimized,
        LineShape::Exact,
    ];
    /// The original's internal code for the shape (as stored in its settings).
    pub fn code(self) -> u8 {
        match self {
            LineShape::VerticalStrip => 1,
            LineShape::LVerticalFirst => 2,
            LineShape::LHorizontalFirst => 3,
            LineShape::LAuto => 4,
            LineShape::OneStep => 5,
            LineShape::Staircase => 6,
            LineShape::HorizontalStrip => 7,
            LineShape::Exact => 8,
            LineShape::Optimized => 9,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            LineShape::VerticalStrip => "Vertical strip",
            LineShape::HorizontalStrip => "Horizontal strip",
            LineShape::LVerticalFirst => "L, vertical cut first",
            LineShape::LHorizontalFirst => "L, horizontal cut first",
            LineShape::LAuto => "L, the squarest remnants",
            LineShape::OneStep => "One step",
            LineShape::Staircase => "Staircase",
            LineShape::Optimized => "Irregular, at the offset",
            LineShape::Exact => "Irregular, at the cut",
        }
    }
    /// Min X / Min Y apply to the first six (manual p. 557).
    pub fn uses_min_xy(self) -> bool {
        matches!(self, LineShape::VerticalStrip | LineShape::HorizontalStrip | LineShape::LVerticalFirst | LineShape::LHorizontalFirst | LineShape::LAuto | LineShape::OneStep)
    }
}

/// The Sheet Remnants line options (manual pp. 556–557).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineSettings {
    pub shape: LineShape,
    /// Offset between the parts and the line (mm). Ignored with `cutter_radius`.
    pub offset: f64,
    /// Use the cutter radius as the offset (stored as −1 in the original).
    pub cutter_radius: bool,
    /// Uncut bridge left where the line meets the sheet edge (mm; 0 = none).
    pub edge_offset: f64,
    /// A strip narrower than this gives no line (mm).
    pub min_width: f64,
    /// The parts side is at least this long from the nesting origin (mm).
    pub min_x: f64,
    pub min_y: f64,
    pub round: bool,
    pub radius: f64,
}

impl Default for LineSettings {
    fn default() -> Self {
        // As in the manual's dialog picture (p. 556) where shown; rounding as
        // the rebuild had it.
        // The original's defaults (rule ST11); radius 10 shown
        // when rounding is switched on (the original's is 0).
        LineSettings { shape: LineShape::VerticalStrip, offset: 10.0, cutter_radius: false, edge_offset: 0.0, min_width: 100.0, min_x: 100.0, min_y: 100.0, round: false, radius: 10.0 }
    }
}

/// Axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct B {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// Which edges nesting started from:
/// true = from the low side (left / bottom).
fn directions(boxes: &[B], w: f64, h: f64) -> (bool, bool) {
    let x0 = boxes.iter().map(|b| b.x0).fold(f64::MAX, f64::min);
    let x1 = boxes.iter().map(|b| b.x1).fold(f64::MIN, f64::max);
    let y0 = boxes.iter().map(|b| b.y0).fold(f64::MAX, f64::min);
    let y1 = boxes.iter().map(|b| b.y1).fold(f64::MIN, f64::max);
    (x0 <= w - x1, y0 <= h - y1)
}

/// The parts' boxes grown by the offset; stretched in X to the origin edge.
fn obstacle_boxes(sheet: &[Placed], off: f64, w: f64, h: f64, stretch: bool) -> (Vec<B>, bool, bool) {
    let raw: Vec<B> = sheet.iter().map(|p| B { x0: p.x - off, y0: p.y - off, x1: p.x + p.w + off, y1: p.y + p.h + off }).collect();
    let (from_left, from_bottom) = directions(&raw, w, h);
    let boxes = raw
        .into_iter()
        .map(|mut b| {
            if stretch {
                if from_left { b.x0 = 0.0 } else { b.x1 = w }
            }
            b
        })
        .collect();
    (boxes, from_left, from_bottom)
}

fn union_box(bs: &[B]) -> B {
    B {
        x0: bs.iter().map(|b| b.x0).fold(f64::MAX, f64::min),
        y0: bs.iter().map(|b| b.y0).fold(f64::MAX, f64::min),
        x1: bs.iter().map(|b| b.x1).fold(f64::MIN, f64::max),
        y1: bs.iter().map(|b| b.y1).fold(f64::MIN, f64::max),
    }
}

fn line(pts: Vec<P>) -> CutLine {
    CutLine { pts, kind: CutKind::Remnant }
}

/// Where a vertical cut goes (for X; Y works the same way), or None when the
/// strip left would be narrower than the minimum width. `lo`/`hi` are the
/// parts box along the axis, `len` the sheet's size along it.
fn cut_at(lo: f64, hi: f64, len: f64, from_low: bool, min_width: f64, min_parts: f64) -> Option<f64> {
    if from_low {
        if len - hi < min_width {
            return None;
        }
        Some(if min_parts > hi && min_parts < len { min_parts } else { hi })
    } else {
        if lo < min_width {
            return None;
        }
        Some(if (len - lo) < min_parts && len - min_parts > 0.0 { len - min_parts } else { lo })
    }
}

/// Short/long side ratio.
fn sq(a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 { 0.0 } else { a.min(b) / a.max(b) }
}

/// The automatic remnant line(s) for a sheet. Several lines make an L or the
/// irregular shapes' separate cuts. Empty when no remnant fits.
pub fn auto_lines(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, s: &LineSettings, cutter_radius: f64) -> Vec<CutLine> {
    if sheet.is_empty() {
        return vec![];
    }
    let (w, h) = (spec.w, spec.h);
    let off = if s.cutter_radius { cutter_radius } else { s.offset.max(0.0) };
    let e = 2.0 * TOL;
    let (mx, my) = if s.shape.uses_min_xy() { (s.min_x, s.min_y) } else { (0.0, 0.0) };
    let mut out = match s.shape {
        LineShape::Staircase => staircase(sheet, w, h, off, s.min_width).into_iter().collect(),
        LineShape::Optimized | LineShape::Exact => {
            let o = if s.shape == LineShape::Exact { cutter_radius.max(3.0 * TOL) } else { off.max(3.0 * TOL) };
            let v = irregular(parts, sheet, w, h, o, s.min_width);
            if v.is_empty() { staircase(sheet, w, h, o, s.min_width).into_iter().collect() } else { v }
        }
        _ => {
            let (boxes, from_left, from_bottom) = obstacle_boxes(sheet, off, w, h, true);
            let pb = union_box(&boxes);
            let vx = cut_at(pb.x0, pb.x1, w, from_left, s.min_width, mx);
            let hy = cut_at(pb.y0, pb.y1, h, from_bottom, s.min_width, my);
            let xa = if from_left { -e } else { w + e };
            let ya = if from_bottom { -e } else { h + e };
            let vert = |x: f64| line(vec![[x, -e], [x, h + e]]);
            let horiz = |y: f64| line(vec![[-e, y], [w + e, y]]);
            // A leg over the parts side only, from the origin edge to the other cut.
            let h_leg = |y: f64, x: f64| line(vec![[xa, y], [x, y]]);
            let v_leg = |x: f64, y: f64| line(vec![[x, ya], [x, y]]);
            let l_vertical = || match (vx, hy) {
                (Some(x), Some(y)) => vec![vert(x), h_leg(y, x)],
                (Some(x), None) => vec![vert(x)],
                (None, Some(y)) => vec![horiz(y)],
                _ => vec![],
            };
            let l_horizontal = || match (vx, hy) {
                (Some(x), Some(y)) => vec![horiz(y), v_leg(x, y)],
                (None, Some(y)) => vec![horiz(y)],
                (Some(x), None) => vec![vert(x)],
                _ => vec![],
            };
            match s.shape {
                LineShape::VerticalStrip => vx.map(vert).into_iter().collect(),
                LineShape::HorizontalStrip => hy.map(horiz).into_iter().collect(),
                LineShape::LVerticalFirst => l_vertical(),
                LineShape::LHorizontalFirst => l_horizontal(),
                LineShape::LAuto => match (vx, hy) {
                    (Some(x), Some(y)) => {
                        // Remnant strip and parts-side sizes along each axis.
                        let (rem_w, parts_w) = if from_left { (w - x, x) } else { (x, w - x) };
                        let (rem_h, parts_h) = if from_bottom { (h - y, y) } else { (y, h - y) };
                        // A: vertical first — (rem_w × h) and (parts_w × rem_h).
                        // B: horizontal first — (rem_w × parts_h) and (w × rem_h).
                        let a = sq(rem_w, h) * sq(parts_w, rem_h);
                        let b = sq(rem_w, parts_h) * sq(w, rem_h);
                        if b <= a { l_vertical() } else { l_horizontal() }
                    }
                    _ => l_vertical(),
                },
                LineShape::OneStep => one_step(sheet, w, h, off, s.min_width, my).into_iter().collect(),
                _ => vec![],
            }
        }
    };
    if s.round && s.shape != LineShape::Exact {
        for l in out.iter_mut() {
            if l.pts.len() > 2 {
                l.pts = crate::cutoff::round_corners(&l.pts, s.radius);
            }
        }
    }
    out
}

/// One step: across the top of the taller parts from the
/// origin edge, down (or up) once, and across the rest — the step placed to
/// leave the largest remnant. Falls back to a straight horizontal cut.
fn one_step(sheet: &[Placed], w: f64, h: f64, off: f64, min_width: f64, min_y: f64) -> Option<CutLine> {
    let (boxes, from_left, from_bottom) = obstacle_boxes(sheet, off, w, h, false);
    if !from_bottom {
        // Mirror in Y, step, mirror back.
        let flipped: Vec<Placed> = sheet.iter().map(|p| Placed { y: h - p.y - p.h, ..*p }).collect();
        return one_step(&flipped, w, h, off, min_width, min_y).map(|mut l| {
            for p in l.pts.iter_mut() {
                p[1] = h - p[1];
            }
            l
        });
    }
    let e = 2.0 * TOL;
    // Candidate step positions: the boxes' X edges.
    let mut xs: Vec<f64> = boxes.iter().flat_map(|b| [b.x0, b.x1]).filter(|x| *x > 0.0 && *x < w).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|a, b| (*a - *b).abs() < TOL);
    let top = |a: f64, b: f64| boxes.iter().filter(|q| q.x1 > a + TOL && q.x0 < b - TOL).map(|q| q.y1).fold(0.0, f64::max).max(min_y);
    let mut best: Option<(f64, CutLine)> = None;
    for &xs_ in &xs {
        let (ya, yb) = if from_left { (top(0.0, xs_), top(xs_, w)) } else { (top(xs_, w), top(0.0, xs_)) };
        if h - ya < min_width && h - yb < min_width {
            continue;
        }
        let (y_l, y_r) = if from_left { (ya, yb) } else { (yb, ya) };
        let area = (xs_) * (h - y_l) + (w - xs_) * (h - y_r);
        let pts = vec![[-e, y_l], [xs_, y_l], [xs_, y_r], [w + e, y_r]];
        if best.as_ref().is_none_or(|b| area > b.0) {
            best = Some((area, line(pts)));
        }
    }
    let flat = top(0.0, w);
    let flat_area = w * (h - flat);
    match best {
        Some((a, l)) if a > flat_area + 1.0 => Some(l),
        _ if h - flat >= min_width => Some(line(vec![[-e, flat], [w + e, flat]])),
        _ => None,
    }
}

/// Orthogonal staircase: bottom to top, the line keeps just
/// outside the boxes in each horizontal band, so the remnant is on the far
/// side in X. Steps narrower than the minimum width are levelled out.
fn staircase(sheet: &[Placed], w: f64, h: f64, off: f64, min_width: f64) -> Option<CutLine> {
    let (boxes, from_left, _) = obstacle_boxes(sheet, off, w, h, true);
    let e = 2.0 * TOL;
    let mut ys: Vec<f64> = boxes.iter().flat_map(|b| [b.y0, b.y1]).map(|y| y.clamp(0.0, h)).chain([0.0, h]).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.dedup_by(|a, b| (*a - *b).abs() < TOL);
    // The line's X in each band (the far edge of the boxes it meets).
    let mut bands: Vec<(f64, f64, f64)> = ys
        .windows(2)
        .map(|y| {
            let hit = boxes.iter().filter(|b| b.y1 > y[0] + TOL && b.y0 < y[1] - TOL);
            let x = if from_left { hit.map(|b| b.x1).fold(0.0, f64::max) } else { hit.map(|b| b.x0).fold(w, f64::min) };
            (y[0], y[1], x)
        })
        .collect();
    // Bands shorter than the minimum width take the further-out neighbour's X
    // (no teeth or slivers).
    let further = |a: f64, b: f64| if from_left { a.max(b) } else { a.min(b) };
    loop {
        let mut changed = false;
        for i in 0..bands.len() {
            let tall = bands[i].1 - bands[i].0;
            if tall < min_width && bands.len() > 1 {
                let j = if i == 0 { 1 } else if i + 1 == bands.len() { i - 1 } else if (bands[i - 1].2 - bands[i].2).abs() < (bands[i + 1].2 - bands[i].2).abs() { i - 1 } else { i + 1 };
                let x = further(bands[i].2, bands[j].2);
                let (lo, hi) = (bands[i].0.min(bands[j].0), bands[i].1.max(bands[j].1));
                bands[j] = (lo, hi, x);
                bands.remove(i);
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    // Merge neighbours at the same X.
    bands.dedup_by(|b, a| {
        if (a.2 - b.2).abs() < TOL {
            a.1 = b.1;
            true
        } else {
            false
        }
    });
    let free = |x: f64| if from_left { w - x } else { x };
    if bands.iter().all(|b| free(b.2) < min_width) {
        return None;
    }
    let mut pts: Vec<P> = vec![];
    for (k, &(y0, y1, x)) in bands.iter().enumerate() {
        let x = if from_left { x.min(w) } else { x.max(0.0) };
        let y0 = if k == 0 { -e } else { y0 };
        let y1 = if k + 1 == bands.len() { h + e } else { y1 };
        pts.push([x, y0]);
        pts.push([x, y1]);
    }
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL);
    Some(line(pts))
}

/// The free space at `off` from the parts, less the slivers narrower than
/// the minimum width next to them (between parts, or between the parts and the
/// sheet edge). `orthogonal`: the parts count by their boxes (square cuts
/// only); otherwise by their outlines, grown at a constant distance (round
/// offsets — mitred ones would cut corners). `holes`: empty part holes are
/// free space too (semi-automatic, manual p. 554); the automatic line keeps to
/// outer outlines.
pub fn free_pieces(parts: &[Part], sheet: &[Placed], w: f64, h: f64, off: f64, min_width: f64, orthogonal: bool, holes: bool) -> Vec<Outline> {
    let shapes: Vec<Outline> = sheet
        .iter()
        .map(|p| {
            if orthogonal {
                Outline { outer: vec![[p.x, p.y], [p.x + p.w, p.y], [p.x + p.w, p.y + p.h], [p.x, p.y + p.h]], holes: vec![] }
            } else {
                let (outer, hs) = crate::nest::placed_shape(parts, p);
                Outline { outer, holes: if holes { hs } else { vec![] } }
            }
        })
        .filter(|o| o.outer.len() >= 3)
        .collect();
    if shapes.is_empty() {
        return vec![];
    }
    let grow = |o: &Outline, d: f64| if orthogonal { crate::region::offset(o, d) } else { crate::region::offset_round(o, d) };
    // Union of the grown shapes, closed over narrow gaps.
    let close = (min_width / 2.0).max(0.0);
    let mut blob: Vec<Outline> = vec![];
    for o in &shapes {
        blob.extend(grow(o, -(off + close)));
    }
    let blob = crate::region::union_all(&blob);
    let blob: Vec<Outline> = blob.iter().flat_map(|o| grow(o, close)).collect();
    let sheet_o = Outline { outer: vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], holes: vec![] };
    // Only slivers touching the parts go: opening also rounds the sheet's own
    // corners, which must stay.
    let free0 = crate::region::difference(&sheet_o, &blob);
    let opened: Vec<Outline> = free0
        .iter()
        .flat_map(|p| grow(p, close))
        .flat_map(|p| grow(&p, -close))
        .flat_map(|p| crate::region::intersection(&p, &sheet_o))
        .collect();
    let near: Vec<Outline> = crate::region::union_all(&blob.iter().flat_map(|b| grow(b, -(close + 1.0))).collect::<Vec<_>>());
    let slivers: Vec<Outline> = free0
        .iter()
        .flat_map(|f| crate::region::difference(f, &opened))
        .filter(|s| near.iter().any(|n| crate::region::intersection(s, n).iter().any(|o| o.area_mm2() > 1.0)))
        .collect();
    free0.iter().flat_map(|f| crate::region::difference(f, &slivers)).collect()
}

fn on_sheet_edge(p: P, w: f64, h: f64) -> bool {
    p[0].abs() < TOL || (p[0] - w).abs() < TOL || p[1].abs() < TOL || (p[1] - h).abs() < TOL
}

/// The cuts that free a piece: its boundary less the runs along the sheet
/// edge, or its whole outline when it doesn't reach the edge (a gap between
/// parts, a part's hole).
pub fn lines_around(piece: &Outline, w: f64, h: f64) -> Vec<CutLine> {
    let ring = &piece.outer;
    let n = ring.len();
    if n < 3 {
        return vec![];
    }
    let Some(start) = (0..n).find(|&i| on_sheet_edge(ring[i], w, h) && !on_sheet_edge(ring[(i + 1) % n], w, h)) else {
        if ring.iter().any(|&p| on_sheet_edge(p, w, h)) {
            return vec![];
        }
        let mut pts = ring.clone();
        pts.push(ring[0]);
        return vec![line(pts)];
    };
    let mut lines = vec![];
    let mut cur: Vec<P> = vec![];
    for k in 0..=n {
        let i = (start + k) % n;
        let p = ring[i];
        if on_sheet_edge(p, w, h) {
            if !cur.is_empty() {
                cur.push(p);
                if cur.len() >= 2 {
                    lines.push(line(std::mem::take(&mut cur)));
                }
                cur.clear();
            }
            if !on_sheet_edge(ring[(i + 1) % n], w, h) {
                cur.push(p);
            }
        } else {
            cur.push(p);
        }
    }
    lines
}

/// Irregular line: the free piece reaching furthest from where
/// nesting started, freed along the parts' outer outlines.
fn irregular(parts: &[Part], sheet: &[Placed], w: f64, h: f64, off: f64, min_width: f64) -> Vec<CutLine> {
    let free = free_pieces(parts, sheet, w, h, off, min_width, false, false);
    let boxes: Vec<B> = sheet.iter().map(|p| B { x0: p.x, y0: p.y, x1: p.x + p.w, y1: p.y + p.h }).collect();
    if boxes.is_empty() {
        return vec![];
    }
    let (from_left, from_bottom) = directions(&boxes, w, h);
    let far = |o: &Outline| -> f64 {
        o.outer.iter().map(|p| (if from_left { p[0] } else { w - p[0] }) + (if from_bottom { p[1] } else { h - p[1] })).fold(0.0, f64::max) * 1e6 + o.area_mm2()
    };
    match free.iter().filter(|o| o.area_mm2() > min_width * min_width).max_by(|a, b| far(a).partial_cmp(&far(b)).unwrap_or(std::cmp::Ordering::Equal)) {
        Some(piece) => lines_around(piece, w, h),
        None => vec![],
    }
}

/// The semi-automatic line types (Remnants – Semi-Automatic, manual p. 554).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum SemiMode {
    /// Square cuts, only in areas between the parts and the sheet edge.
    #[default]
    ExteriorOrthogonal,
    /// Following the parts at the offset; gaps and holes too.
    OptimizedIrregular,
    /// Following the parts at the cutter diameter; gaps and holes too.
    ExactIrregular,
}

impl SemiMode {
    pub const ALL: [SemiMode; 3] = [SemiMode::ExteriorOrthogonal, SemiMode::OptimizedIrregular, SemiMode::ExactIrregular];
    pub fn label(self) -> &'static str {
        match self {
            SemiMode::ExteriorOrthogonal => "Exterior orthogonal",
            SemiMode::OptimizedIrregular => "Optimized irregular",
            SemiMode::ExactIrregular => "Exact irregular",
        }
    }
}

/// Semi-automatic remnant at a clicked point (rule ST9): the free piece holding the click — never one with a part in it —
/// freed by its own cuts. Too small (under the minimum area) or no piece there:
/// an explanation instead.
pub fn semi_auto_lines(parts: &[Part], sheet: &[Placed], w: f64, h: f64, mode: SemiMode, s: &LineSettings, cutter_radius: f64, click: P, min_area_m2: f64) -> Result<Vec<CutLine>, String> {
    let off = match mode {
        SemiMode::ExactIrregular => (2.0 * cutter_radius).max(3.0 * TOL),
        _ if s.cutter_radius => cutter_radius,
        _ => s.offset.max(0.0),
    };
    let orth = mode == SemiMode::ExteriorOrthogonal;
    // The minimum width isn't used in semi-automatic (as in the original's dialog).
    let free = free_pieces(parts, sheet, w, h, off, 0.0, orth, !orth);
    let inside = |o: &Outline| crate::nest::point_in_poly(&o.outer, click) && !o.holes.iter().any(|hh| crate::nest::point_in_poly(hh, click));
    let Some(piece) = free.iter().find(|o| inside(o)) else {
        return Err("No free area there — click between the parts and the sheet edge, in a gap, or in an empty hole.".into());
    };
    if orth && !piece.outer.iter().any(|&p| on_sheet_edge(p, w, h)) {
        return Err("Exterior orthogonal only makes remnants between the parts and the sheet edge.".into());
    }
    if piece.area_mm2() < min_area_m2 * 1e6 {
        return Err(format!("That area is {:.3} m², under the minimum area of remnants ({min_area_m2:.3} m²).", piece.area_mm2() / 1e6));
    }
    let v = lines_around(piece, w, h);
    if v.is_empty() { Err("That area is the whole sheet — there's nothing to cut.".into()) } else { Ok(v) }
}

/// The line as cut on the machine: shortened by `bridge` where it meets the
/// sheet edge, leaving the remnant held by a small uncut bridge.
pub fn with_bridges(pts: &[P], w: f64, h: f64, bridge: f64) -> Vec<P> {
    if bridge <= 1e-6 || pts.len() < 2 {
        return pts.to_vec();
    }
    let near_edge = |p: P| p[0] <= TOL * 3.0 || p[0] >= w - TOL * 3.0 || p[1] <= TOL * 3.0 || p[1] >= h - TOL * 3.0;
    let mut v = pts.to_vec();
    let trim = |a: P, b: P| -> P {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        if l <= bridge { b } else { [a[0] + dx / l * bridge, a[1] + dy / l * bridge] }
    };
    let n = v.len();
    if near_edge(v[0]) {
        v[0] = trim(v[0], v[1]);
    }
    if near_edge(v[n - 1]) {
        v[n - 1] = trim(v[n - 1], v[n - 2]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    fn spec() -> SheetSpec {
        SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() }
    }
    fn part(w: f64, h: f64) -> Part {
        Part { name: "A".into(), shape: Shape::Rectangle, w, h, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn at(i: usize, x: f64, y: f64, w: f64, h: f64) -> Placed {
        Placed { part: i, x, y, w, h, turns: 0, mirror: false }
    }
    fn settings(shape: LineShape) -> LineSettings {
        LineSettings { shape, offset: 10.0, min_width: 100.0, ..Default::default() }
    }

    #[test]
    fn strips_sit_the_offset_past_the_parts() {
        let parts = vec![part(1000.0, 800.0)];
        let sheet = vec![at(0, 5.0, 5.0, 1000.0, 800.0)];
        let v = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::VerticalStrip), 0.15);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pts[0][0], 1015.0);
        assert!(v[0].pts[0][1] < 0.0 && v[0].pts[1][1] > 1500.0, "runs past the edges");
        let hz = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::HorizontalStrip), 0.15);
        assert_eq!(hz[0].pts[0][1], 815.0);
    }

    #[test]
    fn min_x_keeps_the_parts_side_long_and_narrow_strips_give_nothing() {
        let parts = vec![part(1000.0, 800.0), part(2900.0, 800.0)];
        let sheet = vec![at(0, 5.0, 5.0, 1000.0, 800.0)];
        let s = LineSettings { min_x: 1500.0, ..settings(LineShape::VerticalStrip) };
        assert_eq!(auto_lines(&parts, &sheet, &spec(), &s, 0.15)[0].pts[0][0], 1500.0);
        let wide = vec![at(1, 5.0, 5.0, 2900.0, 800.0)];
        assert!(auto_lines(&parts, &wide, &spec(), &settings(LineShape::VerticalStrip), 0.15).is_empty(), "3000 − 2915 < 100");
    }

    #[test]
    fn l_shapes_and_the_squarest_choice() {
        let parts = vec![part(1000.0, 800.0)];
        let sheet = vec![at(0, 5.0, 5.0, 1000.0, 800.0)];
        let v = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::LVerticalFirst), 0.15);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].pts[0][0], 1015.0, "full-height vertical cut");
        assert_eq!((v[1].pts[0][1], v[1].pts[1][0]), (815.0, 1015.0), "horizontal leg over the parts only");
        let v = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::LHorizontalFirst), 0.15);
        assert!(v[0].pts[1][0] > 3000.0 && v[1].pts[1][1] == 815.0);
        // Auto: vertical first gives 1985×1500 and 1015×685; horizontal first
        // 1985×815 and 3000×685 — the first pair is squarer.
        let a = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::LAuto), 0.15);
        assert_eq!(a, auto_lines(&parts, &sheet, &spec(), &settings(LineShape::LVerticalFirst), 0.15));
    }

    #[test]
    fn staircase_and_one_step_follow_the_parts() {
        let parts = vec![part(1000.0, 800.0), part(600.0, 500.0)];
        let sheet = vec![at(0, 5.0, 5.0, 1000.0, 800.0), at(1, 5.0, 815.0, 600.0, 500.0)];
        let st = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::Staircase), 0.15);
        assert_eq!(st.len(), 1);
        let xs: Vec<f64> = st[0].pts.iter().map(|p| p[0]).collect();
        assert!(xs.contains(&1015.0) && xs.contains(&615.0), "{xs:?}");
        let os = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::OneStep), 0.15);
        assert_eq!(os[0].pts.len(), 4);
        // The remnant region is bigger than any straight cut's.
        let f = crate::nest::SheetFormat::rect(3000.0, 1500.0);
        let area = |l: &[CutLine]| crate::remnant::sheet_remnants(&parts, &sheet, &f, l, 0.3, 0.0, false).iter().map(|o| o.area_mm2()).sum::<f64>();
        let hz = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::HorizontalStrip), 0.15);
        assert!(area(&os) > area(&hz));
        assert!(area(&st) > 0.0);
    }

    #[test]
    fn irregular_lines_hug_a_round_part() {
        let mut disc = part(600.0, 600.0);
        disc.shape = Shape::Disc;
        let parts = vec![disc];
        let sheet = vec![at(0, 5.0, 5.0, 600.0, 600.0)];
        let v = auto_lines(&parts, &sheet, &spec(), &settings(LineShape::Optimized), 0.15);
        assert!(!v.is_empty());
        let f = crate::nest::SheetFormat::rect(3000.0, 1500.0);
        let rem: f64 = crate::remnant::sheet_remnants(&parts, &sheet, &f, &v, 0.3, 0.0, false).iter().map(|o| o.area_mm2()).sum();
        let strip = 3000.0 * 1500.0 - 615.0 * 1500.0;
        assert!(rem > strip, "following the disc keeps more than a straight strip: {rem} vs {strip}");
    }

    #[test]
    fn semi_automatic_frees_the_clicked_area_only() {
        let mut ring = part(400.0, 400.0);
        ring.shape = Shape::Disc;
        let parts = vec![part(1000.0, 800.0), ring];
        let sheet = vec![at(0, 5.0, 5.0, 1000.0, 800.0), at(1, 1500.0, 500.0, 400.0, 400.0)];
        let s = settings(LineShape::LAuto);
        // Beside the parts, against the sheet edge.
        let v = semi_auto_lines(&parts, &sheet, 3000.0, 1500.0, SemiMode::ExteriorOrthogonal, &s, 0.15, [2800.0, 200.0], 0.1).unwrap();
        assert!(!v.is_empty());
        assert!(v.iter().all(|l| crate::cutoff::path_hits_part(&parts, &sheet, &l.pts).is_none()));
        assert!(v.iter().flat_map(|l| l.pts.windows(2)).all(|w| (w[0][0] - w[1][0]).abs() < 1e-6 || (w[0][1] - w[1][1]).abs() < 1e-6), "only square cuts: {v:?}");
        // On a part: refused.
        assert!(semi_auto_lines(&parts, &sheet, 3000.0, 1500.0, SemiMode::OptimizedIrregular, &s, 0.15, [300.0, 300.0], 0.1).is_err());
        // Inside the ring's hole (200 mm across at least): one closed cut.
        let g = parts[1].geometry();
        let hole_c = crate::nest::centroid(&g.holes[0]);
        let click = sheet[1].map(hole_c);
        let s2 = LineSettings { min_width: 20.0, ..s.clone() };
        let v = semi_auto_lines(&parts, &sheet, 3000.0, 1500.0, SemiMode::OptimizedIrregular, &s2, 0.15, click, 0.0).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pts.first(), v[0].pts.last(), "a closed cut inside the hole");
        // Exterior orthogonal can't go into a hole.
        assert!(semi_auto_lines(&parts, &sheet, 3000.0, 1500.0, SemiMode::ExteriorOrthogonal, &s2, 0.15, click, 0.0).is_err());
    }

    #[test]
    fn bridges_shorten_the_ends_on_the_edge() {
        let pts = vec![[1000.0, -0.02], [1000.0, 1500.02]];
        let b = with_bridges(&pts, 3000.0, 1500.0, 5.0);
        assert!((b[0][1] - 4.98).abs() < 1e-9 && (b[1][1] - 1495.02).abs() < 1e-9, "{b:?}");
        assert_eq!(with_bridges(&pts, 3000.0, 1500.0, 0.0), pts);
    }
}

#[cfg(test)]
mod sample_tests {
    /// The dense sample sheet: the irregular line must stay clear of every
    /// part (round offsets — mitred ones cut corners), keep no slivers narrower
    /// than the minimum width, and keep more than a straight strip does.
    #[test]
    fn irregular_line_on_the_dense_sample_sheet() {
        let a = crate::App::new(None);
        let sheet = &a.result.sheets[0];
        let f = crate::nest::SheetFormat::rect(a.spec.w, a.spec.h);
        let s = super::LineSettings { shape: super::LineShape::Optimized, ..Default::default() };
        let lines = super::auto_lines(&a.parts, sheet, &a.spec, &s, 0.15);
        assert_eq!(lines.len(), 1, "one line, no corner bits");
        assert!(lines.iter().all(|l| crate::cutoff::path_hits_part(&a.parts, sheet, &l.pts).is_none()));
        let area = |l: &[crate::cutoff::CutLine]| crate::remnant::sheet_remnants(&a.parts, sheet, &f, l, 0.3, 0.0, false).iter().map(|o| o.area_mm2()).sum::<f64>();
        let strip = super::auto_lines(&a.parts, sheet, &a.spec, &super::LineSettings { shape: super::LineShape::VerticalStrip, ..s.clone() }, 0.15);
        assert!(area(&lines) > area(&strip), "{} vs {}", area(&lines), area(&strip));
    }
}
