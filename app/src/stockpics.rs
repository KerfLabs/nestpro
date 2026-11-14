//! Pictures of sheet stock for the Sheet stock window: a swatch for each
//! material and a to-scale drawing of a sheet or remnant in its material's look.
//! Drawn, not photos: mild steel blue-grey with mill scale, stainless brushed,
//! aluminium light and brushed.

use crate::engine::material::{kind, Kind};
use crate::stock::Item;
use crate::theme::th;
use eframe::egui;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Base, light and dark tone of a material, by its kind.
fn tones(material: usize) -> (Color32, Color32, Color32) {
    let c = Color32::from_rgb;
    match kind(material) {
        Kind::MildSteel => (c(0x4a, 0x53, 0x5e), c(0x66, 0x71, 0x7d), c(0x33, 0x39, 0x41)),
        Kind::Stainless => (c(0xbd, 0xc3, 0xc9), c(0xe2, 0xe6, 0xe9), c(0x98, 0x9f, 0xa6)),
        Kind::Aluminium => (c(0xdd, 0xe1, 0xe5), c(0xf6, 0xf7, 0xf8), c(0xc4, 0xc9, 0xce)),
    }
}

/// The edge drawn around a remnant: amber, so remnants stand out from sheets.
pub fn remnant_edge() -> Color32 {
    if th().dark { Color32::from_rgb(232, 184, 96) } else { Color32::from_rgb(190, 125, 20) }
}

fn mix(a: Color32, b: Color32, f: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// A small repeatable hash, so the texture doesn't flicker between frames.
fn hash(n: i64) -> f32 {
    let mut x = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x & 0xFFFF) as f32 / 65535.0
}

/// The colour of one pixel row of the material, `k` rows from the top.
/// Stainless and aluminium are brushed along their length, so row by row
/// streaks give the grain; mild steel gets soft bands.
fn row_colour(material: usize, k: i64, seed: i64) -> Color32 {
    let (base, light, dark) = tones(material);
    let n = hash(k + seed * 7919);
    match kind(material) {
        Kind::MildSteel => {
            let wave = ((k as f32 + seed as f32) * 0.21).sin() * 0.5 + 0.5;
            if wave > 0.5 { mix(base, light, (wave - 0.5) * 0.9 + n * 0.08) } else { mix(base, dark, (0.5 - wave) * 0.9 + n * 0.08) }
        }
        _ => {
            if n > 0.5 { mix(base, light, (n - 0.5) * 1.6) } else { mix(base, dark, (0.5 - n) * 1.2) }
        }
    }
}

/// Fill closed rings (outer and holes, even-odd) one pixel row at a time
/// with the material's texture.
fn fill_rings(p: &Painter, rings: &[Vec<Pos2>], material: usize, seed: i64) {
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    for q in rings.iter().flatten() {
        y0 = y0.min(q.y);
        y1 = y1.max(q.y);
    }
    if y0 >= y1 {
        return;
    }
    let mut xs: Vec<f32> = Vec::new();
    let mut y = y0;
    let mut k = 0i64;
    while y < y1 {
        let mid = y + 0.5;
        xs.clear();
        for r in rings {
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                if (a.y <= mid) != (b.y <= mid) {
                    xs.push(a.x + (mid - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f32::total_cmp);
        let c = row_colour(material, k, seed);
        for s in xs.chunks_exact(2) {
            p.rect_filled(Rect::from_min_max(pos2(s[0], y), pos2(s[1], (y + 1.0).min(y1))), 0.0, c);
        }
        y += 1.0;
        k += 1;
    }
}

/// Mill scale on mild steel: a few soft darker and lighter patches.
fn mill_scale(p: &Painter, rect: Rect, seed: i64) {
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let (_, light, dark) = tones(0);
    let n = ((rect.width() * rect.height()) / 300.0).clamp(3.0, 30.0) as i64;
    for i in 0..n {
        let x = rect.left() + hash(seed * 31 + i * 3) * rect.width();
        let y = rect.top() + hash(seed * 31 + i * 3 + 1) * rect.height();
        let r = (rect.height().min(rect.width()) * (0.06 + 0.14 * hash(seed * 31 + i * 3 + 2))).max(1.5);
        let c = if i % 2 == 0 { dark } else { light };
        p.circle_filled(pos2(x, y), r, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 90));
    }
}

/// Fill `rect` with the material's look.
pub fn paint_swatch(p: &Painter, rect: Rect, material: usize) {
    let ring = vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    fill_rings(p, &[ring], material, 1);
    if kind(material) == Kind::MildSteel {
        mill_scale(p, rect, 1);
    }
    p.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Inside);
}

/// A material swatch of the given size.
pub fn swatch(ui: &mut Ui, material: usize, size: Vec2) -> Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::hover());
    paint_swatch(ui.painter(), rect, material);
    r
}

/// A swatch for every material at once (the "All materials" row).
pub fn paint_all_swatch(p: &Painter, rect: Rect) {
    let ids = crate::engine::material::ids();
    let n = ids.len();
    for (k, m) in ids.into_iter().enumerate() {
        let w = rect.width() / n as f32;
        let r = Rect::from_min_size(pos2(rect.left() + w * k as f32, rect.top()), vec2(w, rect.height()));
        let ring = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        fill_rings(p, &[ring], m, 1);
    }
    p.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Inside);
}

/// Points per mm that fit every item into `size` (to the same scale).
pub fn common_scale<'a>(items: impl IntoIterator<Item = &'a Item>, size: Vec2) -> f32 {
    let (mut l, mut w) = (1.0f64, 1.0f64);
    for it in items {
        l = l.max(it.length);
        w = w.max(it.width);
    }
    ((size.x as f64 / l).min(size.y as f64 / w)) as f32
}

/// Seed for an item's texture, from its reference.
fn seed_of(it: &Item) -> i64 {
    it.reference.bytes().fold(7i64, |h, b| h.wrapping_mul(31).wrapping_add(b as i64)).rem_euclid(10_000)
}

/// Draw the item to `scale` (points per mm) with its lower-left corner at
/// `origin` (Y up, as on the sheet): its real shape for a remnant with one,
/// otherwise its rectangle. Returns the drawing's screen box.
pub fn paint_item(p: &Painter, origin: Pos2, scale: f32, it: &Item) -> Rect {
    let to = |q: [f64; 2]| pos2(origin.x + q[0] as f32 * scale, origin.y - q[1] as f32 * scale);
    let rings: Vec<Vec<Pos2>> = match &it.outline {
        Some(o) if o.outer.len() >= 3 => std::iter::once(&o.outer).chain(o.holes.iter()).map(|r| r.iter().map(|&q| to(q)).collect()).collect(),
        _ => vec![vec![to([0.0, 0.0]), to([it.length, 0.0]), to([it.length, it.width]), to([0.0, it.width])]],
    };
    let bx = Rect::from_two_pos(to([0.0, 0.0]), to([it.length, it.width]));
    let seed = seed_of(it);
    fill_rings(p, &rings, it.material, seed);
    if kind(it.material) == Kind::MildSteel && it.outline.is_none() {
        mill_scale(p, bx, seed);
    }
    let edge = if it.kind.is_remnant() { Stroke::new(1.2, remnant_edge()) } else { Stroke::new(1.0, Color32::from_black_alpha(140)) };
    for r in &rings {
        let mut pts = r.clone();
        pts.extend(r.first().copied());
        p.add(egui::Shape::line(pts, edge));
    }
    bx
}

/// The item drawn to fit `size`, centred, with Sense::click.
pub fn item_fit(ui: &mut Ui, it: &Item, size: Vec2) -> Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let s = common_scale([it], size - vec2(2.0, 2.0));
    let d = vec2(it.length as f32 * s, it.width as f32 * s);
    let origin = pos2(rect.center().x - d.x / 2.0, rect.center().y + d.y / 2.0);
    paint_item(&ui.painter_at(rect.expand(1.0)), origin, s, it);
    r
}

/// The item drawn to a shared `scale` in a cell of `size`, lower-left aligned
/// so sizes compare at a glance, with Sense::click.
pub fn item_scaled(ui: &mut Ui, it: &Item, size: Vec2, scale: f32) -> Response {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let h = it.width as f32 * scale;
    let origin = pos2(rect.left() + 1.0, rect.center().y + h / 2.0);
    paint_item(&ui.painter_at(rect.expand(1.0)), origin, scale, it);
    r
}

/// The item to fit `size` with its length and width written along the
/// bottom and left edges (the New sheet preview and the picked item).
pub fn item_dimensioned(ui: &mut Ui, it: &Item, size: Vec2, u: crate::units::LenUnit) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    let inner = Rect::from_min_max(rect.min + vec2(18.0, 4.0), rect.max - vec2(4.0, 18.0));
    let s = common_scale([it], inner.size());
    let d = vec2(it.length as f32 * s, it.width as f32 * s);
    let origin = pos2(inner.center().x - d.x / 2.0, inner.center().y + d.y / 2.0);
    let bx = paint_item(&p, origin, s, it);
    let dim = th().text_dim;
    let font = egui::FontId::proportional(11.0);
    let y = bx.bottom() + 8.0;
    p.line_segment([pos2(bx.left(), y), pos2(bx.right(), y)], Stroke::new(1.0, dim));
    for x in [bx.left(), bx.right()] {
        p.line_segment([pos2(x, y - 3.0), pos2(x, y + 3.0)], Stroke::new(1.0, dim));
    }
    p.text(pos2(bx.center().x, y + 1.0), egui::Align2::CENTER_TOP, u.len(it.length, 0), font.clone(), dim);
    let x = bx.left() - 8.0;
    p.line_segment([pos2(x, bx.top()), pos2(x, bx.bottom())], Stroke::new(1.0, dim));
    for yy in [bx.top(), bx.bottom()] {
        p.line_segment([pos2(x - 3.0, yy), pos2(x + 3.0, yy)], Stroke::new(1.0, dim));
    }
    let g = ui.painter().layout_no_wrap(u.len(it.width, 0), font, dim);
    let at = pos2(x - 2.0 - g.size().y, bx.center().y + g.size().x / 2.0);
    p.add(egui::epaint::TextShape::new(at, g, dim).with_angle(-std::f32::consts::FRAC_PI_2));
}

/// A dashed rectangle (the "New sheet" card).
pub fn dashed_rect(p: &Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(egui::Shape::dashed_line(&pts, stroke, 5.0, 4.0));
}
