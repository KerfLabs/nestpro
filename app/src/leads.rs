//! Automatic lead-ins and lead-outs by contour class, the way Lantek Expert V44
//! places them on outer contours and holes. Rules table: MECHANICS.md
//! "Automatic lead-ins and micro-joints by contour class" (AL1–AL11).
//!
//! In shop terms: every closed contour is sorted into a class (round hole,
//! round part, rectangular hole/part, slot hole, any other shape). The machine's
//! technology file (CT1) holds one lead setting per class and cutting quality:
//! lead type, distance D, angle A and radius R for the lead-in and lead-out.
//! A keypad zone (1 = bottom left of the contour's box) says where it goes.

use crate::nest::{CutQuality, P};
use std::collections::HashMap;

/// "Not set" marker in the technology tables, compared within 1.0 (rule AL2).
pub const UNSET: f64 = 99999.999;
pub const UNSET_TOL: f64 = 1.0;
/// Qualities per cutting type (the original keeps 12 per machine).
pub const QUALITIES: usize = 12;
/// CT1 codes per quality in the E block (E000 = quality 2, E071 = quality 3 …).
pub const E_BLOCK: usize = 71;
/// Linear / angular tolerance when the CT1 has no A005 / A006 (the laser files' values).
pub const LIN_TOL: f64 = 0.01;
pub const ANG_TOL: f64 = 0.1;
/// Points per arc when a lead arc is drawn as a polyline (our own).
pub const ARC_STEPS: usize = 8;

/// The Lantek quality number for our three cut qualities. **Our own choice**
/// (the original numbers qualities 1–12 per machine): Normal is quality 1 (the
/// B block, which every quality without its own table copies), Fine is 2 (E000…),
/// Rough is 3 (E071…).
pub fn quality_number(q: CutQuality) -> usize {
    match q {
        CutQuality::Normal => 1,
        CutQuality::Fine => 2,
        CutQuality::Rough => 3,
    }
}

/// One lead setting (the original's lead record, rule A2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeadRec {
    /// Lead type 0–16: 0 none, 1–9 perpendicular/inclined/tangent arc
    /// in × out, 10 corner, 11 odd corner, 12/13 laser arc leads, 14–16 special.
    pub kind: u8,
    /// D (distance off the contour), A (degrees to the contour), R (arc radius),
    /// for the lead-in and the lead-out.
    pub d_in: f64,
    pub d_out: f64,
    pub a_in: f64,
    pub a_out: f64,
    pub r_in: f64,
    pub r_out: f64,
    /// Overlap past the start; 99999.999 = use B156.
    pub overlap: f64,
    /// Angular tolerance; overwritten by B175, the angle step for leads on circles.
    pub ang_tol: f64,
}

impl LeadRec {
    pub const fn new(kind: u8, d: (f64, f64), a: (f64, f64), r: (f64, f64)) -> LeadRec {
        LeadRec { kind, d_in: d.0, d_out: d.1, a_in: a.0, a_out: a.1, r_in: r.0, r_out: r.1, overlap: UNSET, ang_tol: 0.0 }
    }
    /// Lead-in and lead-out form for types 1–9 (rule AL8): in = (t−1) div 3,
    /// out = (t−1) mod 3; 0 perpendicular, 1 inclined, 2 tangent arc. Other types: (0, 0).
    pub fn forms(&self) -> (u8, u8) {
        match self.kind {
            t @ 1..=9 => ((t - 1) / 3, (t - 1) % 3),
            _ => (0, 0),
        }
    }
}

/// Contour classes, in the order of the 8 records (k = 0…7; the general class
/// uses records 5, 6, 7 = "contour 1, 2, 3").
pub const CLASS_NAMES: [&str; 8] = ["Round hole", "Round part", "Rectangular hole", "Rectangular part", "Slot hole", "Other shape 1", "Other shape 2", "Other shape 3"];

/// One quality's automatic lead table (rule AL1).
#[derive(Clone, Debug, PartialEq)]
pub struct LeadQuality {
    /// Zone for outer contours / holes (B200 / B271; E000 / E070). −1 = same as outer.
    pub zone: i32,
    pub hole_zone: i32,
    /// Force placement (off by default; its UI switch isn't known).
    pub force: bool,
    /// Largest corner angle for a corner lead (B201), odd-corner max/min (B202/B203).
    pub corner_max: f64,
    pub odd_max: f64,
    pub odd_min: f64,
    /// Shortest element (B204) and smallest arc radius (B205) that take a lead.
    pub min_len: f64,
    pub min_radius: f64,
    /// Pierce radius (B294 / E781…) — read, not used.
    #[allow(dead_code)]
    pub pierce_radius: f64,
    /// Shortest lead-in a collision test may leave (B295 / E792…).
    pub min_lead: f64,
    /// Round holes: keep the lead short of the centre (on by default, rule AL10).
    pub limit_to_centre: bool,
    pub recs: [LeadRec; 8],
}

impl LeadQuality {
    /// The zone for a contour: holes take the table's own hole zone when it has
    /// one (B271 / E070 ≥ 1), otherwise the outer zone; `outer` overrides the
    /// table's outer zone (the Technology tab's position pad — our choice).
    pub fn zone_for(&self, inner: bool, outer: Option<i32>) -> i32 {
        if inner && self.hole_zone >= 1 {
            self.hole_zone
        } else {
            outer.unwrap_or(self.zone)
        }
    }
}

impl Default for LeadQuality {
    /// The laser template's B block (Lantek's PSTBUR01.CT1 and PSTMAZ03.CT1
    /// carry the same values). Every value we have from a BLM laser's
    /// PstAdi06.CT1 (zone 1, corner 135°, element 5 mm, round holes 7/5, contours
    /// 7/5 at 45°, overlap 0.5) matches it, so it is the built-in table.
    fn default() -> Self {
        LeadQuality {
            zone: 1,
            hole_zone: -1,
            force: false,
            corner_max: 135.0,
            odd_max: 135.0,
            odd_min: 45.0,
            min_len: 5.0,
            min_radius: 5.0,
            pierce_radius: -1.0,
            min_lead: 0.0,
            limit_to_centre: true,
            recs: [
                LeadRec::new(13, (7.0, 5.0), (0.0, 0.0), (7.5, 5.5)),
                LeadRec::new(1, (7.0, 5.0), (45.0, 45.0), (7.5, 5.5)),
                LeadRec::new(11, (7.0, 5.0), (5.0, 5.0), (7.5, 5.5)),
                LeadRec::new(10, (7.0, 5.0), (45.0, 45.0), (7.5, 5.5)),
                LeadRec::new(12, (7.0, 5.0), (45.0, 45.0), (7.5, 5.5)),
                LeadRec::new(10, (7.0, 5.0), (45.0, 45.0), (7.5, 5.5)),
                LeadRec::new(11, (7.0, 5.0), (5.0, 5.0), (7.5, 5.5)),
                LeadRec::new(5, (7.0, 5.0), (45.0, 45.0), (7.5, 5.5)),
            ],
        }
    }
}

/// All qualities plus the global lead codes of the CT1.
#[derive(Clone, Debug, PartialEq)]
pub struct LeadTable {
    /// Index 0 = quality 1 (B block), 1… = quality 2… (E block).
    pub q: Vec<LeadQuality>,
    /// B156: overlap for every record left at 99999.999.
    pub overlap: f64,
    /// B175: angular step for leads on circles (also overwrites each record's tolerance).
    pub circle_step: f64,
    /// B185: collision test of automatic leads (0 off, 1 shorten, 2 move, 3 move then shorten, …).
    pub test: i32,
    /// B186: share of the lead cut off per shortening step.
    pub shorten: f64,
    /// B187: tries when moving a lead along its contour.
    pub tries: u32,
    /// A005 / A006: linear (mm) and angular (degrees) tolerance.
    pub lin_tol: f64,
    pub ang_tol: f64,
}

impl Default for LeadTable {
    /// Built-in values: the laser B block for every quality, B156 = 0.5 (BLM
    /// laser's CT1), B185 = 3 / B186 = 0.1 / B187 = 10 (PSTadi02.CT1 template).
    fn default() -> Self {
        LeadTable { q: vec![LeadQuality::default(); QUALITIES], overlap: 0.5, circle_step: 0.0, test: 3, shorten: 0.1, tries: 10, lin_tol: LIN_TOL, ang_tol: ANG_TOL }
    }
}

impl LeadTable {
    /// Read the tables from CT1 codes (rule AL1). Quality 1 = B200–B271 (+B294/B295); quality q ≥ 2 =
    /// E block (q−2)·71 (+E781/E792 + q−2). A quality whose E block is missing
    /// copies quality 1. Overlaps for every quality: E814 + 8(q−1) + class (AL2).
    pub fn from_codes(m: &HashMap<String, String>) -> LeadTable {
        let d = LeadTable::default();
        let num = |k: &str| m.get(k).and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok());
        let mut t = LeadTable {
            overlap: num("B156").unwrap_or(d.overlap),
            circle_step: num("B175").unwrap_or(d.circle_step),
            test: num("B185").map_or(d.test, |v| v as i32),
            shorten: num("B186").unwrap_or(d.shorten),
            tries: num("B187").map_or(d.tries, |v| v.max(0.0) as u32),
            lin_tol: num("A005").filter(|v| *v > 0.0).unwrap_or(d.lin_tol),
            ang_tol: num("A006").filter(|v| *v > 0.0).unwrap_or(d.ang_tol),
            q: vec![],
        };
        // Code of field `off` (0–70, the E-block layout) for quality q.
        let code = |q: usize, off: usize| -> String {
            if q == 1 {
                // B200–B213 = offsets 0–13, B214–B261 = 14–61, B263–B270 = 62–69, B271 = 70.
                format!("B{}", 200 + off + usize::from(off >= 62))
            } else {
                format!("E{:03}", (q - 2) * E_BLOCK + off)
            }
        };
        let base = LeadQuality::default();
        for q in 1..=QUALITIES {
            let fallback = if q == 1 { base.clone() } else { t.q[0].clone() };
            if q > 1 && num(&code(q, 0)).is_none() {
                t.q.push(fallback);
            } else {
                let f = |off: usize, def: f64| num(&code(q, off)).unwrap_or(def);
                let mut l = LeadQuality {
                    zone: f(0, fallback.zone as f64) as i32,
                    hole_zone: f(70, fallback.hole_zone as f64) as i32,
                    corner_max: f(1, fallback.corner_max),
                    odd_max: f(2, fallback.odd_max),
                    odd_min: f(3, fallback.odd_min),
                    min_len: f(4, fallback.min_len),
                    min_radius: f(5, fallback.min_radius),
                    ..fallback.clone()
                };
                let (pr, ml) = if q == 1 { ("B294".to_string(), "B295".to_string()) } else { (format!("E{}", 781 + q - 2), format!("E{}", 792 + q - 2)) };
                l.pierce_radius = num(&pr).unwrap_or(fallback.pierce_radius);
                l.min_lead = num(&ml).unwrap_or(fallback.min_lead);
                for k in 0..8 {
                    let r = &mut l.recs[k];
                    let fb = fallback.recs[k];
                    r.kind = f(6 + k, fb.kind as f64).max(0.0) as u8;
                    let b = 14 + 6 * k;
                    r.d_in = f(b, fb.d_in);
                    r.d_out = f(b + 1, fb.d_out);
                    r.a_in = f(b + 2, fb.a_in);
                    r.a_out = f(b + 3, fb.a_out);
                    r.r_in = f(b + 4, fb.r_in);
                    r.r_out = f(b + 5, fb.r_out);
                    r.ang_tol = f(62 + k, fb.ang_tol);
                }
                t.q.push(l);
            }
            let l = t.q.last_mut().unwrap();
            for k in 0..8 {
                if let Some(v) = num(&format!("E{}", 814 + 8 * (q - 1) + k)) {
                    l.recs[k].overlap = v;
                }
            }
        }
        t
    }

    /// The table for quality `q` (1–12; anything else → quality 1), fixed up the
    /// way the original does before use (rule AL2): overlap 99999.999 → B156,
    /// tolerance ← B175, type ≥ 100 → type mod 100, min element ≥ |B156| + 4·tol.
    /// A hole zone of −1 means the outer zone (see `zone_for`).
    pub fn quality(&self, q: usize) -> LeadQuality {
        let i = if (1..=QUALITIES).contains(&q) { q - 1 } else { 0 };
        let mut l = self.q.get(i).or(self.q.first()).cloned().unwrap_or_default();
        for r in l.recs.iter_mut() {
            if (r.overlap - UNSET).abs() <= UNSET_TOL {
                r.overlap = self.overlap;
            }
            r.ang_tol = self.circle_step;
            if r.kind >= 100 {
                r.kind %= 100;
            }
        }
        l.min_len = l.min_len.max(self.overlap.abs() + 4.0 * self.lin_tol);
        l
    }
}

// ---------------------------------------------------------------------------
// Contour classes (rule AL3)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    RoundHole,
    RoundPart,
    RectHole,
    RectPart,
    SlotHole,
    /// Any other shape (and slot-shaped outer contours, AL3): records 5, 6, 7.
    General,
}

impl Class {
    /// The lead record the class uses (the general class starts at 5).
    pub fn rec(self) -> usize {
        match self {
            Class::RoundHole => 0,
            Class::RoundPart => 1,
            Class::RectHole => 2,
            Class::RectPart => 3,
            Class::SlotHole => 4,
            Class::General => 5,
        }
    }
}

/// Sort a contour into its class. Our contours are polylines, so the standard
/// shapes are recognised from the points (`machine::recognise`, with its
/// tolerances): circle → rectangle → slot, the original's order. A slot-shaped
/// **outer** contour goes the general way, as in the original (AL3).
pub fn classify(pts: &[P], inner: bool) -> (Class, Option<(P, f64)>) {
    use crate::machine::StdShape;
    match crate::machine::recognise(pts) {
        Some(StdShape::Circle { c, d }) => (if inner { Class::RoundHole } else { Class::RoundPart }, Some((c, d / 2.0))),
        Some(StdShape::Rect { .. }) => (if inner { Class::RectHole } else { Class::RectPart }, None),
        Some(StdShape::Obround { .. }) if inner => (Class::SlotHole, None),
        _ => (Class::General, None),
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn norm(v: P) -> P {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-12);
    [v[0] / l, v[1] / l]
}

fn signed_area2(pts: &[P]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum()
}

/// Unit normal of cutting direction `t` pointing into the scrap: outside an
/// outer contour, inside a hole.
pub fn scrap_normal(pts: &[P], inner: bool, t: P) -> P {
    let ccw = signed_area2(pts) >= 0.0;
    let out = if ccw { [t[1], -t[0]] } else { [-t[1], t[0]] };
    if inner {
        [-out[0], -out[1]]
    } else {
        out
    }
}

fn bbox(pts: &[P]) -> [f64; 4] {
    pts.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// The side a two-digit zone code asks for: 0 bottom, 1 left, 2 right, 3 top.
fn zone_side(zone: i32) -> Option<u8> {
    match zone {
        10 | 20 | 21 | 30 => Some(0),
        11 | 40 | 41 | 71 => Some(1),
        31 | 60 | 61 | 91 => Some(2),
        70 | 80 | 81 | 90 => Some(3),
        _ => None,
    }
}

/// Zone point on a box (rule AL4): 1 (x0,y0), 2 (xm,y0), 3 (x1,y0), 4 (x0,ym),
/// 6 (x1,ym), 7 (x0,y1), 8 (xm,y1), 9 (x1,y1); 5 = the given reference point.
/// Two-digit codes use code / 10.
pub fn zone_point(b: [f64; 4], zone: i32, refp: P) -> P {
    let z = if zone > 9 { zone / 10 } else { zone };
    let (xm, ym) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    match z {
        1 => [b[0], b[1]],
        2 => [xm, b[1]],
        3 => [b[2], b[1]],
        4 => [b[0], ym],
        6 => [b[2], ym],
        7 => [b[0], b[3]],
        8 => [xm, b[3]],
        9 => [b[2], b[3]],
        _ => refp,
    }
}

/// Angle (degrees) of the lead on a round contour for a zone (rule AL4): 1 225°, 2 270°, 3 315°, 4 180°, 6 0°, 7 135°, 8 90°, 9 45°;
/// two-digit codes by their side; 5 (or anything else) → toward the reference point.
pub fn zone_angle(zone: i32) -> Option<f64> {
    if let Some(s) = zone_side(zone) {
        return Some([270.0, 180.0, 0.0, 90.0][s as usize]);
    }
    match zone {
        1 => Some(225.0),
        2 => Some(270.0),
        3 => Some(315.0),
        4 => Some(180.0),
        6 => Some(0.0),
        7 => Some(135.0),
        8 => Some(90.0),
        9 => Some(45.0),
        _ => None,
    }
}

/// One element of a contour for lead placement: a straight segment or a run of
/// short segments (an arc). Breaks where the outline turns more than
/// `nest::ELEMENT_CORNER_DEG`, and where a segment meets one more than
/// `ELEMENT_LENGTH_RATIO` times longer or shorter (a straight joining an arc
/// tangentially; our own rule for polylines).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elem {
    /// Distance along the contour where it starts, and its length.
    pub s0: f64,
    pub len: f64,
    /// Total turn (degrees) along it: 0 for a straight line.
    pub turn: f64,
}

/// Segment length ratio that splits a straight from a tangent arc (our own estimate).
pub const ELEMENT_LENGTH_RATIO: f64 = 2.0;

fn turn_at(pts: &[P], i: usize) -> f64 {
    let n = pts.len();
    let (a, v, b) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
    let u = norm([v[0] - a[0], v[1] - a[1]]);
    let w = norm([b[0] - v[0], b[1] - v[1]]);
    (u[0] * w[0] + u[1] * w[1]).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Is vertex i an element break? (pts[0] always is: the contour starts there.)
pub fn is_break(pts: &[P], i: usize) -> bool {
    let n = pts.len();
    if i == 0 {
        return true;
    }
    let la = dist(pts[(i + n - 1) % n], pts[i]);
    let lb = dist(pts[i], pts[(i + 1) % n]);
    turn_at(pts, i) > crate::nest::ELEMENT_CORNER_DEG || la > lb * ELEMENT_LENGTH_RATIO || lb > la * ELEMENT_LENGTH_RATIO
}

/// The contour's elements, starting at pts[0].
pub fn elements(pts: &[P]) -> Vec<Elem> {
    let n = pts.len();
    let mut out: Vec<Elem> = vec![];
    let mut acc = 0.0;
    for i in 0..n {
        let l = dist(pts[i], pts[(i + 1) % n]);
        if is_break(pts, i) || out.is_empty() {
            out.push(Elem { s0: acc, len: 0.0, turn: 0.0 });
        } else {
            out.last_mut().unwrap().turn += turn_at(pts, i);
        }
        let e = out.last_mut().unwrap();
        e.len += l;
        acc += l;
    }
    out
}

/// Point at distance `d` along the closed polyline.
pub fn point_at(pts: &[P], d: f64) -> P {
    crate::cnc::point_at(pts, d)
}

/// Unit cutting direction of the segment holding distance `d` (the segment
/// arriving at the point when `arriving` and `d` is on a vertex).
pub fn dir_at(pts: &[P], d: f64, arriving: bool) -> P {
    let n = pts.len();
    let mut acc = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = dist(a, b);
        if l < 1e-12 {
            continue;
        }
        let end = acc + l;
        if (arriving && d <= end + 1e-9 && d > acc + 1e-9) || (!arriving && d < end - 1e-9 && d >= acc - 1e-9) {
            return norm([b[0] - a[0], b[1] - a[1]]);
        }
        acc = end;
    }
    // Arriving at the contour's start: the last segment.
    let (a, b) = (pts[n - 1], pts[0]);
    if arriving && dist(a, b) > 1e-12 {
        return norm([b[0] - a[0], b[1] - a[1]]);
    }
    let b2 = pts.iter().skip(1).copied().find(|p| dist(*p, pts[0]) > 1e-12).unwrap_or([pts[0][0] + 1.0, pts[0][1]]);
    norm([b2[0] - pts[0][0], b2[1] - pts[0][1]])
}

/// Angle (0–360) between the two elements at vertex i, measured inside the
/// part's material (`AnguloPuntos0360`; outer contours: the polygon's interior
/// angle, holes: 360° minus it).
pub fn material_angle(pts: &[P], i: usize, inner: bool) -> f64 {
    let (interior, _) = crate::cnc::corner_at(pts, i);
    if inner {
        360.0 - interior
    } else {
        interior
    }
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// Where an automatic lead goes: the point on the contour, which record it uses
/// and whether the point is a corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub at: P,
    pub rec: usize,
    pub corner: bool,
}

/// Is a corner lead of `kind` (10 or 11) valid at vertex i? (Rule AL5.) Type 10: the corner points into the scrap (material angle under
/// 180°) and is at most `corner_max` (+ tol). Type 11: measured on the scrap side,
/// between `odd_min` and `odd_max` (± tol); a straight-through point only when
/// `odd_max` ≥ 180 − tol. No side-length test.
pub fn corner_valid(pts: &[P], i: usize, inner: bool, kind: u8, q: &LeadQuality, ang_tol: f64) -> bool {
    let m = material_angle(pts, i, inner);
    if kind == 10 {
        m < 180.0 - ang_tol && m <= q.corner_max + ang_tol
    } else {
        let s = 360.0 - m;
        if (s - 180.0).abs() <= ang_tol {
            return q.odd_max >= 180.0 - ang_tol;
        }
        s < 180.0 && s <= q.odd_max + ang_tol && s >= q.odd_min - ang_tol
    }
}

fn on_side(p: P, b: [f64; 4], side: Option<u8>, tol: f64) -> bool {
    match side {
        None => true,
        Some(0) => (p[1] - b[1]).abs() <= tol,
        Some(1) => (p[0] - b[0]).abs() <= tol,
        Some(2) => (p[0] - b[2]).abs() <= tol,
        _ => (p[1] - b[3]).abs() <= tol,
    }
}

/// Nearest valid corner to the zone point (rule AL5): real corners (both elements longer than tol, turning more than
/// `ELEMENT_CORNER_DEG`), starting from 99999.999.
fn corner_search(pts: &[P], inner: bool, kind: u8, q: &LeadQuality, zp: P, side: Option<u8>, b: [f64; 4], tol: f64, ang_tol: f64) -> Option<P> {
    let n = pts.len();
    let mut best: Option<(P, f64)> = None;
    for i in 0..n {
        let (la, lb) = (dist(pts[(i + n - 1) % n], pts[i]), dist(pts[i], pts[(i + 1) % n]));
        if la <= tol || lb <= tol || turn_at(pts, i) <= crate::nest::ELEMENT_CORNER_DEG {
            continue;
        }
        if !corner_valid(pts, i, inner, kind, q, ang_tol) || !on_side(pts[i], b, side, tol) {
            continue;
        }
        let d = dist(pts[i], zp);
        if d < best.map_or(UNSET, |b| b.1) {
            best = Some((pts[i], d));
        }
    }
    best.map(|b| b.0)
}

/// Point at (L − w)/2 from the element's start, w = |overlap| (rule AL6, as for
/// a pick in the middle third): the lead and its overlap sit
/// centred on the element.
fn element_point(pts: &[P], e: &Elem, w: f64) -> P {
    point_at(pts, e.s0 + (e.len - w) / 2.0)
}

/// Middle of the element nearest the zone point (rule AL6): straight elements
/// at least `min_len`, arcs at least `min_len` with radius ≥ `min_radius` − tol,
/// and longer than 4·tol + |overlap|.
/// Full circles are skipped.
fn element_search(pts: &[P], els: &[Elem], q: &LeadQuality, w: f64, zp: P, side: Option<u8>, b: [f64; 4], tol: f64, long_only: bool) -> Option<P> {
    let mut best: Option<(P, f64)> = None;
    let longest = els.iter().filter(|e| e.turn < 1.0).map(|e| e.len).fold(0.0, f64::max);
    for e in els {
        if e.turn >= 359.0 {
            continue;
        }
        if e.len < q.min_len || e.len <= 4.0 * tol + w {
            continue;
        }
        if e.turn >= 1.0 {
            let r = e.len / e.turn.to_radians();
            if r < q.min_radius - tol {
                continue;
            }
        }
        if long_only && (e.turn >= 1.0 || e.len < longest - tol) {
            continue;
        }
        let mid = point_at(pts, e.s0 + e.len / 2.0);
        if !on_side(mid, b, side, tol) {
            continue;
        }
        let d = dist(mid, zp);
        if d < best.map_or(UNSET, |b| b.1) {
            best = Some((element_point(pts, e, w), d));
        }
    }
    best.map(|b| b.0)
}

/// Where the automatic lead goes on a contour, outer or hole (rules AL3–AL6).
/// `zone` is the zone for this contour (outer or hole zone), `refp` the zone-5 point,
/// `recs` the class records (already fixed up), `circle` the round contour's centre and radius.
pub fn place(pts: &[P], inner: bool, class: Class, circle: Option<(P, f64)>, q: &LeadQuality, recs: &[LeadRec; 8], zone: i32, refp: P, tol: f64, ang_tol: f64) -> Option<Placement> {
    if pts.len() < 3 {
        return None;
    }
    let b = bbox(pts);
    match class {
        Class::RoundHole | Class::RoundPart => {
            let (c, r) = circle?;
            let k = class.rec();
            // Round parts too small for the table get no lead (AL4).
            if class == Class::RoundPart && !q.force && (std::f64::consts::TAU * r + 1e-6 < q.min_len || r + 1e-6 < q.min_radius) {
                return None;
            }
            let mut th = zone_angle(zone).unwrap_or_else(|| (refp[1] - c[1]).atan2(refp[0] - c[0]).to_degrees());
            // B175 (the record's tolerance after the fix-up): on a full circle the
            // angle snaps to its step (AL4).
            let circle_step = recs[k].ang_tol;
            if circle_step > 0.0 && circle_step >= ang_tol {
                th = (th / circle_step).round() * circle_step;
            }
            let a = th.to_radians();
            Some(Placement { at: [c[0] + r * a.cos(), c[1] + r * a.sin()], rec: k, corner: false })
        }
        Class::RectHole | Class::RectPart | Class::SlotHole => {
            // Rectangles (AL6): type 10 → the corner by zone; type 12 → a long side;
            // others → the middle of the side nearest the zone. Slots are placed the
            // same way (estimate).
            let k = class.rec();
            let rec = recs[k];
            let zp = zone_point(b, zone, refp);
            if rec.kind == 0 {
                return None;
            }
            if rec.kind == 10 && class != Class::SlotHole {
                let n = pts.len();
                let best = (0..n).filter(|&i| turn_at(pts, i) > crate::nest::ELEMENT_CORNER_DEG).min_by(|&a, &c| dist(pts[a], zp).total_cmp(&dist(pts[c], zp)))?;
                return Some(Placement { at: pts[best], rec: k, corner: true });
            }
            let els = elements(pts);
            let w = rec.overlap.abs();
            let at = element_search(pts, &els, q, w, zp, None, b, tol, rec.kind == 12).or_else(|| element_search(pts, &els, q, w, zp, None, b, tol, false))?;
            Some(Placement { at, rec: k, corner: false })
        }
        Class::General => {
            // Contour 1, 2, 3 in turn (AL3); contour 1 at type 0 = no lead.
            if recs[5].kind == 0 {
                return None;
            }
            let side = zone_side(zone);
            let els = elements(pts);
            let passes: Vec<Option<u8>> = if side.is_some() { vec![side, None] } else { vec![None] };
            for sd in passes {
                let zp = zone_point(b, zone, refp);
                for k in 5..8 {
                    let rec = recs[k];
                    if rec.kind == 0 {
                        if k == 5 {
                            return None;
                        }
                        continue;
                    }
                    let found = if rec.kind == 10 || rec.kind == 11 {
                        corner_search(pts, inner, rec.kind, q, zp, sd, b, tol, ang_tol).map(|at| Placement { at, rec: k, corner: true })
                    } else {
                        element_search(pts, &els, q, rec.overlap.abs(), zp, sd, b, tol, false).map(|at| Placement { at, rec: k, corner: false })
                    };
                    if found.is_some() {
                        return found;
                    }
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Lead shapes
// ---------------------------------------------------------------------------

/// The lead geometry on a contour that starts at its lead point: the lead-in
/// from the pierce to pts[0], the lead-out from the overlap point outwards
/// (empty = none), and the overlap actually cut.
#[derive(Clone, Debug, PartialEq)]
pub struct LeadGeo {
    pub lead_in: Vec<P>,
    pub lead_out: Vec<P>,
    pub overlap: f64,
}

/// Limit a round hole's lead to its centre (rule AL10). Types 1–9: D ≥ r → D = r and an
/// inclined form turns perpendicular (A ≥ 90°), otherwise A ≥ 45°; R ≤ r/2.
/// Type 13: D ≤ r, R ≤ r/3, D ≤ √(r² − 2rR). Type 12: everything ≤ r/2.
pub fn limit_to_centre(rec: &mut LeadRec, r: f64) {
    let both = |d: &mut f64, a: &mut f64, rr: &mut f64| {
        if *d >= r {
            *d = r;
            *a = a.max(90.0);
        } else {
            *a = a.max(45.0);
        }
        *rr = rr.min(r / 2.0);
    };
    match rec.kind {
        1..=9 => {
            both(&mut rec.d_in, &mut rec.a_in, &mut rec.r_in);
            both(&mut rec.d_out, &mut rec.a_out, &mut rec.r_out);
        }
        13 => {
            for (d, rr) in [(&mut rec.d_in, &mut rec.r_in), (&mut rec.d_out, &mut rec.r_out)] {
                *rr = rr.min(r / 3.0);
                *d = d.min(r).min((r * r - 2.0 * r * *rr).max(0.0).sqrt());
            }
        }
        12 => {
            for v in [&mut rec.d_in, &mut rec.d_out, &mut rec.r_in, &mut rec.r_out] {
                *v = v.min(r / 2.0);
            }
        }
        _ => {}
    }
}

/// A lead-in in the local frame (x = cutting direction, y = scrap side), from
/// the pierce to the origin (rule AL8): perpendicular (0, D); inclined
/// (−D/tan A, D) (A ≤ 0.001° → perpendicular); tangent arc of R arriving along
/// +x, starting at height h = min(D, 2R).
pub fn local_lead_in(form: u8, d: f64, a: f64, r: f64, tol: f64) -> Vec<P> {
    match form {
        1 if a > 0.001 => vec![[-d / a.to_radians().tan(), d], [0.0, 0.0]],
        2 if r > tol => {
            let h = d.min(2.0 * r);
            let x = -(r * r - (h - r).powi(2)).abs().sqrt();
            let mut f0 = (h - r).atan2(x);
            if f0 < 0.0 {
                f0 += std::f64::consts::TAU;
            }
            let f1 = 1.5 * std::f64::consts::PI;
            (0..=ARC_STEPS).map(|k| f0 + (f1 - f0) * k as f64 / ARC_STEPS as f64).map(|f| [r * f.cos(), r + r * f.sin()]).collect()
        }
        _ => vec![[0.0, d], [0.0, 0.0]],
    }
}

/// Laser arc leads (rule AL9), lead-in in
/// the local frame. Type 13: a straight of D, then an arc of R sweeping 90° − A
/// that arrives along +x. Type 12: a straight of D parallel to the contour at
/// height 2R (coming back against the cut), then a half circle down to the start.
pub fn local_laser_in(kind: u8, d: f64, a: f64, r: f64) -> Vec<P> {
    let pi = std::f64::consts::PI;
    if kind == 12 {
        let mut v = vec![[d, 2.0 * r], [0.0, 2.0 * r]];
        v.extend((1..=ARC_STEPS).map(|k| 0.5 * pi + pi * k as f64 / ARC_STEPS as f64).map(|f| [r * f.cos(), r + r * f.sin()]));
        v
    } else {
        let a = a.to_radians().clamp(0.0, 0.5 * pi);
        let (f0, f1) = (pi + a, 1.5 * pi);
        let arc: Vec<P> = (0..=ARC_STEPS).map(|k| f0 + (f1 - f0) * k as f64 / ARC_STEPS as f64).map(|f| [r * f.cos(), r + r * f.sin()]).collect();
        let s = arc[0];
        let mut v = vec![[s[0] - d * a.sin(), s[1] + d * a.cos()]];
        v.extend(arc);
        v
    }
}

/// Mirror a local lead-in into a lead-out (x → −x, reversed: from the origin out).
fn mirror_out(v: Vec<P>) -> Vec<P> {
    v.into_iter().rev().map(|p| [-p[0], p[1]]).collect()
}

fn to_world(local: &[P], o: P, t: P, n: P) -> Vec<P> {
    local.iter().map(|p| [o[0] + t[0] * p[0] + n[0] * p[1], o[1] + t[1] * p[0] + n[1] * p[1]]).collect()
}

fn rotate(v: P, deg: f64) -> P {
    let (s, c) = deg.to_radians().sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// Build the leads for a contour starting at its lead point (rules AL7–AL9:
/// types 1–9, corner type 10, odd corner type 11, laser types 12/13). `circle` = a round contour's centre and radius: its leads use
/// the true tangent there; `limit` = keep a round hole's lead short of its centre.
pub fn build(pts: &[P], inner: bool, rec: &LeadRec, circle: Option<(P, f64)>, limit: bool, tol: f64) -> LeadGeo {
    let mut r = *rec;
    // Types 14–16 (special round-hole leads) are not decoded: built as type 1.
    if (14..=16).contains(&r.kind) {
        r.kind = 1;
    }
    if let (true, true, Some((_, hr))) = (inner, limit, circle) {
        limit_to_centre(&mut r, hr);
    }
    let start = pts[0];
    let per = crate::nest::perimeter(pts);
    let ccw = signed_area2(pts) >= 0.0;
    // Cutting direction and scrap normal at a point (true tangent on a circle).
    let frame = |p: P, t_poly: P| -> (P, P) {
        if let Some((c, _)) = circle {
            let u = norm([p[0] - c[0], p[1] - c[1]]);
            let t = if ccw { [-u[1], u[0]] } else { [u[1], -u[0]] };
            (t, if inner { [-u[0], -u[1]] } else { u })
        } else {
            (t_poly, scrap_normal(pts, inner, t_poly))
        }
    };
    let (t_first, n_first) = frame(start, dir_at(pts, 0.0, false));
    let (t_last, n_last) = frame(start, dir_at(pts, 0.0, true));
    let out_frame = |ov: f64| -> (P, P, P) {
        let o = point_at(pts, ov);
        let (t, n) = if ov < 1e-9 { (t_last, n_last) } else { frame(o, dir_at(pts, ov, true)) };
        (o, t, n)
    };
    // No lead-out at or under 2·tol (AL8).
    let out_ok = |d: f64| d > 2.0 * tol;
    match r.kind {
        0 => LeadGeo { lead_in: vec![start], lead_out: vec![], overlap: r.overlap.clamp(0.0, per / 2.0) },
        10 => {
            // Corner (AL7): the lead-in extends the first side backwards past the
            // corner; the lead-out extends the last side forwards. No overlap (our choice).
            let lin = vec![[start[0] - t_first[0] * r.d_in, start[1] - t_first[1] * r.d_in], start];
            let lout = if out_ok(r.d_out) { vec![start, [start[0] + t_last[0] * r.d_out, start[1] + t_last[1] * r.d_out]] } else { vec![] };
            LeadGeo { lead_in: lin, lead_out: lout, overlap: 0.0 }
        }
        11 => {
            // Odd corner (AL7): along the scrap-side bisector, turned by A (lead-in
            // back against the cut, lead-out forward — our reading); the bisector
            // itself when A would leave the corner's opening.
            let bis = norm([n_first[0] + n_last[0], n_first[1] + n_last[1]]);
            let ccw_turn = |v: P, toward: P| if v[0] * toward[1] - v[1] * toward[0] >= 0.0 { 1.0 } else { -1.0 };
            // Opening on the scrap side: 180° on a straight point.
            let open = 360.0 - material_angle(pts, 0, inner);
            let dir = |a: f64, toward: P| if a.abs() >= open / 2.0 { bis } else { rotate(bis, a * ccw_turn(bis, toward)) };
            let back = [-t_first[0], -t_first[1]];
            let di = dir(r.a_in, back);
            let lin = vec![[start[0] + di[0] * r.d_in, start[1] + di[1] * r.d_in], start];
            let dout = dir(r.a_out, t_last);
            let lout = if out_ok(r.d_out) { vec![start, [start[0] + dout[0] * r.d_out, start[1] + dout[1] * r.d_out]] } else { vec![] };
            LeadGeo { lead_in: lin, lead_out: lout, overlap: 0.0 }
        }
        12 | 13 => {
            let ov = r.overlap.clamp(0.0, per / 2.0);
            let lin = to_world(&local_laser_in(r.kind, r.d_in, r.a_in, r.r_in), start, t_first, n_first);
            let lout = if out_ok(r.r_out) || out_ok(r.d_out) {
                let (o, t, n) = out_frame(ov);
                to_world(&mirror_out(local_laser_in(r.kind, r.d_out, r.a_out, r.r_out)), o, t, n)
            } else {
                vec![]
            };
            LeadGeo { lead_in: lin, lead_out: lout, overlap: ov }
        }
        _ => {
            let (fi, fo) = r.forms();
            let ov = r.overlap.clamp(0.0, per / 2.0);
            let lin = if r.d_in > 0.0 { to_world(&local_lead_in(fi, r.d_in, r.a_in, r.r_in, tol), start, t_first, n_first) } else { vec![start] };
            let has_out = out_ok(r.d_out) && (fo != 2 || out_ok(r.r_out));
            let lout = if has_out {
                let (o, t, n) = out_frame(ov);
                to_world(&mirror_out(local_lead_in(fo, r.d_out, r.a_out, r.r_out, tol)), o, t, n)
            } else {
                vec![]
            };
            LeadGeo { lead_in: lin, lead_out: lout, overlap: ov }
        }
    }
}

// ---------------------------------------------------------------------------
// Collision test of automatic leads (B185–B187)
// ---------------------------------------------------------------------------

fn segs_cross(a: P, b: P, c: P, d: P) -> bool {
    let o = |p: P, q: P, r: P| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) && d1 != 0.0 && d2 != 0.0 && d3 != 0.0 && d4 != 0.0
}

/// Does a lead path cross any of the contours? The end that sits on its own
/// contour (`anchor`) is trimmed by `trim` so the lead's own foot doesn't count.
pub fn path_hits(path: &[P], anchor: P, polys: &[&[P]], trim: f64) -> bool {
    if path.len() < 2 {
        return false;
    }
    // Trim `trim` mm off the end touching the anchor (along the path).
    let mut v = path.to_vec();
    if dist(v[v.len() - 1], anchor) >= dist(v[0], anchor) {
        v.reverse();
    }
    let mut left = trim;
    while v.len() >= 2 {
        let (a, b) = (v[v.len() - 1], v[v.len() - 2]);
        let l = dist(a, b);
        if l <= left {
            left -= l;
            v.pop();
        } else {
            let f = left / l;
            let k = v.len() - 1;
            v[k] = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
            break;
        }
    }
    for w in v.windows(2) {
        let lb = [w[0][0].min(w[1][0]), w[0][1].min(w[1][1]), w[0][0].max(w[1][0]), w[0][1].max(w[1][1])];
        for poly in polys {
            let n = poly.len();
            for k in 0..n {
                let (c, d) = (poly[k], poly[(k + 1) % n]);
                if c[0].max(d[0]) < lb[0] || c[0].min(d[0]) > lb[2] || c[1].max(d[1]) < lb[1] || c[1].min(d[1]) > lb[3] {
                    continue;
                }
                if segs_cross(w[0], w[1], c, d) {
                    return true;
                }
            }
        }
    }
    false
}

/// Scale a lead toward its anchor point (shortening by a share, as the original does).
pub fn scale_path(path: &[P], anchor: P, f: f64) -> Vec<P> {
    path.iter().map(|p| [anchor[0] + (p[0] - anchor[0]) * f, anchor[1] + (p[1] - anchor[1]) * f]).collect()
}

pub fn path_len(path: &[P]) -> f64 {
    path.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Shorten a lead in steps of `step` (B186: share cut off per step) until
/// `clear` says it is free, never below `floor` = max(2·tol, min lead length)
/// (rule AL11, mode 1). Returns the shortened path.
pub fn shorten_until(path: &[P], anchor: P, step: f64, floor: f64, clear: impl Fn(&[P]) -> bool) -> Vec<P> {
    let l0 = path_len(path);
    if !l0.is_finite() || l0 <= floor || clear(path) {
        return path.to_vec();
    }
    let step = if step > 0.0 && step < 1.0 { step } else { 0.1 };
    let mut f = 1.0;
    loop {
        f *= 1.0 - step;
        if l0 * f <= floor {
            return scale_path(path, anchor, floor / l0);
        }
        let p = scale_path(path, anchor, f);
        if clear(&p) {
            return p;
        }
    }
}

/// What a B185 mode does (rule AL11): move the lead along the contour
/// (2, 3, 5, 6) and/or shorten it (1, 3, 6, 7). Mode 4 rebuilds the lead from a
/// table that isn't decoded: skipped.
pub fn test_steps(mode: i32) -> (bool, bool) {
    (matches!(mode, 2 | 3 | 5 | 6), matches!(mode, 1 | 3 | 6 | 7))
}

#[cfg(test)]
mod lead_tests {
    use super::*;

    #[test]
    fn every_sample_shape_gets_its_machine_leads() {
        use crate::nest::*;
        for sh in Shape::ALL {
            let parts = vec![Part { name: "P".into(), shape: sh, w: 150.0, h: 120.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
            let pl = Placed { part: 0, x: 10.0, y: 10.0, w: 150.0, h: 120.0, turns: 0, mirror: false };
            let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
            let plan = crate::cnc::plan(&parts, &[pl], &spec);
            let (o, holes) = placed_shape(&parts, &pl);
            for c in &plan.contours {
                assert!(c.lead_path.len() >= 2, "{sh:?}: every contour has a lead");
                assert!(c.lead_path.iter().chain(c.lead_out_path.iter()).all(|p| p[0].is_finite() && p[1].is_finite()));
                // The pierce is in the scrap: outside the part or inside a hole.
                let pierce = c.lead_in;
                assert!(!point_in_poly(&o, pierce) || holes.iter().any(|h| point_in_poly(h, pierce)), "{sh:?} {pierce:?}");
            }
        }
    }

    fn rect(w: f64, h: f64) -> Vec<P> {
        vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
    }

    #[test]
    fn contours_sort_into_round_rectangle_slot_and_other_classes() {
        let c = crate::nest::circle(50.0, 50.0, 10.0, 24);
        assert_eq!(classify(&c, true).0, Class::RoundHole);
        assert_eq!(classify(&c, false).0, Class::RoundPart);
        assert!((classify(&c, true).1.unwrap().1 - 10.0).abs() < 0.1);
        assert_eq!(classify(&rect(100.0, 50.0), true).0, Class::RectHole);
        assert_eq!(classify(&rect(100.0, 50.0), false).0, Class::RectPart);
        let slot = crate::nest::Part { name: "S".into(), shape: crate::nest::Shape::SlottedPlate, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }.geometry().holes[0].clone();
        assert_eq!(classify(&slot, true).0, Class::SlotHole);
        // A slot-shaped outer contour goes the general way (AL3).
        assert_eq!(classify(&slot, false).0, Class::General);
        let l = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 30.0], [30.0, 30.0], [30.0, 80.0], [0.0, 80.0]];
        assert_eq!(classify(&l, false).0, Class::General);
    }

    #[test]
    fn quality_one_reads_the_b_block_and_quality_two_the_e_block() {
        let t = "B156 0.5\nB200 3\nB209 10\nB232 4.0\nB233 0.0\nE000 7\nE009 1\nE032 9.0\nE033 6.0\nE817 1.25\nE825 99999.999\nB185 1\nB186 0.2\nB187 5\n";
        let lt = LeadTable::from_codes(&crate::ct1::parse(t));
        let q1 = lt.quality(1);
        assert_eq!((q1.zone, q1.recs[3].kind, q1.recs[3].d_in, q1.recs[3].d_out), (3, 10, 4.0, 0.0));
        // E814 + class 3 = quality 1's own overlap for rectangular parts.
        assert_eq!(q1.recs[3].overlap, 1.25);
        // Untouched records keep the built-in laser values; unset overlap → B156.
        assert_eq!((q1.recs[0].kind, q1.recs[0].d_in, q1.recs[0].overlap), (13, 7.0, 0.5));
        let q2 = lt.quality(2);
        assert_eq!((q2.zone, q2.recs[3].kind, q2.recs[3].d_in, q2.recs[3].d_out, q2.recs[3].overlap), (7, 1, 9.0, 6.0, 0.5));
        // Quality 3 has no E071 block: a copy of quality 1.
        assert_eq!(lt.quality(3).recs[3].d_in, 4.0);
        // Outside 1–12: quality 1.
        assert_eq!(lt.quality(0).zone, 3);
        assert_eq!((lt.test, lt.shorten, lt.tries), (1, 0.2, 5));
        // Hole zone −1 takes the outer zone; min element ≥ |B156| + 4·tol.
        assert_eq!((q1.zone_for(true, None), q1.zone_for(true, Some(9)), q1.zone_for(false, Some(9))), (3, 9, 9));
        assert!(q1.min_len >= 0.5 + 4.0 * LIN_TOL);
        assert_eq!(quality_number(CutQuality::Normal), 1);
        assert_eq!(quality_number(CutQuality::Fine), 2);
    }

    #[test]
    fn lead_types_split_into_in_and_out_forms() {
        let f = |k| LeadRec::new(k, (1.0, 1.0), (45.0, 45.0), (1.0, 1.0)).forms();
        assert_eq!([f(1), f(2), f(3), f(5), f(9), f(10)], [(0, 0), (0, 1), (0, 2), (1, 1), (2, 2), (0, 0)]);
    }

    #[test]
    fn type_1_is_perpendicular_type_5_inclined_type_9_tangent_arcs() {
        // Bottom side of a 100 × 50 rectangle, cut left to right from (50, 0).
        let pts = vec![[50.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0], [0.0, 0.0]];
        let g = build(&pts, false, &LeadRec { overlap: 0.0, ..LeadRec::new(1, (3.0, 2.0), (45.0, 45.0), (1.0, 1.0)) }, None, false, LIN_TOL);
        assert_eq!(g.lead_in, vec![[50.0, -3.0], [50.0, 0.0]]);
        assert_eq!(g.lead_out, vec![[50.0, 0.0], [50.0, -2.0]]);
        let g = build(&pts, false, &LeadRec { overlap: 0.0, ..LeadRec::new(5, (3.0, 0.0), (45.0, 45.0), (1.0, 1.0)) }, None, false, LIN_TOL);
        // Height D, then back along the cut by D / tan A: (47, −3).
        assert!((g.lead_in[0][0] - 47.0).abs() < 1e-9 && (g.lead_in[0][1] + 3.0).abs() < 1e-9);
        // D out = 0: no lead-out (AL8).
        assert!(g.lead_out.is_empty());
        let g = build(&pts, false, &LeadRec { overlap: 0.0, ..LeadRec::new(9, (4.0, 4.0), (0.0, 0.0), (2.0, 2.0)) }, None, false, LIN_TOL);
        // Tangent arc: h = min(D, 2R) = 4 → a half circle from (50, −4) round to (50, 0).
        assert!((g.lead_in[0][0] - 50.0).abs() < 1e-9 && (g.lead_in[0][1] + 4.0).abs() < 1e-9);
        assert!(g.lead_in.iter().all(|p| p[1] <= 1e-9 && p[0] <= 50.0 + 1e-9), "arc on the scrap side, behind the start");
        assert!((dist(g.lead_in[4], [48.0, -2.0])) < 1e-9);
    }

    #[test]
    fn corner_lead_extends_the_first_side_backwards() {
        // Corrected: type 10 is a straight extending the first side (AL7), not the bisector.
        let pts = rect(100.0, 50.0);
        let g = build(&pts, false, &LeadRec::new(10, (7.0, 5.0), (45.0, 45.0), (1.0, 1.0)), None, false, LIN_TOL);
        assert_eq!(g.lead_in, vec![[-7.0, 0.0], [0.0, 0.0]]);
        // The last side runs down the left edge: extended forwards to (0, −5).
        assert_eq!(g.lead_out, vec![[0.0, 0.0], [0.0, -5.0]]);
        assert_eq!(g.overlap, 0.0);
    }

    #[test]
    fn round_holes_lead_from_the_zone_angle_and_stop_short_of_the_centre() {
        let c = crate::nest::circle(50.0, 50.0, 10.0, 48);
        let lt = LeadTable::default();
        let q = lt.quality(1);
        let (class, circle) = classify(&c, true);
        let p = place(&c, true, class, circle, &q, &q.recs, 2, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        // Zone 2 → 270°: the bottom of the hole.
        assert!(dist(p.at, [50.0, 40.0]) < 1e-9);
        let pts = crate::cnc::start_at_point(c.clone(), p.at);
        // Type 1 with D 15 ≥ r → D = r: the pierce is the centre.
        let g = build(&pts, true, &LeadRec { overlap: 0.0, ..LeadRec::new(1, (15.0, 0.0), (45.0, 45.0), (9.0, 9.0)) }, circle, true, LIN_TOL);
        assert!(dist(g.lead_in[0], [50.0, 50.0]) < 1e-6, "{:?}", g.lead_in[0]);
        // Type 1 with D 3 < r: a 3 mm straight towards the centre.
        let g = build(&pts, true, &LeadRec { overlap: 0.0, ..LeadRec::new(1, (3.0, 0.0), (45.0, 45.0), (9.0, 9.0)) }, circle, true, LIN_TOL);
        assert!(dist(g.lead_in[0], [50.0, 43.0]) < 1e-6);
        // Type 13 (laser): D ≤ √(r² − 2rR) with R ≤ r/3; the whole lead stays in the hole.
        let mut r13 = LeadRec::new(13, (7.0, 5.0), (0.0, 0.0), (7.5, 5.5));
        limit_to_centre(&mut r13, 10.0);
        assert!((r13.r_in - 10.0 / 3.0).abs() < 1e-9 && (r13.d_in - (100.0f64 - 20.0 * 10.0 / 3.0).sqrt()).abs() < 1e-9);
        let g = build(&pts, true, &LeadRec::new(13, (7.0, 5.0), (0.0, 0.0), (7.5, 5.5)), circle, true, LIN_TOL);
        assert!(g.lead_in.iter().chain(g.lead_out.iter()).all(|q| dist(*q, [50.0, 50.0]) <= 10.0 + 1e-6));
    }

    #[test]
    fn rectangles_take_the_zone_corner_or_the_nearest_side_middle() {
        let pts = rect(200.0, 100.0);
        let q = LeadTable::default().quality(1);
        let p = place(&pts, false, Class::RectPart, None, &q, &q.recs, 9, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        assert_eq!((p.at, p.corner), ([200.0, 100.0], true));
        let mut recs = q.recs;
        recs[3].kind = 1;
        recs[3].overlap = 0.0;
        let p = place(&pts, false, Class::RectPart, None, &q, &recs, 9, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        // The right side's middle (200, 50) is nearest the top-right zone point.
        assert_eq!((p.at, p.corner), ([200.0, 50.0], false));
    }

    #[test]
    fn other_shapes_try_a_valid_corner_then_an_element_middle() {
        // An L: its convex corners are 90°, the inside corner is 270° in the material.
        let l = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 30.0], [30.0, 30.0], [30.0, 80.0], [0.0, 80.0]];
        let mut q = LeadTable::default().quality(1);
        let p = place(&l, false, Class::General, None, &q, &q.recs.clone(), 1, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        assert_eq!((p.at, p.rec, p.corner), ([0.0, 0.0], 5, true));
        // Corners above the max angle are refused: contour 2 (type 11, odd corner
        // 45–135° on the scrap side) takes the inside corner.
        q.corner_max = 80.0;
        let p = place(&l, false, Class::General, None, &q, &q.recs.clone(), 1, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        assert_eq!((p.at, p.rec, p.corner), ([30.0, 30.0], 6, true));
        // Contour 2 off too: contour 3 (type 5) goes in the middle of the nearest element.
        let mut recs = q.recs;
        recs[6].kind = 0;
        recs[7].overlap = 0.0;
        let p = place(&l, false, Class::General, None, &q, &recs, 1, [0.0, 0.0], LIN_TOL, ANG_TOL).unwrap();
        assert_eq!((p.rec, p.corner), (7, false));
        assert!(dist(p.at, [50.0, 0.0]) < 1e-9 || dist(p.at, [0.0, 40.0]) < 1e-9, "{:?}", p.at);
        // Contour 1 at type 0: no lead at all.
        recs[5].kind = 0;
        assert!(place(&l, false, Class::General, None, &q, &recs, 1, [0.0, 0.0], LIN_TOL, ANG_TOL).is_none());
    }

    #[test]
    fn a_colliding_lead_is_shortened_but_not_below_the_floor() {
        let path = vec![[0.0, 10.0], [0.0, 0.0]];
        let wall: Vec<P> = vec![[-5.0, 4.0], [5.0, 4.0], [5.0, 3.9], [-5.0, 3.9]];
        let polys: Vec<&[P]> = vec![&wall];
        assert!(path_hits(&path, [0.0, 0.0], &polys, 0.1));
        let s = shorten_until(&path, [0.0, 0.0], 0.1, 0.02, |p| !path_hits(p, [0.0, 0.0], &polys, 0.1));
        assert!(path_len(&s) < 3.9 && path_len(&s) > 3.0, "{s:?}");
        let s = shorten_until(&path, [0.0, 0.0], 0.1, 5.0, |p| !path_hits(p, [0.0, 0.0], &polys, 0.1));
        assert!((path_len(&s) - 5.0).abs() < 1e-9, "floor kept");
        assert_eq!(test_steps(1), (false, true));
        assert_eq!(test_steps(2), (true, false));
        assert_eq!(test_steps(3), (true, true));
        assert_eq!(test_steps(4), (false, false));
    }
}
