//! The cut technology panel on the part's drawing screen (MECHANICS CS6,
//! CS7): choose a row (Cut at the size bands' quality or at one of the
//! machine's qualities, Mark, No cut, Bend line, Leave out), then click, box
//! or sweep over contours. Each paint goes through the same engine function
//! as the agents' `contour-tech` (`engine::contours::paint`); this module only
//! draws the panel and turns pointer moves into contour numbers.

use crate::dxf::{Contour, ContourState};
use crate::engine::contours::{self, Look, Paint};
use crate::engine::selection::{Take, Window};
use crate::nest::{CutQuality, Part, P};
use crate::qualities::CutQualities;
use crate::theme::th;
use crate::App;
use eframe::egui::{self, Color32};

/// A row of the panel: what painting a contour with it makes the contour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// Cut through the sheet: at a quality set by hand, or with `None` at the
    /// quality the part's size ranges and the machine's size bands give (CQ1).
    Cut(Option<CutQuality>),
    Mark,
    NoCut,
    BendLine,
    LeaveOut,
}

/// Mark contours (CS7): the Drawing module's marking green (C053, DR3).
pub const MARK: Color32 = Color32::from_rgb(0, 255, 0);
/// No cut contours (CS7; our own colour, FEATURES DRW-8).
pub const NO_CUT: Color32 = Color32::from_rgb(150, 150, 150);
/// Bend lines, dashed (CS7; our own colour, FEATURES DRW-8).
pub const BEND: Color32 = Color32::from_rgb(147, 51, 234);
/// Leave out contours, dashed and only while the panel is open, so they can
/// be painted back (CS6; our own colour).
pub const LEAVE_OUT: Color32 = Color32::from_rgb(96, 96, 104);

/// The colour a contour is drawn in on the part and on the sheet (CS7): a
/// cut contour in its cut quality's colour (CQ5), the others in their
/// state's colour.
pub fn look_colour(look: Look, names: &CutQualities) -> Color32 {
    match look {
        Look::Cut(q) => crate::techui::quality_colour(names, q),
        Look::Mark => MARK,
        Look::NoCut => NO_CUT,
        Look::BendLine => BEND,
        Look::LeaveOut => LEAVE_OUT,
    }
}

impl Row {
    /// The row's colour swatch: the colour a contour painted with it is
    /// drawn in (CS7). Cut at the size bands' quality has none of its own:
    /// each contour takes the colour of the quality its band gives.
    pub fn swatch(self, names: &CutQualities) -> Option<Color32> {
        Some(look_colour(
            match self {
                Row::Cut(q) => Look::Cut(q?),
                Row::Mark => Look::Mark,
                Row::NoCut => Look::NoCut,
                Row::BendLine => Look::BendLine,
                Row::LeaveOut => Look::LeaveOut,
            },
            names,
        ))
    }

    /// What the engine sets on a contour painted with this row.
    pub fn paint(self) -> Paint {
        match self {
            Row::Cut(q) => Paint::cut_at(q),
            Row::Mark => Paint::state(ContourState::Mark),
            Row::NoCut => Paint::state(ContourState::NoCut),
            Row::BendLine => Paint::state(ContourState::BendLine),
            Row::LeaveOut => Paint::state(ContourState::LeaveOut),
        }
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn to_segment(p: P, a: P, b: P) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - ab[0] * t).hypot(p[1] - a[1] - ab[1] * t)
}

/// Whether segments `a`–`b` and `c`–`d` cross or touch.
fn crosses(a: P, b: P, c: P, d: P) -> bool {
    let side = |p: P, q: P, r: P| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0 && (d1 != 0.0 || d2 != 0.0 || d3 != 0.0 || d4 != 0.0)
}

/// Distance between segments `a`–`b` and `c`–`d` (0 when they cross).
fn between_segments(a: P, b: P, c: P, d: P) -> f64 {
    if crosses(a, b, c, d) {
        return 0.0;
    }
    to_segment(a, c, d).min(to_segment(b, c, d)).min(to_segment(c, a, b)).min(to_segment(d, a, b))
}

/// The segments of contour `c` as drawn (a closed one back to its start).
fn segments(c: &Contour) -> Vec<(P, P)> {
    let path = c.path();
    if path.len() == 1 {
        return vec![(path[0], path[0])];
    }
    path.windows(2).map(|w| (w[0], w[1])).collect()
}

/// The contour a click at `p` picks (CS6): the nearest one whose line passes
/// within `half` mm of the point (the pick aperture), Leave out contours
/// included, since the panel shows them to set them back.
pub fn contour_at(list: &[Contour], p: P, half: f64) -> Option<usize> {
    list.iter()
        .enumerate()
        .map(|(k, c)| (k, segments(c).into_iter().map(|(a, b)| to_segment(p, a, b)).fold(f64::MAX, f64::min)))
        .filter(|&(_, d)| d <= half)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

/// The contours a box `w` picks (CS6, SW2): those wholly inside it, or with
/// a crossing window (dragged to the left) every one whose line it touches,
/// in number order.
pub fn contours_in_window(list: &[Contour], w: &Window) -> Vec<usize> {
    list.iter().enumerate().filter(|(_, c)| w.takes_line(&c.path())).map(|(k, _)| k).collect()
}

/// What a status line says when a box took no contour (SW1): a window
/// suggests dragging the other way.
pub fn none_in_window(take: Take, what: &str) -> String {
    match take {
        Take::Inside => format!("No {what} lies wholly inside the box — drag to the left to take the ones it touches."),
        Take::Touching => format!("The box touches no {what}."),
    }
}

/// The contours a sweep picks as the pointer moves from `a` to `b` with the
/// button held (CS6): every one whose line comes within `half` mm of the
/// pointer's path, in number order.
pub fn contours_on_stroke(list: &[Contour], a: P, b: P, half: f64) -> Vec<usize> {
    list.iter().enumerate().filter(|(_, c)| segments(c).into_iter().any(|(p, q)| between_segments(a, b, p, q) <= half)).map(|(k, _)| k).collect()
}

/// Where along the move from `a` to `b` (0 at `a`, 1 at `b`) the pointer
/// first comes within `half` mm of the segment `p`–`q`, if it does. The
/// distance is convex along the move, so its nearest point is found by
/// thirds, and the first point within reach before it by halving.
fn first_reach(a: P, b: P, p: P, q: P, half: f64) -> Option<f64> {
    if between_segments(a, b, p, q) > half {
        return None;
    }
    let at = |t: f64| to_segment([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], p, q);
    if at(0.0) <= half {
        return Some(0.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if at(m1) <= at(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let (mut out, mut inside) = (0.0, (lo + hi) / 2.0);
    for _ in 0..50 {
        let m = (out + inside) / 2.0;
        if at(m) <= half {
            inside = m;
        } else {
            out = m;
        }
    }
    Some(inside)
}

/// The contours a sweep passes as the pointer moves from `a` to `b` (CS6,
/// CO11): every one whose line comes within `half` mm of the pointer's
/// path, in the order the pointer reaches them.
pub fn contours_along_stroke(list: &[Contour], a: P, b: P, half: f64) -> Vec<usize> {
    let mut met: Vec<(f64, usize)> = list
        .iter()
        .enumerate()
        .filter_map(|(k, c)| segments(c).into_iter().filter_map(|(p, q)| first_reach(a, b, p, q, half)).min_by(f64::total_cmp).map(|t| (t, k)))
        .collect();
    met.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    met.into_iter().map(|m| m.1).collect()
}

/// What a drag with a row chosen does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drag {
    /// Paint every contour the pointer passes over.
    Sweep,
    /// Draw a box; the contours it takes (wholly inside it, or touched by
    /// it when dragged to the left, SW1) are painted when it is let go.
    Box,
}

/// A drag that starts on a contour sweeps; one that starts off the contours
/// draws a box (CS6).
pub fn drag_kind(list: &[Contour], start: P, half: f64) -> Drag {
    if contour_at(list, start, half).is_some() {
        Drag::Sweep
    } else {
        Drag::Box
    }
}

/// What the pointer did over the part's drawing in one frame while contours
/// are picked by click, box or sweep (CS6, CO11).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    /// A click on contour `k` (numbers from 0).
    Click(usize),
    /// A sweep starts where the button went down, on a contour (part mm).
    SweepStart(P),
    /// The pointer moved on to here with the button held (part mm).
    SweepTo(P),
    /// The button was let go: the sweep ends.
    SweepEnd,
    /// A box was let go: from where the button went down to where it came
    /// up (part mm), which also says what it takes (SW1).
    Box(Window),
}

/// Clicks, sweeps and boxes over contours `list` this frame (CS6): a click
/// picks the contour under the pointer, a drag from a contour sweeps, a drag
/// from open space draws a box (drawn here while the button is held).
/// `sweeping` says a sweep is in progress; `box_from` keeps the box's first
/// corner (screen) between frames. `half` is the pick aperture, mm.
#[allow(clippy::too_many_arguments)]
pub fn gestures(ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, list: &[Contour], to_w: &dyn Fn(egui::Pos2) -> P, half: f64, sweeping: bool, box_from: &mut Option<egui::Pos2>) -> Vec<Gesture> {
    let mut out = vec![];
    let mut sweeping = sweeping;
    let at = resp.hover_pos().or(resp.interact_pointer_pos()).map(to_w);
    if resp.clicked() {
        if let Some(k) = at.and_then(|w| contour_at(list, w, half)) {
            out.push(Gesture::Click(k));
        }
    }
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(w) = resp.interact_pointer_pos().map(to_w) {
            // Where the button went down, not where the drag was noticed.
            let from = ui.input(|i| i.pointer.press_origin()).map(to_w).unwrap_or(w);
            match drag_kind(list, from, half) {
                Drag::Sweep => {
                    out.extend([Gesture::SweepStart(from), Gesture::SweepTo(w)]);
                    sweeping = true;
                }
                Drag::Box => *box_from = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()),
            }
        }
    }
    if resp.dragged_by(egui::PointerButton::Primary) {
        if let (true, Some(w)) = (sweeping, resp.interact_pointer_pos().map(to_w)) {
            out.push(Gesture::SweepTo(w));
        }
        if let (Some(a), Some(b)) = (*box_from, resp.interact_pointer_pos()) {
            crate::ui_parts::paint_select_window(painter, egui::Rect::from_two_pos(a, b), Take::of_drag(a.x as f64, b.x as f64));
        }
    }
    if resp.drag_stopped() {
        if sweeping {
            // The last stretch, from the last held frame to where the button
            // came up, belongs to the same sweep (and the same undo step).
            if let Some(w) = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.latest_pos())).map(to_w) {
                out.push(Gesture::SweepTo(w));
            }
            out.push(Gesture::SweepEnd);
        }
        if let (Some(a), Some(b)) = (box_from.take(), resp.interact_pointer_pos()) {
            out.push(Gesture::Box(Window::dragged(to_w(a), to_w(b))));
        }
    }
    out
}

/// The panel's state: on the part's drawing screen (`B` the job's undo
/// snapshot) or in the import preview (IM11, `B` the drawing's choices).
pub struct CutPanel<B = crate::Snapshot> {
    /// The panel is shown.
    pub open: bool,
    /// The row chosen: while one is, clicks, boxes and sweeps paint (CS6).
    pub row: Option<Row>,
    /// All the same contours: also paint every contour equal to a painted
    /// one (CS5). Off by default.
    pub same: bool,
    /// A sweep in progress.
    sweep: Option<Sweep<B>>,
    /// A box being drawn from this point (screen).
    box_from: Option<egui::Pos2>,
    /// Screenshots only (--paint-card=…): show this row's hover card.
    pub card: Option<Row>,
}

impl<B> Default for CutPanel<B> {
    fn default() -> Self {
        CutPanel { open: false, row: None, same: false, sweep: None, box_from: None, card: None }
    }
}

impl<B> CutPanel<B> {
    /// A row is chosen: the pointer paints contours.
    pub fn painting(&self) -> bool {
        self.open && self.row.is_some()
    }

    /// The part a sweep in progress paints (the caller's number for it).
    pub fn sweeping(&self) -> Option<usize> {
        self.sweep.as_ref().map(|s| s.part)
    }
}

/// A sweep in progress: one undo step for all of it.
struct Sweep<B> {
    part: usize,
    row: Row,
    same: bool,
    /// Where the pointer was last (part mm).
    last: P,
    /// What it was before the sweep.
    before: B,
    /// The contours it painted, and those it passed that can't take the row.
    painted: Vec<usize>,
    refused: Vec<(usize, String)>,
}

/// Where the panel's paints go (CS6, IM11): a part of the job on its drawing
/// screen, or the part a drawing will make in the import preview. Every paint
/// is the engine's `contours::paint`; the target keeps its own undo (Ctrl+Z).
pub trait PaintTarget {
    /// What it was before a paint, to take the paint back.
    type Before;
    /// The part painted.
    fn part(&self) -> Option<&Part>;
    /// Paint contours `picks` with `row` through `contours::paint`, without an
    /// undo step: what it did, or the refusal.
    fn paint_now(&mut self, picks: &[usize], row: Row, same: bool) -> Result<String, String>;
    fn before(&self) -> Self::Before;
    /// Make the change since `before` one undo step.
    fn push_undo(&mut self, before: Self::Before);
    /// Say what was done, or why not.
    fn say(&mut self, s: String);
}

/// Paint contours `picks` (numbers from 0) with `row`, and with `same` every
/// contour equal to one of them (CS5): the engine's `contour-tech` rules, one
/// undo step. Says what it did, or why not; true when something changed.
pub fn paint_contours<T: PaintTarget>(t: &mut T, picks: &[usize], row: Row, same: bool) -> bool {
    paint_step(t, picks, row, same, &[])
}

/// `paint_contours`, naming afterwards the contours `refused` left as they are.
fn paint_step<T: PaintTarget>(t: &mut T, picks: &[usize], row: Row, same: bool, refused: &[(usize, String)]) -> bool {
    let before = t.before();
    match t.paint_now(picks, row, same) {
        Ok(m) => {
            t.push_undo(before);
            t.say(m + &left_as_they_are(refused));
            true
        }
        Err(e) => {
            t.say(first_sentence(&e));
            false
        }
    }
}

/// Of contours `picks`, those that can take `row`, and the others with the
/// engine's reason (CS4, CQ6).
fn takers<T: PaintTarget>(t: &T, picks: &[usize], row: Row) -> (Vec<usize>, Vec<(usize, String)>) {
    let Some(p) = t.part() else { return (vec![], vec![]) };
    let (mut ok, mut refused) = (vec![], vec![]);
    for &k in picks {
        match contours::refusal(p, k, row.paint()) {
            None => ok.push(k),
            Some(e) => refused.push((k, e)),
        }
    }
    (ok, refused)
}

/// Paint the contours a box gathered (CS6): those that can take `row` are
/// painted as one undo step; the rest are left as they are and named, with
/// why for the first of them.
pub fn paint_gathered<T: PaintTarget>(t: &mut T, picks: &[usize], row: Row, same: bool) -> bool {
    let (ok, refused) = takers(t, picks, row);
    if ok.is_empty() {
        if let Some(r) = refused.first() {
            t.say(first_sentence(&r.1));
        }
        return false;
    }
    paint_step(t, &ok, row, same, &refused)
}

/// Start a sweep of part `part` (the caller's number for it) at `at` (part
/// mm): the contour under the pointer is painted at once. `half` is the pick
/// aperture, mm.
#[allow(clippy::too_many_arguments)]
pub fn sweep_start<T: PaintTarget>(t: &mut T, panel: &mut CutPanel<T::Before>, part: usize, at: P, row: Row, same: bool, half: f64) {
    let before = t.before();
    panel.sweep = Some(Sweep { part, row, same, last: at, before, painted: vec![], refused: vec![] });
    sweep_to(t, panel, at, half);
}

/// The pointer moved to `at` with the button held: paint every contour it
/// passed over since the last move (CS6). `t` is the sweep's part.
pub fn sweep_to<T: PaintTarget>(t: &mut T, panel: &mut CutPanel<T::Before>, at: P, half: f64) {
    let Some(mut sw) = panel.sweep.take() else { return };
    let Some(p) = t.part() else { return };
    let met: Vec<usize> = contours_on_stroke(&contours::list(p), sw.last, at, half).into_iter().filter(|k| !sw.painted.contains(k) && !sw.refused.iter().any(|r| r.0 == *k)).collect();
    sw.last = at;
    let (ok, refused) = takers(t, &met, sw.row);
    sw.refused.extend(refused);
    if !ok.is_empty() {
        match t.paint_now(&ok, sw.row, sw.same) {
            Ok(_) => sw.painted.extend(ok),
            Err(e) => sw.refused.extend(ok.into_iter().map(|k| (k, e.clone()))),
        }
    }
    panel.sweep = Some(sw);
}

/// The button was let go: the sweep becomes one undo step, and the status
/// line says what it painted and what it left. `t` is the sweep's part.
pub fn sweep_end<T: PaintTarget>(t: &mut T, panel: &mut CutPanel<T::Before>, names: &CutQualities) {
    let Some(sw) = panel.sweep.take() else { return };
    if sw.painted.is_empty() {
        if let Some(r) = sw.refused.first() {
            t.say(first_sentence(&r.1));
        }
        return;
    }
    t.push_undo(sw.before);
    let mut painted = sw.painted;
    painted.sort_unstable();
    let n: Vec<String> = painted.iter().map(|k| (k + 1).to_string()).collect();
    let same = if sw.same { " and every contour the same as them" } else { "" };
    t.say(format!("Swept contour{} {}{same}: {}.{}", if n.len() == 1 { "" } else { "s" }, n.join(", "), sw.row.label(names), left_as_they_are(&sw.refused)));
}

/// Esc: end painting (CS6). A sweep in progress ends first; no row is
/// chosen after, and the pointer picks again.
pub fn end_painting<T: PaintTarget>(t: &mut T, panel: &mut CutPanel<T::Before>, names: &CutQualities) {
    sweep_end(t, panel, names);
    panel.row = None;
    panel.box_from = None;
    t.say("Painting ended.".into());
}

/// While a row is chosen: a click paints the contour under the pointer, a
/// drag from a contour sweeps, a drag from open space draws a box (CS6); Esc
/// ends painting. `part` is the caller's number for the part `t` paints (a
/// sweep of another part must have ended first); `half` is the pick
/// aperture, mm.
#[allow(clippy::too_many_arguments)]
pub fn canvas<T: PaintTarget>(
    t: &mut T,
    panel: &mut CutPanel<T::Before>,
    ui: &egui::Ui,
    resp: &egui::Response,
    painter: &egui::Painter,
    part: usize,
    to_s: &dyn Fn(P) -> egui::Pos2,
    to_w: &dyn Fn(egui::Pos2) -> P,
    half: f64,
    names: &CutQualities,
) {
    let Some(row) = panel.row else { return };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        end_painting(t, panel, names);
        return;
    }
    let Some(list) = t.part().map(contours::list) else { return };
    let same = panel.same;
    let at = resp.hover_pos().or(resp.interact_pointer_pos()).map(to_w);
    // The contour under the pointer, in the row's colour.
    if let Some(k) = at.and_then(|w| contour_at(&list, w, half)).filter(|_| panel.box_from.is_none()) {
        let col = row.swatch(names).unwrap_or(egui::Color32::WHITE);
        painter.add(egui::Shape::line(list[k].path().into_iter().map(to_s).collect(), egui::Stroke::new(4.0, crate::ui_parts::with_alpha(col, 150))));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let sweeping = panel.sweep.is_some();
    for g in gestures(ui, resp, painter, &list, to_w, half, sweeping, &mut panel.box_from) {
        match g {
            Gesture::Click(k) => {
                paint_contours(t, &[k], row, same);
            }
            Gesture::SweepStart(from) => sweep_start(t, panel, part, from, row, same, half),
            Gesture::SweepTo(w) => sweep_to(t, panel, w, half),
            Gesture::SweepEnd => sweep_end(t, panel, names),
            Gesture::Box(w) => {
                let picks = contours_in_window(&list, &w);
                if picks.is_empty() {
                    t.say(none_in_window(w.take, "contour"));
                } else {
                    paint_gathered(t, &picks, row, same);
                }
            }
        }
    }
}

/// The panel's hint, its rows with their swatches and hover cards, and All
/// the same contours (CS6), on the part's drawing screen and in the import
/// preview (IM11). `locked` says why nothing can be painted (the rows can't be
/// chosen then). True when a row was chosen or let go.
pub fn rows_ui<B>(ui: &mut egui::Ui, panel: &mut CutPanel<B>, names: &CutQualities, locked: Option<&str>) -> bool {
    let editable = locked.is_none();
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| {
        let hint = match locked {
            Some(why) => why,
            None if panel.row.is_some() => "Click, box or sweep (drag across) contours: each takes the row at once. Ctrl+Z undoes, Esc ends painting.",
            None => "Choose a row, then click, box or sweep over contours.",
        };
        ui.label(egui::RichText::new(hint).size(11.5).color(th().text_dim));
    });
    ui.add_space(4.0);
    let mut rows = vec![Row::Cut(None)];
    rows.extend(crate::techui::offered_qualities(names).into_iter().map(|q| Row::Cut(Some(q))));
    rows.extend([Row::Mark, Row::NoCut, Row::BendLine, Row::LeaveOut]);
    let mut clicked = false;
    for row in rows {
        if row == Row::Mark {
            ui.add_space(6.0);
        }
        let r = panel_row(ui, panel.row == Some(row), row, names, editable);
        if panel.card == Some(row) {
            let at = r.rect.left_top() + egui::Vec2::new(-286.0, 0.0);
            egui::Area::new(egui::Id::unique("cut_panel_card")).order(egui::Order::Tooltip).fixed_pos(at).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| row_card(ui, row, names));
            });
        }
        if r.clicked() {
            panel.row = if panel.row == Some(row) { None } else { Some(row) };
            clicked = true;
        }
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.add_enabled(editable, egui::Checkbox::new(&mut panel.same, "All the same contours"))
            .on_hover_text("Also paint every contour of the same shape as the one you paint (moved, not turned or mirrored); never the outline");
    });
    clicked
}

/// One row of the panel: its swatch and name, chosen or not; its hover card
/// says what it does.
fn panel_row(ui: &mut egui::Ui, chosen: bool, row: Row, names: &CutQualities, enabled: bool) -> egui::Response {
    let indent = if matches!(row, Row::Cut(Some(_))) { 22.0 } else { 6.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), 24.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let p = ui.painter();
    if chosen {
        p.rect_filled(rect.shrink2(egui::Vec2::new(3.0, 1.0)), egui::CornerRadius::same(5), th().accent);
    } else if resp.hovered() && enabled {
        p.rect_filled(rect.shrink2(egui::Vec2::new(3.0, 1.0)), egui::CornerRadius::same(5), th().hover);
    }
    let sw = egui::Rect::from_center_size(egui::Pos2::new(rect.left() + indent + 9.0, rect.center().y), egui::Vec2::splat(13.0));
    paint_swatch(p, sw, row, names);
    let text = if chosen { egui::Color32::WHITE } else if enabled { th().text } else { th().text_dim };
    let label = egui::RichText::new(row.label(names)).size(13.0).color(text);
    let label = if matches!(row, Row::Cut(Some(_))) { label } else { label.strong() };
    p.text(egui::Pos2::new(sw.right() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, label.text(), egui::FontId::proportional(13.0), text);
    resp.on_hover_ui(|ui| row_card(ui, row, names))
}

/// A part of the job painted on its drawing screen (CS6).
struct JobPart<'a> {
    app: &'a mut App,
    part: usize,
}

impl PaintTarget for JobPart<'_> {
    type Before = crate::Snapshot;

    fn part(&self) -> Option<&Part> {
        self.app.parts.get(self.part)
    }

    fn paint_now(&mut self, picks: &[usize], row: Row, same: bool) -> Result<String, String> {
        self.app.paint_now(self.part, picks, row, same)
    }

    fn before(&self) -> crate::Snapshot {
        self.app.snapshot()
    }

    fn push_undo(&mut self, before: crate::Snapshot) {
        self.app.push_undo(before);
    }

    fn say(&mut self, s: String) {
        self.app.status = s;
    }
}

impl App {
    /// The engine's paint (`contours::paint`) on the open job, without an
    /// undo step: the message, or the refusal.
    fn paint_now(&mut self, part: usize, picks: &[usize], row: Row, same: bool) -> Result<String, String> {
        if picks.is_empty() || part >= self.parts.len() {
            return Err("No contour picked.".into());
        }
        let hand_picks = self.manual_cuts.clone();
        let done = contours::paint(&mut self.parts, &self.result.sheets, &mut self.manual_cuts, part, picks, row.paint(), same)?;
        if done.nest_changed {
            self.stale = true;
        }
        // The sheets' hand-picked cut order followed the contours (CS4).
        if self.manual_cuts != hand_picks {
            self.mark_cam_changed();
        }
        self.refresh();
        Ok(done.message)
    }

    /// Run `f` on part `part` of the job with the panel (taken out of the
    /// window for the while).
    fn with_panel<R>(&mut self, part: usize, f: impl FnOnce(&mut JobPart, &mut CutPanel) -> R) -> R {
        let mut panel = std::mem::take(&mut self.cut_panel);
        let r = f(&mut JobPart { app: self, part }, &mut panel);
        self.cut_panel = panel;
        r
    }

    /// Paint contours `picks` (numbers from 0) of part `part` with `row`
    /// (`paint_contours`): one undo step (Ctrl+Z); the status line says what
    /// it did, or why not. True when something changed.
    pub fn paint_contours(&mut self, part: usize, picks: &[usize], row: Row, same: bool) -> bool {
        paint_contours(&mut JobPart { app: self, part }, picks, row, same)
    }

    /// Paint the contours of part `part` a box gathered (`paint_gathered`).
    pub fn paint_gathered(&mut self, part: usize, picks: &[usize], row: Row, same: bool) -> bool {
        paint_gathered(&mut JobPart { app: self, part }, picks, row, same)
    }

    /// Start a sweep of part `part` at `at` (part mm).
    pub fn sweep_start(&mut self, part: usize, at: P, row: Row, same: bool, half: f64) {
        self.with_panel(part, |t, p| sweep_start(t, p, part, at, row, same, half));
    }

    /// The pointer moved to `at` with the button held.
    pub fn sweep_to(&mut self, at: P, half: f64) {
        if let Some(part) = self.cut_panel.sweeping() {
            self.with_panel(part, |t, p| sweep_to(t, p, at, half));
        }
    }

    /// The button was let go: the sweep is one undo step.
    pub fn sweep_end(&mut self) {
        let names = crate::ct1::for_spec(&self.spec).qualities;
        if let Some(part) = self.cut_panel.sweeping() {
            self.with_panel(part, |t, p| sweep_end(t, p, &names));
        }
    }

    /// The cut technology panel (CS6): the rows with their swatches and hover
    /// cards, and All the same contours. False when its ✖ was clicked.
    pub fn cut_panel_body(&mut self, ui: &mut egui::Ui) -> bool {
        let names = crate::ct1::for_spec(&self.spec).qualities;
        let part = self.shown_part();
        crate::ui_parts::pane_header(ui, "Cut technology");
        let head = egui::Rect::from_min_size(ui.min_rect().left_top(), egui::Vec2::new(ui.available_width(), 24.0));
        let close = ui.put(egui::Rect::from_center_size(head.right_center() - egui::Vec2::new(14.0, 0.0), egui::Vec2::splat(20.0)), egui::Button::new("✖").small().frame(false));
        let open = !close.on_hover_text("Close the cut technology panel").clicked();
        let editable = part.is_some_and(|i| self.parts[i].shape == crate::nest::Shape::Custom);
        let locked = (!editable).then_some("A built-in shape's contours are always cut. Save it as a drawing and import that to set its contours.");
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("cut_panel_rows").show(ui, |ui| {
            if rows_ui(ui, &mut self.cut_panel, &names, locked) {
                self.dv.measure_on = false;
                self.dv.zoom_window = false;
                self.placing_text = false;
            }
        });
        open
    }

    /// While a row is chosen on the part's drawing screen: click, box or
    /// sweep paint part `idx` (`canvas`). `half` is the pick aperture, mm.
    #[allow(clippy::too_many_arguments)]
    pub fn cut_panel_canvas(&mut self, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, idx: usize, to_s: &dyn Fn(P) -> egui::Pos2, to_w: &dyn Fn(egui::Pos2) -> P, half: f64) {
        if self.cut_panel.row.is_none() {
            return;
        }
        if self.cut_panel.sweeping().is_some_and(|p| p != idx) {
            self.sweep_end();
        }
        let names = crate::ct1::for_spec(&self.spec).qualities;
        self.with_panel(idx, |t, p| canvas(t, p, ui, resp, painter, idx, to_s, to_w, half, &names));
    }
}

/// " Contours 7, 8 left as they are (why the first can't take it)." for the
/// status line, or nothing when every contour took the row.
fn left_as_they_are(refused: &[(usize, String)]) -> String {
    let Some(first) = refused.first() else { return String::new() };
    let n: Vec<String> = refused.iter().map(|r| (r.0 + 1).to_string()).collect();
    let (s, them) = if n.len() == 1 { ("", "it is") } else { ("s", "they are") };
    format!(" Contour{s} {} left as {them} ({}).", n.join(", "), first_sentence(&first.1))
}

impl Row {
    /// The row's name: "Cut · by size", the machine's name for a quality
    /// (CQ4), or the state's.
    pub fn label(self, names: &CutQualities) -> String {
        match self {
            Row::Cut(None) => "Cut · quality by size".into(),
            Row::Cut(Some(q)) => names.label(q),
            Row::Mark => ContourState::Mark.label().into(),
            Row::NoCut => ContourState::NoCut.label().into(),
            Row::BendLine => ContourState::BendLine.label().into(),
            Row::LeaveOut => ContourState::LeaveOut.label().into(),
        }
    }

    /// The hover card's title and its one plain sentence.
    fn card(self, names: &CutQualities) -> (String, String) {
        let s = match self {
            Row::Cut(None) => "Cut through the sheet. Each contour takes the quality its size band gives and shows in that quality's colour.".to_string(),
            Row::Cut(Some(q)) => format!("Cut through the sheet at {}, set by hand: it wins over the size bands.", names.label(q)),
            Row::Mark => "Etch the line on the surface without cutting through, always at the machine's marking quality.".into(),
            Row::NoCut => "Keep the line on the part and on screen, but never cut or mark it.".into(),
            Row::BendLine => "A no-cut line showing where the part is bent later; paint it Mark to etch it.".into(),
            Row::LeaveOut => "Take the line off the part: not drawn on the sheet, not cut, not marked.".into(),
        };
        let title = match self {
            Row::Cut(None) => "Cut".to_string(),
            Row::Cut(Some(q)) => format!("Cut at {}", names.label(q)),
            r => r.label(names),
        };
        (title, s)
    }
}

/// A row's swatch: a filled square in its colour; Cut at the size bands'
/// quality, which has none of its own, as the qualities in a row of stripes;
/// Leave out hollow and dashed.
fn paint_swatch(p: &egui::Painter, r: egui::Rect, row: Row, names: &CutQualities) {
    let round = egui::CornerRadius::same(3);
    match (row, row.swatch(names)) {
        (Row::LeaveOut, Some(c)) => {
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            p.extend(egui::Shape::dashed_line(&pts, egui::Stroke::new(1.5, c), 3.0, 2.0));
        }
        (_, Some(c)) => {
            p.rect_filled(r, round, c);
        }
        (_, None) => {
            let qs: Vec<CutQuality> = (1..=3).filter_map(CutQuality::new).collect();
            let w = r.width() / qs.len() as f32;
            for (k, &q) in qs.iter().enumerate() {
                let s = egui::Rect::from_min_size(egui::Pos2::new(r.left() + w * k as f32, r.top()), egui::Vec2::new(w, r.height()));
                p.rect_filled(s, egui::CornerRadius::ZERO, crate::techui::quality_colour(names, q));
            }
            p.rect_stroke(r, round, egui::Stroke::new(1.0, th().border), egui::StrokeKind::Middle);
        }
    }
}

/// A row's hover card: bold title, its icon (a line drawn the way the row's
/// contours are drawn) and one plain sentence.
fn row_card(ui: &mut egui::Ui, row: Row, names: &CutQualities) {
    let (title, sentence) = row.card(names);
    ui.set_max_width(260.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::Vec2::new(30.0, 22.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, egui::CornerRadius::same(4), crate::drawing::rgb(crate::drawing::C_BACKGROUND));
        let a = r.left_center() + egui::Vec2::new(5.0, 4.0);
        let b = r.right_center() + egui::Vec2::new(-5.0, -4.0);
        match row.swatch(names) {
            Some(c) => contour_line(p, &[a, b], look_of(row), c, 2.0, true),
            None => paint_swatch(p, r.shrink(5.0), row, names),
        }
        ui.label(egui::RichText::new(title).strong().size(13.5).color(th().text_strong));
    });
    ui.label(egui::RichText::new(sentence).size(12.5));
}

/// How a contour painted with `row` shows (CS7); Cut at the size bands'
/// quality as Cut at the last fallback, for its icon only.
fn look_of(row: Row) -> Look {
    match row {
        Row::Cut(q) => Look::Cut(q.unwrap_or(CutQuality::FALLBACK)),
        Row::Mark => Look::Mark,
        Row::NoCut => Look::NoCut,
        Row::BendLine => Look::BendLine,
        Row::LeaveOut => Look::LeaveOut,
    }
}

/// One contour line through screen points `pts`, drawn the way its look is
/// (CS7): cut and mark solid, No cut thin, Bend line dashed, Leave out
/// dashed and faint (only drawn when `left_out`). `w` is the cut line's width.
fn contour_line(p: &egui::Painter, pts: &[egui::Pos2], look: Look, c: Color32, w: f32, left_out: bool) {
    match look {
        Look::Cut(_) | Look::Mark => {
            p.add(egui::Shape::line(pts.to_vec(), egui::Stroke::new(w, c)));
        }
        Look::NoCut => {
            p.add(egui::Shape::line(pts.to_vec(), egui::Stroke::new(w * 0.75, c)));
        }
        Look::BendLine => p.extend(egui::Shape::dashed_line(pts, egui::Stroke::new(w, c), 8.0, 4.0)),
        Look::LeaveOut if left_out => p.extend(egui::Shape::dashed_line(pts, egui::Stroke::new(w * 0.75, c), 4.0, 4.0)),
        Look::LeaveOut => {}
    }
}

/// A part's contours in their colours through `map` (CS7): every cut
/// contour in its cut quality's colour, the others in their state's; Leave
/// out contours only with `left_out` (the panel is open, so they can be
/// painted back). Drawn on the part's drawing screen and on the sheet.
pub fn paint_contour_colours(painter: &egui::Painter, part: &crate::nest::Part, mt: &crate::ct1::MachineTech, map: &dyn Fn(P) -> egui::Pos2, w: f32, left_out: bool) {
    for (c, look) in contours::list(part).iter().zip(contours::looks(part, mt)) {
        let pts: Vec<egui::Pos2> = c.path().into_iter().map(map).collect();
        contour_line(painter, &pts, look, look_colour(look, &mt.qualities), w, left_out);
    }
}

/// A refusal's first sentence: what is wrong, without the agents' advice.
pub fn first_sentence(e: &str) -> String {
    match e.find(". ") {
        Some(i) => e[..=i].to_string(),
        None => e.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::nest::{Part, Shape};

    /// The made-up bend plate of `examples/drawings`: contour 1 a 44 mm
    /// hole, 2–5 four equal 12 mm holes, 6 the outline, 7 a bend line from
    /// edge to edge, 8 a stray open line (No cut).
    pub fn bend_plate() -> Part {
        let (g, _) = crate::dxf::parse(include_str!("../../examples/drawings/DEMO-BEND-PLATE.dxf")).unwrap();
        Part { name: "DEMO-BEND-PLATE".into(), shape: Shape::Custom, w: g.w, h: g.h, qty: 2, rotate: true, color: 0, tech: None, texts: vec![], group: String::new(), erp: vec![], custom: Some(std::sync::Arc::new(g)), rots: Default::default() }
    }

    /// The window with only the bend plate in its job, not nested.
    pub fn app_with_plate() -> App {
        let mut a = App::new(None);
        a.parts = vec![bend_plate()];
        a.result = Default::default();
        a.manual_cuts.clear();
        a.refresh();
        a.undo.clear();
        a
    }

    fn states(a: &App) -> Vec<&'static str> {
        contours::list(&a.parts[0]).iter().map(|c| c.state.key()).collect()
    }

    fn hand_qualities(a: &App) -> Vec<Option<usize>> {
        contours::list(&a.parts[0]).iter().map(|c| c.quality.map(|q| q.number())).collect()
    }

    #[test]
    fn each_paint_changes_contours_at_once_and_ctrl_z_takes_it_back() {
        let mut a = app_with_plate();
        let start = ["cut", "cut", "cut", "cut", "cut", "cut", "bend-line", "no-cut"];
        assert_eq!(states(&a), start);
        let slow = CutQuality::new(1).unwrap();

        // The bend line marked, the big hole left out, every equal small hole Slow.
        assert!(a.paint_contours(0, &[6], Row::Mark, false));
        assert_eq!(states(&a)[6], "mark");
        assert!(a.paint_contours(0, &[0], Row::LeaveOut, false));
        assert_eq!(states(&a)[0], "leave-out");
        assert!(a.paint_contours(0, &[2], Row::Cut(Some(slow)), true));
        assert_eq!(hand_qualities(&a), [None, Some(1), Some(1), Some(1), Some(1), None, None, None]);

        // Ctrl+Z undoes each paint, one at a time.
        a.undo();
        assert_eq!(hand_qualities(&a), [None; 8]);
        assert_eq!(states(&a)[0], "leave-out");
        a.undo();
        assert_eq!(states(&a)[0], "cut");
        a.undo();
        assert_eq!(states(&a), start);
    }

    /// The contour of the plate whose box is centred on `c` (±0.5 mm).
    fn at(list: &[crate::dxf::Contour], c: [f64; 2]) -> usize {
        list.iter()
            .position(|k| {
                let b = crate::nest::bbox_of(&k.pts);
                ((b[0] + b[2]) / 2.0 - c[0]).abs() < 0.5 && ((b[1] + b[3]) / 2.0 - c[1]).abs() < 0.5
            })
            .unwrap_or_else(|| panic!("no contour centred on {c:?}"))
    }

    #[test]
    fn a_click_picks_the_contour_under_the_pointer() {
        let list = contours::list(&bend_plate());
        let half = 1.0;
        // On the top of the small hole at (250, 35), on the bend line, and on
        // the outline's bottom edge.
        assert_eq!(contour_at(&list, [250.0, 41.4], half), Some(at(&list, [250.0, 35.0])));
        assert_eq!(contour_at(&list, [210.6, 80.0], half), Some(6));
        assert_eq!(contour_at(&list, [100.0, 0.5], half), Some(5));
        // Plain material between them, and the middle of a hole: nothing.
        assert_eq!(contour_at(&list, [150.0, 120.0], half), None);
        assert_eq!(contour_at(&list, [70.0, 80.0], half), None);
    }

    #[test]
    fn a_box_picks_the_contours_wholly_inside_it_or_dragged_left_those_it_touches() {
        let list = contours::list(&bend_plate());
        // Round the four small holes, dragged to the right: those four, not
        // the outline it crosses.
        let mut four: Vec<usize> = [[250.0, 35.0], [290.0, 35.0], [250.0, 125.0], [290.0, 125.0]].iter().map(|&c| at(&list, c)).collect();
        four.sort_unstable();
        assert_eq!(contours_in_window(&list, &Window::dragged([235.0, 15.0], [305.0, 145.0])), four);
        assert_eq!(contours_in_window(&list, &Window::dragged([235.0, 145.0], [305.0, 15.0])), four, "down and to the right");
        // A box reaching past the plate's right edge (X 320): dragged to the
        // right still those four; dragged to the left the outline it crosses too.
        assert_eq!(contours_in_window(&list, &Window::dragged([235.0, 15.0], [330.0, 145.0])), four);
        let mut touched = [four.clone(), vec![5]].concat();
        touched.sort_unstable();
        assert_eq!(contours_in_window(&list, &Window::dragged([330.0, 145.0], [235.0, 15.0])), touched);
        // A crossing box over open material takes nothing, and says so.
        let open = Window::dragged([160.0, 130.0], [140.0, 110.0]);
        assert!(contours_in_window(&list, &open).is_empty());
        assert_eq!(none_in_window(open.take, "contour"), "The box touches no contour.");
        // Round the whole part: every contour, the bend line and stray line too.
        assert_eq!(contours_in_window(&list, &Window::dragged([-5.0, -5.0], [330.0, 170.0])), (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn a_sweep_picks_every_contour_the_pointer_passes_over() {
        let list = contours::list(&bend_plate());
        let half = 1.0;
        // Across the bend line and on over the two lower small holes.
        let (a, b) = ([205.0, 35.0], [300.0, 35.0]);
        let mut want = vec![6, at(&list, [250.0, 35.0]), at(&list, [290.0, 35.0])];
        want.sort_unstable();
        assert_eq!(contours_on_stroke(&list, a, b, half), want);
        // A move inside a hole touches nothing.
        assert_eq!(contours_on_stroke(&list, [65.0, 80.0], [75.0, 82.0], half), Vec::<usize>::new());
    }

    #[test]
    fn a_drag_from_a_contour_sweeps_and_from_open_space_draws_a_box() {
        let list = contours::list(&bend_plate());
        assert_eq!(drag_kind(&list, [210.4, 60.0], 1.0), Drag::Sweep);
        assert_eq!(drag_kind(&list, [150.0, 120.0], 1.0), Drag::Box);
        assert_eq!(drag_kind(&list, [-40.0, 80.0], 1.0), Drag::Box);
    }

    const ROWS: [Row; 6] = [Row::Cut(None), Row::Cut(Some(CutQuality::FALLBACK)), Row::Mark, Row::NoCut, Row::BendLine, Row::LeaveOut];

    #[test]
    fn a_contour_takes_a_row_exactly_when_the_engine_would_paint_it() {
        // From the plate as imported and with a hole marked (a quality
        // can't go on a mark), every contour and every row.
        let mut marked = bend_plate();
        contours::set_state(&mut marked, &[1], ContourState::Mark, false).unwrap();
        for part in [bend_plate(), marked] {
            for k in 0..8 {
                for row in ROWS {
                    let mut parts = vec![part.clone()];
                    let painted = contours::paint(&mut parts, &[], &mut [], 0, &[k], row.paint(), false);
                    assert_eq!(contours::refusal(&part, k, row.paint()), painted.err(), "contour {} painted {row:?}", k + 1);
                }
            }
        }
    }

    #[test]
    fn a_box_or_sweep_paints_what_can_take_the_row_and_names_the_rest() {
        let mut a = app_with_plate();
        // A box round the whole plate with Mark: the outline stays cut.
        assert!(a.paint_gathered(0, &(0..8).collect::<Vec<_>>(), Row::Mark, false));
        assert_eq!(states(&a), ["mark", "mark", "mark", "mark", "mark", "cut", "mark", "mark"]);
        assert!(a.status.contains("Contour 6 left as it is") && a.status.contains("outline"), "{}", a.status);
        // With Cut over the holes and the open lines: the open lines can't be cut yet.
        assert!(a.paint_gathered(0, &[0, 1, 6, 7], Row::Cut(None), false));
        assert_eq!(states(&a), ["cut", "cut", "mark", "mark", "mark", "cut", "mark", "mark"]);
        assert!(a.status.contains("Contours 7, 8 left as they are") && a.status.contains("open line"), "{}", a.status);
        // Nothing in it can take the row: nothing changes, nothing to undo.
        let steps = a.undo.len();
        assert!(!a.paint_gathered(0, &[5], Row::Mark, false));
        assert_eq!(a.undo.len(), steps);
        assert!(a.status.contains("outline"), "{}", a.status);
        // A click on the outline with Mark says why, too.
        assert!(!a.paint_contours(0, &[5], Row::Mark, false));
        assert!(a.status.contains("outline") && a.status.contains("always cut"), "{}", a.status);
    }

    #[test]
    fn every_contour_is_drawn_in_the_colour_of_the_row_that_painted_it() {
        let mut a = app_with_plate();
        let mt = crate::ct1::MachineTech::default();
        let rgb = |c: egui::Color32| [c.r(), c.g(), c.b()];
        let drawn = |a: &App| contours::looks(&a.parts[0], &mt).into_iter().map(|l| rgb(look_colour(l, &mt.qualities))).collect::<Vec<_>>();
        let swatch = |row: Row| rgb(row.swatch(&mt.qualities).expect("a row with one colour"));
        let slow = Row::Cut(Some(CutQuality::new(1).unwrap()));
        a.paint_contours(0, &[6], Row::Mark, false);
        a.paint_contours(0, &[0], Row::LeaveOut, false);
        a.paint_contours(0, &[1], slow, true);
        a.paint_contours(0, &[7], Row::BendLine, false);
        let d = drawn(&a);
        // Slow (1) in the colour the original ships for quality 1 (CQ5), on
        // the four equal holes; the outline, which nothing set, in quality 3's
        // (CQ1's last fallback); the mark green, the bend line purple.
        assert_eq!(d[1..5], [[255, 128, 64]; 4]);
        assert_eq!(d[5], [255, 0, 255]);
        assert_eq!((d[6], d[7]), ([0, 255, 0], [147, 51, 234]));
        // Each is its row's swatch.
        assert_eq!(d[1], swatch(slow));
        assert_eq!(d[6], swatch(Row::Mark));
        assert_eq!(d[7], swatch(Row::BendLine));
        assert_eq!(d[0], swatch(Row::LeaveOut));
        a.paint_contours(0, &[7], Row::NoCut, false);
        assert_eq!(drawn(&a)[7], swatch(Row::NoCut));
        // Cut at the size bands' quality has no one swatch: each contour
        // shows the quality the bands give it.
        assert_eq!(Row::Cut(None).swatch(&mt.qualities), None);
    }

    #[test]
    fn a_sweep_paints_as_it_passes_and_ctrl_z_takes_the_whole_sweep_back() {
        let mut a = app_with_plate();
        let start = states(&a);
        // From the bend line, on along y = 35 over the two lower small holes
        // and off the right edge (the outline can't be a mark).
        a.sweep_start(0, [210.0, 35.0], Row::Mark, false, 1.0);
        assert_eq!(states(&a)[6], "mark", "painted as soon as the pointer is on it");
        a.sweep_to([260.0, 35.0], 1.0);
        assert_eq!(states(&a)[at(&contours::list(&a.parts[0]), [250.0, 35.0])], "mark");
        a.sweep_to([330.0, 35.0], 1.0);
        a.sweep_end();
        let list = contours::list(&a.parts[0]);
        let marked: Vec<usize> = (0..8).filter(|&k| list[k].state == ContourState::Mark).collect();
        let mut want = vec![6, at(&list, [250.0, 35.0]), at(&list, [290.0, 35.0])];
        want.sort_unstable();
        assert_eq!(marked, want);
        assert_eq!(states(&a)[5], "cut");
        assert!(a.status.contains("Contour 6 left as it is"), "{}", a.status);
        // One Ctrl+Z takes the whole sweep back.
        assert_eq!(a.undo.len(), 1);
        a.undo();
        assert_eq!(states(&a), start);
    }

    #[test]
    fn the_rows_show_the_machines_quality_names_or_quality_n() {
        let named = crate::qualities::CutQualities::from_codes(&[("A922".to_string(), "\"Slow\"".to_string())].into_iter().collect());
        let slow = Row::Cut(CutQuality::new(1));
        assert_eq!(slow.label(&named), "Slow (1)");
        assert_eq!(slow.label(&CutQualities::default()), "Quality 1");
        assert_eq!(Row::Cut(CutQuality::new(2)).label(&named), "Quality 2");
        // The Mark card says etch, the shop's word.
        assert!(Row::Mark.card(&named).1.to_lowercase().contains("etch"));
    }
}

