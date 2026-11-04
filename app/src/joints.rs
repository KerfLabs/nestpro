//! Automatic micro-joints the way Lantek Expert V44 spreads them along a contour
//! and fits each one on its element. Rules: MECHANICS.md "Automatic lead-ins and
//! micro-joints by contour class" (AM1–AM6).
//!
//! In shop terms: a contour gets one joint per `D` mm of its length (rounded up,
//! kept within the size class's fewest/most, and made even), spread evenly from
//! half a spacing after the lead-in, each one centred on the line or arc it lands
//! on. A joint that lands next to a corner goes on the corner.

use crate::nest::P;

/// How many joints (rule AM1): `n = ceil(P / D)`; none when `|D| ≤ tol`, or when
/// `D > P` and the minimum is 0; then at least `min`, at most `max` (that one
/// marks the count as *clamped*); an odd count above 1 that wasn't clamped is
/// made even. Returns (count, clamped).
pub fn count(per: f64, d: f64, min: u32, max: u32, tol: f64) -> (u32, bool) {
    if d.abs() <= tol || per <= 0.0 {
        return (0, false);
    }
    if d.abs() > per && min == 0 {
        return (0, false);
    }
    let mut n = (per / d.abs()).ceil().max(0.0) as u64;
    n = n.max(min as u64);
    let mut clamped = false;
    if n >= max as u64 {
        n = max as u64;
        clamped = true;
    }
    if n == 0 {
        return (0, clamped);
    }
    // n == 1 is placed on its own before the rounding (AM1).
    if n % 2 == 1 && n > 1 && !clamped {
        n += 1;
    }
    (n.min(u32::MAX as u64) as u32, clamped)
}

/// Settings of the walk: the maximum distance D (F150), minimum distance to a
/// corner (F151) and between joints on one element (F152), linear tolerance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub every: f64,
    pub corner_min: f64,
    pub between_min: f64,
    pub tol: f64,
}

/// Spread `n` joints along a contour made of elements of these lengths, starting
/// at its start (the lead-in point). Returns the joint positions as distances
/// along the contour (rule AM4):
/// * the first joint is half a spacing `s = P / n` after the start;
/// * on each element the positions o, o+s, … that fit are counted (k); when the
///   count wasn't clamped they are **centred on the element**: the first at
///   `(L − (k−1)·s) / 2`;
/// * an element no longer than `2·Cmin + tol` gets none;
/// * a first position nearer than `Cmin` to the element start re-spreads the
///   joints between `Cmin` and `L − Cmin` (our reading of the not-clamped split);
/// * the leftover carries: `rem = L − first − (k−1)·step`, next offset
///   `s − rem` when `rem − tol ≤ D`, else `s/2`; never under `Cmin`.
pub fn walk(lens: &[f64], n: u32, clamped: bool, w: &Walk) -> Vec<f64> {
    let per: f64 = lens.iter().sum();
    let mut out: Vec<f64> = vec![];
    if n == 0 || per <= 0.0 {
        return out;
    }
    let s = per / n as f64;
    // n == 1: one joint, moved off the start to the middle of the first element
    // (or 2·S + P/10 — S isn't known here, the caller's fit moves it).
    if n == 1 {
        return vec![lens[0] / 2.0];
    }
    let step = s.max(w.between_min);
    let mut o = s / 2.0;
    let mut acc = 0.0;
    for &l in lens {
        if out.len() as u32 >= n {
            break;
        }
        if l <= 2.0 * w.corner_min + w.tol {
            o = (o - l).max(w.corner_min);
            acc += l;
            continue;
        }
        let k = if o > l + w.tol { 0 } else { ((l - o + w.tol) / step).floor() as usize + 1 };
        if k == 0 {
            o = (o - l).max(w.corner_min);
            acc += l;
            continue;
        }
        let k = k.min((n as usize).saturating_sub(out.len()));
        let mut first = if clamped { o } else { (l - (k as f64 - 1.0) * step) / 2.0 };
        let mut st = step;
        if first < w.corner_min - w.tol {
            // Re-spread between Cmin and L − Cmin (AM4).
            let span = l - 2.0 * w.corner_min;
            let parts = if clamped {
                k
            } else {
                let mut m = 1usize;
                while span / m as f64 > w.every + w.tol && m < 10_000 {
                    m += 1;
                }
                m
            };
            st = span / parts as f64;
            first = w.corner_min + st / 2.0;
            for j in 0..parts.min((n as usize).saturating_sub(out.len())) {
                out.push(acc + first + j as f64 * st);
            }
            let rem = l - (first + (parts as f64 - 1.0) * st);
            o = if rem - w.tol <= w.every { s - rem } else { s / 2.0 };
            o = o.max(w.corner_min);
            acc += l;
            continue;
        }
        for j in 0..k {
            out.push(acc + first + j as f64 * st);
        }
        let rem = l - first - (k as f64 - 1.0) * st;
        o = if rem - w.tol <= w.every { s - rem } else { s / 2.0 };
        o = o.max(w.corner_min);
        acc += l;
    }
    out
}

/// Where one joint ends up on its element (rules AM4, AM5). `c` = the joint's
/// position, `(s0, s1)`
/// the element holding it, `line` = a straight element, `gap` = S.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// No joint: the element is no longer than the gap (AM5).
    Refused,
    /// An uncut gap from .0 to .1 along the contour.
    Edge(f64, f64),
    /// The joint went to the element's end (a corner) at this distance (AM5).
    Corner(f64),
}

/// Fit a joint on its element (rule AM5). Refused when `L ≤ S` (corrected: not
/// 10 × S). On a straight element a joint within `2·tol + S` of an end goes to
/// that end and becomes a corner joint. Otherwise the gap is centred on the point;
/// gaps with leads keep their centre at least 3·tol from the ends; plain gaps
/// that would run past an end slide flush against it.
pub fn fit(c: f64, s0: f64, s1: f64, line: bool, gap: f64, leads: bool, tol: f64) -> Fit {
    let l = s1 - s0;
    if l <= gap {
        return Fit::Refused;
    }
    if line {
        if c - s0 <= 2.0 * tol + gap {
            return Fit::Corner(s0);
        }
        if s1 - c <= 2.0 * tol + gap {
            return Fit::Corner(s1);
        }
    }
    let half = gap / 2.0;
    let c = if leads { c.clamp(s0 + 3.0 * tol + half, (s1 - 3.0 * tol - half).max(s0 + 3.0 * tol + half)) } else { c };
    let a = (c - half).clamp(s0, s1 - gap);
    Fit::Edge(a, a + gap)
}

/// A corner joint straddles the corner (rule AM5):
/// its ends are where lines parallel to the corner's bisector, ±S/2 off it,
/// cross the two elements — `S / (2·sin(θ/2))` either side of the corner along
/// the contour, θ = the angle between the elements.
pub fn corner_span(at: f64, angle_deg: f64, gap: f64) -> (f64, f64) {
    let h = gap / (2.0 * (angle_deg.to_radians() / 2.0).sin().max(0.05));
    (at - h, at + h)
}

/// The lead shapes of a joint type (rule AM6):
/// (lead-in inclined?, lead-out inclined?). Types 1/4/7 both inclined; 2/3 both
/// perpendicular (the angle is not used); 5 only the lead-out inclined; 6 only
/// the lead-in; 8 lead-in special (not decoded: perpendicular), lead-out perpendicular.
pub fn lead_forms(code: i32) -> (bool, bool) {
    match code {
        1 | 4 | 7 => (true, true),
        5 => (false, true),
        6 => (true, false),
        _ => (false, false),
    }
}

/// A joint landing on a corner turns edge types into corner types (3 → 2,
/// 4/5/6 → 1); elsewhere corner types turn into edge types (1/7 → 4, 2/8 → 3) (AM5).
pub fn type_at(code: i32, corner: bool) -> i32 {
    match (code, corner) {
        (3, true) => 2,
        (4..=6, true) => 1,
        (1 | 7, false) => 4,
        (2 | 8, false) => 3,
        _ => code,
    }
}

/// Which contours a joint setting reaches when the CT1's F block fills only the
/// default record (rule AM3): an edge
/// type (category 4: 3–6, 9–11) reaches every contour, holes too; a corner type
/// (categories 1/2: 1, 2, 7, 8) only the records that allow corners — rectangular,
/// slot and other-shaped holes, and rectangular and other-shaped parts — and
/// there it puts joints on corners only. Round contours get none from a corner type.
/// Returns (joints at all, corner mode).
pub fn reaches(code: i32, round: bool) -> (bool, bool) {
    match crate::nest::JointType::category(code) {
        4 => (true, false),
        1 | 2 => (!round, true),
        _ => (false, false),
    }
}

/// A joint's lead-out (from the gap's start) and lead-in (to the gap's end) (rule AM6):
/// the free ends sit D off the contour on the scrap side, leaning **away from the
/// gap** by D / tan A along the contour (the lead-out back from the gap start, the
/// lead-in from beyond the gap end); perpendicular forms (or |tan A| < 1e-6) have no
/// lean. A lead-out only when D_out > 2·tol (an empty path otherwise, just the gap start).
/// `ta`, `tb` = cutting directions at the gap's ends, `na`, `nb` = scrap normals.
#[allow(clippy::too_many_arguments)]
pub fn lead_paths(pa: P, ta: P, na: P, pb: P, tb: P, nb: P, d_in: f64, d_out: f64, angle: f64, forms: (bool, bool), tol: f64) -> (Vec<P>, Vec<P>) {
    let lean = |d: f64, inclined: bool| {
        let t = angle.to_radians().tan();
        if !inclined || t.abs() < 1e-6 {
            0.0
        } else {
            d / t
        }
    };
    let out = if d_out > 2.0 * tol {
        let x = -lean(d_out, forms.1);
        vec![pa, [pa[0] + ta[0] * x + na[0] * d_out, pa[1] + ta[1] * x + na[1] * d_out]]
    } else {
        vec![pa]
    };
    let x = lean(d_in, forms.0);
    let back = vec![[pb[0] + tb[0] * x + nb[0] * d_in, pb[1] + tb[1] * x + nb[1] * d_in], pb];
    (out, back)
}

#[cfg(test)]
mod joint_rule_tests {
    use super::*;

    const W: Walk = Walk { every: 100.0, corner_min: 0.0, between_min: 0.0, tol: 0.01 };

    #[test]
    fn joints_count_one_per_distance_rounded_up_and_even() {
        // 600 mm at one per 100 mm: 6. 660 mm: ceil 6.6 = 7, odd → 8.
        assert_eq!(count(600.0, 100.0, 0, 99999, 0.01), (6, false));
        assert_eq!(count(660.0, 100.0, 0, 99999, 0.01), (8, false));
        // Shorter than D with no minimum: none; with a minimum of 1: one joint.
        assert_eq!(count(62.8, 100.0, 0, 99999, 0.01), (0, false));
        assert_eq!(count(62.8, 100.0, 1, 99999, 0.01), (1, false));
        assert_eq!(count(250.0, 100.0, 0, 99999, 0.01), (4, false));
        // The maximum clamps and stops the rounding to even.
        assert_eq!(count(2040.0, 100.0, 0, 5, 0.01), (5, true));
        // D at or under the tolerance: none.
        assert_eq!(count(600.0, 0.0, 3, 10, 0.01), (0, false));
    }

    #[test]
    fn joints_are_centred_on_each_side() {
        // 200 × 100 from the corner at the start of a long side: 2 joints at 50 and
        // 150 on each long side, 1 in the middle of each short side.
        let p = walk(&[200.0, 100.0, 200.0, 100.0], 6, false, &W);
        let want = [50.0, 150.0, 250.0, 350.0, 450.0, 550.0];
        assert_eq!(p.len(), 6);
        assert!(p.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9), "{p:?}");
        // 230 × 100: 8 joints, 3 centred on each long side (32.5 / 115 / 197.5).
        let p = walk(&[230.0, 100.0, 230.0, 100.0], 8, false, &W);
        let want = [32.5, 115.0, 197.5, 280.0, 362.5, 445.0, 527.5, 610.0];
        assert!(p.len() == 8 && p.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9), "{p:?}");
        // 50 × 30: 2 joints, the middle of each 50 side.
        let p = walk(&[50.0, 30.0, 50.0, 30.0], 2, false, &W);
        assert!(p.len() == 2 && (p[0] - 25.0).abs() < 1e-9 && (p[1] - 105.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn a_short_element_takes_the_whole_gap_or_nothing() {
        // Corrected (J5): the element only has to be longer than the gap.
        assert_eq!(fit(5.0, 0.0, 10.0, true, 0.5, false, 0.01), Fit::Edge(4.75, 5.25));
        assert_eq!(fit(0.3, 0.0, 0.5, false, 0.5, false, 0.01), Fit::Refused);
        // A plain gap past the end of an arc slides flush against it.
        assert_eq!(fit(9.9, 0.0, 10.0, false, 0.5, false, 0.01), Fit::Edge(9.5, 10.0));
        // Within 2·tol + S of a line's end: a corner joint at that end.
        assert_eq!(fit(9.65, 0.0, 10.0, true, 0.35, true, 0.01), Fit::Corner(10.0));
        assert!(matches!(fit(9.6, 0.0, 10.0, true, 0.35, true, 0.01), Fit::Edge(..)));
        assert_eq!(fit(0.3, 0.0, 10.0, true, 0.35, true, 0.01), Fit::Corner(0.0));
        // A 90° corner joint reaches S / (2 sin 45°) either side.
        let (a, b) = corner_span(100.0, 90.0, 1.0);
        assert!((b - a - 2.0f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn joint_types_switch_between_corner_and_edge_and_set_the_lead_shapes() {
        assert_eq!([type_at(3, true), type_at(5, true), type_at(1, false), type_at(8, false), type_at(9, true)], [2, 1, 4, 3, 9]);
        assert_eq!([lead_forms(3), lead_forms(4), lead_forms(5), lead_forms(6)], [(false, false), (true, true), (false, true), (true, false)]);
        // Edge types reach holes and parts of every shape; corner types skip round ones.
        assert_eq!(reaches(3, true), (true, false));
        assert_eq!(reaches(1, true), (false, true));
        assert_eq!(reaches(1, false), (true, true));
    }

    #[test]
    fn joint_leads_lean_away_from_the_gap() {
        // Gap from (10, 0) to (11, 0) on a line cut along +x; scrap below (−y).
        let (t, n) = ([1.0, 0.0], [0.0, -1.0]);
        let (out, back) = lead_paths([10.0, 0.0], t, n, [11.0, 0.0], t, n, 3.0, 2.0, 45.0, (true, true), 0.01);
        assert_eq!(out, vec![[10.0, 0.0], [8.0, -2.0]]);
        assert_eq!(back, vec![[14.0, -3.0], [11.0, 0.0]]);
        // Perpendicular (type 3): no lean; D out 0: no lead-out.
        let (out, back) = lead_paths([10.0, 0.0], t, n, [11.0, 0.0], t, n, 3.0, 0.0, 45.0, lead_forms(3), 0.01);
        assert_eq!(out, vec![[10.0, 0.0]]);
        assert_eq!(back, vec![[11.0, -3.0], [11.0, 0.0]]);
    }
}
