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
    /// Rotate drawn counter-clockwise.
    RotateLeft,
    Remove,
    Clear,
    Collision,
    Snap,
    Mirror,
    /// Mirror about a level axis (top↔bottom).
    MirrorTb,
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
    Import,
    LeadIn,
    Joint,
    Loop,
    CutTech,
    /// An open folder (open a saved job).
    Folder,
    /// An arrow down into a tray (save).
    Save,
    /// Book spines (the library).
    Library,
    /// Book spines with an arrow in (save to the library).
    LibrarySave,
    /// A cylinder (a database).
    Database,
    /// A plug (database connections).
    Plug,
    /// A clipboard of work orders (an ERP / MES system's jobs).
    WorkOrders,
    /// A ticket (the labor ticket).
    Tag,
    /// A tick in a circle (check / verify).
    Verify,
    /// Play and pause (simulation).
    Play,
    Pause,
    /// Step back / forward one move (simulation).
    StepBack,
    StepFwd,
    /// A letter T (texts).
    Text,
    /// A cog (settings).
    Gear,
    /// A measured length between two ticks (dimensions).
    Dimension,
    /// A cross-hair on a point.
    Points,
    /// A window with a side pane.
    Panels,
    /// A dashed box with a question mark (commands not built yet).
    Missing,
    /// A circle and a square (recognise standard shapes).
    Shapes,
    /// One sheet with two parts on it (nest this sheet).
    NestSheet,
    /// A stack of sheets (sheet stock).
    Stock,
    /// A right angle with its square mark (orthogonal mode).
    Ortho,
    /// Skip to the start / end (simulation Beginning / End).
    SkipStart,
    SkipEnd,
    /// A filled square (stop).
    Stop,
    /// A part with an arrow out of it (open the part on its own).
    OpenPart,
    /// A part with a tool path running round it (machine automatically).
    Toolpath,
    /// A contour and its offset (kerf compensation).
    Kerf,
    /// A sheet with a cut line and the hatched remnant beyond it.
    CutLine,
    /// A pointer and a numbered pick (pick the cut order by hand).
    PickOrder,
    /// A line with a mark in its middle (place on the picked element).
    OnElement,
    /// A part with a hole (its contours).
    Contours,
    /// A small window with a play mark (the simulation window).
    SimWindow,
    /// An "i" in a circle (status / information).
    Info,
    /// Book spines with a plus (add parts from the library).
    LibraryAdd,
    /// A magnifier over a part (check the drawing).
    CheckDrawing,
    /// A magnifier with a plus / minus (zoom in / out).
    ZoomIn,
    ZoomOut,
    /// Bands of growing size (cut quality by contour size).
    SizeBands,
    /// A contour with arrows both ways (reverse the cutting direction).
    Reverse,
    /// Two sheets with arrows either side (go between the sheets).
    SheetNav,
    /// Arrows in four directions (move parts).
    MoveArrows,
    /// A plate edge cut at an angle, with the angle marked (bevels).
    Bevel,
    /// A cutting head inside view corners (keep the head in view).
    FollowHead,
    /// A lead-in on a corner, taken from a small table (the machine's
    /// standard leads).
    StandardLeads,
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

/// Bend lines on the sheet and in part pictures (CS3; our own colour, the
/// same purple as the agents' pictures).
const BEND: Color32 = Color32::from_rgb(147, 51, 234);

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
        // CS3: No cut contours dotted in the part's colour, Bend lines dashed
        // in the bend colour; neither is machined.
        for shown in c.shown_only() {
            let pts: Vec<Pos2> = shown.path().into_iter().map(&map).collect();
            if shown.state == crate::dxf::ContourState::BendLine {
                painter.extend(EShape::dashed_line(&pts, Stroke::new(1.3, BEND), 7.0, 3.0));
            } else {
                painter.extend(EShape::dotted_line(&pts, darker(fill, 0.45), 3.0, 0.7));
            }
        }
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
    let (ink, accent) = if dim { (with_alpha(t.text_dim, 140), with_alpha(t.text_dim, 140)) } else { (t.text, t.accent_fg) };
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
            // A stack of sheets packed with parts, and a spark: nest everything.
            let front = Rect::from_center_size(c + Vec2::new(-2.5, 2.5), Vec2::new(18.0, 14.0));
            let back = front.translate(Vec2::new(4.0, -4.0));
            poly(vec![Pos2::new(front.left() + 4.0, front.top()), Pos2::new(back.left(), back.top()), back.right_top(), Pos2::new(back.right(), front.bottom() - 4.0)], s);
            rr(front, s);
            let fill = if mono { ink } else { accent };
            for (x, y, w, h) in [(2.5, 2.5, 5.5, 9.0), (9.5, 2.5, 6.0, 4.0), (9.5, 7.5, 6.0, 4.0)] {
                p.rect_filled(Rect::from_min_size(front.min + Vec2::new(x, y), Vec2::new(w, h)), CornerRadius::same(1), fill);
            }
            // The spark (automatic).
            let sp = v(9.0, -9.0);
            for d in [Vec2::new(0.0, 3.5), Vec2::new(3.5, 0.0)] {
                p.line_segment([sp - d, sp + d], Stroke::new(1.6, fill));
            }
        }
        Icon::AddPart => {
            rr(Rect::from_min_size(v(-10.0, -4.0), Vec2::splat(14.0)), s);
            p.line_segment([v(6.0, -9.0), v(6.0, -1.0)], a);
            p.line_segment([v(2.0, -5.0), v(10.0, -5.0)], a);
        }
        Icon::Rotate | Icon::RotateLeft => {
            // RotateLeft is Rotate mirrored left↔right.
            let fx = if icon == Icon::RotateLeft { -1.0 } else { 1.0 };
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let ang = -0.4 + i as f32 / 24.0 * 4.6;
                    c + Vec2::new(fx * ang.cos(), ang.sin()) * 8.5
                })
                .collect();
            let end = *pts.last().unwrap();
            poly(pts, s);
            poly(vec![end + Vec2::new(-4.0 * fx, -1.0), end, end + Vec2::new(0.5 * fx, -4.5)], a);
        }
        Icon::Remove => {
            p.line_segment([v(-8.0, -6.0), v(8.0, -6.0)], s);
            poly(vec![v(-3.0, -6.0), v(-3.0, -9.0), v(3.0, -9.0), v(3.0, -6.0)], s);
            rr(Rect::from_min_max(v(-6.0, -6.0), v(6.0, 9.0)), s);
            p.line_segment([v(-2.0, -2.0), v(-2.0, 5.0)], a);
            p.line_segment([v(2.0, -2.0), v(2.0, 5.0)], a);
        }
        Icon::Clear => {
            // A sheet with a waste bin: take every part off the sheets.
            rr(Rect::from_min_size(v(-11.0, -9.0), Vec2::new(17.0, 13.0)), s);
            p.rect_filled(Rect::from_min_size(v(-8.0, -6.0), Vec2::new(5.0, 7.0)), CornerRadius::same(1), with_alpha(ink, 90));
            // Bin.
            let b = Rect::from_min_max(v(2.0, -1.0), v(10.0, 10.0));
            p.rect_filled(b, CornerRadius::same(2), if mono { with_alpha(ink, 60) } else { th().panel });
            p.rect_stroke(b, CornerRadius::same(2), a, egui::StrokeKind::Middle);
            p.line_segment([v(0.5, -2.5), v(11.5, -2.5)], a);
            p.line_segment([v(4.5, -4.5), v(7.5, -4.5)], a);
            p.line_segment([v(5.0, 1.5), v(5.0, 7.5)], Stroke::new(1.2, accent));
            p.line_segment([v(7.0, 1.5), v(7.0, 7.5)], Stroke::new(1.2, accent));
        }
        Icon::Collision => {
            // Two parts kept apart, with a "no" sign where they would meet:
            // parts may not overlap.
            rr(Rect::from_min_size(v(-11.0, -9.0), Vec2::splat(11.0)), s);
            rr(Rect::from_min_size(v(0.0, -2.0), Vec2::splat(11.0)), s);
            let ctr = v(0.5, -0.5);
            p.circle_filled(ctr, 5.0, if mono { with_alpha(ink, 60) } else { th().panel });
            p.circle_stroke(ctr, 5.0, Stroke::new(1.7, accent));
            p.line_segment([ctr + Vec2::new(-3.4, 3.4), ctr + Vec2::new(3.4, -3.4)], Stroke::new(1.7, accent));
        }
        Icon::Snap => {
            // A magnet.
            let mut pts = vec![v(-7.0, -9.0), v(-7.0, 1.0)];
            pts.extend((1..16).map(|i| {
                let t = std::f32::consts::PI * i as f32 / 16.0;
                v(-7.0 * t.cos(), 1.0 + 7.0 * t.sin())
            }));
            pts.extend([v(7.0, 1.0), v(7.0, -9.0)]);
            poly(pts, Stroke::new(2.4, ink));
            p.rect_filled(Rect::from_min_max(v(-9.0, -10.5), v(-5.0, -6.5)), CornerRadius::same(1), accent);
            p.rect_filled(Rect::from_min_max(v(5.0, -10.5), v(9.0, -6.5)), CornerRadius::same(1), accent);
        }
        Icon::Mirror => {
            p.extend(EShape::dashed_line(&[v(0.0, -11.0), v(0.0, 11.0)], Stroke::new(1.3, accent), 2.5, 2.0));
            poly(vec![v(-3.0, -7.0), v(-11.0, 6.0), v(-3.0, 6.0), v(-3.0, -7.0)], s);
            poly(vec![v(3.0, -7.0), v(11.0, 6.0), v(3.0, 6.0), v(3.0, -7.0)], a);
        }
        Icon::MirrorTb => {
            p.extend(EShape::dashed_line(&[v(-11.0, 0.0), v(11.0, 0.0)], Stroke::new(1.3, accent), 2.5, 2.0));
            poly(vec![v(-7.0, -3.0), v(6.0, -11.0), v(6.0, -3.0), v(-7.0, -3.0)], s);
            poly(vec![v(-7.0, 3.0), v(6.0, 11.0), v(6.0, 3.0), v(-7.0, 3.0)], a);
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
            // Parts pushed against the left edge, leaving room on the right.
            p.line_segment([v(-10.0, -10.0), v(-10.0, 10.0)], Stroke::new(2.0, ink));
            rr(Rect::from_min_size(v(-8.0, -7.0), Vec2::new(6.0, 6.0)), s);
            rr(Rect::from_min_size(v(-8.0, 1.0), Vec2::new(6.0, 6.0)), s);
            p.line_segment([v(10.0, 0.0), v(1.0, 0.0)], a);
            poly(vec![v(4.5, -3.5), v(1.0, 0.0), v(4.5, 3.5)], a);
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
        Icon::Folder => {
            poly(vec![v(-11.0, 7.0), v(-11.0, -8.0), v(-4.0, -8.0), v(-2.0, -5.0), v(9.0, -5.0), v(9.0, -1.0)], s);
            poly(vec![v(-11.0, 7.0), v(-7.0, -1.0), v(12.0, -1.0), v(8.0, 7.0), v(-11.0, 7.0)], a);
        }
        Icon::Save => {
            // A floppy disk: everyone's "save".
            poly(vec![v(-9.0, -9.0), v(5.0, -9.0), v(9.0, -5.0), v(9.0, 9.0), v(-9.0, 9.0), v(-9.0, -9.0)], s);
            rr(Rect::from_min_max(v(-5.0, -9.0), v(3.0, -4.0)), s);
            rr(Rect::from_min_max(v(-6.0, 1.0), v(6.0, 9.0)), a);
        }
        Icon::Library | Icon::LibrarySave => {
            rr(Rect::from_min_size(v(-11.0, -8.0), Vec2::new(5.0, 17.0)), s);
            rr(Rect::from_min_size(v(-5.0, -8.0), Vec2::new(5.0, 17.0)), s);
            if icon == Icon::Library {
                // A third book leaning on the others.
                poly(vec![v(2.0, -6.5), v(6.5, -8.0), v(11.0, 7.5), v(6.5, 9.0), v(2.0, -6.5)], a);
            } else {
                p.line_segment([v(7.0, -9.0), v(7.0, 4.0)], a);
                poly(vec![v(3.5, 0.5), v(7.0, 4.0), v(10.5, 0.5)], a);
                p.line_segment([v(3.0, 9.0), v(11.0, 9.0)], a);
            }
        }
        Icon::Database => {
            let ell = |y: f32, half: bool| -> Vec<Pos2> {
                let n = 20;
                (0..=n).map(|i| {
                    let t = if half { std::f32::consts::PI * i as f32 / n as f32 } else { std::f32::consts::TAU * i as f32 / n as f32 };
                    v(9.0 * t.cos(), y + 3.5 * t.sin())
                }).collect()
            };
            poly(ell(-7.0, false), s);
            p.line_segment([v(-9.0, -7.0), v(-9.0, 7.0)], s);
            p.line_segment([v(9.0, -7.0), v(9.0, 7.0)], s);
            poly(ell(7.0, true), s);
            poly(ell(0.0, true), a);
        }
        Icon::Plug => {
            rr(Rect::from_min_max(v(-7.0, -3.0), v(7.0, 5.0)), s);
            p.line_segment([v(-3.0, -3.0), v(-3.0, -9.0)], s);
            p.line_segment([v(3.0, -3.0), v(3.0, -9.0)], s);
            poly(vec![v(0.0, 5.0), v(0.0, 8.0), v(5.0, 10.0), v(10.0, 10.0)], a);
        }
        Icon::WorkOrders => {
            // A clipboard of work orders (the ERP's jobs).
            rr(Rect::from_min_max(v(-8.0, -8.0), v(8.0, 10.0)), s);
            p.rect_filled(Rect::from_min_max(v(-4.0, -11.0), v(4.0, -6.5)), CornerRadius::same(1), ink);
            for (y, x1) in [(-3.0, 5.0), (1.0, 5.0), (5.0, 2.0)] {
                p.line_segment([v(-5.0, y), v(x1, y)], a);
            }
        }
        Icon::Tag => {
            // A labor ticket: a sheet of paper with a bar code.
            rr(Rect::from_min_max(v(-8.0, -10.0), v(8.0, 10.0)), s);
            p.line_segment([v(-5.0, -6.0), v(5.0, -6.0)], Stroke::new(1.2, ink));
            p.line_segment([v(-5.0, -3.0), v(2.0, -3.0)], Stroke::new(1.2, ink));
            for (k, x) in [-5.0f32, -3.5, -1.5, 0.0, 1.0, 3.0, 4.5].into_iter().enumerate() {
                let w = if k % 3 == 0 { 1.6 } else { 0.9 };
                p.line_segment([v(x, 1.5), v(x, 7.0)], Stroke::new(w, accent));
            }
        }
        Icon::Verify => {
            p.circle_stroke(c, 9.5, s);
            poly(vec![v(-4.5, 0.0), v(-1.0, 4.0), v(5.0, -4.0)], Stroke::new(2.2, accent));
        }
        Icon::Play => {
            p.add(EShape::convex_polygon(vec![v(-5.0, -8.0), v(8.0, 0.0), v(-5.0, 8.0)], if mono { ink } else { accent }, Stroke::NONE));
        }
        Icon::Pause => {
            let col = if mono { ink } else { accent };
            p.rect_filled(Rect::from_min_size(v(-6.0, -8.0), Vec2::new(4.0, 16.0)), CornerRadius::same(1), col);
            p.rect_filled(Rect::from_min_size(v(2.0, -8.0), Vec2::new(4.0, 16.0)), CornerRadius::same(1), col);
        }
        Icon::StepBack | Icon::StepFwd => {
            // One step: a single outlined triangle (Beginning / End add a bar).
            let d = if icon == Icon::StepFwd { 1.0 } else { -1.0 };
            let tri = if d > 0.0 { vec![v(-5.0, -8.0), v(6.0, 0.0), v(-5.0, 8.0), v(-5.0, -8.0)] } else { vec![v(5.0, -8.0), v(5.0, 8.0), v(-6.0, 0.0), v(5.0, -8.0)] };
            poly(tri, Stroke::new(1.9, accent));
        }
        Icon::Text => {
            p.line_segment([v(-8.0, -8.0), v(8.0, -8.0)], Stroke::new(2.0, ink));
            p.line_segment([v(0.0, -8.0), v(0.0, 7.0)], Stroke::new(2.0, ink));
            p.line_segment([v(-4.0, 7.0), v(4.0, 7.0)], s);
            p.line_segment([v(-10.0, 11.0), v(10.0, 11.0)], a);
        }
        Icon::Gear => {
            let pts: Vec<Pos2> = (0..32)
                .map(|i| {
                    let t = i as f32 / 32.0 * std::f32::consts::TAU;
                    let tooth = matches!(i % 4, 0 | 1);
                    let rad = if tooth { 10.0 } else { 7.2 };
                    v(rad * t.cos(), rad * t.sin())
                })
                .collect();
            p.add(EShape::closed_line(pts, s));
            p.circle_stroke(c, 3.2, a);
        }
        Icon::Dimension => {
            p.line_segment([v(-10.0, -7.0), v(-10.0, 7.0)], s);
            p.line_segment([v(10.0, -7.0), v(10.0, 7.0)], s);
            p.line_segment([v(-9.0, 0.0), v(9.0, 0.0)], a);
            poly(vec![v(-5.5, -3.0), v(-9.0, 0.0), v(-5.5, 3.0)], a);
            poly(vec![v(5.5, -3.0), v(9.0, 0.0), v(5.5, 3.0)], a);
        }
        Icon::Points => {
            p.circle_stroke(c, 6.5, s);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                p.line_segment([v(dx * 8.0, dy * 8.0), v(dx * 11.0, dy * 11.0)], s);
            }
            p.circle_filled(c, 2.4, accent);
        }
        Icon::Panels => {
            let w = Rect::from_center_size(c, Vec2::new(22.0, 18.0));
            p.rect_filled(Rect::from_min_max(w.min, Pos2::new(w.left() + 7.0, w.bottom())), CornerRadius::same(3), with_alpha(accent, 70));
            rr(w, s);
            p.line_segment([Pos2::new(w.left() + 7.0, w.top()), Pos2::new(w.left() + 7.0, w.bottom())], a);
        }
        Icon::Missing => {
            let w = Rect::from_center_size(c, Vec2::new(20.0, 20.0));
            let q = [w.left_top(), w.right_top(), w.right_bottom(), w.left_bottom(), w.left_top()];
            p.extend(EShape::dashed_line(&q, Stroke::new(1.4, ink), 3.0, 2.0));
            p.text(c, Align2::CENTER_CENTER, "?", FontId::proportional(14.0), accent);
        }
        Icon::Shapes => {
            p.circle_stroke(v(-3.5, 3.0), 6.5, s);
            rr(Rect::from_min_size(v(0.0, -10.0), Vec2::splat(10.0)), a);
        }
        Icon::Stock => {
            // Three sheets, each a little higher and further left.
            for k in 0..3 {
                let off = Vec2::new(3.0 - 3.0 * k as f32, 5.0 - 5.0 * k as f32);
                let r = Rect::from_center_size(c + off, Vec2::new(18.0, 8.0));
                let st = if k == 2 { a } else { s };
                if k < 2 {
                    p.rect_filled(r, CornerRadius::same(2), th().panel);
                }
                p.rect_stroke(r, CornerRadius::same(2), st, egui::StrokeKind::Middle);
            }
        }
        Icon::Ortho => {
            // A right angle marked 90°: lines go straight across or straight up.
            poly(vec![v(-9.0, -10.0), v(-9.0, 8.0), v(10.0, 8.0)], s);
            poly(vec![v(-9.0, 3.0), v(-4.0, 3.0), v(-4.0, 8.0)], a);
            p.text(v(3.0, -2.0), Align2::CENTER_CENTER, "90°", FontId::proportional(9.0), accent);
        }
        Icon::SkipStart | Icon::SkipEnd => {
            let d = if icon == Icon::SkipEnd { 1.0 } else { -1.0 };
            for k in 0..2 {
                let x0 = -8.0 + 7.0 * k as f32;
                let tri = if d > 0.0 { vec![v(x0, -6.5), v(x0 + 7.0, 0.0), v(x0, 6.5)] } else { vec![v(-x0, -6.5), v(-x0, 6.5), v(-x0 - 7.0, 0.0)] };
                p.add(EShape::convex_polygon(tri, accent, Stroke::NONE));
            }
            p.line_segment([v(9.5 * d, -7.0), v(9.5 * d, 7.0)], Stroke::new(2.2, ink));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(14.0)), CornerRadius::same(2), accent);
        }
        Icon::OpenPart => {
            // A window holding one part: open the part in its own view.
            let w = Rect::from_min_max(v(-11.0, -9.0), v(11.0, 9.0));
            rr(w, s);
            p.line_segment([v(-11.0, -5.0), v(11.0, -5.0)], s);
            poly(vec![v(-5.0, -2.0), v(1.0, -2.0), v(1.0, 2.0), v(5.0, 2.0), v(5.0, 6.0), v(-5.0, 6.0), v(-5.0, -2.0)], a);
        }
        Icon::Toolpath => {
            // A part with the cutting path running round it, arrows showing the
            // direction, and the pierce point: machine automatically.
            rr(Rect::from_center_size(c, Vec2::splat(10.0)), Stroke::new(1.4, ink));
            let o = Rect::from_center_size(c, Vec2::splat(19.0));
            poly(vec![v(-9.5, 2.0), o.left_top(), o.right_top(), o.right_bottom(), o.left_bottom(), v(-9.5, 5.0)], a);
            for (tip, l, r) in [((0.0, -9.5), (-3.0, -12.0), (-3.0, -7.0)), ((9.5, 0.0), (7.0, -3.0), (12.0, -3.0)), ((0.0, 9.5), (3.0, 7.0), (3.0, 12.0))] {
                poly(vec![v(l.0, l.1), v(tip.0, tip.1), v(r.0, r.1)], a);
            }
            p.circle_filled(v(-9.5, 2.0), 2.4, accent);
        }
        Icon::Kerf => {
            // The contour, the beam's centre line offset from it by half the
            // kerf (dashed), and the offset marked with an arrow.
            let inner = Rect::from_center_size(c + Vec2::new(-1.0, 0.0), Vec2::new(13.0, 13.0));
            rr(inner, s);
            let o = inner.expand(4.0);
            let q = [o.left_top(), o.right_top(), o.right_bottom(), o.left_bottom(), o.left_top()];
            p.extend(EShape::dashed_line(&q, Stroke::new(1.5, accent), 2.5, 2.0));
            p.line_segment([Pos2::new(inner.right(), c.y), Pos2::new(o.right() + 2.5, c.y)], Stroke::new(1.3, accent));
            poly(vec![Pos2::new(o.right(), c.y - 2.5), Pos2::new(o.right() + 2.5, c.y), Pos2::new(o.right(), c.y + 2.5)], Stroke::new(1.3, accent));
        }
        Icon::CutLine => {
            let sh = Rect::from_center_size(c, Vec2::new(24.0, 18.0));
            rr(sh, s);
            for (x0, y0, x1, y1) in [(5.5, 8.0, 11.0, 2.5), (5.5, 1.0, 11.0, -4.5), (5.5, -6.0, 8.5, -9.0)] {
                p.line_segment([v(x0, y0), v(x1, y1)], Stroke::new(1.0, with_alpha(accent, 170)));
            }
            p.line_segment([v(3.5, -9.0), v(3.5, 9.0)], Stroke::new(2.2, accent));
        }
        Icon::PickOrder => {
            poly(vec![v(-9.0, -9.0), v(-9.0, 5.0), v(-5.5, 1.5), v(-2.5, 8.0), v(0.0, 7.0), v(-3.0, 0.5), v(2.0, 0.5), v(-9.0, -9.0)], s);
            p.circle_stroke(v(6.0, -5.0), 5.0, a);
            p.text(v(6.0, -5.0), Align2::CENTER_CENTER, "1", FontId::proportional(8.5), accent);
        }
        Icon::OnElement => {
            // A pointer clicking on one line of a part: put it on this element.
            poly(vec![v(-11.0, 4.0), v(-11.0, -8.0), v(2.0, -8.0)], Stroke::new(1.4, ink));
            p.line_segment([v(-11.0, -8.0), v(2.0, -8.0)], Stroke::new(3.0, with_alpha(accent, 140)));
            poly(vec![v(-2.0, -6.0), v(-2.0, 7.0), v(1.0, 4.0), v(3.5, 10.0), v(5.5, 9.0), v(3.0, 3.0), v(7.5, 3.0), v(-2.0, -6.0)], s);
        }
        Icon::Contours => {
            // An L-shaped part with a square hole: its outer and inner contours.
            poly(vec![v(-10.0, -9.0), v(2.0, -9.0), v(2.0, -1.0), v(10.0, -1.0), v(10.0, 9.0), v(-10.0, 9.0), v(-10.0, -9.0)], s);
            rr(Rect::from_min_size(v(-6.0, -1.0), Vec2::new(6.0, 6.0)), a);
        }
        Icon::SimWindow => {
            rr(Rect::from_min_max(v(-11.0, -8.0), v(11.0, 9.0)), s);
            p.line_segment([v(-11.0, -4.0), v(11.0, -4.0)], s);
            p.add(EShape::convex_polygon(vec![v(-3.0, -1.0), v(4.0, 2.5), v(-3.0, 6.0)], accent, Stroke::NONE));
        }
        Icon::Info => {
            p.circle_stroke(c, 9.5, s);
            p.circle_filled(v(0.0, -4.5), 1.5, accent);
            p.line_segment([v(0.0, -1.0), v(0.0, 5.5)], Stroke::new(2.0, accent));
        }
        Icon::SizeBands => {
            for (k, (w, y)) in [(6.0, -7.0), (12.0, 0.0), (20.0, 7.0)].into_iter().enumerate() {
                let r = Rect::from_min_size(v(-10.0, y - 2.5), Vec2::new(w, 5.0));
                if k == 2 {
                    p.rect_filled(r, CornerRadius::same(2), accent);
                } else {
                    p.rect_stroke(r, CornerRadius::same(2), s, egui::StrokeKind::Middle);
                }
            }
        }
        Icon::SheetNav => {
            let back = Rect::from_center_size(v(2.0, -2.0), Vec2::new(14.0, 11.0));
            p.rect_filled(back, CornerRadius::same(2), with_alpha(accent, 60));
            rr(back, Stroke::new(1.3, ink));
            let front = Rect::from_center_size(v(-1.0, 2.0), Vec2::new(14.0, 11.0));
            p.rect_filled(front, CornerRadius::same(2), th().card);
            rr(front, s);
            poly(vec![v(-10.0, -1.0), v(-13.0, 2.0), v(-10.0, 5.0)], a);
            poly(vec![v(11.0, -1.0), v(14.0, 2.0), v(11.0, 5.0)], a);
        }
        Icon::MoveArrows => {
            p.line_segment([v(-10.0, 0.0), v(10.0, 0.0)], s);
            p.line_segment([v(0.0, -10.0), v(0.0, 10.0)], s);
            for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let tip = v(dx * 11.0, dy * 11.0);
                let side = Vec2::new(-dy, dx) * 3.5;
                let back = Vec2::new(dx, dy) * -4.0;
                poly(vec![tip + back + side, tip, tip + back - side], a);
            }
        }
        Icon::Bevel => {
            // A plate in section with one edge cut at an angle.
            let pts = vec![v(-11.0, -5.0), v(4.0, -5.0), v(10.0, 6.0), v(-11.0, 6.0), v(-11.0, -5.0)];
            p.add(EShape::convex_polygon(pts[..4].to_vec(), with_alpha(accent, 45), Stroke::NONE));
            poly(pts, s);
            p.line_segment([v(4.0, -5.0), v(4.0, 9.0)], Stroke::new(1.0, ink.gamma_multiply(0.6)));
            let arc: Vec<Pos2> = (0..=6).map(|i| {
                let t = (i as f32 / 6.0) * 0.5;
                v(4.0, -5.0) + Vec2::new(t.sin() * 9.0, t.cos() * 9.0)
            }).collect();
            poly(arc, a);
        }
        Icon::FollowHead => {
            // View corners round a cutting head.
            for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let k = v(dx * 11.0, dy * 10.0);
                poly(vec![k + Vec2::new(-dx * 5.0, 0.0), k, k + Vec2::new(0.0, -dy * 5.0)], s);
            }
            let head = vec![v(-4.0, -6.0), v(4.0, -6.0), v(4.0, -1.0), v(1.5, 4.0), v(-1.5, 4.0), v(-4.0, -1.0)];
            p.add(EShape::convex_polygon(head, accent, Stroke::NONE));
            p.circle_filled(v(0.0, 6.5), 1.6, accent);
        }
        Icon::StandardLeads => {
            // A lead-in on a part corner, and a small table it comes from.
            poly(vec![v(-11.0, 1.0), v(1.0, 1.0), v(1.0, 11.0)], s);
            p.line_segment([v(-6.0, 6.0), v(1.0, 1.0)], a);
            p.circle_filled(v(-6.0, 6.0), 2.0, accent);
            let t = Rect::from_min_size(v(2.0, -11.0), Vec2::new(10.0, 9.0));
            rr(t, Stroke::new(1.3, ink));
            p.line_segment([Pos2::new(t.left(), t.top() + 3.0), Pos2::new(t.right(), t.top() + 3.0)], Stroke::new(1.0, ink));
            p.line_segment([Pos2::new(t.left(), t.top() + 6.0), Pos2::new(t.right(), t.top() + 6.0)], Stroke::new(1.0, ink));
            p.line_segment([Pos2::new(t.center().x, t.top()), Pos2::new(t.center().x, t.bottom())], Stroke::new(1.0, ink));
        }
        Icon::Reverse => {
            p.circle_stroke(c, 8.0, Stroke::new(1.4, ink));
            poly(vec![v(-3.0, -11.0), v(1.0, -8.0), v(-3.0, -5.0)], a);
            poly(vec![v(3.0, 5.0), v(-1.0, 8.0), v(3.0, 11.0)], a);
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            p.circle_stroke(v(-2.0, -2.0), 6.5, s);
            p.line_segment([v(3.0, 3.0), v(9.0, 9.0)], Stroke::new(2.2, ink));
            p.line_segment([v(-5.5, -2.0), v(1.5, -2.0)], Stroke::new(1.6, accent));
            if icon == Icon::ZoomIn {
                p.line_segment([v(-2.0, -5.5), v(-2.0, 1.5)], Stroke::new(1.6, accent));
            }
        }
        Icon::CheckDrawing => {
            poly(vec![v(-10.0, -9.0), v(0.0, -9.0), v(0.0, -3.0), v(4.0, -3.0)], Stroke::new(1.3, ink));
            p.circle_stroke(v(1.0, 0.0), 6.0, a);
            p.line_segment([v(5.5, 4.5), v(10.0, 9.5)], Stroke::new(2.4, accent));
        }
        Icon::LibraryAdd => {
            rr(Rect::from_min_size(v(-11.0, -8.0), Vec2::new(5.0, 17.0)), s);
            rr(Rect::from_min_size(v(-5.0, -8.0), Vec2::new(5.0, 17.0)), s);
            p.line_segment([v(7.0, -5.0), v(7.0, 5.0)], Stroke::new(2.0, accent));
            p.line_segment([v(2.0, 0.0), v(12.0, 0.0)], Stroke::new(2.0, accent));
        }
        Icon::NestSheet => {
            // One sheet with its parts and a turning arrow: nest this sheet again.
            let sh = Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::new(19.0, 15.0));
            rr(sh, s);
            let fill = if mono { ink } else { accent };
            for (x, y, w, h) in [(2.5, 2.5, 6.0, 10.0), (10.0, 2.5, 6.5, 4.5)] {
                p.rect_filled(Rect::from_min_size(sh.min + Vec2::new(x, y), Vec2::new(w, h)), CornerRadius::same(1), fill);
            }
            let ctr = v(7.5, 6.5);
            let pts: Vec<Pos2> = (0..=12).map(|i| {
                let t = -0.3 + i as f32 / 12.0 * 4.4;
                ctr + Vec2::new(t.cos(), t.sin()) * 4.0
            }).collect();
            let end = *pts.last().unwrap();
            poly(pts, Stroke::new(1.5, ink));
            poly(vec![end + Vec2::new(-2.5, -1.0), end, end + Vec2::new(0.5, -3.0)], Stroke::new(1.5, ink));
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
    // The first big button of a ribbon group lends the group its icon for
    // when the group has to fold into one button.
    ui.ctx().data_mut(|d| {
        let first = d.get_temp_mut_or_default::<Option<Option<Icon>>>(egui::Id::unique(GROUP_ICON));
        if let Some(None) = first {
            *first = Some(Some(icon));
        }
    });
    let t = th();
    let text_col = if !enabled {
        with_alpha(t.text_dim, 160)
    } else if primary {
        Color32::WHITE
    } else if active {
        t.accent_fg
    } else {
        t.text
    };
    let galley = ui.painter().layout(label.to_string(), FontId::proportional(12.0), text_col, 70.0);
    let w = galley.size().x.max(36.0) + 12.0;
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
            // The "on" marker always sits at the top edge, the same place on
            // every button whatever its label (it never covers text there).
            let y = rect.top() + 3.5;
            let pill = Rect::from_center_size(Pos2::new(rect.center().x, y), Vec2::new(16.0, 3.0));
            p.rect_filled(pill, CornerRadius::same(2), t.accent_fg);
            // And a tick in the corner: this setting is ON (not a command).
            let cc = Pos2::new(rect.right() - 8.0, rect.top() + 9.0);
            p.circle_filled(cc, 5.5, t.accent);
            let st = Stroke::new(1.4, Color32::WHITE);
            p.line_segment([cc + Vec2::new(-2.5, 0.2), cc + Vec2::new(-0.7, 2.0)], st);
            p.line_segment([cc + Vec2::new(-0.7, 2.0), cc + Vec2::new(2.6, -1.8)], st);
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

/// A small ribbon button: its picture and label on one line, three to a
/// column (Office's small buttons), for groups that would be too wide with
/// big ones. Lit like a big button when `active`.
pub(crate) fn ribbon_small(ui: &mut egui::Ui, icon: Icon, label: &str, active: bool, enabled: bool) -> egui::Response {
    let t = th();
    let col = if !enabled { with_alpha(t.text_dim, 160) } else if active { t.accent_fg } else { t.text };
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), col);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 36.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("h"), enabled && resp.hovered(), 0.1);
    let p = ui.painter();
    if active {
        p.rect_filled(rect, CornerRadius::same(6), t.accent_soft);
    } else if hover > 0.0 {
        p.rect_filled(rect, CornerRadius::same(6), with_alpha(t.hover, (hover * 255.0) as u8));
    }
    let ir = Rect::from_center_size(Pos2::new(rect.left() + 13.0, rect.center().y), Vec2::splat(22.0));
    let (ink, acc) = if enabled { (t.text, t.accent_fg) } else { (with_alpha(t.text_dim, 150), with_alpha(t.text_dim, 150)) };
    draw_icon_with(&p.clone().with_clip_rect(rect.expand(1.0)), ir, icon, ink, acc, false);
    p.galley(Pos2::new(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, col);
    resp
}

/// How one ribbon fits its window, Office-style: the width each group took
/// when last shown in full, and which groups are folded into a single
/// drop-down button because the window is too narrow for all of them.
#[derive(Clone, Default)]
struct RibbonFit {
    full: Vec<f32>,
    folded: Vec<f32>,
    /// Each group's first big button's icon (shown on its folded button).
    icons: Vec<Option<Icon>>,
    /// Each group's fold priority (see `fold_priority`).
    prio: Vec<u8>,
    fold: Vec<bool>,
    next: usize,
}

/// Which groups fold first when a ribbon is too wide: the least needed (low
/// numbers) go first, so what starts or finishes a job — the job itself,
/// nesting, output, and on the Technology tab what the commands apply to —
/// stays open longest.
fn fold_priority(title: &str) -> u8 {
    match title {
        "Move" => 0,
        "Sheets" => 1,
        "Edit" => 2,
        "Remnants" => 3,
        "Modify" => 6,
        "Insert" => 7,
        "Job" | "Output" | "Jobs" => 8,
        "Nest" | "Apply to" | "Machining" | "Simulate" => 9,
        _ => 5,
    }
}

/// Where `ribbon_group` finds the fit of the ribbon being drawn (only set
/// while `App::ribbon` draws, so groups elsewhere never fold).
const RIBBON_FIT_CUR: &str = "ribbon_fit_cur";
/// Collects the icon of the first big button drawn in a group: `None` when not
/// collecting, `Some(None)` while waiting for it.
const GROUP_ICON: &str = "ribbon_group_icon";
/// Width of a folded group before it has been measured.
const FOLDED_W: f32 = 84.0;

/// Which groups fold so the ribbon fits `avail`: none when everything fits;
/// otherwise the lowest-priority groups first (the rightmost among equals),
/// one at a time, until the rest fits.
pub(crate) fn ribbon_fit_plan(full: &[f32], folded: &[f32], prio: &[u8], avail: f32) -> Vec<bool> {
    let n = full.len();
    let fw = |i: usize| folded.get(i).copied().filter(|w| *w > 0.0).unwrap_or(FOLDED_W);
    let mut fold = vec![false; n];
    let mut total: f32 = full.iter().sum();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (prio.get(i).copied().unwrap_or(5), std::cmp::Reverse(i)));
    let mut folded_order = vec![];
    for i in order {
        if total <= avail {
            break;
        }
        total += fw(i) - full[i];
        fold[i] = true;
        folded_order.push(i);
    }
    // A wide group may have had to fold after smaller ones: open those smaller,
    // less needed groups again where they fit in the room it left.
    if let Some(&last) = folded_order.last() {
        let p_last = prio.get(last).copied().unwrap_or(5);
        for &i in folded_order.iter().rev().skip(1) {
            if prio.get(i).copied().unwrap_or(5) < p_last && total + full[i] - fw(i) <= avail {
                total += full[i] - fw(i);
                fold[i] = false;
            }
        }
    }
    fold
}

/// A ribbon group: its controls with the group's name centred underneath —
/// or, when the window is too narrow, one button with the group's name that
/// shows the group's controls in a drop-down.
pub(crate) fn ribbon_group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let slot = ui.ctx().data_mut(|d| {
        let cur = d.get_temp::<Option<egui::Id>>(egui::Id::unique(RIBBON_FIT_CUR)).flatten()?;
        let f = d.get_temp_mut_or_default::<RibbonFit>(cur);
        let i = f.next;
        f.next += 1;
        let icon = f.icons.get(i).copied().flatten();
        if f.prio.len() <= i {
            f.prio.resize(i + 1, 5);
        }
        f.prio[i] = fold_priority(title);
        Some((cur, i, i < f.full.len() && f.fold.get(i).copied().unwrap_or(false), icon))
    });
    let folded = slot.is_some_and(|s| s.2);
    let group_icon = slot.and_then(|s| s.3);
    let top = ui.cursor().top();
    let x0 = ui.cursor().left();
    ui.vertical(|ui| {
        if folded {
            let resp = ui.horizontal(|ui| {
                ui.set_min_height(70.0);
                folded_group_button(ui, title, group_icon)
            });
            let resp = resp.inner;
            // The button carries the group's name, so the caption row stays empty.
            ui.add_space(14.0 + ui.spacing().item_spacing.y);
            egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.vertical(|ui| {
                    let inner = ui.horizontal(|ui| {
                        ui.set_min_height(70.0);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        add(ui);
                    });
                    let w = inner.response.rect.width().max(40.0);
                    ui.add_sized([w, 14.0], egui::Label::new(RichText::new(title).size(11.5).color(th().text_dim)));
                });
            });
        } else {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(GROUP_ICON), Some(None::<Icon>)));
            let inner = ui.horizontal(|ui| {
                ui.set_min_height(70.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                add(ui);
            });
            let first = ui.ctx().data_mut(|d| d.get_temp::<Option<Option<Icon>>>(egui::Id::unique(GROUP_ICON)).flatten().flatten());
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(GROUP_ICON), None::<Option<Icon>>));
            if let Some((id, i, _, _)) = slot {
                ui.ctx().data_mut(|d| {
                    let f = d.get_temp_mut_or_default::<RibbonFit>(id);
                    if f.icons.len() <= i {
                        f.icons.resize(i + 1, None);
                    }
                    f.icons[i] = first;
                });
            }
            // Every group's caption sits on one line across the ribbon, even
            // when a group of form fields is a little taller than the buttons.
            let w = inner.response.rect.width().max(40.0);
            let font = FontId::proportional(12.0);
            let tw = ui.painter().layout_no_wrap(title.to_string(), font.clone(), th().text_dim).size().x;
            let (cap, _) = ui.allocate_exact_size(Vec2::new(w.max(tw), 14.0), Sense::hover());
            let y = top + 70.0 + ui.spacing().item_spacing.y + 7.0;
            ui.painter().text(Pos2::new(cap.center().x, y), Align2::CENTER_CENTER, title, font, th().text_dim);
        }
    });
    ui.add_space(5.0);
    let x = ui.cursor().left();
    ui.painter().line_segment([Pos2::new(x, top + 8.0), Pos2::new(x, top + 78.0)], Stroke::new(1.0, th().border));
    ui.add_space(7.0);
    if let Some((id, i, folded, _)) = slot {
        let w = ui.cursor().left() - x0;
        ui.ctx().data_mut(|d| {
            let f = d.get_temp_mut_or_default::<RibbonFit>(id);
            let v = if folded { &mut f.folded } else { &mut f.full };
            if v.len() <= i {
                v.resize(i + 1, 0.0);
            }
            v[i] = w;
        });
    }
}

/// The button a folded ribbon group shows: the group's own icon (its first
/// command's) and its name with a caret — click for the group's commands.
fn folded_group_button(ui: &mut egui::Ui, title: &str, icon: Option<Icon>) -> egui::Response {
    let t = th();
    let galley = ui.painter().layout(title.to_string(), FontId::proportional(12.0), t.text, 90.0);
    let w = galley.size().x.max(38.0) + 18.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 70.0), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let hover = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let p = ui.painter();
    let card = rect.shrink(1.0);
    if open {
        p.rect_filled(card, CornerRadius::same(8), t.accent_soft);
        p.rect_stroke(card, CornerRadius::same(8), Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
    } else if hover > 0.0 {
        p.rect_filled(card, CornerRadius::same(8), with_alpha(t.hover, (hover * 255.0) as u8));
    }
    let ir = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 21.0), Vec2::new(32.0, 32.0));
    // Groups whose first command's icon would mislead when folded.
    let icon = match title {
        "Sheets" => Some(Icon::SheetNav),
        "Move" => Some(Icon::MoveArrows),
        _ => icon,
    };
    match icon {
        Some(i) => draw_icon(p, ir, i, false),
        None => {
            for (k, (dx, dy)) in [(-5.5, -5.5), (5.5, -5.5), (-5.5, 5.5), (5.5, 5.5)].into_iter().enumerate() {
                let tile = Rect::from_center_size(ir.center() + Vec2::new(dx, dy), Vec2::splat(8.5));
                if k == 0 {
                    p.rect_filled(tile, CornerRadius::same(2), t.accent_fg);
                } else {
                    p.rect_stroke(tile, CornerRadius::same(2), Stroke::new(1.4, t.text), egui::StrokeKind::Inside);
                }
            }
        }
    }
    let gpos = Pos2::new(rect.center().x - galley.size().x / 2.0, rect.top() + 40.0);
    let below = gpos.y + galley.size().y + 6.0;
    p.galley(gpos, galley, t.text);
    let cc = Pos2::new(rect.center().x, below);
    p.add(EShape::convex_polygon(vec![cc + Vec2::new(-4.0, -2.0), cc + Vec2::new(4.0, -2.0), cc + Vec2::new(0.0, 2.5)], t.text_dim, Stroke::NONE));
    resp.on_hover_text(format!("{title}: its commands (the window is too narrow to show them all)"))
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
pub(crate) fn chip(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(9, 2))
        .show(ui, |ui| ui.label(RichText::new(text).size(12.0).color(fg)))
        .response
}

fn row(ui: &mut egui::Ui, k: &str, v: String) {
    // A detail row (label starting with spaces) is dimmer than a total.
    let detail = k.starts_with("  ");
    // The row's stripe runs edge to edge; the text keeps a little inset.
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(if detail { RichText::new(k).color(th().text_dim) } else { RichText::new(k) });
    });
    // Numbers line up on the right of their column.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(6.0);
        ui.label(if detail { RichText::new(v).color(th().text_dim) } else { RichText::new(v).strong() });
    });
    ui.end_row();
}

/// Height of the ribbon bar: a group (big buttons and caption) plus the bar's margins.
pub const RIBBON_H: f32 = 98.0;

/// A soft fade along the bottom of a scrolled area when more content is below,
/// so a pane or dialog that ends mid-form says "there's more" (with its scroll bar).
pub(crate) fn fade_if_more<R>(ui: &egui::Ui, out: &egui::scroll_area::ScrollAreaOutput<R>) {
    let r = out.inner_rect;
    let hidden = out.content_size.y - out.state.clamped_offset().y - r.height();
    if hidden > 2.0 {
        let h = 26.0_f32.min(hidden + 6.0);
        let bg = ui.visuals().panel_fill;
        let mut mesh = egui::Mesh::default();
        let (top, bot) = (r.bottom() - h, r.bottom());
        mesh.colored_vertex(Pos2::new(r.left(), top), with_alpha(bg, 0));
        mesh.colored_vertex(Pos2::new(r.right() - 10.0, top), with_alpha(bg, 0));
        mesh.colored_vertex(Pos2::new(r.right() - 10.0, bot), bg);
        mesh.colored_vertex(Pos2::new(r.left(), bot), bg);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(EShape::mesh(mesh));
    }
}

/// Where a main dialog first opens: centred across the window, just below the
/// ribbon, so it never covers the title row, the workspace switch or the tabs.
pub(crate) fn dialog_pos(ctx: &egui::Context) -> Pos2 {
    let r = ctx.content_rect();
    Pos2::new(r.center().x, r.top() + 176.0)
}

/// A bold section title with a little room above the rows it heads.
fn section_heading(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(13.5).strong().color(th().text_strong));
    ui.add_space(2.0);
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
            // File commands first, as in most desktop programs; then editing,
            // then the job's own commands.
            let small = [Icon::New, Icon::Folder, Icon::Save, Icon::Undo, Icon::Redo, Icon::NestAll, Icon::Export, Icon::Times, Icon::Fit];
            let tips = ["New job", "Open job: recent job files and Library jobs (Ctrl+O browses for a file)", "Save job (Ctrl+S)", "Undo (Ctrl+Z)", "Redo (Ctrl+Y)", "Nest all (F5)", "Export CNC for every sheet", "Times and costs", "Fit sheet"];
            for (ic, tip) in small.into_iter().zip(tips) {
                if matches!(ic, Icon::Undo | Icon::NestAll) {
                    // A thin divider between file, editing and job commands.
                    let (r, _) = ui.allocate_exact_size(Vec2::new(5.0, 22.0), Sense::hover());
                    ui.painter().line_segment([Pos2::new(r.center().x, r.top() + 4.0), Pos2::new(r.center().x, r.bottom() - 4.0)], Stroke::new(1.0, th().border));
                }
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
                if ic == Icon::Folder {
                    // The same recent-jobs list as Home › Open job ▾.
                    let resp = resp.on_hover_text(tip);
                    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| self.open_job_menu(ui));
                    continue;
                }
                if resp.on_hover_text(tip).clicked() && enabled {
                    match ic {
                        Icon::NestAll => self.run_nesting(),
                        Icon::New => self.new_job(),
                        Icon::Folder => self.open_job_dialog(),
                        Icon::Save => self.save_job(false),
                        Icon::Export => self.export_checked(),
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
            // A divider: the switch picks the screen, the tabs pick the ribbon.
            ui.add_space(10.0);
            let (d, _) = ui.allocate_exact_size(Vec2::new(1.0, 22.0), Sense::hover());
            ui.painter().rect_filled(d, CornerRadius::ZERO, th().border);
            ui.add_space(10.0);
            // An opened part has the Drawing module's own tabs (Ui\Drawing.xml).
            let tabs: &[(Tab, &str)] = if self.workspace == crate::Workspace::Part {
                let all: &[(Tab, &str)] = &[(Tab::Home, "Home"), (Tab::Technology, "Technology"), (Tab::Machining, "Machining"), (Tab::View, "View"), (Tab::Simulation, "Simulation"), (Tab::Notes, "Notes"), (Tab::Utilities, "Utilities")];
                if crate::drawing::utility_tab_shown(&self.dv) { all } else { &all[..6] }
            } else if self.workspace == crate::Workspace::Jobs {
                // The job manager has one ribbon; the sheet tabs belong to Nesting.
                &[(Tab::Home, "Home")]
            } else {
                &[(Tab::Home, "Home"), (Tab::Sheet, "Sheet"), (Tab::Technology, "Technology"), (Tab::Machining, "Machining"), (Tab::View, "View"), (Tab::Simulation, "Simulation")]
            };
            for &(t, name) in tabs {
                let selected = self.tab == t || tabs.len() == 1;
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
                        if menu_row(ui, "⚙", "All settings…", "Ctrl + ,") {
                            go = Some(crate::settings_ui::Section::General);
                        }
                        if menu_row(ui, "🗄", "Browse database", "") {
                            browse = true;
                        }
                        // Light / dark is the switch beside this menu (and
                        // Settings → Appearance, which also follows Windows).
                        ui.separator();
                        ui.label(RichText::new("Go to settings").size(12.0).color(th().text_dim));
                        if menu_row(ui, "🔧", "General", "warnings, commands") {
                            go = Some(crate::settings_ui::Section::General);
                        }
                        if menu_row(ui, "◐", "Appearance", "light, dark or Windows") {
                            go = Some(crate::settings_ui::Section::Appearance);
                        }
                        if menu_row(ui, "⚙", "Machines & output", "") {
                            go = Some(crate::settings_ui::Section::Machines);
                        }
                        if menu_row(ui, "⌨", "Shortcuts", "keys and mouse") {
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
        // Groups that don't fit fold, from the right, into one drop-down button
        // each (measured last frame). If even that is too wide the ribbon
        // scrolls sideways rather than squeezing labels into tall columns.
        let tab = if self.workspace == crate::Workspace::Jobs { Tab::Home } else { self.tab };
        let fit = egui::Id::unique(("ribbon_fit", self.workspace as u8, tab as u8));
        let avail = ui.available_width() - 4.0;
        ui.ctx().data_mut(|d| {
            let f = d.get_temp_mut_or_default::<RibbonFit>(fit);
            f.fold = ribbon_fit_plan(&f.full, &f.folded, &f.prio, avail);
            f.next = 0;
            d.insert_temp(egui::Id::unique(RIBBON_FIT_CUR), Some(fit));
        });
        egui::ScrollArea::horizontal()
            .id_salt(("ribbon", self.tab as u8))
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    match self.tab {
                        _ if self.workspace == crate::Workspace::Jobs => self.ribbon_jobs(ui),
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
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::unique(RIBBON_FIT_CUR), None::<egui::Id>);
            let f = d.get_temp_mut_or_default::<RibbonFit>(fit);
            let n = f.next;
            f.full.truncate(n);
            f.folded.truncate(n);
            f.icons.truncate(n);
            f.prio.truncate(n);
        });
    }

    fn ribbon_home(&mut self, ui: &mut egui::Ui) {
        let has_sheets = !self.result.sheets.is_empty();
        // The groups follow the job: the job itself, its parts, nesting, editing,
        // remnants, output; nudging (least used) comes last, so it is the first
        // to fold when the window is narrow.
        // Small buttons: the job commands are also in the quick-access bar.
        ribbon_group(ui, "Job", |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(1.0);
                if ribbon_small(ui, Icon::New, "New job", false, true).on_hover_text("Start an empty job").clicked() {
                    self.new_job();
                }
                let r = ribbon_small(ui, Icon::Folder, "Open job ▾", false, true).on_hover_text("Recent jobs, the Library, or any job file (Ctrl + O)");
                egui::Popup::menu(&r).show(|ui| self.open_job_menu(ui));
                let saved = self.job_path.as_ref().map_or("not saved yet".to_string(), |p| p.display().to_string());
                if ribbon_small(ui, Icon::Save, "Save job", false, true).on_hover_text(format!("Save the job — products, sheets and cut lines (Ctrl + S; Ctrl + Shift + S to choose where)\n{saved}")).clicked() {
                    self.save_job(false);
                }
            });
        });
        ribbon_group(ui, "Insert", |ui| {
            if ribbon_button(ui, Icon::Import, "Import\ndrawings", false, true)
                .on_hover_text("Add part drawings: DXF, DWG, Lantek MEC, IGES, PDF, ESSI, DSTV or GEO files (or drop them on the window)")
                .clicked()
            {
                self.import_dialog();
            }
            let r = ribbon_button(ui, Icon::AddPart, "Add\npart", false, true);
            add_part_menu(self, &r);
            let can_open = self.sel_part.is_some_and(|i| i < self.parts.len());
            if ribbon_button(ui, Icon::OpenPart, "Open\npart", false, can_open)
                .on_hover_text(if can_open { "Open the selected part on its own to apply technology (or double-click a part)" } else { "Pick a part first (in the list, on the sheet or in Parts View), or double-click one" })
                .clicked()
            {
                if let Some(i) = self.sel_part {
                    self.open_part(i);
                }
            }
        });
        ribbon_group(ui, "Nest", |ui| {
            if ribbon_button_ex(ui, Icon::NestAll, "Nest\nall", false, !self.parts.is_empty(), true).on_hover_text("Place every part (F5)").clicked() {
                self.run_nesting();
            }
            if ribbon_button(ui, Icon::NestSheet, "Re-nest\nsheet", false, self.result.sheets.get(self.cur_sheet).is_some_and(|s| !s.is_empty()))
                .on_hover_text("Nest only this sheet again: the parts already on it, on the same sheet — the other sheets stay as they are")
                .clicked()
            {
                self.renest_sheet();
            }
            if ribbon_button(ui, Icon::Stock, "Sheet\nstock", self.stockui.open, true).on_hover_text("Sheets and remnants in stock: put one on this sheet, or manage the stock").clicked() {
                self.stockui.open = !self.stockui.open;
                self.stockui.loaded = false;
            }
            // What Nest all will nest in, in view before nesting.
            ui.vertical(|ui| {
                ui.add_space(8.0);
                let mat = format!("{} · {} mm  ▾", cnc::MATERIALS[self.spec.material.min(cnc::MATERIALS.len() - 1)], self.spec.thickness);
                let resp = ui.add(egui::Button::new(RichText::new(mat).strong()).min_size(Vec2::new(120.0, 26.0))).on_hover_text("The job's material and thickness — click to change them");
                // Change them in place; the rest of the sheet set-up is on the Sheet tab.
                egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                    let before = self.spec.clone();
                    ui.set_min_width(300.0);
                    ui.label(RichText::new("Material").strong());
                    ui.horizontal_wrapped(|ui| {
                        for (k, m) in cnc::MATERIALS.iter().enumerate() {
                            ui.selectable_value(&mut self.spec.material, k, *m);
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Thickness").strong());
                    ui.horizontal_wrapped(|ui| {
                        for t in cnc::THICKNESSES {
                            ui.selectable_value(&mut self.spec.thickness, t, format!("{t} mm"));
                        }
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    if ui.link("Sheet size, machine and spacing — Sheet tab").clicked() {
                        self.tab = Tab::Sheet;
                    }
                    self.after_material_edit(before);
                });
                ui.add_space(2.0);
                ui.label(RichText::new(format!("{:.0} × {:.0} sheet", self.spec.w, self.spec.h)).size(11.5).color(th().text_dim));
            });
        });
        ribbon_group(ui, "Edit", |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(1.0);
                if ribbon_small(ui, Icon::Undo, "Undo", false, !self.undo.is_empty()).on_hover_text("Ctrl+Z").clicked() {
                    self.undo();
                }
                if ribbon_small(ui, Icon::Redo, "Redo", false, !self.redo.is_empty()).on_hover_text("Ctrl+Y").clicked() {
                    self.redo();
                }
            });
        });
        let has = self.sel_placed.is_some();
        // Greyed-out part commands say why: they act on parts picked on the sheet.
        let need = |t: &str| if has { t.to_string() } else { format!("Click a part on the sheet first (Ctrl+A picks them all).\n{t}") };
        ribbon_group(ui, "Modify", |ui| {
            if ribbon_button(ui, Icon::Collision, "Collision", self.collision, true)
                .on_hover_text("Stop moved parts from overlapping")
                .clicked()
            {
                self.collision = !self.collision;
            }
            if ribbon_button(ui, Icon::Snap, "Snap", self.cfg.nesting.snap, true)
                .on_hover_text("Dragged parts snap to one part gap from a neighbour or to the margin when they come near (hold Alt while dragging to place freely)")
                .clicked()
            {
                self.cfg.nesting.snap = !self.cfg.nesting.snap;
                self.save_prefs();
            }
            // The part commands as small buttons, three to a column, so the
            // whole group fits beside the others on a 1440 px screen.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(1.0);
                if ribbon_small(ui, Icon::Copy, "Copy part", false, has).on_hover_text(need("Ctrl+D — copy the selected parts to the first free spot (or Ctrl+drag them to where you want the copy)")).clicked() {
                    self.duplicate_selected();
                }
                if ribbon_small(ui, Icon::Rotate, "Rotate 90°", false, has).on_hover_text(need("R / Ctrl+Q — turn left 90° about the selection's centre; Ctrl+W turns right")).clicked() {
                    self.rotate_selected();
                }
                if ribbon_small(ui, Icon::Mirror, "Mirror", false, has).on_hover_text(need("M — mirror left↔right about the selection's centre; Shift+M top↔bottom")).clicked() {
                    self.mirror_selected(true);
                }
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add_space(1.0);
                if ribbon_small(ui, Icon::Remove, "Remove part", false, has).on_hover_text(need("Del — take the selected parts off the sheet")).clicked() {
                    self.remove_selected();
                }
                if ribbon_small(ui, Icon::Clear, "Clear sheets", false, has_sheets).on_hover_text("Take every part off every sheet (Ctrl+Z undoes it)").clicked() {
                    self.clear();
                }
            });
        });
        ribbon_group(ui, "Remnants", |ui| {
            if ribbon_button(ui, Icon::Compact, "Compact\nto left", false, has_sheets)
                .on_hover_text("Slide parts left and down so one large remnant is left on the right")
                .clicked()
            {
                self.compact();
            }
            if ribbon_button(ui, Icon::CutLine, "Cut off\nleftover", false, has_sheets)
                .on_hover_text("Add a remnant line: the leftover part of the sheet is cut off and kept as a remnant in stock (more remnant tools on the Sheet tab)")
                .clicked()
            {
                self.auto_remnant_edge();
            }
        });
        ribbon_group(ui, "Output", |ui| {
            if ribbon_button(ui, Icon::Export, "Export\nCNC", false, has_sheets)
                .on_hover_text(crate::export_tip(self))
                .clicked()
            {
                self.export_checked();
            }
            // Cutting path and Machine program are on the View tab.
            if ribbon_button(ui, Icon::Times, "Times and\ncosts", self.show_times, true).on_hover_text("How long the job takes on the machine, sheet by sheet, and what it costs (a panel beside the sheet)").clicked() {
                self.show_times = !self.show_times;
            }
        });
        // Going between sheets is on the View tab, the drawing's ‹ › and the
        // right-click ring, so Home keeps room for its commands.
        let has = self.sel_placed.is_some();
        ribbon_group(ui, "Move", |ui| self.move_group(ui, has));
    }

    /// After the material or thickness was changed outside the Sheet tab:
    /// with parts nested, ask first (T4) as the Sheet tab does; else refresh.
    fn after_material_edit(&mut self, before: crate::nest::SheetSpec) {
        if self.spec.material == before.material && self.spec.thickness == before.thickness {
            return;
        }
        if !self.result.sheets.is_empty() {
            let new_spec = self.spec.clone();
            self.spec = before;
            if self.pending.is_none() {
                self.pending = Some(crate::Pending::NestedSpecChange { new_spec });
            }
            return;
        }
        self.refresh();
    }

    fn ribbon_technology(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut cost_changed = false;
        let spec_before = self.spec.clone();
        ribbon_group(ui, "Material", |ui| {
            egui::Grid::new("mat_grid").num_columns(2).spacing([6.0, 5.0]).show(ui, |ui| {
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
            // Which section of the machine file applies (MTL1–MTL6).
            let mt = crate::ct1::for_spec(&self.spec);
            let short = match mt.applied() {
                Some(s) => format!("Machine table {}", s.label()),
                None => "Machine table: global".to_string(),
            };
            ui.label(RichText::new(short).size(11.5).color(th().text_dim)).on_hover_text(format!(
                "{}.\nLeads, micro-joints, joint gap, pierce and loop radius come from it; the part gap and margin are offered in Spacing.",
                mt.table_label()
            ));
        });
        ribbon_group(ui, "Machine", |ui| {
            if self.machine_picker(ui) {
                self.refresh();
            }
        });
        ribbon_group(ui, "Sheet", |ui| {
            egui::Grid::new("sheet_grid").num_columns(4).spacing([6.0, 5.0]).show(ui, |ui| {
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
                changed |= ui.add(egui::DragValue::new(&mut self.spec.w).range(200.0..=12000.0).clamp_existing_to_range(false).suffix(" mm")).changed();
                ui.end_row();
                ui.label("");
                ui.label("");
                ui.label("Width Y");
                changed |= ui.add(egui::DragValue::new(&mut self.spec.h).range(200.0..=3000.0).clamp_existing_to_range(false).suffix(" mm")).changed();
                ui.end_row();
            });
        });
        ribbon_group(ui, "Spacing", |ui| {
            egui::Grid::new("space_grid").num_columns(2).spacing([6.0, 5.0]).show(ui, |ui| {
                let u = self.cfg.nesting.units;
                ui.label("Edge margin").on_hover_text("Distance every part keeps from the sheet edge (type 1/4 or 6.35mm…)");
                changed |= crate::units::drag_len(ui, &mut self.spec.margin, u, 100.0).changed();
                ui.end_row();
                ui.label("Part gap").on_hover_text("Smallest distance between two parts (type 1/4 or 6.35mm…)");
                changed |= crate::units::drag_len(ui, &mut self.spec.gap, u, 50.0).changed();
                ui.end_row();
            });
            // The machine's gap and margin for this material and thickness
            // (MTL9): offered, never applied by themselves.
            let (g, m) = crate::engine::machine_spacing(&self.spec);
            if g.is_some() || m.is_some() {
                let fmt = |v: Option<f64>| v.map_or("—".to_string(), |v| format!("{v}"));
                let same = g.is_none_or(|g| (g - self.spec.gap).abs() < 1e-9) && m.is_none_or(|m| (m - self.spec.margin).abs() < 1e-9);
                let txt = format!("Machine: gap {} · margin {}", fmt(g), fmt(m));
                if same {
                    ui.label(RichText::new(txt).size(11.5).color(th().text_dim)).on_hover_text("The sheet already uses the machine's part gap and margin for this material and thickness.");
                } else if ui.small_button(RichText::new(format!("Use {txt}")).size(11.5)).on_hover_text("Take the machine's part gap and margin for this material and thickness (the widest of its four margins)").clicked() {
                    match crate::engine::use_machine_spacing(&mut self.spec, true, true) {
                        Ok(s) => {
                            self.status = s;
                            changed = true;
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
        });
        ribbon_group(ui, "Nest", |ui| {
            if ribbon_button_ex(ui, Icon::NestAll, "Nest\nall", false, !self.parts.is_empty(), true).clicked() {
                self.run_nesting();
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                // Three rows, as tall as a big button, so every tab's ribbon is the
                // same height; the methods count is in the level's tooltip.
                let cur = self.spec.nest_level().name;
                let level_tip = if self.result.method.is_empty() {
                    format!("{} methods tried. {}", self.spec.nest_level().methods, level_summary(self.spec.nest_level()))
                } else {
                    format!("{} methods tried. {}\n\nLast run won with: {}.", self.spec.nest_level().methods, level_summary(self.spec.nest_level()), self.result.method)
                };
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.add_space(2.0);
                ui.label(RichText::new("Automatic level").size(11.5).color(th().text_dim)).on_hover_text(level_tip.as_str());
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
            if ribbon_button(ui, Icon::Program, "CNC\nprogram", self.show_program, true).clicked() {
                self.show_program = !self.show_program;
            }
            if ribbon_button(ui, Icon::Times, "Times and\ncosts", self.show_times, true).clicked() {
                self.show_times = !self.show_times;
            }
            let resp = ribbon_button(ui, Icon::Panels, "Side\npanels", false, true)
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
        let n = self.result.sheets.len();
        ribbon_group(ui, "Sheets", |ui| {
            if ribbon_button(ui, Icon::Prev, "Previous\nsheet", false, self.cur_sheet > 0).on_hover_text("Show the previous sheet").clicked() {
                self.select_sheet(self.cur_sheet - 1);
            }
            if ribbon_button(ui, Icon::Next, "Next\nsheet", false, self.cur_sheet + 1 < n).on_hover_text("Show the next sheet").clicked() {
                self.select_sheet(self.cur_sheet + 1);
            }
        });
    }

    pub fn nest_tree_body(&mut self, ui: &mut egui::Ui) {
        let mut pick_sheet = None;
        let mut pick_part = None;
        let mut renest = None;
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
                        resp.header_response.context_menu(|ui| {
                            if ui.button("Nest this sheet again").on_hover_text("Only the parts already on this sheet; the other sheets stay as they are").clicked() {
                                renest = Some(i);
                                ui.close();
                            }
                        });
                    }
                });
        });
        if let Some(i) = pick_sheet {
            self.select_sheet(i);
        }
        if let Some(p) = pick_part {
            self.sel_part = Some(p);
        }
        if let Some(i) = renest {
            self.select_sheet(i);
            self.renest_sheet();
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
            if ui.small_button("📚 Save part to library").on_hover_text("Keep this part — shape, technology and texts — in the library, ready to add to any job (to save the whole job, use Save job)").clicked() {
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
                if ui.add(egui::DragValue::new(&mut p.w).range(0.5..=3000.0).clamp_existing_to_range(false).speed(1.0).suffix(" mm")).changed() {
                    p.h = p.w;
                    changed = true;
                }
                ui.end_row();
            } else {
                ui.label("Width");
                changed |= ui.add(egui::DragValue::new(&mut p.w).range(0.5..=6000.0).clamp_existing_to_range(false).speed(1.0).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Height");
                changed |= ui.add(egui::DragValue::new(&mut p.h).range(0.5..=3000.0).clamp_existing_to_range(false).speed(1.0).suffix(" mm")).changed();
                ui.end_row();
            }
            ui.label("Quantity");
            changed |= ui.add(egui::DragValue::new(&mut p.qty).range(0..=99999).clamp_existing_to_range(false)).changed();
            ui.end_row();
            ui.label("Rotation");
            changed |= ui.checkbox(&mut p.rotate, "may turn 90°").changed();
            ui.end_row();
            // The original's eight rotation permissions: 0/90/180/270°, plain and mirrored.
            ui.label("Angles");
            ui.horizontal(|ui| {
                // Narrow buttons, so all four fit a default-width pane.
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.spacing_mut().button_padding = Vec2::new(6.0, 3.0);
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
            // The machine's names and colours for its cut qualities (CQ4, CQ5).
            let names = crate::ct1::for_spec(&self.spec).qualities;
            egui::Grid::new("tech_edit").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Lead-in");
                changed |= ui.add(egui::DragValue::new(&mut t.lead_in).range(0.0..=30.0).clamp_existing_to_range(false).speed(0.1).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Lead-out");
                changed |= ui.add(egui::DragValue::new(&mut t.lead_out).range(0.0..=20.0).clamp_existing_to_range(false).speed(0.1).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Overlap").on_hover_text("Cut on past the start before the lead-out, so the contour closes cleanly");
                changed |= ui.add(egui::DragValue::new(&mut t.overlap).range(0.0..=10.0).clamp_existing_to_range(false).speed(0.05).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Micro-joints");
                changed |= ui.add(egui::DragValue::new(&mut t.micro_joints).range(0..=12).clamp_existing_to_range(false)).changed();
                ui.end_row();
                ui.label("Joint width");
                changed |= ui.add(egui::DragValue::new(&mut t.joint_width).range(0.1..=5.0).clamp_existing_to_range(false).speed(0.05).suffix(" mm")).changed();
                ui.end_row();
                ui.label("Cut quality").on_hover_text("The machine's quality for every contour the size ranges don't set; Auto lets the machine's size bands decide, so small holes are cut slower");
                let shown: egui::WidgetText = match t.quality {
                    Some(q) => crate::techui::quality_text(ui, &names, q).into(),
                    None => crate::techui::part_quality_label(None, &names).into(),
                };
                egui::ComboBox::from_id_salt("quality").width(130.0).selected_text(shown).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut t.quality, None, crate::techui::part_quality_label(None, &names)).changed();
                    for q in crate::techui::offered_qualities(&names, t.quality) {
                        changed |= ui.selectable_value(&mut t.quality, Some(q), crate::techui::quality_text(ui, &names, q)).changed();
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

    /// Placed parts on the shown sheet that overlap another part or stick out
    /// past the sheet's margin: (number of overlapping pairs, number off the
    /// sheet, the part types involved).
    pub fn sheet_problems(&self) -> (usize, usize, std::collections::HashSet<usize>) {
        let Some(sheet) = self.result.sheets.get(self.cur_sheet) else { return (0, 0, Default::default()) };
        let spec = self.cur_spec();
        let mut types: std::collections::HashSet<usize> = self.overlaps.iter().flat_map(|&(i, j)| [i, j]).filter_map(|i| sheet.get(i).map(|p| p.part)).collect();
        let off: Vec<&crate::nest::Placed> = sheet.iter().filter(|p| !spec.fits(p)).collect();
        types.extend(off.iter().map(|p| p.part));
        (self.overlaps.len(), off.len(), types)
    }

    /// Placed parts (indices on the shown sheet) that overlap another part or
    /// stick out past the sheet, in order.
    pub fn problem_parts(&self) -> Vec<usize> {
        let Some(sheet) = self.result.sheets.get(self.cur_sheet) else { return vec![] };
        let spec = self.cur_spec();
        let mut v: Vec<usize> = self.overlaps.iter().flat_map(|&(i, j)| [i, j]).filter(|&i| i < sheet.len()).collect();
        v.extend(sheet.iter().enumerate().filter(|(_, p)| !spec.fits(p)).map(|(i, _)| i));
        v.sort_unstable();
        v.dedup();
        v
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
            // A sheet with overlapping parts or parts off the sheet is never
            // "all nested", even when every part has a place.
            let (pairs, off, _) = self.sheet_problems();
            if pairs > 0 || off > 0 {
                let mut t = vec![];
                if pairs > 0 {
                    t.push(format!("{pairs} overlap{}", if pairs == 1 { "" } else { "s" }));
                }
                if off > 0 {
                    t.push(format!("{off} off the sheet"));
                }
                chip(ui, &format!("⚠ {}", t.join(" · ")), Color32::from_rgb(254, 226, 226), Color32::from_rgb(185, 28, 28))
                    .on_hover_text("This sheet can't be cut like this: move the red parts apart (or turn Collision on and nest again).");
            } else if placed >= need && need > 0 {
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
        let (_, _, trouble) = self.sheet_problems();
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
                    if self.scroll_to_part == Some(i) {
                        ui.scroll_to_rect(slot, Some(egui::Align::Center));
                    }
                    let hover = ui.ctx().animate_bool_with_time(resp.id.with("lift"), resp.hovered() || ui.rect_contains_pointer(slot), 0.12);
                    let r = Rect::from_min_size(slot.min + Vec2::new(0.0, 4.0 - 3.0 * hover), Vec2::new(86.0, 86.0));
                    let p = ui.painter();
                    let round = CornerRadius::same(8);
                    p.rect_filled(r.translate(Vec2::new(0.0, 2.0 + 2.0 * hover)), round, Color32::from_black_alpha((th().shadow as f32 * (0.6 + 0.8 * hover)) as u8));
                    p.rect_filled(r, round, th().card);
                    let head = Rect::from_min_size(r.min, Vec2::new(r.width(), 20.0));
                    // Red when a copy of this part overlaps or sticks out on the
                    // shown sheet; green when all are placed; orange when some wait.
                    let bad = trouble.contains(&i);
                    let (hbg, hfg) = if bad {
                        (Color32::from_rgb(254, 226, 226), Color32::from_rgb(185, 28, 28))
                    } else if pending == 0 {
                        (th().ok_bg, th().ok_fg)
                    } else {
                        (th().warn_bg, th().warn_fg)
                    };
                    p.rect_filled(head, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, hbg);
                    p.text(
                        head.left_center() + Vec2::new(4.0, 0.0),
                        Align2::LEFT_CENTER,
                        // Placed / needed, e.g. "4/4" with a tick, "1/4" in orange.
                        format!("{}/{}", part.qty.saturating_sub(pending), part.qty),
                        FontId::proportional(12.5),
                        hfg,
                    );
                    if bad {
                        p.text(head.right_center() - Vec2::new(10.0, 0.0), Align2::CENTER_CENTER, "!", FontId::proportional(14.0), hfg);
                    } else if pending == 0 {
                        check_mark(p, head.right_center() - Vec2::new(10.0, 0.0), 12.0);
                    }
                    if part.tech.is_some() {
                        // "T" badge: the part has its own technology.
                        let b = Rect::from_min_size(Pos2::new(r.right() - 22.0, head.bottom() + 4.0), Vec2::new(18.0, 16.0));
                        p.rect_filled(b, CornerRadius::same(4), th().accent);
                        p.text(b.center(), Align2::CENTER_CENTER, "T", FontId::proportional(11.0), Color32::WHITE);
                    }
                    // The part's reference at the foot of the tile, shortened to fit.
                    let mut label = part.name.clone();
                    let font = FontId::proportional(10.5);
                    while label.chars().count() > 3 && p.layout_no_wrap(label.clone(), font.clone(), th().text).size().x > r.width() - 10.0 {
                        label.pop();
                        while label.ends_with('…') {
                            label.pop();
                        }
                        label.push('…');
                    }
                    p.text(Pos2::new(r.center().x, r.bottom() - 8.0), Align2::CENTER_CENTER, label, font, th().text_dim);
                    let body = Rect::from_min_max(head.left_bottom() + Vec2::new(8.0, 5.0), r.max - Vec2::new(8.0, 18.0));
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
            self.scroll_to_part = None;
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
        let mut split = cnc::TimeSplit::default();
        for p in &self.plans {
            split += p.times;
        }
        let pierces: usize = self.plans.iter().map(|p| p.pierces).sum();
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        let placed = self.total_placed();
        let (m, t) = (self.spec.material, self.spec.thickness);
        // The answer first: how long the machine runs for the whole job.
        ui.horizontal(|ui| {
            ui.label(RichText::new(fmt_time(secs)).size(22.0).strong().color(th().text_strong));
            ui.label(RichText::new(format!("estimated machine time · {sheets} sheet(s)")).color(th().text_dim)).on_hover_text("Worked out from the cut length, rapids and pierces with the speeds noted at the bottom; the machine itself may differ.");
        });
        ui.add_space(6.0);
        section_heading(ui, "Whole job");
        egui::Grid::new("res_grid").num_columns(2).spacing([0.0, 3.0]).min_col_width(130.0).striped(true).show(ui, |ui| {
            row(ui, "Sheets", format!("{sheets}"));
            row(ui, "Parts nested", format!("{placed} / {need}"));
            row(ui, "Usage", format!("{util:.1} %"));
            row(ui, "Cut length", format!("{:.1} m", cut / 1000.0));
            row(ui, "Pierces", format!("{pierces}"));
            row(ui, "Machine time", fmt_time(secs));
            row(ui, "  Cutting", fmt_time(split.cut));
            if split.mark > 0.0 {
                row(ui, "  Marking", fmt_time(split.mark));
            }
            row(ui, "  Rapids", fmt_time(split.rapid));
            row(ui, "  Pierces", fmt_time(split.pierce));
            if split.head > 0.0 {
                row(ui, "  Head up / down", fmt_time(split.head));
            }
            row(ui, "Sheet weight", format!("{:.1} kg", cnc::weight_kg(m, self.total_sheet_area(), t)));
            row(ui, "Parts weight", format!("{:.1} kg", cnc::weight_kg(m, parts_area, t)));
        });
        if let Some(pl) = self.plans.get(self.cur_sheet) {
            ui.add_space(10.0);
            section_heading(ui, &format!("Sheet {} of {}", self.cur_sheet + 1, sheets));
            let (u, _, _) = self.sheet_usage(self.cur_sheet);
            let count = self.result.sheets[self.cur_sheet].len();
            egui::Grid::new("cur_grid").num_columns(2).spacing([0.0, 3.0]).min_col_width(130.0).striped(true).show(ui, |ui| {
                row(ui, "Parts", format!("{count}"));
                row(ui, "Usage", format!("{u:.1} %"));
                row(ui, "Cut length", format!("{:.1} m", pl.cut_len / 1000.0));
                row(ui, "Rapid moves", format!("{:.1} m", pl.rapid_len / 1000.0));
                row(ui, "Time", fmt_time(pl.seconds));
            });
        }
        ui.add_space(8.0);
        if let Some(pl) = self.plans.get(self.cur_sheet).or(self.plans.first()) {
            // How the times were worked out: readable, not fine print.
            ui.set_max_width(270.0);
            ui.label(RichText::new(pl.timing.speeds.label()).size(11.5).color(th().text_dim));
            ui.label(RichText::new(pl.timing.settings.label()).size(11.5).color(th().text_dim));
        }
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
        // The cursor position keeps its place on the right; the message on the
        // left gets what is left and is shortened with "…" (full text on hover).
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
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let (u, rem, cut_x) = self.sheet_usage(self.cur_sheet);
                let n = if self.result.sheets.is_empty() { 0 } else { self.cur_sheet + 1 };
                ui.label(format!("{n}"));
                ui.label(self.sheet_label(self.cur_sheet, 3));
                ui.add_space(8.0);
                ui.label(format!("Usage {u:.1} %"));
                let (bar, _) = ui.allocate_exact_size(Vec2::new(70.0, 6.0), Sense::hover());
                ui.painter().rect_filled(bar, CornerRadius::same(3), th().border);
                let mut fill = bar;
                fill.set_width(bar.width() * (u as f32 / 100.0).clamp(0.0, 1.0));
                ui.painter().rect_filled(fill, CornerRadius::same(3), th().accent);
                ui.add_space(8.0);
                // The remnant column of the sheet list (rule 2): the usage of the
                // part of the sheet up to where it is free, the remnant left out.
                ui.label(format!("without remnant {rem:.1} %")).on_hover_text("Usage of the sheet up to Free X — the leftover beyond it (the remnant) not counted");
                ui.add_space(8.0);
                ui.label(format!("Free X {cut_x:.0} mm")).on_hover_text("Where the sheet is free from: everything right of this X is left over");
                ui.add_space(12.0);
                // The sheet's CNC program name, as the original shows it.
                ui.label(format!("CNC: {}", self.cnc_name(self.cur_sheet + 1)));
                ui.add_space(16.0);
                if !self.status.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&self.status).color(th().text_dim)).truncate()).on_hover_text(&self.status);
                }
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

#[cfg(test)]
mod part_editor_tests {
    use crate::nest::Shape;
    use crate::App;
    use eframe::egui;

    #[test]
    fn looking_at_a_part_never_changes_it() {
        // A quantity over the box's old 999 and a disc under its old 10 mm.
        let mut a = App::new(None);
        let p = &mut a.parts[0];
        (p.shape, p.w, p.h, p.qty) = (Shape::Disc, 8.0, 8.0, 1000);
        a.sel_part = Some(0);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| a.part_editor(ui));
            out.textures_delta.clear();
        }
        assert_eq!((a.parts[0].qty, a.parts[0].w, a.parts[0].h), (1000, 8.0, 8.0));
    }
}

#[cfg(test)]
mod ribbon_fit_tests {
    use super::ribbon_fit_plan;

    #[test]
    fn groups_fold_least_needed_first_only_when_needed() {
        let full = [200.0, 300.0, 250.0, 150.0];
        let same = [5, 5, 5, 5];
        // Room for all: nothing folds.
        assert_eq!(ribbon_fit_plan(&full, &[], &same, 900.0), vec![false; 4]);
        // Equal priority: the rightmost folds first (84 px when not yet measured).
        assert_eq!(ribbon_fit_plan(&full, &[], &same, 850.0), vec![false, false, false, true]);
        assert_eq!(ribbon_fit_plan(&full, &[], &same, 800.0), vec![false, false, true, true]);
        // A measured folded width is used instead of the estimate.
        assert_eq!(ribbon_fit_plan(&full, &[0.0, 0.0, 70.0, 60.0], &same, 630.0), vec![false, false, true, true]);
        // The least needed group folds first wherever it is.
        assert_eq!(ribbon_fit_plan(&full, &[], &[0, 8, 8, 8], 850.0), vec![true, false, false, false]);
        // A wide group that had to fold leaves room: a smaller, less needed one
        // folded before it opens again.
        assert_eq!(ribbon_fit_plan(&[100.0, 400.0, 300.0], &[], &[0, 3, 9], 560.0), vec![false, true, false]);
        // Never measured yet (first frame of a ribbon): nothing to fold.
        assert!(ribbon_fit_plan(&[], &[], &[], 100.0).is_empty());
        // Too narrow for anything: all fold.
        assert_eq!(ribbon_fit_plan(&full, &[], &same, 50.0), vec![true; 4]);
    }
}
