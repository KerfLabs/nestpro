//! Manual machining on the sheet, like the original's manual cutting
//! (see MECHANICS.md "Manual machining"): pick contours one by one in
//! the order they should be cut; where you click is where the cut starts, and the
//! direction can be reversed. Picked contours are cut first, the rest automatically.

use crate::cnc::{self, ManualCut};
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

    pub fn manual_undo(&mut self) {
        if !self.manual_list().is_empty() {
            self.checkpoint();
            self.manual_list().pop();
            self.refresh();
            self.status = "Last manual pick removed.".into();
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
            if ribbon_button(ui, Icon::CutPath, "Pick\ncontours", self.manual_mode, has)
                .on_hover_text("Click contours on the sheet in the order they should be cut. The cut starts where you click; holes of a part are always cut before its outside.")
                .clicked()
            {
                self.manual_mode = !self.manual_mode;
                self.status = if self.manual_mode { "Manual cutting: click contours in cutting order (Esc to stop).".into() } else { String::new() };
            }
            if ribbon_button(ui, Icon::Rotate, "Reverse\ndirection", self.manual_reverse, has)
                .on_hover_text("Cut the next picked contours the other way round (Shift+click does it for one pick)")
                .clicked()
            {
                self.manual_reverse = !self.manual_reverse;
            }
            if ribbon_button(ui, Icon::Undo, "Undo\npick", false, picks > 0).clicked() {
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
    /// with its start point and direction, and pick it on a click.
    pub fn manual_canvas(&mut self, ui: &egui::Ui, painter: &egui::Painter, resp: &egui::Response, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, scale: f32) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
    }

    /// Order badges on manually cut contours.
    pub fn manual_badges(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2) {
        let Some(plan) = self.plans.get(self.cur_sheet) else { return };
        let mut n = 0;
        for c in plan.contours.iter().filter(|c| c.manual) {
            n += 1;
            let p = to_s(c.points[0]);
            let dir = if c.points.len() > 1 { (to_s(c.points[1]) - p).normalized() } else { Vec2::X };
            painter.arrow(p, dir * 16.0, Stroke::new(2.0, th().accent));
            let r = Rect::from_center_size(p + Vec2::new(0.0, -14.0), Vec2::new(22.0, 16.0));
            painter.rect_filled(r, CornerRadius::same(8), th().accent);
            painter.text(r.center(), Align2::CENTER_CENTER, format!("M{n}"), FontId::proportional(10.5), Color32::WHITE);
        }
    }
}
