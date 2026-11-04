//! Cut lines on the sheet that don't belong to a part: remnant (cut-off) lines
//! that split the leftover sheet off as a reusable remnant, and free cut paths
//! drawn by hand — the original's Remnants group (see MECHANICS.md
//! "Remnants and free cut lines").

use crate::nest::{self, Part, Placed, SheetSpec, P};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, RichText, Shape as EShape, Stroke, Vec2};

/// Cut width (kerf) used to place the line half a cut clear of the gap.
pub const KERF_MM: f64 = 0.3;
/// A remnant narrower than this (or smaller than this × the sheet side) is scrap.
pub const REMNANT_MIN_MM: f64 = 100.0;
/// Corner radius when "Round the remnant line" is on.
pub const EDGE_ROUND_R: f64 = 10.0;
/// Exact edge: bands shorter than this are merged into a neighbour (no teeth).
pub const EXACT_MIN_STEP: f64 = 50.0;
/// Pick distance for cut lines, and snap distance to the sheet edges (screen px).
pub const LINE_PICK_PX: f32 = 8.0;
pub const EDGE_SNAP_PX: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum CutKind {
    /// Separates the remnant from the used part of the sheet (cut last).
    Remnant,
    /// A cut path drawn by hand, not part of any part.
    Free,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CutLine {
    pub pts: Vec<P>,
    pub kind: CutKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CutTool {
    /// "Manual straight line": two clicks.
    Straight,
    /// "Draw the edge" / free path: clicks, double-click or Enter to finish.
    Path,
    /// "Semi-automatic": click an area to make it a remnant.
    Semi(crate::remline::SemiMode),
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn seg_point(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-18 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Distance from a point to a polyline.
/// The turns Rotate remnants offers (multiples of 90°, or a mirror).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemnantTurn {
    Rotate90,
    Rotate180,
    Rotate270,
    MirrorX,
    MirrorY,
}

impl RemnantTurn {
    pub const ALL: [RemnantTurn; 5] = [RemnantTurn::Rotate90, RemnantTurn::Rotate180, RemnantTurn::Rotate270, RemnantTurn::MirrorX, RemnantTurn::MirrorY];
    pub fn label(self) -> &'static str {
        match self {
            RemnantTurn::Rotate90 => "rotate 90°",
            RemnantTurn::Rotate180 => "rotate 180°",
            RemnantTurn::Rotate270 => "rotate 270°",
            RemnantTurn::MirrorX => "mirror in X",
            RemnantTurn::MirrorY => "mirror in Y",
        }
    }
}

pub fn polyline_dist(p: P, pts: &[P]) -> f64 {
    pts.windows(2).map(|w| seg_point(p, w[0], w[1])).fold(f64::MAX, f64::min)
}

/// Where the automatic remnant line goes in X: past the parts' right-hand extent
/// by the part gap plus half the cut width (RM1: origin + cut width × 0.5 +
/// distance). None when the remnant left
/// would be narrower than the minimum (the original refuses it, code 10).
pub fn remnant_line_x(sheet: &[Placed], spec: &SheetSpec) -> Option<f64> {
    let ext = sheet.iter().map(|p| p.x + p.w).fold(spec.margin, f64::max);
    let x = ext + spec.gap + KERF_MM * 0.5;
    (spec.w - x >= REMNANT_MIN_MM).then_some(x)
}

/// The same across Y (RM1).
pub fn remnant_line_y(sheet: &[Placed], spec: &SheetSpec) -> Option<f64> {
    let ext = sheet.iter().map(|p| p.y + p.h).fold(spec.margin, f64::max);
    let y = ext + spec.gap + KERF_MM * 0.5;
    (spec.h - y >= REMNANT_MIN_MM).then_some(y)
}

/// Automatic edge: a straight line from sheet edge to sheet edge, in whichever
/// direction leaves the larger remnant.
pub fn auto_edge(sheet: &[Placed], spec: &SheetSpec) -> Option<CutLine> {
    let ax = remnant_line_x(sheet, spec).map(|x| ((spec.w - x) * spec.h, CutLine { pts: vec![[x, 0.0], [x, spec.h]], kind: CutKind::Remnant }));
    let ay = remnant_line_y(sheet, spec).map(|y| ((spec.h - y) * spec.w, CutLine { pts: vec![[0.0, y], [spec.w, y]], kind: CutKind::Remnant }));
    match (ax, ay) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a.1 } else { b.1 }),
        (a, b) => a.or(b).map(|v| v.1),
    }
}

/// Rounded corners: each corner of the polyline becomes an arc of radius `r`
/// (or less, where the sides are short) — "Round the remnant line"
/// (RM3: the original puts a fillet between elements that aren't tangent
/// or aligned).
pub fn round_corners(pts: &[P], r: f64) -> Vec<P> {
    if pts.len() < 3 || r <= 0.0 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let (a, v, b) = (pts[i - 1], pts[i], pts[i + 1]);
        let (la, lb) = (dist(a, v), dist(v, b));
        let ua = [(a[0] - v[0]) / la.max(1e-9), (a[1] - v[1]) / la.max(1e-9)];
        let ub = [(b[0] - v[0]) / lb.max(1e-9), (b[1] - v[1]) / lb.max(1e-9)];
        let cos = (ua[0] * ub[0] + ua[1] * ub[1]).clamp(-1.0, 1.0);
        if cos < -0.999 {
            out.push(v); // straight on: nothing to round
            continue;
        }
        let half = cos.acos() / 2.0;
        // Distance from the corner to where the arc touches each side.
        let t = (r / half.tan()).min(la / 2.0).min(lb / 2.0);
        let p0 = [v[0] + ua[0] * t, v[1] + ua[1] * t];
        let p1 = [v[0] + ub[0] * t, v[1] + ub[1] * t];
        // Quadratic curve through the corner as a short arc.
        for k in 0..=6 {
            let s = k as f64 / 6.0;
            let (c0, c1, c2) = ((1.0 - s) * (1.0 - s), 2.0 * (1.0 - s) * s, s * s);
            out.push([c0 * p0[0] + c1 * v[0] + c2 * p1[0], c0 * p0[1] + c1 * v[1] + c2 * p1[1]]);
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// Exact edge ("Draw the cutting line of the remnant between parts"): a stepped
/// line that hugs the parts band by band, so the remnant keeps every free corner.
/// Runs across the sheet in X (remnant on the right).
pub fn exact_edge(sheet: &[Placed], spec: &SheetSpec, round: bool) -> Option<CutLine> {
    if sheet.is_empty() {
        return None;
    }
    let d = spec.gap + KERF_MM * 0.5;
    let mut ys: Vec<f64> = vec![0.0, spec.h];
    for p in sheet {
        ys.push((p.y - d).max(0.0));
        ys.push((p.y + p.h + d).min(spec.h));
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let mut bands: Vec<(f64, f64, f64)> = ys
        .windows(2)
        .map(|w| {
            let (y0, y1) = (w[0], w[1]);
            let x = sheet
                .iter()
                .filter(|p| p.y - d < y1 - 1e-6 && p.y + p.h + d > y0 + 1e-6)
                .map(|p| p.x + p.w + d)
                .fold(spec.margin, f64::max);
            (y0, y1, x)
        })
        .collect();
    // A band too short to give a usable strip of remnant is not worth a step: it
    // takes the larger x of its neighbours (no teeth, no thin slivers along the
    // sheet's edge).
    loop {
        let Some(k) = (0..bands.len()).filter(|&k| bands.len() > 1 && bands[k].1 - bands[k].0 < EXACT_MIN_STEP).min_by(|&a, &b| (bands[a].1 - bands[a].0).total_cmp(&(bands[b].1 - bands[b].0))) else { break };
        let n = if k == 0 {
            1
        } else if k + 1 == bands.len() || bands[k - 1].2 >= bands[k + 1].2 {
            k - 1
        } else {
            k + 1
        };
        let (lo, hi) = (k.min(n), k.max(n));
        bands[lo] = (bands[lo].0, bands[hi].1, bands[lo].2.max(bands[hi].2));
        bands.remove(hi);
    }
    let mut steps: Vec<(f64, f64, f64)> = vec![]; // (y0, y1, x)
    for (y0, y1, x) in bands {
        match steps.last_mut() {
            Some(s) if (s.2 - x).abs() < 1.0 => {
                s.1 = y1;
                s.2 = s.2.max(x);
            }
            _ => steps.push((y0, y1, x)),
        }
    }
    // The staircase: vertical runs, joined by horizontal steps between bands.
    let mut stair = vec![];
    for &(y0, y1, x) in &steps {
        stair.push([x, y0]);
        stair.push([x, y1]);
    }
    let area = remnant_area_of(&stair, spec);
    if area < REMNANT_MIN_MM * spec.h {
        return None;
    }
    let stair = if round { round_corners(&stair, EDGE_ROUND_R) } else { stair };
    Some(CutLine { pts: stair, kind: CutKind::Remnant })
}

/// The remnant a line cuts off, as a polygon: a line from the bottom edge to the
/// top edge leaves the area on its right; from the left edge to the right edge,
/// the area above. None for a line that doesn't cross the sheet.
pub fn remnant_region(line: &[P], spec: &SheetSpec) -> Option<Vec<P>> {
    let (a, b) = (*line.first()?, *line.last()?);
    let near = |v: f64, t: f64| (v - t).abs() < 1.0;
    let mut pts = line.to_vec();
    if near(a[1].min(b[1]), 0.0) && near(a[1].max(b[1]), spec.h) {
        if a[1] > b[1] {
            pts.reverse();
        }
        pts.extend([[spec.w, spec.h], [spec.w, 0.0]]);
        Some(pts)
    } else if near(a[0].min(b[0]), 0.0) && near(a[0].max(b[0]), spec.w) {
        if a[0] > b[0] {
            pts.reverse();
        }
        pts.extend([[spec.w, spec.h], [0.0, spec.h]]);
        Some(pts)
    } else {
        None
    }
}

/// Centre of area of a polygon (where the remnant's label goes).
pub fn area_centre(poly: &[P]) -> P {
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = p[0] * q[1] - q[0] * p[1];
        a += c;
        cx += (p[0] + q[0]) * c;
        cy += (p[1] + q[1]) * c;
    }
    if a.abs() < 1e-9 { nest::centroid(poly) } else { [cx / (3.0 * a), cy / (3.0 * a)] }
}

fn remnant_area_of(line: &[P], spec: &SheetSpec) -> f64 {
    remnant_region(line, spec).map_or(0.0, |r| nest::area(&r))
}

/// Does the path cut into a part? It must stay at least half a cut away from
/// every part's outline (RM4: the original checks contours against the
/// remnant line). Returns the part index hit.
pub fn path_hits_part(parts: &[Part], sheet: &[Placed], pts: &[P]) -> Option<usize> {
    let clear = KERF_MM * 0.5;
    for (i, pl) in sheet.iter().enumerate() {
        let (lo, hi) = (pts.iter().fold([f64::MAX; 2], |m, p| [m[0].min(p[0]), m[1].min(p[1])]), pts.iter().fold([f64::MIN; 2], |m, p| [m[0].max(p[0]), m[1].max(p[1])]));
        if hi[0] < pl.x - clear || lo[0] > pl.x + pl.w + clear || hi[1] < pl.y - clear || lo[1] > pl.y + pl.h + clear {
            continue;
        }
        let (outline, _) = nest::placed_shape(parts, pl);
        if pts.iter().any(|&p| nest::point_in_poly(&outline, p)) {
            return Some(i);
        }
        let n = outline.len();
        for w in pts.windows(2) {
            for k in 0..n {
                if nest::seg_seg_dist(w[0], w[1], outline[k], outline[(k + 1) % n]) < clear {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// G-code blocks for the sheet's cut lines: free paths first, the remnant line
/// last so the remnant only comes loose at the end.
/// Remnant lines stop `bridge` short of the sheet edge (Offset in the sheet
/// edge, RE16) so the remnant stays held.
pub fn gcode_lines(lines: &[CutLine], spec: &SheetSpec, bridge: f64) -> Vec<(String, Vec<P>)> {
    let mut v: Vec<(String, Vec<P>)> = lines.iter().filter(|l| l.kind == CutKind::Free).map(|l| ("CUT LINE".to_string(), l.pts.clone())).collect();
    v.extend(lines.iter().filter(|l| l.kind == CutKind::Remnant).map(|l| ("REMNANT LINE".to_string(), crate::remline::with_bridges(&l.pts, spec.w, spec.h, bridge))));
    v
}

impl App {
    pub fn cut_list(&self) -> &[CutLine] {
        self.cut_lines.get(self.cur_sheet).map_or(&[][..], |v| v.as_slice())
    }

    fn cut_list_mut(&mut self) -> &mut Vec<CutLine> {
        let s = self.cur_sheet;
        if self.cut_lines.len() <= s {
            self.cut_lines.resize(s + 1, vec![]);
        }
        &mut self.cut_lines[s]
    }

    fn sheet_now(&self) -> &[Placed] {
        self.result.sheets.get(self.cur_sheet).map_or(&[][..], |v| v.as_slice())
    }

    /// Add a line; a new remnant line replaces the old one (one remnant per sheet).
    pub fn add_cut_line(&mut self, line: CutLine) -> bool {
        let kind = line.kind;
        let (parts, sheet) = (self.parts.clone(), self.sheet_now().to_vec());
        if let Err(e) = crate::engine::add_cut_line(&parts, &sheet, self.cut_list_mut(), line) {
            self.status = e;
            return false;
        }
        self.sel_cut = Some(self.cut_list().len() - 1);
        self.refresh();
        self.status = match (kind, self.remnant_info()) {
            (CutKind::Remnant, Some((w, h, a))) => format!("Remnant line added — remnant about {w:.0} × {h:.0} mm ({:.2} m²).", a / 1e6),
            _ => "Cut line added — it is cut after the parts.".into(),
        };
        true
    }

    /// Automatic edge: the remnant line in the shape chosen in the Sheet
    /// Remnants options. Like the original, it isn't made when the sheet already
    /// has one (RM2, RE2) — delete that edge first.
    pub fn auto_remnant_edge(&mut self) {
        if self.cut_list().iter().any(|l| l.kind == CutKind::Remnant) {
            self.status = "This sheet already has a remnant line — delete the edge first.".into();
            return;
        }
        let ls = self.cfg.stock.line.clone();
        let lines = crate::remline::auto_lines(&self.parts, self.sheet_now(), &self.cur_spec(), &ls, KERF_MM / 2.0);
        if lines.is_empty() {
            self.status = format!("No remnant: less than the minimum width ({:.0} mm) is left past the parts.", ls.min_width);
            return;
        }
        self.set_remnant_lines(lines, ls.shape.label());
    }

    /// Replace the sheet's remnant lines with these (they're refused if one
    /// cuts through a part).
    pub fn set_remnant_lines(&mut self, lines: Vec<CutLine>, what: &str) -> bool {
        for l in &lines {
            if let Some(i) = path_hits_part(&self.parts, self.sheet_now(), &l.pts) {
                self.status = format!("The {what} line would cut through part #{} — try another shape or a larger offset.", i + 1);
                return false;
            }
        }
        let list = self.cut_list_mut();
        list.retain(|l| l.kind != CutKind::Remnant);
        list.extend(lines);
        self.sel_cut = None;
        self.refresh();
        self.status = match self.remnant_info() {
            Some((w, h, a)) => format!("{what}: remnant about {w:.0} × {h:.0} mm ({:.2} m²).", a / 1e6),
            None => format!("{what}: remnant line added."),
        };
        true
    }

    /// Semi-automatic: the area clicked becomes a remnant (its cuts are added).
    pub fn semi_auto_at(&mut self, mode: crate::remline::SemiMode, w: P) {
        let spec = self.cur_spec();
        let ls = self.cfg.stock.line.clone();
        match crate::remline::semi_auto_lines(&self.parts, self.sheet_now(), spec.w, spec.h, mode, &ls, KERF_MM / 2.0, w, self.cfg.stock.min_remnant_m2) {
            Ok(lines) => {
                if lines.iter().any(|l| path_hits_part(&self.parts, self.sheet_now(), &l.pts).is_some()) {
                    self.status = "That cut would touch a part — try a larger offset.".into();
                    return;
                }
                self.checkpoint();
                self.cut_list_mut().extend(lines);
                self.refresh();
                self.status = format!("{}: remnant added — {} remnant(s) on this sheet.", mode.label(), self.remnant_pieces().len());
            }
            Err(e) => self.status = e,
        }
    }

    /// Rotate or mirror a remnant sheet (Rotate remnants): only on a sheet that
    /// is a remnant and has nothing on it yet (ST10, RE27).
    pub fn transform_remnant(&mut self, op: RemnantTurn) {
        let i = self.cur_sheet;
        let f = self.result.format(i, &self.spec);
        if !f.remnant || f.outline.is_none() {
            self.status = "Rotate remnants works on a sheet that is a remnant.".into();
            return;
        }
        if self.result.sheets.get(i).is_some_and(|s| !s.is_empty()) || !self.cut_list().is_empty() {
            self.status = "Take the parts and lines off the remnant first — it can only be turned while empty.".into();
            return;
        }
        let (w, h) = (f.w, f.h);
        let map = |p: P| -> P {
            match op {
                RemnantTurn::Rotate90 => [h - p[1], p[0]],
                RemnantTurn::Rotate180 => [w - p[0], h - p[1]],
                RemnantTurn::Rotate270 => [p[1], w - p[0]],
                RemnantTurn::MirrorX => [w - p[0], p[1]],
                RemnantTurn::MirrorY => [p[0], h - p[1]],
            }
        };
        let mut nf = f.clone();
        if let Some(o) = nf.outline.as_mut() {
            for p in o.outer.iter_mut().chain(o.holes.iter_mut().flatten()) {
                *p = map(*p);
            }
            let mirrored = matches!(op, RemnantTurn::MirrorX | RemnantTurn::MirrorY);
            if mirrored {
                o.outer.reverse();
                for hh in o.holes.iter_mut() {
                    hh.reverse();
                }
            }
        }
        if matches!(op, RemnantTurn::Rotate90 | RemnantTurn::Rotate270) {
            nf.w = h;
            nf.h = w;
        }
        self.checkpoint();
        self.result.normalise(&self.spec);
        if self.result.formats.len() > i {
            self.result.formats[i] = nf;
        }
        self.refresh();
        self.status = format!("Remnant {} turned ({}).", f.stock_ref, op.label());
    }

    /// Put the automatic remnant line on every sheet that has none (Generate
    /// remnants after nesting).
    pub fn auto_lines_all_sheets(&mut self) {
        let keep = self.cur_sheet;
        let ls = self.cfg.stock.line.clone();
        for i in 0..self.result.sheets.len() {
            self.cur_sheet = i;
            if self.cut_list().iter().any(|l| l.kind == CutKind::Remnant) {
                continue;
            }
            let lines = crate::remline::auto_lines(&self.parts, self.sheet_now(), &self.cur_spec(), &ls, KERF_MM / 2.0);
            if !lines.is_empty() && lines.iter().all(|l| path_hits_part(&self.parts, self.sheet_now(), &l.pts).is_none()) {
                self.cut_list_mut().extend(lines);
            }
        }
        self.cur_sheet = keep;
    }

    pub fn exact_remnant_edge(&mut self) {
        match exact_edge(self.sheet_now(), &self.cur_spec(), self.round_edge) {
            Some(l) => {
                self.add_cut_line(l);
            }
            None => self.status = "No remnant worth keeping between the parts and the sheet edge.".into(),
        }
    }

    pub fn delete_cut_line(&mut self) {
        let Some(i) = self.sel_cut.take() else { return };
        let list = self.cut_list_mut();
        if i < list.len() {
            list.remove(i);
            self.refresh();
            self.status = "Cut line deleted.".into();
        }
    }

    /// The remnants the current sheet's remnant lines cut off (sheet coordinates).
    pub fn remnant_pieces(&self) -> Vec<crate::stock::Outline> {
        if !self.cut_list().iter().any(|l| l.kind == CutKind::Remnant) {
            return vec![];
        }
        let f = self.result.format(self.cur_sheet, &self.spec);
        crate::remnant::sheet_remnants_cached(&self.parts, self.sheet_now(), &f, self.cut_list(), KERF_MM)
    }

    /// (width, height, area) of the largest remnant on the current sheet.
    pub fn remnant_info(&self) -> Option<(f64, f64, f64)> {
        let r = self.remnant_pieces().into_iter().next()?;
        let (w, h) = r.size();
        Some((w, h, r.area_mm2()))
    }

    /// Start moving a straight remnant/cut line ("Modify Edge") if `w` is on one.
    pub fn cut_drag_begin(&mut self, w: P, tol: f64) -> bool {
        let Some(i) = self.cut_list().iter().position(|l| l.pts.len() == 2 && polyline_dist(w, &l.pts) <= tol) else { return false };
        self.sel_cut = Some(i);
        self.sel_placed = None;
        self.cut_drag = Some((i, w, self.cut_list()[i].pts.clone()));
        true
    }

    /// Move it across itself only (a vertical line moves in X, a horizontal one in Y).
    pub fn cut_drag_move(&mut self, w: P) {
        let Some((i, grab, orig)) = self.cut_drag.clone() else { return };
        let vertical = (orig[0][0] - orig[1][0]).abs() < (orig[0][1] - orig[1][1]).abs();
        let d = if vertical { [w[0] - grab[0], 0.0] } else { [0.0, w[1] - grab[1]] };
        if let Some(l) = self.cut_lines.get_mut(self.cur_sheet).and_then(|v| v.get_mut(i)) {
            l.pts = orig.iter().map(|p| [p[0] + d[0], p[1] + d[1]]).collect();
        }
    }

    pub fn cut_drag_end(&mut self) {
        let Some((i, _, orig)) = self.cut_drag.take() else { return };
        let pts = self.cut_list().get(i).map(|l| l.pts.clone()).unwrap_or_default();
        if let Some(k) = path_hits_part(&self.parts, self.sheet_now(), &pts) {
            if let Some(l) = self.cut_lines.get_mut(self.cur_sheet).and_then(|v| v.get_mut(i)) {
                l.pts = orig;
            }
            self.status = format!("The line would cut through part #{} — moved back.", k + 1);
        } else {
            self.status = "Cut line moved.".into();
        }
        self.refresh();
    }

    fn finish_path(&mut self) {
        let pts = std::mem::take(&mut self.cut_draft);
        if pts.len() >= 2 {
            // A line from one sheet edge to the opposite one separates a remnant.
            let line = crate::engine::classify_path(pts, &self.cur_spec());
            self.add_cut_line(line);
        }
    }

    /// Snap a point to the sheet edges, and square it to the previous point.
    fn cut_point(&self, w: P, snap: f64, ortho: bool) -> P {
        let mut p = w;
        if let (true, Some(prev)) = (ortho, self.cut_draft.last()) {
            if (p[0] - prev[0]).abs() < (p[1] - prev[1]).abs() {
                p[0] = prev[0];
            } else {
                p[1] = prev[1];
            }
        }
        let cs = self.cur_spec();
        for (k, edge) in [(0, 0.0), (0, cs.w), (1, 0.0), (1, cs.h)] {
            if (p[k] - edge).abs() <= snap {
                p[k] = edge;
            }
        }
        let cs = self.cur_spec();
        [p[0].clamp(0.0, cs.w), p[1].clamp(0.0, cs.h)]
    }

    /// Ribbon group (Sheet tab): the remnant and cut-line tools.
    pub fn ribbon_cutoff(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        ribbon_group(ui, "Remnants & cut lines", |ui| {
            if ribbon_button(ui, Icon::Compact, "Automatic\nedge", false, has)
                .on_hover_text(format!("The remnant line in the chosen shape ({}), {:.0} mm past the parts", self.cfg.stock.line.shape.label(), self.cfg.stock.line.offset))
                .clicked()
            {
                self.auto_remnant_edge();
            }
            ui.vertical(|ui| {
                ui.add_space(6.0);
                let mut shape = self.cfg.stock.line.shape;
                egui::ComboBox::from_id_salt("remline_shape").width(150.0).selected_text(shape.label()).show_ui(ui, |ui| {
                    for s in crate::remline::LineShape::ALL {
                        ui.selectable_value(&mut shape, s, s.label());
                    }
                });
                if shape != self.cfg.stock.line.shape {
                    self.cfg.stock.line.shape = shape;
                    let _ = self.cfg.save();
                }
                if ui.small_button("Remnant options…").on_hover_text("Sheet Remnants: offset, minimum width, Min X/Y, rounding, bridges at the edge, minimum area").clicked() {
                    self.stockui.open = true;
                    self.stockui.show_options = true;
                    self.stockui.loaded = false;
                }
            });
            if ribbon_button(ui, Icon::Fit, "Exact\nedge", false, has)
                .on_hover_text("A stepped remnant line that hugs the parts, keeping every free corner of the sheet")
                .clicked()
            {
                self.exact_remnant_edge();
            }
            let straight = self.cut_tool == Some(CutTool::Straight);
            if ribbon_button(ui, Icon::CutPath, "Straight\nline", straight, true)
                .on_hover_text("Click two points. Edge to edge it cuts a remnant off; ends snap to the sheet edges. Shift = square.")
                .clicked()
            {
                self.cut_tool = if straight { None } else { Some(CutTool::Straight) };
                self.cut_draft.clear();
            }
            let path = self.cut_tool == Some(CutTool::Path);
            if ribbon_button(ui, Icon::Joint, "Draw\npath", path, true)
                .on_hover_text("Draw your own cut path on the sheet, no part needed: click points, double-click or Enter to finish, Backspace undoes a point")
                .clicked()
            {
                self.cut_tool = if path { None } else { Some(CutTool::Path) };
                self.cut_draft.clear();
            }
            let semi = matches!(self.cut_tool, Some(CutTool::Semi(_)));
            let r = ribbon_button(ui, Icon::Fit, "Semi-\nautomatic", semi, has).on_hover_text("Click an area — beside the parts, in a gap or in an empty hole — to make it a remnant");
            if r.clicked() {
                self.cut_tool = if semi { None } else { Some(CutTool::Semi(self.semi_mode)) };
                self.cut_draft.clear();
            }
            r.context_menu(|ui| {
                for m in crate::remline::SemiMode::ALL {
                    if ui.selectable_label(self.semi_mode == m, m.label()).clicked() {
                        self.semi_mode = m;
                        if semi {
                            self.cut_tool = Some(CutTool::Semi(m));
                        }
                        ui.close();
                    }
                }
            });
            if ribbon_button(ui, Icon::Remove, "Delete\nline", false, self.sel_cut.is_some()).clicked() {
                self.delete_cut_line();
            }
            let f = self.result.format(self.cur_sheet, &self.spec);
            let can_turn = f.remnant && f.outline.is_some() && self.result.sheets.get(self.cur_sheet).is_none_or(|s| s.is_empty());
            let r = ribbon_button(ui, Icon::Rotate, "Rotate\nremnant", false, can_turn).on_hover_text("Turn or mirror this remnant sheet (only while it's empty)");
            r.context_menu(|ui| {
                for t in RemnantTurn::ALL {
                    if ui.button(t.label()).clicked() {
                        self.transform_remnant(t);
                        ui.close();
                    }
                }
            });
            if r.clicked() {
                self.transform_remnant(RemnantTurn::Rotate90);
            }
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.checkbox(&mut self.round_edge, "Round corners").on_hover_text("Round the corners of the exact edge");
                let txt = match self.remnant_info() {
                    Some((w, h, _)) => format!("Remnant {w:.0} × {h:.0}"),
                    None => format!("{} line(s)", self.cut_list().len()),
                };
                ui.label(RichText::new(txt).size(11.0).color(th().text_dim));
            });
        });
        if self.cut_tool.is_some() {
            self.manual_mode = false;
            self.ghost = None;
        }
    }

    /// Draw the sheet's cut lines and remnant, and run the drawing tools.
    pub fn cut_canvas(&mut self, ui: &egui::Ui, painter: &egui::Painter, resp: &egui::Response, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, s: f32) {
        let ctx = ui.ctx().clone();
        let tol = (LINE_PICK_PX / s.max(1e-6)) as f64;
        let snap = (EDGE_SNAP_PX / s.max(1e-6)) as f64;
        let keys = !ctx.egui_wants_keyboard_input();
        // --- the remnant, then the lines ---
        let hover_w = resp.hover_pos().map(to_w);
        for piece in self.remnant_pieces() {
            {
                let r = piece.outer.clone();
                let pts: Vec<Pos2> = r.iter().map(|&p| to_s(p)).collect();
                // Hatch: diagonal stripes inside the remnant's box, kept to the region.
                let bb = Rect::from_points(&pts);
                let step = 14.0;
                let mut k = bb.left() - bb.height();
                while k < bb.right() {
                    let (a, b) = (Pos2::new(k, bb.bottom()), Pos2::new(k + bb.height(), bb.top()));
                    let (wa, wb) = (to_w(a), to_w(b));
                    // Keep the stripe pieces that are inside the region.
                    let n = 24;
                    let mut run: Option<Pos2> = None;
                    for j in 0..=n {
                        let t = j as f32 / n as f32;
                        let q = a + (b - a) * t;
                        let wq = [wa[0] + (wb[0] - wa[0]) * t as f64, wa[1] + (wb[1] - wa[1]) * t as f64];
                        let inside = nest::point_in_poly(&r, wq) && !piece.holes.iter().any(|hh| nest::point_in_poly(hh, wq));
                        match (inside, run) {
                            (true, None) => run = Some(q),
                            (false, Some(st)) => {
                                painter.line_segment([st, q], Stroke::new(1.0, Color32::from_rgba_unmultiplied(250, 204, 21, 60)));
                                run = None;
                            }
                            _ => {}
                        }
                    }
                    if let Some(st) = run {
                        painter.line_segment([st, b], Stroke::new(1.0, Color32::from_rgba_unmultiplied(250, 204, 21, 60)));
                    }
                    k += step;
                }
                let c = area_centre(&r);
                let (w, h) = piece.size();
                let a = piece.area_mm2();
                let txt = format!("Remnant  {w:.0} × {h:.0} mm\n{:.2} m²", a / 1e6);
                let g = painter.layout(txt, FontId::proportional(12.0), Color32::WHITE, 200.0);
                let rr = Rect::from_center_size(to_s(c), g.size() + Vec2::new(16.0, 10.0));
                painter.rect_filled(rr, CornerRadius::same(6), Color32::from_rgba_unmultiplied(120, 90, 10, 210));
                painter.galley(rr.min + Vec2::new(8.0, 5.0), g, Color32::WHITE);
            }
        }
        // The automatic marks on the remnants (engraved before the remnant line).
        for st in self.remnant_marks(self.cur_sheet) {
            let pts: Vec<Pos2> = st.iter().map(|&p| to_s(p)).collect();
            painter.add(EShape::line(pts, Stroke::new(1.2, Color32::from_rgb(255, 240, 200))));
        }
        for (i, l) in self.cut_list().iter().enumerate() {
            let hovered = self.cut_tool.is_none() && hover_w.is_some_and(|w| polyline_dist(w, &l.pts) <= tol);
            let sel = self.sel_cut == Some(i);
            let col = match l.kind {
                CutKind::Remnant => Color32::from_rgb(250, 204, 21),
                CutKind::Free => Color32::from_rgb(56, 220, 230),
            };
            let pts: Vec<Pos2> = l.pts.iter().map(|&p| to_s(p)).collect();
            if sel || hovered {
                painter.add(EShape::line(pts.clone(), Stroke::new(7.0, with_alpha(Color32::WHITE, if sel { 90 } else { 50 }))));
            }
            painter.add(EShape::line(pts.clone(), Stroke::new(2.4, col)));
            if let Some(&p0) = pts.first() {
                painter.circle_filled(p0, 3.5, col);
            }
        }
        // --- drawing a new line ---
        if let Some(tool) = self.cut_tool {
            if keys && ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.cut_tool = None;
                self.cut_draft.clear();
                self.status = "Stopped drawing cut lines.".into();
                return;
            }
            if keys && ctx.input(|i| i.key_pressed(Key::Backspace)) {
                self.cut_draft.pop();
            }
            let shift = ctx.input(|i| i.modifiers.shift);
            let cur = resp.hover_pos().map(|hp| self.cut_point(to_w(hp), snap, shift));
            if let CutTool::Semi(mode) = tool {
                if resp.clicked() {
                    if let Some(w) = resp.interact_pointer_pos().map(to_w) {
                        self.semi_auto_at(mode, w);
                    }
                }
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                return;
            }
            if resp.double_clicked() && tool == CutTool::Path {
                self.finish_path();
                return;
            }
            if resp.clicked() {
                if let Some(p) = cur {
                    if self.cut_draft.last().is_none_or(|q| dist(*q, p) > 0.5) {
                        self.cut_draft.push(p);
                    }
                    if tool == CutTool::Straight && self.cut_draft.len() == 2 {
                        self.finish_path();
                        return;
                    }
                }
            }
            if keys && ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.finish_path();
                return;
            }
            // Preview: the points so far plus the pointer; red when it cuts a part.
            let mut prev = self.cut_draft.clone();
            if let Some(p) = cur {
                prev.push(p);
                painter.circle_stroke(to_s(p), 5.0, Stroke::new(1.5, Color32::WHITE));
            }
            if prev.len() >= 2 {
                let bad = path_hits_part(&self.parts, self.sheet_now(), &prev).is_some();
                let col = if bad { Color32::from_rgb(248, 80, 90) } else { Color32::from_rgb(56, 220, 230) };
                painter.extend(EShape::dashed_line(&prev.iter().map(|&p| to_s(p)).collect::<Vec<_>>(), Stroke::new(2.0, col), 8.0, 5.0));
                let edge_to_edge = remnant_region(&prev, &self.cur_spec()).is_some();
                if let Some(&last) = prev.last() {
                    let len: f64 = prev.windows(2).map(|w| dist(w[0], w[1])).sum();
                    let tip = format!("{len:.0} mm{}{}", if edge_to_edge { " · cuts a remnant off" } else { "" }, if bad { " · crosses a part" } else { "" });
                    painter.text(to_s(last) + Vec2::new(12.0, -12.0), Align2::LEFT_BOTTOM, tip, FontId::proportional(12.0), col);
                }
            }
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            return;
        }
        // --- picking and deleting lines ---
        if resp.clicked() {
            if let Some(w) = resp.interact_pointer_pos().map(to_w) {
                if let Some(i) = self.cut_list().iter().position(|l| polyline_dist(w, &l.pts) <= tol) {
                    self.sel_cut = Some(i);
                    self.sel_placed = None;
                    self.status = "Cut line selected — Delete removes it; drag a straight one to move it.".into();
                } else {
                    self.sel_cut = None;
                }
            }
        }
        if keys && self.sel_cut.is_some() && ctx.input(|i| i.key_pressed(Key::Delete)) {
            self.delete_cut_line();
        }
    }
}

#[cfg(test)]
mod cutoff_tests {
    use super::*;
    use crate::nest::Shape;

    fn spec() -> SheetSpec {
        SheetSpec { w: 2000.0, h: 1000.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() }
    }
    fn rect_part() -> Part {
        Part { name: "R".into(), shape: Shape::Rectangle, w: 500.0, h: 300.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn at(x: f64, y: f64) -> Placed {
        Placed { part: 0, x, y, w: 500.0, h: 300.0, turns: 0, mirror: false }
    }

    #[test]
    fn automatic_line_sits_a_gap_and_half_a_cut_past_the_parts() {
        let sheet = [at(10.0, 10.0), at(10.0, 320.0)];
        let x = remnant_line_x(&sheet, &spec()).unwrap();
        assert!((x - (510.0 + 5.0 + KERF_MM * 0.5)).abs() < 1e-9);
        // Wider remnant on the right than on top: the X line wins.
        let l = auto_edge(&sheet, &spec()).unwrap();
        assert_eq!(l.pts[0][0], x);
        assert_eq!(l.kind, CutKind::Remnant);
        // Parts reaching the far side: no remnant (narrower than the minimum).
        let full = [Placed { x: 1450.0, ..at(0.0, 0.0) }];
        assert!(remnant_line_x(&full, &spec()).is_none());
    }

    #[test]
    fn exact_edge_steps_around_the_parts_and_keeps_more() {
        // A tall stack on the left and one long part at the bottom.
        let sheet = [at(10.0, 10.0), Placed { w: 1200.0, ..at(10.0, 320.0) }];
        let s = spec();
        let e = exact_edge(&sheet, &s, false).unwrap();
        let straight = auto_edge(&sheet, &s).unwrap();
        let a_exact = nest::area(&remnant_region(&e.pts, &s).unwrap());
        let a_straight = nest::area(&remnant_region(&straight.pts, &s).unwrap());
        assert!(a_exact > a_straight + 1000.0, "{a_exact} vs {a_straight}");
        let parts = [rect_part(), Part { w: 1200.0, ..rect_part() }];
        let sheet2 = [at(10.0, 10.0), Placed { part: 1, w: 1200.0, ..at(10.0, 320.0) }];
        assert_eq!(path_hits_part(&parts, &sheet2, &exact_edge(&sheet2, &s, true).unwrap().pts), None);
    }

    #[test]
    fn a_line_through_a_part_is_refused() {
        let parts = [rect_part()];
        let sheet = [at(100.0, 100.0)];
        assert_eq!(path_hits_part(&parts, &sheet, &[[300.0, 0.0], [300.0, 1000.0]]), Some(0));
        assert_eq!(path_hits_part(&parts, &sheet, &[[700.0, 0.0], [700.0, 1000.0]]), None);
    }

    #[test]
    fn remnant_lines_are_cut_last_in_the_program() {
        let lines = vec![CutLine { pts: vec![[600.0, 0.0], [600.0, 1000.0]], kind: CutKind::Remnant }, CutLine { pts: vec![[50.0, 900.0], [300.0, 900.0]], kind: CutKind::Free }];
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
        let g = gcode_lines(&lines, &spec, 0.0);
        assert_eq!(g[0].0, "CUT LINE");
        assert_eq!(g[1].0, "REMNANT LINE");
        let r = round_corners(&[[0.0, 0.0], [0.0, 100.0], [100.0, 100.0]], 10.0);
        assert!(r.len() > 3 && !r.contains(&[0.0, 100.0]));
    }
}
