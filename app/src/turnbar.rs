//! Turning placed parts to any angle by hand (HM14–HM17): the turn bar under
//! the selected parts (turn left or right by the step, type an angle, choose
//! the step) and the round handle above them that turns them by dragging.
//! The rules are in `engine::arrange` (`turn_by`, `turned_about`);
//! MECHANICS.md "Moving parts by hand".

use crate::engine::arrange;
use crate::nest::{self, Placed, P};
use crate::theme::th;
use crate::ui_parts::{draw_icon_with, with_alpha, Icon};
use crate::App;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Key, PointerButton, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, Vec2};

/// The steps offered on the turn bar, degrees; 0 = free (HM16).
pub const STEPS: [f64; 7] = [0.0, 5.0, 10.0, 15.0, 30.0, 45.0, 90.0];
/// A free turn by the handle lands on tenths of a degree (HM15).
pub const FREE_RESOLUTION: f64 = 0.1;
/// What the turn buttons and Ctrl+Q / Ctrl+W turn by while the step is free.
pub const FREE_KEY_STEP: f64 = 1.0;
/// The handle sits this far above the selection's box, screen px.
pub const HANDLE_GAP_PX: f32 = 28.0;
/// The handle's radius, screen px.
pub const HANDLE_R_PX: f32 = 9.0;
/// How near the pointer must be to take the handle, screen px.
const HANDLE_PICK_PX: f32 = 13.0;
/// The custom step the bar offers until one is chosen, degrees.
pub const CUSTOM_STEP_DEFAULT: f64 = 7.5;

/// A turn of the selection by dragging its handle (HM15). The sheet shows the
/// turned parts while the button is held; letting go turns them for good (one
/// undo step) and finds room as any turn does.
pub struct TurnDrag {
    /// The parts that turn (sorted indices into the sheet).
    pub sel: Vec<usize>,
    /// The sheet as it was when the drag started.
    pub start: Vec<Placed>,
    /// The middle of the selection's box, which they turn about (mm).
    pub centre: P,
    /// The pointer's direction from the centre when the drag started, degrees.
    pub a0: f64,
    /// The turn now (degrees, counter-clockwise) and whether the parts fit there.
    pub deg: f64,
    pub valid: bool,
    /// The pointer (mm).
    pub at: P,
}

/// A turn by the handle of `raw` degrees, rounded to the step (as the
/// original rounds a turn while dragging, G3: Round(angle / step) · step);
/// a step of 0 (free) rounds to a tenth of a degree.
pub fn snap_turn(raw: f64, step: f64) -> f64 {
    let s = if step > 0.0 { step } else { FREE_RESOLUTION };
    let a = ((raw / s).round() * s * 1e6).round() / 1e6;
    if a == 0.0 { 0.0 } else { a }
}

/// An angle in (-180°, 180°].
pub fn wrap180(a: f64) -> f64 {
    let a = a.rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// The direction from `c` to `p`, degrees counter-clockwise from +X.
fn dir_deg(c: P, p: P) -> f64 {
    (p[1] - c[1]).atan2(p[0] - c[0]).to_degrees()
}

/// A turn in words: "15° left", "90° right", "180°".
pub fn turn_words(deg: f64) -> String {
    let a = wrap180(deg);
    if (a.abs() - 180.0).abs() < nest::ANGLE_EPS {
        "180°".into()
    } else {
        format!("{} {}", nest::angle_text(a.abs()), if a > 0.0 { "left" } else { "right" })
    }
}

/// A typed angle: "30", "-12.5°", "45,5".
pub fn parse_angle(s: &str) -> Option<f64> {
    s.trim().trim_end_matches('°').trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The step's name on the bar: "Free", "15°".
fn step_label(step: f64) -> String {
    if step <= 0.0 { "Free".into() } else { nest::angle_text(step) }
}

/// A small icon button for the bar.
fn icon_button(ui: &mut egui::Ui, icon: Icon, enabled: bool) -> egui::Response {
    let t = th();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), t.hover);
    }
    let (ink, acc) = if enabled { (t.text, t.accent_fg) } else { (with_alpha(t.text_dim, 150), with_alpha(t.text_dim, 150)) };
    draw_icon_with(&ui.painter().with_clip_rect(rect.expand(1.0)), Rect::from_center_size(rect.center(), Vec2::splat(22.0)), icon, ink, acc, false);
    resp
}

/// A small turning arrow (the handle's mark), counter-clockwise.
fn turn_glyph(painter: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    let pts: Vec<Pos2> = (0..=16).map(|i| {
        let a = 0.5 + i as f32 / 16.0 * 4.4;
        c + Vec2::new(a.cos(), -a.sin()) * r
    }).collect();
    let end = *pts.last().unwrap();
    let st = Stroke::new(1.4, col);
    painter.add(EShape::line(pts, st));
    let a: f32 = 4.9;
    let tangent = Vec2::new(-a.sin(), -a.cos());
    let side = Vec2::new(tangent.y, -tangent.x);
    painter.add(EShape::line(vec![end - tangent * 3.0 + side * 2.5, end, end - tangent * 3.0 - side * 2.5], st));
}

/// What a click on the bar does.
enum BarAct {
    By(f64),
    To(f64),
    Hide,
}

impl App {
    /// What the turn buttons and Ctrl+Q / Ctrl+W turn by: the step, 1° while
    /// the step is free (HM16).
    pub fn key_turn_step(&self) -> f64 {
        let s = self.cfg.nesting.turn_step;
        if s > 0.0 { s } else { FREE_KEY_STEP }
    }

    /// The angle of the part the selection follows (the one clicked first).
    pub fn primary_angle(&self) -> Option<f64> {
        self.sel_placed.and_then(|i| self.result.sheets.get(self.cur_sheet)?.get(i)).map(|p| p.angle())
    }

    /// " — now at 30°" when one part turned.
    fn now_at(&self, sel: &[usize]) -> String {
        match (sel, self.result.sheets.get(self.cur_sheet)) {
            ([i], Some(sh)) => sh.get(*i).map_or(String::new(), |p| format!(" — now at {}", nest::angle_text(p.angle()))),
            _ => String::new(),
        }
    }

    /// Turn the selection `deg` degrees (positive = counter-clockwise) about
    /// the centre of its box (HM5, HM14), moving it to the nearest free spot
    /// when it would land on a part or off the sheet.
    pub fn turn_selected_by(&mut self, deg: f64) {
        let sel = self.selection();
        let si = self.cur_sheet;
        if sel.is_empty() || si >= self.result.sheets.len() || nest::split_angle(deg) == (0, 0.0) {
            return;
        }
        let f = self.result.format(si, &self.spec);
        match arrange::turn_by(&self.parts, &self.result.sheets[si], &sel, deg, &self.spec, &f, self.collision) {
            Ok(new) => {
                self.checkpoint();
                for (k, &i) in sel.iter().enumerate() {
                    self.result.sheets[si][i] = new[k];
                }
                self.refresh();
                self.status = format!("Turned {} {}{}.", self.sel_label(&sel), turn_words(deg), self.now_at(&sel));
            }
            Err(e) => self.status = e,
        }
    }

    /// Turn so the selection's first part sits at `angle`; the others turn
    /// with it (HM14: the typed angle, Ctrl+T).
    pub fn turn_selected_to(&mut self, angle: f64) {
        let Some(a) = self.primary_angle() else { return };
        let d = wrap180(angle - a);
        if d.abs() < nest::ANGLE_EPS {
            self.status = format!("Already at {}.", nest::angle_text(a));
            return;
        }
        self.turn_selected_by(d);
    }

    /// The handle and the bar belong on the sheet: parts are selected and no
    /// other tool or move has the pointer.
    pub fn turn_tools_on(&self) -> bool {
        self.workspace == crate::Workspace::Nesting
            && self.ghost.is_none()
            && self.cut_tool.is_none()
            && self.cut_drag.is_none()
            && !self.manual_mode
            && self.carry.is_none()
            && self.drag.is_none()
            && self.turn_drag.is_none()
            && self.window_sel.is_none()
            && self.ssim.is_none()
            && self.radial.is_none()
            && !self.selection().is_empty()
    }

    /// The middle of the selection's box and the handle's place above it (mm)
    /// when the selected parts may take any angle (HM15); `s` = screen px per mm.
    pub fn turn_handle(&self, s: f32) -> Option<(P, P)> {
        if !self.turn_tools_on() {
            return None;
        }
        let sheet = self.result.sheets.get(self.cur_sheet)?;
        let sel = self.selection();
        if !sel.iter().all(|&i| nest::free_angles_allowed(&self.parts[sheet[i].part], sheet[i].mirror)) {
            return None;
        }
        let b = arrange::group_box(sheet, &sel);
        let c = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        Some((c, [c[0], b[3] + (HANDLE_GAP_PX / s) as f64]))
    }

    fn on_turn_handle(&self, w: P, s: f32) -> bool {
        self.turn_handle(s).is_some_and(|(_, h)| ((w[0] - h[0]).hypot(w[1] - h[1]) as f32) * s <= HANDLE_PICK_PX)
    }

    /// Take the handle at `w` (mm): the turn starts (one undo step).
    pub fn start_turn_drag(&mut self, w: P, s: f32) -> bool {
        let Some((c, _)) = self.turn_handle(s) else { return false };
        self.checkpoint();
        let start = self.result.sheets[self.cur_sheet].clone();
        self.turn_drag = Some(TurnDrag { sel: self.selection(), start, centre: c, a0: dir_deg(c, w), deg: 0.0, valid: true, at: w });
        self.status = "Turning — the step snaps the angle (hold Alt to turn freely); let go to place, Esc puts the parts back.".into();
        true
    }

    /// The handle dragged to `w` (mm): the parts turn by the angle the pointer
    /// went round their centre, rounded to the step (`free`: to 0.1°).
    pub fn turn_drag_move(&mut self, w: P, free: bool) {
        let si = self.cur_sheet;
        let Some(d) = self.turn_drag.as_ref() else { return };
        let step = if free { 0.0 } else { self.cfg.nesting.turn_step };
        let deg = snap_turn(wrap180(dir_deg(d.centre, w) - d.a0), step);
        let mut trial = d.start.clone();
        for (k, p) in arrange::turned_about(&self.parts, &d.start, &d.sel, deg).into_iter().enumerate() {
            trial[d.sel[k]] = p;
        }
        let f = self.result.format(si, &self.spec);
        let valid = arrange::group_ok(&self.parts, &trial, &d.sel, [0.0, 0.0], &self.spec, &f, self.collision);
        self.result.sheets[si] = trial;
        if let Some(d) = self.turn_drag.as_mut() {
            d.deg = deg;
            d.valid = valid;
            d.at = w;
        }
    }

    /// Let go of the handle: the turn is made as any turn is (room found when
    /// the parts land on another part or off the sheet).
    pub fn end_turn_drag(&mut self) {
        let Some(d) = self.turn_drag.take() else { return };
        let si = self.cur_sheet;
        if si >= self.result.sheets.len() {
            return;
        }
        if nest::split_angle(d.deg) == (0, 0.0) {
            self.result.sheets[si] = d.start;
            self.undo.pop();
            self.status = "Not turned.".into();
            return;
        }
        let f = self.result.format(si, &self.spec);
        match arrange::turn_by(&self.parts, &d.start, &d.sel, d.deg, &self.spec, &f, self.collision) {
            Ok(new) => {
                let mut sheet = d.start;
                for (k, &i) in d.sel.iter().enumerate() {
                    sheet[i] = new[k];
                }
                self.result.sheets[si] = sheet;
                self.refresh();
                let moved = if d.valid { "" } else { " — moved to the nearest free spot" };
                self.status = format!("Turned {} {}{}{moved}.", self.sel_label(&d.sel), turn_words(d.deg), self.now_at(&d.sel));
            }
            Err(e) => {
                self.result.sheets[si] = d.start;
                self.undo.pop();
                self.status = e;
            }
        }
    }

    /// Esc while turning: the parts go back as they were.
    pub fn cancel_turn_drag(&mut self) {
        let Some(d) = self.turn_drag.take() else { return };
        if let Some(sh) = self.result.sheets.get_mut(self.cur_sheet) {
            *sh = d.start;
        }
        self.undo.pop();
        self.status = "Turn cancelled.".into();
    }

    /// The handle on the sheet: hovering it shows a hand; pressing on it and
    /// dragging turns the selection (HM15). True while it has the pointer.
    pub(crate) fn turn_pointer(&mut self, ui: &egui::Ui, resp: &egui::Response, to_w: &dyn Fn(Pos2) -> P, s: f32) -> bool {
        if self.turn_drag.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.cancel_turn_drag();
                return true;
            }
            if resp.dragged_by(PointerButton::Primary) {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let alt = ui.input(|i| i.modifiers.alt);
                    self.turn_drag_move(to_w(pp), alt);
                }
            }
            if resp.drag_stopped() {
                self.end_turn_drag();
            }
            return true;
        }
        let on = |app: &App, p: Option<Pos2>| p.is_some_and(|p| app.on_turn_handle(to_w(p), s));
        if on(self, resp.hover_pos()) {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        if resp.drag_started_by(PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos());
            if let Some(o) = origin.filter(|&o| on(self, Some(o))) {
                return self.start_turn_drag(to_w(o), s);
            }
        }
        // A click on the handle keeps the selection.
        resp.clicked() && on(self, resp.interact_pointer_pos())
    }

    /// Drawn over the sheet: the handle above the selection and, while it is
    /// dragged, the angle the parts turn by (HM15).
    pub fn turn_canvas(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2, s: f32) {
        let t = th();
        if let Some(d) = &self.turn_drag {
            let col = if d.valid { t.accent } else { Color32::from_rgb(248, 80, 90) };
            let c = to_s(d.centre);
            let p = to_s(d.at);
            let r = (p - c).length().max(24.0);
            let ray = |a: f64| c + Vec2::new(a.to_radians().cos() as f32, -(a.to_radians().sin() as f32)) * r;
            painter.extend(EShape::dashed_line(&[c, ray(d.a0)], Stroke::new(1.0, col.gamma_multiply(0.6)), 4.0, 4.0));
            painter.line_segment([c, ray(d.a0 + d.deg)], Stroke::new(1.5, col));
            let arc: Vec<Pos2> = (0..=32).map(|k| ray(d.a0 + d.deg * k as f64 / 32.0)).map(|q| c + (q - c) * 0.35).collect();
            painter.add(EShape::line(arc, Stroke::new(2.0, col)));
            painter.circle_filled(c, 3.0, col);
            let h = ray(d.a0 + d.deg);
            painter.circle_filled(h, HANDLE_R_PX, col);
            turn_glyph(painter, h, HANDLE_R_PX * 0.5, Color32::WHITE);
            // The angle next to the pointer.
            let mut text = turn_words(d.deg);
            if d.deg == 0.0 {
                text = "0°".into();
            }
            if let [i] = d.sel[..] {
                if let Some(pl) = self.result.sheets.get(self.cur_sheet).and_then(|sh| sh.get(i)) {
                    text += &format!("  ·  at {}", nest::angle_text(pl.angle()));
                }
            }
            if !d.valid {
                text += if self.collision { "  ·  no room here: it moves to the nearest free spot" } else { "  ·  off the sheet" };
            }
            let galley = painter.layout_no_wrap(text, FontId::proportional(12.5), Color32::WHITE);
            let size = galley.size() + Vec2::new(16.0, 8.0);
            // Up and right of the pointer, kept inside the view.
            let mut chip = Rect::from_min_size(p + Vec2::new(18.0, -size.y - 14.0), size);
            let view = painter.clip_rect().shrink(6.0);
            if chip.top() < view.top() {
                chip = chip.translate(Vec2::new(0.0, size.y + 32.0));
            }
            if chip.right() > view.right() {
                chip = chip.translate(Vec2::new(-(size.x + 36.0), 0.0));
            }
            painter.rect_filled(chip.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(7), Color32::from_black_alpha(70));
            painter.rect_filled(chip, CornerRadius::same(7), col);
            painter.galley(chip.min + Vec2::new(8.0, 4.0), galley, Color32::WHITE);
            return;
        }
        let Some((c, h)) = self.turn_handle(s) else { return };
        let hb = to_s(h);
        let top = to_s([c[0], h[1] - (HANDLE_GAP_PX / s) as f64]);
        let hot = self.cursor_mm.is_some_and(|w| self.on_turn_handle(w, s));
        painter.line_segment([top, hb + Vec2::new(0.0, HANDLE_R_PX)], Stroke::new(1.2, with_alpha(Color32::WHITE, 170)));
        painter.circle_filled(hb + Vec2::new(0.0, 1.5), HANDLE_R_PX + 1.0, Color32::from_black_alpha(60));
        painter.circle_filled(hb, HANDLE_R_PX, if hot { t.accent } else { t.card });
        painter.circle_stroke(hb, HANDLE_R_PX, Stroke::new(1.5, t.accent));
        turn_glyph(painter, hb, HANDLE_R_PX * 0.5, if hot { Color32::WHITE } else { t.accent_fg });
    }

    /// The turn bar under the selected parts (HM17): turn left or right by the
    /// step, the angle of the part the selection follows (type one and press
    /// Enter; Ctrl+T goes there), and the step — free, 5° to 90° or your own.
    pub fn turn_bar(&mut self, ui: &egui::Ui, canvas: Rect, to_s: &dyn Fn(P) -> Pos2) {
        if !self.cfg.nesting.turn_bar || !self.turn_tools_on() {
            self.turn_focus = false;
            return;
        }
        let t = th();
        let sel = self.selection();
        let sheet = &self.result.sheets[self.cur_sheet];
        let b = arrange::group_box(sheet, &sel);
        let bottom = to_s([(b[0] + b[2]) / 2.0, b[1]]);
        let top = to_s([(b[0] + b[2]) / 2.0, b[3]]);
        let step = self.key_turn_step();
        let can_left = arrange::can_turn_by(&self.parts, sheet, &sel, step);
        let can_right = arrange::can_turn_by(&self.parts, sheet, &sel, -step);
        let free = sel.iter().all(|&i| nest::free_angles_allowed(&self.parts[sheet[i].part], sheet[i].mirror));
        let angle = self.primary_angle().unwrap_or(0.0);
        let many = sel.len() > 1;
        // Under the parts, or above the handle when there is no room below.
        const BAR_H: f32 = 66.0;
        let (pos, pivot) = if bottom.y + 12.0 + BAR_H <= canvas.bottom() - 4.0 {
            (bottom + Vec2::new(0.0, 12.0), Align2::CENTER_TOP)
        } else {
            (top - Vec2::new(0.0, HANDLE_GAP_PX + HANDLE_R_PX + 8.0), Align2::CENTER_BOTTOM)
        };
        let mut act = None;
        let mut step_changed = false;
        let field = egui::Id::unique("turn_bar_angle");
        egui::Area::new(egui::Id::unique("turn_bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(pivot)
            .constrain_to(canvas.shrink(4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .shadow(egui::Shadow { offset: [0, 3], blur: 14, spread: 0, color: Color32::from_black_alpha(80) })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 5.0);
                        ui.horizontal(|ui| {
                            let what = if many { "the selected parts" } else { "the part" };
                            let tip_l = format!("Turn {what} {} left (Ctrl+Q)", nest::angle_text(step));
                            if icon_button(ui, Icon::RotateLeft, can_left).on_hover_text(tip_l).clicked() {
                                act = Some(BarAct::By(step));
                            }
                            if !ui.memory(|m| m.has_focus(field)) {
                                self.turn_text = nest::angle_num(angle);
                            }
                            let tip = if many { "The angle of the part clicked first; type another and press Enter — the others turn with it (Ctrl+T)" } else { "The part's angle, counter-clockwise from the drawing; type another and press Enter (Ctrl+T)" };
                            let r = ui.add_enabled(free || !many, egui::TextEdit::singleline(&mut self.turn_text).id(field).desired_width(52.0).align(egui::Align2::RIGHT_CENTER)).on_hover_text(tip);
                            if std::mem::take(&mut self.turn_focus) {
                                r.request_focus();
                                if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), field) {
                                    let n = self.turn_text.chars().count();
                                    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                                    st.store(ui.ctx(), field);
                                }
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                match parse_angle(&self.turn_text) {
                                    Some(v) => act = Some(BarAct::To(v)),
                                    None => self.status = format!("'{}' isn't an angle — type degrees, e.g. 30 or -12.5.", self.turn_text.trim()),
                                }
                            }
                            ui.label(RichText::new("°").color(t.text_dim));
                            let tip_r = format!("Turn {what} {} right (Ctrl+W)", nest::angle_text(step));
                            if icon_button(ui, Icon::Rotate, can_right).on_hover_text(tip_r).clicked() {
                                act = Some(BarAct::By(-step));
                            }
                            ui.add_space(4.0);
                            let hint = if free { "or drag the round handle" } else { "may turn by 90° only" };
                            ui.label(RichText::new(hint).size(11.0).color(t.text_dim));
                            ui.add_space(2.0);
                            if ui.add(egui::Button::new(RichText::new("×").size(14.0).color(t.text_dim)).frame(false)).on_hover_text("Hide the turn bar (Ctrl+T or Settings → Nesting shows it again)").clicked() {
                                act = Some(BarAct::Hide);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Snap").size(11.5).color(t.text_dim)).on_hover_text("The step: what the buttons and Ctrl+Q / Ctrl+W turn by, and what a turn by the handle snaps to");
                            let cur = self.cfg.nesting.turn_step;
                            for v in STEPS {
                                let tip = if v == 0.0 { "Turn by the handle to any angle (to 0.1°); the buttons turn 1°".to_string() } else { format!("Snap to {} steps", nest::angle_text(v)) };
                                if ui.selectable_label((cur - v).abs() < 1e-9, step_label(v)).on_hover_text(tip).clicked() {
                                    self.cfg.nesting.turn_step = v;
                                    step_changed = true;
                                }
                            }
                            let custom = !STEPS.iter().any(|&v| (v - cur).abs() < 1e-9);
                            if custom {
                                self.turn_custom = cur;
                            }
                            let mut v = self.turn_custom;
                            let dv = egui::DragValue::new(&mut v).range(0.1..=crate::settings::TURN_STEP_MAX).speed(0.25).max_decimals(2).suffix("°");
                            let r = ui.scope(|ui| {
                                if custom {
                                    ui.visuals_mut().widgets.inactive.weak_bg_fill = t.accent_soft;
                                    ui.visuals_mut().widgets.inactive.bg_fill = t.accent_soft;
                                }
                                ui.add(dv)
                            });
                            let r = r.inner.on_hover_text("Your own step: drag or type a value");
                            if r.changed() || (r.clicked() && !custom) {
                                self.turn_custom = v;
                                self.cfg.nesting.turn_step = v;
                                step_changed = true;
                            }
                        });
                    });
            });
        if step_changed {
            self.save_prefs();
            self.status = format!("Turn step: {}.", step_label(self.cfg.nesting.turn_step));
        }
        match act {
            Some(BarAct::By(d)) => self.turn_selected_by(d),
            Some(BarAct::To(a)) => self.turn_selected_to(a),
            Some(BarAct::Hide) => {
                self.cfg.nesting.turn_bar = false;
                self.save_prefs();
                self.status = "Turn bar hidden — Ctrl+T or Settings → Nesting shows it again.".into();
            }
            None => {}
        }
    }

    /// Ctrl+T: type an angle for the selection — the turn bar's angle box
    /// (shown again if it was hidden).
    pub fn focus_turn_angle(&mut self) {
        if self.selection().is_empty() {
            self.status = "Select parts on the sheet first, then Ctrl+T types their angle.".into();
            return;
        }
        if !self.cfg.nesting.turn_bar {
            self.cfg.nesting.turn_bar = true;
            self.save_prefs();
        }
        self.turn_focus = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::{NestResult, Part, SheetFormat, Shape};

    fn app() -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "A".into(), shape: Shape::Rectangle, w: 100.0, h: 40.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.spec.gap = 5.0;
        let at = |x| Placed { part: 0, x, y: 200.0, w: 100.0, h: 40.0, turns: 0, mirror: false, spin: Default::default() };
        a.result = NestResult { sheets: vec![vec![at(300.0), at(600.0)]], formats: vec![SheetFormat::rect(2000.0, 1000.0)], method: String::new(), tried: 0 };
        a.cur_sheet = 0;
        a.workspace = crate::Workspace::Nesting;
        a.refresh();
        a
    }

    /// The handle `h` carried `deg` round the centre `c`.
    fn round(c: P, h: P, deg: f64) -> P {
        let (sn, cs) = deg.to_radians().sin_cos();
        let v = [h[0] - c[0], h[1] - c[1]];
        [c[0] + v[0] * cs - v[1] * sn, c[1] + v[0] * sn + v[1] * cs]
    }

    #[test]
    fn dragging_the_handle_turns_the_selection_by_the_step() {
        // HM15: the angle the pointer goes round the centre, rounded to the
        // step (Alt: to 0.1°); letting go is one undo step; Esc puts it back.
        let mut a = app();
        a.cfg.nesting.turn_step = 15.0;
        assert!(a.turn_handle(1.0).is_none(), "nothing selected, no handle");
        a.set_selection(vec![0]);
        let (c, h) = a.turn_handle(1.0).unwrap();
        assert!((c[0] - 350.0).abs() < 1e-9 && (h[1] - (240.0 + HANDLE_GAP_PX as f64)).abs() < 1e-9, "{c:?} {h:?}");
        let undo = a.undo.len();
        assert!(a.start_turn_drag(h, 1.0));
        a.turn_drag_move(round(c, h, 37.0), false);
        assert!((a.result.sheets[0][0].angle() - 30.0).abs() < 1e-9, "{:?}", a.result.sheets[0][0]);
        a.turn_drag_move(round(c, h, 37.04), true);
        assert!((a.result.sheets[0][0].angle() - 37.0).abs() < 1e-9, "{:?}", a.result.sheets[0][0]);
        a.turn_drag_move(round(c, h, -50.0), false);
        a.end_turn_drag();
        let p = a.result.sheets[0][0];
        assert!((p.angle() - 315.0).abs() < 1e-9, "{p:?} {}", a.status);
        assert!(a.status.contains("45° right"), "{}", a.status);
        assert_eq!(a.undo.len(), undo + 1);
        // Esc while turning: the part goes back, no undo step.
        let (c, h) = a.turn_handle(1.0).unwrap();
        a.start_turn_drag(h, 1.0);
        a.turn_drag_move(round(c, h, 80.0), false);
        a.cancel_turn_drag();
        assert_eq!(a.result.sheets[0][0], p);
        assert_eq!(a.undo.len(), undo + 1);
        a.undo();
        assert_eq!(a.result.sheets[0][0].angle(), 0.0);
    }

    #[test]
    fn the_turn_bar_turns_by_the_step_or_to_a_typed_angle() {
        // HM14, HM16: the buttons turn by the step (1° when free); a typed
        // angle turns the part there. Parts that may not turn 90° get no handle.
        let mut a = app();
        a.set_selection(vec![0]);
        a.cfg.nesting.turn_step = 0.0;
        assert_eq!(a.key_turn_step(), 1.0);
        a.cfg.nesting.turn_step = 10.0;
        a.turn_selected_by(-a.key_turn_step());
        assert!((a.primary_angle().unwrap() - 350.0).abs() < 1e-9, "{}", a.status);
        a.turn_selected_to(parse_angle("32.5°").unwrap());
        assert!((a.primary_angle().unwrap() - 32.5).abs() < 1e-9, "{}", a.status);
        assert!(a.status.contains("now at 32.5°"), "{}", a.status);
        a.parts[0].rotate = false;
        assert!(a.turn_handle(1.0).is_none());
    }

    #[test]
    fn a_turn_by_the_handle_snaps_to_the_step() {
        // HM15 / G3: Round(angle / step) · step; free = tenths of a degree.
        assert_eq!(snap_turn(37.0, 15.0), 30.0);
        assert_eq!(snap_turn(38.0, 15.0), 45.0);
        assert_eq!(snap_turn(-8.0, 5.0), -10.0);
        assert_eq!(snap_turn(12.345, 0.0), 12.3);
        assert_eq!(snap_turn(-0.04, 0.0), 0.0);
        assert_eq!(snap_turn(16.0, 7.5), 15.0);
        assert_eq!(wrap180(270.0), -90.0);
        assert_eq!(wrap180(-190.0), 170.0);
        assert_eq!(turn_words(15.0), "15° left");
        assert_eq!(turn_words(-90.0), "90° right");
        assert_eq!(turn_words(180.0), "180°");
        assert_eq!(parse_angle(" -12,5° "), Some(-12.5));
        assert_eq!(parse_angle("abc"), None);
    }
}
