//! A single-stroke marking font. Like the original's marking fonts (one file per
//! character in `Mrkfonts`, lines on a small grid ending in `999`), every glyph
//! is a few straight strokes on a grid — 4 wide, 6 tall here — so a laser can
//! mark it as lines instead of cutting outlines. The glyphs are the rebuild's own.

use crate::nest::P;

/// Grid height of a capital letter; text height maps onto it.
pub const GRID_H: f64 = 6.0;
/// Advance from one character to the next, in grid units (glyph 4 + gap 2).
pub const ADVANCE: f64 = 6.0;

fn glyph(c: char) -> &'static str {
    match c.to_ascii_uppercase() {
        'A' => "0,0 0,4 2,6 4,4 4,0;0,3 4,3",
        'B' => "0,0 0,6 3,6 4,5 4,4 3,3 0,3;3,3 4,2 4,1 3,0 0,0",
        'C' => "4,5 3,6 1,6 0,5 0,1 1,0 3,0 4,1",
        'D' => "0,0 0,6 3,6 4,5 4,1 3,0 0,0",
        'E' => "4,0 0,0 0,6 4,6;0,3 3,3",
        'F' => "0,0 0,6 4,6;0,3 3,3",
        'G' => "4,5 3,6 1,6 0,5 0,1 1,0 3,0 4,1 4,3 2,3",
        'H' => "0,0 0,6;4,0 4,6;0,3 4,3",
        'I' => "1,0 3,0;2,0 2,6;1,6 3,6",
        'J' => "0,1 1,0 3,0 4,1 4,6",
        'K' => "0,0 0,6;4,6 0,2;1,3 4,0",
        'L' => "0,6 0,0 4,0",
        'M' => "0,0 0,6 2,3 4,6 4,0",
        'N' => "0,0 0,6 4,0 4,6",
        'O' => "1,0 0,1 0,5 1,6 3,6 4,5 4,1 3,0 1,0",
        'P' => "0,0 0,6 3,6 4,5 4,4 3,3 0,3",
        'Q' => "1,0 0,1 0,5 1,6 3,6 4,5 4,1 3,0 1,0;2,2 4,0",
        'R' => "0,0 0,6 3,6 4,5 4,4 3,3 0,3;2,3 4,0",
        'S' => "4,5 3,6 1,6 0,5 0,4 1,3 3,3 4,2 4,1 3,0 1,0 0,1",
        'T' => "0,6 4,6;2,6 2,0",
        'U' => "0,6 0,1 1,0 3,0 4,1 4,6",
        'V' => "0,6 2,0 4,6",
        'W' => "0,6 1,0 2,3 3,0 4,6",
        'X' => "0,0 4,6;0,6 4,0",
        'Y' => "0,6 2,3 4,6;2,3 2,0",
        'Z' => "0,6 4,6 0,0 4,0",
        '0' => "1,0 0,1 0,5 1,6 3,6 4,5 4,1 3,0 1,0;0,1 4,5",
        '1' => "1,5 2,6 2,0;1,0 3,0",
        '2' => "0,5 1,6 3,6 4,5 4,4 0,0 4,0",
        '3' => "0,5 1,6 3,6 4,5 4,4 3,3 4,2 4,1 3,0 1,0 0,1;1,3 3,3",
        '4' => "3,0 3,6 0,2 4,2",
        '5' => "4,6 0,6 0,3 3,3 4,2 4,1 3,0 1,0 0,1",
        '6' => "4,5 3,6 1,6 0,5 0,1 1,0 3,0 4,1 4,2 3,3 0,3",
        '7' => "0,6 4,6 1,0",
        '8' => "1,3 0,4 0,5 1,6 3,6 4,5 4,4 3,3 1,3 0,2 0,1 1,0 3,0 4,1 4,2 3,3",
        '9' => "0,1 1,0 3,0 4,1 4,5 3,6 1,6 0,5 0,4 1,3 4,3",
        '-' => "1,3 3,3",
        '+' => "2,1 2,5;0,3 4,3",
        '=' => "0,2 4,2;0,4 4,4",
        '.' => "2,0 2,0.5",
        ',' => "2,0.5 1.5,-1",
        ':' => "2,1 2,1.5;2,4 2,4.5",
        '/' => "0,0 4,6",
        '#' => "1,0 1,6;3,0 3,6;0,2 4,2;0,4 4,4",
        '_' => "0,0 4,0",
        '(' => "3,6 2,4 2,2 3,0",
        ')' => "1,6 2,4 2,2 1,0",
        '*' => "2,1 2,5;0.5,2 3.5,4;0.5,4 3.5,2",
        // Anything unknown: a small box, so it's visible that a character is missing.
        ' ' => "",
        _ => "0,0 4,0 4,6 0,6 0,0",
    }
}

fn parse(s: &str) -> Vec<Vec<P>> {
    s.split(';')
        .filter(|g| !g.trim().is_empty())
        .map(|g| {
            g.split_whitespace()
                .filter_map(|xy| {
                    let mut it = xy.split(',');
                    Some([it.next()?.parse().ok()?, it.next()?.parse().ok()?])
                })
                .collect()
        })
        .collect()
}

/// Strokes of `text` in part coordinates: `at` is the lower-left of the first
/// character, `height` the capital height in mm, `angle` in degrees (counter-clockwise).
pub fn strokes(text: &str, at: P, height: f64, angle: f64) -> Vec<Vec<P>> {
    let s = height / GRID_H;
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut out = vec![];
    for (k, c) in text.chars().enumerate() {
        let x0 = k as f64 * ADVANCE;
        for stroke in parse(glyph(c)) {
            let pts: Vec<P> = stroke
                .iter()
                .map(|g| {
                    let (lx, ly) = ((x0 + g[0]) * s, g[1] * s);
                    [at[0] + lx * cos - ly * sin, at[1] + lx * sin + ly * cos]
                })
                .collect();
            if pts.len() >= 2 {
                out.push(pts);
            }
        }
    }
    out
}

/// Strokes of `text` bent along an arc around `center`. `radius` > 0: the baseline
/// is at that radius, letters stand outward and the text reads clockwise (the top
/// of a circle). `radius` < 0: the baseline is at |radius|, letters stand inward
/// and it reads anticlockwise (the bottom of a circle). `mid_deg` is where the
/// middle of the text sits, seen from the centre.
pub fn strokes_on_arc(text: &str, center: P, radius: f64, mid_deg: f64, height: f64) -> Vec<Vec<P>> {
    let r = radius.abs().max(1e-6);
    let w = width(text, height);
    let mid = mid_deg.to_radians();
    // Lay the text out straight from (0, 0), split long strokes so they bend.
    let straight = strokes(text, [0.0, 0.0], height, 0.0);
    let max_seg = (height / 6.0).max(0.2);
    straight
        .into_iter()
        .map(|s| {
            let mut fine = vec![s[0]];
            for pair in s.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let n = (len / max_seg).ceil().max(1.0) as usize;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    fine.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                }
            }
            fine.into_iter()
                .map(|[x, y]| {
                    let (theta, rr) = if radius > 0.0 { (mid - (x - w / 2.0) / r, r + y) } else { (mid + (x - w / 2.0) / r, r - y) };
                    [center[0] + rr * theta.cos(), center[1] + rr * theta.sin()]
                })
                .collect()
        })
        .collect()
}

/// Width of `text` at `height` (without the gap after the last character).
pub fn width(text: &str, height: f64) -> f64 {
    let n = text.chars().count() as f64;
    if n == 0.0 {
        0.0
    } else {
        (n * ADVANCE - 2.0) * height / GRID_H
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_scale_to_the_height_and_advance_per_character() {
        let st = strokes("T", [10.0, 20.0], 12.0, 0.0);
        assert_eq!(st.len(), 2);
        // Top bar of the T at the full height.
        assert_eq!(st[0], vec![[10.0, 32.0], [18.0, 32.0]]);
        assert!((width("AB", 6.0) - 10.0).abs() < 1e-9);
        let two = strokes("11", [0.0, 0.0], 6.0, 0.0);
        assert!(two.iter().any(|s| s.iter().all(|p| p[0] >= 6.0)));
        // Rotated 90°: the text runs upwards.
        let up = strokes("-", [0.0, 0.0], 6.0, 90.0);
        assert!(up[0].iter().all(|p| p[0] < 0.0 + 1e-9));
    }
}
