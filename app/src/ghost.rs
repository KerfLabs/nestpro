//! Dropping parts by hand: pick a part in the Parts View and a see-through "ghost"
//! of it follows the pointer over the sheet until you click to drop it — the
//! original's manual nesting, where a copy of the part's outline follows the
//! pointer until the drop (G1–G5), and its approach snap (G6). See MECHANICS.md
//! "Dropping parts by hand (ghost)".

use crate::nest::{self, Part, Placed, SheetSpec, P};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Key, Pos2, Rect, Shape as EShape, Stroke, Vec2};

/// Ghost fill opacity (0-255).
pub const GHOST_ALPHA: u8 = 120;
/// Approach-snap reach, in screen pixels.
pub const SNAP_PX: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub part: usize,
    pub turns: u8,
    pub mirror: bool,
    /// Started by dragging a tile: it drops where the button is let go.
    pub dragging: bool,
    /// Last pointer position on the sheet (mm).
    pub at: Option<P>,
}

/// Footprint (w, h) of a part turned `turns` × 90°.
pub fn footprint(part: &Part, turns: u8) -> (f64, f64) {
    let (w, h) = part.bbox();
    if turns % 2 == 1 { (h, w) } else { (w, h) }
}

/// First orientation the part may take: plain turns first, then mirrored
/// (whether the dragged part may turn is limited as in the original, G4).
pub fn first_orientation(part: &Part) -> (u8, bool) {
    (0..8u8).map(|k| (k % 4, k >= 4)).find(|&(t, m)| nest::turn_allowed(part, t, m)).unwrap_or((0, false))
}

/// Next allowed quarter turn after `turns` with the same mirroring
/// (the original rounds the angle to the allowed step, G3).
pub fn next_turn(part: &Part, turns: u8, mirror: bool) -> Option<u8> {
    (1..4u8).map(|k| (turns + k) % 4).find(|&t| nest::turn_allowed(part, t, mirror))
}

/// Keep the ghost inside the sheet's usable area. The original clamps the dragged
/// point to its minimum/maximum x and y instead of refusing it
/// (G2).
pub fn clamp_to_limits(c: &mut Placed, spec: &SheetSpec) {
    let (x1, y1) = (spec.w - spec.margin - c.w, spec.h - spec.margin - c.h);
    c.x = c.x.min(x1).max(spec.margin);
    c.y = c.y.min(y1).max(spec.margin);
}

fn consider(best: &mut Option<f64>, d: f64, reach: f64) {
    if d.abs() <= reach && best.is_none_or(|b| d.abs() < b.abs()) {
        *best = Some(d);
    }
}

/// Approach snap for a round part against round neighbours: the original projects
/// onto arcs and circles along the radius (G6), so a ring slides in until it sits
/// one gap from a neighbouring ring's edge — or, near two of them, into the
/// pocket between them (touching both, the tight staggered spot). `circle` is the
/// ghost's circle (part coordinates). Returns the moved box.
pub fn circle_snap(c: Placed, circle: (P, f64), others: &[(P, f64)], gap: f64, reach: f64, ok: impl Fn(&Placed) -> bool) -> Option<Placed> {
    let (lc, r) = circle;
    let at = c.map(lc);
    let d = |a: P, b: P| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let moved = |p: P| Placed { x: c.x + p[0] - at[0], y: c.y + p[1] - at[1], ..c };
    // Neighbours whose edge is within reach, nearest first.
    let mut near: Vec<(f64, P, f64)> = others
        .iter()
        .map(|&(oc, or)| ((d(at, oc) - (r + or + gap)).abs(), oc, or))
        .filter(|n| n.0 <= reach)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(4);
    // Pocket between two neighbours.
    let mut best: Option<(f64, Placed)> = None;
    for i in 0..near.len() {
        for j in i + 1..near.len() {
            for p in nest::pocket_points(near[i].1, near[i].2, near[j].1, near[j].2, r, gap) {
                let m = d(p, at);
                let cand = moved(p);
                if m <= 2.0 * reach && best.as_ref().is_none_or(|b| m < b.0) && ok(&cand) {
                    best = Some((m, cand));
                }
            }
        }
    }
    if let Some((_, b)) = best {
        return Some(b);
    }
    // Against one neighbour, along the line between the centres.
    for &(_, oc, or) in &near {
        let k = d(at, oc);
        if k < 1e-9 {
            continue;
        }
        let t = (r + or + gap) / k;
        let cand = moved([oc[0] + (at[0] - oc[0]) * t, oc[1] + (at[1] - oc[1]) * t]);
        if ok(&cand) {
            return Some(cand);
        }
    }
    None
}

/// Approach snap ("adosar por aproximación", G6): when the ghost comes within `reach` of a
/// neighbour's side or the sheet margin, it moves to sit exactly the part gap away
/// from it. Each axis snaps to its nearest target; if both together would clash,
/// one axis alone is tried. Returns the box and whether it moved.
pub fn approach_snap(c: Placed, others: &[Placed], spec: &SheetSpec, reach: f64, ok: impl Fn(&Placed) -> bool) -> (Placed, bool) {
    let gap = spec.gap;
    let (mut bx, mut by) = (None, None);
    consider(&mut bx, spec.margin - c.x, reach);
    consider(&mut bx, spec.w - spec.margin - (c.x + c.w), reach);
    consider(&mut by, spec.margin - c.y, reach);
    consider(&mut by, spec.h - spec.margin - (c.y + c.h), reach);
    for o in others {
        // Side by side: the boxes share some height (or width).
        if c.y < o.y + o.h && o.y < c.y + c.h {
            consider(&mut bx, o.x + o.w + gap - c.x, reach);
            consider(&mut bx, o.x - gap - (c.x + c.w), reach);
        }
        if c.x < o.x + o.w && o.x < c.x + c.w {
            consider(&mut by, o.y + o.h + gap - c.y, reach);
            consider(&mut by, o.y - gap - (c.y + c.h), reach);
        }
    }
    let shift = |dx: Option<f64>, dy: Option<f64>| Placed { x: c.x + dx.unwrap_or(0.0), y: c.y + dy.unwrap_or(0.0), ..c };
    for (dx, dy) in [(bx, by), (bx, None), (None, by)] {
        if dx.is_none() && dy.is_none() {
            continue;
        }
        let s = shift(dx, dy);
        if ok(&s) {
            return (s, true);
        }
    }
    (c, false)
}

impl App {
    /// Pick a part to drop: its ghost follows the pointer over the sheet.
    pub fn start_ghost(&mut self, part: usize, dragging: bool) {
        let (turns, mirror) = first_orientation(&self.parts[part]);
        self.ghost = Some(Ghost { part, turns, mirror, dragging, at: None });
        self.manual_mode = false;
        self.sel_part = Some(part);
        self.sel_placed = None;
        let left = self.parts[part].qty.saturating_sub(self.placed_of(part) as u32);
        self.status = format!(
            "Dropping '{}' ({} left) — click the sheet · R turn · M mirror · Enter auto-place · Esc stop.",
            self.parts[part].name, left
        );
    }

    pub fn stop_ghost(&mut self) {
        if self.ghost.take().is_some() {
            self.status = "Stopped dropping parts.".into();
        }
    }

    pub fn ghost_rotate(&mut self) {
        let Some(g) = self.ghost.as_mut() else { return };
        let part = &self.parts[g.part];
        match next_turn(part, g.turns, g.mirror) {
            Some(t) => g.turns = t,
            None => self.status = format!("'{}' may not be turned.", part.name),
        }
    }

    pub fn ghost_mirror(&mut self) {
        let Some(g) = self.ghost.as_mut() else { return };
        let part = &self.parts[g.part];
        let m = !g.mirror;
        // Keep the turn if allowed mirrored too, else take the first one that is.
        match (0..4u8).map(|k| (g.turns + k) % 4).find(|&t| nest::turn_allowed(part, t, m)) {
            Some(t) => {
                g.turns = t;
                g.mirror = m;
            }
            None => self.status = format!("'{}' may not be mirrored.", part.name),
        }
    }

    fn ghost_others(&self) -> &[Placed] {
        self.result.sheets.get(self.cur_sheet).map_or(&[][..], |v| v.as_slice())
    }

    /// Inside the sheet's margin and (with collision checking on) clear of every
    /// other part by the part gap.
    pub fn ghost_ok(&self, c: &Placed) -> bool {
        self.fits_sheet(c, self.cur_sheet) && (!self.collision || self.ghost_others().iter().all(|o| !nest::clash_exact(&self.parts, c, o, self.spec.gap - 1e-6)))
    }

    /// Where the ghost goes for the pointer at `w` (mm): centred on the pointer,
    /// clamped into the sheet, then approach-snapped. Returns (box, snapped, ok).
    pub fn ghost_candidate(&self, g: &Ghost, w: P, reach: f64) -> (Placed, bool, bool) {
        let (fw, fh) = footprint(&self.parts[g.part], g.turns);
        let mut c = Placed { part: g.part, x: (w[0] - fw / 2.0).round(), y: (w[1] - fh / 2.0).round(), w: fw, h: fh, turns: g.turns, mirror: g.mirror };
        clamp_to_limits(&mut c, &self.cur_spec());
        if let Some(circle) = nest::part_circle(&self.parts[g.part]) {
            let rings: Vec<(P, f64)> = self.ghost_others().iter().filter_map(|o| nest::placed_circle(&self.parts, o)).collect();
            if let Some(s) = circle_snap(c, circle, &rings, self.spec.gap, reach, |q| self.ghost_ok(q)) {
                return (s, true, true);
            }
        }
        if self.ghost_ok(&c) {
            if let Some(s) = self.contour_snap(c, reach) {
                return (s, true, true);
            }
        }
        // Overlapping, or nothing within reach: the box snap (it can push it clear).
        let (c, snapped) = approach_snap(c, self.ghost_others(), &self.cur_spec(), reach, |q| self.ghost_ok(q));
        (c, snapped, self.ghost_ok(&c))
    }

    /// Approach snap on the real contours: slide the ghost toward the nearest
    /// contour within reach (left, right, down or up) until it is exactly one gap
    /// away, then the same once across, so it settles into corners and notches.
    pub fn contour_snap(&self, c: Placed, reach: f64) -> Option<Placed> {
        let others = self.ghost_others();
        let dirs = [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]];
        let slide = |c: Placed, us: &[[f64; 2]]| -> Option<(f64, Placed)> {
            us.iter()
                .filter_map(|&u| {
                    let t = nest::free_slide(&self.parts, others, usize::MAX, c, u, &self.cur_spec(), reach + 1.0);
                    (t > 0.05 && t <= reach).then(|| (t, Placed { x: c.x + u[0] * t, y: c.y + u[1] * t, ..c }))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
        };
        let (_, first) = slide(c, &dirs)?;
        let across: &[[f64; 2]] = if first.y == c.y { &dirs[2..] } else { &dirs[..2] };
        Some(slide(first, across).map_or(first, |s| s.1))
    }

    /// Drop a copy at `c`. Beyond the ordered quantity the order grows by one.
    pub fn ghost_drop(&mut self, c: Placed) -> bool {
        if !self.ghost_ok(&c) {
            self.status = "It would overlap another part or leave the sheet — move it to a free spot.".into();
            return false;
        }
        self.checkpoint();
        if self.result.sheets.is_empty() {
            self.result.push_sheet(nest::SheetFormat::rect(self.spec.w, self.spec.h));
            self.cur_sheet = 0;
        }
        let s = self.cur_sheet.min(self.result.sheets.len() - 1);
        self.cur_sheet = s;
        let extra = self.placed_of(c.part) as u32 >= self.parts[c.part].qty;
        if extra {
            self.parts[c.part].qty += 1;
        }
        self.result.sheets[s].push(c);
        self.sel_placed = Some(self.result.sheets[s].len() - 1);
        self.sel_more.clear();
        self.refresh();
        let left = self.parts[c.part].qty.saturating_sub(self.placed_of(c.part) as u32);
        let name = self.parts[c.part].name.clone();
        self.status = if extra {
            format!("Dropped an extra '{name}' at X {:.0}  Y {:.0} (quantity now {}).", c.x, c.y, self.parts[c.part].qty)
        } else if left == 0 {
            format!("Dropped '{name}' at X {:.0}  Y {:.0} — all of them are placed.", c.x, c.y)
        } else {
            format!("Dropped '{name}' at X {:.0}  Y {:.0} — {left} left. Keep clicking, Esc to stop.", c.x, c.y)
        };
        true
    }

    /// Enter: place one copy automatically at the first free spot, lowest then
    /// leftmost, like the original's *Semi-Automatic* nesting button (text 1302: "Nest one part or module only").
    pub fn ghost_auto(&mut self) -> bool {
        match self.ghost_free_spot() {
            Some(c) => self.ghost_drop(c),
            None => {
                self.status = "No free spot on this sheet for it.".into();
                false
            }
        }
    }

    /// First free spot for the ghost, lowest then leftmost.
    pub fn ghost_free_spot(&self) -> Option<Placed> {
        let g = self.ghost?;
        crate::engine::free_spot(&self.parts, self.ghost_others(), &self.spec, &self.result.format(self.cur_sheet, &self.spec), g.part, g.turns, g.mirror, self.collision)
    }

    /// Keys, pointer and drawing of the ghost on the sheet canvas.
    pub fn ghost_canvas(&mut self, ui: &egui::Ui, painter: &egui::Painter, rect: Rect, resp: &egui::Response, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, s: f32) {
        let ctx = ui.ctx().clone();
        if !ctx.egui_wants_keyboard_input() {
            let (esc, rot, mir, enter) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::R) || i.key_pressed(Key::Space), i.key_pressed(Key::M), i.key_pressed(Key::Enter)));
            if esc {
                self.stop_ghost();
                return;
            }
            if rot {
                self.ghost_rotate();
            }
            if mir {
                self.ghost_mirror();
            }
            if enter {
                self.ghost_auto();
            }
        }
        if resp.secondary_clicked() {
            self.stop_ghost();
            return;
        }
        let Some(mut g) = self.ghost else { return };
        // (The automatic screenshot keeps the ghost where it was put.)
        let pos = ctx.pointer_latest_pos().filter(|p| rect.contains(*p) && self.shot.is_none());
        if let Some(p) = pos {
            g.at = Some(to_w(p));
        }
        let released = ctx.input(|i| i.pointer.primary_released());
        if g.dragging && released {
            // Dragged from a tile: drop where it's let go (outside the sheet view = cancel).
            self.ghost = None;
            if let (Some(_), Some(w)) = (pos, g.at) {
                let (c, _, _) = self.ghost_candidate(&g, w, (SNAP_PX / s) as f64);
                self.ghost_drop(c);
            } else {
                self.status = "Drop cancelled.".into();
            }
            return;
        }
        self.ghost = Some(g);
        let Some(w) = g.at else { return };
        let (c, snapped, ok) = self.ghost_candidate(&g, w, (SNAP_PX / s) as f64);
        if !g.dragging && resp.clicked() && pos.is_some() && self.ghost_drop(c) {
            // Keep dropping copies while some are left (Shift: keep going anyway).
            let left = self.parts[g.part].qty.saturating_sub(self.placed_of(g.part) as u32);
            if left == 0 && !ctx.input(|i| i.modifiers.shift) {
                self.ghost = None;
            }
            return;
        }
        self.paint_ghost(painter, &c, snapped, ok, to_s, pos);
        ctx.request_repaint();
    }

    fn paint_ghost(&self, painter: &egui::Painter, c: &Placed, snapped: bool, ok: bool, to_s: &dyn Fn(P) -> Pos2, pointer: Option<Pos2>) {
        let part = &self.parts[c.part];
        let tone = if ok { Color32::from_rgb(74, 222, 128) } else { Color32::from_rgb(248, 80, 90) };
        // The gap the part keeps around itself, then the see-through part.
        let r = Rect::from_two_pos(to_s([c.x, c.y]), to_s([c.x + c.w, c.y + c.h]));
        let g = Rect::from_two_pos(to_s([c.x - self.spec.gap, c.y - self.spec.gap]), to_s([c.x + c.w + self.spec.gap, c.y + c.h + self.spec.gap]));
        let round = nest::part_circle(part).map(|(lc, rad)| (c.map(lc), rad));
        match round {
            // Round parts keep a round clearance: no box around them.
            Some((ctr, rad)) => {
                let px = (to_s([ctr[0] + rad + self.spec.gap, ctr[1]]).x - to_s(ctr).x).abs();
                painter.circle_filled(to_s(ctr), px, with_alpha(tone, 28));
            }
            None => {
                painter.rect_filled(g, CornerRadius::same(2), with_alpha(tone, 28));
            }
        }
        paint_part_placed(painter, part, c, to_s, with_alpha(part_color(part.color), GHOST_ALPHA), Stroke::new(2.0, tone), with_alpha(th().sheet, 150));
        if round.is_none() {
            let dash = Stroke::new(1.0, with_alpha(tone, 200));
            painter.extend(EShape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], dash, 5.0, 4.0));
        }
        // Readout next to the ghost.
        let mut info = format!("X {:.0}  Y {:.0}", c.x, c.y);
        if c.turns % 4 != 0 || c.mirror {
            info += &format!("  ·  {}°{}", c.angle(), if c.mirror { " mirrored" } else { "" });
        }
        if snapped {
            info += "  ·  snapped";
        }
        if !ok {
            info += "  ·  no room";
        }
        let anchor = pointer.unwrap_or(r.right_top());
        let galley = painter.layout_no_wrap(info, FontId::proportional(12.0), Color32::WHITE);
        let size = galley.size() + Vec2::new(14.0, 8.0);
        let mut at = anchor + Vec2::new(16.0, -size.y - 10.0);
        let clip = painter.clip_rect();
        at.x = at.x.min(clip.right() - size.x - 4.0);
        at.y = at.y.max(clip.top() + 4.0);
        let chip = Rect::from_min_size(at, size);
        painter.rect_filled(chip, CornerRadius::same(6), with_alpha(if ok { th().accent } else { tone }, 230));
        painter.galley(chip.min + Vec2::new(7.0, 4.0), galley, Color32::WHITE);
    }
}

#[cfg(test)]
mod ghost_tests {
    use super::*;
    use crate::nest::Shape;

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None }
    }
    fn part() -> Part {
        Part { name: "P".into(), shape: Shape::Rectangle, w: 100.0, h: 40.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn at(x: f64, y: f64) -> Placed {
        Placed { part: 0, x, y, w: 100.0, h: 40.0, turns: 0, mirror: false }
    }

    #[test]
    fn clamps_into_the_sheet_instead_of_refusing() {
        let mut c = at(-50.0, 480.0);
        clamp_to_limits(&mut c, &spec());
        assert_eq!((c.x, c.y), (10.0, 450.0));
    }

    #[test]
    fn snaps_one_gap_from_a_neighbour() {
        let o = at(200.0, 100.0);
        // Ghost 8 mm right of the neighbour's right side: snaps to 5 mm (the gap).
        let c = at(308.0, 100.0);
        let (s, moved) = approach_snap(c, &[o], &spec(), 10.0, |q| !q.clashes(&o, 5.0 - 1e-6));
        assert!(moved);
        assert_eq!(s.x, 305.0);
        // Too far: stays put.
        let (s, moved) = approach_snap(at(330.0, 100.0), &[o], &spec(), 10.0, |_| true);
        assert!(!moved);
        assert_eq!(s.x, 330.0);
    }

    #[test]
    fn snaps_to_the_sheet_margin() {
        let (s, moved) = approach_snap(at(16.0, 14.0), &[], &spec(), 10.0, |_| true);
        assert!(moved);
        assert_eq!((s.x, s.y), (10.0, 10.0));
    }

    fn disc() -> Part {
        Part { shape: Shape::Disc, w: 100.0, h: 100.0, ..part() }
    }
    fn disc_at(x: f64, y: f64) -> Placed {
        Placed { part: 0, x, y, w: 100.0, h: 100.0, turns: 0, mirror: false }
    }

    #[test]
    fn round_parts_clash_by_their_circle_not_their_box() {
        let parts = [disc()];
        let (a, b) = (disc_at(0.0, 0.0), disc_at(90.0, 90.0));
        // The boxes overlap, the circles (127 mm apart, 105 needed) don't.
        assert!(a.clashes(&b, 5.0));
        assert!(!nest::clash_exact(&parts, &a, &b, 5.0));
        assert!(nest::clash_exact(&parts, &a, &disc_at(60.0, 60.0), 5.0));
        // A square part still uses its box.
        let sq = [part()];
        assert!(nest::clash_exact(&sq, &at(0.0, 0.0), &at(90.0, 30.0), 5.0));
        assert_eq!(nest::part_circle(&part()), None);
    }

    #[test]
    fn a_ring_snaps_into_the_pocket_between_two_rings() {
        let others = [([60.0, 60.0], 50.0), ([170.0, 60.0], 50.0)];
        // Ghost centre near (118, 145): the pocket is at (115, 149.44).
        let g = disc_at(68.0, 95.0);
        let s = circle_snap(g, ([50.0, 50.0], 50.0), &others, 5.0, 10.0, |_| true).expect("snaps");
        let c = [s.x + 50.0, s.y + 50.0];
        assert!((c[0] - 115.0).abs() < 1e-6, "{c:?}");
        assert!((c[1] - (60.0 + (105.0f64 * 105.0 - 55.0 * 55.0).sqrt())).abs() < 1e-6, "{c:?}");
        // Far from both: no snap.
        assert!(circle_snap(disc_at(300.0, 300.0), ([50.0, 50.0], 50.0), &others, 5.0, 10.0, |_| true).is_none());
    }

    fn tri() -> Part {
        Part { shape: Shape::Triangle, w: 100.0, h: 100.0, ..part() }
    }
    fn tri_at(x: f64, y: f64, turns: u8) -> Placed {
        Placed { part: 0, x, y, w: 100.0, h: 100.0, turns, mirror: false }
    }

    #[test]
    fn any_part_clashes_by_its_contour_not_its_box() {
        let parts = [tri()];
        // Two gussets facing each other across the diagonal: boxes overlap a lot,
        // the slanted edges are ~21 mm apart.
        let (a, b) = (tri_at(10.0, 10.0, 0), tri_at(40.0, 10.0, 2));
        assert!(a.clashes(&b, 5.0));
        assert!(!nest::clash_exact(&parts, &a, &b, 5.0));
        assert!(nest::clash_exact(&parts, &a, &tri_at(10.0, 10.0, 2), 5.0));
        // A small part inside a ring's hole is fine; across its edge it is not.
        let ring = Part { shape: Shape::PlateHole, w: 300.0, h: 300.0, ..part() };
        let small = Part { shape: Shape::Rectangle, w: 20.0, h: 20.0, ..part() };
        let ps = [ring, small];
        let big = Placed { part: 0, x: 0.0, y: 0.0, w: 300.0, h: 300.0, turns: 0, mirror: false };
        let hole = nest::placed_shape(&ps, &big).1;
        if let Some(h) = hole.first() {
            let c = nest::centroid(h);
            let inside = Placed { part: 1, x: c[0] - 10.0, y: c[1] - 10.0, w: 20.0, h: 20.0, turns: 0, mirror: false };
            assert!(!nest::clash_exact(&ps, &inside, &big, 5.0), "fits in the hole");
            let on_edge = Placed { x: 5.0, y: 140.0, ..inside };
            assert!(nest::clash_exact(&ps, &on_edge, &big, 5.0));
        }
    }

    #[test]
    fn compaction_tucks_parts_along_their_contours() {
        let parts = [tri()];
        let s = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        // The flipped gusset starts box-to-box right of the first one.
        let mut sheet = vec![tri_at(10.0, 10.0, 0), tri_at(115.0, 10.0, 2)];
        nest::compact_contours(&parts, &mut sheet, &s);
        assert!(sheet[1].x < 30.0, "slid into the diagonal: {:?}", sheet[1]);
        assert!(!nest::clash_exact(&parts, &sheet[0], &sheet[1], 5.0 - 1e-6));
        // Exactly one gap apart along the slanted edges (within the back-off).
        let (a, b) = (nest::placed_shape(&parts, &sheet[0]).0, nest::placed_shape(&parts, &sheet[1]).0);
        assert!(nest::contours_within(&a, &b, 5.3));
    }

    #[test]
    fn turns_follow_the_part_permissions() {
        let mut p = part();
        assert_eq!(next_turn(&p, 0, false), Some(1));
        p.rotate = false;
        assert_eq!(next_turn(&p, 0, false), Some(2));
        p.rots.plain = [true, false, false, false];
        assert_eq!(next_turn(&p, 0, false), None);
        assert_eq!(footprint(&part(), 1), (40.0, 100.0));
    }
}
