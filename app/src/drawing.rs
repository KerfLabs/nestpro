//! Opening a part: the original's Drawing module (ribbon in
//! Ui\Drawing.xml). It has its own tab set — Home (drawing and editing),
//! Technology, Machining, View, Simulation, Notes, Utilities — and its own look:
//! the part on a black background, contours in the cut colour, marking texts in
//! green, red dashed axes through the origin and a full-screen cross cursor.
//! See MECHANICS.md "Opening a part (Drawing module)".

use crate::nest::{self, ElemKind, Part, TextKind, P};
use crate::theme::th;
use crate::ui_parts::*;
use crate::{cnc, App, Tab, Workspace};
use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

// Colours from the original's LANTEKIN.CNI (lines "Cnnn R G B style width",
// rule DR2).
/// C002 255 0 0, style 2 (dashed): the axes.
pub const C_AXES: [u8; 3] = [255, 0, 0];
/// C008 255 255 0: cut contours.
pub const C_CONTOUR: [u8; 3] = [255, 255, 0];
/// C011 255 255 255, style 2: the cross cursor.
pub const C_CURSOR: [u8; 3] = [255, 255, 255];
/// C053 0 255 0: marking (machined) texts.
pub const C_MARK: [u8; 3] = [0, 255, 0];
/// C057 0 200 0, style 2: contours already cut in the simulation.
pub const C_DONE: [u8; 3] = [0, 200, 0];
/// Drawing background (near-black navy, as in the original).
pub const C_BACKGROUND: [u8; 3] = [0, 0, 16];
/// Simulation: seconds per contour when running.
pub const SIM_STEP_S: f64 = 0.7;
/// Snap aperture for the measuring tool (screen px).
pub const SNAP_PX: f32 = 10.0;

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Point snap modes ("Access to points", the original's point types).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Snap {
    Picked,
    End,
    Mid,
    Center,
    Near,
}

impl Snap {
    pub const ALL: [Snap; 5] = [Snap::Picked, Snap::End, Snap::Mid, Snap::Center, Snap::Near];
    pub fn label(self) -> &'static str {
        match self {
            Snap::Picked => "Picked point",
            Snap::End => "End point",
            Snap::Mid => "Mid-point",
            Snap::Center => "Center point",
            Snap::Near => "Near point",
        }
    }
}

/// State of the drawing view.
#[derive(Clone, Debug)]
pub struct DrawView {
    pub axes: bool,
    /// The original's colours (black background, contour/marking colours).
    pub lantek_colors: bool,
    pub dims: bool,
    pub texts_dims: bool,
    /// Machining (cut order, pierces) shown; "Delete instructions" hides it.
    pub machining: bool,
    pub info: bool,
    pub vertices: bool,
    pub ortho: bool,
    pub snap: Snap,
    pub measure_on: bool,
    /// "Angle between three points" instead of distance and angle.
    pub angle3: bool,
    /// The "What's still missing" window.
    pub show_missing: bool,
    /// Show the original's commands this rebuild doesn't have, greyed out. Off
    /// by default: the original leaves out commands that aren't available and
    /// drops groups and tabs left empty (rules RB4, RB5).
    pub show_unbuilt: bool,
    /// Label every contour with the standard shape it was recognised as.
    pub std_shapes: bool,
    pub measure: Vec<P>,
    pub zoom_window: bool,
    pub full_screen: bool,
    /// Simulation: how many contours are cut so far.
    pub sim: Option<usize>,
    pub sim_run: bool,
    pub sim_last: f64,
    pub cursor: Option<P>,
}

impl Default for DrawView {
    fn default() -> Self {
        DrawView {
            axes: true,
            lantek_colors: true,
            dims: true,
            texts_dims: true,
            machining: true,
            info: true,
            vertices: false,
            ortho: false,
            snap: Snap::End,
            measure_on: false,
            angle3: false,
            show_missing: false,
            show_unbuilt: false,
            std_shapes: false,
            measure: vec![],
            zoom_window: false,
            full_screen: false,
            sim: None,
            sim_run: false,
            sim_last: 0.0,
            cursor: None,
        }
    }
}

/// What the original's Drawing module has that this rebuild doesn't yet, tab by
/// tab and group by group (Ui\Drawing.xml with the original's captions).
pub const MISSING: &[(&str, &str, &[&str])] = &[
    ("Home", "Draw", &["Delete Geometry", "2D points", "Line segments", "Lines", "Continuous line/arc", "Arc segments", "Full circles", "Squares", "Rectangles", "Modified rectangles", "Polygons", "Obround slots", "Radial slots", "Ovals", "Ellipses", "Pulleys"]),
    ("Home", "Access to points", &["Center point contour", "Intersection point", "Double intersection", "Tangent point", "Projected point", "Quadrant point", "Mixed point"]),
    ("Home", "Modify", &["Break elements", "Fillets", "Chamfer", "Offset", "Lengthen elements", "Join elements", "Fitting", "Shorten-lengthen", "Join and fillet"]),
    ("Home", "One-touch Figures", &["One-touch figures 1–12"]),
    ("Home", "Transform", &["Copy", "Move", "Stretch", "Rotate", "Scale", "Place Near", "Mirror", "Break Parts", "Union", "Subtraction", "Intersection", "Change Cutting Direction", "Contour Ordering", "Transform One-Touch Figures"]),
    ("Home", "Review", &["Detect part", "View Verify Geometry Points"]),
    ("Home", "Macros", &["Macros", "Record a macro"]),
    ("Technology", "Lead-in/Lead-outs", &["12 more lead types (16 in all)", "Dynamic lead-in/outs", "Custom lead-in/outs", "Move point of lead-in or lead-out", "Lead-out like the lead-in", "Add element", "Tangent arc by 2 segments", "Lead-in/out in bisector"]),
    ("Technology", "Micro-Joints", &["Nano-joints", "Move point of a micro-joint lead", "Transform to geometry"]),
    ("Technology", "Loops", &["Second loop type", "Round angles"]),
    ("Technology", "Bevel", &["Standard bevel", "Bevel on contour", "3D bevel (lower face, connections)"]),
    ("Technology", "Cut Technology", &["Marking", "No cut", "Clean cut", "Cutting (per contour)", "Micro-joint", "Dynamic", "Auxiliary geometry", "Hatch pattern", "Bending geometry", "Colour per technology type"]),
    ("Technology", "Punch Technology", &["Punches, nibbling, shear (punch machines)"]),
    ("Technology", "Texts", &["Written text", "QR codes", "Templates", "Vectorize text"]),
    ("Machining", "Machining", &["Initialize", "Postprocess", "Rapids (dry run)", "Goto"]),
    ("Machining", "Contours", &["Manual Contours (in the drawing)", "All Drill Cycles", "All Marks", "All Contours of one Quality", "Automatic Cycles", "Flying cut in holes", "Manual / automatic / semi-automatic hole destruction", "Hatch pattern machining"]),
    ("Machining", "Cycles", &["Simple, circular, linear, grid and irregular cycles", "Milling pockets and nibbling"]),
    ("Machining", "Modify", &["Order machining", "Micro-joints at the end of the contour", "Regenerate", "Modify lead-in/outs", "Avoid holes already cut", "Continuous cut", "Copy / transform machining", "Modify compensation"]),
    ("Machining", "Options", &["Optimize", "Speed rates", "Postprocessor words", "Contour attributes"]),
    ("Machining", "Component Extraction", &["Automatic", "Manual stop / trap / lift", "Semiautomatic stop / trap / lift", "Assign extractions", "Assign boxes"]),
    ("View", "Display", &["Part Machining List", "Simulation Window", "Machining Explorer", "3D view", "Change co-ordinates", "Auxiliary geometry", "Bending geometry", "Redraw"]),
    ("View", "Zoom", &["Zoom Dynamic", "Zoom Scale", "Zoom Base", "Define limits"]),
    ("Simulation", "Simulate", &["Program (step by G-code line)", "ISN position", "Mouse positioning", "Previous / next tool", "Previous / next reposition", "Multihead", "Previous / next extraction"]),
    ("Notes", "Dimension Notes", &["Semi-automatic", "Dimension note styles 1–7", "Configuration", "Change data", "Modify dimension", "Dimension size", "Dimension constraints", "Explode dimensions"]),
    ("Utilities", "Utilities", &["Cut of double or triple bevels", "Partial cut of a contour", "Open Drafting", "Compare Geometries", "Bitmap", "Mosaic", "Search all lift positions", "Delete all lift positions", "Add arcs in pockets", "Add arcs in pockets manually"]),
];

/// What the rebuild already has in the Drawing module.
pub const BUILT: &[(&str, &str)] = &[
    ("Home", "Back/previous/next part, Access to points (picked, end, mid, centre, near), Verify Part, Time and cost, View Geometry, Distance and Angle, Angle between three points, Orthogonal"),
    ("Technology", "Lead-ins (4 types, keypad position, on element), micro-joints (gap, leads, corner; edit leads), loops, cut quality, marking / special / note texts (curved), apply to selected or all parts"),
    ("Machining", "Automatic, Delete instructions, Contours (Automatic, Parts, All Cut), CN and system (kerf) compensation, Current status, Verify the Machining"),
    ("View", "Axis, Color Display, Machining Window, Restore Panels, Full Screen, Show Parts, Zoom All / Window / Previous / In / Out"),
    ("Simulation", "Beginning, Previous, Run/Pause, Next, End, Stop"),
    ("Notes", "Automatic / Delete dimension notes, Texts, Modify text, Text and Dimensions"),
];

fn soon(name: &str) -> String {
    format!("{name} — in the original's Drawing module; not in this rebuild yet.")
}

/// Is the Utilities tab shown? None of its commands are built, so — like the
/// original dropping a tab with no groups (rule RB5) —
/// it only appears when unbuilt commands are shown.
pub fn utility_tab_shown(v: &DrawView) -> bool {
    v.show_unbuilt
}

/// One small ribbon command (the original's small-icon rows).
pub struct Mini {
    pub label: &'static str,
    pub enabled: bool,
    /// Some(on) for an on/off setting (shows a dot), None for a command.
    pub state: Option<bool>,
    pub tip: String,
}

pub fn act(label: &'static str, enabled: bool, tip: &str) -> Mini {
    Mini { label, enabled, state: None, tip: tip.to_string() }
}

pub fn toggle(label: &'static str, on: bool, tip: &str) -> Mini {
    Mini { label, enabled: true, state: Some(on), tip: tip.to_string() }
}

/// Height of a small ribbon command; three fill a ribbon group exactly.
pub const MINI_H: f32 = 22.0;

/// Small commands stacked three to a column, lined up with the big buttons:
/// text left-aligned, a soft highlight on hover, on/off settings with a dot
/// (filled when on). Returns the clicked index.
pub fn minis(ui: &mut egui::Ui, items: &[Mini]) -> Option<usize> {
    let t = th();
    let mut hit = None;
    // Columns go straight into the group's row (a nested row would be
    // centred in a taller line and sit lower than the big buttons).
    {
        for (c, chunk) in items.chunks(3).enumerate() {
            if c > 0 {
                ui.add_space(4.0);
            }
            let font = FontId::proportional(12.0);
            let colw = chunk.iter().map(|m| ui.painter().layout_no_wrap(m.label.to_string(), font.clone(), t.text).size().x).fold(0.0, f32::max) + 30.0;
            // The column is exactly as tall as a big ribbon button (3 rows + 2 gaps).
            let (col, _) = ui.allocate_exact_size(Vec2::new(colw, 3.0 * MINI_H + 4.0), egui::Sense::hover());
            {
                for (k, m) in chunk.iter().enumerate() {
                    let sense = if m.enabled { egui::Sense::click() } else { egui::Sense::hover() };
                    let r = Rect::from_min_size(Pos2::new(col.left(), col.top() + k as f32 * (MINI_H + 2.0)), Vec2::new(colw, MINI_H));
                    let resp = ui.interact(r, ui.scope_id().with(("mini", c, k, m.label)), sense);
                    let on = m.state == Some(true);
                    let hover = ui.ctx().animate_bool_with_time(resp.id.with("mh"), m.enabled && resp.hovered(), 0.1);
                    let p = ui.painter();
                    if on {
                        p.rect_filled(r, CornerRadius::same(6), t.accent_soft);
                    } else if hover > 0.0 {
                        p.rect_filled(r, CornerRadius::same(6), with_alpha(t.hover, (hover * 255.0) as u8));
                    }
                    let col = if !m.enabled { with_alpha(t.text_dim, 120) } else if on { t.accent_fg } else { t.text };
                    let dot = Pos2::new(r.left() + 11.0, r.center().y);
                    match m.state {
                        Some(true) => {
                            p.circle_filled(dot, 4.0, t.accent_fg);
                        }
                        Some(false) => {
                            p.circle_stroke(dot, 3.5, Stroke::new(1.2, with_alpha(t.text_dim, 200)));
                        }
                        None => {
                            // A small chevron marks a command.
                            let s = Stroke::new(1.4, col);
                            p.line_segment([dot + Vec2::new(-2.0, -3.5), dot + Vec2::new(1.5, 0.0)], s);
                            p.line_segment([dot + Vec2::new(1.5, 0.0), dot + Vec2::new(-2.0, 3.5)], s);
                        }
                    }
                    p.text(Pos2::new(r.left() + 22.0, r.center().y), egui::Align2::LEFT_CENTER, m.label, font.clone(), col);
                    let resp = resp.on_hover_text(m.tip.as_str());
                    if resp.clicked() {
                        hit = Some(c * 3 + k);
                    }
                }
            }
        }
    }
    hit
}

/// A big ribbon button that opens a menu (small caret under its label).
pub fn dropdown(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> egui::Response {
    let r = ribbon_button(ui, icon, label, false, enabled);
    let c = Pos2::new(r.rect.center().x, r.rect.bottom() - 7.0);
    let col = if enabled { th().text_dim } else { with_alpha(th().text_dim, 110) };
    ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-3.5, -2.0), c + Vec2::new(3.5, -2.0), c + Vec2::new(0.0, 2.0)], col, Stroke::NONE));
    r
}

/// The menu of a dropdown whose commands aren't built yet: listed greyed out,
/// each with a "not yet" tag, so the original's whole group is visible.
pub fn soon_menu(r: &egui::Response, title: &str, items: &[&str]) {
    egui::Popup::menu(r).show(|ui| {
        ui.label(RichText::new(title).strong().color(th().text_strong));
        ui.add_space(2.0);
        for name in items {
            ui.horizontal(|ui| {
                ui.add_enabled(false, egui::Button::new(*name).frame(false)).on_disabled_hover_text(soon(name));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    chip(ui, "not yet", th().hover, th().text_dim);
                });
            });
        }
        ui.add_space(4.0);
        ui.label(RichText::new("In the original's Drawing module — not in this rebuild yet.").size(11.0).color(th().text_dim));
    });
}

/// Problems found by "Verify Part": outline closed and simple, holes inside
/// the outline, holes apart from each other, texts on the part.
pub fn verify_part(part: &Part) -> Vec<String> {
    let g = part.geometry();
    let mut out = vec![];
    if g.outline.len() < 3 {
        out.push("the outside contour is not closed".into());
        return out;
    }
    let crosses = |poly: &[P]| {
        let n = poly.len();
        (0..n).any(|i| (i + 2..n).any(|j| (i, j) != (0, n - 1) && nest::seg_seg_dist(poly[i], poly[(i + 1) % n], poly[j], poly[(j + 1) % n]) < 1e-9))
    };
    if crosses(&g.outline) {
        out.push("the outside contour crosses itself".into());
    }
    for (k, h) in g.holes.iter().enumerate() {
        if !h.iter().all(|&q| nest::point_in_poly(&g.outline, q)) || nest::contours_within(h, &g.outline, 1e-6) {
            out.push(format!("hole {} is not inside the outside contour", k + 1));
        }
        for (j, h2) in g.holes.iter().enumerate().skip(k + 1) {
            if nest::contours_within(h, h2, 1e-6) || nest::point_in_poly(h2, h[0]) || nest::point_in_poly(h, h2[0]) {
                out.push(format!("holes {} and {} overlap", k + 1, j + 1));
            }
        }
    }
    let (w, h) = part.bbox();
    for t in &part.texts {
        if t.strokes(&t.text).iter().flatten().any(|q| q[0] < -1e-6 || q[1] < -1e-6 || q[0] > w + 1e-6 || q[1] > h + 1e-6) {
            out.push(format!("text '{}' sticks out of the part", t.text));
        }
    }
    out
}

/// Problems found by "Verify the Machining": every contour pierced in its
/// scrap (outside the outline, inside a hole) and holes cut before the outside.
pub fn verify_machining(plan: &cnc::SheetPlan) -> Vec<String> {
    let mut out = vec![];
    // Parts whose outside is already cut (holes are checked per part, so a
    // sheet with many parts isn't flagged for the next part's holes).
    let mut outer_done = std::collections::HashSet::new();
    for (k, c) in plan.contours.iter().enumerate() {
        let inside = nest::point_in_poly(&c.points, c.lead_in);
        let on = nest::contours_within(&[c.lead_in], &c.points, 1e-6);
        if c.inner && !inside && !on {
            out.push(format!("contour {} (a hole) is pierced outside the hole", k + 1));
        }
        if !c.inner && inside {
            out.push(format!("contour {} (the outside) is pierced on the part", k + 1));
        }
        if c.inner && outer_done.contains(&c.part_no) {
            out.push(format!("contour {} (a hole) is cut after the outside — the part may drop first", k + 1));
        }
        if !c.inner {
            outer_done.insert(c.part_no);
        }
    }
    // KC18 (our check): what system compensation can't cut.
    for (part, inner, what) in &plan.kerf_issues {
        out.push(format!("part {part}: {} {what}", if *inner { "a hole —" } else { "the outside —" }));
    }
    out
}

/// Snap a point to the part's elements by the chosen mode.
pub fn snap_point(elems: &[nest::Element], w: P, tol: f64, mode: Snap) -> P {
    let d = |a: P, b: P| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let best = |cands: Vec<P>| cands.into_iter().filter(|&c| d(c, w) <= tol).min_by(|a, b| d(*a, w).total_cmp(&d(*b, w))).unwrap_or(w);
    match mode {
        Snap::Picked => w,
        Snap::End => best(elems.iter().flat_map(|e| [e.pts[0], *e.pts.last().unwrap_or(&e.pts[0])]).collect()),
        Snap::Mid => best(elems.iter().map(|e| e.midpoint()).collect()),
        Snap::Center => best(
            elems
                .iter()
                .filter(|e| matches!(e.kind, ElemKind::Arc | ElemKind::Circle) && e.pts.len() >= 3)
                .filter_map(|e| crate::partview::circle_through(e.pts[0], e.pts[e.pts.len() / 3], e.pts[2 * e.pts.len() / 3]).map(|c| c.0))
                .collect(),
        ),
        Snap::Near => {
            let mut b = (f64::MAX, w);
            for e in elems {
                for s in e.pts.windows(2) {
                    let (a, c) = (s[0], s[1]);
                    let v = [c[0] - a[0], c[1] - a[1]];
                    let l2 = (v[0] * v[0] + v[1] * v[1]).max(1e-18);
                    let t = (((w[0] - a[0]) * v[0] + (w[1] - a[1]) * v[1]) / l2).clamp(0.0, 1.0);
                    let q = [a[0] + v[0] * t, a[1] + v[1] * t];
                    if d(q, w) < b.0 {
                        b = (d(q, w), q);
                    }
                }
            }
            if b.0 <= tol { b.1 } else { w }
        }
    }
}

/// Angle at `v` between the lines to `a` and to `c` (degrees, 0..180) —
/// "Angle between three points" (the original's command of that name).
pub fn angle3(a: P, v: P, c: P) -> f64 {
    let (u, w) = ([a[0] - v[0], a[1] - v[1]], [c[0] - v[0], c[1] - v[1]]);
    let (lu, lw) = ((u[0] * u[0] + u[1] * u[1]).sqrt(), (w[0] * w[0] + w[1] * w[1]).sqrt());
    if lu < 1e-12 || lw < 1e-12 {
        return 0.0;
    }
    ((u[0] * w[0] + u[1] * w[1]) / (lu * lw)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Decimals shown by the original's "Distance and Angle" box (its four
/// read-only fields are created with precision 2, rule DR7).
pub const MEASURE_DECIMALS: usize = 2;

/// Distance, dX, dY and angle (degrees, 0..360) from a to b.
pub fn measure(a: P, b: P) -> (f64, f64, f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    ((dx * dx + dy * dy).sqrt(), dx, dy, nest::norm_angle(dy.atan2(dx).to_degrees()))
}

/// The part as the Drawing module shows it: contours as lines in the cut
/// colour, marking texts in green, notes thin and grey.
pub fn paint_drawing_part(painter: &egui::Painter, part: &Part, to_s: &dyn Fn(P) -> Pos2, texts: bool) {
    let g = part.geometry();
    let st = Stroke::new(1.4, rgb(C_CONTOUR));
    painter.add(egui::Shape::closed_line(g.outline.iter().map(|&p| to_s(p)).collect(), st));
    for h in &g.holes {
        painter.add(egui::Shape::closed_line(h.iter().map(|&p| to_s(p)).collect(), st));
    }
    if let Some(c) = &part.custom {
        for m in &c.marks {
            painter.add(egui::Shape::line(m.iter().map(|&p| to_s(p)).collect(), Stroke::new(1.2, rgb(C_MARK))));
        }
    }
    if !texts {
        return;
    }
    for t in &part.texts {
        let s = match t.kind {
            TextKind::Mark | TextKind::Special => Stroke::new(1.3, rgb(C_MARK)),
            TextKind::Note => Stroke::new(1.0, Color32::from_gray(150)),
        };
        for line in t.strokes(&t.text) {
            painter.add(egui::Shape::line(line.iter().map(|&p| to_s(p)).collect(), s));
        }
    }
}

impl App {
    /// The ribbon while a part is open: the Drawing module's own tabs.
    pub fn ribbon_drawing(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Technology => self.ribbon_part_tech(ui),
            Tab::Machining => self.ribbon_draw_machining(ui),
            Tab::View => self.ribbon_draw_view(ui),
            Tab::Simulation => self.ribbon_draw_sim(ui),
            Tab::Notes => self.ribbon_draw_notes(ui),
            Tab::Utilities => self.ribbon_draw_utilities(ui),
            _ => self.ribbon_draw_home(ui),
        }
    }

    fn ribbon_draw_home(&mut self, ui: &mut egui::Ui) {
        let many = self.open_set.len() > 1;
        ribbon_group(ui, "Parts", |ui| {
            if ribbon_button(ui, Icon::Prev, "Back to\nnesting", false, true).clicked() {
                self.workspace = Workspace::Nesting;
                self.tab = Tab::Home;
            }
            if ribbon_button(ui, Icon::Prev, "Previous\npart", false, many).clicked() {
                self.open_part_step(-1);
            }
            if ribbon_button(ui, Icon::Next, "Next\npart", false, many).clicked() {
                self.open_part_step(1);
            }
        });
        ribbon_group(ui, "Draw", |ui| {
            let r = dropdown(ui, Icon::Fit, "Points", true).on_hover_text(format!("Access to points — snap: {}", self.dv.snap.label()));
            egui::Popup::menu(&r).show(|ui| {
                ui.label(RichText::new("Access to points").strong().color(th().text_strong));
                for m in Snap::ALL {
                    if ui.selectable_label(self.dv.snap == m, m.label()).clicked() {
                        self.dv.snap = m;
                    }
                }
                let more: &[&str] = if self.dv.show_unbuilt { &["Center point contour", "Intersection point", "Double intersection", "Tangent point", "Projected point", "Quadrant point", "Mixed point"] } else { &[] };
                if !more.is_empty() {
                    ui.separator();
                }
                for &name in more {
                    ui.add_enabled(false, egui::Button::new(name).frame(false)).on_disabled_hover_text(soon(name));
                }
            });
            if !self.dv.show_unbuilt {
                return;
            }
            let r = dropdown(ui, Icon::AddPart, "Shapes", true);
            soon_menu(&r, "Draw", &["2D points", "Line segments", "Lines", "Continuous line/arc", "Arc segments", "Full circles", "Squares", "Rectangles", "Modified rectangles", "Polygons", "Obround slots", "Radial slots", "Ovals", "Ellipses", "Pulleys"]);
            ribbon_button(ui, Icon::Remove, "Delete\nGeometry", false, false).on_hover_text(soon("Delete Geometry"));
        });
        if self.dv.show_unbuilt {
        ribbon_group(ui, "Edit geometry", |ui| {
            let r = dropdown(ui, Icon::Joint, "Modify", true);
            soon_menu(&r, "Modify", &["Break elements", "Fillets", "Chamfer", "Offset", "Lengthen elements", "Join elements", "Fitting", "Shorten-lengthen", "Join and fillet"]);
            let r = dropdown(ui, Icon::Rotate, "Transform", true);
            soon_menu(&r, "Transform", &["Copy", "Move", "Stretch", "Rotate", "Scale", "Place Near", "Mirror", "Break Parts", "Union", "Subtraction", "Intersection", "Change Cutting Direction", "Contour Ordering", "Transform One-Touch Figures"]);
            let r = dropdown(ui, Icon::Grid, "Figures", true);
            soon_menu(&r, "One-touch Figures", &["One-touch figures 1–12"]);
        });
        }
        ribbon_group(ui, "Review", |ui| {
            if ribbon_button(ui, Icon::Collision, "Verify\nPart", false, true).on_hover_text("Check the part's contours and texts").clicked() {
                self.verify_part_now();
            }
            let dist = self.dv.measure_on && !self.dv.angle3;
            let ang = self.dv.measure_on && self.dv.angle3;
            let items = [
                toggle("Time and cost", self.dv.info, "Show the cut length, pierces and time"),
                toggle("View Geometry", self.dv.vertices, "Show every element's points and direction"),
                toggle("Distance and Angle", dist, "Click two points (snaps to the Points mode)"),
                toggle("Angle, 3 points", ang, "Angle between three points: a point, the corner, another point"),
            ];
            match minis(ui, &items) {
                Some(0) => self.dv.info = !self.dv.info,
                Some(1) => self.dv.vertices = !self.dv.vertices,
                Some(k @ (2 | 3)) => {
                    let three = k == 3;
                    self.dv.measure_on = !(self.dv.measure_on && self.dv.angle3 == three);
                    self.dv.angle3 = three;
                    self.dv.measure.clear();
                    self.dv.zoom_window = false;
                    self.status = match (self.dv.measure_on, three) {
                        (false, _) => String::new(),
                        (true, false) => format!("Distance and angle: click two points ({}). Esc stops.", self.dv.snap.label()),
                        (true, true) => format!("Angle between three points: click a point, the corner, then another point ({}). Esc stops.", self.dv.snap.label()),
                    };
                }
                _ => {}
            }
            if ribbon_button(ui, Icon::Grid, "Ortho-\ngonal", self.dv.ortho, true).on_hover_text("Orthogonal: measured points go straight across or straight up").clicked() {
                self.dv.ortho = !self.dv.ortho;
            }
        });
        if self.dv.show_unbuilt {
            ribbon_group(ui, "Macros", |ui| {
                let r = dropdown(ui, Icon::Program, "Macros", true);
                soon_menu(&r, "Macros", &["Macros", "Record a macro"]);
            });
        }
        self.missing_group(ui);
    }

    /// Ribbon group with the "What's still missing" button.
    fn missing_group(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Rebuild", |ui| {
            let n: usize = MISSING.iter().map(|g| g.2.len()).sum();
            if ribbon_button(ui, Icon::Numbers, "Still\nmissing", self.dv.show_missing, true)
                .on_hover_text(format!("{n} commands of the original's Drawing module aren't built yet — see the list"))
                .clicked()
            {
                self.dv.show_missing = !self.dv.show_missing;
            }
        });
    }

    /// Window listing what the original's Drawing module has and this rebuild
    /// doesn't yet, tab by tab, with what is already built.
    pub fn missing_window(&mut self, ctx: &egui::Context) {
        if !self.dv.show_missing {
            return;
        }
        let mut open = true;
        let total: usize = MISSING.iter().map(|g| g.2.len()).sum();
        egui::Window::new("What's still missing in the Drawing module")
            .open(&mut open)
            .default_size([560.0, 560.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("{total} commands of the original aren't built yet.")).color(th().text_dim));
                ui.checkbox(&mut self.dv.show_unbuilt, "Show them greyed out in the ribbon (the original leaves out commands that aren't available)");
                if ui.button("What Lantek v44 added, and what it would take…").clicked() {
                    self.show_v44 = true;
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut last_tab = "";
                    for (tab, group, items) in MISSING {
                        if *tab != last_tab {
                            ui.add_space(6.0);
                            let built = BUILT.iter().find(|b| b.0 == *tab).map_or("", |b| b.1);
                            let n: usize = MISSING.iter().filter(|g| g.0 == *tab).map(|g| g.2.len()).sum();
                            ui.label(RichText::new(format!("{tab}  ·  {n} missing")).strong().size(14.0).color(th().text_strong));
                            if !built.is_empty() {
                                ui.label(RichText::new(format!("Built: {built}")).size(11.5).color(th().ok_fg));
                            }
                            last_tab = tab;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{group}:")).strong());
                            ui.label(items.join(" · "));
                        });
                    }
                });
            });
        if !open {
            self.dv.show_missing = false;
        }
    }

    /// Status line: how many contours of the shown part are standard shapes.
    pub fn report_std_shapes(&mut self) {
        let Some(i) = self.shown_part() else { return };
        let g = self.parts[i].geometry();
        let all: Vec<&Vec<P>> = g.holes.iter().chain(std::iter::once(&g.outline)).collect();
        let found: Vec<String> = all.iter().filter_map(|c| crate::machine::recognise(c)).map(|s| s.label()).collect();
        self.status = if found.is_empty() {
            format!("No standard shapes among the {} contour(s).", all.len())
        } else {
            format!("{} of {} contour(s) are standard shapes: {}.", found.len(), all.len(), found.join(", "))
        };
    }

    pub fn verify_part_now(&mut self) {
        let Some(i) = self.shown_part() else { return };
        let probs = verify_part(&self.parts[i]);
        let g = self.parts[i].geometry();
        self.status = if probs.is_empty() {
            format!("Verify Part: OK — {} contour(s), {} element(s), {} text(s).", g.holes.len() + 1, nest::elements_of(&g).len(), self.parts[i].texts.len())
        } else {
            format!("Verify Part: {}.", probs.join("; "))
        };
    }

    fn ribbon_draw_machining(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Machining", |ui| {
            if ribbon_button_ex(ui, Icon::CutPath, "Automatic", self.dv.machining, true, true).on_hover_text("Machine the part automatically: cut order, pierces, leads").clicked() {
                self.dv.machining = true;
                self.status = "Part machined automatically (holes first, then the outside).".into();
            }
            if ribbon_button(ui, Icon::Remove, "Delete\ninstructions", false, self.dv.machining).clicked() {
                self.dv.machining = false;
                self.dv.sim = None;
                self.status = "Machining deleted — Automatic machines the part again.".into();
            }
        });
        ribbon_group(ui, "Machine", |ui| {
            if self.machine_picker(ui) {
                self.refresh();
            }
        });
        ribbon_group(ui, "Standard shapes", |ui| {
            if ribbon_button(ui, Icon::Fit, "Recognise\nshapes", self.dv.std_shapes, true)
                .on_hover_text("Label each contour that is a standard shape (circle, rectangle, obround slot) — what a punch tool or cycle can do in one go")
                .clicked()
            {
                self.dv.std_shapes = !self.dv.std_shapes;
                if self.dv.std_shapes {
                    self.report_std_shapes();
                }
            }
        });
        ribbon_group(ui, "Contours", |ui| {
            let r = dropdown(ui, Icon::CutTech, "Contours", true);
            egui::Popup::menu(&r).show(|ui| {
                ui.label(RichText::new("Contours").strong().color(th().text_strong));
                for (name, on) in [
                    ("Manual Contours", false),
                    ("Automatic Contours", true),
                    ("Parts", true),
                    ("All Drill Cycles", false),
                    ("All Marks", false),
                    ("All Cut", true),
                    ("All Contours of one Quality", false),
                    ("Automatic Cycles", false),
                    ("Flying cut in holes", false),
                    ("Manual hole destruction", false),
                    ("Automatic hole destruction", false),
                    ("Semi-automatic hole destruction", false),
                    ("Hatch pattern machining", false),
                ] {
                    if on {
                        if ui.button(name).clicked() {
                            self.dv.machining = true;
                            self.status = format!("{name}: every contour machined in the automatic order.");
                        }
                    } else if self.dv.show_unbuilt {
                        ui.add_enabled(false, egui::Button::new(name).frame(false)).on_disabled_hover_text(soon(name));
                    }
                }
            });
            if !self.dv.show_unbuilt {
                return;
            }
            let r = dropdown(ui, Icon::Loop, "Cycles", true);
            soon_menu(&r, "Cycles", &["Simple cycle", "Circular cycle", "Linear cycle", "Grid cycle", "Irregular cycle", "Milling pockets", "Milling nibblings"]);
        });
        ribbon_group(ui, "Compensation", |ui| {
            let r = dropdown(ui, Icon::CutPath, "Kerf", true).on_hover_text(crate::kerf::status_line(&self.spec));
            egui::Popup::menu(&r).show(|ui| self.kerf_menu(ui));
        });
        if self.dv.show_unbuilt {
        ribbon_group(ui, "Settings", |ui| {
            let r = dropdown(ui, Icon::Numbers, "Options", true);
            soon_menu(&r, "Options", &["Optimize", "Speed rates", "Postprocessor words", "Contour attributes"]);
            let r = dropdown(ui, Icon::Export, "Extraction", true);
            soon_menu(&r, "Component Extraction", &["Automatic", "Manual stop", "Manual trap", "Manual lift", "Semiautomatic stop / trap / lift", "Assign extractions", "Assign boxes"]);
        });
        }
        ribbon_group(ui, "View", |ui| {
            if ribbon_button(ui, Icon::Times, "Current\nstatus", self.dv.info, true).on_hover_text("Show the machining summary on the drawing").clicked() {
                self.dv.info = !self.dv.info;
            }
            if ribbon_button(ui, Icon::Collision, "Verify\nMachining", false, self.dv.machining).on_hover_text("Check pierces are in the scrap and holes are cut first").clicked() {
                self.verify_machining_now();
            }
        });
    }

    /// Machining → Compensation → Kerf: the job's control / system
    /// compensation (KC1), each defaulting to the machine file's.
    pub fn kerf_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Kerf compensation").strong().color(th().text_strong));
        ui.label(RichText::new(crate::kerf::status_line(&self.spec)).size(11.5).color(th().text_dim));
        let mt = crate::ct1::for_spec(&self.spec);
        let mut control = self.spec.kerf.control.unwrap_or(mt.comp.control);
        let mut system = self.spec.kerf.system.unwrap_or(mt.comp.system);
        let mut changed = false;
        if ui.checkbox(&mut control, "CN compensation").on_hover_text("The program writes G41/G42 and the corrector number; the CNC offsets the drawn path by the radius in that corrector").changed() {
            self.spec.kerf.control = Some(control);
            changed = true;
        }
        if ui.checkbox(&mut system, "System compensation").on_hover_text("The program carries the path already offset by the cutter radius (no G41/G42): corners, leads, joints and the pierce move with it").changed() {
            self.spec.kerf.system = Some(system);
            changed = true;
        }
        if ui.add_enabled(self.spec.kerf != Default::default(), egui::Button::new("Use the machine's settings")).clicked() {
            self.spec.kerf = Default::default();
            changed = true;
        }
        if self.dv.show_unbuilt {
            ui.add_enabled(false, egui::Button::new("Modify compensation").frame(false)).on_disabled_hover_text(soon("Modify compensation (per contour)"));
        }
        if changed {
            self.refresh();
            self.status = format!("{}.", crate::kerf::status_line(&self.spec));
        }
    }

    /// Machine choice (combo + what it does). Returns true when it changed.
    pub fn machine_picker(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.spacing_mut().interact_size.y = 21.0;
            ui.add_space(6.0);
            let cur = self.spec.machine;
            egui::ComboBox::from_id_salt(("machine", ui.scope_id())).width(150.0).selected_text(cur.label()).show_ui(ui, |ui| {
                for (k, group) in [("Cutting", crate::machine::Family::Cut), ("Punch + cut", crate::machine::Family::CutPunch), ("Punching", crate::machine::Family::Punch)] {
                    ui.label(RichText::new(k).size(11.0).color(th().text_dim));
                    for m in crate::machine::MachineKind::ALL.into_iter().filter(|m| m.family() == group) {
                        if ui.selectable_label(cur == m, m.label()).on_hover_text(m.describe()).clicked() && cur != m {
                            self.spec.machine = m;
                            changed = true;
                        }
                    }
                }
            });
            let fam = match self.spec.machine.family() {
                crate::machine::Family::Cut => "cuts every contour",
                crate::machine::Family::CutPunch => "punches standard holes",
                crate::machine::Family::Punch => "punches and nibbles",
            };
            ui.label(RichText::new(fam).size(11.0).color(th().text_dim)).on_hover_text(self.spec.machine.describe());
        });
        if changed {
            self.status = format!("Machine: {} — {}.", self.spec.machine.label(), self.spec.machine.describe().to_lowercase());
        }
        changed
    }

    pub fn verify_machining_now(&mut self) {
        let Some(i) = self.shown_part() else { return };
        let (w, h) = self.parts[i].bbox();
        let pl = nest::Placed { part: i, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false };
        let plan = cnc::plan(&self.parts, &[pl], &self.spec);
        let probs = verify_machining(&plan);
        self.status = if probs.is_empty() {
            format!("Verify the Machining: OK — {} contour(s), every pierce in the scrap, holes before the outside.", plan.contours.len())
        } else {
            format!("Verify the Machining: {}.", probs.join("; "))
        };
    }

    fn ribbon_draw_view(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Display", |ui| {
            let items = [
                toggle("Axis", self.dv.axes, "Red dashed axes through the drawing origin"),
                toggle("Color Display", self.dv.lantek_colors, "The original's colours: black background, yellow contours, green marking"),
                toggle("Machining Window", self.show_program, "The part's machine program"),
                toggle("Full Screen", self.dv.full_screen, "Hide the side panels"),
                act("Restore Panels", true, "Put the side panels back where they started"),
            ];
            let items: Vec<Mini> = if self.dv.show_unbuilt { items.into_iter().chain([act("3D view", false, &soon("3D view of the part"))]).collect() } else { items.into_iter().collect() };
            match minis(ui, &items) {
                Some(0) => self.dv.axes = !self.dv.axes,
                Some(1) => self.dv.lantek_colors = !self.dv.lantek_colors,
                Some(2) => self.show_program = !self.show_program,
                Some(3) => self.dv.full_screen = !self.dv.full_screen,
                Some(4) => {
                    self.dock.reset(Workspace::Part);
                    self.dv.full_screen = false;
                }
                _ => {}
            }
            let many = self.open_set.len() > 1;
            let r = dropdown(ui, Icon::Next, "Parts", many).on_hover_text("Show parts: first, previous, next, last");
            egui::Popup::menu(&r).show(|ui| {
                let n = self.open_set.len();
                for (label, to) in [("First part", Some(0)), ("Previous part", None), ("Next part", None), ("Last part", Some(n.saturating_sub(1)))] {
                    if ui.button(label).clicked() {
                        match (label, to) {
                            (_, Some(k)) => {
                                self.open_idx = k;
                                self.sel_part = self.open_set.get(k).copied();
                            }
                            ("Previous part", _) => self.open_part_step(-1),
                            _ => self.open_part_step(1),
                        }
                    }
                }
            });
        });
        ribbon_group(ui, "Zoom", |ui| {
            if ribbon_button(ui, Icon::Fit, "Zoom\nAll", false, true).clicked() {
                self.push_part_view();
                self.part_zoom = 1.0;
                self.part_pan = Vec2::ZERO;
            }
            let items = [
                toggle("Zoom Window", self.dv.zoom_window, "Drag a box to zoom to it"),
                act("Zoom Previous", !self.part_view_hist.is_empty(), "Back to the view before"),
                act("Zoom In", true, "Zoom in"),
                act("Zoom Out", true, "Zoom out"),
            ];
            match minis(ui, &items) {
                Some(0) => {
                    self.dv.zoom_window = !self.dv.zoom_window;
                    self.dv.measure_on = false;
                }
                Some(1) => self.previous_part_view(),
                Some(2) => {
                    self.push_part_view();
                    self.part_zoom = (self.part_zoom * crate::partview::ZOOM_STEP).min(crate::partview::PART_ZOOM_MAX);
                }
                Some(3) => {
                    self.push_part_view();
                    self.part_zoom = (self.part_zoom * (1.0 / crate::partview::ZOOM_STEP)).max(crate::partview::PART_ZOOM_MIN);
                }
                _ => {}
            }
        });
        ribbon_group(ui, "Appearance", |ui| {
            let dark = crate::theme::is_dark();
            if ribbon_button(ui, Icon::Sun, "Light", !dark, true).clicked() {
                crate::theme::set_dark(ui.ctx(), false);
                self.follow_system = false;
            }
            if ribbon_button(ui, Icon::Moon, "Dark", dark, true).clicked() {
                crate::theme::set_dark(ui.ctx(), true);
                self.follow_system = false;
            }
        });
    }

    fn ribbon_draw_sim(&mut self, ui: &mut egui::Ui) {
        let n = self.shown_part().map_or(0, |i| {
            let g = self.parts[i].geometry();
            g.holes.len() + 1
        });
        let on = self.dv.machining;
        ribbon_group(ui, "Simulate", |ui| {
            let k = self.dv.sim.unwrap_or(0);
            if ribbon_button(ui, Icon::Prev, "Beginning", false, on).clicked() {
                self.dv.sim = Some(0);
                self.dv.sim_run = false;
            }
            if ribbon_button(ui, Icon::Undo, "Previous", false, on && k > 0).clicked() {
                self.dv.sim = Some(k.saturating_sub(1));
                self.dv.sim_run = false;
            }
            if ribbon_button_ex(ui, Icon::NestAll, if self.dv.sim_run { "Pause" } else { "Run" }, self.dv.sim_run, on, true).clicked() {
                if self.dv.sim.is_none_or(|s| s >= n) {
                    self.dv.sim = Some(0);
                }
                self.dv.sim_run = !self.dv.sim_run;
            }
            if ribbon_button(ui, Icon::Redo, "Next", false, on && k < n).clicked() {
                self.dv.sim = Some((k + 1).min(n));
                self.dv.sim_run = false;
            }
            if ribbon_button(ui, Icon::Next, "End", false, on).clicked() {
                self.dv.sim = Some(n);
                self.dv.sim_run = false;
            }
            if ribbon_button(ui, Icon::Clear, "Stop", false, self.dv.sim.is_some()).clicked() {
                self.dv.sim = None;
                self.dv.sim_run = false;
            }
            ui.vertical(|ui| {
                ui.add_space(10.0);
                let txt = match self.dv.sim {
                    Some(k) => format!("{k} of {n} contours cut"),
                    None => format!("{n} contours"),
                };
                ui.label(RichText::new(txt).size(12.0).color(th().text_dim));
            });
        });
        if !self.dv.show_unbuilt {
            return;
        }
        ribbon_group(ui, "More", |ui| {
            let r = dropdown(ui, Icon::Times, "Steps", true);
            soon_menu(&r, "Simulate by", &["Program (G-code line)", "ISN position", "Mouse positioning", "Previous / next tool", "Previous / next reposition", "Multihead", "Previous / next extraction"]);
        });
    }

    fn ribbon_draw_notes(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Dimension Notes", |ui| {
            if ribbon_button(ui, Icon::Fit, "Automatic", self.dv.dims, true).on_hover_text("Overall width and height of the part").clicked() {
                self.dv.dims = true;
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, self.dv.dims).clicked() {
                self.dv.dims = false;
            }
        });
        ribbon_group(ui, "Texts", |ui| {
            if ribbon_button(ui, Icon::CutTech, "Texts", false, true).on_hover_text("Write a text on the part").clicked() {
                let at = ui.ctx().content_rect().center();
                self.open_text_editor(at, crate::textedit::TextTarget::New { on_element: false });
            }
            let one = self.sel_texts.len() == 1;
            if ribbon_button(ui, Icon::Copy, "Modify", false, one).on_hover_text("Edit the selected text (or double-click a text)").clicked() {
                if let (Some(i), Some(&k)) = (self.shown_part(), self.sel_texts.first()) {
                    let at = ui.ctx().content_rect().center();
                    self.edit_text_at(at, i, k);
                }
            }
        });
        ribbon_group(ui, "View", |ui| {
            if ribbon_button(ui, Icon::Numbers, "Text and\nDimensions", self.dv.texts_dims, true).clicked() {
                self.dv.texts_dims = !self.dv.texts_dims;
            }
        });
    }

    fn ribbon_draw_utilities(&mut self, ui: &mut egui::Ui) {
        if !self.dv.show_unbuilt {
            self.missing_group(ui);
            return;
        }
        ribbon_group(ui, "Utilities", |ui| {
            let r = dropdown(ui, Icon::Compact, "Bevels", true);
            soon_menu(&r, "Bevels", &["Cut of double or triple bevels"]);
            let r = dropdown(ui, Icon::CutPath, "Contours", true);
            soon_menu(&r, "Contours", &["Partial cut of a contour", "Add arcs in pockets", "Add arcs in pockets manually"]);
            let r = dropdown(ui, Icon::Copy, "Compare", true);
            soon_menu(&r, "Geometry", &["Compare geometries", "Bitmap", "Mosaic", "Open Drafting"]);
            let r = dropdown(ui, Icon::Export, "Lifting", true);
            soon_menu(&r, "Two-head lifting", &["Search all lift positions", "Delete all lift positions"]);
        });
        self.missing_group(ui);
    }

    /// Simulation: advance while running.
    pub fn sim_tick(&mut self, now: f64, n: usize, ctx: &egui::Context) {
        if !self.dv.sim_run {
            return;
        }
        ctx.request_repaint();
        if now - self.dv.sim_last >= SIM_STEP_S {
            self.dv.sim_last = now;
            let k = self.dv.sim.unwrap_or(0) + 1;
            self.dv.sim = Some(k.min(n));
            if k >= n {
                self.dv.sim_run = false;
                self.status = format!("Simulation finished: {n} contours cut.");
            }
        }
    }

    /// Axes, cross cursor and measuring overlay on the drawing.
    pub fn paint_drawing_overlays(&self, painter: &egui::Painter, rect: Rect, to_s: &dyn Fn(P) -> Pos2, hover: Option<Pos2>) {
        if self.dv.axes {
            // Two full-view lines through the origin (DR1).
            let o = to_s([0.0, 0.0]);
            let st = Stroke::new(1.0, rgb(C_AXES));
            painter.extend(egui::Shape::dashed_line(&[Pos2::new(rect.left(), o.y), Pos2::new(rect.right(), o.y)], st, 6.0, 4.0));
            painter.extend(egui::Shape::dashed_line(&[Pos2::new(o.x, rect.top()), Pos2::new(o.x, rect.bottom())], st, 6.0, 4.0));
        }
        if let (true, Some(hp)) = (self.dv.lantek_colors, hover) {
            let st = Stroke::new(1.0, with_alpha(rgb(C_CURSOR), 150));
            painter.line_segment([Pos2::new(rect.left(), hp.y), Pos2::new(rect.right(), hp.y)], st);
            painter.line_segment([Pos2::new(hp.x, rect.top()), Pos2::new(hp.x, rect.bottom())], st);
        }
        // Distance and angle.
        let pts = &self.dv.measure;
        if self.dv.angle3 {
            let col = Color32::from_rgb(56, 220, 230);
            let mut all: Vec<P> = pts.clone();
            if all.len() < 3 {
                if let Some(c) = self.dv.cursor {
                    all.push(c);
                }
            }
            for w in all.windows(2) {
                painter.line_segment([to_s(w[0]), to_s(w[1])], Stroke::new(1.6, col));
            }
            for &q in &all {
                painter.circle_stroke(to_s(q), 4.0, Stroke::new(1.5, col));
            }
            if all.len() == 3 {
                let a = angle3(all[0], all[1], all[2]);
                let txt = format!("Angle {a:.prec$}°", prec = MEASURE_DECIMALS);
                let g = painter.layout_no_wrap(txt, FontId::proportional(12.5), Color32::WHITE);
                let r = Rect::from_min_size(to_s(all[1]) + Vec2::new(14.0, -26.0), g.size() + Vec2::new(14.0, 8.0));
                painter.rect_filled(r, CornerRadius::same(6), th().overlay);
                painter.galley(r.min + Vec2::new(7.0, 4.0), g, Color32::WHITE);
            }
            return;
        }
        if let Some(&a) = pts.first() {
            let b = pts.get(1).copied().or(self.dv.cursor).unwrap_or(a);
            let col = Color32::from_rgb(56, 220, 230);
            painter.line_segment([to_s(a), to_s(b)], Stroke::new(1.6, col));
            painter.circle_stroke(to_s(a), 4.0, Stroke::new(1.5, col));
            painter.circle_stroke(to_s(b), 4.0, Stroke::new(1.5, col));
            let (d, dx, dy, ang) = measure(a, b);
            let p = MEASURE_DECIMALS;
            let txt = format!("D {d:.p$}   dX {dx:.p$}   dY {dy:.p$}   A {ang:.p$}°");
            let g = painter.layout_no_wrap(txt, FontId::proportional(12.5), Color32::WHITE);
            let at = to_s(b) + Vec2::new(14.0, -26.0);
            let r = Rect::from_min_size(at, g.size() + Vec2::new(14.0, 8.0));
            painter.rect_filled(r, CornerRadius::same(6), th().overlay);
            painter.galley(r.min + Vec2::new(7.0, 4.0), g, Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod drawing_tests {
    use super::*;
    use crate::nest::Shape;

    fn part(shape: Shape) -> Part {
        Part { name: "P".into(), shape, w: 200.0, h: 120.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn built_in_parts_verify_clean() {
        for sh in Shape::ALL {
            if sh == Shape::Custom {
                continue;
            }
            assert!(verify_part(&part(sh)).is_empty(), "{sh:?}: {:?}", verify_part(&part(sh)));
        }
    }

    #[test]
    fn a_text_off_the_part_is_reported() {
        let mut p = part(Shape::Rectangle);
        p.texts.push(nest::PartText { text: "LONG TEXT".into(), at: [190.0, 60.0], height: 20.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark });
        assert!(verify_part(&p).iter().any(|m| m.contains("sticks out")));
    }

    #[test]
    fn automatic_machining_verifies() {
        let parts = vec![part(Shape::PlateHole), part(Shape::Flange)];
        let spec = nest::SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        for i in 0..parts.len() {
            let pl = nest::Placed { part: i, x: 0.0, y: 0.0, w: 200.0, h: 120.0, turns: 0, mirror: false };
            let plan = cnc::plan(&parts, &[pl], &spec);
            assert!(verify_machining(&plan).is_empty(), "{:?}", verify_machining(&plan));
        }
    }

    #[test]
    fn unbuilt_commands_are_left_out_by_default() {
        // The original adds only available controls and drops empty groups/tabs.
        assert!(!DrawView::default().show_unbuilt);
        assert!(!utility_tab_shown(&DrawView::default()));
        let mut v = DrawView::default();
        v.show_unbuilt = true;
        assert!(utility_tab_shown(&v));
    }

    #[test]
    fn three_point_angle_and_missing_list() {
        assert!((angle3([10.0, 0.0], [0.0, 0.0], [0.0, 5.0]) - 90.0).abs() < 1e-9);
        assert!((angle3([1.0, 0.0], [0.0, 0.0], [-1.0, 0.0]) - 180.0).abs() < 1e-9);
        // Every tab of the Drawing module is covered, and nothing listed as built is also missing.
        for tab in ["Home", "Technology", "Machining", "View", "Simulation", "Notes", "Utilities"] {
            assert!(MISSING.iter().any(|g| g.0 == tab), "{tab}");
        }
        assert!(MISSING.iter().all(|g| !g.2.contains(&"Distance and Angle")));
    }

    #[test]
    fn measuring_snaps_and_reads_angles() {
        let g = part(Shape::Rectangle).geometry();
        let el = nest::elements_of(&g);
        assert_eq!(snap_point(&el, [198.0, 2.0], 5.0, Snap::End), [200.0, 0.0]);
        let (d, _, _, a) = measure([0.0, 0.0], [0.0, 10.0]);
        assert!((d - 10.0).abs() < 1e-9 && (a - 90.0).abs() < 1e-9);
    }
}
