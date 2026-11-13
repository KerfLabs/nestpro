//! The Part view: one part opened on its own to apply technology, the way the
//! original opens parts in its drawing module.
//!
//! Original flow (see MECHANICS.md "Opening a part", O1–O6): the "open part"
//! command writes the chosen part and the list of parts to open with it to
//! PIEZADIB.SYS and PIEZADIBCODES.SYS, asks first if those parts don't share
//! machine, material and thickness, then starts or brings forward the original's
//! drawing/nesting program in its drawing mode.

use crate::engine::selection::{Take, Window};
use crate::nest::P;
use crate::theme::th;
use crate::ui_parts::*;
use crate::{cnc, App, OpenScope, Tab, Workspace};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, Vec2};


/// Half-size of the capture square around the cursor, in screen pixels (estimate;
/// the original's caller passes the pick aperture in, rule E1).
pub(crate) const APERTURE_PX: f32 = 6.0;
/// Part view zoom limits (× the fitted size).
pub const PART_ZOOM_MIN: f32 = 0.3;
pub const PART_ZOOM_MAX: f32 = 40.0;
/// Free room around the drawing when it is fitted (screen px).
pub const PART_FIT_MARGIN_PX: f32 = 64.0;
/// Height kept free at the bottom for the Simulation Window while the part
/// is simulated (the window and the legend under it).
pub const SIM_WINDOW_BAND_PX: f32 = 150.0;
/// Zoom step of the - / + buttons; zooming in divides by the same factor
/// (rule V1 uses 1.0 / f).
pub const ZOOM_STEP: f32 = 1.25;
/// Wheel zoom speed, per scrolled pixel (exp, so in and out undo each other).
pub const WHEEL_ZOOM: f32 = 0.0015;
/// Views kept for "Previous", and the pause that starts a new scroll step.
pub const PART_VIEW_HISTORY: usize = 20;
pub const PART_VIEW_PAUSE_S: f64 = 0.4;

/// Zoom factor for a scroll: exp, so scrolling back by the same amount returns
/// exactly to the old zoom (like the original's f and 1/f).
/// Colour of the beam's centre path under system compensation.
pub const KERF_PATH: Color32 = Color32::from_rgb(56, 220, 230);

/// The Part view's technology card (top left), screen px.
const INFO_CARD: Vec2 = Vec2::new(400.0, 101.0);

pub fn wheel_factor(scroll: f32) -> f32 {
    (scroll * WHEEL_ZOOM).exp()
}

/// Extent of everything drawn for the part: its box plus its technology's leads,
/// lead-outs, loops and joint leads ([x0, y0, x1, y1], mm).
pub fn view_extent(plan: &cnc::SheetPlan, w: f64, h: f64) -> [f64; 4] {
    let mut e = [0.0, 0.0, w, h];
    let mut add = |q: P| {
        e = [e[0].min(q[0]), e[1].min(q[1]), e[2].max(q[0]), e[3].max(q[1])];
    };
    for c in &plan.contours {
        c.lead_path.iter().for_each(|&q| add(q));
        add(c.lead_in);
        c.lead_out_path.iter().for_each(|&q| add(q));
        for (_, l) in &c.loops {
            l.iter().for_each(|&q| add(q));
        }
        for (out, back) in &c.joint_leads {
            out.iter().chain(back.iter()).for_each(|&q| add(q));
        }
    }
    e
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
}

/// Element under the cursor, the way the original captures: an element counts
/// if a piece of it falls inside the capture square around the point
/// (rule E1); of those the nearest wins, and when two are equally near, the
/// one whose start is nearer (rule E3, equality within 1e-6).
/// Starting "far" distance of the pick search (rule E3).
pub const PICK_FAR: f64 = 100000.999;

pub fn capture(elems: &[crate::nest::Element], p: P, half: f64) -> Option<usize> {
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, e) in elems.iter().enumerate() {
        // A segment within the square is also within half·√2 of its centre; use the
        // square test proper: nearest distance along both axes.
        let d = e.pts.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::MAX, f64::min);
        let in_square = e.pts.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            // Clip test: closest point of the segment to p, checked against the square.
            let ab = [b[0] - a[0], b[1] - a[1]];
            let l2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
            (q[0] - p[0]).abs() <= half && (q[1] - p[1]).abs() <= half
        });
        if !in_square {
            continue;
        }
        let ds = ((e.pts[0][0] - p[0]).powi(2) + (e.pts[0][1] - p[1]).powi(2)).sqrt();
        if d >= PICK_FAR {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, bd, bs)) => d < bd - 1e-6 || ((d - bd).abs() <= 1e-6 && ds < bs),
        };
        if better {
            best = Some((i, d, ds));
        }
    }
    best.map(|b| b.0)
}

impl App {
    /// Parts that open together with part `idx` (original PIEZADIB.SYS modes).
    /// In this rebuild every part of a job shares the job's machine, material and
    /// thickness, so the last three scopes give the same list.
    pub fn open_set_for(&self, idx: usize, scope: OpenScope) -> Vec<usize> {
        match scope {
            OpenScope::Selected => vec![idx],
            OpenScope::All | OpenScope::SameMachine | OpenScope::SameTechnology => (0..self.parts.len()).collect(),
        }
    }

    /// Open a part on its own, with the parts that go with it.
    pub fn open_part(&mut self, idx: usize) {
        if idx >= self.parts.len() {
            return;
        }
        self.store_job();
        // An opened part's technology commands work on that part.
        self.tech_all = false;
        self.open_set = self.open_set_for(idx, self.open_scope);
        self.open_idx = self.open_set.iter().position(|&i| i == idx).unwrap_or(0);
        self.sel_part = Some(idx);
        self.workspace = Workspace::Part;
        self.tab = Tab::Home;
        self.dv.sim = None;
        self.dv.sim_run = false;
        self.dv.measure.clear();
        self.part_zoom = 1.0;
        self.part_pan = Vec2::ZERO;
        self.part_view_hist.clear();
        self.status = format!("Opened '{}' ({} part(s) in this drawing).", self.parts[idx].name, self.open_set.len());
    }

    pub fn open_part_step(&mut self, delta: i32) {
        if self.open_set.is_empty() {
            return;
        }
        let n = self.open_set.len() as i32;
        self.open_idx = ((self.open_idx as i32 + delta).rem_euclid(n)) as usize;
        self.sel_part = Some(self.open_set[self.open_idx]);
    }

    /// Lead-in on the selected element: the contour starts at the element's middle
    /// (rule E4), if the element is long enough for a lead.
    pub fn lead_on_element(&mut self) {
        let Some(i) = self.shown_part() else { return };
        let Some(&k) = self.sel_elems.first() else {
            self.status = "Click a line, arc or circle on the part first.".into();
            return;
        };
        // The click counts only when it was on this element (not a stale click
        // from before a box or right-click selection).
        let pick = self.sel_pick.filter(|p| p.0 == k).map(|p| p.1);
        let mut part = self.parts[i].clone();
        match crate::engine::lead_on_element(&mut part, k, pick) {
            Ok(msg) => {
                self.checkpoint();
                self.parts[i] = part;
                self.refresh();
                self.status = msg;
            }
            Err(e) => self.status = e,
        }
    }

    /// A micro-joint in the middle of every selected element.
    pub fn joint_on_elements(&mut self) {
        let Some(i) = self.shown_part() else { return };
        if self.sel_elem_info.is_empty() {
            self.status = "Click (or box-select) elements on the part first.".into();
            return;
        }
        self.checkpoint();
        let picks: Vec<(usize, P)> = self.sel_elem_info.iter().map(|&(ci, _, _, mid)| (ci, mid)).collect();
        let placed = crate::engine::joints_on_elements(&mut self.parts[i], &picks);
        self.refresh();
        self.status = format!("{placed} micro-joint(s) placed on the selected element(s).");
    }

    /// The part shown in the Part view.
    pub fn shown_part(&self) -> Option<usize> {
        self.open_set.get(self.open_idx).copied().filter(|&i| i < self.parts.len())
    }

    pub fn part_workspace(&mut self, ui: &mut egui::Ui) {
        if self.shown_part().is_none() {
            self.workspace = Workspace::Nesting;
            return;
        }
        if self.tab == Tab::Utilities && !crate::drawing::utility_tab_shown(&self.dv) {
            self.tab = Tab::Home;
        }
        if !self.dv.full_screen {
            self.dock_panels(ui);
        }
        self.missing_window(&ui.ctx().clone());
        // The cut technology panel (CS6), opened from Technology → Paint contours.
        if self.cut_panel.open {
            let mut open = true;
            egui::Panel::right("cut_tech_panel")
                .resizable(true)
                .default_size(250.0)
                .frame(egui::Frame::new().fill(th().panel))
                .show(ui, |ui| open = self.cut_panel_body(ui));
            if !open {
                self.cut_panel.open = false;
                self.cut_panel.row = None;
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(th().bg).inner_margin(egui::Margin::same(3)))
            .show(ui, |ui| self.part_canvas(ui));
    }

    /// The parts opened together, to step between them.
    pub fn part_list_body(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(format!("{} part(s) opened together", self.open_set.len())).size(11.5).color(th().text_dim));
        let mut pick = None;
        for (k, &i) in self.open_set.iter().enumerate() {
            let Some(p) = self.parts.get(i) else { continue };
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let (r, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(2), part_color(p.color));
                let mut txt = RichText::new(&p.name);
                if k == self.open_idx {
                    txt = txt.strong();
                }
                if ui.selectable_label(k == self.open_idx, txt).clicked() {
                    pick = Some(k);
                }
                if p.tech.is_some() {
                    chip(ui, "T", th().accent_soft, th().accent_fg);
                }
            });
        }
        if let Some(k) = pick {
            self.open_idx = k;
            self.sel_part = Some(self.open_set[k]);
        }
    }

    /// The part's machining on its own, at the drawing's origin.
    fn part_plan(&self, idx: usize) -> cnc::SheetPlan {
        cnc::plan_part(&self.parts, idx, &self.spec)
    }

    /// The shown part's program as simulation steps (the sheet's, for this
    /// part alone), and its number of contours.
    pub fn part_sim_ops(&self) -> (Vec<crate::sheetsim::Op>, usize) {
        let Some(idx) = self.shown_part() else { return (vec![], 0) };
        let plan = self.part_plan(idx);
        (crate::sheetsim::ops(&plan, std::slice::from_ref(&self.parts[idx].name)), plan.contours.len())
    }

    /// The opened part, large, with everything its technology does.
    fn part_canvas(&mut self, ui: &mut egui::Ui) {
        let Some(idx) = self.shown_part() else { return };
        let (w, h) = self.parts[idx].bbox();
        let plan = self.part_plan(idx);

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // The last check of this part stays in view (top right) until closed.
        if let Some((pi, title, ok, lines)) = self.dv.check.clone().filter(|c| c.0 == idx) {
            let _ = pi;
            let mut close = false;
            egui::Area::new(egui::Id::unique("part_check")).order(egui::Order::Foreground).fixed_pos(rect.right_top() + Vec2::new(-352.0, 12.0)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_width(316.0);
                    ui.horizontal(|ui| {
                        let (mark, col) = if ok { ("✔", th().ok_fg) } else { ("⚠", th().warn_fg) };
                        ui.label(RichText::new(format!("{mark} {title}")).strong().color(col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    for l in &lines {
                        ui.label(RichText::new(l).size(12.5));
                    }
                });
            });
            if close {
                self.dv.check = None;
            }
        }
        let painter = ui.painter_at(rect);
        // The Drawing module's black background, or our gradient.
        let lantek = self.dv.lantek_colors;
        let bg = if lantek { crate::drawing::rgb(crate::drawing::C_BACKGROUND) } else { th().canvas_a };
        if lantek {
            painter.rect_filled(rect, CornerRadius::ZERO, bg);
        } else {
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(rect.left_top(), th().canvas_a);
            mesh.colored_vertex(rect.right_top(), th().canvas_b);
            mesh.colored_vertex(rect.right_bottom(), th().canvas_b);
            mesh.colored_vertex(rect.left_bottom(), th().canvas_a);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(EShape::mesh(mesh));
        }
        // "Zoom All" fits everything drawn — the part and its technology (leads,
        // lead-outs, loops) — like the original (rule V4: good + bad elements).
        let live_ext = view_extent(&plan, w, h);
        let margin = PART_FIT_MARGIN_PX;
        // While simulating, the part fits above the Simulation Window.
        let view = if self.dv.sim.is_some() && self.dv.machining { Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.bottom() - SIM_WINDOW_BAND_PX)) } else { rect };
        // The scale and centre that fit an extent in the view.
        let framing = |ext: [f64; 4]| {
            let (ew, eh) = ((ext[2] - ext[0]).max(1.0) as f32, (ext[3] - ext[1]).max(1.0) as f32);
            let fit = ((view.width() - 2.0 * margin) / ew).min((view.height() - 2.0 * margin) / eh).max(0.001);
            (fit, Vec2::new(((ext[0] + ext[2]) / 2.0) as f32, ((ext[1] + ext[3]) / 2.0) as f32))
        };
        // A lead end being dragged changes the extent; the view keeps its
        // framing while the button is down, so the part stays under the
        // pointer. On release the zoom and pan take over that framing, so
        // nothing jumps; Fit refits as usual.
        let mut ext = live_ext;
        if let Some((li, frozen)) = self.part_ext_lock {
            if li != idx {
                self.part_ext_lock = None;
            } else if ui.input(|i| i.pointer.primary_down()) {
                ext = frozen;
            } else {
                let ((f0, c0), (f1, c1)) = (framing(frozen), framing(live_ext));
                let s0 = f0 * self.part_zoom;
                let o0 = view.center() + self.part_pan + Vec2::new(-c0.x * s0, c0.y * s0);
                self.part_zoom = s0 / f1;
                self.part_pan = o0 - view.center() - Vec2::new(-c1.x * s0, c1.y * s0);
                self.part_ext_lock = None;
            }
        }
        let (fit, ctr) = framing(ext);
        let origin = |zoom: f32, pan: Vec2| view.center() + pan + Vec2::new(-ctr.x * fit * zoom, ctr.y * fit * zoom);
        let now = ui.input(|i| i.time);
        // Scroll zooms about the pointer: the point under it stays put
        // (rule V1: x0' = p - (p - x0)*f, and zooming in uses 1/f).
        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 && rect.contains(hp) {
                // One history step per burst of scrolling (V3).
                if now - self.part_view_last > PART_VIEW_PAUSE_S {
                    self.push_part_view();
                }
                self.part_view_last = now;
                let o0 = origin(self.part_zoom, self.part_pan);
                let s0 = fit * self.part_zoom;
                let wpt = Vec2::new((hp.x - o0.x) / s0, (o0.y - hp.y) / s0);
                self.part_zoom = (self.part_zoom * wheel_factor(scroll)).clamp(PART_ZOOM_MIN, PART_ZOOM_MAX);
                let s1 = fit * self.part_zoom;
                let o1 = origin(self.part_zoom, self.part_pan);
                self.part_pan += hp - Pos2::new(o1.x + wpt.x * s1, o1.y - wpt.y * s1);
            }
        }
        // Pan: drag with the middle button, or hold Space and drag.
        let space = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::Space));
        let pan_start = resp.drag_started_by(egui::PointerButton::Middle) || (space && resp.drag_started_by(egui::PointerButton::Primary));
        if pan_start {
            self.push_part_view();
        }
        let panning = resp.dragged_by(egui::PointerButton::Middle) || (space && resp.dragged_by(egui::PointerButton::Primary));
        if panning {
            self.part_pan += resp.drag_delta();
            self.box_start = None;
        }
        if space || resp.dragged_by(egui::PointerButton::Middle) {
            ui.ctx().set_cursor_icon(if panning { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        let s = fit * self.part_zoom;
        let o = origin(self.part_zoom, self.part_pan);
        let to_s = |p: P| Pos2::new(o.x + p[0] as f32 * s, o.y - p[1] as f32 * s);

        let to_w = |q: Pos2| -> P { [((q.x - o.x) / s) as f64, ((o.y - q.y) / s) as f64] };

        self.dv.cursor = resp.hover_pos().map(to_w);
        self.paint_drawing_overlays(&painter, rect, &to_s, resp.hover_pos());
        let now_t = ui.input(|i| i.time);
        if !self.dv.machining {
            self.dv.sim = None;
            self.dv.sim_run = false;
        }
        let sim_ops = if self.dv.sim.is_some() { crate::sheetsim::ops(&plan, std::slice::from_ref(&self.parts[idx].name)) } else { vec![] };
        let sim_total: f64 = sim_ops.iter().map(|o| o.secs).sum();
        self.sim_tick(now_t, sim_total, ui.ctx());

        // Part.
        let part = &self.parts[idx];
        let mt = crate::ct1::for_spec(&self.spec);
        if lantek {
            crate::drawing::paint_drawing_part(&painter, part, &mt, &to_s, self.dv.texts_dims, self.cut_panel.open);
        } else {
            let base = part_color(part.color);
            paint_part(&painter, part, to_s, base, Stroke::new(1.5, darker(base, 0.42)), th().canvas_a);
            // Every contour in its state or quality colour (CS7).
            crate::cutpanel::paint_contour_colours(&painter, part, &mt, &to_s, 1.6, self.cut_panel.open);
        }
        // Painting with the cut technology panel (CS6), or picking the cut
        // order (CO11): clicks, boxes and sweeps take contours instead of
        // picking elements.
        let painting = self.cut_panel.painting() || self.order_pick.on.is_some();
        if painting && !space {
            self.cut_panel_canvas(ui, &resp, &painter, idx, &to_s, &to_w, (APERTURE_PX / s) as f64);
            self.order_pick_canvas(ui, &resp, &painter, rect, idx, &to_s, &to_w, (APERTURE_PX / s) as f64);
        }
        // Escape leaves measuring / zoom window.
        if (self.dv.measure_on || self.dv.zoom_window) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dv.measure_on = false;
            self.dv.zoom_window = false;
            self.dv.measure.clear();
        }

        // ---- Placing a text: a preview follows the pointer, a click drops it ----
        if self.placing_text {
            if let Some(hp) = resp.hover_pos() {
                let at = to_w(hp);
                let preview = self.text_at_click(at);
                for s in preview.strokes(&preview.text) {
                    painter.add(EShape::line(s.iter().map(|&q| to_s(q)).collect(), Stroke::new(1.6, th().accent)));
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                if resp.clicked() {
                    self.checkpoint();
                    self.parts[idx].texts.push(preview);
                    self.placing_text = false;
                    self.refresh();
                    self.status = format!("Text '{}' placed as {}.", self.text_draft, self.text_kind.label().to_lowercase());
                    return;
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.placing_text = false;
            }
        }
        let part = &self.parts[idx].clone();

        // ---- Selecting elements (lines, arcs, circles) ----
        if self.elem_part != Some(idx) {
            self.sel_elems.clear();
            self.sel_texts.clear();
            self.elem_part = Some(idx);
        }
        let elems = crate::nest::elements_of(&part.geometry());
        let ap = (APERTURE_PX / s) as f64; // capture square half-size in mm
        let hover = resp.hover_pos().filter(|_| !painting).and_then(|hp| capture(&elems, to_w(hp), ap));
        // Right-click (or S): the radial tool menu. An element under the pointer is
        // selected first, so the element tools act on it.
        if let Some(hp) = resp.hover_pos() {
            if self.radial_trigger(ui) {
                let tu = crate::textedit::text_at(&part.texts, to_w(hp), ap.max(1.0));
                if let (Some(e), None) = (hover, tu) {
                    if !self.sel_elems.contains(&e) {
                        self.sel_elems = vec![e];
                    }
                }
                self.open_radial(ui.ctx(), hp, crate::radial::Ring::Part);
                if let Some(r) = self.radial.as_mut() {
                    r.text_under = tu;
                }
            }
        }
        // Double-click a text to edit it.
        if let (true, Some(hp)) = (resp.double_clicked(), resp.hover_pos()) {
            if let Some(k) = crate::textedit::text_at(&part.texts, to_w(hp), ap.max(1.0)) {
                self.edit_text_at(hp, idx, k);
            }
        }
        self.paint_edited_text(&painter, idx, &to_s);
        let shift = ui.input(|i| i.modifiers.shift);
        if self.dv.measure_on {
            if let (true, Some(hp)) = (resp.clicked(), resp.hover_pos()) {
                let tol = (crate::drawing::SNAP_PX / s) as f64;
                let mut q = crate::drawing::snap_point(&elems, to_w(hp), tol, self.dv.snap);
                let need = if self.dv.angle3 { 3 } else { 2 };
                if self.dv.measure.len() >= need {
                    self.dv.measure.clear();
                }
                if let (true, Some(&a)) = (self.dv.ortho, self.dv.measure.last()) {
                    if (q[0] - a[0]).abs() >= (q[1] - a[1]).abs() { q[1] = a[1] } else { q[0] = a[0] }
                }
                self.dv.measure.push(q);
                let pr = crate::drawing::MEASURE_DECIMALS;
                if self.dv.angle3 && self.dv.measure.len() == 3 {
                    let m = &self.dv.measure;
                    self.status = format!("Angle between three points: {:.pr$}°", crate::drawing::angle3(m[0], m[1], m[2]));
                } else if !self.dv.angle3 && self.dv.measure.len() == 2 {
                    let (d, dx, dy, ang) = crate::drawing::measure(self.dv.measure[0], self.dv.measure[1]);
                    let u = self.cfg.nesting.units;
                    let k = u.factor();
                    let pr = if u == crate::units::LenUnit::Inch { pr + 1 } else { pr };
                    self.status = format!("Distance {:.pr$} {}  ·  dX {:.pr$}  dY {:.pr$}  ·  angle {ang:.pr$}°", d / k, u.suffix(), dx / k, dy / k);
                }
            }
        } else if resp.clicked() && !painting {
            match hover {
                Some(e) if shift => {
                    if let Some(k) = self.sel_elems.iter().position(|&x| x == e) {
                        self.sel_elems.remove(k);
                    } else {
                        self.sel_elems.push(e);
                    }
                }
                Some(e) => {
                    self.sel_elems = vec![e];
                    self.sel_pick = resp.hover_pos().map(|hp| (e, to_w(hp)));
                }
                None => {
                    self.sel_elems.clear();
                    self.sel_texts.clear();
                }
            }
        }
        // Window selection: drag a box to the right for the elements wholly
        // inside it, to the left for every element it touches (E2, SW1, SW2).
        if resp.drag_started_by(egui::PointerButton::Primary) && !space && !painting {
            // Where the button went down, so the drag's direction is true.
            self.box_start = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos());
        }
        if let (Some(a), Some(b)) = (self.box_start, resp.interact_pointer_pos()) {
            if resp.dragged_by(egui::PointerButton::Primary) {
                // Zoom Window only frames the view: it is drawn as a window whichever way.
                let take = if self.dv.zoom_window { Take::Inside } else { Take::of_drag(a.x as f64, b.x as f64) };
                paint_select_window(&painter, Rect::from_two_pos(a, b), take);
            }
            if resp.drag_stopped() && self.dv.zoom_window {
                // Zoom Window: the box fills the view.
                let (wa, wb) = (to_w(a), to_w(b));
                let (bw, bh) = ((wa[0] - wb[0]).abs().max(1e-3) as f32, (wa[1] - wb[1]).abs().max(1e-3) as f32);
                let s2 = ((view.width() - 2.0 * margin) / bw).min((view.height() - 2.0 * margin) / bh);
                self.push_part_view();
                self.part_zoom = (s2 / fit).clamp(PART_ZOOM_MIN, PART_ZOOM_MAX);
                let s2 = fit * self.part_zoom;
                let c = Vec2::new(((wa[0] + wb[0]) / 2.0) as f32, ((wa[1] + wb[1]) / 2.0) as f32);
                self.part_pan = Vec2::new(-(c.x - ctr.x) * s2, (c.y - ctr.y) * s2);
                self.dv.zoom_window = false;
                self.box_start = None;
            } else if resp.drag_stopped() {
                let w = Window::dragged(to_w(a), to_w(b));
                let inside: Vec<usize> = elems.iter().enumerate().filter(|(_, e)| w.takes_line(&e.pts)).map(|(i, _)| i).collect();
                // The texts it takes are selected too (T14, SW3).
                let texts_in: Vec<usize> = part.texts.iter().enumerate().filter(|(_, t)| crate::textedit::text_in_window(t, &w)).map(|(k, _)| k).collect();
                if !shift {
                    self.sel_texts.clear();
                }
                for k in texts_in {
                    if !self.sel_texts.contains(&k) {
                        self.sel_texts.push(k);
                    }
                }
                if !self.sel_texts.is_empty() {
                    self.status = format!("{} text(s) selected — Delete removes them, double-click one to edit it.", self.sel_texts.len());
                }
                if shift {
                    for i in inside {
                        if !self.sel_elems.contains(&i) {
                            self.sel_elems.push(i);
                        }
                    }
                } else {
                    self.sel_elems = inside;
                }
                self.box_start = None;
            }
        }
        self.sel_elems.retain(|&i| i < elems.len());
        // Selected texts: outlined; Delete removes them.
        self.sel_texts.retain(|&k| k < part.texts.len());
        for &k in &self.sel_texts {
            let b = crate::textedit::text_box(&part.texts[k]);
            let mut pts: Vec<Pos2> = b.iter().map(|&q| to_s(q)).collect();
            pts.push(pts[0]);
            painter.add(EShape::line(pts, Stroke::new(1.5, th().accent_fg)));
        }
        if !self.sel_texts.is_empty() && resp.hovered() && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            self.checkpoint();
            let mut ks = std::mem::take(&mut self.sel_texts);
            ks.sort_unstable_by(|a, b| b.cmp(a));
            for k in &ks {
                self.parts[idx].texts.remove(*k);
            }
            self.refresh();
            self.status = format!("{} text(s) deleted.", ks.len());
        }
        for (i, e) in elems.iter().enumerate() {
            let sel = self.sel_elems.contains(&i);
            if sel || hover == Some(i) {
                let pts: Vec<Pos2> = e.pts.iter().map(|&q| to_s(q)).collect();
                let st = if sel { Stroke::new(4.0, th().accent_fg) } else { Stroke::new(3.0, Color32::from_white_alpha(170)) };
                painter.add(EShape::line(pts, st));
                if sel {
                    painter.circle_filled(to_s(e.midpoint()), 3.5, th().accent_fg);
                }
            }
        }
        if let Some(e) = hover.and_then(|i| elems.get(i)) {
            if let Some(hp) = resp.hover_pos() {
                let txt = format!("{} {}", e.kind.label(), self.cfg.nesting.units.len(e.length, 1));
                let g = painter.layout_no_wrap(txt, FontId::proportional(12.0), Color32::WHITE);
                let r = Rect::from_min_size(hp + Vec2::new(14.0, 12.0), g.size() + Vec2::new(12.0, 6.0));
                painter.rect_filled(r, CornerRadius::same(6), th().overlay);
                painter.galley(r.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
            }
        }
        self.sel_elem_info = self.sel_elems.iter().filter_map(|&i| elems.get(i).map(|e| (e.contour, e.kind, e.length, e.midpoint()))).collect();

        // Technology: joints, leads, loops, pierce points and cut order.
        let jcol = Color32::from_rgb(250, 204, 21);
        let mut joint_edit: Option<(f64, f64)> = None;
        // A lead end dragged (contour, end, pointer) or double-clicked (contour) (LE2, LE6).
        let mut lead_drag: Option<(usize, crate::engine::LeadEnd, P)> = None;
        let mut lead_reset: Option<usize> = None;
        // A lead end let go (contour), and the leads shared by the holes of a shape (LE8).
        let mut lead_drop: Option<usize> = None;
        let shared_leads = crate::engine::hand_leads(&self.parts[idx]);
        let lead_out_col = Color32::from_rgb(129, 140, 248);
        let pierce_col = Color32::from_rgb(248, 80, 90);
        let punch_col = Color32::from_rgb(255, 140, 60);
        for (k, c) in plan.contours.iter().enumerate() {
            // Standard-shape recognition label.
            if self.dv.std_shapes {
                if let Some(sh) = crate::machine::recognise(&c.points) {
                    // Holes are labelled at their centre, the outside along its top
                    // edge, so labels of nested contours don't sit on each other.
                    let at = if c.inner {
                        to_s(sh.centre())
                    } else {
                        let top = c.points.iter().fold(f64::MIN, |m, q| m.max(q[1]));
                        to_s([sh.centre()[0], top]) + Vec2::new(0.0, 6.0)
                    };
                    let txt = format!("{}{}", sh.label(), if c.punch.is_some() { " · punched" } else { "" });
                    let g = painter.layout_no_wrap(txt, FontId::proportional(11.5), Color32::WHITE);
                    let r = Rect::from_center_size(at + Vec2::new(0.0, 16.0), g.size() + Vec2::new(12.0, 6.0));
                    painter.rect_filled(r, CornerRadius::same(5), with_alpha(Color32::from_rgb(20, 120, 160), 220));
                    painter.galley(r.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
                    let mut pts: Vec<Pos2> = c.points.iter().map(|&q| to_s(q)).collect();
                    pts.push(pts[0]);
                    painter.add(EShape::line(pts, Stroke::new(2.5, with_alpha(Color32::from_rgb(56, 220, 230), 160))));
                }
            }
            // Punched (combined / punching machine): the tool outline and its hits.
            if let Some(pu) = &c.punch {
                match pu.tool {
                    Some(_) => {
                        let mut pts: Vec<Pos2> = c.points.iter().map(|&q| to_s(q)).collect();
                        pts.push(pts[0]);
                        painter.add(EShape::line(pts, Stroke::new(3.0, punch_col)));
                    }
                    None => {}
                }
                for &h in &pu.hits {
                    let q = to_s(h);
                    if pu.tool.is_some() {
                        painter.line_segment([q - Vec2::new(5.0, 0.0), q + Vec2::new(5.0, 0.0)], Stroke::new(1.6, punch_col));
                        painter.line_segment([q - Vec2::new(0.0, 5.0), q + Vec2::new(0.0, 5.0)], Stroke::new(1.6, punch_col));
                    } else {
                        painter.circle_stroke(q, 2.6, Stroke::new(1.2, punch_col));
                    }
                }
                continue;
            }
            // System compensation: the beam's centre path, beside the part's edge (KC8).
            if c.comp.system {
                let mut pts: Vec<Pos2> = c.points.iter().map(|&q| to_s(q)).collect();
                pts.push(pts[0]);
                painter.add(EShape::line(pts, Stroke::new(1.2, KERF_PATH)));
            }
            for (jk, &(a, b)) in c.joints.iter().enumerate() {
                let pa = to_s(cnc::point_at(&c.points, a));
                let pb = to_s(cnc::point_at(&c.points, b));
                painter.line_segment([pa, pb], Stroke::new(4.0, bg));
                painter.circle_filled(pa + (pb - pa) * 0.5, 3.0, jcol);
                // Joint leads: lead-out (blue) into the scrap, lead-in (white) back.
                if let Some((out, back)) = c.joint_leads.get(jk) {
                    if out.len() > 1 {
                        painter.add(EShape::line(out.iter().map(|&q| to_s(q)).collect(), Stroke::new(1.8, Color32::from_rgb(129, 140, 248))));
                    }
                    painter.line_segment([to_s(back[0]), to_s(back[1])], Stroke::new(1.8, Color32::WHITE));
                    painter.circle_filled(to_s(back[0]), 2.5, Color32::from_rgb(248, 80, 90));
                    // Drag the lead-in's free end to edit the joint leads
                    // (J8: foot stays, angle 0–90°, length to the pointer).
                    // The lead-in leans forward, away from the gap (AM6).
                    // Contour painting and cut-order picking keep the
                    // canvas's drag, so a sweep never moves a joint's lead.
                    if painting {
                        continue;
                    }
                    let h = to_s(back[0]);
                    let hr = ui.interact(Rect::from_center_size(h, Vec2::splat(14.0)), ui.scope_id().with(("joint_lead", k, jk)), Sense::drag());
                    if hr.hovered() || hr.dragged() {
                        painter.circle_stroke(h, 6.0, Stroke::new(1.5, th().accent));
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    if hr.drag_started() {
                        self.checkpoint();
                    }
                    if let (true, Some(pp)) = (hr.dragged(), hr.interact_pointer_pos()) {
                        let foot = back[1];
                        let p0 = cnc::point_at(&c.points, a);
                        let t = { let v = [foot[0] - p0[0], foot[1] - p0[1]]; let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-9); [v[0] / l, v[1] / l] };
                        let n = crate::leads::scrap_normal(&c.points, c.inner, t);
                        joint_edit = Some(cnc::edit_joint_lead(foot, t, n, to_w(pp)));
                    }
                }
            }
            let lp: Vec<Pos2> = c.lead_path.iter().map(|&q| to_s(q)).collect();
            painter.add(EShape::line(lp, Stroke::new(2.0, Color32::WHITE)));
            if c.lead_out_path.len() > 1 {
                painter.add(EShape::line(c.lead_out_path.iter().map(|&q| to_s(q)).collect(), Stroke::new(2.0, lead_out_col)));
            }
            // Drag a lead's free end — the lead-in's pierce, the lead-out's end —
            // to swing or stretch that one lead (LE2); double-click puts it back
            // (LE6). Space (panning) and contour painting keep the canvas's drag.
            if c.punch.is_none() && !c.closing && !space && !painting {
                use crate::engine::LeadEnd;
                let ends = crate::engine::lead_ends(c);
                let u = self.unit();
                let free = [(LeadEnd::In, (c.lead_path.len() > 1).then(|| c.lead_path[0]), ends.lead_in, pierce_col), (LeadEnd::Out, (c.lead_out_path.len() > 1).then(|| c.lead_out_path[c.lead_out_path.len() - 1]), ends.lead_out, lead_out_col)];
                for (end, at, now, col) in free {
                    let (Some(at), Some((len, ang))) = (at, now) else { continue };
                    let h = to_s(at);
                    // The pierce has its own mark while machining is shown.
                    if end == LeadEnd::Out || !self.dv.machining {
                        painter.circle_filled(h, 2.5, col);
                    }
                    // LE8: a lead shared by every hole of this shape.
                    let shared = shared_leads.get(c.contour).copied().flatten().is_some_and(|e| e.same);
                    let tip = format!(
                        "{} {} at {ang:.0}° from the contour{}.\nDrag to swing or stretch this {}{}.{}",
                        if end == LeadEnd::In { "Lead-in" } else { "Lead-out" },
                        u.len(len, 1),
                        if shared {
                            " (moved by hand, on every hole of this shape)"
                        } else if ends.hand {
                            " (moved by hand)"
                        } else {
                            ""
                        },
                        end.label(),
                        if shared {
                            "; double-click to put it back on all of them"
                        } else if ends.hand {
                            "; double-click to put it back"
                        } else {
                            ""
                        },
                        match (shared, self.lead_same) {
                            (true, false) => "\nAll the same contours is off: a drag gives this hole a lead of its own.",
                            _ => "",
                        }
                    );
                    let hr = ui.interact(Rect::from_center_size(h, Vec2::splat(14.0)), ui.scope_id().with(("lead_end", c.contour, end == LeadEnd::In)), Sense::click_and_drag()).on_hover_text(tip);
                    if hr.hovered() || hr.dragged() {
                        painter.circle_stroke(h, 6.0, Stroke::new(1.5, th().accent));
                        ui.ctx().set_cursor_icon(if hr.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                    }
                    if hr.drag_started() {
                        self.checkpoint();
                        self.part_ext_lock = Some((idx, ext));
                        self.lead_offer = None;
                    }
                    if hr.drag_stopped() {
                        lead_drop = Some(c.contour);
                    }
                    if let (true, Some(pp)) = (hr.dragged(), hr.interact_pointer_pos()) {
                        lead_drag = Some((c.contour, end, to_w(pp)));
                        // The length and angle beside the pointer while dragging.
                        let g = painter.layout_no_wrap(format!("{} · {ang:.0}°", u.len(len, 1)), FontId::proportional(12.0), Color32::WHITE);
                        let r = Rect::from_min_size(pp + Vec2::new(14.0, -26.0), g.size() + Vec2::new(10.0, 6.0));
                        painter.rect_filled(r, CornerRadius::same(5), th().overlay);
                        painter.galley(r.min + Vec2::new(5.0, 3.0), g, Color32::WHITE);
                    }
                    if hr.double_clicked() {
                        lead_reset = Some(c.contour);
                    }
                }
            }
            for (vi, l) in &c.loops {
                let mut pts = vec![to_s(c.points[*vi])];
                pts.extend(l.iter().map(|&q| to_s(q)));
                painter.add(EShape::line(pts, Stroke::new(2.0, jcol)));
            }
            if !self.dv.machining {
                continue;
            }
            let pierce = to_s(c.lead_in);
            painter.circle_filled(pierce, 4.0, Color32::WHITE);
            painter.circle_filled(pierce, 2.0, pierce_col);
        }
        // The cut order (CO13): each contour's number where it starts and
        // the rapids between them, from the engine's order; while sweeping,
        // as it would be were the sweep let go.
        if self.dv.machining {
            let preview = self.order_preview(idx);
            let stops = crate::orderpick::stops(preview.as_ref().unwrap_or(&self.parts[idx]), &plan);
            crate::orderpick::paint_order(&painter, &stops, &to_s);
        }

        // Simulation: the head follows the beam's path (pierce, lead-in,
        // contour, joints, lead-out), cuts done so far green, as on the sheet.
        if let Some(t) = self.dv.sim.filter(|_| !sim_ops.is_empty()) {
            let kerf_px = (crate::kerf::kerf_mm(&self.spec) as f32 * s).max(2.4);
            if let Some((cur, head)) = crate::sheetsim::paint_progress(&painter, &sim_ops, t, &to_s, kerf_px) {
                // Keep the head in view while it runs, and bring it back after
                // a step (when zoomed in).
                let hp = to_s(head);
                if (self.dv.sim_run || self.dv.sim_follow) && !view.shrink2(view.size() * 0.2).contains(hp) {
                    self.part_pan += view.center() - hp;
                }
                self.dv.sim_follow = false;
                // The Simulation Window, above the legend.
                let wr = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.bottom() - SIM_WINDOW_BAND_PX - 12.0), Vec2::new(560.0f32.min(rect.width() - 24.0), 120.0));
                let cutter = crate::kerf::cutter_radius(&crate::ct1::for_spec(&self.spec));
                let mut tt = t;
                if crate::sheetsim::sim_window(ui, "part_sim_window", wr, &mut tt, sim_total, &sim_ops[cur], head, cutter) {
                    self.dv.sim = Some(tt);
                    self.dv.sim_run = false;
                }
            }
        }

        // View Geometry: every element's points and direction.
        if self.dv.vertices {
            for e in &elems {
                for &q in &e.pts {
                    painter.rect_stroke(Rect::from_center_size(to_s(q), Vec2::splat(3.0)), CornerRadius::ZERO, Stroke::new(1.0, Color32::from_rgb(56, 220, 230)), egui::StrokeKind::Middle);
                }
                if e.pts.len() >= 2 {
                    let m = e.pts.len() / 2;
                    let (a, b) = (to_s(e.pts[m.saturating_sub(1)]), to_s(e.pts[m]));
                    let dir = (b - a).normalized();
                    if dir.length() > 0.5 {
                        painter.arrow(a, dir * 12.0, Stroke::new(1.2, Color32::from_rgb(56, 220, 230)));
                    }
                }
            }
        }
        // Dimensions.
        let dim = Color32::from_rgb(170, 178, 200);
        if self.dv.dims && self.dv.texts_dims {
            let y = o.y + 28.0;
            painter.line_segment([Pos2::new(o.x, y), Pos2::new(o.x + w as f32 * s, y)], Stroke::new(1.0, dim));
            painter.text(Pos2::new(o.x + w as f32 * s / 2.0, y + 4.0), Align2::CENTER_TOP, self.cfg.nesting.units.len(w, 1), FontId::proportional(12.0), dim);
            let x = o.x - 28.0;
            painter.line_segment([Pos2::new(x, o.y), Pos2::new(x, o.y - h as f32 * s)], Stroke::new(1.0, dim));
            painter.text(Pos2::new(x - 4.0, o.y - h as f32 * s / 2.0), Align2::RIGHT_CENTER, self.cfg.nesting.units.num_dp(h, 1), FontId::proportional(12.0), dim);
        }

        // Technology summary card.
        let t = part.tech;
        let lines = [
            format!("{}  ·  {} contour(s)", part.name, plan.contours.len()),
            match t {
                Some(t) if t.lead_type == crate::nest::LeadType::Machine => format!("Leads from the machine's tables by contour class{}", if t.corner_start { format!(" · automatic, pos {}", t.position) } else { String::new() }),
                Some(t) => format!(
                    "Lead-in {} {}{}  ·  lead-out {}",
                    self.cfg.nesting.units.len(t.lead_in, 1),
                    t.lead_type.label(),
                    if t.corner_start { format!(" · automatic, pos {}", t.position) } else { String::new() },
                    self.cfg.nesting.units.len(t.lead_out, 1)
                ),
                None => "Automatic sheet technology (machine lead tables)".into(),
            },
            format!(
                "Micro-joints {}{}  ·  loops {}  ·  quality: {}",
                plan.contours.iter().map(|c| c.joints.len()).sum::<usize>(),
                t.filter(|t| t.micro_joints > 0 || t.joint_every > 0.0).map_or(String::new(), |t| format!(" ({})", t.joint_type.label().to_lowercase())),
                t.map_or("off", |t| if t.loops { "on" } else { "off" }),
                crate::techui::part_quality_label(t.and_then(|t| t.quality), &crate::ct1::for_spec(&self.spec).qualities)
            ),
            format!("Cut {}  ·  {} pierce(s)  ·  {:.1} s", self.cfg.nesting.units.len(plan.cut_len, 0), plan.pierces, plan.seconds),
            // Kerf compensation (KC1): width, kind, and what can't be cut.
            format!(
                "{}{}",
                crate::kerf::status_line(&self.spec),
                if plan.kerf_issues.is_empty() { String::new() } else { format!("  ·  {} contour(s) not fully cut", plan.kerf_issues.len()) }
            ),
        ];
        let card = Rect::from_min_size(rect.left_top() + Vec2::new(12.0, 12.0), INFO_CARD);
        if self.dv.info {
            painter.rect_filled(card, CornerRadius::same(10), th().overlay);
        }
        for (k, l) in lines.iter().enumerate().filter(|_| self.dv.info) {
            let col = if k == 0 { Color32::WHITE } else { Color32::from_rgb(200, 205, 220) };
            painter.text(card.left_top() + Vec2::new(12.0, 10.0 + k as f32 * 17.0), Align2::LEFT_TOP, l, FontId::proportional(if k == 0 { 13.5 } else { 12.0 }), col);
        }
        // Legend.
        let lg = rect.left_bottom() + Vec2::new(14.0, -18.0);
        let mut items: Vec<(&str, Color32)> = vec![("lead-in", Color32::WHITE), ("lead-out", Color32::from_rgb(129, 140, 248)), ("joint / loop", jcol), ("pierce", Color32::from_rgb(248, 80, 90))];
        if plan.contours.iter().any(|c| c.comp.system) {
            items.push(("kerf path", KERF_PATH));
        }
        let mut x = lg.x;
        for (name, col) in items {
            painter.line_segment([Pos2::new(x, lg.y), Pos2::new(x + 18.0, lg.y)], Stroke::new(2.0, col));
            painter.text(Pos2::new(x + 24.0, lg.y), Align2::LEFT_CENTER, name, FontId::proportional(11.5), dim);
            x += 104.0;
        }
        if self.dv.machining {
            crate::orderpick::legend(&painter, Pos2::new(x, lg.y), dim);
        }
        self.part_zoom_bar(ui, rect);
        // A lead end was dragged: that contour's lead takes it (LE2).
        if let Some((ci, end, p)) = lead_drag {
            let u = self.unit();
            self.status = match crate::engine::drag_lead_end(&mut self.parts, idx, &self.spec, ci, end, p, self.lead_same, u) {
                Ok(m) => format!("{m} Double-click its end to put it back."),
                Err(e) => e,
            };
            self.refresh();
        }
        // A lead let go on one hole alone: offer it to the holes of its shape (LE8).
        if let Some(ci) = lead_drop.filter(|_| !self.lead_same) {
            let n = crate::engine::same_holes(&self.parts[idx], ci).len();
            self.lead_offer = (n > 0 && lead_alone(&self.parts[idx], ci)).then_some((idx, ci, n));
        }
        if let Some(ci) = lead_reset {
            let no = crate::engine::contour_number(&self.parts[idx], ci);
            // Tried on a copy first, so a double-click that changes nothing leaves no undo step.
            let mut part = self.parts[idx].clone();
            let n = crate::engine::reset_lead_edits(&mut part, Some(ci), self.lead_same);
            if n > 0 {
                self.checkpoint();
                self.parts[idx] = part;
                self.refresh();
                let follows = crate::engine::hand_lead(&self.parts[idx], ci).map(|(_, k)| crate::engine::contour_number(&self.parts[idx], k));
                self.status = match (follows, n) {
                    (Some(k), _) => format!("Contour {no}'s lead is back to the one the holes of its shape share (set on contour {k})."),
                    (None, 1) => format!("Contour {no}'s lead is back where the technology puts it."),
                    (None, n) => format!("Contour {no}'s lead and {} more of the same shape are back where the technology puts them.", n - 1),
                };
            } else {
                self.status = format!("Contour {no}'s lead wasn't moved by hand — drag its end to move it.");
            }
        }
        self.lead_same_overlay(ui, rect, idx, painting);
        // A joint lead was dragged: all joints of the part take the new lead.
        if let Some((len, ang)) = joint_edit {
            // The lead's distance off the contour is what the joint record keeps (D, AM6).
            let mut t = self.parts[idx].tech.unwrap_or_default();
            t.joint_lead = (len * ang.to_radians().sin() * 10.0).round() / 10.0;
            t.joint_angle = ang.round().max(1.0);
            self.parts[idx].tech = Some(t);
            self.refresh();
            let reach = self.spec.gap * crate::nest::JOINT_LEAD_REACH;
            let u = self.cfg.nesting.units;
            let clamp = if t.joint_lead > reach && crate::ct1::active().joint_test <= 0 { format!(" (cut short to stay {} out, inside the part gap)", u.len(reach, 1)) } else { String::new() };
            self.status = format!("Joint lead-in {} off the contour at {:.0}°{clamp}.", u.len(t.joint_lead, 1), t.joint_angle);
        }
    }

    /// Remember the current view (rule V3: the original stores the window
    /// before every change).
    pub fn push_part_view(&mut self) {
        let v = (self.part_zoom, self.part_pan);
        if self.part_view_hist.last() != Some(&v) {
            self.part_view_hist.push(v);
            if self.part_view_hist.len() > PART_VIEW_HISTORY {
                self.part_view_hist.remove(0);
            }
        }
    }

    /// Back to the view before the last change (V3: Zoom Previous pops it).
    pub fn previous_part_view(&mut self) {
        match self.part_view_hist.pop() {
            Some((z, p)) => {
                self.part_zoom = z;
                self.part_pan = p;
            }
            None => self.status = "No previous view.".into(),
        }
    }

    /// Zoom pill in the Part view's right-hand margin, the same as on the
    /// sheet: +, zoom %, −, Fit, then the previous view — and a hint that the
    /// drawing can be moved.
    fn part_zoom_bar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let items: [(&str, f32); 5] = [("+", 32.0), ("", 22.0), ("−", 32.0), ("Fit", 32.0), ("↶", 30.0)];
        let total: f32 = items.iter().map(|i| i.1).sum::<f32>() + 12.0;
        let bw = 48.0;
        let bar = Rect::from_min_size(Pos2::new(rect.right() - bw - 12.0, rect.center().y - total / 2.0), Vec2::new(bw, total));
        let p = ui.painter_at(rect);
        p.rect_filled(bar.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(14), Color32::from_black_alpha(60));
        p.rect_filled(bar, CornerRadius::same(14), th().overlay);
        p.rect_stroke(bar, CornerRadius::same(14), Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)), egui::StrokeKind::Inside);
        let mut y = bar.top() + 6.0;
        for (k, (label, h)) in items.into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(bar.left() + 4.0, y), Vec2::new(bw - 8.0, h));
            y += h;
            if label.is_empty() {
                p.text(r.center(), Align2::CENTER_CENTER, format!("{:.0}%", self.part_zoom * 100.0), FontId::proportional(11.5), Color32::WHITE);
                continue;
            }
            if k == 4 {
                p.line_segment([Pos2::new(bar.left() + 10.0, r.top() - 1.0), Pos2::new(bar.right() - 10.0, r.top() - 1.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60)));
            }
            let resp = ui.interact(r, ui.scope_id().with(("part_zoom", k)), Sense::click());
            let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
            if hov > 0.0 {
                p.rect_filled(r.shrink(1.0), CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, (hov * 45.0) as u8));
            }
            let col = if resp.hovered() { Color32::from_rgb(199, 210, 254) } else { Color32::WHITE };
            p.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(if label == "Fit" { 11.5 } else { 18.0 }), col);
            let tip = match label {
                "↶" => "Previous view",
                "−" => "Zoom out",
                "+" => "Zoom in",
                _ => "Fit the part in the view",
            };
            if resp.on_hover_text(tip).clicked() {
                match label {
                    "↶" => self.previous_part_view(),
                    "−" => {
                        self.push_part_view();
                        self.part_zoom = (self.part_zoom * (1.0 / ZOOM_STEP)).max(PART_ZOOM_MIN);
                    }
                    "+" => {
                        self.push_part_view();
                        self.part_zoom = (self.part_zoom * ZOOM_STEP).min(PART_ZOOM_MAX);
                    }
                    _ => {
                        self.push_part_view();
                        self.part_zoom = 1.0;
                        self.part_pan = Vec2::ZERO;
                    }
                }
            }
        }
        let g = p.layout_no_wrap("Scroll to zoom · middle-drag or Space + drag to move".into(), FontId::proportional(11.5), Color32::from_rgb(200, 205, 220));
        let hint = Rect::from_min_size(Pos2::new(rect.right() - 14.0 - g.size().x - 20.0, rect.bottom() - 46.0), g.size() + Vec2::new(20.0, 24.0 - g.size().y));
        p.rect_filled(hint, CornerRadius::same(12), th().overlay);
        p.galley(Pos2::new(hint.left() + 10.0, hint.center().y - g.size().y / 2.0), g, Color32::WHITE);
    }

    /// All the same contours for leads (LE8), top left of the Part view (under
    /// the technology card) when two or more of the part's holes are the same
    /// shape, and under it the offer to put a lead just moved on one of them
    /// on the others.
    fn lead_same_overlay(&mut self, ui: &mut egui::Ui, rect: Rect, idx: usize, painting: bool) {
        let card = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::new()
                .fill(th().overlay)
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| ui.horizontal(|ui| add(ui)));
        };
        // Below the technology card (INFO_CARD) when it is shown.
        let mut at = rect.left_top() + Vec2::new(12.0, if self.dv.info { 12.0 + INFO_CARD.y + 8.0 } else { 12.0 });
        if !painting && crate::engine::has_same_holes(&self.parts[idx]) {
            let shown = egui::Area::new(egui::Id::unique("lead_same_chip")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
                card(ui, &mut |ui| {
                    let r = ui.checkbox(&mut self.lead_same, RichText::new("All the same contours").color(Color32::WHITE)).on_hover_text(
                        "Leads moved by hand: dragging the end of a hole's lead moves the lead on every hole of the same shape (moved, not turned or mirrored), at the same place on each; double-clicking puts all of them back. Off: only the hole you drag.",
                    );
                    if r.changed() {
                        self.lead_offer = None;
                    }
                });
            });
            at.y = shown.response.rect.bottom() + 6.0;
        }
        let Some((pi, ci, n)) = self.lead_offer else { return };
        // Gone once the lead is (undo, put back, shared) or another part is shown.
        if pi != idx || painting || !lead_alone(&self.parts[idx], ci) {
            self.lead_offer = None;
            return;
        }
        let no = crate::engine::contour_number(&self.parts[idx], ci);
        let (mut apply, mut close) = (false, false);
        egui::Area::new(egui::Id::unique("lead_offer")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
            card(ui, &mut |ui| {
                let holes = if n == 1 { "1 other hole".to_string() } else { format!("{n} other holes") };
                ui.label(RichText::new(format!("Contour {no}'s lead moved · {holes} like this one")).color(Color32::WHITE));
                let b = egui::Button::new(RichText::new("Apply to them").color(Color32::WHITE).strong()).fill(th().accent);
                apply = ui.add(b).on_hover_text(format!("Put this lead on the {holes} of the same shape too, at the same place on each (Ctrl+Z takes it back)")).clicked();
                close = ui.add(egui::Button::new(RichText::new("✖").color(Color32::from_rgb(200, 205, 220))).frame(false)).on_hover_text("Keep it on this hole only").clicked();
            });
        });
        if apply {
            self.checkpoint();
            self.status = match crate::engine::lead_to_same(&mut self.parts[idx], ci) {
                Ok(m) => m,
                Err(e) => e,
            };
            self.refresh();
        }
        if apply || close {
            self.lead_offer = None;
        }
    }

    /// Status line: the original's product format plus the position in the set.
    pub fn part_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(i) = self.shown_part() {
                let p = &self.parts[i];
                ui.label(
                    RichText::new(format!(
                        "{} / {} / {} / {} / {:.4}",
                        p.name,
                        p.qty,
                        self.machine_name(),
                        cnc::MATERIALS[self.spec.material],
                        self.spec.thickness / self.unit().factor()
                    ))
                    .monospace()
                    .color(th().text_strong),
                );
                ui.separator();
                ui.label(format!("Part {} of {}", self.open_idx + 1, self.open_set.len()));
                ui.separator();
                let mt = crate::ct1::for_spec(&self.spec);
                ui.label(RichText::new(mt.applied().map_or("Machine table: global".to_string(), |s| format!("Machine table {}", s.label()))).color(th().text_dim)).on_hover_text(mt.table_label());
            }
            ui.add_space(12.0);
            // Leave room for the pointer's X / Y on the right; a longer message
            // is cut short and shows whole on hover.
            let room = (ui.available_width() - 260.0).max(80.0);
            ui.scope(|ui| {
                ui.set_max_width(room);
                ui.add(egui::Label::new(RichText::new(&self.status).color(th().text_dim)).truncate());
            });
            let u = self.cfg.nesting.units;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(u.suffix());
                ui.add_space(14.0);
                let (x, y) = self.dv.cursor.map_or(("—".to_string(), "—".to_string()), |c| (u.num_dp(c[0], 3), u.num_dp(c[1], 3)));
                ui.label(format!("Y: {y}"));
                ui.add_space(30.0);
                ui.label(format!("X: {x}"));
            });
        });
    }
}

/// Circle through three points: (centre, radius).
/// Whether cut contour `ci` has a lead moved by hand of its own that the
/// holes of its shape don't share (LE8): what *Apply to them* is offered for.
fn lead_alone(part: &crate::nest::Part, ci: usize) -> bool {
    crate::engine::hand_lead(part, ci).is_some_and(|(e, k)| k == ci && !e.same)
}

pub fn circle_through(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let sq = |p: P| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let r = ((a[0] - ux).powi(2) + (a[1] - uy).powi(2)).sqrt();
    Some(([ux, uy], r))
}

/// Is a point in the part's material (inside the outline, not in a hole)?
fn in_material(g: &crate::nest::Geo, p: P) -> bool {
    crate::nest::point_in_poly(&g.outline, p) && !g.holes.iter().any(|h| crate::nest::point_in_poly(h, p))
}

/// A text laid out relative to one element, offset into the material: along a
/// line (kept readable left to right), or curved around an arc or circle
/// (letters outside a hole, inside an outer edge). `None` if it can't fit.
pub fn text_for_element(g: &crate::nest::Geo, e: &crate::nest::Element, text: &str, height: f64, kind: crate::nest::TextKind) -> Option<crate::nest::PartText> {
    use crate::nest::{ElemKind, PartText};
    let n = e.pts.len();
    if n < 2 {
        return None;
    }
    let o = (height * 0.3).max(1.0);
    match e.kind {
        ElemKind::Line => {
            let (mut a, mut b) = (e.pts[0], e.pts[n - 1]);
            if b[0] < a[0] - 1e-9 || ((b[0] - a[0]).abs() < 1e-9 && b[1] < a[1]) {
                std::mem::swap(&mut a, &mut b);
            }
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let dir = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
            let left = [-dir[1], dir[0]];
            let mut h = height;
            let w = crate::font::width(text, h);
            if w > len * 0.9 {
                h *= len * 0.9 / w;
            }
            let w = crate::font::width(text, h);
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            // Material above the line (left of reading direction): text sits on top;
            // otherwise it hangs below the line.
            let probe = [mid[0] + left[0] * o, mid[1] + left[1] * o];
            let base = if in_material(g, probe) {
                [mid[0] + left[0] * o, mid[1] + left[1] * o]
            } else {
                [mid[0] - left[0] * (o + h), mid[1] - left[1] * (o + h)]
            };
            let at = [base[0] - dir[0] * w / 2.0, base[1] - dir[1] * w / 2.0];
            Some(PartText { text: text.into(), at, height: h, angle: dir[1].atan2(dir[0]).to_degrees(), curve: 0.0, kind })
        }
        ElemKind::Arc | ElemKind::Circle => {
            let (p0, p1, p2) = if e.kind == ElemKind::Circle { (e.pts[0], e.pts[n / 3], e.pts[2 * n / 3]) } else { (e.pts[0], e.pts[n / 2], e.pts[n - 1]) };
            let (c, r) = circle_through(p0, p1, p2)?;
            let m = if e.kind == ElemKind::Circle { [c[0], c[1] + r] } else { e.midpoint() };
            let mid = (m[1] - c[1]).atan2(m[0] - c[0]);
            let top = mid.sin() >= -1e-9;
            // Material outside the curve (a hole) or inside it (an outer edge)?
            let outside = in_material(g, [c[0] + (r + o) * mid.cos(), c[1] + (r + o) * mid.sin()]);
            let radius = match (outside, top) {
                (true, true) => r + o,
                (true, false) => -(r + o + height),
                (false, true) => r - o - height,
                (false, false) => -(r - o),
            };
            if radius.abs() < height {
                return None;
            }
            Some(PartText { text: text.into(), at: c, height, angle: mid.to_degrees(), curve: radius, kind })
        }
    }
}

impl App {
    /// A new text from the Text group, dropped at a click: straight text starts
    /// there; curved text is centred there on an arc of the chosen radius.
    pub fn text_at_click(&self, p: P) -> crate::nest::PartText {
        let (text, height, kind, curve) = (self.text_draft.clone(), self.text_height, self.text_kind, self.text_curve);
        if curve.abs() < 1e-9 {
            crate::nest::PartText { text, at: p, height, angle: 0.0, curve: 0.0, kind }
        } else if curve > 0.0 {
            crate::nest::PartText { text, at: [p[0], p[1] - curve], height, angle: 90.0, curve, kind }
        } else {
            crate::nest::PartText { text, at: [p[0], p[1] - curve], height, angle: 270.0, curve, kind }
        }
    }

    /// Put the text relative to the selected element (Part view).
    pub fn text_on_element(&mut self) {
        let Some(i) = self.shown_part() else { return };
        if self.text_draft.trim().is_empty() {
            self.status = "Type the text first (Technology tab → Text).".into();
            return;
        }
        let g = self.parts[i].geometry();
        let elems = crate::nest::elements_of(&g);
        let Some(e) = self.sel_elems.first().and_then(|&k| elems.get(k)) else {
            self.status = "Click a line, arc or circle on the part first — the text goes along it.".into();
            return;
        };
        match text_for_element(&g, e, &self.text_draft, self.text_height, self.text_kind) {
            Some(t) => {
                self.checkpoint();
                let how = if t.curve != 0.0 { format!("curved (radius {})", self.cfg.nesting.units.len(t.curve.abs(), 1)) } else { "along the line".to_string() };
                self.parts[i].texts.push(t);
                self.refresh();
                self.status = format!("Text '{}' placed {how} next to the {}.", self.text_draft, e.kind.label().to_lowercase());
            }
            None => self.status = "That curve is too tight for text this tall — lower the height.".into(),
        }
    }
}

#[cfg(test)]
mod curved_text_tests {
    use super::*;
    use crate::nest::{Part, Shape, TextKind};

    fn part(shape: Shape, w: f64, h: f64) -> Part {
        Part { name: "P".into(), shape, w, h, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn arc_text_stays_on_its_band() {
        let c = [100.0, 100.0];
        for s in crate::font::strokes_on_arc("ABC", c, 50.0, 90.0, 10.0) {
            for p in s {
                let r = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt();
                assert!((49.9..=60.1).contains(&r), "outward letters between 50 and 60: {r}");
            }
        }
        for s in crate::font::strokes_on_arc("ABC", c, -50.0, 270.0, 10.0) {
            for p in s {
                let r = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt();
                assert!((39.9..=50.1).contains(&r), "inward letters between 40 and 50: {r}");
                assert!(p[1] < c[1], "bottom of the circle");
            }
        }
    }

    #[test]
    fn text_on_a_hole_curves_round_it_in_the_material() {
        let p = part(Shape::PlateHole, 200.0, 200.0);
        let g = p.geometry();
        let elems = crate::nest::elements_of(&g);
        let hole = elems.iter().find(|e| e.contour == 0).unwrap();
        let t = text_for_element(&g, hole, "REF", 8.0, TextKind::Mark).unwrap();
        assert!(t.curve > 0.0, "over the top of the hole, letters outward");
        for s in t.strokes(&t.text) {
            for q in s {
                assert!(in_material(&g, q), "{q:?} is in the plate, not the hole");
            }
        }
    }

    #[test]
    fn text_on_an_edge_runs_along_it_inside() {
        let p = part(Shape::Rectangle, 200.0, 100.0);
        let g = p.geometry();
        let elems = crate::nest::elements_of(&g);
        for e in &elems {
            let t = text_for_element(&g, e, "EDGE", 8.0, TextKind::Mark).unwrap();
            assert_eq!(t.curve, 0.0);
            let a = crate::nest::norm_angle(t.angle);
            assert!(a <= 90.0 + 1e-9 || a > 270.0 - 1e-9, "readable left to right: {a}");
            for s in t.strokes(&t.text) {
                for q in s {
                    assert!(in_material(&g, q), "{q:?} inside the part");
                }
            }
        }
    }

    #[test]
    fn a_click_with_an_arc_radius_centres_the_text_there() {
        let mut a = App::new(None);
        a.text_draft = "ABCD".into();
        a.text_curve = 40.0;
        let t = a.text_at_click([100.0, 80.0]);
        assert_eq!((t.at, t.angle, t.curve), ([100.0, 40.0], 90.0, 40.0));
        a.text_curve = -40.0;
        let t = a.text_at_click([100.0, 80.0]);
        assert_eq!((t.at, t.angle), ([100.0, 120.0], 270.0));
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn wheel_zoom_in_and_out_undo_each_other() {
        // The original zooms in by 1/f of its zoom-out factor f.
        let z = 1.7 * wheel_factor(120.0) * wheel_factor(-120.0);
        assert!((z - 1.7).abs() < 1e-5);
        assert!(((1.0 / ZOOM_STEP) * ZOOM_STEP - 1.0).abs() < 1e-6);
    }

    #[test]
    fn previous_view_steps_back() {
        let mut a = App::new(None);
        a.push_part_view();
        a.part_zoom = 3.0;
        a.part_pan = Vec2::new(40.0, -10.0);
        a.push_part_view();
        a.part_zoom = 5.0;
        a.previous_part_view();
        assert_eq!((a.part_zoom, a.part_pan), (3.0, Vec2::new(40.0, -10.0)));
        a.previous_part_view();
        assert_eq!(a.part_zoom, 1.0);
    }

    #[test]
    fn the_part_simulation_plays_its_program_in_real_time() {
        // A part with a hole and micro-joints with leads: the simulation runs
        // on the part's own program, at the chosen × real time.
        let mut a = App::new(None);
        let i = a.parts.iter().position(|p| p.name == "Square hole").expect("demo part");
        a.open_part(i);
        let (ops, n) = a.part_sim_ops();
        assert_eq!(n, 2);
        let total: f64 = ops.iter().map(|o| o.secs).sum();
        assert!((total - a.part_plan(i).seconds).abs() < 1e-6);
        // Running moves the clock by elapsed time × speed, and stops at the end.
        let ctx = egui::Context::default();
        a.dv.sim = Some(0.0);
        a.dv.sim_run = true;
        a.dv.sim_speed = 0.5;
        a.dv.sim_last = 10.0;
        a.sim_tick(10.2, total, &ctx);
        assert!((a.dv.sim.unwrap() - 0.1).abs() < 1e-9);
        a.dv.sim = Some(total - 0.01);
        a.sim_tick(10.4, total, &ctx);
        assert_eq!(a.dv.sim, Some(total));
        assert!(!a.dv.sim_run);
        assert_eq!(crate::sheetsim::contours_done(&ops, n, total), n);
    }

    #[test]
    fn fit_covers_the_technology_too() {
        let a = App::new(None);
        let i = a.parts.iter().position(|p| p.tech.is_some_and(|t| t.lead_in > 0.0)).expect("a part with leads");
        let (w, h) = a.parts[i].bbox();
        let plan = cnc::plan_part(&a.parts, i, &a.spec);
        let e = view_extent(&plan, w, h);
        assert!(e[0] <= 0.0 && e[1] <= 0.0 && e[2] >= w && e[3] >= h);
        let leads = plan.contours.iter().flat_map(|c| c.lead_path.iter().chain(c.lead_out_path.iter()));
        assert!(leads.into_iter().all(|q| q[0] >= e[0] && q[1] >= e[1] && q[0] <= e[2] && q[1] <= e[3]));
    }
}
