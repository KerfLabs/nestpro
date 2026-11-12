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

/// The quick ring's Copy (QR4): copies of the selection on the pointer. They
/// are only drawn until a click drops them; the sheet and the parts' order
/// quantities change only then, so cancelling has nothing to undo.
pub struct Carry {
    /// The sheet it was started on, exactly as it was. Any change to it (a
    /// command, undo, nesting, another sheet or job) ends the carry.
    pub sheet: usize,
    pub base: Vec<Placed>,
    /// The sheet's size, margin and part gap then (the ribbon edits these).
    pub limits: [f64; 4],
    /// The copies where the originals are.
    pub copies: Vec<Placed>,
    /// The copy the pointer holds, and where on it (mm from its corner).
    pub grab_idx: usize,
    pub grab: P,
    /// The offset now, the last one that fitted, and whether this one fits.
    pub off: P,
    pub last: Option<P>,
    pub valid: bool,
    /// The originals (selected again on cancel).
    pub orig: Vec<usize>,
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
        if ctx.egui_wants_keyboard_input() || self.ghost.is_some() || self.cut_tool.is_some() || self.manual_mode || self.drag.is_some() || self.carry.is_some() || self.result.sheets.is_empty() {
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
        if self.carry.is_some() {
            self.carry_pointer(ui, resp, to_w, s, mods.alt);
            return;
        }
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
                    // A drag sweeps or boxes whole parts (MM8, MM9; manual.rs).
                    self.panning = false;
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

    /// The quick ring's Copy (QR4): copies of the whole selection, spacing
    /// kept, follow the pointer from `at` (mm) until a click drops them.
    pub fn start_carry_copy(&mut self, at: P) {
        let sel = self.selection();
        let si = self.cur_sheet;
        let Some(sheet) = self.result.sheets.get(si).filter(|_| !sel.is_empty()) else { return };
        let copies = arrange::copies(sheet, &sel, [0.0, 0.0]);
        let grab_idx = sel.iter().position(|&i| nest::placed_contains(&self.parts, &sheet[i], at)).unwrap_or(0);
        let g = copies[grab_idx];
        let n = copies.len();
        self.carry = Some(Carry { sheet: si, base: sheet.clone(), limits: self.sheet_limits(), copies, grab_idx, grab: [at[0] - g.x, at[1] - g.y], off: [0.0, 0.0], last: None, valid: false, orig: sel });
        self.status = format!("Copy of {} on the pointer — click to drop it, Esc or right-click to cancel.", if n == 1 { "the part".to_string() } else { format!("{n} parts") });
    }

    fn sheet_limits(&self) -> [f64; 4] {
        let s = self.cur_spec();
        [s.w, s.h, s.margin, s.gap]
    }

    /// The carry still belongs to the sheet as it is now (nothing else has
    /// changed it, it's still shown, no other tool has started).
    fn carry_current(&self) -> bool {
        self.carry.as_ref().is_some_and(|c| {
            c.sheet == self.cur_sheet
                && self.result.sheets.get(c.sheet) == Some(&c.base)
                && self.sheet_limits() == c.limits
                && c.copies.iter().all(|p| p.part < self.parts.len())
                && self.workspace == crate::Workspace::Nesting
                && self.ghost.is_none()
                && self.cut_tool.is_none()
                && !self.manual_mode
        })
    }

    /// End a carry that no longer belongs to the sheet shown (see `carry_current`).
    pub fn check_carry(&mut self) {
        if self.carry.is_some() && !self.carry_current() {
            self.carry = None;
        }
    }

    /// Move the carried copies toward the pointer at `w` (mm), sliding and
    /// snapping as a Ctrl + drag does.
    pub(crate) fn carry_move(&mut self, w: P, s: f32, no_snap: bool) {
        self.check_carry();
        let Some(c) = self.carry.as_ref() else { return };
        let mut all = c.base.clone();
        let n = all.len();
        all.extend(c.copies.iter().copied());
        let sel: Vec<usize> = (n..all.len()).collect();
        let g = c.copies[c.grab_idx];
        let want = [(w[0] - c.grab[0]).round() - g.x, (w[1] - c.grab[1]).round() - g.y];
        let reach = if self.cfg.nesting.snap && !no_snap { (self.cfg.nesting.snap_px / s) as f64 } else { 0.0 };
        let f = self.result.format(c.sheet, &self.spec);
        let (off, ok) = arrange::drag(&self.parts, &all, &sel, c.last, want, &self.spec, &f, self.collision, reach);
        if let Some(c) = self.carry.as_mut() {
            c.off = off;
            c.valid = ok;
            if ok {
                c.last = Some(off);
            }
        }
    }

    /// Drop the carried copies where they are: only now do they go on the
    /// sheet, one undo step, each adding one to its part's order.
    pub(crate) fn carry_drop(&mut self) -> bool {
        self.check_carry();
        let Some(c) = self.carry.take() else { return false };
        if !c.valid {
            self.status = "The copy would overlap a part or leave the sheet — move it to a free spot (Esc cancels).".into();
            self.carry = Some(c);
            return false;
        }
        self.checkpoint();
        let si = c.sheet;
        let n = self.result.sheets[si].len();
        for p in arrange::copies(&c.copies, &(0..c.copies.len()).collect::<Vec<_>>(), c.off) {
            self.parts[p.part].qty += 1;
            self.result.sheets[si].push(p);
        }
        self.set_selection((n..self.result.sheets[si].len()).collect());
        self.refresh();
        let p = self.result.sheets[si][n + c.grab_idx];
        self.status = if c.copies.len() == 1 {
            format!("Copied {} to X {}  Y {} (quantity now {}).", self.parts[p.part].name, self.len_txt(p.x), self.len_txt(p.y), self.parts[p.part].qty)
        } else {
            format!("Copied {} parts (each part's quantity grew by one).", c.copies.len())
        };
        true
    }

    /// While a copy is carried: it follows the pointer; a click (or a press
    /// that wobbles into a short drag) drops it where it fits; Esc or a
    /// right-click cancels.
    fn carry_pointer(&mut self, ui: &egui::Ui, resp: &egui::Response, to_w: &dyn Fn(Pos2) -> P, s: f32, no_snap: bool) {
        if resp.dragged_by(PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }
        if let Some(hp) = resp.hover_pos() {
            self.carry_move(to_w(hp), s, no_snap);
        }
        let esc = ui.input(|i| i.key_pressed(Key::Escape));
        if esc || resp.secondary_clicked() {
            self.cancel_carry();
        } else if resp.clicked() || resp.drag_stopped_by(PointerButton::Primary) {
            self.carry_drop();
        }
        ui.ctx().request_repaint();
    }

    /// Put the carried copies away; nothing was changed, so nothing to undo.
    pub fn cancel_carry(&mut self) {
        let Some(c) = self.carry.take() else { return };
        if self.carry_base_ok(&c) {
            self.set_selection(c.orig);
        }
        self.status = "Copy cancelled.".into();
    }

    fn carry_base_ok(&self, c: &Carry) -> bool {
        c.sheet == self.cur_sheet && self.result.sheets.get(c.sheet) == Some(&c.base)
    }

    /// The carried copies, see-through, green where they fit and red where not.
    pub fn carry_canvas(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2) {
        let Some(c) = self.carry.as_ref().filter(|_| self.carry_current()) else { return };
        let tone = if c.valid { Color32::from_rgb(74, 222, 128) } else { Color32::from_rgb(248, 80, 90) };
        for p in arrange::copies(&c.copies, &(0..c.copies.len()).collect::<Vec<_>>(), c.off) {
            let part = &self.parts[p.part];
            crate::ui_parts::paint_part_placed(painter, part, &p, to_s, crate::ui_parts::with_alpha(crate::ui_parts::part_color(part.color), crate::ghost::GHOST_ALPHA), Stroke::new(2.0, tone), crate::ui_parts::with_alpha(th().sheet, 150));
        }
    }

    pub(crate) fn drag_move(&mut self, w: P, s: f32, no_snap: bool) {
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

    pub(crate) fn end_drag(&mut self) {
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

    /// Overlapping parts outlined in red, and while collision checking is off
    /// a badge saying so with how many parts overlap (QR5, QR6).
    pub fn overlap_canvas(&self, painter: &egui::Painter, rect: Rect, to_s: &dyn Fn(P) -> Pos2) {
        let red = Color32::from_rgb(239, 68, 68);
        let Some(sheet) = self.result.sheets.get(self.cur_sheet) else { return };
        // While parts are being moved the list is from before the move.
        if self.drag.is_none() {
            let mut bad: Vec<usize> = self.overlaps.iter().flat_map(|&(i, j)| [i, j]).filter(|&i| i < sheet.len()).collect();
            bad.sort_unstable();
            bad.dedup();
            for i in bad {
                let (outer, holes) = nest::placed_shape(&self.parts, &sheet[i]);
                for ring in std::iter::once(&outer).chain(holes.iter()) {
                    let mut pts: Vec<Pos2> = ring.iter().map(|&q| to_s(q)).collect();
                    pts.extend(pts.first().copied());
                    painter.add(EShape::line(pts, Stroke::new(2.5, red)));
                }
            }
        }
        let (_, off, _) = self.sheet_problems();
        if self.collision && self.overlaps.is_empty() && off == 0 {
            return;
        }
        let mut text = if self.collision { String::new() } else { "Collisions OFF".to_string() };
        if !self.overlaps.is_empty() || off > 0 {
            let n = self.overlaps.len();
            if !text.is_empty() {
                text += "  ·  ";
            }
            let mut what = vec![];
            if n > 0 {
                what.push(format!("{n} overlap{}", if n == 1 { "" } else { "s" }));
            }
            if off > 0 {
                what.push(format!("{off} part{} off the sheet", if off == 1 { "" } else { "s" }));
            }
            text += &format!("{} — the CNC export will warn", what.join(", "));
        }
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.5), Color32::WHITE);
        let size = galley.size() + Vec2::new(20.0, 10.0);
        let chip = Rect::from_min_size(Pos2::new(rect.center().x - size.x / 2.0, rect.top() + 10.0), size);
        painter.rect_filled(chip.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(8), Color32::from_black_alpha(60));
        painter.rect_filled(chip, CornerRadius::same(8), if self.overlaps.is_empty() { Color32::from_rgb(180, 83, 9) } else { red });
        painter.galley(chip.min + Vec2::new(10.0, 5.0), galley, Color32::WHITE);
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

#[cfg(test)]
mod quick_ring_tests {
    use super::*;
    use crate::nest::{NestResult, Part, Shape, SheetFormat};
    use crate::radial::{Act, Radial, Ring};

    fn app() -> App {
        let mut a = App::new(None);
        let r = Part { name: "A".into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![r];
        a.spec.gap = 5.0;
        a.result = NestResult {
            sheets: vec![vec![Placed { part: 0, x: 100.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false }, Placed { part: 0, x: 300.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false }]],
            formats: vec![SheetFormat::rect(2000.0, 1000.0)],
            method: String::new(),
            tried: 0,
        };
        a.cur_sheet = 0;
        a.refresh();
        a
    }

    #[test]
    fn the_sheet_rings_offer_what_can_run() {
        // QR1: the part ring needs a selection; the sheet ring's sheet arrows,
        // undo and redo follow what there is.
        let mut a = app();
        assert!(!a.radial_enabled(Act::Turn) && !a.radial_enabled(Act::CopyHere));
        assert!(a.radial_enabled(Act::RenestSheet) && a.radial_enabled(Act::CutOff) && a.radial_enabled(Act::Stock));
        assert!(!a.radial_enabled(Act::NextSheet) && !a.radial_enabled(Act::PrevSheet));
        assert!(!a.radial_enabled(Act::Undo) && !a.radial_enabled(Act::Redo));
        a.radial_run(Act::Stock, Radial::new(Pos2::ZERO, Ring::Sheet));
        assert!(a.stockui.open);
        a.select_all_on_sheet();
        assert_eq!(a.selection(), vec![0, 1]);
        for k in [Act::Turn, Act::TurnBack, Act::CopyHere, Act::Delete] {
            assert!(a.radial_enabled(k), "{k:?}");
        }
        // Parts may not be mirrored unless their rotations say so (QR2).
        assert!(!a.radial_enabled(Act::MirrorLr) && !a.radial_enabled(Act::MirrorTb));
        a.parts[0].rots.mirrored = [true; 4];
        assert!(a.radial_enabled(Act::MirrorLr) && a.radial_enabled(Act::MirrorTb));
        a.radial_run(Act::Delete, Radial::new(Pos2::ZERO, Ring::OnPart));
        assert!(a.result.sheets.is_empty() || a.result.sheets[0].is_empty());
        assert!(a.radial_enabled(Act::Undo));
        a.radial_run(Act::Undo, Radial::new(Pos2::ZERO, Ring::Sheet));
        assert_eq!(a.result.sheets[0].len(), 2);
        assert!(a.radial_enabled(Act::Redo));
    }

    #[test]
    fn turn_back_goes_to_the_previous_allowed_turn() {
        // QR3: the next allowed turn is a quarter left, back is a quarter right;
        // a part that may only turn 180° flips between 0° and 180° either way.
        let mut a = app();
        a.set_selection(vec![0]);
        a.radial_run(Act::TurnBack, Radial::new(Pos2::ZERO, Ring::OnPart));
        assert_eq!(a.result.sheets[0][0].turns, 3, "{}", a.status);
        a.radial_run(Act::Turn, Radial::new(Pos2::ZERO, Ring::OnPart));
        assert_eq!(a.result.sheets[0][0].turns, 0, "{}", a.status);
        a.parts[0].rots.plain = [true, false, true, false];
        a.turn_back_selected();
        assert_eq!(a.result.sheets[0][0].turns, 2, "{}", a.status);
    }

    #[test]
    fn a_carried_copy_follows_the_pointer_and_drops_on_a_click() {
        // QR4: copies of the whole selection, layout kept, on the pointer; the
        // sheet and the order change only when they're dropped.
        let mut a = app();
        a.set_selection(vec![0, 1]);
        let undo0 = a.undo.len();
        a.radial_run(Act::CopyHere, Radial { at: Some([150.0, 150.0]), ..Radial::new(Pos2::ZERO, Ring::OnPart) });
        assert!(a.carry.is_some(), "{}", a.status);
        assert_eq!((a.result.sheets[0].len(), a.parts[0].qty, a.undo.len()), (2, 2, undo0), "nothing placed yet");
        // Over the originals it can't drop and stays on the pointer.
        a.carry_move([150.0, 150.0], 1.0, true);
        assert!(!a.carry_drop() && a.carry.is_some());
        // Esc: gone, the originals selected again, nothing to undo.
        a.cancel_carry();
        assert!(a.carry.is_none());
        assert_eq!((a.result.sheets[0].len(), a.parts[0].qty, a.undo.len()), (2, 2, undo0));
        assert_eq!(a.selection(), vec![0, 1], "the originals are selected again");
        // Carried up into free sheet and dropped: both copies, 200 mm apart as before.
        a.start_carry_copy([150.0, 150.0]);
        a.carry_move([150.0, 650.0], 1.0, true);
        assert!(a.carry_drop());
        let s = &a.result.sheets[0];
        assert_eq!(s.len(), 4);
        assert_eq!((s[2].x, s[2].y, s[3].x, s[3].y), (100.0, 600.0, 300.0, 600.0));
        assert_eq!(a.parts[0].qty, 4);
        assert_eq!(a.undo.len(), undo0 + 1, "one undo step");
        assert_eq!(a.selection(), vec![2, 3], "the copies are selected");
        assert!(a.overlaps.is_empty());
    }

    #[test]
    fn any_other_change_ends_a_carried_copy() {
        // QR4: a command, another sheet or another tool ends the carry, and
        // the sheet and quantities are left as that command made them.
        let mut a = app();
        a.set_selection(vec![0]);
        a.start_carry_copy([150.0, 150.0]);
        a.remove_selected();
        assert!(a.carry.is_none(), "Delete ends it");
        assert_eq!((a.result.sheets[0].len(), a.parts[0].qty), (1, 2));
        a.set_selection(vec![0]);
        a.start_carry_copy([350.0, 150.0]);
        a.undo();
        a.check_carry();
        assert!(a.carry.is_none(), "undo changed the sheet");
        a.set_selection(vec![0, 1]);
        a.start_carry_copy([150.0, 150.0]);
        a.workspace = crate::Workspace::Jobs;
        a.check_carry();
        assert!(a.carry.is_none(), "another screen");
        a.workspace = crate::Workspace::Nesting;
        a.start_carry_copy([150.0, 150.0]);
        a.start_ghost(0, false);
        a.check_carry();
        assert!(a.carry.is_none(), "another tool");
        a.stop_ghost();
        a.start_carry_copy([150.0, 150.0]);
        a.spec.margin += 50.0;
        a.check_carry();
        assert!(a.carry.is_none(), "the margin changed on the ribbon");
        assert_eq!((a.result.sheets[0].len(), a.parts[0].qty), (2, 2));
    }

    #[test]
    fn turn_and_mirror_grey_out_when_the_part_may_not() {
        // QR2: a part that may only turn 180° can still Turn (to 180°); one
        // that may not turn or mirror at all greys them.
        let mut a = app();
        a.set_selection(vec![0]);
        a.parts[0].rots.plain = [true, false, true, false];
        assert!(a.radial_enabled(Act::Turn) && a.radial_enabled(Act::TurnBack));
        a.parts[0].rots.plain = [true, false, false, false];
        a.parts[0].rots.mirrored = [false; 4];
        for k in [Act::Turn, Act::TurnBack, Act::MirrorLr, Act::MirrorTb] {
            assert!(!a.radial_enabled(k), "{k:?}");
        }
        assert!(a.radial_enabled(Act::CopyHere) && a.radial_enabled(Act::Delete));
        // Two parts turn 90° together: greyed when one may not.
        a.parts[0].rots.plain = [true; 4];
        a.set_selection(vec![0, 1]);
        assert!(a.radial_enabled(Act::Turn));
        a.parts[0].rotate = false;
        assert!(!a.radial_enabled(Act::Turn));
    }

    #[test]
    fn overlapping_parts_are_found_after_a_move_with_collision_off() {
        // QR5, QR6: with collision off a part may land on another; it is
        // outlined and counted until it is moved clear.
        let mut a = app();
        a.collision = false;
        a.set_selection(vec![1]);
        a.cfg.nesting.step = 150.0;
        a.nudge(Dir::Left, false, 1.0);
        assert_eq!(a.overlaps, vec![(0, 1)], "{}", a.status);
        a.radial_run(Act::Undo, Radial::new(Pos2::ZERO, Ring::Sheet));
        assert!(a.overlaps.is_empty());
    }
}
