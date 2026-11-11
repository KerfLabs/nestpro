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
// C008 255 255 0 (yellow) is the original's cut contour colour; here each
// contour is drawn in its cut quality's or its state's colour (CS7).
/// C011 255 255 255, style 2: the cross cursor.
pub const C_CURSOR: [u8; 3] = [255, 255, 255];
/// C053 0 255 0: marking (machined) texts.
pub const C_MARK: [u8; 3] = [0, 255, 0];
/// C057 0 200 0, style 2: contours already cut in the simulation.
pub const C_DONE: [u8; 3] = [0, 200, 0];
/// Drawing background (near-black navy, as in the original).
pub const C_BACKGROUND: [u8; 3] = [0, 0, 16];
/// The part's simulation speeds, × real time: slower than the sheet's, so a
/// lead-in can be followed into its contour (our choice).
pub const PART_SIM_SPEEDS: [f64; 5] = [0.1, 0.25, 0.5, 1.0, 5.0];
/// The part's default simulation speed: real time.
pub const PART_SIM_SPEED: f64 = 1.0;
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
    /// The last check of the shown part (Verify part / Verify machining):
    /// part, title, passed, findings. Shown on the drawing until closed.
    pub check: Option<(usize, String, bool, Vec<String>)>,
    /// Show the original's commands this rebuild doesn't have, greyed out. Off
    /// by default: the original leaves out commands that aren't available and
    /// drops groups and tabs left empty (rules RB4, RB5).
    pub show_unbuilt: bool,
    /// Label every contour with the standard shape it was recognised as.
    pub std_shapes: bool,
    pub measure: Vec<P>,
    pub zoom_window: bool,
    pub full_screen: bool,
    /// Simulation: seconds into the part's program (None = not simulating).
    pub sim: Option<f64>,
    pub sim_run: bool,
    pub sim_last: f64,
    /// Simulation speed, × real time (the Speed group).
    pub sim_speed: f64,
    /// Bring the head into view on the next frame (after Beginning,
    /// Previous, Next or End, when zoomed in).
    pub sim_follow: bool,
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
            check: None,
            show_unbuilt: false,
            std_shapes: false,
            measure: vec![],
            zoom_window: false,
            full_screen: false,
            sim: None,
            sim_run: false,
            sim_last: 0.0,
            sim_speed: PART_SIM_SPEED,
            sim_follow: false,
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
    format!("{name} — not available yet.")
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
    /// Some(on) for an on/off setting (shows a check box), None for a command.
    pub state: Option<bool>,
    /// One of several tools that exclude each other (shows a radio dot).
    pub radio: bool,
    /// A command's small picture (on/off settings show their box instead).
    pub icon: Option<Icon>,
    pub tip: String,
}

impl Mini {
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

pub fn act(label: &'static str, enabled: bool, tip: &str) -> Mini {
    Mini { label, enabled, state: None, radio: false, icon: None, tip: tip.to_string() }
}

pub fn toggle(label: &'static str, on: bool, tip: &str) -> Mini {
    Mini { label, enabled: true, state: Some(on), radio: false, icon: None, tip: tip.to_string() }
}

/// One of a set of tools of which only one is on at a time.
pub fn choice(label: &'static str, on: bool, tip: &str) -> Mini {
    Mini { label, enabled: true, state: Some(on), radio: true, icon: None, tip: tip.to_string() }
}

/// Height of a small ribbon command; three fill a ribbon group exactly.
pub const MINI_H: f32 = 22.0;

/// Small commands stacked three to a column, lined up with the big buttons:
/// text left-aligned, a soft highlight on hover, on/off settings with a check
/// box (ticked when on). Returns the clicked index.
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
                    let col = if !m.enabled { with_alpha(t.text_dim, 160) } else if on { t.accent_fg } else { t.text };
                    let dot = Pos2::new(r.left() + 11.0, r.center().y);
                    // On/off settings show a check box (radio dots are for one-of-several choices).
                    let bx = Rect::from_center_size(dot, Vec2::splat(11.0));
                    match m.state {
                        Some(on) if m.radio => {
                            p.circle_stroke(dot, 5.0, Stroke::new(1.2, if on { t.accent_fg } else { t.text_dim }));
                            if on {
                                p.circle_filled(dot, 2.8, t.accent_fg);
                            }
                        }
                        Some(true) => {
                            p.rect_filled(bx, CornerRadius::same(3), t.accent);
                            let s = Stroke::new(1.6, Color32::WHITE);
                            p.line_segment([dot + Vec2::new(-3.0, 0.2), dot + Vec2::new(-0.8, 2.6)], s);
                            p.line_segment([dot + Vec2::new(-0.8, 2.6), dot + Vec2::new(3.2, -2.4)], s);
                        }
                        Some(false) => {
                            p.rect_stroke(bx, CornerRadius::same(3), Stroke::new(1.2, t.text_dim), egui::StrokeKind::Inside);
                        }
                        // A command: its small picture, if it has one.
                        None => {
                            if let Some(ic) = m.icon {
                                let ir = Rect::from_center_size(dot + Vec2::new(1.0, 0.0), Vec2::splat(20.0));
                                let (ink, acc) = if m.enabled { (t.text, t.accent_fg) } else { (with_alpha(t.text_dim, 150), with_alpha(t.text_dim, 150)) };
                                // Drawn a little smaller than a big button's picture.
                                let p2 = p.clone().with_clip_rect(r.expand(2.0));
                                crate::ui_parts::draw_icon_with(&p2, ir, ic, ink, acc, false);
                            }
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
    // Under a one-line label; beside the last line of a two-line one.
    let c = if label.contains('\n') { Pos2::new(r.rect.right() - 6.0, r.rect.bottom() - 13.0) } else { Pos2::new(r.rect.center().x, r.rect.bottom() - 7.0) };
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
        ui.label(RichText::new("Not available yet.").size(11.0).color(th().text_dim));
    });
}

/// Problems found by "Verify Part": outline closed and simple, holes inside
/// the outline, holes apart from each other, texts on the part.
pub fn verify_part(part: &Part) -> Vec<String> {
    let mut out = cut_problems(part);
    let (w, h) = part.bbox();
    for t in &part.texts {
        if t.strokes(&t.text).iter().flatten().any(|q| q[0] < -1e-6 || q[1] < -1e-6 || q[0] > w + 1e-6 || q[1] > h + 1e-6) {
            out.push(format!("text '{}' sticks out of the part", t.text));
        }
    }
    out
}

/// Whether a closed contour crosses or touches itself away from its corners.
/// Edges are swept by x, so a long outline is checked in step with its size.
fn crosses_itself(poly: &[P]) -> bool {
    let n = poly.len();
    let mut segs: Vec<(f64, f64, usize)> = (0..n).map(|i| (poly[i][0].min(poly[(i + 1) % n][0]), poly[i][0].max(poly[(i + 1) % n][0]), i)).collect();
    segs.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (k, &(_, x1, i)) in segs.iter().enumerate() {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        for &(x0j, _, j) in &segs[k + 1..] {
            if x0j > x1 + 1e-9 {
                break;
            }
            // Neighbouring edges share a corner.
            if j == (i + 1) % n || i == (j + 1) % n {
                continue;
            }
            let (c, d) = (poly[j], poly[(j + 1) % n]);
            if a[1].max(b[1]) < c[1].min(d[1]) - 1e-9 || c[1].max(d[1]) < a[1].min(b[1]) - 1e-9 {
                continue;
            }
            if nest::seg_seg_dist(a, b, c, d) < 1e-9 {
                return true;
            }
        }
    }
    false
}

/// The part's geometry problems that would cut it wrong (JOB-41, VP1): an
/// outline that isn't closed or crosses itself, a hole that isn't wholly
/// inside the outline, holes that overlap. Export stops on these.
pub fn cut_problems(part: &Part) -> Vec<String> {
    let g = part.geometry();
    let mut out = vec![];
    if g.outline.len() < 3 {
        out.push("the outside contour is not closed".into());
        return out;
    }
    if crosses_itself(&g.outline) {
        out.push("the outside contour crosses itself".into());
    }
    // Holes are named by their contour numbers, as part-info lists them (CS1).
    let order = crate::dxf::cut_order(&crate::engine::contours::list(part));
    let no = |k: usize| order.get(k).map_or(k, |n| *n) + 1;
    let boxes: Vec<[f64; 4]> = g.holes.iter().map(|h| nest::bbox_of(h)).collect();
    for (k, h) in g.holes.iter().enumerate() {
        if !h.iter().all(|&q| nest::point_in_poly(&g.outline, q)) || nest::contours_within(h, &g.outline, 1e-6) {
            out.push(format!("hole {} is not inside the outside contour", no(k)));
        }
        for (j, h2) in g.holes.iter().enumerate().skip(k + 1) {
            let (a, b) = (boxes[k], boxes[j]);
            if a[2] < b[0] - 1e-6 || b[2] < a[0] - 1e-6 || a[3] < b[1] - 1e-6 || b[3] < a[1] - 1e-6 {
                continue;
            }
            if nest::contours_within(h, h2, 1e-6) || nest::point_in_poly(h2, h[0]) || nest::point_in_poly(h, h2[0]) {
                out.push(format!("holes {} and {} overlap", no(k), no(j)));
            }
        }
    }
    out
}

/// Pierce points off the sheet (MAC-46, VP2): a lead-in that starts outside
/// the sheet's edges (a lead longer than the margin), or a lead path that
/// leaves it. (contour number from 1, placed part index, the point.)
pub fn pierces_off_sheet(plan: &cnc::SheetPlan, w: f64, h: f64) -> Vec<(usize, usize, P)> {
    const TOL: f64 = 0.01;
    let off = |q: &P| q[0] < -TOL || q[1] < -TOL || q[0] > w + TOL || q[1] > h + TOL;
    plan.contours
        .iter()
        .enumerate()
        .filter(|(_, c)| c.punch.is_none())
        .filter_map(|(k, c)| std::iter::once(&c.lead_in).chain(&c.lead_path).find(|q| off(q)).map(|q| (k + 1, c.part_no.saturating_sub(1), *q)))
        .collect()
}

/// Problems found by "Verify the Machining": every contour pierced in its
/// scrap (outside the outline, inside a hole), holes cut before the outside, and
/// parts sitting in a hole cut before that hole (CO2).
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
    // CO2: a part cut after a hole it sits in drops with the hole's scrap.
    for (k, c) in plan.contours.iter().enumerate().filter(|(_, c)| c.inner) {
        let lo = c.points.iter().fold([f64::MAX; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
        let hi = c.points.iter().fold([f64::MIN; 2], |a, p| [a[0].max(p[0]), a[1].max(p[1])]);
        let mut late: Vec<usize> = vec![];
        for o in &plan.contours[k + 1..] {
            let Some(&q) = o.points.first() else { continue };
            if o.part_no != c.part_no && !late.contains(&o.part_no) && q[0] >= lo[0] && q[0] <= hi[0] && q[1] >= lo[1] && q[1] <= hi[1] && nest::point_in_poly(&c.points, q) {
                late.push(o.part_no);
            }
        }
        if !late.is_empty() {
            let list = late.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
            out.push(format!("contour {} (a hole of part {}) is cut before the part(s) sitting in it ({list}) — they may drop first", k + 1, c.part_no));
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

/// The part as the Drawing module shows it: each contour in its state or
/// cut quality's colour (CS7; Leave out ones too with `left_out`, while the
/// cut technology panel is open), marking texts in green, notes thin and grey.
pub fn paint_drawing_part(painter: &egui::Painter, part: &Part, mt: &crate::ct1::MachineTech, to_s: &dyn Fn(P) -> Pos2, texts: bool, left_out: bool) {
    crate::cutpanel::paint_contour_colours(painter, part, mt, to_s, 1.4, left_out);
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
            let r = dropdown(ui, Icon::Points, "Points", true).on_hover_text(format!("Access to points — snap: {}", self.dv.snap.label()));
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
            ribbon_button(ui, Icon::Remove, "Delete\ngeometry", false, false).on_hover_text(soon("Delete Geometry"));
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
        // Kept apart by kind: a check (a command), what is shown (on/off), and
        // the measuring tools (one at a time).
        ribbon_group(ui, "Check", |ui| {
            if ribbon_button(ui, Icon::CheckDrawing, "Check\ngeometry", false, true).on_hover_text("Check the drawing itself: its contours (closed, not crossing) and texts. To check the machining (lead-ins, pierces, cut order), use Machining → Verify machining.").clicked() {
                self.verify_part_now();
            }
        });
        ribbon_group(ui, "Show", |ui| {
            let items = [
                toggle("Time and cost", self.dv.info, "Show the cut length, pierces and time"),
                toggle("View geometry", self.dv.vertices, "Show every element's points and direction"),
            ];
            match minis(ui, &items) {
                Some(0) => self.dv.info = !self.dv.info,
                Some(1) => self.dv.vertices = !self.dv.vertices,
                _ => {}
            }
        });
        ribbon_group(ui, "Measure", |ui| {
            let dist = self.dv.measure_on && !self.dv.angle3;
            let ang = self.dv.measure_on && self.dv.angle3;
            let items = [
                choice("Distance and angle", dist, "Click two points (snaps to the Points mode)"),
                choice("Angle, 3 points", ang, "Angle between three points: a point, the corner, another point"),
            ];
            match minis(ui, &items).map(|k| k + 2) {
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
            if ribbon_button(ui, Icon::Ortho, "Orthogonal", self.dv.ortho, true).on_hover_text("Orthogonal: measured points go straight across or straight up").clicked() {
                self.dv.ortho = !self.dv.ortho;
            }
        });
        if self.dv.show_unbuilt {
            ribbon_group(ui, "Macros", |ui| {
                let r = dropdown(ui, Icon::Program, "Macros", true);
                soon_menu(&r, "Macros", &["Macros", "Record a macro"]);
            });
            // "Coming soon" sits with the other unavailable commands, not in
            // the everyday ribbon (Settings → General shows them).
            self.missing_group(ui);
        }
    }

    /// Ribbon group with the "What's still missing" button.
    fn missing_group(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Help", |ui| {
            let n: usize = MISSING.iter().map(|g| g.2.len()).sum();
            if ribbon_button(ui, Icon::Missing, "Coming\nsoon", self.dv.show_missing, true)
                .on_hover_text(format!("{n} drawing commands that aren't available yet — see the list"))
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
        egui::Window::new("Drawing commands coming soon")
            .open(&mut open)
            .default_size([560.0, 560.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("{total} commands of the original aren't built yet.")).color(th().text_dim));
                ui.checkbox(&mut self.dv.show_unbuilt, "Show them greyed out in the ribbon (the original leaves out commands that aren't available)");
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
        let summary = format!("{} contour(s), {} element(s), {} text(s)", g.holes.len() + 1, nest::elements_of(&g).len(), self.parts[i].texts.len());
        self.dv.check = Some((i, "Verify part".into(), probs.is_empty(), if probs.is_empty() { vec![format!("No problems — {summary}.")] } else { probs.clone() }));
        self.status = if probs.is_empty() {
            format!("Verify Part: OK — {} contour(s), {} element(s), {} text(s).", g.holes.len() + 1, nest::elements_of(&g).len(), self.parts[i].texts.len())
        } else {
            format!("Verify Part: {}.", probs.join("; "))
        };
    }

    fn ribbon_draw_machining(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Machining", |ui| {
            if ribbon_button_ex(ui, Icon::Toolpath, "Machine\npart", self.dv.machining, true, true).on_hover_text("Machine the part automatically: cut order, pierces, leads").clicked() {
                self.dv.machining = true;
                self.status = "Part machined automatically (holes first, then the outside).".into();
            }
            if ribbon_button(ui, Icon::Remove, "Clear\nmachining", false, self.dv.machining).clicked() {
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
            if ribbon_button(ui, Icon::Shapes, "Recognise\nshapes", self.dv.std_shapes, true)
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
            let r = dropdown(ui, Icon::Contours, "Contours", true);
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
            let r = dropdown(ui, Icon::Kerf, "Kerf", true).on_hover_text(crate::kerf::status_line(&self.spec));
            egui::Popup::menu(&r).show(|ui| self.kerf_menu(ui));
        });
        if self.dv.show_unbuilt {
        ribbon_group(ui, "Settings", |ui| {
            let r = dropdown(ui, Icon::Gear, "Options", true);
            soon_menu(&r, "Options", &["Optimize", "Speed rates", "Postprocessor words", "Contour attributes"]);
            let r = dropdown(ui, Icon::Export, "Extraction", true);
            soon_menu(&r, "Component Extraction", &["Automatic", "Manual stop", "Manual trap", "Manual lift", "Semiautomatic stop / trap / lift", "Assign extractions", "Assign boxes"]);
        });
        }
        // The same "Check" group as on the sheet's Machining tab.
        ribbon_group(ui, "Check", |ui| {
            if ribbon_button(ui, Icon::Verify, "Verify\nmachining", false, self.dv.machining).on_hover_text("Check pierces are in the scrap and holes are cut first").clicked() {
                self.verify_machining_now();
            }
            if ribbon_button(ui, Icon::Info, "Current\nstatus", self.dv.info, true).on_hover_text("Show the machining summary on the drawing").clicked() {
                self.dv.info = !self.dv.info;
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
    /// The machine the sheet is cut on (its kind: laser, plasma, punch…).
    pub fn machine_combo(&mut self, ui: &mut egui::Ui, width: f32) -> bool {
        let mut changed = false;
        let cur = self.spec.machine;
        egui::ComboBox::from_id_salt(("machine", ui.scope_id())).width(width).selected_text(cur.label()).show_ui(ui, |ui| {
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
        if changed {
            self.status = format!("Machine: {} — {}.", self.spec.machine.label(), self.spec.machine.describe().to_lowercase());
        }
        changed
    }

    pub fn machine_picker(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.spacing_mut().interact_size.y = 21.0;
            ui.add_space(6.0);
            changed = self.machine_combo(ui, 150.0);
            let fam = match self.spec.machine.family() {
                crate::machine::Family::Cut => "cuts every contour",
                crate::machine::Family::CutPunch => "punches standard holes",
                crate::machine::Family::Punch => "punches and nibbles",
            };
            ui.label(RichText::new(fam).size(11.0).color(th().text_dim)).on_hover_text(self.spec.machine.describe());
        });
        changed
    }

    pub fn verify_machining_now(&mut self) {
        let Some(i) = self.shown_part() else { return };
        let plan = cnc::plan_part(&self.parts, i, &self.spec);
        let probs = verify_machining(&plan);
        self.dv.check = Some((
            i,
            "Verify machining".into(),
            probs.is_empty(),
            if probs.is_empty() { vec![format!("No problems — {} contour(s), every pierce in the scrap, holes before the outside.", plan.contours.len())] } else { probs.clone() },
        ));
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
                toggle("Classic colours", self.dv.lantek_colors, "The original's colours: black background, yellow contours, green marking"),
                toggle("Machining window", self.show_program, "The part's machine program"),
                toggle("Full screen", self.dv.full_screen, "Hide the side panels"),
                act("Restore panels", true, "Put the side panels back where they started").with_icon(Icon::Panels),
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
            if ribbon_button(ui, Icon::Fit, "Zoom\nall", false, true).clicked() {
                self.push_part_view();
                self.part_zoom = 1.0;
                self.part_pan = Vec2::ZERO;
            }
            let items = [
                toggle("Zoom window", self.dv.zoom_window, "Drag a box to zoom to it"),
                act("Zoom previous", !self.part_view_hist.is_empty(), "Back to the view before").with_icon(Icon::Undo),
                act("Zoom in", true, "Zoom in").with_icon(Icon::ZoomIn),
                act("Zoom out", true, "Zoom out").with_icon(Icon::ZoomOut),
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
    }

    fn ribbon_draw_sim(&mut self, ui: &mut egui::Ui) {
        // The part's program, played like the sheet's: the head moves along
        // each beam run (pierce, lead-in, contour, lead-out) in real time.
        let (ops, n) = self.part_sim_ops();
        let total: f64 = ops.iter().map(|o| o.secs).sum();
        let on = self.dv.machining && !ops.is_empty();
        let now = ui.input(|i| i.time);
        ribbon_group(ui, "Simulate", |ui| {
            let t = self.dv.sim.unwrap_or(0.0);
            if ribbon_button(ui, Icon::SkipStart, "Beginning", false, on).clicked() {
                self.dv.sim = Some(0.0);
                self.dv.sim_run = false;
                self.dv.sim_follow = true;
            }
            if ribbon_button(ui, Icon::StepBack, "Previous", false, on && t > 0.0).on_hover_text("Back to the previous pierce").clicked() {
                self.dv.sim = Some(crate::sheetsim::step_time(&ops, t, -1));
                self.dv.sim_run = false;
                self.dv.sim_follow = true;
            }
            if ribbon_button_ex(ui, if self.dv.sim_run { Icon::Pause } else { Icon::Play }, if self.dv.sim_run { "Pause" } else { "Run" }, self.dv.sim_run, on, true).clicked() {
                if self.dv.sim.is_none_or(|s| s >= total - 1e-6) {
                    self.dv.sim = Some(0.0);
                }
                self.dv.sim_run = !self.dv.sim_run;
                self.dv.sim_last = now;
            }
            if ribbon_button(ui, Icon::StepFwd, "Next", false, on && t < total - 1e-6).on_hover_text("On to the next pierce").clicked() {
                self.dv.sim = Some(crate::sheetsim::step_time(&ops, t, 1));
                self.dv.sim_run = false;
                self.dv.sim_follow = true;
            }
            if ribbon_button(ui, Icon::SkipEnd, "End", false, on).clicked() {
                self.dv.sim = Some(total);
                self.dv.sim_run = false;
                self.dv.sim_follow = true;
            }
            if ribbon_button(ui, Icon::Stop, "Stop", false, self.dv.sim.is_some()).clicked() {
                self.dv.sim = None;
                self.dv.sim_run = false;
            }
            ui.vertical(|ui| {
                ui.add_space(10.0);
                let txt = match self.dv.sim {
                    Some(t) => format!("{} of {n} contours cut", crate::sheetsim::contours_done(&ops, n, t)),
                    None => format!("{n} contours"),
                };
                ui.label(RichText::new(txt).size(12.0).color(th().text_dim));
                if let Some(t) = self.dv.sim {
                    ui.label(RichText::new(format!("{t:.1} of {total:.1} s")).size(12.0).color(th().text_dim));
                }
            });
        });
        // Like the sheet's Speed group (× real time), with slow motion.
        ribbon_group(ui, "Speed", |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);
                let label = |s: f64| if s < 1.0 { format!("{s}× real time") } else { format!("{s:.0}× real time") };
                egui::ComboBox::from_id_salt("part_sim_speed").width(110.0).selected_text(label(self.dv.sim_speed)).show_ui(ui, |ui| {
                    for s in PART_SIM_SPEEDS {
                        if ui.selectable_label((self.dv.sim_speed - s).abs() < 1e-9, label(s)).clicked() {
                            self.dv.sim_speed = s;
                        }
                    }
                });
                ui.label(RichText::new("Simulation speed").size(12.0).color(th().text_dim));
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
        ribbon_group(ui, "Dimension notes", |ui| {
            if ribbon_button(ui, Icon::Dimension, "Auto\ndimensions", self.dv.dims, true).on_hover_text("Overall width and height of the part").clicked() {
                self.dv.dims = true;
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, self.dv.dims).clicked() {
                self.dv.dims = false;
            }
        });
        ribbon_group(ui, "Texts", |ui| {
            if ribbon_button(ui, Icon::Text, "Texts", false, true).on_hover_text("Write a text on the part").clicked() {
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
            if ribbon_button(ui, Icon::Text, "Texts and\ndimensions", self.dv.texts_dims, true).clicked() {
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
            let r = dropdown(ui, Icon::Bevel, "Bevels", true);
            soon_menu(&r, "Bevels", &["Cut of double or triple bevels"]);
            let r = dropdown(ui, Icon::Contours, "Contours", true);
            soon_menu(&r, "Contours", &["Partial cut of a contour", "Add arcs in pockets", "Add arcs in pockets manually"]);
            let r = dropdown(ui, Icon::Copy, "Compare", true);
            soon_menu(&r, "Geometry", &["Compare geometries", "Bitmap", "Mosaic", "Open Drafting"]);
            let r = dropdown(ui, Icon::Export, "Lifting", true);
            soon_menu(&r, "Two-head lifting", &["Search all lift positions", "Delete all lift positions"]);
        });
        self.missing_group(ui);
    }

    /// Simulation: advance the part's program clock while running (the
    /// program lasts `total` seconds at real time).
    pub fn sim_tick(&mut self, now: f64, total: f64, ctx: &egui::Context) {
        if !self.dv.sim_run {
            return;
        }
        ctx.request_repaint();
        let t = (self.dv.sim.unwrap_or(0.0) + (now - self.dv.sim_last).clamp(0.0, 0.25) * self.dv.sim_speed).min(total);
        self.dv.sim_last = now;
        self.dv.sim = Some(t);
        if t >= total {
            self.dv.sim_run = false;
            self.status = format!("Simulation finished: {total:.1} s of machining.");
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
