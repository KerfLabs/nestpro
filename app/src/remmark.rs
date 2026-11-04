//! The automatic mark on remnants (Sheet Remnants → Automatic mark; rules
//! RF9–RF10): text engraved on each remnant so it can be told apart in the rack.
//!
//! - The text uses the original's parameters: `#300` reference, `#301` length,
//!   `#302` width, `#303` area, `#304` weight, `#200` sheet, `#201`–`#203` its
//!   user data, `#400` material, `#401` thickness, `#104` date, `#105` time;
//!   `#N.d` gives d decimals. Anything not available on a remnant prints `?`.
//! - Position 1–9 is the numeric keypad on the remnant's box (1 bottom-left,
//!   5 centre, 9 top-right) with X/Y margins; 0 searches for room on a grid,
//!   then tries smaller text down to the minimum height, else no mark.

use crate::nest::P;
use crate::stock::Outline;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkSettings {
    pub on: bool,
    pub text: String,
    /// Character height (mm) and the smallest it may shrink to when searching.
    pub height: f64,
    pub min_height: f64,
    /// 1–9 keypad position on the remnant's box; 0 = search for room.
    pub position: u8,
    pub margin_x: f64,
    pub margin_y: f64,
}

impl Default for MarkSettings {
    fn default() -> Self {
        // Estimated defaults (the original's weren't found).
        MarkSettings { on: false, text: "#300".into(), height: 10.0, min_height: 4.0, position: 0, margin_x: 20.0, margin_y: 20.0 }
    }
}

/// What the mark text can print about one remnant.
#[derive(Clone, Debug, Default)]
pub struct MarkValues {
    pub reference: String,
    pub length: f64,
    pub width: f64,
    pub area_m2: f64,
    pub weight_kg: f64,
    pub sheet: String,
    pub sheet_user_data: [String; 3],
    pub material: String,
    pub thickness: f64,
    pub date: String,
    pub time: String,
}

/// The mark text with its `#N` / `#N.d` parameters filled in.
pub fn expand_text(text: &str, v: &MarkValues) -> String {
    let mut out = String::new();
    let cs: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '#' && i + 1 < cs.len() && cs[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < cs.len() && cs[j].is_ascii_digit() {
                j += 1;
            }
            let n: u32 = cs[i + 1..j].iter().collect::<String>().parse().unwrap_or(0);
            let mut dec: Option<usize> = None;
            if j + 1 < cs.len() && cs[j] == '.' && cs[j + 1].is_ascii_digit() {
                dec = cs[j + 1].to_digit(10).map(|d| d as usize);
                j += 2;
            }
            let num = |x: f64, d0: usize| format!("{:.d$}", x, d = dec.unwrap_or(d0));
            let s = match n {
                300 => v.reference.clone(),
                301 => num(v.length, 0),
                302 => num(v.width, 0),
                303 => num(v.area_m2, 3),
                304 => num(v.weight_kg, 1),
                200 => v.sheet.clone(),
                201..=203 => v.sheet_user_data[(n - 201) as usize].clone(),
                400 => v.material.clone(),
                401 => num(v.thickness, 1),
                104 => v.date.clone(),
                105 => v.time.clone(),
                _ => "?".into(),
            };
            out.push_str(if s.is_empty() { "?" } else { &s });
            i = j;
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

/// The mark's strokes on a remnant (sheet coordinates), or None when there's
/// no room for it.
pub fn place(piece: &Outline, text: &str, s: &MarkSettings) -> Option<Vec<Vec<P>>> {
    if text.trim().is_empty() {
        return None;
    }
    let (x0, y0, x1, y1) = piece.outer.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1])));
    let fits = |x: f64, y: f64, w: f64, h: f64| crate::region::box_inside(piece, 0.0, x, y, w, h);
    let at = |pos: u8, w: f64, h: f64| -> (f64, f64) {
        let col = (pos - 1) % 3;
        let row = (pos - 1) / 3;
        let x = match col {
            0 => x0 + s.margin_x,
            1 => (x0 + x1 - w) / 2.0,
            _ => x1 - s.margin_x - w,
        };
        let y = match row {
            0 => y0 + s.margin_y,
            1 => (y0 + y1 - h) / 2.0,
            _ => y1 - s.margin_y - h,
        };
        (x, y)
    };
    if (1..=9).contains(&s.position) {
        let h = s.height;
        let w = crate::font::width(text, h);
        let (x, y) = at(s.position, w, h);
        return Some(crate::font::strokes(text, [x, y], h, 0.0));
    }
    // Search: keypad spots first (centre, then the corners), then a grid, at
    // full height and smaller, down to the minimum.
    let mut h = s.height.max(0.5);
    let step = (s.margin_x.max(s.margin_y)).max(10.0);
    while h >= s.min_height.max(0.5) - 1e-9 {
        let w = crate::font::width(text, h);
        // Level first, then turned 90° (reading upward) — its footprint is h × w
        // and the text starts at the footprint's lower-right corner.
        for turned in [false, true] {
            let (fw, fh) = if turned { (h, w) } else { (w, h) };
            let mark_at = |x: f64, y: f64| if turned { crate::font::strokes(text, [x + h, y], h, 90.0) } else { crate::font::strokes(text, [x, y], h, 0.0) };
            for pos in [5u8, 1, 3, 7, 9, 2, 4, 6, 8] {
                let (x, y) = at(pos, fw, fh);
                if fits(x, y, fw, fh) {
                    return Some(mark_at(x, y));
                }
            }
            let mut y = y0 + s.margin_y;
            while y + fh <= y1 - s.margin_y {
                let mut x = x0 + s.margin_x;
                while x + fw <= x1 - s.margin_x {
                    if fits(x, y, fw, fh) {
                        return Some(mark_at(x, y));
                    }
                    x += step;
                }
                y += step;
            }
        }
        h *= 0.8;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_text_parameters() {
        let v = MarkValues { reference: "2-16".into(), length: 1200.4, width: 800.0, area_m2: 0.8153, thickness: 10.0, material: "S235".into(), ..Default::default() };
        assert_eq!(expand_text("#300 #301x#302 #303.2 m2", &v), "2-16 1200x800 0.82 m2");
        assert_eq!(expand_text("#400 #401.0 #5 #200", &v), "S235 10 ? ?");
        assert_eq!(expand_text("no params", &v), "no params");
    }

    #[test]
    fn marks_go_at_the_keypad_spot_or_where_there_is_room() {
        let l = Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [600.0, 1000.0], [600.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let s = MarkSettings { on: true, position: 1, ..Default::default() };
        let st = place(&l, "2-16", &s).unwrap();
        let minx = st.iter().flatten().map(|p| p[0]).fold(f64::MAX, f64::min);
        assert!((minx - 20.0).abs() < 2.0, "{minx}");
        // Searching: the centre of the L's box is outside it, so it goes elsewhere, inside.
        let s = MarkSettings { on: true, position: 0, ..Default::default() };
        let st = place(&l, "2-16", &s).unwrap();
        let pts: Vec<P> = st.into_iter().flatten().collect();
        let (x0, y0) = (pts.iter().map(|p| p[0]).fold(f64::MAX, f64::min), pts.iter().map(|p| p[1]).fold(f64::MAX, f64::min));
        let (x1, y1) = (pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max), pts.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
        assert!(crate::region::box_inside(&l, 0.0, x0, y0, x1 - x0, y1 - y0));
        // A tall narrow strip: the text is turned to read upward at full height.
        let strip = Outline { outer: vec![[0.0, 0.0], [120.0, 0.0], [120.0, 1500.0], [0.0, 1500.0]], holes: vec![] };
        let st = place(&strip, "REMNANT 2-16 1500x120", &s).unwrap();
        let pts: Vec<P> = st.into_iter().flatten().collect();
        let tall = pts.iter().map(|p| p[1]).fold(f64::MIN, f64::max) - pts.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        assert!(tall > 150.0, "reads upward along the strip: {tall}");
        // No room at all: no mark.
        let tiny = Outline { outer: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]], holes: vec![] };
        assert!(place(&tiny, "LONG REFERENCE", &s).is_none());
    }
}
