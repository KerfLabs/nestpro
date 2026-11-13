//! Technology → Lead-ins → Type: the lead shape picker. Each type is shown
//! with a picture of the lead on a part's edge and a short hint; the menu's
//! right side shows the type under the mouse large, with what each colour is
//! and one plain sentence on when to use it (our own design).

use crate::nest::LeadType;
use crate::theme::th;
use crate::ui_parts::with_alpha;
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};

/// The colours the part screen draws leads in (lead-in, lead-out, pierce).
const LEAD_IN: Color32 = Color32::WHITE;
const LEAD_OUT: Color32 = Color32::from_rgb(129, 140, 248);
const PIERCE: Color32 = Color32::from_rgb(248, 80, 90);
const EDGE: Color32 = Color32::from_rgb(150, 156, 172);
const PART: Color32 = Color32::from_rgb(62, 68, 90);

/// A few words under the type's name in the list.
pub fn hint(t: LeadType) -> &'static str {
    match t {
        LeadType::Machine => "Per contour, from the machine's tables",
        LeadType::Perpendicular => "Straight in at 90°",
        LeadType::Line => "Straight in at 45°",
        LeadType::Arc => "Curves in along the edge",
        LeadType::LineArc => "Straight, then curves in",
        LeadType::Corner => "Starts on a corner",
    }
}

/// One plain sentence on what the type does, in the preview.
pub fn sentence(t: LeadType) -> &'static str {
    match t {
        LeadType::Machine => "Each contour gets the lead its machine table gives for its class (outline, large or small hole) and cut quality: shape, lengths and angles.",
        LeadType::Perpendicular => "Comes straight in at 90° to the edge and leaves the same way. The shortest lead, so it fits narrow gaps between parts.",
        LeadType::Line => "Comes in at 45° on the scrap side and leaves at 45°. A good all-round lead.",
        LeadType::Arc => "Curves onto the edge so the cut starts moving along it, then leaves at 45°. The smoothest start on the edge.",
        LeadType::LineArc => "A straight run from the pierce, then a quarter arc onto the edge; it leaves the same way. Keeps the pierce clear of the part.",
        LeadType::Corner => "Starts on a corner, in line with one of its sides, so no flat edge gets a start mark.",
    }
}

/// Points on an arc of the 40 × 30 picture: centre `c`, radius `r`, from
/// angle `a0` to `a1` in degrees (y down).
fn arc(c: [f32; 2], r: f32, a0: f32, a1: f32) -> Vec<[f32; 2]> {
    (0..=12)
        .map(|k| {
            let a = (a0 + (a1 - a0) * k as f32 / 12.0).to_radians();
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// The type's picture in `r`: a piece of a part (shaded) with its edge, the
/// lead-in from its pierce and the lead-out, drawn on the drawing's dark
/// background in the part screen's lead colours. `big` draws finer lines.
pub fn paint_icon(p: &egui::Painter, r: Rect, t: LeadType, big: bool) {
    p.rect_filled(r, CornerRadius::same(4), crate::drawing::rgb(crate::drawing::C_BACKGROUND));
    // The picture is drawn on a 40 × 30 grid, kept in proportion.
    let s = (r.width() / 40.0).min(r.height() / 30.0);
    let o = r.center() - Vec2::new(20.0 * s, 15.0 * s);
    let at = |q: [f32; 2]| Pos2::new(o.x + q[0] * s, o.y + q[1] * s);
    let w = if big { 2.2 } else { 1.6 };
    let line = |pts: Vec<[f32; 2]>, c: Color32| Shape::line(pts.into_iter().map(at).collect(), Stroke::new(w, c));
    let dot = if big { 4.0 } else { 2.2 };
    let part = |pts: &[[f32; 2]]| Shape::convex_polygon(pts.iter().map(|&q| at(q)).collect(), PART, Stroke::NONE);
    let (lead_in, lead_out, pierce): (Vec<[f32; 2]>, Vec<[f32; 2]>, [f32; 2]) = match t {
        LeadType::Machine => {
            // A machine table with a lead coming out of it.
            let (a, b) = (at([8.0, 5.0]), at([32.0, 25.0]));
            p.rect_stroke(Rect::from_two_pos(a, b), CornerRadius::same(2), Stroke::new(w * 0.7, EDGE), egui::StrokeKind::Middle);
            p.line_segment([at([8.0, 10.5]), at([32.0, 10.5])], Stroke::new(w * 0.7, EDGE));
            p.line_segment([at([8.0, 17.5]), at([32.0, 17.5])], Stroke::new(w * 0.5, with_alpha(EDGE, 140)));
            p.line_segment([at([16.0, 5.0]), at([16.0, 25.0])], Stroke::new(w * 0.5, with_alpha(EDGE, 140)));
            let mut l = vec![[21.0, 21.5], [21.0, 17.5]];
            l.extend(arc([25.0, 17.5], 4.0, 180.0, 270.0));
            (l, vec![], [21.0, 21.5])
        }
        LeadType::Corner => {
            // The part's corner: the lead runs down the extension of the side
            // it cuts first; the lead-out carries on past the corner along
            // the last side.
            p.add(part(&[[2.0, 20.0], [26.0, 20.0], [26.0, 29.0], [2.0, 29.0]]));
            p.add(line(vec![[2.0, 20.0], [26.0, 20.0], [26.0, 29.0]], EDGE));
            (vec![[26.0, 6.0], [26.0, 20.0]], vec![[26.0, 20.0], [36.0, 20.0]], [26.0, 6.0])
        }
        _ => {
            p.add(part(&[[2.0, 20.0], [38.0, 20.0], [38.0, 29.0], [2.0, 29.0]]));
            p.add(line(vec![[2.0, 20.0], [38.0, 20.0]], EDGE));
            match t {
                LeadType::Perpendicular => (vec![[20.0, 6.0], [20.0, 20.0]], vec![[21.6, 20.0], [21.6, 13.0]], [20.0, 6.0]),
                LeadType::Line => (vec![[8.0, 8.0], [20.0, 20.0]], vec![[20.0, 20.0], [26.0, 14.0]], [8.0, 8.0]),
                LeadType::Arc => (arc([20.0, 10.0], 10.0, 180.0, 90.0), vec![[20.0, 20.0], [26.0, 14.0]], [10.0, 10.0]),
                _ => {
                    let mut i = vec![[10.0, 3.0]];
                    i.extend(arc([20.0, 10.0], 10.0, 180.0, 90.0));
                    let mut out = arc([20.0, 15.0], 5.0, 90.0, 0.0);
                    out.push([25.0, 11.0]);
                    (i, out, [10.0, 3.0])
                }
            }
        }
    };
    if !lead_out.is_empty() {
        p.add(line(lead_out, LEAD_OUT));
    }
    p.add(line(lead_in, LEAD_IN));
    p.circle_filled(at(pierce), dot, PIERCE);
}

/// The Type box: the chosen type's picture and name, opening the picker.
/// `demo` (screenshots) shows the picker open as if the mouse were on that
/// type. Returns true when a type was picked.
pub fn picker(ui: &mut egui::Ui, choice: &mut LeadType, demo: Option<LeadType>) -> bool {
    let t = th();
    let galley = ui.painter().layout_no_wrap(choice.label().to_string(), egui::FontId::proportional(13.0), t.text);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 58.0, 26.0), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() { t.hover } else { t.field };
    p.rect(rect, CornerRadius::same(5), fill, Stroke::new(1.0, t.control_edge), egui::StrokeKind::Inside);
    paint_icon(p, Rect::from_min_size(rect.min + Vec2::new(4.0, 3.0), Vec2::new(27.0, 20.0)), *choice, false);
    p.galley(Pos2::new(rect.left() + 36.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    let a = Pos2::new(rect.right() - 11.0, rect.center().y);
    p.add(Shape::convex_polygon(vec![a + Vec2::new(-4.0, -2.0), a + Vec2::new(4.0, -2.0), a + Vec2::new(0.0, 3.0)], t.text_dim, Stroke::NONE));
    let resp = resp.on_hover_text("The lead shape Auto lead-ins puts on every contour. Click to see each one.");
    let mut picked = None;
    egui::Popup::menu(&resp).show(|ui| picked = menu(ui, *choice, None));
    if let Some(h) = demo {
        egui::Area::new(egui::Id::unique("demo_lead_menu")).order(egui::Order::Foreground).fixed_pos(rect.left_bottom() + Vec2::new(0.0, 4.0)).show(ui.ctx(), |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| menu(ui, *choice, Some(h)));
        });
    }
    match picked {
        Some(n) if n != *choice => {
            *choice = n;
            true
        }
        _ => false,
    }
}

/// The open picker: the list on the left (the machine's tables first, then
/// the fixed shapes), the type under the mouse — or the chosen one — large
/// on the right. `hover` forces the previewed type (screenshots).
fn menu(ui: &mut egui::Ui, choice: LeadType, hover: Option<LeadType>) -> Option<LeadType> {
    let t = th();
    let mut picked = None;
    let mut shown = hover.unwrap_or(choice);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(272.0);
            for (k, lt) in LeadType::ALL.into_iter().enumerate() {
                if k == 1 {
                    ui.add_space(2.0);
                    ui.separator();
                    ui.label(RichText::new("Same shape on every contour").size(11.0).color(t.text_dim));
                }
                let r = row(ui, lt, lt == choice, hover == Some(lt));
                if r.hovered() && hover.is_none() {
                    shown = lt;
                }
                if r.clicked() {
                    picked = Some(lt);
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(236.0);
            ui.label(RichText::new(shown.label()).strong().size(13.5).color(t.text_strong));
            ui.add_space(2.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(236.0, 150.0), Sense::hover());
            paint_icon(ui.painter(), r, shown, true);
            if shown != LeadType::Machine {
                ui.horizontal(|ui| {
                    for (name, c) in [("pierce", PIERCE), ("lead-in", LEAD_IN), ("lead-out", LEAD_OUT), ("part", EDGE)] {
                        // On the drawing's background, as in the picture (the
                        // white lead-in would vanish on a light menu).
                        let (r, _) = ui.allocate_exact_size(Vec2::new(16.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(3), crate::drawing::rgb(crate::drawing::C_BACKGROUND));
                        ui.painter().rect_filled(Rect::from_center_size(r.center(), Vec2::new(10.0, 3.0)), CornerRadius::same(1), c);
                        ui.label(RichText::new(name).size(11.0).color(t.text_dim));
                    }
                });
            }
            ui.add_space(2.0);
            ui.label(RichText::new(sentence(shown)).size(12.5).color(t.text));
        });
    });
    picked
}

/// One row of the list: the picture, the name (the machine's tables marked
/// as the usual choice) and its hint; the chosen type is filled.
fn row(ui: &mut egui::Ui, lt: LeadType, chosen: bool, lit: bool) -> egui::Response {
    let t = th();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(272.0, 40.0), Sense::click());
    let p = ui.painter();
    if chosen {
        p.rect(rect, CornerRadius::same(5), t.accent_soft, Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
    } else if resp.hovered() || lit {
        p.rect_filled(rect, CornerRadius::same(5), t.hover);
    }
    paint_icon(p, Rect::from_min_size(rect.min + Vec2::new(5.0, 4.0), Vec2::new(43.0, 32.0)), lt, false);
    let x = rect.left() + 56.0;
    let name = p.layout_no_wrap(lt.label().to_string(), egui::FontId::proportional(13.0), t.text_strong);
    let nw = name.size().x;
    p.galley(Pos2::new(x, rect.top() + 4.0), name, t.text_strong);
    if lt == LeadType::Machine {
        let tag = p.layout_no_wrap("usual".into(), egui::FontId::proportional(10.0), t.ok_fg);
        let tr = Rect::from_min_size(Pos2::new(x + nw + 7.0, rect.top() + 5.0), tag.size() + Vec2::new(8.0, 2.0));
        p.rect(tr, CornerRadius::same(3), t.ok_bg, Stroke::NONE, egui::StrokeKind::Inside);
        p.galley(tr.min + Vec2::new(4.0, 1.0), tag, t.ok_fg);
    }
    p.galley(Pos2::new(x, rect.top() + 22.0), p.layout_no_wrap(hint(lt).into(), egui::FontId::proportional(11.5), t.text_dim), t.text_dim);
    resp
}

/// A type by the name the screenshots flag gives (the operations' keys:
/// machine, perpendicular, line, arc, line-arc, corner).
pub fn named(s: &str) -> Option<LeadType> {
    LeadType::ALL.into_iter().find(|&t| {
        let key = match t {
            LeadType::Machine => "machine",
            LeadType::Perpendicular => "perpendicular",
            LeadType::Line => "line",
            LeadType::Arc => "arc",
            LeadType::LineArc => "line-arc",
            LeadType::Corner => "corner",
        };
        key == s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_words_and_a_name() {
        for t in LeadType::ALL {
            assert!(!hint(t).is_empty() && !sentence(t).is_empty());
        }
        assert_eq!(named("line-arc"), Some(LeadType::LineArc));
        assert_eq!(named("nope"), None);
    }
}
