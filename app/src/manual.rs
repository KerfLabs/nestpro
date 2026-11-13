//! Manual machining on the sheet, like the original's manual cutting
//! (see MECHANICS.md "Manual machining"): pick contours one by one in
//! the order they should be cut; where you click is where the cut starts, and the
//! direction can be reversed. A sweep across parts, or a box round them, takes
//! whole parts (MM8, MM9; `engine::sheetpick`, with the cut technology panel's
//! gestures). Picked contours and parts are cut first, the rest automatically.

use crate::cnc::{self, ManualCut};
use crate::cutpanel::{self, Gesture};
use crate::engine::sheetpick::{self, Gathered, PartSweep};
use crate::nest::P;
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

/// Pick tolerance around a contour, in screen pixels (like element capture).
pub const PICK_PX: f32 = 8.0;

impl App {
    fn manual_list(&mut self) -> &mut Vec<ManualCut> {
        let s = self.cur_sheet;
        if self.manual_cuts.len() <= s {
            self.manual_cuts.resize(s + 1, vec![]);
        }
        &mut self.manual_cuts[s]
    }

    /// Pick the contour nearest `p` (sheet mm) as the next manual cut.
    pub fn manual_pick(&mut self, p: P, tol: f64) {
        let s = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(s).cloned() else { return };
        let (parts, reverse) = (self.parts.clone(), self.manual_reverse);
        // Each pick is its own undo step (undo restores the cut order, RN5).
        let before = self.snapshot();
        match crate::engine::manual_pick(&parts, &sheet, self.manual_list(), p, tol, reverse) {
            Ok(msg) => {
                self.push_undo(before);
                self.refresh();
                self.status = msg;
            }
            Err(e) => self.status = e,
        }
    }

    /// Take the parts a sweep or box gathered whole, as one undo step (MM8, MM9).
    fn manual_take(&mut self, got: &Gathered) {
        let s = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(s).cloned() else { return };
        let parts = self.parts.clone();
        let before = self.snapshot();
        match sheetpick::take(&parts, &sheet, self.manual_list(), got) {
            Ok(msg) => {
                self.push_undo(before);
                self.refresh();
                self.status = msg;
            }
            Err(e) => self.status = e,
        }
    }

    /// The sweep in progress goes in (MM8): when the button is let go, or on Esc.
    fn manual_sweep_end(&mut self) {
        if let Some((sweep, _)) = self.manual_sweep.take() {
            self.manual_take(&sweep.got);
        }
    }

    /// A sweep along `path` (sheet mm), as the pointer would make it (MM8):
    /// for screenshots (`--manual-sweep=`).
    pub fn manual_sweep_along(&mut self, path: &[P]) {
        let s = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(s).cloned() else { return };
        let done = sheetpick::picked(&self.parts, &sheet, self.manual_cuts.get(s).map_or(&[][..], |v| v.as_slice()));
        let got = sheetpick::sweep(&self.parts, &sheet, &done, path, 2.0);
        self.manual_take(&got);
    }

    /// Undo pick: the last click, or the whole last sweep or box (MM9).
    pub fn manual_undo(&mut self) {
        if !self.manual_list().is_empty() {
            self.checkpoint();
            let msg = sheetpick::undo(self.manual_list());
            self.refresh();
            self.status = msg.unwrap_or_default();
        }
    }

    pub fn manual_clear(&mut self) {
        if !self.manual_list().is_empty() {
            self.checkpoint();
        }
        self.manual_list().clear();
        self.refresh();
        self.status = "Manual cuts cleared — the whole sheet is cut automatically.".into();
    }

    /// Sheet tab group: pick contours, reverse, undo, clear.
    pub fn ribbon_manual(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        let picks = self.manual_cuts.get(self.cur_sheet).map_or(0, |v| v.len());
        ribbon_group(ui, "Manual cutting", |ui| {
            if ribbon_button(ui, Icon::PickOrder, "Pick\ncontours", self.manual_mode, has)
                .on_hover_text("Click contours on the sheet in the order they should be cut; the cut starts where you click. Drag across parts to take them whole in the order you pass them, or draw a box round parts from open space. Holes of a part are always cut before its outside, and parts sitting in a hole before that hole.")
                .clicked()
            {
                self.manual_mode = !self.manual_mode;
                self.status = if self.manual_mode { "Manual cutting: click contours in cutting order, or drag across parts (or box them) to take them whole (Esc to stop).".into() } else { String::new() };
            }
            if ribbon_button(ui, Icon::Reverse, "Reverse\ndirection", self.manual_reverse, has)
                .on_hover_text("Cut the next picked contours the other way round (Shift+click does it for one pick)")
                .clicked()
            {
                self.manual_reverse = !self.manual_reverse;
            }
            if ribbon_button(ui, Icon::Undo, "Undo\npick", false, picks > 0).on_hover_text("Take back the last pick: one contour, or every part of the last sweep or box").clicked() {
                self.manual_undo();
            }
            if ribbon_button(ui, Icon::Remove, "Clear", false, picks > 0).on_hover_text("Back to fully automatic cutting").clicked() {
                self.manual_clear();
            }
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{picks} picked")).size(12.0).color(th().text_dim));
                ui.label(RichText::new("rest automatic").size(11.0).color(th().text_dim));
            });
        });
    }

    /// On the sheet canvas while picking: highlight the contour under the pointer
    /// with its start point and direction, and pick it on a click; sweep or
    /// box whole parts with a drag (MM8, MM9).
    pub fn manual_canvas(&mut self, ui: &egui::Ui, painter: &egui::Painter, resp: &egui::Response, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, scale: f32) {
        // A sweep or box left over from a drag this canvas didn't see end
        // (picking was switched off meanwhile) is dropped.
        if !ui.input(|i| i.pointer.primary_down()) && !resp.drag_stopped() {
            self.manual_sweep = None;
            self.manual_box = None;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            // A sweep still held goes in first, as on the part screen (CO11).
            self.manual_sweep_end();
            self.manual_box = None;
            self.manual_mode = false;
            return;
        }
        let tol = (PICK_PX / scale.max(1e-6)) as f64;
        let s = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(s) else { return };
        if let Some(hp) = resp.hover_pos() {
            if let Some((i, ci, q)) = cnc::nearest_contour(&self.parts, sheet, to_w(hp), tol) {
                let mut pts = cnc::sheet_contours(&self.parts[sheet[i].part], &sheet[i]).swap_remove(ci).0;
                pts.push(pts[0]);
                let done = self.manual_cuts.get(s).is_some_and(|v| v.iter().any(|m| m.placed == i && m.contour == ci));
                let col = if done { th().text_dim } else { th().accent };
                painter.add(egui::Shape::line(pts.iter().map(|&p| to_s(p)).collect(), Stroke::new(3.0, col)));
                painter.circle_filled(to_s(q), 5.0, Color32::from_rgb(248, 80, 90));
                ui.ctx().set_cursor_icon(if done { egui::CursorIcon::NotAllowed } else { egui::CursorIcon::Crosshair });
            }
            if resp.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                if shift {
                    self.manual_reverse = !self.manual_reverse;
                }
                self.manual_pick(to_w(hp), tol);
                if shift {
                    self.manual_reverse = !self.manual_reverse;
                }
            }
        }
        self.manual_gestures(ui, painter, resp, to_s, to_w, tol);
    }

    /// Sweeps and boxes over the sheet's parts (MM8, MM9), with the cut
    /// technology panel's gestures: a drag that starts on a contour sweeps
    /// whole parts, one from open space draws a box. A click is left to
    /// `manual_canvas` (one contour, MM4). Space + drag and the middle
    /// button still pan, and a straight cut line under the pointer moves.
    fn manual_gestures(&mut self, ui: &egui::Ui, painter: &egui::Painter, resp: &egui::Response, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, half: f64) {
        if ui.input(|i| i.key_down(egui::Key::Space)) || self.cut_drag.is_some() {
            // Panning takes over: a sweep still held goes in first, as on Esc.
            self.manual_sweep_end();
            return;
        }
        let s = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(s).cloned() else { return };
        let picked = |app: &App| sheetpick::picked(&app.parts, &sheet, app.manual_cuts.get(s).map_or(&[][..], |v| v.as_slice()));
        // Only a drag that starts needs the contours, to tell a sweep from a box.
        let list = if resp.drag_started_by(egui::PointerButton::Primary) { sheetpick::contours(&self.parts, &sheet) } else { vec![] };
        let mut box_from = self.manual_box.take();
        let gs = cutpanel::gestures(ui, resp, painter, &list, to_w, half, self.manual_sweep.is_some(), &mut box_from);
        self.manual_box = box_from;
        for g in gs {
            match g {
                Gesture::Click(_) => {}
                Gesture::SweepStart(at) => {
                    let done = picked(self);
                    self.manual_sweep = Some((PartSweep::start(&self.parts, &sheet, &done, at, half), done));
                }
                Gesture::SweepTo(at) => {
                    if let Some((sweep, done)) = self.manual_sweep.as_mut() {
                        sweep.to(&self.parts, &sheet, done, at, half);
                    }
                }
                Gesture::SweepEnd => self.manual_sweep_end(),
                Gesture::Box(a, b) => {
                    let got = sheetpick::boxed(&self.parts, &sheet, &self.sheet_spec(s), &picked(self), a, b);
                    self.manual_take(&got);
                }
            }
        }
        // While the sweep is held: the parts it takes, outlined and numbered
        // as they will be.
        if let Some((sweep, _)) = &self.manual_sweep {
            let picks = self.manual_cuts.get(s).map_or(0, |v| v.len());
            for (k, &i) in sweep.got.add.iter().enumerate() {
                let Some(pl) = sheet.get(i) else { continue };
                let mut pts: Vec<Pos2> = self.parts[pl.part].geometry().outline.iter().map(|&p| to_s(pl.map(p))).collect();
                if let Some(&first) = pts.first() {
                    pts.push(first);
                }
                painter.add(egui::Shape::line(pts, Stroke::new(3.0, th().accent)));
                let c = to_s([pl.x + pl.w / 2.0, pl.y + pl.h / 2.0]);
                badge(painter, c, &format!("M{}", picks + k + 1));
            }
        }
    }

    /// Order badges on the picks, M1, M2… in the order they are cut (MM8):
    /// a picked contour with its direction, a part taken whole at the first
    /// of its contours it cuts.
    pub fn manual_badges(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2) {
        let Some(plan) = self.plans.get(self.cur_sheet) else { return };
        let manual = self.manual_cuts.get(self.cur_sheet).map_or(&[][..], |v| v.as_slice());
        for (n, (k, arrow)) in sheetpick::badges(plan, manual).into_iter().enumerate() {
            let c = &plan.contours[k];
            let p = to_s(c.points[0]);
            if arrow {
                let dir = if c.points.len() > 1 { (to_s(c.points[1]) - p).normalized() } else { Vec2::X };
                painter.arrow(p, dir * 16.0, Stroke::new(2.0, th().accent));
            }
            badge(painter, p + Vec2::new(0.0, -14.0), &format!("M{}", n + 1));
        }
    }
}

/// One order badge, "M3", centred on `at`.
fn badge(painter: &egui::Painter, at: Pos2, text: &str) {
    let r = Rect::from_center_size(at, Vec2::new(if text.len() > 3 { 28.0 } else { 22.0 }, 16.0));
    painter.rect_filled(r, CornerRadius::same(8), th().accent);
    painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.5), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    //! The sheet's Manual cutting driven by the pointer, frame by frame:
    //! a click picks one contour, a drag from a contour sweeps whole parts,
    //! a drag from open space boxes them (MM4, MM8, MM9).
    use super::*;
    use crate::nest::{NestResult, Part, Placed, SheetFormat, Shape};
    use eframe::egui::{Event, Modifiers, PointerButton, RawInput, Sense};

    /// Three made-up 100 × 50 plates in a row on sheet 1, #1 at the left,
    /// with Pick contours on.
    fn app() -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "DEMO-PLATE".into(), shape: Shape::Rectangle, w: 100.0, h: 50.0, qty: 3, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let at = |x| Placed { part: 0, x, y: 100.0, w: 100.0, h: 50.0, turns: 0, mirror: false, spin: Default::default() };
        a.result = NestResult { sheets: vec![vec![at(100.0), at(300.0), at(500.0)]], formats: vec![SheetFormat::rect(2000.0, 1000.0)], method: String::new(), tried: 0 };
        a.cur_sheet = 0;
        a.manual_cuts.clear();
        a.manual_mode = true;
        a.refresh();
        a.undo.clear();
        a
    }

    /// The pointer goes down at `path[0]`, through the rest, and up at the
    /// last point, one frame each (sheet mm drawn 1:1 in screen pixels).
    fn drag(a: &mut App, path: &[[f32; 2]]) {
        drag_with(a, path, None);
    }

    /// As `drag`, with Space pressed before the pointer reaches
    /// `path[space]` and let go only after the button.
    fn drag_with(a: &mut App, path: &[[f32; 2]], space: Option<usize>) {
        let ctx = egui::Context::default();
        let n = path.len();
        let mut t = 0.0;
        let mut frame = |a: &mut App, events: Vec<Event>| {
            t += 0.05;
            let raw = RawInput { events, time: Some(t), screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))), ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| {
                let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
                a.manual_canvas(ui, &painter, &resp, &|p: P| Pos2::new(p[0] as f32, p[1] as f32), &|q: Pos2| [q.x as f64, q.y as f64], 1.0);
            });
            out.textures_delta.clear();
        };
        let pos = |k: usize| Pos2::new(path[k][0], path[k][1]);
        let button = |k: usize, pressed| Event::PointerButton { pos: pos(k), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        let key = |pressed| Event::Key { key: egui::Key::Space, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        frame(a, vec![Event::PointerMoved(pos(0))]);
        frame(a, vec![button(0, true)]);
        for k in 1..n {
            if space == Some(k) {
                frame(a, vec![key(true)]);
            }
            frame(a, vec![Event::PointerMoved(pos(k))]);
        }
        frame(a, vec![button(n - 1, false)]);
        if space.is_some() {
            frame(a, vec![key(false)]);
        }
        frame(a, vec![]);
    }

    fn picks(a: &App) -> Vec<(usize, Option<u32>)> {
        a.manual_cuts.first().map_or(vec![], |v| v.iter().map(|m| (m.placed, m.whole)).collect())
    }

    #[test]
    fn a_drag_from_a_part_sweeps_whole_parts_in_the_order_passed() {
        let mut a = app();
        // From #3's right edge to the left, over #2 and #1.
        drag(&mut a, &[[600.0, 125.0], [560.0, 125.0], [380.0, 125.0], [150.0, 125.0], [80.0, 125.0]]);
        assert_eq!(picks(&a), vec![(2, Some(1)), (1, Some(1)), (0, Some(1))], "{}", a.status);
        assert_eq!(a.status, "Manual cut: #3, #2 and #1 taken whole, each in its own cut order.");
        // The plan cuts them in that order.
        let order: Vec<usize> = a.plans[0].contours.iter().map(|c| c.part_no).collect();
        assert_eq!(order, vec![3, 2, 1]);
        // One Ctrl+Z step takes the whole sweep back.
        a.undo();
        assert_eq!(picks(&a), vec![]);
    }

    #[test]
    fn space_pressed_during_a_sweep_keeps_the_parts_swept_so_far() {
        let mut a = app();
        // From #3's right edge over #2, then Space to pan before #1: the
        // button comes up while Space is still held.
        drag_with(&mut a, &[[600.0, 125.0], [560.0, 125.0], [380.0, 125.0], [150.0, 125.0], [80.0, 125.0]], Some(3));
        assert_eq!(picks(&a), vec![(2, Some(1)), (1, Some(1))], "{}", a.status);
    }

    #[test]
    fn a_click_still_picks_one_contour_and_a_box_takes_whole_parts() {
        let mut a = app();
        // A click on #2's left edge: one contour, from there.
        drag(&mut a, &[[300.0, 125.0]]);
        assert_eq!(picks(&a), vec![(1, None)], "{}", a.status);
        // A box from open space round #1 and #3's left half: #1 only.
        drag(&mut a, &[[50.0, 50.0], [200.0, 120.0], [550.0, 200.0]]);
        assert_eq!(picks(&a), vec![(1, None), (0, Some(1))], "{}", a.status);
        // A box round everything: #2 and #1 are picked already, #3 is taken.
        drag(&mut a, &[[50.0, 50.0], [700.0, 200.0]]);
        assert_eq!(picks(&a), vec![(1, None), (0, Some(1)), (2, Some(2))], "{}", a.status);
        // Undo pick: the box, the box, then the click.
        a.manual_undo();
        a.manual_undo();
        assert_eq!(picks(&a), vec![(1, None)]);
        a.manual_undo();
        assert_eq!(picks(&a), vec![]);
    }
}
