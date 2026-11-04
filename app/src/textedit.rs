//! A small text editor that pops up at the pointer (from the radial menu, or by
//! double-clicking a text in the Part view). The cursor is already in the box, so
//! you can just type. New texts are then placed with a click (or along the selected
//! element); an existing text changes live as you type.

use crate::nest::{PartText, TextKind, P, TEXT_PARAMS};
use crate::theme::th;
use crate::ui_parts::with_alpha;
use crate::App;
use eframe::egui;
use egui::{Color32, CornerRadius, Id, Order, Pos2, Rect, RichText, Stroke, Vec2};

#[derive(Clone, Debug)]
pub enum TextTarget {
    /// A new text: placed by a click on the part, or along the selected element.
    New { on_element: bool },
    /// An existing text on the shown part (with its state before editing).
    Existing { part: usize, idx: usize, original: PartText },
}

#[derive(Clone, Debug)]
pub struct TextEditor {
    pub pos: Pos2,
    pub target: TextTarget,
    focused: bool,
    rect: Option<Rect>,
}

/// Index of the text whose strokes are under `p` (part mm, within `tol`).
/// The original's picking (T12): a text is hit
/// when the capture square crosses a side of the text's box or lies inside it; of
/// several, the nearest by distance to the box's sides wins (T13).
pub fn text_at(texts: &[PartText], p: P, tol: f64) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (k, t) in texts.iter().enumerate() {
        let b = text_box(t);
        let d = (0..4).map(|i| seg_dist(p, b[i], b[(i + 1) % 4])).fold(f64::MAX, f64::min);
        let hit = d <= tol || crate::nest::point_in_poly(&b, p);
        if hit && best.is_none_or(|x| d < x.1) {
            best = Some((k, d));
        }
    }
    best.map(|b| b.0)
}

/// The four corners of a text's box: the turned box of a straight text (the
/// original's text record keeps these four corners, T12);
/// for a curved text the box around its strokes.
pub fn text_box(t: &PartText) -> [P; 4] {
    if t.curve.abs() < 1e-9 {
        let w = crate::font::width(&t.text, t.height);
        let (s, c) = t.angle.to_radians().sin_cos();
        let at = |x: f64, y: f64| [t.at[0] + x * c - y * s, t.at[1] + x * s + y * c];
        return [at(0.0, 0.0), at(w, 0.0), at(w, t.height), at(0.0, t.height)];
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in t.strokes(&t.text).iter().flatten() {
        x0 = x0.min(q[0]);
        y0 = y0.min(q[1]);
        x1 = x1.max(q[0]);
        y1 = y1.max(q[1]);
    }
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Window selection of texts (T14): all four box corners inside.
pub fn text_in_window(t: &PartText, a: P, b: P) -> bool {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    text_box(t).iter().all(|q| q[0] >= x0 && q[0] <= x1 && q[1] >= y0 && q[1] <= y1)
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((p[0] - a[0] - ab[0] * t).powi(2) + (p[1] - a[1] - ab[1] * t).powi(2)).sqrt()
}

/// Change a text's arc radius, keeping the middle of its baseline in place.
pub fn with_curve(t: &PartText, curve: f64) -> PartText {
    let mut n = t.clone();
    if (curve.abs() > 1e-9) == (t.curve.abs() > 1e-9) {
        n.curve = curve;
        return n;
    }
    let w = crate::font::width(&t.text, t.height);
    // Middle of the baseline now.
    let mid = if t.curve.abs() > 1e-9 {
        let a = t.angle.to_radians();
        [t.at[0] + t.curve.abs() * a.cos(), t.at[1] + t.curve.abs() * a.sin()]
    } else {
        let a = t.angle.to_radians();
        [t.at[0] + w / 2.0 * a.cos(), t.at[1] + w / 2.0 * a.sin()]
    };
    if curve.abs() > 1e-9 {
        // Curved: centre below (over the top) or above (round the bottom).
        n.at = [mid[0], mid[1] - curve];
        n.angle = if curve > 0.0 { 90.0 } else { 270.0 };
    } else {
        n.at = [mid[0] - w / 2.0, mid[1]];
        n.angle = 0.0;
    }
    n.curve = curve;
    n
}

impl App {
    /// Open the editor at `pos` (screen) for a new or an existing text.
    pub fn open_text_editor(&mut self, pos: Pos2, target: TextTarget) {
        self.placing_text = false;
        match &target {
            TextTarget::Existing { original, .. } => {
                self.checkpoint();
                self.text_draft = original.text.clone();
                self.text_height = original.height;
                self.text_kind = original.kind;
                self.text_curve = original.curve;
            }
            TextTarget::New { .. } => self.text_draft.clear(),
        }
        self.text_editor = Some(TextEditor { pos, target, focused: false, rect: None });
    }

    /// Edit the text under the pointer in the Part view, if there is one.
    pub fn edit_text_at(&mut self, pos: Pos2, part: usize, idx: usize) {
        if let Some(t) = self.parts.get(part).and_then(|p| p.texts.get(idx)).cloned() {
            self.open_text_editor(pos, TextTarget::Existing { part, idx, original: t });
        }
    }

    /// The text being edited, for highlighting: (part, index).
    pub fn editing_text(&self) -> Option<(usize, usize)> {
        match self.text_editor.as_ref()?.target {
            TextTarget::Existing { part, idx, .. } => Some((part, idx)),
            _ => None,
        }
    }

    pub fn text_editor_ui(&mut self, ctx: &egui::Context) {
        let Some(ed) = self.text_editor.clone() else { return };
        let mut close = false;
        let mut commit = false;
        let mut delete = false;
        let mut cancel = false;
        let title = match &ed.target {
            TextTarget::New { on_element: false } => "New text",
            TextTarget::New { on_element: true } => "New text on the element",
            TextTarget::Existing { .. } => "Edit text",
        };
        let screen = ctx.content_rect();
        let size = Vec2::new(300.0, 250.0);
        let mut at = ed.pos + Vec2::new(14.0, 14.0);
        at.x = at.x.min(screen.right() - size.x - 8.0).max(screen.left() + 8.0);
        at.y = at.y.min(screen.bottom() - size.y - 8.0).max(screen.top() + 8.0);
        let mut focused = ed.focused;
        let area = egui::Area::new(Id::unique("quick_text_editor")).order(Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::new()
                .fill(th().panel)
                .stroke(Stroke::new(1.0, th().border))
                .corner_radius(CornerRadius::same(10))
                .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(70) })
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(size.x - 20.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).strong().color(th().text_strong));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Cancel (Esc)").clicked() {
                                cancel = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let te = ui.add(
                        egui::TextEdit::singleline(&mut self.text_draft)
                            .desired_width(f32::INFINITY)
                            .font(egui::FontId::proportional(17.0))
                            .hint_text("Type the text…"),
                    );
                    if !focused {
                        te.request_focus();
                        focused = true;
                    }
                    if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        commit = true;
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (k, name) in [(TextKind::Mark, "Marking"), (TextKind::Special, "Special"), (TextKind::Note, "Note")] {
                            if ui.add(egui::Button::selectable(self.text_kind == k, name)).on_hover_text(k.label()).clicked() {
                                self.text_kind = k;
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (tok, what) in TEXT_PARAMS {
                            if ui.small_button(tok).on_hover_text(format!("Insert the {what}")).clicked() {
                                self.text_draft.push_str(tok);
                                self.text_kind = TextKind::Special;
                                focused = false; // back into the box
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Height").size(11.5).color(th().text_dim));
                        ui.add(egui::DragValue::new(&mut self.text_height).range(2.0..=100.0).speed(0.2).suffix(" mm"));
                        ui.label(RichText::new("Arc").size(11.5).color(th().text_dim));
                        ui.add(egui::DragValue::new(&mut self.text_curve).range(-2000.0..=2000.0).speed(0.5).suffix(" r"))
                            .on_hover_text("0 = straight; positive bends over the top, negative round the bottom");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let ok = !self.text_draft.trim().is_empty();
                        let label = match &ed.target {
                            TextTarget::New { on_element: false } => "Place on part ↵",
                            TextTarget::New { on_element: true } => "Put on element ↵",
                            TextTarget::Existing { .. } => "Done ↵",
                        };
                        if ui.add_enabled(ok, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(th().accent)).clicked() {
                            commit = true;
                        }
                        if matches!(ed.target, TextTarget::Existing { .. }) && ui.button("Delete text").clicked() {
                            delete = true;
                        }
                    });
                });
        });
        let rect = area.response.rect;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        // Clicking outside: keep an edited text, drop an unplaced new one.
        let outside = ctx.input(|i| i.pointer.primary_pressed()) && ctx.pointer_latest_pos().is_some_and(|p| !rect.contains(p)) && ed.rect.is_some();
        if outside {
            if matches!(ed.target, TextTarget::Existing { .. }) {
                close = true;
            } else {
                cancel = true;
            }
        }
        if let Some(e) = self.text_editor.as_mut() {
            e.focused = focused;
            e.rect = Some(rect);
        }

        // Live update of an existing text.
        if let TextTarget::Existing { part, idx, original } = &ed.target {
            let mut changed = false;
            if let Some(t) = self.parts.get_mut(*part).and_then(|p| p.texts.get_mut(*idx)) {
                let new_t = if cancel {
                    original.clone()
                } else {
                    let base = PartText { text: self.text_draft.clone(), height: self.text_height, kind: self.text_kind, ..t.clone() };
                    with_curve(&base, self.text_curve)
                };
                if *t != new_t {
                    *t = new_t;
                    changed = true;
                }
            }
            if delete {
                if let Some(p) = self.parts.get_mut(*part) {
                    if *idx < p.texts.len() {
                        p.texts.remove(*idx);
                    }
                }
                self.status = "Text deleted.".into();
                close = true;
                changed = true;
            }
            if changed {
                self.refresh();
            }
        }
        if cancel {
            self.text_editor = None;
            if matches!(ed.target, TextTarget::Existing { .. }) {
                self.undo.pop(); // nothing changed
            }
            return;
        }
        if commit && !self.text_draft.trim().is_empty() {
            self.text_editor = None;
            match ed.target {
                TextTarget::New { on_element: false } => {
                    self.placing_text = true;
                    self.status = format!("Click on the part where '{}' should go (Esc to stop).", self.text_draft);
                }
                TextTarget::New { on_element: true } => self.text_on_element(),
                TextTarget::Existing { .. } => self.status = format!("Text changed to '{}'.", self.text_draft),
            }
            return;
        }
        if close {
            self.text_editor = None;
        }
    }

    /// Outline the text being edited (Part view).
    pub fn paint_edited_text(&self, painter: &egui::Painter, part: usize, to_s: &dyn Fn(P) -> Pos2) {
        let Some((p, idx)) = self.editing_text() else { return };
        if p != part {
            return;
        }
        let Some(t) = self.parts.get(p).and_then(|x| x.texts.get(idx)) else { return };
        let pts: Vec<Pos2> = t.strokes(&t.text).iter().flatten().map(|&q| to_s(q)).collect();
        if pts.is_empty() {
            return;
        }
        let r = Rect::from_points(&pts).expand(5.0);
        painter.rect_filled(r, CornerRadius::same(4), with_alpha(th().accent, 30));
        painter.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.5, th().accent), egui::StrokeKind::Outside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_a_text_between_straight_and_curved_keeps_its_middle() {
        let t = PartText { text: "ABC".into(), at: [10.0, 20.0], height: 6.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark };
        let w = crate::font::width("ABC", 6.0);
        let c = with_curve(&t, 40.0);
        assert_eq!((c.at, c.angle, c.curve), ([10.0 + w / 2.0, -20.0], 90.0, 40.0));
        let back = with_curve(&c, 0.0);
        assert!((back.at[0] - 10.0).abs() < 1e-9 && (back.at[1] - 20.0).abs() < 1e-9 && back.angle == 0.0);
        assert_eq!(with_curve(&c, 55.0).at, c.at);
    }

    #[test]
    fn picking_uses_the_turned_box_and_the_nearest_side() {
        // A text turned 90°: its box runs up from (50, 0).
        let up = PartText { text: "ABCD".into(), at: [50.0, 0.0], height: 6.0, angle: 90.0, curve: 0.0, kind: TextKind::Mark };
        let w = crate::font::width("ABCD", 6.0);
        assert_eq!(text_at(std::slice::from_ref(&up), [47.0, w / 2.0], 0.5), Some(0), "inside the turned box");
        assert_eq!(text_at(std::slice::from_ref(&up), [w / 2.0 + 50.0, 3.0], 0.5), None, "where an unturned box would be");
        // Two overlapping texts: the one whose box side is nearer wins.
        let a = PartText { text: "AAAAAA".into(), at: [0.0, 0.0], height: 10.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark };
        let b = PartText { text: "B".into(), at: [2.0, 2.0], height: 3.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark };
        assert_eq!(text_at(&[a.clone(), b.clone()], [2.5, 3.0], 0.5), Some(1));
        assert_eq!(text_at(&[b, a], [20.0, 9.8], 0.5), Some(1));
        // Window selection needs the whole box inside.
        assert!(text_in_window(&up, [40.0, -1.0], [60.0, w + 1.0]));
        assert!(!text_in_window(&up, [40.0, -1.0], [60.0, w / 2.0]));
    }

    #[test]
    fn a_text_is_found_under_the_pointer() {
        let t = PartText { text: "AB".into(), at: [10.0, 10.0], height: 6.0, angle: 0.0, curve: 0.0, kind: TextKind::Mark };
        let texts = vec![t];
        assert_eq!(text_at(&texts, [13.0, 12.0], 1.0), Some(0));
        assert_eq!(text_at(&texts, [50.0, 50.0], 1.0), None);
    }

    #[test]
    fn editing_an_existing_text_changes_it_and_escape_restores_it() {
        let mut a = App::new(None);
        let i = a.parts.iter().position(|p| p.name == "Square hole").unwrap();
        a.open_part(i);
        let before = a.parts[i].texts[0].clone();
        a.edit_text_at(Pos2::new(100.0, 100.0), i, 0);
        assert_eq!(a.text_draft, before.text);
        assert_eq!(a.editing_text(), Some((i, 0)));
        a.open_text_editor(Pos2::ZERO, TextTarget::New { on_element: false });
        assert!(a.text_draft.is_empty(), "a new text starts empty, ready to type");
    }
}
