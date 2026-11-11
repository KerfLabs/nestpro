//! The Nesting and Machining module's Machining and Simulation tabs: machine
//! the whole sheet, and play its program back with a moving head and the
//! original's Simulation Window (position slider, current operation, X/Y,
//! cutter radius, number of heads). See MECHANICS.md "Nesting and Machining:
//! machining and simulating the sheet".

use crate::cnc::SheetPlan;
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
    /// The head stands at the start of a beam run while the beam pierces.
    Pierce,
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
    /// The plan's contour this step belongs to (the rapid to it included).
    pub contour: Option<usize>,
}

fn len(path: &[P]) -> f64 {
    path.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// The sheet's machining as timed operations, in program order: each part's
/// markings first, then each contour along the beam's own path (the one the
/// program and the times use, `cnc::beam_runs`): rapid to the pierce, the
/// pierce, then the run — lead-in, contour, loops, overlap, lead-out. A
/// micro-joint is jumped with a rapid and pierced again; a joint with leads
/// leaves along its lead-out and comes back along its lead-in. Punched
/// contours go hit by hit. Each step takes the plan's time for it
/// (MECHANICS "Machining time and speeds"), so the whole run lasts as long
/// as the sheet's time.
pub fn ops(plan: &SheetPlan, part_names: &[String]) -> Vec<Op> {
    let tm = &plan.timing;
    let rapid = |a: P, b: P, contour: Option<usize>| Op { kind: OpKind::Rapid, label: "Rapid".into(), path: vec![a, b], secs: tm.rapid(a, b), contour };
    let mut out = vec![];
    let mut pos: P = [0.0, 0.0];
    let name_of = |no: usize| part_names.get(no.wrapping_sub(1)).cloned().unwrap_or_default();
    // The markings in the plan's order: those due before contour `k`.
    let mut next_mark = 0;
    let mut marks_upto = |k: usize, out: &mut Vec<Op>, pos: &mut P| {
        while next_mark < plan.marks.len() && plan.mark_before.get(next_mark).is_none_or(|&b| b <= k) {
            let (no, m) = &plan.marks[next_mark];
            if let Some(&s) = m.first() {
                out.push(rapid(*pos, s, None));
                out.push(Op { kind: OpKind::Mark, label: format!("Mark contour : part {no} ({})", name_of(*no)), path: m.clone(), secs: tm.mark(m).total(), contour: None });
                *pos = *m.last().unwrap_or(&s);
            }
            next_mark += 1;
        }
    };
    for (k, c) in plan.contours.iter().enumerate() {
        marks_upto(k, &mut out, &mut pos);
        let name = name_of(c.part_no);
        if let Some(p) = &c.punch {
            // The same split as the plan's: rapids between hits, a fixed time per hit.
            for h in &p.hits {
                out.push(rapid(pos, *h, Some(k)));
                out.push(Op { kind: OpKind::Punch, label: format!("{} : part {} ({name})", p.label(), c.part_no), path: vec![*h], secs: crate::machine::HIT_SECONDS, contour: Some(k) });
                pos = *h;
            }
            continue;
        }
        let what = if c.inner { "hole" } else { "outside" };
        let cut = format!("Cut contour {} : {what} of part {} ({name})", k + 1, c.part_no);
        let pierce = format!("Pierce contour {} : {what} of part {} ({name})", k + 1, c.part_no);
        // The contour's time split the way `Timing::contour` adds it up:
        // head down before the first pierce, head up after the last run.
        let s = &tm.settings;
        let runs = crate::cnc::beam_runs(c);
        let last = runs.len().saturating_sub(1);
        for (r, run) in runs.iter().enumerate() {
            let Some(&start) = run.first() else { continue };
            out.push(rapid(pos, start, Some(k)));
            let t = tm.run(c, run);
            let down = if r == 0 { s.head_down } else { 0.0 };
            let up = if r == last { s.head_up } else { 0.0 };
            out.push(Op { kind: OpKind::Pierce, label: pierce.clone(), path: vec![start], secs: down + t.pierce, contour: Some(k) });
            out.push(Op { kind: OpKind::Cut, label: cut.clone(), path: run.clone(), secs: t.cut + t.head + up, contour: Some(k) });
            pos = *run.last().unwrap_or(&start);
        }
    }
    // Markings of parts none of whose contours are cut.
    marks_upto(usize::MAX, &mut out, &mut pos);
    out
}

/// Where Previous / Next go from time `t`: the start of the previous / next
/// step that begins work — a pierce (each beam run), a marking or a punch
/// hit — or the very end.
pub fn step_time(ops: &[Op], t: f64, dir: i32) -> f64 {
    let mut starts = vec![];
    let mut acc = 0.0;
    for o in ops {
        if matches!(o.kind, OpKind::Pierce | OpKind::Mark | OpKind::Punch) {
            starts.push(acc);
        }
        acc += o.secs;
    }
    starts.push(acc);
    if dir > 0 {
        starts.iter().copied().find(|&s| s > t + 1e-6).unwrap_or(acc)
    } else {
        starts.iter().rev().copied().find(|&s| s < t - 1e-6).unwrap_or(0.0)
    }
}

/// How many of the plan's `n` contours are finished at time `t`.
pub fn contours_done(ops: &[Op], n: usize, t: f64) -> usize {
    let mut end = vec![0.0f64; n];
    let mut acc = 0.0;
    for o in ops {
        acc += o.secs;
        if let Some(e) = o.contour.and_then(|k| end.get_mut(k)) {
            *e = acc;
        }
    }
    end.iter().filter(|&&e| e <= t + 1e-9).count()
}

/// Where the head is at time `t`: (op index, point, share of that op done).
/// A step owns its start, so a step's first moment (where Previous / Next
/// land) shows that step, not the one before.
pub fn head_at(ops: &[Op], t: f64) -> Option<(usize, P, f64)> {
    let mut acc = 0.0;
    for (i, o) in ops.iter().enumerate() {
        if t < acc + o.secs || i + 1 == ops.len() {
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
        ops(plan, &names)
    }

    fn sim_total(&self) -> f64 {
        self.sheet_ops().iter().map(|o| o.secs).sum()
    }

    /// Step to the start of the previous / next pierce, marking or hit.
    fn sim_step(&mut self, dir: i32) {
        let ops = self.sheet_ops();
        let t = self.ssim.as_ref().map_or(0.0, |s| s.t);
        let to = step_time(&ops, t, dir);
        let s = self.ssim.get_or_insert_with(SheetSim::default);
        s.t = to;
        s.running = false;
    }

    /// The Nesting module's Machining tab.
    pub fn ribbon_nest_machining(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        ribbon_group(ui, "Machining", |ui| {
            if ribbon_button(ui, Icon::Toolpath, "Machine\nall", self.show_path, has).on_hover_text("Machine every sheet automatically and show the cutting path (Export CNC does this too as it writes each program)").clicked() {
                self.refresh();
                self.show_path = true;
                self.status = format!("{} sheet(s) machined automatically.", self.result.sheets.len());
            }
            let picks = self.manual_cuts.get(self.cur_sheet).map_or(0, |v| v.len());
            if ribbon_button(ui, Icon::Remove, "Clear cut\norder", false, has && (picks > 0 || self.show_path)).on_hover_text("Clear the hand-picked order and hide the path").clicked() {
                self.manual_clear();
                self.show_path = false;
            }
        });
        // One machine choice: the shop's machine the sheet is cut on, which
        // sets both how parts are machined and the program format (MK1).
        ribbon_group(ui, "Machine", |ui| {
            if self.machine_picker(ui) {
                self.refresh();
            }
        });
        ribbon_group(ui, "Compensation", |ui| {
            let r = crate::drawing::dropdown(ui, Icon::Kerf, "Kerf", true).on_hover_text(crate::kerf::status_line(&self.spec));
            egui::Popup::menu(&r).show(|ui| self.kerf_menu(ui));
        });
        self.ribbon_manual(ui);
        self.ribbon_verify(ui);
        ribbon_group(ui, "CNC", |ui| {
            if ribbon_button(ui, Icon::Program, "CNC\nprogram", self.show_program, has).on_hover_text("The sheet's machine program").clicked() {
                self.show_program = !self.show_program;
            }
            let on = self.setui.open && self.setui.section == crate::settings_ui::Section::Machines;
            if ribbon_button(ui, Icon::Gear, "Machine\n& output", on, true).on_hover_text("Which machine, and how its programs are written").clicked() {
                self.open_settings(crate::settings_ui::Section::Machines);
            }
            if ribbon_button_ex(ui, Icon::Export, "Export\nCNC", false, has, true).on_hover_text(crate::export_tip(self)).clicked() {
                self.export_checked();
            }
        });
    }

    /// The Nesting module's Simulation tab.
    pub fn ribbon_nest_sim(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        let total = self.sim_total();
        ribbon_group(ui, "Simulate", |ui| {
            if ribbon_button(ui, Icon::SkipStart, "Beginning", false, has).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.t = 0.0;
                s.running = false;
            }
            if ribbon_button(ui, Icon::StepBack, "Previous", false, has).clicked() {
                self.sim_step(-1);
            }
            let running = self.ssim.as_ref().is_some_and(|s| s.running);
            if ribbon_button_ex(ui, if running { Icon::Pause } else { Icon::Play }, if running { "Pause" } else { "Run" }, running, has, true).clicked() {
                let now = ui.input(|i| i.time);
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                if s.t >= total - 1e-6 {
                    s.t = 0.0;
                }
                s.running = !s.running;
                s.last = now;
            }
            if ribbon_button(ui, Icon::StepFwd, "Next", false, has).clicked() {
                self.sim_step(1);
            }
            if ribbon_button(ui, Icon::SkipEnd, "End", false, has).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.t = total;
                s.running = false;
            }
            if ribbon_button(ui, Icon::Stop, "Stop", false, self.ssim.is_some()).clicked() {
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
                ui.label(RichText::new("Simulation speed").size(12.0).color(th().text_dim));
            });
        });
        ribbon_group(ui, "Display", |ui| {
            let shown = self.ssim.as_ref().is_some_and(|s| s.window);
            if ribbon_button(ui, Icon::SimWindow, "Simulation\nwindow", shown, true).clicked() {
                let s = self.ssim.get_or_insert_with(SheetSim::default);
                s.window = !s.window;
            }
            let follow = self.ssim.as_ref().is_none_or(|s| s.follow);
            if ribbon_button(ui, Icon::FollowHead, "Follow\nhead", follow, true).on_hover_text("Keep the cutting head in view while it runs (when zoomed in)").clicked() {
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
        // A cut is drawn at least as wide as the real kerf at this zoom.
        let px_per_mm = (to_s([1.0, 0.0]).x - to_s([0.0, 0.0]).x).abs();
        let kerf_px = (crate::kerf::kerf_mm(&self.cur_spec()) as f32 * px_per_mm).max(2.4);
        let Some((cur, head)) = paint_progress(painter, &ops, sim.t, to_s, kerf_px) else { return };
        // Follow the head: if it leaves the middle of the view, bring it back.
        let h = to_s(head);
        if sim.follow && sim.running && !rect.shrink2(rect.size() * 0.2).contains(h) {
            self.pan += rect.center() - h;
            ui.ctx().request_repaint();
        }
        if sim.window {
            let w = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.bottom() - 132.0), Vec2::new(560.0f32.min(rect.width() - 24.0), 120.0));
            let cutter = crate::kerf::cutter_radius(&crate::ct1::for_spec(&self.cur_spec()));
            if sim_window(ui, "sim_window", w, &mut sim.t, total, &ops[cur], head, cutter) {
                sim.running = false;
            }
        }
        self.ssim = Some(sim);
    }
}

/// Colours of the simulation: done cuts (C057), markings, punch hits, pierces.
fn op_colour(kind: OpKind) -> Color32 {
    match kind {
        OpKind::Cut => crate::drawing::rgb(crate::drawing::C_DONE),
        OpKind::Mark => Color32::from_rgb(140, 255, 140),
        OpKind::Punch => Color32::from_rgb(255, 140, 60),
        OpKind::Pierce => Color32::from_rgb(248, 80, 90),
        OpKind::Rapid => Color32::from_gray(150),
    }
}

/// Draw the program done up to time `t` (the sheet's and the part's
/// simulation): cuts green and at least `kerf_px` wide, markings light
/// green, punch hits orange, a pierce mark where each run started (SM2);
/// the rapid being made dashed, the last of the cut being made bright white,
/// and the head (SM3). Returns the step being made and the head's point.
pub fn paint_progress(painter: &egui::Painter, ops: &[Op], t: f64, to_s: &dyn Fn(P) -> Pos2, kerf_px: f32) -> Option<(usize, P)> {
    let (cur, head, f) = head_at(ops, t)?;
    let mut pierces = vec![];
    for (i, o) in ops.iter().enumerate().take(cur + 1) {
        let pts: Vec<P> = if i == cur { path_upto(&o.path, f) } else { o.path.clone() };
        let sp: Vec<Pos2> = pts.iter().map(|&q| to_s(q)).collect();
        match o.kind {
            OpKind::Cut => {
                painter.add(egui::Shape::line(sp, Stroke::new(kerf_px, op_colour(o.kind))));
            }
            OpKind::Mark => {
                painter.add(egui::Shape::line(sp, Stroke::new(1.6, op_colour(o.kind))));
            }
            OpKind::Punch => {
                if let Some(&q) = sp.first() {
                    painter.circle_filled(q, 3.0, op_colour(o.kind));
                }
            }
            OpKind::Pierce => pierces.extend(sp.first().copied()),
            OpKind::Rapid => {
                if i == cur {
                    painter.extend(egui::Shape::dashed_line(&sp, Stroke::new(1.0, Color32::from_rgb(120, 255, 160)), 4.0, 4.0));
                }
            }
        }
    }
    for q in pierces {
        painter.circle_filled(q, 2.6, op_colour(OpKind::Pierce));
    }
    // The cut being made right now: bright, so the head is seen on it.
    if let Some(o) = ops.get(cur).filter(|o| o.kind == OpKind::Cut || o.kind == OpKind::Mark) {
        let sp: Vec<Pos2> = path_upto(&o.path, f).iter().map(|&q| to_s(q)).collect();
        let tail = sp.len().saturating_sub(12);
        painter.add(egui::Shape::line(sp[tail..].to_vec(), Stroke::new(3.0, Color32::WHITE)));
    }
    // The head (SM3): a red ring with a cross at its point.
    let h = to_s(head);
    painter.circle_filled(h, 3.5, Color32::from_rgb(255, 230, 120));
    let red = Color32::from_rgb(255, 40, 40);
    painter.circle_stroke(h, 9.0, Stroke::new(2.0, red));
    painter.line_segment([h - Vec2::new(13.0, 0.0), h + Vec2::new(13.0, 0.0)], Stroke::new(1.5, red));
    painter.line_segment([h - Vec2::new(0.0, 13.0), h + Vec2::new(0.0, 13.0)], Stroke::new(1.5, red));
    Some((cur, head))
}

/// The Simulation Window (SM4) at `at`: time, position slider, the step
/// being made with its colour, the head's X / Y, the cutter radius and the
/// number of heads. Returns true when the slider moved `t`.
#[allow(clippy::too_many_arguments)]
pub fn sim_window(ui: &egui::Ui, id: &str, at: Rect, t: &mut f64, total: f64, op: &Op, head: P, cutter: f64) -> bool {
    let mut moved = false;
    egui::Area::new(egui::Id::unique(id)).fixed_pos(at.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(at.width() - 20.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Simulation Window").strong().color(th().text_strong));
                ui.label(RichText::new(format!("{:.1} / {:.1} s", *t, total)).color(th().text_dim));
            });
            ui.spacing_mut().slider_width = at.width() - 40.0;
            if ui.add(egui::Slider::new(t, 0.0..=total).show_value(false)).changed() {
                moved = true;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Operation:").strong());
                ui.label(&op.label);
                let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(2), op_colour(op.kind));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("X:").strong());
                ui.label(format!("{:.3}", head[0]));
                ui.add_space(12.0);
                ui.label(RichText::new("Y:").strong());
                ui.label(format!("{:.3}", head[1]));
                ui.add_space(12.0);
                ui.label(RichText::new("Cutter radius:").strong());
                ui.label(format!("{cutter:.3}"));
                ui.add_space(12.0);
                ui.label(RichText::new("Number of heads:").strong());
                ui.label(format!("{HEADS}"));
            });
        });
    });
    moved
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

    fn d(a: P, b: P) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    /// Distance from `q` to a polyline.
    fn off_path(q: P, path: &[P]) -> f64 {
        path.windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let ab = [b[0] - a[0], b[1] - a[1]];
                let l2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-12);
                let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0);
                d(q, [a[0] + ab[0] * t, a[1] + ab[1] * t])
            })
            .fold(f64::MAX, f64::min)
    }

    #[test]
    fn the_head_follows_the_beam_from_lead_in_into_the_contour() {
        // Every demo part on its own (as the part's Simulation tab plays it),
        // and the first sheet: the head never jumps, each contour is cut along
        // the program's beam runs (lead-in first, then the contour), and
        // micro-joint gaps stay uncut.
        let a = App::new(None);
        let mut plans: Vec<(crate::cnc::SheetPlan, Vec<String>)> = (0..a.parts.len())
            .map(|i| {
                let (w, h) = a.parts[i].bbox();
                let pl = crate::nest::Placed { part: i, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false };
                (crate::cnc::plan(&a.parts, &[pl], &a.spec), vec![a.parts[i].name.clone()])
            })
            .collect();
        if let Some(p) = a.plans.first() {
            plans.push((crate::cnc::plan(&a.parts, &a.result.sheets[0], &a.spec), a.result.sheets[0].iter().map(|p| a.parts[p.part].name.clone()).collect()));
            assert!(!p.contours.is_empty());
        }
        let mut joints_seen = 0;
        for (plan, names) in &plans {
            let ops = ops(plan, names);
            let total: f64 = ops.iter().map(|o| o.secs).sum();
            assert!((total - plan.seconds).abs() < 1e-6, "{total} vs {}", plan.seconds);
            // The head starts each step where the last one left it.
            for w in ops.windows(2) {
                let (e, s) = (*w[0].path.last().unwrap(), w[1].path[0]);
                assert!(d(e, s) < 1e-6, "{names:?}: '{}' ends at {e:?}, '{}' starts at {s:?}", w[0].label, w[1].label);
            }
            for (k, c) in plan.contours.iter().enumerate().filter(|(_, c)| c.punch.is_none()) {
                let cuts: Vec<&Op> = ops.iter().filter(|o| o.contour == Some(k) && o.kind == OpKind::Cut).collect();
                let runs = crate::cnc::beam_runs(c);
                assert_eq!(cuts.len(), runs.len(), "{names:?} contour {}", k + 1);
                for (o, r) in cuts.iter().zip(&runs) {
                    assert_eq!(&o.path, r);
                }
                // Each run is pierced first, where it starts.
                let pierces: Vec<&Op> = ops.iter().filter(|o| o.contour == Some(k) && o.kind == OpKind::Pierce).collect();
                assert_eq!(pierces.len(), runs.len());
                assert_eq!(pierces[0].path, vec![c.lead_in]);
                // The first run is the lead-in, then the contour from its start.
                if c.lead_path.len() >= 2 {
                    assert_eq!(&cuts[0].path[..c.lead_path.len()], &c.lead_path[..]);
                }
                assert!(cuts[0].path.iter().any(|&q| d(q, c.points[0]) < 1e-6));
                // No cut crosses a micro-joint.
                for &(ja, jb) in &c.joints {
                    let mid = crate::cnc::point_at(&c.points, (ja + jb) / 2.0);
                    for o in &cuts {
                        assert!(off_path(mid, &o.path) > (jb - ja) / 4.0, "{names:?} contour {} cut across its joint at {mid:?}", k + 1);
                    }
                    joints_seen += 1;
                }
            }
        }
        assert!(joints_seen > 0, "the demo parts have micro-joints");
        // Stepping goes pierce by pierce, then to the end.
        let (plan, names) = &plans[0];
        let ops = ops(plan, names);
        let total: f64 = ops.iter().map(|o| o.secs).sum();
        let mut t = 0.0;
        let mut steps = 0;
        while t < total - 1e-9 {
            let n = step_time(&ops, t, 1);
            assert!(n > t);
            if n < total - 1e-9 {
                let (i, _, _) = head_at(&ops, n).unwrap();
                assert!(matches!(ops[i].kind, OpKind::Pierce | OpKind::Mark | OpKind::Punch), "{:?}", ops[i].kind);
            }
            t = n;
            steps += 1;
        }
        assert_eq!(steps, ops.iter().filter(|o| matches!(o.kind, OpKind::Pierce | OpKind::Mark | OpKind::Punch)).count() + 1);
        assert!(step_time(&ops, t, -1) < t);
        assert_eq!(contours_done(&ops, plan.contours.len(), 0.0), 0);
        assert_eq!(contours_done(&ops, plan.contours.len(), total), plan.contours.len());
    }
}
