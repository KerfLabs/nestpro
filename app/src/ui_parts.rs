//! Look-and-feel pieces: ribbon, docked panes, explorer tree, part tiles and
//! the status bar. All colours come from the light/dark theme in `theme.rs`.

use crate::nest::{Part, Shape, P};
use crate::theme::{is_dark, th};
use crate::{cnc, App, BottomTab, Tab};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, Vec2};

// Modern part colours that read well on the dark sheet.
pub const PALETTE: [(u8, u8, u8); 10] = [
    (16, 185, 129),  // emerald
    (251, 146, 60),  // orange
    (244, 114, 182), // pink
    (168, 162, 158), // stone
    (56, 189, 248),  // sky
    (167, 139, 250), // violet
    (251, 113, 133), // rose
    (99, 102, 241),  // indigo
    (217, 160, 70),  // amber
    (45, 212, 191),  // teal
];

pub fn part_color(i: usize) -> Color32 {
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    Color32::from_rgb(r, g, b)
}

pub fn darker(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    New,
    Prev,
    Next,
    NestAll,
    AddPart,
    Rotate,
    Remove,
    Clear,
    Collision,
    Times,
    Program,
    CutPath,
    Fit,
    Numbers,
    Grid,
    Undo,
    Redo,
    Compact,
    Copy,
    Export,
    Sun,
    Moon,
    Import,
    LeadIn,
    Joint,
    Loop,
    CutTech,
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Draw a part's geometry with a mapping from part coordinates to the screen.
pub fn paint_part(painter: &egui::Painter, part: &Part, map: impl Fn(P) -> Pos2, fill: Color32, edge: Stroke, hole: Color32) {
    paint_part_ex(painter, part, map, fill, edge, hole, true);
}

/// A placed part on the sheet: its texts stay readable (see `PartText::sheet_strokes`).
pub fn paint_part_placed(painter: &egui::Painter, part: &Part, pl: &crate::nest::Placed, to_s: impl Fn(P) -> Pos2, fill: Color32, edge: Stroke, hole: Color32) {
    paint_part_ex(painter, part, |q| to_s(pl.map(q)), fill, edge, hole, false);
    for t in &part.texts {
        let st = match t.kind {
            crate::nest::TextKind::Mark => Stroke::new(1.2, darker(fill, 0.55)),
            crate::nest::TextKind::Special => Stroke::new(1.2, Color32::from_rgb(124, 58, 237)),
            crate::nest::TextKind::Note => Stroke::new(0.8, with_alpha(darker(fill, 0.5), 150)),
        };
        for s in t.sheet_strokes(&t.text, pl, crate::nest::TEXT_ANGLE_STEP) {
            painter.add(EShape::line(s.iter().map(|&p| to_s(p)).collect(), st));
        }
    }
}

fn paint_part_ex(painter: &egui::Painter, part: &Part, map: impl Fn(P) -> Pos2, fill: Color32, edge: Stroke, hole: Color32, with_texts: bool) {
    let g = part.geometry();
    // Imported drawings are filled from triangles; paint them as one seamless mesh
    // (separate anti-aliased triangles leave hairline seams).
    if let (Shape::Custom, Some(c)) = (part.shape, &part.custom) {
        let mesh_of = |tris: &[Vec<P>], col: Color32| {
            let mut m = egui::Mesh::default();
            for t in tris {
                let base = m.vertices.len() as u32;
                for &q in t {
                    m.colored_vertex(map(q), col);
                }
                m.add_triangle(base, base + 1, base + 2);
            }
            m
        };
        painter.add(EShape::mesh(mesh_of(&c.fills, fill)));
        painter.add(EShape::mesh(mesh_of(&c.hole_fills, hole)));
        for h in &g.holes {
            painter.add(EShape::closed_line(h.iter().map(|&p| map(p)).collect(), edge));
        }
        painter.add(EShape::closed_line(g.outline.iter().map(|&p| map(p)).collect(), edge));
        // Marking lines (drawing layers set to "Mark"): thin dashed lines.
        let mark = Stroke::new(1.2, darker(fill, 0.3));
        for m in &c.marks {
            let pts: Vec<Pos2> = m.iter().map(|&p| map(p)).collect();
            painter.extend(EShape::dashed_line(&pts, mark, 4.0, 2.5));
        }
        if with_texts {
            paint_texts(painter, part, &map, fill);
        }
        return;
    }
    for f in &g.fills {
        painter.add(EShape::convex_polygon(f.iter().map(|&p| map(p)).collect(), fill, Stroke::NONE));
    }
    {
        for h in &g.holes {
            let pts: Vec<Pos2> = h.iter().map(|&p| map(p)).collect();
            painter.add(EShape::convex_polygon(pts.clone(), hole, Stroke::NONE));
            painter.add(EShape::closed_line(pts, edge));
        }
    }
    painter.add(EShape::closed_line(g.outline.iter().map(|&p| map(p)).collect(), edge));
    if with_texts {
        paint_texts(painter, part, &map, fill);
    }
}

/// Texts on a part: marking and special texts as solid strokes (what the laser
/// marks), notes as thin dashed strokes (drawn only, never machined).
pub fn paint_texts(painter: &egui::Painter, part: &Part, map: &impl Fn(P) -> Pos2, fill: Color32) {
    for t in &part.texts {
        let (st, dashed) = match t.kind {
            crate::nest::TextKind::Mark => (Stroke::new(1.4, darker(fill, 0.55)), false),
            crate::nest::TextKind::Special => (Stroke::new(1.4, Color32::from_rgb(124, 58, 237)), false),
            crate::nest::TextKind::Note => (Stroke::new(1.0, with_alpha(darker(fill, 0.5), 170)), true),
        };
        for s in t.strokes(&t.text) {
            let pts: Vec<Pos2> = s.iter().map(|&p| map(p)).collect();
            if dashed {
                painter.extend(EShape::dashed_line(&pts, st, 2.0, 2.0));
            } else {
                painter.add(EShape::line(pts, st));
            }
        }
    }
}

pub fn draw_icon(p: &egui::Painter, r: Rect, icon: Icon, dim: bool) {
    let t = th();
    let (ink, accent) = if dim { (with_alpha(t.text_dim, 110), with_alpha(t.text_dim, 110)) } else { (t.text, t.accent_fg) };
    draw_icon_with(p, r, icon, ink, accent, false);
}

/// Clean line-style icons. `mono` draws everything in `ink` (used on the
/// filled accent button).
/// Outline of a crescent moon of radius `r` centred on `c`: the outer circle's
/// arc away from the bite, then the bite's arc back inside it.
pub fn crescent(c: Pos2, r: f32) -> Vec<Pos2> {
    use std::f32::consts::TAU;
    let ic = c + Vec2::new(0.48, -0.40) * r; // centre of the bite
    let ir = 0.85 * r;
    let d = (ic - c).length();
    let u = (ic - c) / d;
    let a = (r * r - ir * ir + d * d) / (2.0 * d);
    let h = (r * r - a * a).max(0.0).sqrt();
    let base = c + u * a;
    let perp = Vec2::new(-u.y, u.x);
    let (p1, p2) = (base + perp * h, base - perp * h);
    // Points along a circle from one angle to another, going the way that passes `via`.
    let arc = |ctr: Pos2, rad: f32, from: f32, to: f32, via: f32| -> Vec<Pos2> {
        let ccw = (to - from).rem_euclid(TAU);
        let sweep = if (via - from).rem_euclid(TAU) < ccw { ccw } else { ccw - TAU };
        (0..=18).map(|i| from + sweep * i as f32 / 18.0).map(|t| ctr + Vec2::new(t.cos(), t.sin()) * rad).collect()
    };
    let away = (c - ic).angle();
    let mut pts = arc(c, r, (p1 - c).angle(), (p2 - c).angle(), away);
    pts.extend(arc(ic, ir, (p2 - ic).angle(), (p1 - ic).angle(), away).into_iter().skip(1));
    pts.pop();
    pts
}

pub fn draw_icon_with(p: &egui::Painter, r: Rect, icon: Icon, ink: Color32, accent: Color32, mono: bool) {
    let c = r.center();
    let s = Stroke::new(1.7, ink);
    let a = Stroke::new(1.9, accent);
    let v = |x: f32, y: f32| c + Vec2::new(x, y);
    let rr = |rect: Rect, st: Stroke| p.rect_stroke(rect, CornerRadius::same(3), st, egui::StrokeKind::Middle);
    let poly = |pts: Vec<Pos2>, st: Stroke| {
        p.add(EShape::line(pts, st));
    };
    match icon {
        Icon::New => {
            rr(Rect::from_center_size(c, Vec2::new(18.0, 22.0)), s);
            p.line_segment([v(-4.0, 2.0), v(4.0, 2.0)], a);
            p.line_segment([v(0.0, -2.0), v(0.0, 6.0)], a);
        }
        Icon::Prev | Icon::Next => {
            rr(Rect::from_center_size(c, Vec2::new(24.0, 18.0)), s);
            let d = if icon == Icon::Next { 1.0 } else { -1.0 };
            poly(vec![v(-2.5 * d, -5.0), v(2.5 * d, 0.0), v(-2.5 * d, 5.0)], a);
        }
        Icon::NestAll => {
            let cols = [4usize, 1, 0, 5];
            for (k, col) in cols.iter().enumerate() {
                let off = Vec2::new(if k % 2 == 0 { -5.5 } else { 5.5 }, if k < 2 { -5.5 } else { 5.5 });
                let sq = Rect::from_center_size(c + off, Vec2::splat(9.0));
                let fill = if mono { with_alpha(ink, [255, 200, 200, 150][k]) } else { part_color(*col) };
                p.rect_filled(sq, CornerRadius::same(2), fill);
            }
        }
        Icon::AddPart => {
            rr(Rect::from_min_size(v(-10.0, -4.0), Vec2::splat(14.0)), s);
            p.line_segment([v(6.0, -9.0), v(6.0, -1.0)], a);
            p.line_segment([v(2.0, -5.0), v(10.0, -5.0)], a);
        }
        Icon::Rotate => {
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let ang = -0.4 + i as f32 / 24.0 * 4.6;
                    c + Vec2::new(ang.cos(), ang.sin()) * 8.5
                })
                .collect();
            let end = *pts.last().unwrap();
            poly(pts, s);
            poly(vec![end + Vec2::new(-4.0, -1.0), end, end + Vec2::new(0.5, -4.5)], a);
        }
        Icon::Remove => {
            p.line_segment([v(-8.0, -6.0), v(8.0, -6.0)], s);
            poly(vec![v(-3.0, -6.0), v(-3.0, -9.0), v(3.0, -9.0), v(3.0, -6.0)], s);
            rr(Rect::from_min_max(v(-6.0, -6.0), v(6.0, 9.0)), s);
            p.line_segment([v(-2.0, -2.0), v(-2.0, 5.0)], a);
            p.line_segment([v(2.0, -2.0), v(2.0, 5.0)], a);
        }
        Icon::Clear => {
            rr(Rect::from_center_size(c, Vec2::new(22.0, 16.0)), s);
            p.line_segment([v(-6.0, 5.0), v(6.0, -5.0)], a);
        }
        Icon::Collision => {
            rr(Rect::from_min_size(v(-10.0, -10.0), Vec2::splat(13.0)), s);
            rr(Rect::from_min_size(v(-3.0, -3.0), Vec2::splat(13.0)), a);
        }
        Icon::Times => {
            p.circle_stroke(c, 9.0, s);
            p.line_segment([c, v(0.0, -5.5)], a);
            p.line_segment([c, v(4.0, 2.0)], a);
        }
        Icon::Program => {
            poly(vec![v(-5.0, -6.0), v(-10.0, 0.0), v(-5.0, 6.0)], s);
            poly(vec![v(5.0, -6.0), v(10.0, 0.0), v(5.0, 6.0)], s);
            p.line_segment([v(2.0, -8.0), v(-2.0, 8.0)], a);
        }
        Icon::CutPath => {
            p.extend(EShape::dashed_line(&[v(-10.0, 8.0), v(-2.0, 0.0)], Stroke::new(1.5, ink), 2.5, 2.0));
            p.line_segment([v(-2.0, 0.0), v(8.0, -6.0)], a);
            p.circle_filled(v(-2.0, 0.0), 2.4, accent);
        }
        Icon::Fit => {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let cn = v(9.0 * dx, 9.0 * dy);
                poly(vec![cn - Vec2::new(0.0, 5.0 * dy), cn, cn - Vec2::new(5.0 * dx, 0.0)], s);
            }
            rr(Rect::from_center_size(c, Vec2::splat(7.0)), a);
        }
        Icon::Numbers => {
            p.text(c, Align2::CENTER_CENTER, "123", FontId::proportional(12.5), ink);
            p.line_segment([v(-9.0, 8.0), v(9.0, 8.0)], a);
        }
        Icon::Grid => {
            let g = Rect::from_center_size(c, Vec2::new(20.0, 16.0));
            rr(g, s);
            for k in 1..3 {
                let x = g.left() + g.width() * k as f32 / 3.0;
                p.line_segment([Pos2::new(x, g.top()), Pos2::new(x, g.bottom())], Stroke::new(1.0, accent));
            }
            p.line_segment([Pos2::new(g.left(), c.y), Pos2::new(g.right(), c.y)], Stroke::new(1.0, accent));
        }
        Icon::Undo | Icon::Redo => {
            let dir = if icon == Icon::Undo { -1.0 } else { 1.0 };
            let ctr = v(0.0, 3.0);
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let ang = std::f32::consts::PI * (i as f32 / 16.0);
                    ctr + Vec2::new(-dir * ang.cos() * 8.0, -ang.sin() * 7.0)
                })
                .collect();
            let end = *pts.last().unwrap();
            poly(pts, s);
            poly(vec![end + Vec2::new(-3.5, 3.5), end + Vec2::new(0.0, 0.5), end + Vec2::new(3.5, 3.5)].into_iter().map(|q| q + Vec2::new(0.0, 0.0)).collect(), a);
        }
        Icon::Compact => {
            for (k, len) in [14.0, 10.0, 6.0].into_iter().enumerate() {
                let y = -6.0 + k as f32 * 6.0;
                p.rect_filled(Rect::from_min_size(v(-10.0, y - 1.5), Vec2::new(len, 3.0)), CornerRadius::same(2), ink);
            }
            p.line_segment([v(10.0, 0.0), v(2.0, 0.0)], a);
            poly(vec![v(5.0, -3.0), v(2.0, 0.0), v(5.0, 3.0)], a);
        }
        Icon::Copy => {
            rr(Rect::from_min_size(v(-9.0, -9.0), Vec2::new(12.0, 14.0)), s);
            rr(Rect::from_min_size(v(-3.0, -4.0), Vec2::new(12.0, 14.0)), a);
        }
        Icon::Export => {
            poly(vec![v(-9.0, 2.0), v(-9.0, 8.0), v(9.0, 8.0), v(9.0, 2.0)], s);
            p.line_segment([v(0.0, 4.0), v(0.0, -9.0)], a);
            poly(vec![v(-4.0, -5.0), v(0.0, -9.0), v(4.0, -5.0)], a);
        }
        Icon::LeadIn => {
            // Part corner with a lead coming in on the bisector.
            poly(vec![v(-10.0, -2.0), v(4.0, -2.0), v(4.0, 10.0)], s);
            p.line_segment([v(10.0, -9.0), v(4.0, -2.0)], a);
            p.circle_filled(v(10.0, -9.0), 2.2, accent);
        }
        Icon::Joint => {
            // Contour with a small uncut bridge.
            rr(Rect::from_center_size(c, Vec2::new(20.0, 16.0)), s);
            p.line_segment([v(-2.5, -8.0), v(2.5, -8.0)], Stroke::new(3.5, ink.gamma_multiply(0.0)));
            p.rect_filled(Rect::from_center_size(v(0.0, -8.0), Vec2::new(5.0, 4.0)), CornerRadius::ZERO, accent);
            p.rect_filled(Rect::from_center_size(v(0.0, 8.0), Vec2::new(5.0, 4.0)), CornerRadius::ZERO, accent);
        }
        Icon::Loop => {
            // Corner with a loop outside it.
            poly(vec![v(-10.0, 8.0), v(-10.0, -4.0), v(6.0, -4.0)], s);
            p.circle_stroke(v(-13.0, -7.0), 3.5, a);
        }
        Icon::CutTech => {
            // Laser head and beam.
            poly(vec![v(-6.0, -10.0), v(6.0, -10.0), v(3.0, -2.0), v(-3.0, -2.0), v(-6.0, -10.0)], s);
            p.line_segment([v(0.0, -2.0), v(0.0, 7.0)], a);
            p.line_segment([v(-8.0, 9.0), v(8.0, 9.0)], s);
        }
        Icon::Import => {
            // A drawing sheet with "DXF" and an arrow coming in.
            rr(Rect::from_center_size(v(-2.0, 0.0), Vec2::new(18.0, 22.0)), s);
            p.text(v(-2.0, 3.0), Align2::CENTER_CENTER, "DXF", FontId::proportional(7.5), ink);
            p.line_segment([v(12.0, -9.0), v(12.0, -1.0)], a);
            poly(vec![v(9.0, -4.0), v(12.0, -1.0), v(15.0, -4.0)], a);
        }
        Icon::Sun => {
            p.circle_stroke(c, 4.0, s);
            for k in 0..8 {
                let ang = k as f32 / 8.0 * std::f32::consts::TAU;
                let d = Vec2::new(ang.cos(), ang.sin());
                p.line_segment([c + d * 7.0, c + d * 9.5], s);
            }
        }
        Icon::Moon => {
            p.add(EShape::closed_line(crescent(c, 8.0), s));
        }
    }
}

/// Big ribbon button: a picture with a (possibly two-line) caption below.
pub(crate) fn ribbon_button(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool, enabled: bool) -> egui::Response {
    ribbon_button_ex(ui, icon, label, active, enabled, false)
}

/// Modern ribbon button: flat until hovered, a soft tint when hovered,
/// an accent tint with a pill marker when switched on, and a solid accent
/// fill for the main action (`primary`).
pub(crate) fn ribbon_button_ex(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool, enabled: bool, primary: bool) -> egui::Response {
    let t = th();
    let text_col = if !enabled {
        with_alpha(t.text_dim, 120)
    } else if primary {
        Color32::WHITE
    } else if active {
        t.accent_fg
    } else {
        t.text
    };
    let galley = ui.painter().layout(label.to_string(), FontId::proportional(12.0), text_col, 70.0);
    let w = galley.size().x.max(38.0) + 14.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 70.0), sense);
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("hover"), enabled && resp.hovered(), 0.12);
    let pressed = enabled && resp.is_pointer_button_down_on();
    let p = ui.painter();
    let card = rect.shrink(1.0);
    let radius = CornerRadius::same(8);
    let ir = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 21.0), Vec2::new(32.0, 32.0));
    if primary && enabled {
        let base = if pressed { darker(t.accent, 0.85) } else { lerp_color(t.accent, Color32::WHITE, 0.10 * hover) };
        p.rect_filled(card.translate(Vec2::new(0.0, 2.0)), radius, with_alpha(t.accent, (50.0 + 50.0 * hover) as u8));
        p.rect_filled(card, radius, base);
        draw_icon_with(p, ir, icon, Color32::WHITE, Color32::WHITE, true);
    } else {
        if active {
            p.rect_filled(card, radius, t.accent_soft);
            p.rect_stroke(card, radius, Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
            // The "on" marker sits under a one-line label, or at the top of the
            // button when the label takes two lines (so it never covers text).
            let y = if galley.rows.len() > 1 { rect.top() + 3.5 } else { rect.bottom() - 3.5 };
            let pill = Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(16.0, 3.0));
            p.rect_filled(pill, CornerRadius::same(2), t.accent_fg);
        } else if hover > 0.0 || pressed {
            let fill = if pressed { t.pressed } else { with_alpha(t.hover, (hover * 255.0) as u8) };
            p.rect_filled(card, radius, fill);
        }
        let lift = if pressed { 1.0 } else { -1.0 * hover };
        draw_icon(p, ir.translate(Vec2::new(0.0, lift)), icon, !enabled);
    }
    let gpos = Pos2::new(rect.center().x - galley.size().x / 2.0, rect.top() + 40.0);
    p.galley(gpos, galley, text_col);
    resp
}

/// A ribbon group: its controls with the group's name centred underneath.
pub(crate) fn ribbon_group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let top = ui.cursor().top();
    ui.vertical(|ui| {
        let inner = ui.horizontal(|ui| {
            ui.set_min_height(70.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui);
        });
        let w = inner.response.rect.width().max(40.0);
        ui.add_sized([w, 14.0], egui::Label::new(RichText::new(title).size(11.0).color(th().text_dim)));
    });
    ui.add_space(5.0);
    let x = ui.cursor().left();
    ui.painter().line_segment([Pos2::new(x, top + 8.0), Pos2::new(x, top + 78.0)], Stroke::new(1.0, th().border));
    ui.add_space(7.0);
}

/// Title bar of a docked pane.
pub fn pane_header(ui: &mut egui::Ui, title: &str) {
    let t = th();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, t.pane_header);
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.border));
    p.text(rect.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, title, FontId::proportional(13.0), t.text_strong);
}

pub(crate) fn check_mark(p: &egui::Painter, at: Pos2, size: f32) {
    p.add(EShape::line(
        vec![at + Vec2::new(-size * 0.5, 0.0), at + Vec2::new(-size * 0.1, size * 0.45), at + Vec2::new(size * 0.6, -size * 0.55)],
        Stroke::new(2.2, th().ok_fg),
    ));
}

/// Small rounded label "chip".
pub(crate) fn chip(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(9, 2))
        .show(ui, |ui| ui.label(RichText::new(text).size(12.0).color(fg)));
}

fn row(ui: &mut egui::Ui, k: &str, v: String) {
    ui.label(k);
    ui.label(RichText::new(v).strong());
    ui.end_row();
}

fn fmt_time(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 { format!("{} h {:02} min", s / 3600, (s % 3600) / 60) } else { format!("{} min {:02} s", s / 60, s % 60) }
}

/// Gallery of part shapes (like a parametric-parts library): click one to add it.
/// Most library parts shown in the Add product menu (the rest via search
/// or the Library window).
pub const MENU_LIBRARY_PARTS: usize = 8;

pub(crate) fn add_part_menu(app: &mut App, resp: &egui::Response) {
    if resp.clicked() && !app.libui.loaded {
        app.lib_refresh();
    }
    egui::Popup::menu(resp).show(|ui| {
        // Preprogrammed parts from this app's library (as Lantek's "Parts
        // From Database": products with their own technology).
        ui.horizontal(|ui| {
            ui.label(RichText::new("From the library").strong().color(th().text_strong));
            ui.add(egui::TextEdit::singleline(&mut app.libui.part_search).hint_text("🔍 search").desired_width(150.0));
        });
        // Lantek lists products of the job's material and thickness
        // (PK3); "all" drops both filters.
        ui.checkbox(
            &mut app.libui.part_all,
            RichText::new(format!("Every material — not only {} {} mm", crate::cnc::MATERIALS.get(app.spec.material).copied().unwrap_or(""), app.spec.thickness)).size(11.5),
        );
        ui.add_space(4.0);
        let needle = app.libui.part_search.to_lowercase();
        let (mat, thk) = if app.libui.part_all { (None, crate::library::THICKNESS_FILTER_MIN - 1.0) } else { (Some(app.spec.material), app.spec.thickness) };
        let found: Vec<(u64, crate::nest::Part, bool, String)> = app
            .libui
            .products
            .iter()
            .filter(|(e, _)| crate::library::product_matches(e, mat, thk, &needle))
            .take(MENU_LIBRARY_PARTS)
            .map(|(e, p)| (e.id, p.clone(), e.tech, format!("{} {} mm", crate::cnc::MATERIALS.get(e.material).copied().unwrap_or(""), e.thickness)))
            .collect();
        if app.libui.products.is_empty() {
            ui.label(RichText::new("No parts in the library yet — set a part's technology, then \"Save to library\" in Part Properties.").size(11.5).color(th().text_dim));
        } else if found.is_empty() {
            ui.label(RichText::new("No library part matches.").size(11.5).color(th().text_dim));
        }
        let mut pick: Option<u64> = None;
        egui::Grid::new("lib_gallery").spacing([6.0, 6.0]).show(ui, |ui| {
            for (k, (id, part, tech, info)) in found.iter().enumerate() {
                let (r, cell) = ui.allocate_exact_size(Vec2::new(104.0, 104.0), Sense::click());
                let hov = ui.ctx().animate_bool_with_time(cell.id.with("lg"), cell.hovered(), 0.1);
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(8), lerp_color(th().hover, th().accent_soft, hov));
                let area = Rect::from_min_max(r.min + Vec2::new(14.0, 10.0), Pos2::new(r.right() - 14.0, r.bottom() - 38.0));
                let (bw, bh) = part.bbox();
                let s = (area.width() / bw.max(1e-6) as f32).min(area.height() / bh.max(1e-6) as f32);
                let o = area.center() + Vec2::new(-(bw as f32) * s / 2.0, bh as f32 * s / 2.0);
                paint_part(p, part, |q| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s), part_color(part.color), Stroke::new(1.0, darker(part_color(part.color), 0.5)), lerp_color(th().hover, th().accent_soft, hov));
                let name: String = if part.name.chars().count() > 14 { format!("{}…", part.name.chars().take(13).collect::<String>()) } else { part.name.clone() };
                p.text(Pos2::new(r.center().x, r.bottom() - 25.0), Align2::CENTER_CENTER, name, FontId::proportional(11.5), th().text_strong);
                p.text(Pos2::new(r.center().x, r.bottom() - 11.0), Align2::CENTER_CENTER, info, FontId::proportional(10.0), th().text_dim);
                if *tech {
                    let b = Rect::from_min_size(Pos2::new(r.right() - 20.0, r.top() + 4.0), Vec2::new(16.0, 16.0));
                    p.rect_filled(b, CornerRadius::same(4), th().accent);
                    p.text(b.center(), Align2::CENTER_CENTER, "T", FontId::proportional(11.0), Color32::WHITE);
                }
                if cell.on_hover_text(format!("{} — {info}{}", part.name, if *tech { " · with its technology" } else { "" })).clicked() {
                    pick = Some(*id);
                }
                if k % 4 == 3 {
                    ui.end_row();
                }
            }
        });
        let mut all = false;
        if !app.libui.products.is_empty() && ui.button(format!("All {} library parts…", app.libui.products.len())).clicked() {
            all = true;
        }
        if let Some(id) = pick {
            app.add_library_product(id);
            ui.close();
        }
        if all {
            app.open_library_products();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Blank shapes").strong().color(th().text_strong));
        ui.add_space(4.0);
        egui::Grid::new("part_gallery").spacing([6.0, 6.0]).show(ui, |ui| {
            for (k, sh) in Shape::ALL.into_iter().enumerate() {
                let (w, h) = match sh {
                    Shape::Strip => (600.0, 60.0),
                    Shape::SlottedPlate => (400.0, 140.0),
                    Shape::LBracket => (200.0, 160.0),
                    _ => (200.0, 200.0),
                };
                let sample = Part { name: String::new(), shape: sh, w, h, qty: 1, rotate: true, color: k + 4, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
                let (r, cell) = ui.allocate_exact_size(Vec2::new(104.0, 92.0), Sense::click());
                let hov = ui.ctx().animate_bool_with_time(cell.id.with("g"), cell.hovered(), 0.1);
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(8), lerp_color(th().hover, th().accent_soft, hov));
                if hov > 0.0 {
                    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, with_alpha(th().accent_border, (hov * 255.0) as u8)), egui::StrokeKind::Inside);
                }
                let area = Rect::from_min_max(r.min + Vec2::new(14.0, 10.0), Pos2::new(r.right() - 14.0, r.bottom() - 26.0));
                let (bw, bh) = sample.bbox();
                let s = (area.width() / bw as f32).min(area.height() / bh as f32);
                let o = area.center() + Vec2::new(-(bw as f32) * s / 2.0, bh as f32 * s / 2.0);
                paint_part(
                    p,
                    &sample,
                    |q| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s),
                    part_color(sample.color),
                    Stroke::new(1.0, darker(part_color(sample.color), 0.5)),
                    lerp_color(th().hover, th().accent_soft, hov),
                );
                p.text(Pos2::new(r.center().x, r.bottom() - 13.0), Align2::CENTER_CENTER, sh.label(), FontId::proportional(11.5), th().text);
                if cell.clicked() {
                    app.add_part(sh);
                    ui.close();
                }
                if k % 4 == 3 {
                    ui.end_row();
                }
            }
        });
    });
}

impl App {
    /// Window title row with a small quick-access toolbar on the left.
    pub fn title_row(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let small = [Icon::NestAll, Icon::New, Icon::Export, Icon::Undo, Icon::Redo, Icon::Times, Icon::Fit];
            let tips = ["Nest all (F5)", "New job", "Export programs", "Undo (Ctrl+Z)", "Redo (Ctrl+Y)", "Times and costs", "Fit sheet"];
            for (ic, tip) in small.into_iter().zip(tips) {
                let enabled = match ic {
                    Icon::Undo => !self.undo.is_empty(),
                    Icon::Redo => !self.redo.is_empty(),
                    _ => true,
                };
                let (r, resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                let hov = ui.ctx().animate_bool_with_time(resp.id.with("qa"), enabled && resp.hovered(), 0.1);
                if hov > 0.0 {
                    ui.painter().rect_filled(r, CornerRadius::same(5), with_alpha(th().hover, (hov * 255.0) as u8));
                }
                let painter = ui.painter_at(r);
                draw_icon(&painter, Rect::from_center_size(r.center(), Vec2::splat(22.0)), ic, !enabled);
                if resp.on_hover_text(tip).clicked() && enabled {
                    match ic {
                        Icon::NestAll => self.run_nesting(),
                        Icon::New => self.new_job(),
                        Icon::Export => self.export(),
                        Icon::Undo => self.undo(),
                        Icon::Redo => self.redo(),
                        Icon::Times => self.show_times = !self.show_times,
                        _ => {
                            self.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                        }
                    }
                }
            }
        });
        let n = self.result.sheets.len();
        let title = if let (crate::Workspace::Part, Some(i)) = (self.workspace, self.shown_part()) {
            // Like the original drawing module: "Drawing - <part> (<machine> / <material> / <thickness>) - [<database>]".
            format!(
                "Drawing - {} ({} / {} / {:.4}) - [{}]",
                self.parts[i].name,
                self.spec.machine.label(),
                cnc::MATERIALS[self.spec.material].to_uppercase(),
                self.spec.thickness,
                self.db_label()
            )
        } else if self.workspace == crate::Workspace::Jobs {
            // Original title rule (MECHANICS.md "Mechanics read from the original" #8): "(screen) - " prefix, then
            // "<order> - [<database>] - <app>", or "[<database>] - <app>" with no order.
            if self.job_name.is_empty() {
                format!("Jobs (CAM) - [{}] - Sheet Nesting", self.db_label())
            } else {
                format!("Jobs (CAM) - {} - [{}] - Sheet Nesting", self.job_name, self.db_label())
            }
        } else {
            format!(
                "Nesting and machining - {} - {}{} [sheet {} of {}]",
                self.job_name,
                if n > 0 { "1W " } else { "" },
                self.sheet_label(self.cur_sheet, 0),
                if n > 0 { self.cur_sheet + 1 } else { 0 },
                n
            )
        };
        ui.painter().text(
            Pos2::new(full.center().x, full.center().y),
            Align2::CENTER_CENTER,
            title,
            FontId::proportional(13.0),
            th().text_dim,
        );
    }

    pub fn ribbon_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(4.0);
            // Segmented switch: Jobs screen, Nesting screen, Part (one part's drawing).
            let (track, _) = ui.allocate_exact_size(Vec2::new(216.0, 26.0), Sense::hover());
            ui.painter().rect_filled(track, CornerRadius::same(8), th().hover);
            let seg_w = track.width() / 3.0;
            let target = match self.workspace {
                crate::Workspace::Jobs => 0.0,
                crate::Workspace::Nesting => 1.0,
                crate::Workspace::Part => 2.0,
            };
            let k = ui.ctx().animate_value_with_time(ui.make_persistent_id("ws_knob"), target, 0.18);
            let knob = Rect::from_min_size(Pos2::new(track.left() + 2.0 + k * seg_w, track.top() + 2.0), Vec2::new(seg_w - 4.0, track.height() - 4.0));
            ui.painter().rect_filled(knob, CornerRadius::same(6), th().accent);
            for (idx, (ws, label)) in [(crate::Workspace::Jobs, "Jobs"), (crate::Workspace::Nesting, "Nesting"), (crate::Workspace::Part, "Part")]
                .into_iter()
                .enumerate()
            {
                let r = Rect::from_min_size(Pos2::new(track.left() + idx as f32 * seg_w, track.top()), Vec2::new(seg_w, track.height()));
                let resp = ui.interact(r, ui.make_persistent_id(("ws", idx)), Sense::click());
                let on = self.workspace == ws;
                let col = if on { Color32::WHITE } else if resp.hovered() { th().text_strong } else { th().text_dim };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), col);
                if resp.clicked() && !on {
                    self.store_job();
                    if ws == crate::Workspace::Part {
                        if let Some(i) = self.sel_part.filter(|&i| i < self.parts.len()).or((!self.parts.is_empty()).then_some(0)) {
                            self.open_part(i);
                        }
                    } else {
                        self.workspace = ws;
                        // The Drawing-only tabs don't exist outside an opened part.
                        if matches!(self.tab, Tab::Notes | Tab::Utilities) {
                            self.tab = Tab::Home;
                        }
                    }
                }
            }
            ui.add_space(10.0);
            // An opened part has the Drawing module's own tabs (Ui\Drawing.xml).
            let tabs: &[(Tab, &str)] = if self.workspace == crate::Workspace::Part {
                let all: &[(Tab, &str)] = &[(Tab::Home, "Home"), (Tab::Technology, "Technology"), (Tab::Machining, "Machining"), (Tab::View, "View"), (Tab::Simulation, "Simulation"), (Tab::Notes, "Notes"), (Tab::Utilities, "Utilities")];
                if crate::drawing::utility_tab_shown(&self.dv) { all } else { &all[..6] }
            } else {
                &[(Tab::Home, "Home"), (Tab::Sheet, "Sheet"), (Tab::Technology, "Technology"), (Tab::Machining, "Machining"), (Tab::View, "View"), (Tab::Simulation, "Simulation")]
            };
            for &(t, name) in tabs {
                let selected = self.tab == t;
                let col = if selected { th().text_strong } else { th().text_dim };
                let galley = ui.painter().layout_no_wrap(name.to_string(), FontId::proportional(13.0), col);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 28.0, 24.0), Sense::click());
                let h = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered() && !selected, 0.12);
                if !selected && h > 0.0 {
                    ui.painter().rect_filled(r.shrink2(Vec2::new(2.0, 2.0)), CornerRadius::same(6), with_alpha(th().hover, (h * 255.0) as u8));
                }
                // Animated underline that grows from the centre.
                let u = ui.ctx().animate_bool_with_time(resp.id.with("u"), selected, 0.18);
                if u > 0.0 {
                    let line = Rect::from_center_size(Pos2::new(r.center().x, r.bottom() - 2.0), Vec2::new((r.width() - 16.0) * u, 3.0));
                    ui.painter().rect_filled(line, CornerRadius::same(2), th().accent_fg);
                }
                ui.painter().galley(r.center() - galley.size() / 2.0 - Vec2::new(0.0, 1.0), galley, col);
                if resp.clicked() {
                    self.tab = t;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                self.theme_switch(ui);
                ui.add_space(8.0);
                let resp = ui.add(egui::Button::new("Options").selected(self.options_open));
                if resp.clicked() {
                    self.options_open = !self.options_open;
                }
                let mut go: Option<crate::settings_ui::Section> = None;
                let mut browse = false;
                egui::Popup::from_response(&resp)
                    .kind(egui::PopupKind::Menu)
                    .open_bool(&mut self.options_open)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .width(270.0)
                    .show(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if menu_row(ui, "⚙", "Settings", "Ctrl + ,") {
                            go = Some(crate::settings_ui::Section::General);
                        }
                        if menu_row(ui, "🗄", "Browse database", "") {
                            browse = true;
                        }
                        ui.separator();
                        ui.label(RichText::new("Theme").size(11.0).color(th().text_dim));
                        ui.horizontal(|ui| {
                            let cur = if self.follow_system { 2 } else if is_dark() { 1 } else { 0 };
                            for (k, name) in ["☀ Light", "🌙 Dark", "◐ System"].iter().enumerate() {
                                if ui.add(egui::Button::new(*name).selected(cur == k).min_size(Vec2::new(78.0, 26.0))).clicked() {
                                    if k == 2 {
                                        self.follow_system = true;
                                    } else {
                                        self.follow_system = false;
                                        crate::theme::set_dark(ui.ctx(), k == 1);
                                    }
                                }
                            }
                        });
                        ui.separator();
                        if menu_row(ui, "🔧", "General", "warnings, commands") {
                            go = Some(crate::settings_ui::Section::General);
                        }
                        if menu_row(ui, "⚙", "Machines & output", "") {
                            go = Some(crate::settings_ui::Section::Machines);
                        }
                        if menu_row(ui, "⌨", "Keyboard shortcuts", "") {
                            go = Some(crate::settings_ui::Section::Shortcuts);
                        }
                        if menu_row(ui, "ℹ", "About", "") {
                            go = Some(crate::settings_ui::Section::About);
                        }
                    });
                if let Some(s) = go {
                    self.options_open = false;
                    self.open_settings(s);
                }
                if browse {
                    self.options_open = false;
                    self.open_db_browser();
                }
            });
        });
    }

    /// Sliding sun / moon switch between light and dark mode.
    fn theme_switch(&mut self, ui: &mut egui::Ui) {
        let t = th();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(58.0, 28.0), Sense::click());
        let dark = is_dark();
        let k = ui.ctx().animate_bool_with_time(resp.id.with("knob"), dark, 0.2);
        let hover = resp.hovered();
        let p = ui.painter();
        // Track: a pill, slightly tinted on hover.
        let track = if hover { lerp_color(t.hover, t.accent, 0.12) } else { t.hover };
        p.rect_filled(r, CornerRadius::same(14), track);
        p.rect_stroke(r, CornerRadius::same(14), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        // Two slots; the round knob slides from the sun (light) to the moon (dark).
        let (ls, rs) = (Pos2::new(r.left() + 14.0, r.center().y), Pos2::new(r.right() - 14.0, r.center().y));
        let kc = ls + (rs - ls) * k;
        p.circle_filled(kc + Vec2::new(0.0, 1.0), 11.5, Color32::from_black_alpha(t.shadow.saturating_add(20)));
        p.circle_filled(kc, 11.0, t.card);
        p.circle_stroke(kc, 11.0, Stroke::new(1.0, with_alpha(t.accent, 90)));
        let sun_col = if dark { t.text_dim } else { Color32::from_rgb(245, 158, 11) };
        let moon_col = if dark { t.accent_fg } else { t.text_dim };
        // Sun: a small disc and eight short rays.
        let w = Stroke::new(1.5, sun_col);
        p.circle_filled(ls, 3.0, sun_col);
        for i in 0..8 {
            let d = Vec2::angled(i as f32 / 8.0 * std::f32::consts::TAU);
            p.line_segment([ls + d * 5.0, ls + d * 7.0], w);
        }
        // Moon: a crescent, filled when it's the active mode.
        if dark {
            p.circle_filled(rs, 6.0, moon_col);
            // The bite takes the colour of whatever is under it (knob or track).
            let under = lerp_color(track, t.card, ((k - 0.5) * 2.0).clamp(0.0, 1.0));
            p.circle_filled(rs + Vec2::new(2.9, -2.4), 5.1, under);
        } else {
            p.add(EShape::closed_line(crescent(rs, 6.0), Stroke::new(1.4, moon_col)));
        }
        if resp.on_hover_text(if dark { "Switch to light mode" } else { "Switch to dark mode" }).clicked() {
            crate::theme::set_dark(ui.ctx(), !is_dark());
            // A choice made by hand stops following Windows.
            self.follow_system = false;
        }
    }

    pub fn ribbon(&mut self, ui: &mut egui::Ui) {
        // A full ribbon wider than the window scrolls sideways instead of
        // squeezing its groups (which would wrap their labels into tall columns).
        egui::ScrollArea::horizontal()
            .id_salt(("ribbon", self.tab as u8))
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    match self.tab {
                        Tab::Home if self.workspace == crate::Workspace::Jobs => self.ribbon_jobs(ui),
                        _ if self.workspace == crate::Workspace::Part => self.ribbon_drawing(ui),
                        Tab::Home => self.ribbon_home(ui),
                        Tab::Sheet => self.ribbon_technology(ui),
                        Tab::Technology => self.ribbon_part_tech(ui),
                        Tab::View => self.ribbon_view(ui),
                        Tab::Machining if self.workspace == crate::Workspace::Nesting => self.ribbon_nest_machining(ui),
                        Tab::Simulation if self.workspace == crate::Workspace::Nesting => self.ribbon_nest_sim(ui),
                        _ => self.ribbon_home(ui),
                    }
                });
            });
    }

    fn ribbon_home(&mut self, ui: &mut egui::Ui) {
        let has_sheets = !self.result.sheets.is_empty();
        let n = self.result.sheets.len();
        ribbon_group(ui, "Sheets", |ui| {
            if ribbon_button(ui, Icon::New, "New\njob", false, true).clicked() {
                self.new_job();
            }
            if ribbon_button(ui, Icon::Prev, "Previous\nsheet", false, self.cur_sheet > 0).clicked() {
                self.select_sheet(self.cur_sheet - 1);
            }
            if ribbon_button(ui, Icon::Next, "Next\nsheet", false, self.cur_sheet + 1 < n).clicked() {
                self.select_sheet(self.cur_sheet + 1);
            }
            if ribbon_button(ui, Icon::Grid, "Sheet\nstock", self.stockui.open, true).on_hover_text("Pick this sheet from stock, or manage the stock").clicked() {
                self.stockui.open = !self.stockui.open;
                self.stockui.loaded = false;
            }
        });
        ribbon_group(ui, "Insert", |ui| {
            if ribbon_button_ex(ui, Icon::NestAll, "Nest\nall", false, !self.parts.is_empty(), true).on_hover_text("Place every part (F5)").clicked() {
                self.run_nesting();
            }
            let r = ribbon_button(ui, Icon::AddPart, "Add\npart", false, true);
            add_part_menu(self, &r);
            let can_open = self.sel_part.is_some_and(|i| i < self.parts.len());
            if ribbon_button(ui, Icon::LeadIn, "Open\npart", false, can_open)
                .on_hover_text("Open the selected part on its own to apply technology (or double-click a part)")
                .clicked()
            {
                if let Some(i) = self.sel_part {
                    self.open_part(i);
                }
            }
            if ribbon_button(ui, Icon::Import, "Import\nDXF", false, true)
                .on_hover_text("Add part drawings from DXF files (or drop them on the window)")
                .clicked()
            {
                self.import_dialog();
            }
        });
        ribbon_group(ui, "Edit", |ui| {
            if ribbon_button(ui, Icon::Undo, "Undo", false, !self.undo.is_empty()).on_hover_text("Ctrl+Z").clicked() {
                self.undo();
            }
            if ribbon_button(ui, Icon::Redo, "Redo", false, !self.redo.is_empty()).on_hover_text("Ctrl+Y").clicked() {
                self.redo();
            }
        });
        let has = self.sel_placed.is_some();
        ribbon_group(ui, "Modify", |ui| {
            if ribbon_button(ui, Icon::Collision, "Collision", self.collision, true)
                .on_hover_text("Stop moved parts from overlapping")
                .clicked()
            {
                self.collision = !self.collision;
            }
            if ribbon_button(ui, Icon::Copy, "Copy\npart", false, has).on_hover_text("Ctrl+D — place a copy in the first free spot").clicked() {
                self.duplicate_selected();
            }
            if ribbon_button(ui, Icon::Rotate, "Rotate\n90°", false, has).clicked() {
                self.rotate_selected();
            }
            if ribbon_button(ui, Icon::Remove, "Remove\npart", false, has).clicked() {
                self.remove_selected();
            }
            if ribbon_button(ui, Icon::Clear, "Clear\nsheets", false, has_sheets).clicked() {
                self.clear();
            }
        });
        ribbon_group(ui, "Remnants", |ui| {
            if ribbon_button(ui, Icon::Compact, "Compact\nto left", false, has_sheets)
                .on_hover_text("Slide parts left and down so one large remnant is left on the right")
                .clicked()
            {
                self.compact();
            }
            if ribbon_button(ui, Icon::CutPath, "Remnant
line", false, has_sheets)
                .on_hover_text("Cut the leftover sheet off as a remnant (more tools on the Sheet tab)")
                .clicked()
            {
                self.auto_remnant_edge();
            }
        });
        ribbon_group(ui, "Review", |ui| {
            if ribbon_button(ui, Icon::Times, "Times and\ncosts", self.show_times, true).clicked() {
                self.show_times = !self.show_times;
            }
            if ribbon_button(ui, Icon::CutPath, "Cutting\npath", self.show_path, true).clicked() {
                self.show_path = !self.show_path;
            }
            if ribbon_button(ui, Icon::Program, "Machine\nprogram", self.show_program, true).clicked() {
                self.show_program = !self.show_program;
            }
            if ribbon_button(ui, Icon::Export, "Export\ndata", false, has_sheets)
                .on_hover_text("Save every sheet's machine program and a parts report")
                .clicked()
            {
                self.export();
            }
        });
    }

    fn ribbon_technology(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut cost_changed = false;
        let spec_before = self.spec.clone();
        ribbon_group(ui, "Material", |ui| {
            egui::Grid::new("mat_grid").num_columns(2).spacing([6.0, 8.0]).show(ui, |ui| {
                ui.label("Material");
                egui::ComboBox::from_id_salt("mat").width(130.0).selected_text(cnc::MATERIALS[self.spec.material]).show_ui(ui, |ui| {
                    for (k, m) in cnc::MATERIALS.iter().enumerate() {
                        cost_changed |= ui.selectable_value(&mut self.spec.material, k, *m).changed();
                    }
                });
                ui.end_row();
                ui.label("Thickness");
                egui::ComboBox::from_id_salt("thk").width(130.0).selected_text(format!("{} mm", self.spec.thickness)).show_ui(ui, |ui| {
                    for t in cnc::THICKNESSES {
                        cost_changed |= ui.selectable_value(&mut self.spec.thickness, t, format!("{t} mm")).changed();
                    }
                });
                ui.end_row();
            });
        });
        ribbon_group(ui, "Machine", |ui| {
            if self.machine_picker(ui) {
                self.refresh();
            }
        });
        ribbon_group(ui, "Sheet", |ui| {
            egui::Grid::new("sheet_grid").num_columns(4).spacing([6.0, 8.0]).show(ui, |ui| {
                ui.label("Size");
                let cur = format!("{:.0} × {:.0}", self.spec.w, self.spec.h);
                egui::ComboBox::from_id_salt("sheet").width(110.0).selected_text(cur).show_ui(ui, |ui| {
                    for (w, h) in [(2000.0, 1000.0), (2500.0, 1250.0), (3000.0, 1500.0), (4000.0, 2000.0), (6000.0, 2000.0)] {
                        if ui.selectable_label(self.spec.w == w && self.spec.h == h, format!("{w:.0} × {h:.0}")).clicked() {
                            self.spec.w = w;
                            self.spec.h = h;
                            changed = true;
                        }
                    }
                });
                ui.label("Length X");
                changed |= ui.add(egui::DragValue::new(&mut self.spec.w).range(200.0..=12000.0).suffix(" mm")).changed();
                ui.end_row();
                ui.label("");
                ui.label("");
                ui.label("Width Y");
                changed |= ui.add(egui::DragValue::new(&mut self.spec.h).range(200.0..=3000.0).suffix(" mm")).changed();
                ui.end_row();
            });
        });
        ribbon_group(ui, "Spacing", |ui| {
            egui::Grid::new("space_grid").num_columns(2).spacing([6.0, 8.0]).show(ui, |ui| {
                ui.label("Edge margin");
                changed |= ui.add(egui::DragValue::new(&mut self.spec.margin).range(0.0..=100.0).speed(0.5).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Part gap");
                changed |= ui.add(egui::DragValue::new(&mut self.spec.gap).range(0.0..=50.0).speed(0.2).suffix(" mm")).changed();
                ui.end_row();
            });
        });
        ribbon_group(ui, "Nest", |ui| {
            if ribbon_button(ui, Icon::NestAll, "Nest\nall", false, !self.parts.is_empty()).clicked() {
                self.run_nesting();
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Automatic level").size(11.0).color(th().text_dim));
                let cur = self.spec.nest_level().name;
                egui::ComboBox::from_id_salt("nest_level").width(150.0).selected_text(cur).show_ui(ui, |ui| {
                    for (k, lv) in crate::nest::LEVELS.iter().enumerate() {
                        let tip = level_summary(lv);
                        if ui.selectable_label(self.spec.level as usize == k + 1, lv.name).on_hover_text(tip).clicked() {
                            self.spec.level = k as u8 + 1;
                            changed = true;
                        }
                    }
                });
                let ng = crate::nest::part_groups(&self.parts).len();
                if ui.checkbox(&mut self.group_nest, RichText::new("Group by attribute").size(11.0))
                    .on_hover_text(format!("Nest each group of parts (Part Properties → Group: customer, order, date…) in its own zone of the sheet, one after another. {ng} group(s) now."))
                    .changed()
                {
                    changed = true;
                }
                ui.label(RichText::new(format!("{} methods tried", self.spec.nest_level().methods)).size(11.0).color(th().text_dim))
                    .on_hover_text(if self.result.method.is_empty() { level_summary(self.spec.nest_level()) } else { format!("{}

Last run won with: {}.", level_summary(self.spec.nest_level()), self.result.method) });
            });
        });
        self.ribbon_cutoff(ui);
        // Material or thickness changed while parts are nested: ask first
        // (T4: keep nestings / remove them / cancel).
        let mat_changed = self.spec.material != spec_before.material || self.spec.thickness != spec_before.thickness;
        if mat_changed && !self.result.sheets.is_empty() {
            let new_spec = self.spec.clone();
            self.spec = spec_before;
            if self.pending.is_none() {
                self.pending = Some(crate::Pending::NestedSpecChange { new_spec });
            }
            return;
        }
        if changed {
            self.stale = true;
        }
        if changed || cost_changed {
            self.refresh();
        }
    }

    fn ribbon_view(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Appearance", |ui| {
            let dark = is_dark();
            if ribbon_button(ui, Icon::Sun, "Light\nmode", !dark, true).clicked() {
                crate::theme::set_dark(ui.ctx(), false);
            }
            if ribbon_button(ui, Icon::Moon, "Dark\nmode", dark, true).clicked() {
                crate::theme::set_dark(ui.ctx(), true);
            }
        });
        ribbon_group(ui, "Show", |ui| {
            if ribbon_button(ui, Icon::CutPath, "Cutting\npath", self.show_path, true).clicked() {
                self.show_path = !self.show_path;
            }
            if ribbon_button(ui, Icon::Numbers, "Part\nnumbers", self.show_numbers, true).clicked() {
                self.show_numbers = !self.show_numbers;
            }
            if ribbon_button(ui, Icon::Grid, "Grid\n100 mm", self.show_grid, true).clicked() {
                self.show_grid = !self.show_grid;
            }
        });
        ribbon_group(ui, "Panes", |ui| {
            if ribbon_button(ui, Icon::Program, "Machine\nprogram", self.show_program, true).clicked() {
                self.show_program = !self.show_program;
            }
            if ribbon_button(ui, Icon::Times, "Times and\ncosts", self.show_times, true).clicked() {
                self.show_times = !self.show_times;
            }
            let resp = ribbon_button(ui, Icon::Grid, "Side\npanels", false, true)
                .on_hover_text("Show or hide side panels. Drag a panel's title to move it: drop it in another place in its column, on the left or right edge to dock it there, or anywhere else to float it.");
            egui::Popup::menu(&resp).show(|ui| {
                ui.label(RichText::new("Side panels").strong().color(th().text_strong));
                self.panels_menu(ui);
            });
        });
        ribbon_group(ui, "Zoom", |ui| {
            if ribbon_button(ui, Icon::Fit, "Fit\nsheet", false, true).clicked() {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
        });
    }

    pub fn nest_tree_body(&mut self, ui: &mut egui::Ui) {
        let mut pick_sheet = None;
        let mut pick_part = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            egui::CollapsingHeader::new(RichText::new(&self.job_name).strong())
                .id_salt("job_node")
                .default_open(true)
                .show(ui, |ui| {
                    if self.result.sheets.is_empty() {
                        ui.label(RichText::new("No sheets — press Nest all").weak());
                    }
                    for (i, s) in self.result.sheets.iter().enumerate() {
                        let title = format!("{}W {}", i + 1, self.sheet_label(i, 3));
                        let txt = if i == self.cur_sheet {
                            RichText::new(title).background_color(th().accent_soft)
                        } else {
                            RichText::new(title)
                        };
                        let resp = egui::CollapsingHeader::new(txt).id_salt(("sheet_node", i)).default_open(i == 0).show(ui, |ui| {
                            for (pi, part) in self.parts.iter().enumerate() {
                                let count = s.iter().filter(|p| p.part == pi).count();
                                if count == 0 {
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                                    ui.painter().rect_filled(r.shrink(1.0), CornerRadius::ZERO, part_color(part.color));
                                    let (r2, _) = ui.allocate_exact_size(Vec2::new(16.0, 14.0), Sense::hover());
                                    check_mark(ui.painter(), r2.center(), 10.0);
                                    if ui.selectable_label(self.sel_part == Some(pi), format!("{}, {}", part.name, count)).clicked() {
                                        pick_part = Some(pi);
                                        pick_sheet = Some(i);
                                    }
                                });
                            }
                        });
                        if resp.header_response.clicked() {
                            pick_sheet = Some(i);
                        }
                    }
                });
        });
        if let Some(i) = pick_sheet {
            self.select_sheet(i);
        }
        if let Some(p) = pick_part {
            self.sel_part = Some(p);
        }
    }

    pub fn props_body(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 0)).show(ui, |ui| self.part_editor(ui));
    }

    pub(crate) fn part_editor(&mut self, ui: &mut egui::Ui) {
        let Some(i) = self.sel_part.filter(|&i| i < self.parts.len()) else {
            ui.label(RichText::new("Select a part in the tree or in Parts View.").weak());
            return;
        };
        ui.horizontal(|ui| {
            if ui.small_button("📚 Save to library").on_hover_text("Keep this part — shape, technology and texts — in the library, ready to add to any job").clicked() {
                self.save_part_to_library(i);
            }
            if self.parts[i].tech.is_some() {
                chip(ui, "own technology", th().accent_soft, th().accent_fg);
            }
        });
        ui.add_space(2.0);
        let before = self.snapshot();
        let now = ui.input(|inp| inp.time);
        let mut changed = false;
        // Edit a draft so a geometry change on a part with technology can be held
        // back and questioned first (T1–T3).
        let mut draft = self.parts[i].clone();
        egui::Grid::new("part_edit").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
            let p = &mut draft;
            ui.label("Reference");
            changed |= ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(110.0)).changed();
            ui.end_row();
            ui.label("Group").on_hover_text("Customer, order, delivery date… — with Group by attribute on, each group is nested in its own zone");
            changed |= ui.add(egui::TextEdit::singleline(&mut p.group).desired_width(110.0).hint_text("none")).changed();
            ui.end_row();
            ui.label("Shape");
            egui::ComboBox::from_id_salt("shape_combo").width(110.0).selected_text(p.shape.label()).show_ui(ui, |ui| {
                for sh in Shape::ALL {
                    changed |= ui.selectable_value(&mut p.shape, sh, sh.label()).changed();
                }
            });
            ui.end_row();
            if p.shape == Shape::Custom {
                // Size comes from the drawing; it can't be stretched here.
                ui.label("Size");
                ui.label(RichText::new(format!("{:.1} × {:.1} mm", p.w, p.h)).strong());
                ui.end_row();
                ui.label("");
                ui.label(RichText::new("from the drawing").size(11.0).color(th().text_dim));
                ui.end_row();
            } else if p.shape == Shape::Disc {
                ui.label("Diameter");
                if ui.add(egui::DragValue::new(&mut p.w).range(10.0..=3000.0).speed(1.0).suffix(" mm")).changed() {
                    p.h = p.w;
                    changed = true;
                }
                ui.end_row();
            } else {
                ui.label("Width");
                changed |= ui.add(egui::DragValue::new(&mut p.w).range(10.0..=6000.0).speed(1.0).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Height");
                changed |= ui.add(egui::DragValue::new(&mut p.h).range(10.0..=3000.0).speed(1.0).suffix(" mm")).changed();
                ui.end_row();
            }
            ui.label("Quantity");
            changed |= ui.add(egui::DragValue::new(&mut p.qty).range(0..=999)).changed();
            ui.end_row();
            ui.label("Rotation");
            changed |= ui.checkbox(&mut p.rotate, "may turn 90°").changed();
            ui.end_row();
            // The original's eight rotation permissions: 0/90/180/270°, plain and mirrored.
            ui.label("Angles");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for (k, a) in ["0", "90", "180", "270"].iter().enumerate() {
                    let enabled = k % 2 == 0 || p.rotate;
                    let r = ui.add_enabled(enabled, egui::Button::selectable(p.rots.plain[k], format!("{a}°")));
                    if r.on_hover_text("Allowed rotation when nesting").clicked() {
                        p.rots.plain[k] = !p.rots.plain[k];
                        if !p.rots.plain.iter().any(|&b| b) {
                            p.rots.plain[0] = true; // at least one way to place it
                        }
                        changed = true;
                    }
                }
            });
            ui.end_row();
            ui.label("Mirror");
            ui.horizontal(|ui| {
                let mut m = p.rots.mirrored.iter().any(|&b| b);
                if ui.checkbox(&mut m, "may be flipped").on_hover_text("Lets the nester flip the part (mirrored pairs fit some shapes tighter)").changed() {
                    p.rots.mirrored = if m { p.rots.plain } else { [false; 4] };
                    changed = true;
                }
            });
            ui.end_row();
        });

        // ---- Part technology ----
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Technology").strong().color(th().text_strong));
            let mut own = draft.tech.is_some();
            if ui.checkbox(&mut own, "part's own").on_hover_text("Off = the sheet's automatic technology").changed() {
                draft.tech = if own { Some(crate::nest::PartTech::default()) } else { None };
                changed = true;
            }
        });
        if let Some(t) = draft.tech.as_mut() {
            egui::Grid::new("tech_edit").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Lead-in");
                changed |= ui.add(egui::DragValue::new(&mut t.lead_in).range(0.0..=30.0).speed(0.1).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Lead-out");
                changed |= ui.add(egui::DragValue::new(&mut t.lead_out).range(0.0..=20.0).speed(0.1).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Overlap").on_hover_text("Cut on past the start before the lead-out, so the contour closes cleanly");
                changed |= ui.add(egui::DragValue::new(&mut t.overlap).range(0.0..=10.0).speed(0.05).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Micro-joints");
                changed |= ui.add(egui::DragValue::new(&mut t.micro_joints).range(0..=12)).changed();
                ui.end_row();
                ui.label("Joint width");
                changed |= ui.add(egui::DragValue::new(&mut t.joint_width).range(0.1..=5.0).speed(0.05).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Cut quality");
                egui::ComboBox::from_id_salt("quality").width(110.0).selected_text(t.quality.label()).show_ui(ui, |ui| {
                    for q in crate::nest::CutQuality::ALL {
                        changed |= ui.selectable_value(&mut t.quality, q, q.label()).changed();
                    }
                });
                ui.end_row();
            });
        } else {
            ui.label(RichText::new("Automatic: 5 mm lead-in, no micro-joints.").size(11.5).color(th().text_dim));
        }

        if changed {
            let old = &self.parts[i];
            let geometry_changed = old.shape != draft.shape || old.w != draft.w || old.h != draft.h;
            if geometry_changed && old.tech.is_some() && self.warn_tech {
                // Part with technology: warn and ask what to do, or reuse the answer
                // given "for all" earlier (original: warn, then ask; T1–T2).
                match self.tech_answer_all {
                    Some(choice) => self.apply_tech_choice(i, draft.clone(), choice),
                    None if self.pending.is_none() => {
                        self.pending = Some(crate::Pending::TechPartEdit { idx: i, draft: draft.clone() });
                    }
                    None => {}
                }
                changed = false;
            } else {
                self.parts[i] = draft;
            }
        }
        let p = &self.parts[i];
        let area = p.net_area();
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!(
                "Area {:.0} cm² · cut {:.0} mm · {} pierce(s) · {:.2} kg",
                area / 100.0,
                p.cut_length(),
                p.pierces(),
                cnc::weight_kg(self.spec.material, area, self.spec.thickness)
            ))
            .size(11.5),
        );
        let remove = ui
            .horizontal(|ui| {
                let r = ui.button("🗑 Delete part").clicked();
                let e = ui.button("Export DXF").on_hover_text("Save this part as a DXF drawing (with a .txt note beside it)").clicked();
                let o = self.workspace != crate::Workspace::Part
                    && ui.button("Open part").on_hover_text("Open this part on its own to apply technology").clicked();
                (r, e, o)
            })
            .inner;
        let (remove, export, open) = (remove.0, remove.1, remove.2);
        let _ = export;
        if export {
            self.export_part_dxf(i);
        }
        if open {
            self.open_part(i);
            return;
        }
        if changed {
            // Rapid edits (dragging a number) collapse into one undo step.
            self.push_undo_coalesced(before, now);
            self.stale = true;
            self.refresh();
        }
        if remove {
            self.remove_part(i);
        }
    }

    pub fn parts_view(&mut self, ui: &mut egui::Ui) {
        pane_header(ui, if self.bottom == BottomTab::PartsView { "Parts View" } else { "Parts List" });
        ui.add_space(3.0);
        match self.bottom {
            BottomTab::PartsView => self.part_tiles(ui),
            BottomTab::PartsList => self.parts_table(ui),
        }
    }

    /// Search box and summary chips, shown in the bottom tab strip.
    pub fn parts_search_bar(&mut self, ui: &mut egui::Ui) {
        {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("🔍 Search parts…")
                    .desired_width(200.0)
                    .margin(egui::Margin::symmetric(8, 3)),
            );
            if !self.search.is_empty() && ui.small_button("✖").on_hover_text("Clear search").clicked() {
                self.search.clear();
            }
            let need: u32 = self.parts.iter().map(|p| p.qty).sum();
            let placed = self.total_placed() as u32;
            ui.add_space(8.0);
            chip(ui, &format!("{} types", self.parts.len()), th().hover, th().text);
            chip(ui, &format!("{} pieces", need), th().hover, th().text);
            if placed >= need && need > 0 {
                chip(ui, "✔ all nested", th().ok_bg, th().ok_fg);
            } else {
                chip(ui, &format!("{} pending", need.saturating_sub(placed)), th().warn_bg, th().warn_fg);
            }
        }
    }

    fn part_tiles(&mut self, ui: &mut egui::Ui) {
        let mut pick = None;
        let mut add_resp = None;
        let mut qty_change: Option<(usize, i32)> = None;
        let mut open_tile: Option<usize> = None;
        let needle = self.search.to_lowercase();
        egui::ScrollArea::horizontal().id_salt("tiles").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                for (i, part) in self.parts.iter().enumerate() {
                    if !needle.is_empty() && !part.name.to_lowercase().contains(&needle) && !part.shape.label().to_lowercase().contains(&needle) {
                        continue;
                    }
                    let nested = self.placed_of(i) as u32;
                    let pending = part.qty.saturating_sub(nested);
                    let (slot, resp) = ui.allocate_exact_size(Vec2::new(86.0, 92.0), Sense::click_and_drag());
                    let hover = ui.ctx().animate_bool_with_time(resp.id.with("lift"), resp.hovered() || ui.rect_contains_pointer(slot), 0.12);
                    let r = Rect::from_min_size(slot.min + Vec2::new(0.0, 4.0 - 3.0 * hover), Vec2::new(86.0, 86.0));
                    let p = ui.painter();
                    let round = CornerRadius::same(8);
                    p.rect_filled(r.translate(Vec2::new(0.0, 2.0 + 2.0 * hover)), round, Color32::from_black_alpha((th().shadow as f32 * (0.6 + 0.8 * hover)) as u8));
                    p.rect_filled(r, round, th().card);
                    let head = Rect::from_min_size(r.min, Vec2::new(r.width(), 20.0));
                    p.rect_filled(head, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, if pending == 0 { th().ok_bg } else { th().warn_bg });
                    p.text(
                        head.left_center() + Vec2::new(4.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("{} | {}", pending, part.qty),
                        FontId::proportional(12.5),
                        if pending == 0 { th().ok_fg } else { th().warn_fg },
                    );
                    if pending == 0 {
                        check_mark(p, head.right_center() - Vec2::new(10.0, 0.0), 12.0);
                    }
                    if part.tech.is_some() {
                        // "T" badge: the part has its own technology.
                        let b = Rect::from_min_size(Pos2::new(r.right() - 22.0, head.bottom() + 4.0), Vec2::new(18.0, 16.0));
                        p.rect_filled(b, CornerRadius::same(4), th().accent);
                        p.text(b.center(), Align2::CENTER_CENTER, "T", FontId::proportional(11.0), Color32::WHITE);
                    }
                    let body = Rect::from_min_max(head.left_bottom() + Vec2::new(8.0, 6.0), r.max - Vec2::new(8.0, 6.0));
                    let (w, h) = part.bbox();
                    let s = (body.width() / w as f32).min(body.height() / h as f32);
                    let o = body.center() + Vec2::new(-(w as f32) * s / 2.0, h as f32 * s / 2.0);
                    paint_part(
                        p,
                        part,
                        |q| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s),
                        part_color(part.color),
                        Stroke::new(1.0, darker(part_color(part.color), 0.4)),
                        th().card,
                    );
                    let sel = self.sel_part == Some(i);
                    let dropping = self.ghost.is_some_and(|g| g.part == i);
                    p.rect_stroke(
                        r,
                        round,
                        if sel || dropping { Stroke::new(2.0, th().accent) } else { Stroke::new(1.0, th().border) },
                        egui::StrokeKind::Inside,
                    );
                    if dropping {
                        // "Dropping" pill: this part's ghost is on the sheet.
                        let b = Rect::from_center_size(Pos2::new(r.center().x, r.bottom() - 9.0), Vec2::new(60.0, 15.0));
                        p.rect_filled(b, CornerRadius::same(7), th().accent);
                        p.text(b.center(), Align2::CENTER_CENTER, "Dropping", FontId::proportional(10.5), Color32::WHITE);
                    }
                    let name = part.name.clone();
                    let resp = resp.on_hover_text(format!("{} — {} pcs, {:.0}×{:.0} mm · click or drag onto the sheet to place · double-click to open", name, part.qty, w, h));
                    if resp.clicked() {
                        pick = Some((i, false));
                    }
                    if resp.drag_started_by(egui::PointerButton::Primary) {
                        pick = Some((i, true));
                    }
                    if resp.double_clicked() {
                        open_tile = Some(i);
                    }
                    // Round −/+ quantity steppers appear on hover.
                    if hover > 0.05 {
                        let a = (hover * 255.0) as u8;
                        for (k, (label, delta)) in [("−", -1), ("+", 1)].into_iter().enumerate() {
                            let c = Pos2::new(if k == 0 { r.left() + 13.0 } else { r.right() - 13.0 }, r.bottom() - 13.0);
                            let br = Rect::from_center_size(c, Vec2::splat(20.0));
                            let bresp = ui.interact(br, ui.scope_id().with(("qty", i, k)), Sense::click());
                            let fill = if bresp.hovered() { th().accent } else { with_alpha(th().accent, (a as f32 * 0.85) as u8) };
                            ui.painter().circle_filled(c, 10.0, fill);
                            ui.painter().text(c, Align2::CENTER_CENTER, label, FontId::proportional(15.0), Color32::from_rgba_unmultiplied(255, 255, 255, a));
                            if bresp.on_hover_text(if delta < 0 { "One less" } else { "One more" }).clicked() {
                                qty_change = Some((i, delta));
                            }
                        }
                    }
                }
                let (slot, resp) = ui.allocate_exact_size(Vec2::new(64.0, 92.0), Sense::click());
                let r = Rect::from_min_size(slot.min + Vec2::new(0.0, 4.0), Vec2::new(64.0, 86.0));
                let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
                let g = lerp_color(th().text_dim, th().accent_fg, hov);
                ui.painter().rect_filled(r, CornerRadius::same(8), with_alpha(th().accent, (hov * 24.0) as u8));
                ui.painter().extend(EShape::dashed_line(
                    &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()],
                    Stroke::new(1.2, g),
                    5.0,
                    3.0,
                ));
                ui.painter().text(r.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "+", FontId::proportional(28.0), g);
                ui.painter().text(r.center() + Vec2::new(0.0, 18.0), Align2::CENTER_CENTER, "New part", FontId::proportional(11.0), g);
                add_resp = Some(resp.on_hover_text("Add a part"));
            });
        });
        if let Some(r) = add_resp {
            add_part_menu(self, &r);
        }
        if let Some((i, dragging)) = pick {
            self.sel_part = Some(i);
            // Clicking the tile that is already dropping puts it away again.
            if !dragging && self.ghost.is_some_and(|g| g.part == i && !g.dragging) {
                self.stop_ghost();
            } else if self.workspace == crate::Workspace::Nesting {
                self.start_ghost(i, dragging);
            }
        }
        if let Some(i) = open_tile {
            self.ghost = None;
            self.open_part(i);
        }
        if let Some((i, d)) = qty_change {
            let q = self.parts[i].qty as i32 + d;
            if q >= 0 {
                self.checkpoint();
                self.parts[i].qty = q as u32;
                self.sel_part = Some(i);
                self.stale = true;
            }
        }
    }

    fn parts_table(&mut self, ui: &mut egui::Ui) {
        let mut clicked = None;
        egui::ScrollArea::vertical().id_salt("parts_scroll").show(ui, |ui| {
            egui::Grid::new("parts_grid").striped(true).num_columns(8).spacing([18.0, 3.0]).show(ui, |ui| {
                for h in ["", "Reference", "Shape", "Size mm", "Quantity", "Nested", "Pending", "Rotation"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                let needle = self.search.to_lowercase();
                for (i, p) in self.parts.iter().enumerate() {
                    if !needle.is_empty() && !p.name.to_lowercase().contains(&needle) && !p.shape.label().to_lowercase().contains(&needle) {
                        continue;
                    }
                    let (r, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::ZERO, part_color(p.color));
                    if ui.selectable_label(self.sel_part == Some(i), &p.name).clicked() {
                        clicked = Some(i);
                    }
                    ui.label(p.shape.label());
                    let (w, h) = p.bbox();
                    ui.label(format!("{:.0} × {:.0}", w, h));
                    ui.label(format!("{}", p.qty));
                    let n = self.placed_of(i) as u32;
                    ui.label(format!("{n}"));
                    let pend = p.qty.saturating_sub(n);
                    let col = if pend == 0 { th().ok_fg } else { th().warn_fg };
                    ui.label(RichText::new(format!("{pend}")).color(col));
                    ui.label(if p.rotate { "90°" } else { "none" });
                    ui.end_row();
                }
            });
        });
        if let Some(i) = clicked {
            self.sel_part = Some(i);
        }
    }

    pub fn results(&mut self, ui: &mut egui::Ui) {
        let sheets = self.result.sheets.len();
        let parts_area: f64 = self.result.sheets.iter().flatten().map(|p| self.parts[p.part].net_area()).sum();
        let util = if sheets > 0 { parts_area / self.total_sheet_area().max(1.0) * 100.0 } else { 0.0 };
        let cut: f64 = self.plans.iter().map(|p| p.cut_len).sum();
        let secs: f64 = self.plans.iter().map(|p| p.seconds).sum();
        let pierces: usize = self.plans.iter().map(|p| p.pierces).sum();
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        let placed = self.total_placed();
        let (m, t) = (self.spec.material, self.spec.thickness);
        ui.label(RichText::new("Whole job").strong());
        egui::Grid::new("res_grid").num_columns(2).spacing([16.0, 3.0]).striped(true).show(ui, |ui| {
            row(ui, "Sheets", format!("{sheets}"));
            row(ui, "Parts nested", format!("{placed} / {need}"));
            row(ui, "Usage", format!("{util:.1} %"));
            row(ui, "Cut length", format!("{:.1} m", cut / 1000.0));
            row(ui, "Pierces", format!("{pierces}"));
            row(ui, "Cutting time", fmt_time(secs));
            row(ui, "Sheet weight", format!("{:.1} kg", cnc::weight_kg(m, self.total_sheet_area(), t)));
            row(ui, "Parts weight", format!("{:.1} kg", cnc::weight_kg(m, parts_area, t)));
        });
        if let Some(pl) = self.plans.get(self.cur_sheet) {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Sheet {} of {}", self.cur_sheet + 1, sheets)).strong());
            let (u, _, _) = self.sheet_usage(self.cur_sheet);
            let count = self.result.sheets[self.cur_sheet].len();
            egui::Grid::new("cur_grid").num_columns(2).spacing([16.0, 3.0]).striped(true).show(ui, |ui| {
                row(ui, "Parts", format!("{count}"));
                row(ui, "Usage", format!("{u:.1} %"));
                row(ui, "Cut length", format!("{:.1} m", pl.cut_len / 1000.0));
                row(ui, "Rapid moves", format!("{:.1} m", pl.rapid_len / 1000.0));
                row(ui, "Time", fmt_time(pl.seconds));
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Times use typical laser speeds (estimate).").small().weak());
    }

    pub fn program_panel(&mut self, ui: &mut egui::Ui) {
        pane_header(ui, &format!("Machine Program — sheet {}", self.cur_sheet + 1));
        ui.horizontal(|ui| {
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(self.program.clone());
                self.status = "Program copied to the clipboard.".into();
            }
            ui.label(RichText::new(format!("{} lines", self.program.lines().count())).weak());
        });
        egui::ScrollArea::vertical().id_salt("prog").auto_shrink([false, false]).show(ui, |ui| {
            let text: String = self.program.lines().take(2000).collect::<Vec<_>>().join("\n");
            ui.label(RichText::new(text).monospace().size(11.5));
        });
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (u, rem, cut_x) = self.sheet_usage(self.cur_sheet);
            let n = if self.result.sheets.is_empty() { 0 } else { self.cur_sheet + 1 };
            ui.label(format!("{n}"));
            ui.label(self.sheet_label(self.cur_sheet, 3));
            ui.add_space(8.0);
            ui.label(format!("Usage : {u:.1} %"));
            let (bar, _) = ui.allocate_exact_size(Vec2::new(70.0, 6.0), Sense::hover());
            ui.painter().rect_filled(bar, CornerRadius::same(3), th().border);
            let mut fill = bar;
            fill.set_width(bar.width() * (u as f32 / 100.0).clamp(0.0, 1.0));
            ui.painter().rect_filled(fill, CornerRadius::same(3), th().accent);
            ui.add_space(8.0);
            ui.label(format!("Remnant : {rem:.1} %"));
            ui.add_space(8.0);
            ui.label(format!("{cut_x:.3}"));
            ui.add_space(12.0);
            // The sheet's CNC program name, as the original shows it.
            ui.label(format!("CNC: {}", self.cnc_name(self.cur_sheet + 1)));
            ui.add_space(16.0);
            ui.label(RichText::new(&self.status).color(th().text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("mm");
                ui.separator();
                let (x, y) = match self.cursor_mm {
                    Some(p) => (format!("X: {:.2}", p[0]), format!("Y: {:.3}", p[1])),
                    None => ("X: —".to_string(), "Y: —".to_string()),
                };
                ui.add_sized([110.0, 16.0], egui::Label::new(y));
                ui.separator();
                ui.add_sized([110.0, 16.0], egui::Label::new(x));
                ui.separator();
            });
        });
    }
}

/// What an automatic nesting level does, in words.
pub fn level_summary(lv: &crate::nest::NestLevel) -> String {
    use crate::nest::NestDir;
    let mut s = vec![format!("Tries {} methods and keeps the best", lv.methods)];
    s.push(match lv.dir {
        NestDir::Vertical => "fills columns from the left (remnant on the right)".into(),
        NestDir::Horizontal => "fills rows from the bottom (remnant at the top)".into(),
        NestDir::Both => "tries columns and rows".into(),
    });
    if lv.regular {
        s.push("tidy arrays of identical parts".into());
    }
    if lv.modules {
        s.push(format!("pairs parts turned 180° when the pair uses ≥ {:.0}% of its box", lv.min_module_eff * 100.0));
    }
    if lv.semi_complex {
        s.push("mirrored pairs (for parts that may be flipped)".into());
    }
    if lv.staggered {
        s.push("staggered rows of discs".into());
    }
    if lv.fill_holes || crate::ct1::active().parts_in_holes {
        s.push("puts small parts in the holes of big ones".into());
    }
    if lv.corners {
        s.push("tries every corner of each free space".into());
    }
    s.join("; ") + "."
}

/// One row of a menu: icon, label, and a dim hint on the right.
pub fn menu_row(ui: &mut egui::Ui, icon: &str, label: &str, hint: &str) -> bool {
    let t = th();
    let w = ui.available_width().max(240.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(6), t.hover);
    }
    ui.painter().text(r.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), t.accent_fg);
    ui.painter().text(r.left_center() + Vec2::new(36.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(13.5), t.text_strong);
    if !hint.is_empty() {
        ui.painter().text(r.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, hint, FontId::proportional(11.0), t.text_dim);
    }
    resp.clicked()
}
