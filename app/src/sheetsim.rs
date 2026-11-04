//! The Nesting and Machining module's Machining and Simulation tabs: machine
//! the whole sheet, and play its program back with a moving head and the
//! original's Simulation Window (position slider, current operation, X/Y,
//! cutter radius, number of heads). See MECHANICS.md "Nesting and Machining:
//! machining and simulating the sheet".

use crate::cnc::{self, SheetPlan};
use crate::nest::P;
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Stroke, Vec2};

/// Simulation speed choices (× real time).
pub const SPEEDS: [f64; 5] = [1.0, 5.0, 20.0, 50.0, 200.0];
/// Default speed (× real time).
pub const DEFAULT_SPEED: f64 = 20.0;
/// Number of cutting heads simulated (the original shows "Number of heads").
pub const HEADS: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    Rapid,
    Cut,
    Mark,
    Punch,
}

/// One step of the sheet's program, with its path and time.
#[derive(Clone, Debug)]
pub struct Op {
    pub kind: OpKind,
    pub label: String,
    pub path: Vec<P>,
    pub secs: f64,
}

fn len(path: &[P]) -> f64 {
    path.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// The sheet's machining as timed operations, in program order: per part its
/// markings first (once), then each contour — rapid to the pierce, then the
/// lead-in and contour (or the punch hits).
pub fn ops(plan: &SheetPlan, part_names: &[String], spec_material: usize, thickness: f64) -> Vec<Op> {
    let mark_speed = cnc::cut_speed(spec_material, thickness) * cnc::MARK_SPEED_FACTOR;
    let rapid = |a: P, b: P| Op { kind: OpKind::Rapid, label: "Rapid".into(), path: vec![a, b], secs: len(&[a, b]) / cnc::RAPID_MM_MIN * 60.0 };
    let mut out = vec![];
    let mut pos: P = [0.0, 0.0];
    let mut marked: Vec<usize> = vec![];
    for (k, c) in plan.contours.iter().enumerate() {
        let name = part_names.get(c.part_no - 1).cloned().unwrap_or_default();
        if !marked.contains(&c.part_no) {
            marked.push(c.part_no);
            for (_, m) in plan.marks.iter().filter(|m| m.0 == c.part_no) {
                if let Some(&s) = m.first() {
                    out.push(rapid(pos, s));
                    out.push(Op { kind: OpKind::Mark, label: format!("Mark contour : part {} ({name})", c.part_no), path: m.clone(), secs: len(m) / mark_speed * 60.0 });
                    pos = *m.last().unwrap_or(&s);
                }
            }
        }
        if let Some(p) = &c.punch {
            for h in &p.hits {
                out.push(rapid(pos, *h));
                out.push(Op { kind: OpKind::Punch, label: format!("{} : part {} ({name})", p.label(), c.part_no), path: vec![*h], secs: crate::machine::HIT_SECONDS });
                pos = *h;
            }
            continue;
        }
        out.push(rapid(pos, c.lead_in));
        let mut path = c.lead_path.clone();
        if path.is_empty() {
            path.push(c.lead_in);
        }
        path.extend(c.points.iter().copied());
        path.push(c.points[0]);
        if let Some(lo) = c.lead_out {
            path.push(lo);
        }
        let what = if c.inner { "hole" } else { "outside" };
        out.push(Op { kind: OpKind::Cut, label: format!("Cut contour {} : {what} of part {} ({name})", k + 1, c.part_no), secs: len(&path) / c.feed.max(1.0) * 60.0 + cnc::pierce_time(thickness), path });
        pos = c.points[0];
    }
    out
}

/// Where the head is at time `t`: (op index, point, share of that op done).
pub fn head_at(ops: &[Op], t: f64) -> Option<(usize, P, f64)> {
    let mut acc = 0.0;
    for (i, o) in ops.iter().enumerate() {
        if t <= acc + o.secs || i + 1 == ops.len() {
            let f = if o.secs > 0.0 { ((t - acc) / o.secs).clamp(0.0, 1.0) } else { 1.0 };
            return Some((i, point_along(&o.path, f), f));
        }
        acc += o.secs;
    }
    None
}

/// Point at share `f` of a polyline's length.
pub fn point_along(path: &[P], f: f64) -> P {
    if path.len() < 2 {
        return path.first().copied().unwrap_or([0.0, 0.0]);
    }
    // The whole way (within rounding of the time sums): exactly the last point.
    if f >= 1.0 - 1e-9 {
        return *path.last().unwrap();
    }
    let total = len(path);
    let mut want = total * f;
    for w in path.windows(2) {
        let l = len(w);
        // Zero-length steps (a contour's start repeated after its lead-in) are
        // skipped - stopping on one left the head behind the cut.
        if l <= 0.0 {
            continue;
        }
        if want <= l {
            let t = want / l;
            return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
        }
        want -= l;
    }
    *path.last().unwrap()
}

/// The part of a polyline up to share `f`.
pub fn path_upto(path: &[P], f: f64) -> Vec<P> {
    if path.len() < 2 {
        return path.to_vec();
    }
    let total = len(path);
    let mut want = total * f;
    let mut out = vec![path[0]];
    for w in path.windows(2) {
        let l = len(w);
        if want <= l {
            let t = if l > 0.0 { want / l } else { 0.0 };
            out.push([w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t]);
            return out;
        }
        out.push(w[1]);
        want -= l;
    }
    out
}

/// Simulation state of the current sheet.
#[derive(Clone, Debug)]
pub struct SheetSim {
    pub t: f64,
    pub running: bool,
    pub speed: f64,
    pub last: f64,
    /// The Simulation Window is shown.
    pub window: bool,
    /// Keep the head in view (pans the sheet while it runs).
    pub follow: bool,
}

impl Default for SheetSim {
    fn default() -> Self {
        SheetSim { t: 0.0, running: false, speed: DEFAULT_SPEED, last: 0.0, window: true, follow: true }
    }
}

impl App {
    fn sheet_ops(&self) -> Vec<Op> {
        let Some(plan) = self.plans.get(self.cur_sheet) else { return vec![] };
        let names: Vec<String> = self.result.sheets.get(self.cur_sheet).map_or(vec![], |s| s.iter().map(|p| self.parts[p.part].name.clone()).collect());
        ops(plan, &names, self.spec.material, self.spec.thickness)
    }

    fn sim_total(&self) -> f64 {
        self.sheet_ops().iter().map(|o| o.secs).sum()
    }

    /// Step to the start of the previous / next cutting operation.
    fn sim_step(&mut self, dir: i32) {
        let ops = self.sheet_ops();
        let t = self.ssim.as_ref().map_or(0.0, |s| s.t);
        let mut starts = vec![];
        let mut acc = 0.0;
        for o in &ops {
            if o.kind != OpKind::Rapid {
                starts.push(acc);
            }
            acc += o.secs;
        }
        starts.push(acc);
        let to = if dir > 0 { starts.iter().copied().find(|&s| s > t + 1e-6).unwrap_or(acc) } else { starts.iter().rev().copied().find(|&s| s < t - 1e-6).unwrap_or(0.0) };
        let s = self.ssim.get_or_insert_with(SheetSim::default);
        s.t = to;
        s.running = false;
    }

    /// The Nesting module's Machining tab.
    pub fn ribbon_nest_machining(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        ribbon_group(ui, "Machining", |ui| {
            if ribbon_button_ex(ui, Icon::CutPath, "Machine\nAll", self.show_path, has, true).on_hover_text("Machine every sheet automatically and show the cutting path").clicked() {
                self.refresh();
                self.show_path = true;
                self.status = format!("{} sheet(s) machined automatically.", self.result.sheets.len());
            }
            let picks = self.manual_cuts.get(self.cur_sheet).map_or(0, |v| v.len());
            if ribbon_button(ui, crate::ui_parts::Icon::Remove, "Delete\ninstructions", false, has && (picks > 0 || self.show_path)).on_hover_text("Clear the hand-picked order and hide the path").clicked() {
                self.manual_clear();
                self.show_path = false;
            }
        });
        ribbon_group(ui, "Machine", |ui| {
            if self.machine_picker(ui) {
                self.refresh();
            }
        });
        self.ribbon_manual(ui);
        self.ribbon_verify(ui);
        ribbon_group(ui, "CNC", |ui| {
            if ribbon_button(ui, Icon::Program, "View\nCNC", self.show_program, has).on_hover_text("The sheet's machine program").clicked() {
                self.show_program = !self.show_program;
            }
            let on = self.setui.open && self.setui.section == crate::settings_ui::Section::Machines;
            if ribbon_button(ui, Icon::Numbers, "Machine\n& output", on, true).on_hover_text("Which machine, and how its programs are written").clicked() {
                self.open_settings(crate::settings_ui::Section::Machines);
            }
            if ribbon_button(ui, Icon::Export, "Export\nCNC", false, has).on_hover_text("Save every sheet's program").clicked() {
                self.export();
            }
        });
    }

    /// The Nesting module's Simulation tab.
    pub fn ribbon_nest_sim(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        let total = self.sim_total();
        ribbon_group(ui, "Simulate", |ui| {
            if ribbon_button(ui, Icon::Prev, "Beginning", false, has).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.t = 0.0;
                s.running = false;
            }
            if ribbon_button(ui, Icon::Undo, "Previous", false, has).clicked() {
                self.sim_step(-1);
            }
            let running = self.ssim.as_ref().is_some_and(|s| s.running);
            if ribbon_button_ex(ui, Icon::NestAll, if running { "Pause" } else { "Run" }, running, has, true).clicked() {
                let now = ui.input(|i| i.time);
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                if s.t >= total - 1e-6 {
                    s.t = 0.0;
                }
                s.running = !s.running;
                s.last = now;
            }
            if ribbon_button(ui, Icon::Redo, "Next", false, has).clicked() {
                self.sim_step(1);
            }
            if ribbon_button(ui, Icon::Next, "End", false, has).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.t = total;
                s.running = false;
            }
            if ribbon_button(ui, Icon::Clear, "Stop", false, self.ssim.is_some()).clicked() {
                self.ssim = None;
            }
        });
        ribbon_group(ui, "Speed", |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);
                let cur = self.ssim.as_ref().map_or(DEFAULT_SPEED, |s| s.speed);
                egui::ComboBox::from_id_salt("sim_speed").width(90.0).selected_text(format!("{cur:.0}×")).show_ui(ui, |ui| {
                    for sp in SPEEDS {
                        if ui.selectable_label(cur == sp, format!("{sp:.0}× real time")).clicked() {
                            self.ssim.get_or_insert_with(SheetSim::default).speed = sp;
                        }
                    }
                });
                ui.label(RichText::new("Simulation Speed").size(11.0).color(th().text_dim));
            });
        });
        ribbon_group(ui, "Display", |ui| {
            let shown = self.ssim.as_ref().is_some_and(|s| s.window);
            if ribbon_button(ui, Icon::Times, "Simulation\nWindow", shown, true).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.window = !s.window;
            }
            let follow = self.ssim.as_ref().is_none_or(|s| s.follow);
            if ribbon_button(ui, Icon::Fit, "Follow\nhead", follow, true).on_hover_text("Keep the cutting head in view while it runs (when zoomed in)").clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.follow = !s.follow;
            }
        });
    }

    /// Draw the simulation on the sheet canvas and its window.
    pub fn sheet_sim_canvas(&mut self, ui: &egui::Ui, painter: &egui::Painter, rect: Rect, to_s: &dyn Fn(P) -> Pos2) {
        let Some(mut sim) = self.ssim.clone() else { return };
        let ops = self.sheet_ops();
        if ops.is_empty() {
            return;
        }
        let total: f64 = ops.iter().map(|o| o.secs).sum();
        let now = ui.input(|i| i.time);
        if sim.running {
            sim.t = (sim.t + (now - sim.last).clamp(0.0, 0.25) * sim.speed).min(total);
            sim.last = now;
            if sim.t >= total {
                sim.running = false;
                self.status = format!("Simulation finished: {:.0} s of machining.", total);
            }
            ui.ctx().request_repaint();
        }
        let Some((cur, head, f)) = head_at(&ops, sim.t) else { return };
        // Done so far: cuts green, markings light green, punch hits orange; rapids dashed.
        let cut = crate::drawing::rgb(crate::drawing::C_DONE);
        let mark = Color32::from_rgb(140, 255, 140);
        let punch = Color32::from_rgb(255, 140, 60);
        for (i, o) in ops.iter().enumerate().take(cur + 1) {
            let pts: Vec<P> = if i == cur { path_upto(&o.path, f) } else { o.path.clone() };
            let sp: Vec<Pos2> = pts.iter().map(|&q| to_s(q)).collect();
            match o.kind {
                OpKind::Cut => {
                    painter.add(egui::Shape::line(sp, Stroke::new(2.4, cut)));
                }
                OpKind::Mark => {
                    painter.add(egui::Shape::line(sp, Stroke::new(1.6, mark)));
                }
                OpKind::Punch => {
                    if let Some(&q) = sp.first() {
                        painter.circle_filled(q, 3.0, punch);
                    }
                }
                OpKind::Rapid => {
                    if i == cur {
                        painter.extend(egui::Shape::dashed_line(&sp, Stroke::new(1.0, Color32::from_rgb(120, 255, 160)), 4.0, 4.0));
                    }
                }
            }
        }
        // The cut being made right now: bright, so the head is seen on it.
        if let Some(o) = ops.get(cur).filter(|o| o.kind != OpKind::Rapid) {
            let sp: Vec<Pos2> = path_upto(&o.path, f).iter().map(|&q| to_s(q)).collect();
            let tail = sp.len().saturating_sub(12);
            painter.add(egui::Shape::line(sp[tail..].to_vec(), Stroke::new(3.0, Color32::WHITE)));
        }
        // The head (SM3): a red ring with a cross at its point.
        let h = to_s(head);
        painter.circle_filled(h, 3.5, Color32::from_rgb(255, 230, 120));
        // Follow the head: if it leaves the middle of the view, bring it back.
        if sim.follow && sim.running && !rect.shrink2(rect.size() * 0.2).contains(h) {
            self.pan += rect.center() - h;
            ui.ctx().request_repaint();
        }
        let red = Color32::from_rgb(255, 40, 40);
        painter.circle_stroke(h, 9.0, Stroke::new(2.0, red));
        painter.line_segment([h - Vec2::new(13.0, 0.0), h + Vec2::new(13.0, 0.0)], Stroke::new(1.5, red));
        painter.line_segment([h - Vec2::new(0.0, 13.0), h + Vec2::new(0.0, 13.0)], Stroke::new(1.5, red));
        // Simulation Window.
        if sim.window {
            let w = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.bottom() - 132.0), Vec2::new(560.0f32.min(rect.width() - 24.0), 120.0));
            egui::Area::new(egui::Id::unique("sim_window")).fixed_pos(w.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.set_width(w.width() - 20.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Simulation Window").strong().color(th().text_strong));
                        ui.label(RichText::new(format!("{:.1} / {:.1} s", sim.t, total)).color(th().text_dim));
                    });
                    let mut t = sim.t;
                    ui.spacing_mut().slider_width = w.width() - 40.0;
                    if ui.add(egui::Slider::new(&mut t, 0.0..=total).show_value(false)).changed() {
                        sim.t = t;
                        sim.running = false;
                    }
                    let op = &ops[cur];
                    let col = match op.kind {
                        OpKind::Cut => cut,
                        OpKind::Mark => mark,
                        OpKind::Punch => punch,
                        OpKind::Rapid => Color32::from_gray(150),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Operation :").strong());
                        ui.label(&op.label);
                        let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(2), col);
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("X :").strong());
                        ui.label(format!("{:.3}", head[0]));
                        ui.add_space(12.0);
                        ui.label(RichText::new("Y :").strong());
                        ui.label(format!("{:.3}", head[1]));
                        ui.add_space(12.0);
                        ui.label(RichText::new("Cutter radius :").strong());
                        ui.label(format!("{:.3}", crate::cutoff::KERF_MM * 0.5));
                        ui.add_space(12.0);
                        ui.label(RichText::new("Number of heads :").strong());
                        ui.label(format!("{HEADS}"));
                    });
                });
            });
        }
        self.ssim = Some(sim);
    }
}

#[cfg(test)]
mod sim_tests {
    use super::*;

    #[test]
    fn the_head_walks_the_program_in_order() {
        let mut a = App::new(None);
        a.cur_sheet = 0;
        let ops = a.sheet_ops();
        assert!(!ops.is_empty());
        assert_eq!(ops[0].kind, OpKind::Rapid);
        assert!(ops.iter().any(|o| o.kind == OpKind::Cut));
        // Time adds up to the end, and the head sits at the end of the last op there.
        let total: f64 = ops.iter().map(|o| o.secs).sum();
        let (i, p, f) = head_at(&ops, total).unwrap();
        assert_eq!(i, ops.len() - 1);
        assert!((f - 1.0).abs() < 1e-9);
        assert_eq!(p, *ops[i].path.last().unwrap());
        // A repeated point (zero-length step) doesn't stop the head.
        assert_eq!(point_along(&[[0.0, 0.0], [0.0, 0.0], [10.0, 0.0]], 0.5), [5.0, 0.0]);
        // At every moment the head sits at the end of the drawn progress.
        for k in 0..=40 {
            let (i, p, f) = head_at(&ops, total * k as f64 / 40.0).unwrap();
            let tip = *path_upto(&ops[i].path, f).last().unwrap();
            assert!((p[0] - tip[0]).abs() < 1e-6 && (p[1] - tip[1]).abs() < 1e-6, "head {p:?} vs tip {tip:?}");
        }
        // Halfway along a straight path is its midpoint.
        assert_eq!(point_along(&[[0.0, 0.0], [10.0, 0.0]], 0.5), [5.0, 0.0]);
        assert_eq!(path_upto(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], 0.75).last().copied(), Some([10.0, 5.0]));
        // Stepping lands on the next cutting operation.
        a.sim_step(1);
        assert!(a.ssim.as_ref().unwrap().t > 0.0);
        a.ssim = None;
        let _ = a.sim_total();
        let _ = &mut a;
    }
}
