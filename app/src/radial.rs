//! A circular tool menu on the canvas, like a CAD program's mouse gestures:
//! press the right mouse button (or S) and eight tools appear in a ring.
//! - Flick: keep the right button down, move toward a tool, let go — it runs.
//! - Click: a plain right-click leaves the ring open; click a tool to run it,
//!   click the middle, press Esc or right-click again to close it.
//!
//! The ring shows the tools for where it was opened (QR1): the Part view's
//! technology tools, the moving tools on a part on the sheet, or the sheet
//! tools on empty sheet. A tool found in several rings keeps its direction in
//! each (QR2), and a tool that can't run is greyed, never moved.

use crate::nest::P;
use crate::theme::th;
use crate::ui_parts::{draw_icon_with, with_alpha, Icon};
use crate::{App, Workspace};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Id, Order, PointerButton, Pos2, Rect, Sense, Shape as EShape, Stroke, Vec2};

/// Ring size (screen pixels).
const INNER: f32 = 30.0;
const OUTER: f32 = 120.0;
const ICON_R: f32 = 80.0;
/// Moving at least this far with the button held makes it a flick.
const FLICK: f32 = 26.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    // Part view (technology).
    LeadAuto,
    LeadOnElement,
    JointsAuto,
    JointOnElement,
    PlaceText,
    TextOnElement,
    Loops,
    // A part on the sheet.
    Turn,
    TurnBack,
    CopyHere,
    MirrorLr,
    MirrorTb,
    Delete,
    // Empty sheet.
    RenestSheet,
    NextSheet,
    PrevSheet,
    CutOff,
    Stock,
    // Shared, at the same place in every ring that has them (QR2).
    Undo,
    Redo,
    FitView,
}

/// Which ring opens (QR1): decided by where the right-click landed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ring {
    /// The Part view: technology on the part and its elements.
    Part,
    /// On a part on the sheet: move, turn, mirror, copy, delete the selection.
    OnPart,
    /// On empty sheet: the sheet and the view.
    Sheet,
}

impl Ring {
    /// The tools clockwise from the top (N, NE, E, SE, S, SW, W, NW).
    pub fn slots(self) -> [Act; 8] {
        use Act::*;
        match self {
            Ring::Part => [LeadAuto, LeadOnElement, JointsAuto, JointOnElement, PlaceText, TextOnElement, Loops, FitView],
            Ring::OnPart => [Turn, TurnBack, CopyHere, MirrorTb, MirrorLr, Undo, Delete, FitView],
            Ring::Sheet => [RenestSheet, Redo, NextSheet, CutOff, Stock, Undo, PrevSheet, FitView],
        }
    }
}

impl Act {
    pub fn label(self) -> &'static str {
        match self {
            Act::LeadAuto => "Automatic lead-ins",
            Act::LeadOnElement => "Place a lead-in on this element (every piece of this part type gets it)",
            Act::JointsAuto => "Automatic micro-joints",
            Act::JointOnElement => "Place a micro-joint on this element (every piece of this part type gets it)",
            Act::PlaceText => "Place text",
            Act::TextOnElement => "Text on element",
            Act::Loops => "Automatic loops",
            Act::Turn => "Turn to the next allowed angle",
            Act::TurnBack => "Turn back to the previous angle",
            Act::CopyHere => "Copy — click to drop it",
            Act::MirrorLr => "Mirror left ↔ right",
            Act::MirrorTb => "Mirror top ↕ bottom",
            Act::Delete => "Take off the sheet",
            Act::RenestSheet => "Re-nest this sheet (its parts only)",
            Act::NextSheet => "Next sheet",
            Act::PrevSheet => "Previous sheet",
            Act::CutOff => "Cut off the leftover as a remnant",
            Act::Stock => "Sheet stock — put a sheet or remnant on this sheet",
            Act::Undo => "Undo",
            Act::Redo => "Redo",
            Act::FitView => "Fit view",
        }
    }
    /// Short name on the ring ("here" = on the selected element).
    fn short(self) -> &'static str {
        match self {
            Act::LeadAuto => "Auto leads",
            Act::LeadOnElement => "Lead here",
            Act::JointsAuto => "Auto joints",
            Act::JointOnElement => "Joint here",
            Act::PlaceText => "Add text",
            Act::TextOnElement => "Text here",
            Act::Loops => "Auto loops",
            Act::Turn => "Turn",
            Act::TurnBack => "Turn back",
            Act::CopyHere => "Copy",
            Act::MirrorLr => "Mirror ↔",
            Act::MirrorTb => "Mirror ↕",
            Act::Delete => "Delete",
            Act::RenestSheet => "Re-nest sheet",
            Act::NextSheet => "Next sheet",
            Act::PrevSheet => "Prev sheet",
            Act::CutOff => "Cut off leftover",
            Act::Stock => "Sheet stock",
            Act::Undo => "Undo",
            Act::Redo => "Redo",
            Act::FitView => "Fit",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Act::LeadAuto => Icon::LeadIn,
            Act::LeadOnElement => Icon::OnElement,
            Act::JointsAuto | Act::JointOnElement => Icon::Joint,
            Act::PlaceText | Act::TextOnElement => Icon::Text,
            Act::Loops => Icon::Loop,
            // The next allowed turn is counter-clockwise, the way back clockwise.
            Act::Turn => Icon::RotateLeft,
            Act::TurnBack => Icon::Rotate,
            Act::CopyHere => Icon::Copy,
            Act::MirrorLr => Icon::Mirror,
            Act::MirrorTb => Icon::MirrorTb,
            Act::Delete => Icon::Remove,
            Act::RenestSheet => Icon::NestSheet,
            Act::NextSheet => Icon::Next,
            Act::PrevSheet => Icon::Prev,
            Act::CutOff => Icon::CutLine,
            Act::Stock => Icon::Stock,
            Act::Undo => Icon::Undo,
            Act::Redo => Icon::Redo,
            Act::FitView => Icon::Fit,
        }
    }
    /// Element tools work on the selected line/arc in the Part view.
    fn on_element(self) -> bool {
        matches!(self, Act::LeadOnElement | Act::JointOnElement | Act::TextOnElement)
    }
}

/// Which slot a pointer offset from the centre points at (None in the middle).
pub fn slot_at(d: Vec2) -> Option<usize> {
    if d.length() < INNER {
        return None;
    }
    // Screen y grows downwards; slot 0 is straight up, then clockwise.
    let a = d.y.atan2(d.x).to_degrees() + 90.0;
    Some(((a / 45.0).round() as i32).rem_euclid(8) as usize)
}

#[derive(Clone, Copy, Debug)]
pub struct Radial {
    pub center: Pos2,
    pub opened: f64,
    /// The right button is still down since it opened (a flick is possible).
    pub held: bool,
    /// A text on the part under the pointer when the ring opened (Text edits it).
    pub text_under: Option<usize>,
    /// Which tools it shows.
    pub ring: Ring,
    /// Where it opened on the sheet (mm), for Copy.
    pub at: Option<P>,
}

impl Radial {
    pub fn new(center: Pos2, ring: Ring) -> Radial {
        Radial { center, opened: -1.0, held: false, text_under: None, ring, at: None }
    }
}

impl App {
    /// Right button pressed (or S) over a hovered canvas.
    pub fn radial_trigger(&self, ui: &egui::Ui) -> bool {
        // Not while parts are being dragged or carried (right-click cancels a carry).
        if self.radial.is_some() || self.dock.dragging.is_some() || self.drag.is_some() || self.turn_drag.is_some() || self.carry.is_some() {
            return false;
        }
        let right = ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary));
        let s_key = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.is_none());
        right || s_key
    }

    pub fn open_radial(&mut self, ctx: &egui::Context, at: Pos2, ring: Ring) {
        let held = ctx.input(|i| i.pointer.button_down(PointerButton::Secondary));
        self.radial = Some(Radial { opened: ctx.input(|i| i.time), held, ..Radial::new(at, ring) });
        self.placing_text = false;
    }

    fn sheet_len(&self) -> usize {
        self.result.sheets.get(self.cur_sheet).map_or(0, |s| s.len())
    }

    pub(crate) fn radial_enabled(&self, a: Act) -> bool {
        let in_part = self.workspace == Workspace::Part;
        let has_sel = !in_part && !self.selection().is_empty();
        match a {
            Act::FitView | Act::Stock => true,
            Act::PlaceText => in_part,
            a if a.on_element() => in_part && !self.sel_elems.is_empty(),
            Act::LeadAuto | Act::JointsAuto | Act::Loops => !self.tech_targets().is_empty(),
            // Greyed when the selection's rotations don't allow it (QR2).
            Act::Turn | Act::TurnBack => has_sel && self.can_step_turn(a == Act::TurnBack),
            Act::MirrorLr | Act::MirrorTb => {
                has_sel && self.result.sheets.get(self.cur_sheet).is_some_and(|s| crate::engine::arrange::can_mirror(&self.parts, s, &self.selection(), a == Act::MirrorLr))
            }
            Act::CopyHere | Act::Delete => has_sel,
            Act::RenestSheet | Act::CutOff => self.sheet_len() > 0,
            Act::NextSheet => self.cur_sheet + 1 < self.result.sheets.len(),
            Act::PrevSheet => self.cur_sheet > 0,
            Act::Undo => !self.undo.is_empty(),
            Act::Redo => !self.redo.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn radial_run(&mut self, a: Act, rm: Radial) {
        match a {
            // On a part, the ring works on that part type (the ribbon's
            // "Apply to" is for the ribbon), and says what it changed.
            Act::LeadAuto | Act::JointsAuto | Act::Loops => {
                let k = match a {
                    Act::LeadAuto => crate::techui::TechRerun::AutoLeads,
                    Act::JointsAuto => crate::techui::TechRerun::AutoJoints,
                    _ => crate::techui::TechRerun::AutoLoops,
                };
                let keep = self.tech_all;
                if self.sel_part.is_some_and(|i| i < self.parts.len()) {
                    self.tech_all = false;
                }
                self.run_tech(k);
                self.tech_all = keep;
            }
            Act::LeadOnElement => self.lead_on_element(),
            Act::JointOnElement => self.joint_on_elements(),
            // The text tools open the quick editor, ready to type.
            Act::PlaceText => match (rm.text_under, self.shown_part()) {
                (Some(k), Some(p)) => self.edit_text_at(rm.center, p, k),
                _ => self.open_text_editor(rm.center, crate::textedit::TextTarget::New { on_element: false }),
            },
            Act::TextOnElement => self.open_text_editor(rm.center, crate::textedit::TextTarget::New { on_element: true }),
            Act::Turn => self.rotate_selected(),
            Act::TurnBack => self.turn_back_selected(),
            Act::CopyHere => {
                if let Some(w) = rm.at {
                    self.start_carry_copy(w);
                }
            }
            Act::MirrorLr => self.mirror_selected(true),
            Act::MirrorTb => self.mirror_selected(false),
            Act::Delete => self.remove_selected(),
            Act::RenestSheet => self.renest_sheet(),
            Act::NextSheet => self.select_sheet(self.cur_sheet + 1),
            Act::PrevSheet => self.select_sheet(self.cur_sheet.saturating_sub(1)),
            Act::CutOff => self.auto_remnant_edge(),
            Act::Stock => self.stockui.open = true,
            Act::Undo => self.undo(),
            Act::Redo => self.redo(),
            Act::FitView => {
                if self.workspace == Workspace::Part {
                    self.part_zoom = 1.0;
                    self.part_pan = Vec2::ZERO;
                } else {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
            }
        }
    }

    /// Draw the ring and handle flicks and clicks. Call once per frame, after the canvas.
    pub fn radial_ui(&mut self, ctx: &egui::Context) {
        let Some(rm) = self.radial else { return };
        let slots = rm.ring.slots();
        let p = ctx.pointer_latest_pos().unwrap_or(rm.center);
        let d = p - rm.center;
        let hover = slot_at(d).filter(|&k| d.length() <= OUTER * 1.6 && self.radial_enabled(slots[k]));
        let (right_up, left_click, right_click, esc) = ctx.input(|i| {
            (
                i.pointer.button_released(PointerButton::Secondary),
                i.pointer.button_pressed(PointerButton::Primary),
                i.pointer.button_pressed(PointerButton::Secondary),
                i.key_pressed(egui::Key::Escape),
            )
        });

        // Draw.
        let t = ((ctx.input(|i| i.time) - rm.opened) / 0.12).clamp(0.0, 1.0) as f32;
        let ease = 1.0 - (1.0 - t).powi(3);
        let scale = 0.7 + 0.3 * ease;
        let rect = Rect::from_center_size(rm.center, Vec2::splat(OUTER * 2.0 + 40.0));
        egui::Area::new(Id::unique("radial_menu")).order(Order::Foreground).fixed_pos(rect.min).constrain(false).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
            let painter = ui.painter_at(r.expand(4.0));
            let c = rm.center;
            let th = th();
            // Nearly opaque, so busy geometry under the ring doesn't show through.
            let alpha = (252.0 * ease) as u8;
            painter.circle_filled(c + Vec2::new(0.0, 6.0), OUTER * scale + 2.0, Color32::from_black_alpha((60.0 * ease) as u8));
            painter.circle_filled(c, OUTER * scale, with_alpha(th.panel, alpha));
            painter.circle_stroke(c, OUTER * scale, Stroke::new(1.0, with_alpha(th.border, alpha)));
            let wedge = |k: usize, col: Color32| {
                let mid = (k as f32 * 45.0 - 90.0).to_radians();
                let mut mesh = egui::Mesh::default();
                let steps = 12;
                for s in 0..=steps {
                    let a = mid - 22.5f32.to_radians() + 45f32.to_radians() * s as f32 / steps as f32;
                    let u = Vec2::new(a.cos(), a.sin());
                    mesh.colored_vertex(c + u * INNER * scale, col);
                    mesh.colored_vertex(c + u * OUTER * scale, col);
                }
                for s in 0..steps as u32 {
                    let i = s * 2;
                    mesh.add_triangle(i, i + 1, i + 2);
                    mesh.add_triangle(i + 1, i + 3, i + 2);
                }
                painter.add(EShape::mesh(mesh));
            };
            // Highlight the wedge being pointed at.
            if let Some(k) = hover {
                wedge(k, with_alpha(th.accent, (90.0 * ease) as u8));
            }
            // Dividers between the tools.
            for k in 0..8 {
                let a = (k as f32 * 45.0 - 90.0 + 22.5).to_radians();
                let u = Vec2::new(a.cos(), a.sin());
                painter.line_segment([c + u * (INNER + 4.0) * scale, c + u * (OUTER - 6.0) * scale], Stroke::new(1.0, with_alpha(th.border, (160.0 * ease) as u8)));
            }
            // Tools.
            for (k, a) in slots.iter().enumerate() {
                let ang = (k as f32 * 45.0 - 90.0).to_radians();
                let pos = c + Vec2::new(ang.cos(), ang.sin()) * ICON_R * scale;
                let en = self.radial_enabled(*a);
                let hot = hover == Some(k);
                let ir = Rect::from_center_size(pos - Vec2::new(0.0, 7.0), Vec2::splat(24.0));
                let ink = if !en { with_alpha(th.text_dim, 150) } else if hot { th.accent_fg } else { th.text };
                let accent = if !en { with_alpha(th.text_dim, 150) } else { th.accent_fg };
                draw_icon_with(&painter, ir, a.icon(), ink, accent, false);
                painter.text(pos + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, self.radial_short(*a, &rm), FontId::proportional(12.0), ink);
            }
            // Middle: what will happen.
            painter.circle_filled(c, INNER * scale, with_alpha(th.pane_header, alpha));
            painter.circle_stroke(c, INNER * scale, Stroke::new(1.0, with_alpha(th.border, alpha)));
            // The middle closes the ring: a small cross, hidden while a tool is pointed at.
            if hover.is_none() {
                let k = 4.5 * scale;
                let st = Stroke::new(1.6, th.text_dim);
                painter.line_segment([c + Vec2::new(-k, -k), c + Vec2::new(k, k)], st);
                painter.line_segment([c + Vec2::new(-k, k), c + Vec2::new(k, -k)], st);
            }
            if rm.held && d.length() > 4.0 {
                painter.line_segment([c, p], Stroke::new(2.0, with_alpha(th.accent, 180)));
            }
            if let Some(k) = hover {
                let tip = Rect::from_center_size(c + Vec2::new(0.0, OUTER * scale + 16.0), Vec2::new(230.0, 22.0));
                painter.rect_filled(tip, CornerRadius::same(6), th.overlay);
                painter.text(tip.center(), Align2::CENTER_CENTER, slots[k].label(), FontId::proportional(12.0), Color32::WHITE);
            }
        });
        ctx.request_repaint();

        // Input.
        let run = |me: &mut App, k: Option<usize>| {
            me.radial = None;
            if let Some(k) = k {
                me.radial_run(slots[k], rm);
            }
        };
        if esc {
            self.radial = None;
        } else if rm.held && right_up {
            if d.length() >= FLICK && hover.is_some() {
                run(self, hover); // a flick
            } else {
                // A plain right-click: keep the ring open for a click.
                if let Some(r) = self.radial.as_mut() {
                    r.held = false;
                }
            }
        } else if !rm.held && (left_click || right_click) {
            run(self, if left_click { hover } else { None });
        }
    }

    /// The name under a tool, which for some tools shows their state.
    fn radial_short(&self, a: Act, rm: &Radial) -> &'static str {
        match a {
            Act::PlaceText if rm.text_under.is_some() => "Edit text",
            _ => a.short(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Up-left in every ring.
    const FIT_SLOT: usize = 7;
    /// Down-left in every ring that has it.
    const UNDO_SLOT: usize = 5;

    #[test]
    fn directions_map_to_the_eight_slots() {
        assert_eq!(slot_at(Vec2::new(0.0, -60.0)), Some(0)); // up: lead-ins
        assert_eq!(slot_at(Vec2::new(60.0, 0.0)), Some(2)); // right
        assert_eq!(slot_at(Vec2::new(0.0, 60.0)), Some(4)); // down
        assert_eq!(slot_at(Vec2::new(-60.0, 0.0)), Some(6)); // left
        assert_eq!(slot_at(Vec2::new(-42.0, -42.0)), Some(7)); // up-left
        assert_eq!(slot_at(Vec2::new(5.0, 5.0)), None); // the middle cancels
        assert_eq!(Ring::Part.slots()[4], Act::PlaceText);
    }

    #[test]
    fn shared_tools_keep_their_direction_in_every_ring() {
        // QR2: a tool in several rings sits in the same slot in each.
        let rings = [Ring::Part, Ring::OnPart, Ring::Sheet];
        for a in rings.iter().flat_map(|r| r.slots()) {
            let at: Vec<usize> = rings.iter().filter_map(|r| r.slots().iter().position(|&b| b == a)).collect();
            assert!(at.windows(2).all(|w| w[0] == w[1]), "{a:?} moves between rings: {at:?}");
        }
        for r in rings {
            assert_eq!(r.slots()[FIT_SLOT], Act::FitView);
        }
        assert_eq!(Ring::OnPart.slots()[UNDO_SLOT], Act::Undo);
        assert_eq!(Ring::Sheet.slots()[UNDO_SLOT], Act::Undo);
        // No tool twice in one ring.
        for r in rings {
            let s = r.slots();
            for i in 0..8 {
                assert!(!s[i + 1..].contains(&s[i]), "{r:?} has {:?} twice", s[i]);
            }
        }
    }

    #[test]
    fn element_tools_need_a_selected_element() {
        let mut a = App::new(None);
        a.open_part(3);
        assert!(!a.radial_enabled(Act::LeadOnElement));
        a.sel_elems = vec![0];
        assert!(a.radial_enabled(Act::LeadOnElement));
        assert!(a.radial_enabled(Act::FitView));
        a.radial_run(Act::LeadAuto, Radial::new(Pos2::ZERO, Ring::Part));
        assert!(a.parts[3].tech.is_some_and(|t| t.corner_start));
    }
}
