//! A part's cut order on its drawing screen (MECHANICS CO11–CO13): the
//! Machining → Contours commands that set it, and its numbers and rapids on
//! the drawing. Every command is the engine's `cutorder::apply`, the same
//! function as the agents' `part-cut-order`, and the numbers come from
//! `cutorder::order`; this module turns clicks, boxes and sweeps (the cut
//! technology panel's, CS6) into contour numbers, and draws.

use crate::cnc::SheetPlan;
use crate::cutpanel::{self, Gesture};
use crate::dxf::{self, Contour, ContourState};
use crate::engine::contours;
use crate::engine::cutorder::{self, Command};
use crate::nest::{CutQuality, Part, P};
use crate::qualities::CutQualities;
use crate::theme::th;
use crate::ui_parts::Icon;
use crate::App;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, Vec2};

/// Numbers and rapids of the contours ordered by hand: solid, in this
/// colour (CO13; our own colour).
pub const HAND: Color32 = Color32::from_rgb(34, 211, 238);
/// Numbers and rapids of the automatic order after them: grey (CO13).
pub const AUTO: Color32 = Color32::from_rgb(140, 144, 156);
/// The status line while picking (CO11).
pub const PICK_HINT: &str = "Pick contours in cut order — Esc ends";

/// The contours as picking sees them (CO11): a Leave out contour isn't
/// drawn, so it can't be picked (its points are dropped; numbers stay).
fn drawn(list: &[Contour]) -> Vec<Contour> {
    list.iter().map(|c| if c.state == ContourState::LeaveOut { Contour { pts: vec![], ..c.clone() } } else { c.clone() }).collect()
}

/// Whether a contour has a place in the cut order: it is cut or marked.
fn in_order(c: &Contour) -> bool {
    matches!(c.state, ContourState::Cut | ContourState::Mark)
}

/// The contour a click picks (CO11): the nearest drawn contour whose line
/// passes within `half` mm of the point.
pub fn pick_at(list: &[Contour], p: P, half: f64) -> Option<usize> {
    cutpanel::contour_at(&drawn(list), p, half)
}

/// What a box or a sweep gathered for the cut order (CO11, CO12): the
/// contours to add, in order, and those it met that are already in the
/// order set by hand (skipped).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gathered {
    pub add: Vec<usize>,
    pub already: Vec<usize>,
}

impl Gathered {
    /// The command: the contours to add, then those already in the order,
    /// which the engine skips and names (CO5).
    fn command(&self) -> Command {
        Command::Contours(self.add.iter().chain(&self.already).copied().collect())
    }

    fn is_empty(&self) -> bool {
        self.add.is_empty() && self.already.is_empty()
    }
}

/// The contours a box from `a` to `b` (part mm) adds (CO12): the cut and
/// marked ones wholly inside it, in the order `shown` (the part's cut order
/// as the part screen numbers it, `cutorder::order`: the automatic part with
/// its holes in the machine's hole order, CO14); those already ordered by
/// hand (`ordered`) are skipped, named in number order.
pub fn box_picks(list: &[Contour], shown: &[usize], a: P, b: P, ordered: &[usize]) -> Gathered {
    let inside = cutpanel::contours_in_box(&drawn(list), a, b);
    let (mut already, add): (Vec<usize>, Vec<usize>) = shown.iter().copied().filter(|k| inside.contains(k)).partition(|k| ordered.contains(k));
    already.sort_unstable();
    Gathered { add, already }
}

/// A sweep in progress (CO11): what it gathered, and where the pointer was.
#[derive(Clone, Debug)]
pub struct Sweep {
    last: P,
    pub got: Gathered,
}

impl Sweep {
    /// A sweep from `at` (part mm): the contour under the pointer is taken
    /// at once. `ordered` are the contours already ordered by hand.
    pub fn start(list: &[Contour], at: P, half: f64, ordered: &[usize]) -> Sweep {
        let mut s = Sweep { last: at, got: Gathered::default() };
        s.to(list, at, half, ordered);
        s
    }

    /// The pointer moved on to `at` with the button held: the cut and
    /// marked contours it passed since, in the order it reached them. Each
    /// is taken once; those already ordered by hand are skipped, and lines
    /// with no place in the order (No cut, Bend line) are passed over.
    pub fn to(&mut self, list: &[Contour], at: P, half: f64, ordered: &[usize]) {
        for k in cutpanel::contours_along_stroke(&drawn(list), self.last, at, half) {
            if !in_order(&list[k]) || self.got.add.contains(&k) || self.got.already.contains(&k) {
                continue;
            }
            if ordered.contains(&k) {
                self.got.already.push(k);
            } else {
                self.got.add.push(k);
            }
        }
        self.last = at;
    }

    /// What it adds when let go (CO11): an outline passed before another
    /// contour is left out, with why.
    fn held(&self, list: &[Contour]) -> (Gathered, Option<String>) {
        let (add, left) = cutorder::hold_outline(list, &self.got.add);
        (Gathered { add, already: self.got.already.clone() }, left.map(|l| l.1))
    }
}

/// Why part `part` can't take a cut order set by hand (CO3), in the
/// screen's words: a built-in shape follows the automatic order.
pub fn unavailable(part: &Part) -> Option<String> {
    cutorder::check(part).err().map(|e| format!("{} Save it as a drawing and import that to set its cut order.", cutpanel::first_sentence(&e)))
}

/// Picking contours in cut order (Machining → Contours → Manual Contours, CO11).
#[derive(Default)]
pub struct OrderPick {
    /// Picking is on, for this part (the job's number for it).
    pub on: Option<usize>,
    /// A sweep in progress, of this part.
    sweep: Option<(usize, Sweep)>,
    /// A box being drawn from this point (screen).
    box_from: Option<Pos2>,
    /// Screenshots only: the Contours menu shown open (--order-menu), with
    /// its quality list (--order-quality) or an item's hover card
    /// (--order-card=<item>).
    pub demo_menu: bool,
    pub demo_quality: bool,
    pub demo_card: Option<String>,
}

/// One contour of the cut order as the drawing shows it (CO13).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// Its contour number (from 0).
    pub contour: usize,
    /// Set by hand: its number and the rapid to it solid, else grey.
    pub by_hand: bool,
    /// Where it starts: a cut contour at its pierce, a mark at its first point.
    pub start: P,
    /// Where the head leaves it: a cut contour at the end of its lead-out,
    /// a mark at its last point.
    pub end: P,
}

/// The part's cut order as the drawing shows it (CO13): every cut or marked
/// contour in the order `cutorder::order` gives, where it starts and ends.
/// `plan` is the part's machining on its own (`cnc::plan_part`), which gives
/// each cut contour's pierce and lead-out.
pub fn stops(part: &Part, plan: &SheetPlan) -> Vec<Stop> {
    let list = contours::list(part);
    let cut = dxf::cut_order(&list);
    let ends = |c: &Contour| -> (P, P) {
        let first = c.pts.first().copied().unwrap_or([0.0, 0.0]);
        (first, if c.closed { first } else { c.pts.last().copied().unwrap_or(first) })
    };
    cutorder::order(part)
        .into_iter()
        .map(|(k, by_hand)| {
            let planned = plan.contours.iter().find(|c| cut.get(c.contour) == Some(&k));
            let (start, end) = match planned {
                Some(c) if list[k].state == ContourState::Cut => {
                    let start = c.punch.as_ref().and_then(|pu| pu.hits.first().copied()).unwrap_or(c.lead_in);
                    let end = c.lead_out.or(c.lead_out_path.last().copied()).or(c.points.first().copied()).unwrap_or(start);
                    (start, end)
                }
                _ => ends(&list[k]),
            };
            Stop { contour: k, by_hand, start, end }
        })
        .collect()
}

/// The cut order on the drawing (CO13): each contour's number where it
/// starts, and dashed rapids from where one ends to where the next starts.
/// Set by hand solid, the automatic order after them grey.
pub fn paint_order(painter: &egui::Painter, stops: &[Stop], to_s: &dyn Fn(P) -> Pos2) {
    for w in stops.windows(2) {
        let col = if w[1].by_hand { HAND } else { AUTO };
        painter.extend(egui::Shape::dashed_line(&[to_s(w[0].end), to_s(w[1].start)], Stroke::new(1.4, col), 6.0, 4.0));
    }
    for (n, s) in stops.iter().enumerate() {
        number_tag(painter, to_s(s.start) + Vec2::new(13.0, -13.0), n + 1, s.by_hand);
    }
}

/// A cut-order number centred on `at`: solid when set by hand, grey after.
fn number_tag(painter: &egui::Painter, at: Pos2, n: usize, by_hand: bool) {
    let text = n.to_string();
    let r = Rect::from_center_size(at, Vec2::new(10.0 + 6.5 * text.len() as f32, 16.0).max(Vec2::new(18.0, 16.0)));
    let font = FontId::proportional(10.5);
    if by_hand {
        painter.rect_filled(r, CornerRadius::same(4), HAND);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, text, font, Color32::from_rgb(6, 24, 32));
    } else {
        painter.rect_filled(r, CornerRadius::same(4), th().overlay);
        painter.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, AUTO), egui::StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, text, font, AUTO);
    }
}

/// The legend's cut-order entries, from `at` (the left of the first): a
/// dashed rapid, and a number set by hand. Gives where the next would go.
pub fn legend(painter: &egui::Painter, at: Pos2, dim: Color32) -> f32 {
    let font = FontId::proportional(11.5);
    painter.extend(egui::Shape::dashed_line(&[at, at + Vec2::new(18.0, 0.0)], Stroke::new(1.4, AUTO), 4.0, 3.0));
    painter.text(at + Vec2::new(24.0, 0.0), egui::Align2::LEFT_CENTER, "rapid", font.clone(), dim);
    let x = at.x + 104.0;
    number_tag(painter, Pos2::new(x + 9.0, at.y), 1, true);
    painter.text(Pos2::new(x + 24.0, at.y), egui::Align2::LEFT_CENTER, "order set by hand", font, dim);
    x + 150.0
}

/// An item of the Machining → Contours menu (CO11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContoursItem {
    Manual,
    Automatic,
    Parts,
    AllMarks,
    AllCut,
    Quality(CutQuality),
}

impl ContoursItem {
    /// The item's name, its picture and its hover card's one plain sentence.
    pub fn card(self) -> (&'static str, Icon, &'static str) {
        match self {
            ContoursItem::Manual => ("Manual Contours", Icon::PickOrder, "Click, sweep or box contours to add them to the cut order in that order; picking sets only the order, not where each cut starts."),
            ContoursItem::Automatic => ("Automatic Contours", Icon::Toolpath, "Cut every contour not ordered by hand in the automatic order: marks first, then the holes, then the outline."),
            ContoursItem::Parts => ("Parts", Icon::Contours, "Machine every contour of the part: those ordered by hand first, the rest in the automatic order."),
            ContoursItem::AllMarks => ("All Marks", Icon::OrderMarks, "Add every etching not yet in the cut order to its end, in the automatic order."),
            ContoursItem::AllCut => ("All Cut", Icon::OrderCut, "Add every cut contour not yet in the cut order to its end, in the automatic order, the outline last."),
            ContoursItem::Quality(_) => ("All Contours of one Quality", Icon::OrderQuality, "Add every contour cut at the quality you choose, and not yet in the cut order, to its end."),
        }
    }

    /// Whether it sets a cut order by hand, which a built-in shape can't take (CO3).
    pub fn sets_order(self) -> bool {
        matches!(self, ContoursItem::Manual | ContoursItem::AllMarks | ContoursItem::Quality(_))
    }

    /// The item for the screenshot flag `--order-card=<name>`.
    pub fn named(name: &str) -> Option<ContoursItem> {
        Some(match name {
            "manual" => ContoursItem::Manual,
            "automatic" => ContoursItem::Automatic,
            "parts" => ContoursItem::Parts,
            "all-marks" => ContoursItem::AllMarks,
            "all-cut" => ContoursItem::AllCut,
            "quality" => ContoursItem::Quality(CutQuality::FALLBACK),
            _ => return None,
        })
    }
}

/// What the Contours menu shows for the shown part: why it can't be ordered
/// by hand (a built-in shape), and the machine's qualities, each with its
/// name and colour and how many of the part's contours are cut at it.
pub struct ContoursMenu {
    pub why_not: Option<String>,
    pub names: std::sync::Arc<CutQualities>,
    pub qualities: Vec<(CutQuality, usize)>,
    pub show_unbuilt: bool,
    /// Screenshots only: show the quality list, or this item's hover card.
    pub demo_quality: bool,
    pub demo_card: Option<ContoursItem>,
}

/// The machine's qualities as the menu lists them (CQ4, CQ5): each with its
/// name and colour, and how many of the part's cut contours are at it.
pub fn quality_list(ui: &mut egui::Ui, m: &ContoursMenu) -> Option<CutQuality> {
    let mut pick = None;
    ui.set_min_width(220.0);
    ui.label(egui::RichText::new("Add the contours cut at").size(11.5).color(th().text_dim));
    for &(q, n) in &m.qualities {
        let text = crate::techui::quality_text(ui, &m.names, q);
        let count = if n == 1 { "1 contour".to_string() } else { format!("{n} contours") };
        let b = egui::Button::new(text).right_text(egui::RichText::new(count).size(11.5).color(th().text_dim)).frame(false).min_size(Vec2::new(210.0, 22.0));
        if ui.add(b).on_hover_text(format!("Add every contour cut at {} not yet in the cut order", m.names.label(q))).clicked() {
            pick = Some(q);
        }
    }
    pick
}

/// A message from the engine as the status line shows it: first letter capital.
fn capital(m: &str) -> String {
    let mut c = m.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

impl App {
    /// What the Contours menu shows for the shown part.
    pub fn contours_menu(&self) -> ContoursMenu {
        let mt = crate::ct1::for_spec(&self.spec);
        let part = self.shown_part().and_then(|i| self.parts.get(i));
        let looks = part.map_or(vec![], |p| contours::looks(p, &mt));
        let qualities = crate::techui::offered_qualities(&mt.qualities).into_iter().map(|q| (q, looks.iter().filter(|l| **l == contours::Look::Cut(q)).count())).collect();
        ContoursMenu {
            why_not: part.and_then(unavailable),
            names: mt.qualities,
            qualities,
            show_unbuilt: self.dv.show_unbuilt,
            demo_quality: self.order_pick.demo_quality,
            demo_card: self.order_pick.demo_card.as_deref().and_then(ContoursItem::named),
        }
    }

    /// A Machining → Contours item (CO11): the part's machining shown, and
    /// Manual Contours starts picking, All Marks, All Cut and one quality
    /// add to the cut order, Automatic Contours and Parts machine the rest
    /// in the automatic order. All Cut on a built-in shape machines it.
    pub fn contours_item(&mut self, item: ContoursItem) {
        let Some(i) = self.shown_part() else { return };
        self.dv.machining = true;
        let built_in = unavailable(&self.parts[i]).is_some();
        match item {
            ContoursItem::Manual => self.order_pick_start(),
            ContoursItem::AllMarks => {
                self.cut_order(i, &Command::AllMarks);
            }
            ContoursItem::AllCut if !built_in => {
                self.cut_order(i, &Command::AllCut);
            }
            ContoursItem::Quality(q) => {
                self.cut_order(i, &Command::Quality(q));
            }
            ContoursItem::Automatic | ContoursItem::Parts | ContoursItem::AllCut => {
                self.status = format!("{}: the contours not ordered by hand are cut in the automatic order (marks, then holes, then the outline).", item.card().0);
            }
        }
    }

    /// Run a cut-order command on part `part` (CO3–CO6, CO11): the engine's
    /// `cutorder::apply`, as the agents' `part-cut-order`. One undo step
    /// (Ctrl+Z) when it changed the order; the status line says what it
    /// did, or why not. True when the order changed.
    pub fn cut_order(&mut self, part: usize, cmd: &Command) -> bool {
        if part >= self.parts.len() {
            return false;
        }
        let before = self.snapshot();
        let mt = crate::ct1::for_spec(&self.spec);
        match cutorder::apply(&mut self.parts[part], &mt, cmd) {
            Ok(done) => {
                if done.changed {
                    self.push_undo(before);
                    self.refresh();
                }
                self.status = capital(&done.message);
                done.changed
            }
            Err(e) => {
                self.status = cutpanel::first_sentence(&e);
                false
            }
        }
    }

    /// Machining → Machine part (Automatic): the part's machining shown, and
    /// its cut order set by hand cleared (CO6), as one undo step.
    pub fn machine_part(&mut self) {
        self.dv.machining = true;
        let Some(i) = self.shown_part() else { return };
        if !cutorder::hand(&self.parts[i]).is_empty() && self.cut_order(i, &Command::Clear) {
            self.status = "Part machined automatically: the cut order set by hand is cleared, every contour is cut in the automatic order.".into();
        } else {
            self.status = "Part machined automatically (marks, then holes, then the outside).".into();
        }
    }

    /// Machining → Contours → Manual Contours: picking starts on the shown
    /// part (CO11). A built-in shape can't be ordered by hand; the status
    /// line says why.
    pub fn order_pick_start(&mut self) {
        let Some(i) = self.shown_part() else { return };
        if let Some(why) = unavailable(&self.parts[i]) {
            self.status = why;
            return;
        }
        self.order_pick.on = Some(i);
        self.order_pick.sweep = None;
        self.order_pick.box_from = None;
        self.dv.machining = true;
        // Picking replaces the other pointer tools.
        self.cut_panel.row = None;
        self.dv.measure_on = false;
        self.dv.zoom_window = false;
        self.placing_text = false;
        self.status = PICK_HINT.into();
    }

    /// Esc: picking ends (CO11). A sweep in progress ends first and goes in,
    /// as one command.
    pub fn order_pick_end(&mut self) {
        let swept = self.order_pick.sweep.is_some();
        self.order_sweep_end();
        self.order_pick.on = None;
        self.order_pick.box_from = None;
        self.status = if swept { format!("{} Picking ended.", self.status) } else { "Picking ended.".into() };
    }

    /// A click at `at` (part mm) on part `part` while picking (CO11): the
    /// contour under the pointer goes to the end of the order. On open
    /// space nothing happens. True when the order changed.
    pub fn order_click(&mut self, part: usize, at: P, half: f64) -> bool {
        let Some(k) = self.parts.get(part).and_then(|p| pick_at(&contours::list(p), at, half)) else { return false };
        self.cut_order(part, &Command::Contours(vec![k]))
    }

    /// A box from `a` to `b` (part mm) on part `part` while picking (CO12):
    /// the contours wholly inside it go in, in the automatic order among
    /// themselves. True when the order changed.
    pub fn order_box(&mut self, part: usize, a: P, b: P) -> bool {
        let Some(p) = self.parts.get(part) else { return false };
        let shown: Vec<usize> = cutorder::order(p).into_iter().map(|o| o.0).collect();
        let got = box_picks(&contours::list(p), &shown, a, b, &cutorder::hand(p));
        if got.is_empty() {
            self.status = "No cut or marked contour lies wholly inside the box.".into();
            return false;
        }
        self.cut_order(part, &got.command())
    }

    /// A sweep of part `part` starts at `at` (part mm) (CO11).
    pub fn order_sweep_start(&mut self, part: usize, at: P, half: f64) {
        let Some(p) = self.parts.get(part) else { return };
        self.order_pick.sweep = Some((part, Sweep::start(&contours::list(p), at, half, &cutorder::hand(p))));
    }

    /// The pointer moved on to `at` (part mm) with the button held.
    pub fn order_sweep_to(&mut self, at: P, half: f64) {
        let Some((part, mut sw)) = self.order_pick.sweep.take() else { return };
        if let Some(p) = self.parts.get(part) {
            sw.to(&contours::list(p), at, half, &cutorder::hand(p));
            self.order_pick.sweep = Some((part, sw));
        }
    }

    /// The button was let go: the contours swept go in as one command, one
    /// undo step, in the order passed. An outline passed before another
    /// contour is left out, and the status line says why (CO11). True when
    /// the order changed.
    pub fn order_sweep_end(&mut self) -> bool {
        let Some((part, sw)) = self.order_pick.sweep.take() else { return false };
        let Some(p) = self.parts.get(part) else { return false };
        let (got, left) = sw.held(&contours::list(p));
        if got.is_empty() {
            return false;
        }
        let changed = self.cut_order(part, &got.command());
        if let Some(why) = left {
            self.status = format!("{} {why}", self.status);
        }
        changed
    }

    /// The shown part as it would be were the sweep in progress let go: what
    /// the drawing numbers while sweeping (CO13).
    pub fn order_preview(&self, idx: usize) -> Option<Part> {
        let (part, sw) = self.order_pick.sweep.as_ref().filter(|s| s.0 == idx)?;
        let mut p = self.parts.get(*part)?.clone();
        let (got, _) = sw.held(&contours::list(&p));
        cutorder::apply(&mut p, &crate::ct1::for_spec(&self.spec), &got.command()).ok()?;
        Some(p)
    }

    /// While picking on part `idx`'s drawing (CO11): a click adds the
    /// contour under the pointer, a sweep each contour as the pointer passes
    /// it, a box the contours inside it in the automatic order; Esc ends.
    /// `half` is the pick aperture, mm.
    #[allow(clippy::too_many_arguments)]
    pub fn order_pick_canvas(&mut self, ui: &egui::Ui, resp: &egui::Response, painter: &egui::Painter, rect: Rect, idx: usize, to_s: &dyn Fn(P) -> Pos2, to_w: &dyn Fn(Pos2) -> P, half: f64) {
        let Some(on) = self.order_pick.on else { return };
        // Picking belongs to the part it started on, and ends when another
        // pointer tool starts (painting, measuring, zoom window, a text).
        let other = self.cut_panel.painting() || self.dv.measure_on || self.dv.zoom_window || self.placing_text;
        if on != idx || self.parts.get(idx).is_none() || other {
            self.order_pick.on = None;
            self.order_pick.sweep = None;
            self.order_pick.box_from = None;
            return;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.order_pick_end();
            return;
        }
        let list = drawn(&contours::list(&self.parts[idx]));
        // The contours swept so far, and the contour under the pointer.
        if let Some((_, sw)) = &self.order_pick.sweep {
            for &k in &sw.got.add {
                painter.add(egui::Shape::line(list[k].path().into_iter().map(to_s).collect(), Stroke::new(4.0, crate::ui_parts::with_alpha(HAND, 120))));
            }
        }
        let at = resp.hover_pos().map(to_w);
        if let Some(k) = at.and_then(|w| cutpanel::contour_at(&list, w, half)).filter(|_| self.order_pick.box_from.is_none()) {
            painter.add(egui::Shape::line(list[k].path().into_iter().map(to_s).collect(), Stroke::new(4.0, crate::ui_parts::with_alpha(HAND, 150))));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let mut box_from = self.order_pick.box_from.take();
        let gs = cutpanel::gestures(ui, resp, painter, &list, to_w, half, self.order_pick.sweep.is_some(), &mut box_from);
        self.order_pick.box_from = box_from;
        for g in gs {
            match g {
                Gesture::Click(k) => {
                    self.cut_order(idx, &Command::Contours(vec![k]));
                }
                Gesture::SweepStart(from) => self.order_sweep_start(idx, from, half),
                Gesture::SweepTo(w) => self.order_sweep_to(w, half),
                Gesture::SweepEnd => {
                    self.order_sweep_end();
                }
                Gesture::Box(a, b) => {
                    self.order_box(idx, a, b);
                }
            }
        }
        // What picking does, at the top right of the drawing while it is on.
        let g = painter.layout_no_wrap("Picking the cut order: click, sweep or box contours  ·  Ctrl+Z undoes  ·  Esc ends".into(), FontId::proportional(12.5), Color32::WHITE);
        let size = g.size() + Vec2::new(22.0, 10.0);
        let r = Rect::from_min_size(Pos2::new(rect.right() - 64.0 - size.x, rect.top() + 12.0), size);
        painter.rect_filled(r, CornerRadius::same(8), th().overlay);
        painter.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, HAND), egui::StrokeKind::Inside);
        painter.galley(r.min + Vec2::new(11.0, 5.0), g, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cutpanel::tests::{app_with_plate, bend_plate};

    /// The made-up bend plate with its bend line (contour 7) etched: holes
    /// 1 (Ø44) and 2–5 (Ø12), the outline 6, the mark 7, a No cut line 8.
    fn marked_plate() -> Part {
        let mut p = bend_plate();
        contours::set_state(&mut p, &[6], ContourState::Mark, false).unwrap();
        p
    }

    /// The contour of the plate centred on `c` (±0.5 mm).
    fn at(list: &[Contour], c: P) -> usize {
        list.iter()
            .position(|k| {
                let b = crate::nest::bbox_of(&k.pts);
                ((b[0] + b[2]) / 2.0 - c[0]).abs() < 0.5 && ((b[1] + b[3]) / 2.0 - c[1]).abs() < 0.5
            })
            .unwrap_or_else(|| panic!("no contour centred on {c:?}"))
    }

    /// The plate's holes: the big one and the four small ones, by place.
    struct Holes {
        big: usize,
        low_left: usize,
        low_right: usize,
        top_left: usize,
        top_right: usize,
    }

    fn holes(list: &[Contour]) -> Holes {
        Holes { big: at(list, [70.0, 80.0]), low_left: at(list, [250.0, 35.0]), low_right: at(list, [290.0, 35.0]), top_left: at(list, [250.0, 125.0]), top_right: at(list, [290.0, 125.0]) }
    }

    const OUTLINE: usize = 5;
    const MARK: usize = 6;
    const NO_CUT: usize = 7;

    #[test]
    fn a_sweep_takes_contours_in_the_order_the_pointer_passes_them() {
        let list = contours::list(&marked_plate());
        let h = holes(&list);
        // Right to left along y = 35: the right hole, the left one, then the
        // etched bend line.
        let mut sw = Sweep::start(&list, [296.0, 35.0], 1.0, &[]);
        sw.to(&list, [205.0, 35.0], 1.0, &[]);
        assert_eq!(sw.got, Gathered { add: vec![h.low_right, h.low_left, MARK], already: vec![] });
        // Left to right, the other way round.
        let mut sw = Sweep::start(&list, [210.0, 35.0], 1.0, &[]);
        sw.to(&list, [300.0, 35.0], 1.0, &[]);
        assert_eq!(sw.got.add, [MARK, h.low_left, h.low_right]);
    }

    #[test]
    fn a_sweep_takes_a_contour_once_and_skips_those_already_ordered() {
        let list = contours::list(&marked_plate());
        let h = holes(&list);
        // Out over both lower holes and back again: each once, as first passed.
        let mut sw = Sweep::start(&list, [244.0, 35.0], 1.0, &[]);
        sw.to(&list, [300.0, 35.0], 1.0, &[]);
        sw.to(&list, [240.0, 35.0], 1.0, &[]);
        assert_eq!(sw.got.add, [h.low_left, h.low_right]);
        // With the right one already ordered by hand: it is skipped, and named.
        let mut sw = Sweep::start(&list, [244.0, 35.0], 1.0, &[h.low_right]);
        sw.to(&list, [300.0, 35.0], 1.0, &[h.low_right]);
        assert_eq!(sw.got, Gathered { add: vec![h.low_left], already: vec![h.low_right] });
    }

    #[test]
    fn a_sweep_passes_over_lines_with_no_place_in_the_order() {
        let list = contours::list(&marked_plate());
        let h = holes(&list);
        // Down from the big hole across the No cut line at y = 22.
        let mut sw = Sweep::start(&list, [70.0, 58.0], 1.0, &[]);
        sw.to(&list, [70.0, 10.0], 1.0, &[]);
        assert_eq!(sw.got.add, [h.big]);
        assert!(!sw.got.add.contains(&NO_CUT));
        // A Leave out hole isn't drawn, so the sweep can't take it either.
        let mut left_out = marked_plate();
        contours::set_state(&mut left_out, &[h.low_left], ContourState::LeaveOut, false).unwrap();
        let list = contours::list(&left_out);
        let mut sw = Sweep::start(&list, [210.0, 35.0], 1.0, &[]);
        sw.to(&list, [300.0, 35.0], 1.0, &[]);
        assert_eq!(sw.got.add, [MARK, h.low_right]);
    }

    #[test]
    fn a_box_takes_the_contours_inside_in_the_automatic_order() {
        let list = contours::list(&marked_plate());
        let h = holes(&list);
        // Round the whole plate: the mark first, then the holes in number
        // order, then the outline; never the No cut line.
        let mut holes_in_order = vec![h.big, h.low_left, h.low_right, h.top_left, h.top_right];
        holes_in_order.sort_unstable();
        // With no order set by hand and the holes as drawn (Order mode of
        // the holes None, CO14), the automatic order is by number.
        let auto = cutorder::automatic(&list);
        let all = box_picks(&list, &auto, [-5.0, -5.0], [330.0, 170.0], &[]);
        assert_eq!(all.add, [vec![MARK], holes_in_order.clone(), vec![OUTLINE]].concat());
        // Drawn from the other corner, the same; with two holes already
        // ordered by hand, those are skipped and named.
        let some = box_picks(&list, &auto, [330.0, 170.0], [-5.0, -5.0], &[h.top_right, h.big]);
        let mut rest: Vec<usize> = holes_in_order.iter().copied().filter(|&k| k != h.top_right && k != h.big).collect();
        rest.sort_unstable();
        assert_eq!(some.add, [vec![MARK], rest, vec![OUTLINE]].concat());
        assert_eq!(some.already, [h.big.min(h.top_right), h.big.max(h.top_right)]);
        // Round the four small holes only.
        let mut four = vec![h.low_left, h.low_right, h.top_left, h.top_right];
        four.sort_unstable();
        assert_eq!(box_picks(&list, &auto, [235.0, 15.0], [305.0, 145.0], &[]).add, four);
    }

    #[test]
    fn a_click_takes_the_contour_under_the_pointer_and_nothing_on_open_space() {
        let list = contours::list(&marked_plate());
        let h = holes(&list);
        assert_eq!(pick_at(&list, [250.0, 41.4], 1.0), Some(h.low_left));
        assert_eq!(pick_at(&list, [150.0, 120.0], 1.0), None);
        // A Leave out contour isn't drawn: a click on where it was takes nothing.
        let mut left_out = marked_plate();
        contours::set_state(&mut left_out, &[h.big], ContourState::LeaveOut, false).unwrap();
        assert_eq!(pick_at(&contours::list(&left_out), [70.0, 102.0], 1.0), None);
    }

    /// While it lives, the test's thread works on a machine whose file
    /// holds `codes` (made up); then on the built-in machine again.
    struct OnMachine;

    fn on_machine(codes: &str) -> OnMachine {
        crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text(codes)));
        OnMachine
    }

    impl Drop for OnMachine {
        fn drop(&mut self) {
            crate::ct1::set_thread_active(None);
        }
    }

    /// The window with the marked plate as its only part, opened on its
    /// drawing screen.
    fn app() -> App {
        let mut a = app_with_plate();
        a.parts = vec![marked_plate()];
        a.open_part(0);
        a.refresh();
        a.undo.clear();
        a
    }

    /// The part's cut order as the drawing numbers it: (contour, set by hand).
    fn shown(a: &App) -> Vec<(usize, bool)> {
        let plan = crate::cnc::plan_part(&a.parts, 0, &a.spec);
        stops(&a.parts[0], &plan).iter().map(|s| (s.contour, s.by_hand)).collect()
    }

    /// The contours ordered by hand, in their order.
    fn by_hand(a: &App) -> Vec<usize> {
        shown(a).into_iter().filter(|s| s.1).map(|s| s.0).collect()
    }

    #[test]
    fn a_box_adds_the_holes_in_the_order_the_part_screen_shows_them() {
        // By proximity (the original's default, CO20): the drawing numbers
        // the holes nearest-first from the part's lower left corner (CO14),
        // not by contour number. A box round the whole part makes that
        // order the one set by hand, as shown: nothing moves.
        let _m = on_machine("B565 1\n");
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        let before: Vec<usize> = shown(&a).into_iter().map(|s| s.0).collect();
        // The big hole (70, 80) first; from it the two left-hand small holes
        // are as far, so the one first in the part's own order goes next
        // (CO21), then round the other three.
        let nearest = if h.low_left < h.top_left { vec![h.big, h.low_left, h.low_right, h.top_right, h.top_left] } else { vec![h.big, h.top_left, h.top_right, h.low_right, h.low_left] };
        assert_eq!(before, [vec![MARK], nearest.clone(), vec![OUTLINE]].concat());
        let mut by_number = nearest.clone();
        by_number.sort_unstable();
        assert_ne!(nearest, by_number, "nearest-first differs from the number order here");
        a.order_pick_start();
        assert!(a.order_box(0, [-5.0, -5.0], [330.0, 170.0]));
        assert_eq!(by_hand(&a), before);
    }

    #[test]
    fn each_command_changes_the_order_through_the_engine_and_ctrl_z_takes_back_one() {
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        a.order_pick_start();
        assert_eq!(a.order_pick.on, Some(0));
        assert_eq!(a.status, PICK_HINT);
        // A click, a sweep, a box: each goes to the end of the order.
        assert!(a.order_click(0, [290.0, 41.4], 1.0));
        assert_eq!(by_hand(&a), [h.low_right]);
        a.order_sweep_start(0, [244.0, 35.0], 1.0);
        a.order_sweep_to([244.0, 125.0], 1.0);
        assert_eq!(by_hand(&a), [h.low_right], "nothing changes until the button is let go");
        assert!(a.order_sweep_end());
        assert_eq!(by_hand(&a), [h.low_right, h.low_left, h.top_left]);
        assert!(a.order_box(0, [280.0, 110.0], [300.0, 140.0]));
        assert_eq!(by_hand(&a), [h.low_right, h.low_left, h.top_left, h.top_right]);
        // All Marks and All Cut add the rest, the outline last.
        assert!(a.cut_order(0, &Command::AllMarks));
        assert!(a.cut_order(0, &Command::AllCut));
        assert_eq!(by_hand(&a), [h.low_right, h.low_left, h.top_left, h.top_right, MARK, h.big, OUTLINE]);
        assert_eq!(a.undo.len(), 5, "one undo step per command");
        // Ctrl+Z takes back one command at a time.
        a.undo();
        assert_eq!(by_hand(&a), [h.low_right, h.low_left, h.top_left, h.top_right, MARK]);
        a.undo();
        a.undo();
        assert_eq!(by_hand(&a), [h.low_right, h.low_left, h.top_left]);
        a.undo();
        assert_eq!(by_hand(&a), [h.low_right]);
        a.undo();
        assert_eq!(by_hand(&a), Vec::<usize>::new());
    }

    #[test]
    fn a_pick_that_adds_nothing_is_no_undo_step() {
        let mut a = app();
        a.order_pick_start();
        let status = a.status.clone();
        // Open space: nothing, and the status line keeps the hint.
        assert!(!a.order_click(0, [150.0, 120.0], 1.0));
        assert_eq!(a.status, status);
        // A box with nothing in it says so.
        assert!(!a.order_box(0, [140.0, 100.0], [160.0, 130.0]));
        assert!(a.status.contains("No cut or marked contour"), "{}", a.status);
        // The same hole twice: the second time it is already in the order.
        assert!(a.order_click(0, [290.0, 41.4], 1.0));
        assert!(!a.order_click(0, [290.0, 41.4], 1.0));
        assert!(a.status.starts_with("Nothing added") && a.status.contains("already in the cut order"), "{}", a.status);
        // A click on the No cut line says why it has no place.
        assert!(!a.order_click(0, [60.0, 22.4], 1.0));
        assert!(a.status.contains("No cut") && a.status.contains("no place in the cut order"), "{}", a.status);
        assert_eq!(a.undo.len(), 1);
    }

    #[test]
    fn a_sweep_that_passes_the_outline_first_leaves_it_out_and_says_why() {
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        a.order_pick_start();
        // Up from the bottom edge (the outline) over the lower left hole.
        a.order_sweep_start(0, [250.0, 0.4], 1.0);
        a.order_sweep_to([250.0, 40.0], 1.0);
        assert!(a.order_sweep_end());
        assert_eq!(by_hand(&a), [h.low_left]);
        assert!(a.status.contains("Contour 6, the part's outline, is left out") && a.status.contains("always cut last"), "{}", a.status);
        // The outline swept last is taken, and stays last.
        a.order_sweep_start(0, [250.0, 130.0], 1.0);
        a.order_sweep_to([250.0, 159.6], 1.0);
        assert!(a.order_sweep_end());
        assert_eq!(by_hand(&a), [h.low_left, h.top_left, OUTLINE]);
        assert_eq!(shown(&a).last(), Some(&(OUTLINE, true)));
    }

    #[test]
    fn esc_mid_sweep_keeps_what_was_swept_and_ends_picking() {
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        a.order_pick_start();
        a.order_sweep_start(0, [296.0, 35.0], 1.0);
        a.order_sweep_to([240.0, 35.0], 1.0);
        // While sweeping, the drawing numbers the contours swept so far.
        let preview = a.order_preview(0).expect("a sweep in progress");
        assert_eq!(cutorder::hand(&preview), [h.low_right, h.low_left]);
        a.order_pick_end();
        assert_eq!(a.order_pick.on, None);
        assert_eq!(by_hand(&a), [h.low_right, h.low_left]);
        assert!(a.status.ends_with("Picking ended."), "{}", a.status);
        // One Ctrl+Z takes the whole sweep back.
        assert_eq!(a.undo.len(), 1);
        a.undo();
        assert_eq!(by_hand(&a), Vec::<usize>::new());
    }

    #[test]
    fn machine_part_clears_the_order_set_by_hand_as_one_undo_step() {
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        a.order_click(0, [290.0, 41.4], 1.0);
        a.order_click(0, [250.0, 41.4], 1.0);
        a.dv.machining = false;
        a.machine_part();
        assert!(a.dv.machining);
        assert_eq!(by_hand(&a), Vec::<usize>::new());
        assert!(a.status.contains("cleared"), "{}", a.status);
        a.undo();
        assert_eq!(by_hand(&a), [h.low_right, h.low_left]);
        // With nothing set by hand, it only machines: no undo step.
        let steps = a.undo.len();
        a.cut_order(0, &Command::Clear);
        let steps_after_clear = a.undo.len();
        a.machine_part();
        assert_eq!(a.undo.len(), steps_after_clear);
        assert_eq!(steps_after_clear, steps + 1);
    }

    #[test]
    fn the_drawing_numbers_each_contour_where_it_starts_set_by_hand_first() {
        // The holes nobody ordered by hand in the part's own order (Order
        // mode of the holes None, CO14), so they are numbered by contour.
        let _drawn = on_machine("B565 0\n");
        let mut a = app();
        let h = holes(&contours::list(&a.parts[0]));
        let mut holes_in_order = vec![h.big, h.low_left, h.low_right, h.top_left, h.top_right];
        holes_in_order.sort_unstable();
        // No order set by hand: the mark, the holes in number order, the outline, all grey.
        let auto: Vec<(usize, bool)> = [vec![MARK], holes_in_order.clone(), vec![OUTLINE]].concat().into_iter().map(|k| (k, false)).collect();
        assert_eq!(shown(&a), auto);
        // Two holes by hand: solid and first; the rest grey after them.
        a.order_click(0, [290.0, 128.6 + 2.4], 1.0);
        a.order_click(0, [250.0, 41.4], 1.0);
        let rest: Vec<(usize, bool)> = [vec![MARK], holes_in_order.iter().copied().filter(|&k| k != h.top_right && k != h.low_left).collect(), vec![OUTLINE]].concat().into_iter().map(|k| (k, false)).collect();
        assert_eq!(shown(&a), [vec![(h.top_right, true), (h.low_left, true)], rest].concat());
        // A cut contour is numbered at its pierce; the mark at its first point.
        let plan = crate::cnc::plan_part(&a.parts, 0, &a.spec);
        let st = stops(&a.parts[0], &plan);
        let list = contours::list(&a.parts[0]);
        let cut = dxf::cut_order(&list);
        let pierce = |k: usize| plan.contours.iter().find(|c| cut[c.contour] == k).map(|c| c.lead_in).unwrap();
        assert_eq!(st[0].start, pierce(h.top_right));
        assert_eq!(st.iter().find(|s| s.contour == MARK).unwrap().start, list[MARK].pts[0]);
        // The pierce is inside the hole (in the scrap), within its radius.
        let d = (st[0].start[0] - 290.0).hypot(st[0].start[1] - 125.0);
        assert!(d < 6.0, "{d}");
    }

    #[test]
    fn the_screen_and_part_cut_order_set_the_same_order() {
        // The holes nobody ordered by hand in the part's own order (Order
        // mode of the holes None, CO14), so they are numbered by contour.
        let _drawn = on_machine("B565 0\n");
        use crate::engine::{session::Session, Ctx};
        use serde_json::{json, Value};
        let d = std::env::temp_dir().join(format!("nestpro_orderpick_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let job = d.join("plate.snjob");
        let call = |s: &mut Session, op: &str, job: Option<&str>, args: Value| s.call(op, job, &args).unwrap_or_else(|e| panic!("{op} {args}: {e}")).json;
        let mut s = Session::new(Ctx::builtin());
        call(&mut s, "new", Some(job.to_str().unwrap()), json!({"w": 1000, "h": 500, "material": "steel", "thickness": 3}));
        let dxf = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/drawings/DEMO-BEND-PLATE.dxf");
        call(&mut s, "import-drawing", None, json!({"path": dxf.to_str().unwrap(), "qty": 1}));
        call(&mut s, "contour-tech", None, json!({"part": 1, "contours": "7", "state": "mark"}));
        // part-info's cut order: (contours in the order cut, those set by hand).
        let info = |s: &mut Session| {
            let pi = call(s, "part-info", None, json!({"part": 1}));
            let nums = |v: &Value| v.as_array().unwrap_or_else(|| panic!("{pi}")).iter().map(|n| n.as_u64().unwrap() as usize).collect::<Vec<_>>();
            (nums(&pi["data"]["cut_order"]["contours"]), nums(&pi["data"]["cut_order"]["by_hand"]))
        };
        // The drawing's numbers, as contour numbers from 1.
        let screen = |a: &App| {
            let st = shown(a);
            (st.iter().map(|x| x.0 + 1).collect::<Vec<_>>(), st.iter().filter(|x| x.1).map(|x| x.0 + 1).collect::<Vec<_>>())
        };

        // On the screen: a click on hole 5 (top right), a sweep up over holes
        // 2 and 3 (the left column), a box round hole 1 (the big one), All Marks.
        let mut a = app();
        a.order_pick_start();
        a.order_click(0, [290.0, 131.0], 1.0);
        a.order_sweep_start(0, [244.0, 35.0], 1.0);
        a.order_sweep_to([244.0, 125.0], 1.0);
        a.order_sweep_end();
        a.order_box(0, [40.0, 50.0], [100.0, 110.0]);
        a.cut_order(0, &Command::AllMarks);
        // The agent, the same commands by number.
        for args in [json!({"mode": "add", "contours": "5"}), json!({"mode": "add", "contours": "2, 3"}), json!({"mode": "add", "contours": "1"}), json!({"mode": "all-marks"})] {
            let mut args = args;
            args["part"] = json!(1);
            call(&mut s, "part-cut-order", None, args);
        }
        assert_eq!(info(&mut s), (vec![5, 2, 3, 1, 7, 4, 6], vec![5, 2, 3, 1, 7]));
        assert_eq!(screen(&a), info(&mut s));
        // Ctrl+Z and the agents' undo take back the same command: All Marks,
        // then the box.
        for _ in 0..2 {
            a.undo();
            call(&mut s, "part-cut-order", None, json!({"part": 1, "mode": "undo"}));
            assert_eq!(screen(&a), info(&mut s));
        }
        assert_eq!(info(&mut s), (vec![5, 2, 3, 7, 1, 4, 6], vec![5, 2, 3]));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_built_in_shape_cant_be_ordered_by_hand_and_says_why() {
        let mut a = app();
        let square = crate::nest::Part { name: "Square".into(), shape: crate::nest::Shape::PlateHole, w: 190.0, h: 190.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![square];
        a.open_part(0);
        let why = unavailable(&a.parts[0]).expect("a built-in shape");
        assert!(why.contains("built-in") && why.contains("automatic order") && why.contains("import"), "{why}");
        assert!(!why.contains("export-part-dxf"), "screen words, not an operation: {why}");
        a.order_pick_start();
        assert_eq!(a.order_pick.on, None);
        assert_eq!(a.status, why);
        // Its numbers still show, in the automatic order: the hole, then the outline.
        let plan = crate::cnc::plan_part(&a.parts, 0, &a.spec);
        assert_eq!(stops(&a.parts[0], &plan).iter().map(|s| (s.contour, s.by_hand)).collect::<Vec<_>>(), [(0, false), (1, false)]);
    }
}
