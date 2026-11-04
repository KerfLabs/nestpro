//! A circular tool menu on the canvas, like a CAD program's mouse gestures:
//! press the right mouse button (or S) and eight tools appear in a ring.
//! - Flick: keep the right button down, move toward a tool, let go — it runs.
//! - Click: a plain right-click leaves the ring open; click a tool to run it,
//!   click the middle, press Esc or right-click again to close it.

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
    LeadAuto,
    LeadOnElement,
    JointsAuto,
    JointOnElement,
    PlaceText,
    TextOnElement,
    Loops,
    FitView,
}

/// Clockwise from the top (N, NE, E, SE, S, SW, W, NW).
pub const SLOTS: [Act; 8] = [Act::LeadAuto, Act::LeadOnElement, Act::JointsAuto, Act::JointOnElement, Act::PlaceText, Act::TextOnElement, Act::Loops, Act::FitView];

impl Act {
    pub fn label(self) -> &'static str {
        match self {
            Act::LeadAuto => "Lead-ins",
            Act::LeadOnElement => "Lead on element",
            Act::JointsAuto => "Micro-joints",
            Act::JointOnElement => "Joint on element",
            Act::PlaceText => "Place text",
            Act::TextOnElement => "Text on element",
            Act::Loops => "Loops",
            Act::FitView => "Fit view",
        }
    }
    /// Short name on the ring ("here" = on the selected element).
    fn short(self) -> &'static str {
        match self {
            Act::LeadAuto => "Lead-ins",
            Act::LeadOnElement => "Lead here",
            Act::JointsAuto => "Joints",
            Act::JointOnElement => "Joint here",
            Act::PlaceText => "Text",
            Act::TextOnElement => "Text here",
            Act::Loops => "Loops",
            Act::FitView => "Fit",
        }
    }
    fn icon(self) -> Icon {
        match self {
            Act::LeadAuto => Icon::LeadIn,
            Act::LeadOnElement => Icon::CutPath,
            Act::JointsAuto | Act::JointOnElement => Icon::Joint,
            Act::PlaceText | Act::TextOnElement => Icon::Numbers,
            Act::Loops => Icon::Loop,
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
}

impl App {
    /// Right button pressed (or S) over a hovered canvas.
    pub fn radial_trigger(&self, ui: &egui::Ui) -> bool {
        if self.radial.is_some() || self.dock.dragging.is_some() {
            return false;
        }
        let right = ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary));
        let s_key = !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.is_none());
        right || s_key
    }

    pub fn open_radial(&mut self, ctx: &egui::Context, at: Pos2) {
        let held = ctx.input(|i| i.pointer.button_down(PointerButton::Secondary));
        self.radial = Some(Radial { center: at, opened: ctx.input(|i| i.time), held, text_under: None });
        self.placing_text = false;
    }

    fn radial_enabled(&self, a: Act) -> bool {
        let in_part = self.workspace == Workspace::Part;
        match a {
            Act::FitView => true,
            Act::PlaceText => in_part,
            Act::TextOnElement => in_part && !self.sel_elems.is_empty(),
            a if a.on_element() => in_part && !self.sel_elems.is_empty(),
            _ => !self.tech_targets().is_empty(),
        }
    }

    fn radial_run(&mut self, a: Act, rm: Radial) {
        match a {
            Act::LeadAuto => self.auto_leads(),
            Act::LeadOnElement => self.lead_on_element(),
            Act::JointsAuto => self.auto_joints_now(),
            Act::JointOnElement => self.joint_on_elements(),
            // The text tools open the quick editor, ready to type.
            Act::PlaceText => match (rm.text_under, self.shown_part()) {
                (Some(k), Some(p)) => self.edit_text_at(rm.center, p, k),
                _ => self.open_text_editor(rm.center, crate::textedit::TextTarget::New { on_element: false }),
            },
            Act::TextOnElement => self.open_text_editor(rm.center, crate::textedit::TextTarget::New { on_element: true }),
            Act::Loops => self.auto_loops(),
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
        let p = ctx.pointer_latest_pos().unwrap_or(rm.center);
        let d = p - rm.center;
        let hover = slot_at(d).filter(|&k| d.length() <= OUTER * 1.6 && self.radial_enabled(SLOTS[k]));
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
            let alpha = (235.0 * ease) as u8;
            painter.circle_filled(c + Vec2::new(0.0, 6.0), OUTER * scale + 2.0, Color32::from_black_alpha((60.0 * ease) as u8));
            painter.circle_filled(c, OUTER * scale, with_alpha(th.panel, alpha));
            painter.circle_stroke(c, OUTER * scale, Stroke::new(1.0, with_alpha(th.border, alpha)));
            // Highlight the wedge being pointed at.
            if let Some(k) = hover {
                let mid = (k as f32 * 45.0 - 90.0).to_radians();
                let mut mesh = egui::Mesh::default();
                let col = with_alpha(th.accent, (90.0 * ease) as u8);
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
            }
            // Dividers between the tools.
            for k in 0..8 {
                let a = (k as f32 * 45.0 - 90.0 + 22.5).to_radians();
                let u = Vec2::new(a.cos(), a.sin());
                painter.line_segment([c + u * (INNER + 4.0) * scale, c + u * (OUTER - 6.0) * scale], Stroke::new(1.0, with_alpha(th.border, (160.0 * ease) as u8)));
            }
            // Tools.
            for (k, a) in SLOTS.iter().enumerate() {
                let ang = (k as f32 * 45.0 - 90.0).to_radians();
                let pos = c + Vec2::new(ang.cos(), ang.sin()) * ICON_R * scale;
                let en = self.radial_enabled(*a);
                let hot = hover == Some(k);
                let ir = Rect::from_center_size(pos - Vec2::new(0.0, 7.0), Vec2::splat(24.0));
                let ink = if !en { with_alpha(th.text_dim, 90) } else if hot { th.accent_fg } else { th.text };
                let accent = if !en { with_alpha(th.text_dim, 90) } else { th.accent_fg };
                draw_icon_with(&painter, ir, a.icon(), ink, accent, false);
                painter.text(pos + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, if *a == Act::PlaceText && rm.text_under.is_some() { "Edit text" } else { a.short() }, FontId::proportional(11.0), ink);
            }
            // Middle: what will happen.
            painter.circle_filled(c, INNER * scale, with_alpha(th.pane_header, alpha));
            painter.circle_stroke(c, INNER * scale, Stroke::new(1.0, with_alpha(th.border, alpha)));
            let mid_txt = if hover.is_some() { "go" } else { "×" };
            painter.text(c, Align2::CENTER_CENTER, mid_txt, FontId::proportional(13.0), th.text_dim);
            if rm.held && d.length() > 4.0 {
                painter.line_segment([c, p], Stroke::new(2.0, with_alpha(th.accent, 180)));
            }
            if let Some(k) = hover {
                let tip = Rect::from_center_size(c + Vec2::new(0.0, OUTER * scale + 16.0), Vec2::new(150.0, 22.0));
                painter.rect_filled(tip, CornerRadius::same(6), th.overlay);
                painter.text(tip.center(), Align2::CENTER_CENTER, SLOTS[k].label(), FontId::proportional(12.0), Color32::WHITE);
            }
        });
        ctx.request_repaint();

        // Input.
        let run = |me: &mut App, k: Option<usize>| {
            me.radial = None;
            if let Some(k) = k {
                me.radial_run(SLOTS[k], rm);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_map_to_the_eight_slots() {
        assert_eq!(slot_at(Vec2::new(0.0, -60.0)), Some(0)); // up: lead-ins
        assert_eq!(slot_at(Vec2::new(60.0, 0.0)), Some(2)); // right
        assert_eq!(slot_at(Vec2::new(0.0, 60.0)), Some(4)); // down
        assert_eq!(slot_at(Vec2::new(-60.0, 0.0)), Some(6)); // left
        assert_eq!(slot_at(Vec2::new(-42.0, -42.0)), Some(7)); // up-left
        assert_eq!(slot_at(Vec2::new(5.0, 5.0)), None); // the middle cancels
        assert_eq!(SLOTS[4], Act::PlaceText);
    }

    #[test]
    fn element_tools_need_a_selected_element() {
        let mut a = App::new(None);
        a.open_part(3);
        assert!(!a.radial_enabled(Act::LeadOnElement));
        a.sel_elems = vec![0];
        assert!(a.radial_enabled(Act::LeadOnElement));
        assert!(a.radial_enabled(Act::FitView));
        a.radial_run(Act::LeadAuto, Radial { center: Pos2::ZERO, opened: 0.0, held: false, text_under: None });
        assert!(a.parts[3].tech.is_some_and(|t| t.corner_start));
    }
}
