//! Moving parts by hand on the Nesting screen: selecting (click, Shift/Ctrl
//! click, window), dragging with snap and slide, Ctrl+drag copies, the
//! keyboard moves (Alt+arrow = the movement value, Ctrl+arrow = to the next
//! part), turning and mirroring the selection. The rules are in
//! `engine::arrange`; MECHANICS.md "Moving parts by hand" (HM1–HM12).

use crate::engine::arrange::{self, Dir};
use crate::nest::{self, Placed, P};
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, Key, Modifiers, PointerButton, Pos2, Rect, Shape as EShape, Stroke, Vec2};

/// How far a plain arrow key moves the view, screen pixels (HM13, estimate).
pub const PAN_STEP_PX: f32 = 80.0;
/// Page Down zooms in, Page Up out, by this factor (the original's keyboard
/// zoom factor starts at 2; HM13).
pub const KEY_ZOOM: f32 = 2.0;

/// A drag of the selection (or of fresh copies of it, with Ctrl).
pub struct DragState {
    /// The parts that move (indices into the sheet).
    pub sel: Vec<usize>,
    /// The sheet as it was when the drag started (copies included).
    pub start: Vec<Placed>,
    /// The part under the pointer and where on it the pointer took hold (mm from its corner).
    pub grab_idx: usize,
    pub grab: P,
    /// The offset now and the last one that was valid.
    pub off: P,
    pub last: Option<P>,
    pub valid: bool,
    /// Ctrl+drag: the moving parts are new copies (`copied_from` = sheet length before).
    pub copied_from: Option<usize>,
    /// Gap halos of the moving parts at their start (mm, sheet coordinates).
    pub halo: Vec<Vec<P>>,
}

impl App {
    /// The selected parts on the current sheet (sorted, existing).
    pub fn selection(&self) -> Vec<usize> {
        let Some(p) = self.sel_placed else { return vec![] };
        let n = self.result.sheets.get(self.cur_sheet).map_or(0, |s| s.len());
        let mut v: Vec<usize> = std::iter::once(p).chain(self.sel_more.iter().copied()).filter(|&i| i < n).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    pub fn is_selected(&self, i: usize) -> bool {
        self.sel_placed == Some(i) || (self.sel_placed.is_some() && self.sel_more.contains(&i))
    }

    /// Select these parts (the first is the one the part list follows).
    pub fn set_selection(&mut self, v: Vec<usize>) {
        let mut it = v.into_iter();
        self.sel_placed = it.next();
        self.sel_more = it.collect();
        if let Some(p) = self.sel_placed.and_then(|i| self.result.sheets.get(self.cur_sheet)?.get(i)) {
            self.sel_part = Some(p.part);
        }
    }

    fn toggle_selected(&mut self, i: usize) {
        let mut v = self.selection();
        match v.iter().position(|&k| k == i) {
            Some(k) => {
                v.remove(k);
            }
            None => v.insert(0, i),
        }
        self.set_selection(v);
    }

    /// Ctrl+E / Ctrl+A: every part on the sheet.
    pub fn select_all_on_sheet(&mut self) {
        let n = self.result.sheets.get(self.cur_sheet).map_or(0, |s| s.len());
        self.set_selection((0..n).collect());
        self.status = format!("{n} part(s) selected.");
    }

    fn sel_label(&self, sel: &[usize]) -> String {
        match sel {
            [i] => {
                let p = self.result.sheets[self.cur_sheet][*i];
                format!("{} (#{})", self.parts[p.part].name, i + 1)
            }
            _ => format!("{} parts", sel.len()),
        }
    }

    pub fn len_txt(&self, mm: f64) -> String {
        self.cfg.nesting.units.show(mm)
    }

    /// Alt+arrow moves by the movement value without a collision test, as the
    /// original does (only the sheet stops it, HM3); Ctrl+arrow slides to the
    /// next part at the part gap or the margin, always testing for parts
    /// whatever the Collision button says (HM2). Moves held down in a row make
    /// one undo step.
    pub fn nudge(&mut self, dir: Dir, to_end: bool, now: f64) {
        let sel = self.selection();
        let si = self.cur_sheet;
        if sel.is_empty() || si >= self.result.sheets.len() {
            self.status = "Select parts on the sheet first (click, Shift+click or drag a window).".into();
            return;
        }
        let before = self.snapshot();
        let f = self.result.format(si, &self.spec);
        let limit = if to_end { f64::INFINITY } else { self.cfg.nesting.step };
        let t = arrange::slide(&self.parts, &mut self.result.sheets[si], &sel, dir, &self.spec, &f, limit, to_end);
        if t <= arrange::EPS {
            let it = if sel.len() == 1 { "it is" } else { "they are" };
            let why = if to_end { "at the part gap from the next part, or at the margin" } else { "at the margin" };
            self.status = format!("{} can't move {}: {it} {why}.", self.sel_label(&sel), dir.name());
            return;
        }
        self.push_undo_coalesced(before, now);
        self.refresh();
        let close = arrange::too_close(&self.parts, &self.result.sheets[si], &sel, &self.spec);
        let room = arrange::rooms(&self.parts, &self.result.sheets[si], &sel, &self.spec, &f, 1e5, true);
        let r = |d: Dir| format!("{} {}", d.arrow(), self.len_txt(room[d as usize]));
        let moved = format!("Moved {} {} {}", self.sel_label(&sel), self.len_txt(t), dir.name());
        self.status = if close.is_empty() {
            format!(
                "{moved}{}.  Room left: {} · {} · {} · {}",
                if to_end { " — now at the part gap / margin" } else if t + 1e-6 < limit { " — now at the margin" } else { "" },
                r(Dir::Left),
                r(Dir::Right),
                r(Dir::Down),
                r(Dir::Up)
            )
        } else {
            let names: Vec<String> = close.iter().map(|j| format!("#{}", j + 1)).collect();
            format!("⚠ {moved} — now closer than the part gap ({}) to {}. Ctrl+arrow the other way puts it back at the gap; Ctrl+Z undoes.", self.len_txt(self.spec.gap), names.join(", "))
        };
    }

    /// Turn the selection by quarter turns about its centre (HM5).
    pub fn turn_selected(&mut self, quarters: u8) {
        let sel = self.selection();
        let si = self.cur_sheet;
        if sel.is_empty() || si >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(si, &self.spec);
        match arrange::turn(&self.parts, &self.result.sheets[si], &sel, quarters, &self.spec, &f, self.collision) {
            Ok(new) => {
                self.checkpoint();
                for (k, &i) in sel.iter().enumerate() {
                    self.result.sheets[si][i] = new[k];
                }
                self.refresh();
                let ang = match quarters % 4 {
                    1 => "90° left",
                    2 => "180°",
                    _ => "90° right",
                };
                self.status = format!("Turned {} {ang}.", self.sel_label(&sel));
            }
            Err(e) => self.status = e,
        }
    }

    /// Mirror the selection left↔right (`vertical`) or top↔bottom (HM6).
    pub fn mirror_selected(&mut self, vertical: bool) {
        let sel = self.selection();
        let si = self.cur_sheet;
        if sel.is_empty() || si >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(si, &self.spec);
        match arrange::mirror(&self.parts, &self.result.sheets[si], &sel, vertical, &self.spec, &f, self.collision) {
            Ok(new) => {
                self.checkpoint();
                for (k, &i) in sel.iter().enumerate() {
                    self.result.sheets[si][i] = new[k];
                }
                self.refresh();
                self.status = format!("Mirrored {} {}.", self.sel_label(&sel), if vertical { "left↔right" } else { "top↔bottom" });
            }
            Err(e) => self.status = e,
        }
    }

    /// The Nesting screen's keys for the selected parts (HM1, HM5, HM6).
    pub fn sheet_keys(&mut self, ctx: &egui::Context) {
        // Only a text box being typed in keeps the keys (a focused button doesn't).
        if ctx.egui_wants_keyboard_input() || self.ghost.is_some() || self.cut_tool.is_some() || self.manual_mode || self.drag.is_some() || self.result.sheets.is_empty() {
            return;
        }
        let now = ctx.input(|i| i.time);
        let arrows = [(Key::ArrowLeft, Dir::Left), (Key::ArrowRight, Dir::Right), (Key::ArrowDown, Dir::Down), (Key::ArrowUp, Dir::Up)];
        let mut moves: Vec<(Dir, bool)> = vec![];
        let mut pan = Vec2::ZERO;
        let mut zoom = 0i32;
        let (mut turn, mut mirror, mut all, mut step, mut esc) = (None, None, false, false, false);
        let has = self.sel_placed.is_some();
        ctx.input_mut(|i| {
            for (k, d) in arrows {
                // Every press counts (a held key repeats).
                for _ in 0..i.count_and_consume_key(Modifiers::ALT, k) {
                    moves.push((d, false));
                }
                for _ in 0..i.count_and_consume_key(Modifiers::COMMAND, k) {
                    moves.push((d, true));
                }
                // Plain arrows look around the sheet (HM13).
                let n = i.count_and_consume_key(Modifiers::NONE, k) as f32;
                let u = d.u();
                pan += Vec2::new(-u[0] as f32, u[1] as f32) * PAN_STEP_PX * n;
            }
            zoom += i.count_and_consume_key(Modifiers::NONE, Key::PageDown) as i32;
            zoom -= i.count_and_consume_key(Modifiers::NONE, Key::PageUp) as i32;
            if i.consume_key(Modifiers::COMMAND, Key::Q) {
                turn = Some(1);
            }
            if i.consume_key(Modifiers::COMMAND, Key::Num9) {
                // The fixed-angle key turns the way the setting says (HM5).
                turn = Some(if self.cfg.nesting.keys_clockwise { 3 } else { 1 });
            }
            if i.consume_key(Modifiers::COMMAND, Key::W) {
                turn = Some(3);
            }
            if i.consume_key(Modifiers::COMMAND, Key::E) || i.consume_key(Modifiers::COMMAND, Key::A) {
                all = true;
            }
            if i.consume_key(Modifiers::COMMAND, Key::R) {
                step = true;
            }
            if i.consume_key(Modifiers::SHIFT, Key::M) {
                mirror = Some(false);
            } else if i.consume_key(Modifiers::NONE, Key::M) {
                mirror = Some(true);
            }
            if has && i.key_pressed(Key::Escape) {
                esc = true;
            }
        });
        if !has {
            moves.clear();
        }
        for (d, end) in moves {
            self.nudge(d, end, now);
        }
        self.pan += pan;
        if zoom != 0 {
            // About the middle of the view, by the keyboard zoom factor.
            let f = KEY_ZOOM.powi(zoom);
            let z = (self.zoom * f).clamp(0.3, 40.0);
            self.pan *= z / self.zoom;
            self.zoom = z;
        }
        if let Some(q) = turn {
            self.turn_selected(q);
        }
        if let Some(v) = mirror {
            self.mirror_selected(v);
        }
        if all {
            self.select_all_on_sheet();
        }
        if step {
            self.step_popup = true;
        }
        if esc && !self.step_popup {
            self.set_selection(vec![]);
            self.status = "Selection cleared.".into();
        }
    }

    /// Pointer on the sheet when no tool is active: select, drag, window, pan.
    pub fn sheet_pointer(&mut self, ui: &egui::Ui, resp: &egui::Response, to_w: &dyn Fn(Pos2) -> P, s: f32) {
        let si = self.cur_sheet;
        let (mods, space) = ui.input(|i| (i.modifiers, i.key_down(Key::Space)));
        let hit = |app: &App, w: P| app.result.sheets[si].iter().rposition(|p| nest::placed_contains(&app.parts, p, w));
        // Middle button, or Space + left button: pan.
        if resp.dragged_by(PointerButton::Middle) || (space && resp.dragged_by(PointerButton::Primary)) {
            self.pan += resp.drag_delta();
        }
        if resp.drag_started_by(PointerButton::Primary) && !space {
            // Where the button went down, so a quick drag takes the part pressed on.
            if let Some(pp) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                let w = to_w(pp);
                // A straight cut line under the pointer moves ("Modify Edge").
                if self.cut_drag_begin(w, (crate::cutoff::LINE_PICK_PX / s) as f64) {
                    self.panning = false;
                } else if self.manual_mode {
                    self.panning = true;
                } else if let Some(i) = hit(self, w) {
                    if !self.is_selected(i) {
                        if mods.shift {
                            self.toggle_selected(i);
                        } else {
                            self.set_selection(vec![i]);
                        }
                    }
                    self.start_drag(i, w, mods.command);
                    self.panning = false;
                } else {
                    // Empty sheet: a selection window (HM7).
                    self.window_sel = Some((w, w, mods.shift || mods.command));
                    self.panning = false;
                }
            }
        }
        if resp.dragged_by(PointerButton::Primary) && !space {
            if self.cut_drag.is_some() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    self.cut_drag_move(to_w(pp));
                }
            } else if self.panning {
                self.pan += resp.drag_delta();
            } else if let (Some(ws), Some(pp)) = (self.window_sel.as_mut(), resp.interact_pointer_pos()) {
                ws.1 = to_w(pp);
            } else if let Some(pp) = resp.interact_pointer_pos() {
                self.drag_move(to_w(pp), s, mods.alt);
            }
        }
        if resp.drag_stopped() {
            self.cut_drag_end();
            self.end_drag();
            if let Some((a, b, add)) = self.window_sel.take() {
                let mut v = arrange::window_select(&self.result.sheets[si], a, b);
                if add {
                    let mut cur = self.selection();
                    cur.retain(|i| !v.contains(i));
                    v.splice(0..0, cur);
                }
                let n = v.len();
                self.set_selection(v);
                self.status = if n == 0 { "No part lies wholly inside the window.".into() } else { format!("{n} part(s) selected — Alt+arrows move them by the step, Ctrl+arrows slide them to the next part.") };
            }
            self.panning = false;
        }
        if resp.clicked() && !self.manual_mode {
            if let Some(pp) = resp.interact_pointer_pos() {
                match hit(self, to_w(pp)) {
                    Some(i) if mods.shift || mods.command => self.toggle_selected(i),
                    Some(i) => self.set_selection(vec![i]),
                    None if mods.shift || mods.command => {}
                    None => self.set_selection(vec![]),
                }
            }
        }
    }

    fn start_drag(&mut self, grab_idx: usize, w: P, copy: bool) {
        let si = self.cur_sheet;
        self.checkpoint();
        let mut sel = self.selection();
        let mut copied_from = None;
        let mut grab_idx = grab_idx;
        if copy {
            // Ctrl+drag: fresh copies follow the pointer; the originals stay (HM10).
            let n = self.result.sheets[si].len();
            let copies = arrange::copies(&self.result.sheets[si], &sel, [0.0, 0.0]);
            for c in &copies {
                self.parts[c.part].qty += 1;
            }
            self.result.sheets[si].extend(copies);
            grab_idx = n + sel.iter().position(|&i| i == grab_idx).unwrap_or(0);
            sel = (n..self.result.sheets[si].len()).collect();
            copied_from = Some(n);
            self.set_selection(sel.clone());
        }
        let start = self.result.sheets[si].clone();
        let g = start[grab_idx];
        let halo = if self.cfg.nesting.halo { sel.iter().flat_map(|&i| arrange::gap_halo(&self.parts, &start[i], self.spec.gap)).collect() } else { vec![] };
        self.drag = Some(DragState { sel, start, grab_idx, grab: [w[0] - g.x, w[1] - g.y], off: [0.0, 0.0], last: copied_from.is_none().then_some([0.0, 0.0]), valid: copied_from.is_none(), copied_from, halo });
    }

    fn drag_move(&mut self, w: P, s: f32, no_snap: bool) {
        let si = self.cur_sheet;
        let Some(d) = self.drag.as_ref() else { return };
        let g = d.start[d.grab_idx];
        // The part under the pointer lands on whole millimetres; snapping and
        // sliding then place it exactly.
        let want = [(w[0] - d.grab[0]).round() - g.x, (w[1] - d.grab[1]).round() - g.y];
        let reach = if self.cfg.nesting.snap && !no_snap { (self.cfg.nesting.snap_px / s) as f64 } else { 0.0 };
        let f = self.result.format(si, &self.spec);
        let (off, ok) = arrange::drag(&self.parts, &d.start, &d.sel, d.last, want, &self.spec, &f, self.collision, reach);
        let (sel, start) = (d.sel.clone(), d.start.clone());
        for &i in &sel {
            self.result.sheets[si][i] = Placed { x: start[i].x + off[0], y: start[i].y + off[1], ..start[i] };
        }
        if let Some(d) = self.drag.as_mut() {
            d.off = off;
            d.valid = ok;
            if ok {
                d.last = Some(off);
            }
        }
    }

    fn end_drag(&mut self) {
        let si = self.cur_sheet;
        let Some(d) = self.drag.take() else { return };
        let moved = d.off[0].abs() > arrange::EPS || d.off[1].abs() > arrange::EPS;
        if !d.valid {
            self.undo.pop();
            if let Some(n) = d.copied_from {
                for c in self.result.sheets[si].drain(n..).collect::<Vec<_>>() {
                    self.parts[c.part].qty = self.parts[c.part].qty.saturating_sub(1);
                }
                self.set_selection(vec![]);
                self.status = "The copy would overlap a part or leave the sheet — not placed.".into();
            } else {
                for &i in &d.sel {
                    self.result.sheets[si][i] = d.start[i];
                }
                self.status = "Parts would collide or leave the sheet — moved back.".into();
            }
        } else if !moved && d.copied_from.is_none() {
            // A click that didn't move anything isn't an undo step.
            self.undo.pop();
            return;
        } else {
            let p = self.result.sheets[si][d.grab_idx];
            self.status = match (d.copied_from, d.sel.len()) {
                (Some(_), 1) => format!("Copied {} to X {}  Y {} (quantity now {}).", self.parts[p.part].name, self.len_txt(p.x), self.len_txt(p.y), self.parts[p.part].qty),
                (Some(_), n) => format!("Copied {n} parts (each part's quantity grew by one)."),
                (None, 1) => format!("Part moved to X {}  Y {}.", self.len_txt(p.x), self.len_txt(p.y)),
                (None, n) => format!("{n} parts moved by X {}  Y {}.", self.len_txt(d.off[0]), self.len_txt(d.off[1])),
            };
        }
        self.refresh();
    }

    /// Drawn over the sheet: the selection window and the gap halos of the
    /// parts being dragged (HM7, HM11).
    pub fn arrange_canvas(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2) {
        if let Some(d) = &self.drag {
            let col = if d.valid { th().accent } else { Color32::from_rgb(248, 80, 90) };
            let stroke = Stroke::new(1.0, col.gamma_multiply(0.8));
            for ring in &d.halo {
                let mut pts: Vec<Pos2> = ring.iter().map(|q| to_s([q[0] + d.off[0], q[1] + d.off[1]])).collect();
                if let Some(&f) = pts.first() {
                    pts.push(f);
                }
                painter.extend(EShape::dashed_line(&pts, stroke, 5.0, 4.0));
            }
        }
        if let Some((a, b, _)) = self.window_sel {
            let r = Rect::from_two_pos(to_s(a), to_s(b));
            painter.rect_filled(r, CornerRadius::ZERO, th().accent.gamma_multiply(0.12));
            painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, th().accent), egui::StrokeKind::Middle);
        }
    }

    /// Screenshots: `--select=1,3` selects those parts on the sheet;
    /// `--drag-sel=dx,dy` then holds a drag of them moved by dx, dy (mm).
    pub fn shot_arrange(&mut self) {
        let arg = |k: &str| std::env::args().find_map(|a| a.strip_prefix(k).map(str::to_string));
        let n = self.result.sheets.get(self.cur_sheet).map_or(0, |s| s.len());
        if let Some(v) = arg("--select=").and_then(|l| arrange::parse_items(&l, n).ok()) {
            self.set_selection(v);
        }
        let d: Option<Vec<f64>> = arg("--drag-sel=").map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect());
        if let (Some(d), Some(i)) = (d.filter(|d| d.len() == 2), self.sel_placed) {
            let p = self.result.sheets[self.cur_sheet][i];
            let w = [p.x + p.w / 2.0, p.y + p.h / 2.0];
            self.start_drag(i, w, false);
            let s = 0.5;
            self.drag_move([w[0] + d[0], w[1] + d[1]], s, false);
        }
    }

    /// Save Settings → Nesting after a change made outside the Settings window
    /// (not while taking automatic screenshots).
    pub fn save_prefs(&mut self) {
        if !std::env::args().any(|a| a == "--screenshot") && self.cfg.save().is_ok() {
            self.cfg_saved = self.cfg.clone();
        }
    }

    /// The ribbon's Move group: the movement value and slide-to buttons.
    pub fn move_group(&mut self, ui: &mut egui::Ui, has: bool) {
        let u = self.cfg.nesting.units;
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Step").size(11.5).color(th().text_dim))
                    .on_hover_text("The movement value: Alt + arrow keys move the selected parts this far (Ctrl+R)");
                let r = crate::units::drag_len(ui, &mut self.cfg.nesting.step, u, 1000.0);
                if r.changed() {
                    self.cfg.nesting.step = self.cfg.nesting.step.max(0.001);
                }
                if r.drag_stopped() || r.lost_focus() {
                    self.save_prefs();
                }
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Slide").size(11.5).color(th().text_dim))
                    .on_hover_text("Slide the selected parts until they meet the next part at the part gap, or the margin");
                let now = ui.input(|i| i.time);
                for d in [Dir::Left, Dir::Down, Dir::Up, Dir::Right] {
                    let tip = format!("Ctrl + {} — slide {} to the next part (at the part gap) or the margin.\nAlt + {} moves exactly by the step.", d.arrow(), d.name(), d.arrow());
                    if ui.add_enabled(has, egui::Button::new(d.arrow()).min_size(Vec2::new(22.0, 20.0))).on_hover_text(tip).clicked() {
                        self.nudge(d, true, now);
                    }
                }
            });
        });
    }

    /// Ctrl+R: set the movement value (HM1).
    pub fn step_window(&mut self, ctx: &egui::Context) {
        if !self.step_popup {
            return;
        }
        let unit = self.cfg.nesting.units;
        let mut open = true;
        let mut done = false;
        egui::Window::new("Movement value")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 120.0))
            .show(ctx, |ui| {
                ui.label("How far Alt + arrow keys move the selected parts (they don't stop at other parts).");
                ui.horizontal(|ui| {
                    ui.label("Move by");
                    let r = crate::units::drag_len(ui, &mut self.cfg.nesting.step, unit, 1000.0);
                    if r.changed() {
                        self.cfg.nesting.step = self.cfg.nesting.step.max(0.001);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let picks: &[f64] = match unit {
                        crate::units::LenUnit::Mm => &[0.1, 0.5, 1.0, 2.0, 5.0, 10.0],
                        crate::units::LenUnit::Inch => &[1.0 / 64.0, 1.0 / 32.0, 1.0 / 16.0, 1.0 / 8.0, 0.25, 1.0],
                    };
                    for &v in picks {
                        let mm = v * unit.factor();
                        let label = match unit {
                            crate::units::LenUnit::Inch => format!("{}\"", crate::units::fraction(v).unwrap_or_else(|| unit.num(mm))),
                            crate::units::LenUnit::Mm => unit.show(mm),
                        };
                        if ui.selectable_label((self.cfg.nesting.step - mm).abs() < 1e-6, label).clicked() {
                            self.cfg.nesting.step = mm;
                        }
                    }
                });
                ui.label(egui::RichText::new("Ctrl + arrow keys slide the parts all the way to the next part (at the part gap) or the margin.").size(11.5).color(th().text_dim));
                if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = true;
                }
            });
        if !open || done || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.step_popup = false;
            self.save_prefs();
            self.status = format!("Movement value: {}.", unit.show_long(self.cfg.nesting.step));
        }
    }
}
