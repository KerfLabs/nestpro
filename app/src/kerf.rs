//! Kerf (cutter-radius) compensation. See MECHANICS.md "Kerf compensation"
//! (KC1–KC18).
//!
//! In shop terms: the beam removes a strip of metal one kerf wide, so its
//! centre has to run half a kerf (the cutter radius) on the scrap side of every
//! cut — outside a part's outline, inside a hole. There are two ways:
//!
//! * **control compensation** — the program keeps the drawn outline and tells
//!   the CNC the side (G41 left / G42 right / G40 off) and a corrector number:
//!   the CNC's tool-table slot that holds the radius;
//! * **system compensation** — the program itself carries the offset path
//!   (no G41/G42); then corners, tiny holes, narrow slots, micro-joints, leads
//!   and the pierce point are dealt with here.
//!
//! A machine may use either or both. Everything here is a plain function of
//! the contour and the machine's values, so every post and every view (drawing,
//! simulation, times, program) follows the same path.

use crate::nest::P;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Small vector helpers
// ---------------------------------------------------------------------------

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
fn unit(a: P) -> P {
    let l = (a[0] * a[0] + a[1] * a[1]).sqrt();
    if l < 1e-15 {
        [0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l]
    }
}
/// Twice the signed area: positive = counter-clockwise (Y up).
pub fn signed_area2(pts: &[P]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| cross(pts[i], pts[(i + 1) % n])).sum()
}
/// Where the lines p + t·d and q + u·e cross (None when parallel).
fn line_x(p: P, d: P, q: P, e: P) -> Option<P> {
    let c = cross(d, e);
    if c.abs() < 1e-12 {
        return None;
    }
    let t = cross(sub(q, p), e) / c;
    Some(add(p, mul(d, t)))
}

// ---------------------------------------------------------------------------
// The side (KC2)
// ---------------------------------------------------------------------------

/// Which side of the cutting direction the beam's centre runs on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    #[default]
    Off,
}

impl Side {
    /// The ISO word: G41 left, G42 right, G40 off.
    pub fn word(self) -> &'static str {
        match self {
            Side::Left => "G41",
            Side::Right => "G42",
            Side::Off => "G40",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Off => "off",
        }
    }
    /// Left ↔ right (a negative radius in the quality table, KC5).
    pub fn flipped(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Off => Side::Off,
        }
    }
}

/// KC2: the scrap side from the contour's real cutting direction on the
/// sheet. An outside cut counter-clockwise has the scrap on its right (G42),
/// clockwise on its left (G41); a hole the other way round (counter-clockwise
/// → left, clockwise → right). An open path (marking) gets no compensation.
/// Mirroring a part or reversing a contour flips the direction and therefore
/// the side.
pub fn side_for(pts: &[P], inner: bool, closed: bool) -> Side {
    if !closed || pts.len() < 3 {
        return Side::Off;
    }
    let a = signed_area2(pts);
    if a.abs() < 1e-12 {
        return Side::Off;
    }
    match (inner, a > 0.0) {
        (false, true) | (true, false) => Side::Right,
        (false, false) | (true, true) => Side::Left,
    }
}

// ---------------------------------------------------------------------------
// The machine's values (KC1, KC3, KC5, KC10, KC13, KC14)
// ---------------------------------------------------------------------------

/// How the system offset shapes an outside corner (A017, units digit, KC10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerMode {
    /// An arc of the cutter radius around the corner (0).
    #[default]
    Round,
    /// The corner arc replaced by the line tangent at its middle (1).
    Chamfer,
    /// The two offset elements run on to where they meet (2 and higher).
    Sharp,
}

impl CornerMode {
    pub fn from_code(code: i32) -> CornerMode {
        match code.rem_euclid(10) {
            0 => CornerMode::Round,
            1 => CornerMode::Chamfer,
            _ => CornerMode::Sharp,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CornerMode::Round => "round corners",
            CornerMode::Chamfer => "chamfered corners",
            CornerMode::Sharp => "sharp corners",
        }
    }
}

/// One cutting quality's row of the quality-dependent table (KC5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QualityComp {
    pub hole_corrector: u32,
    pub outside_corrector: u32,
    /// Cutter radius for holes / outsides, mm; negative = the other side.
    pub hole_radius: f64,
    pub outside_radius: f64,
}

/// The machine file's compensation values.
#[derive(Clone, Debug, PartialEq)]
pub struct CompSettings {
    /// KC1: the program carries the offset path (system compensation).
    pub system: bool,
    /// KC1: the control offsets (G41/G42 + corrector); on unless the file says off.
    pub control: bool,
    /// KC3: corrector numbers for holes and for outsides (a section may override).
    pub hole_corrector: u32,
    pub outside_corrector: u32,
    /// Common-cut contours switched to system compensation (read only: the app
    /// has no common cut).
    pub common_cut: u8,
    /// KC5: use the quality-dependent table.
    pub by_quality: bool,
    pub quality: [QualityComp; 12],
    /// KC5: normal, common-cut main and common-cut auxiliary contours:
    /// 0 control, 1 system, 2 both.
    pub modes: [u8; 3],
    /// KC13: leads under system compensation: 0 offset with the contour,
    /// 1 move corner leads, 2 move element leads, 3 move both.
    pub lead_mode: u8,
    /// KC14: joint leads: 0 offset with their piece, 1 moved.
    pub joint_mode: u8,
    /// KC10: corner mode code (units digit: 0 round, 1 chamfer, 2 sharp).
    pub corner_code: i32,
}

impl Default for CompSettings {
    fn default() -> Self {
        CompSettings {
            system: false,
            control: true,
            hole_corrector: 1,
            outside_corrector: 2,
            common_cut: 0,
            by_quality: false,
            quality: [QualityComp::default(); 12],
            modes: [0; 3],
            lead_mode: 0,
            joint_mode: 0,
            corner_code: 0,
        }
    }
}

/// "ON" / "OFF" or a number.
fn flag(v: &str) -> Option<i64> {
    let v = v.trim().trim_matches('"');
    if v.eq_ignore_ascii_case("ON") {
        Some(1)
    } else if v.eq_ignore_ascii_case("OFF") {
        Some(0)
    } else {
        v.parse::<f64>().ok().map(|x| x as i64)
    }
}

impl CompSettings {
    /// Read the values from the machine file's codes (a material section
    /// already laid over them).
    pub fn from_codes(m: &HashMap<String, String>) -> CompSettings {
        let d = CompSettings::default();
        let int = |k: &str| m.get(k).and_then(|v| flag(v));
        let num = |k: &str| m.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        let mut quality = [QualityComp::default(); 12];
        for (k, q) in quality.iter_mut().enumerate() {
            let code = |o: usize| format!("B{}", 700 + 4 * k + o);
            *q = QualityComp {
                hole_corrector: int(&code(0)).map_or(0, |v| v.max(0) as u32),
                outside_corrector: int(&code(1)).map_or(0, |v| v.max(0) as u32),
                hole_radius: num(&code(2)).unwrap_or(0.0),
                outside_radius: num(&code(3)).unwrap_or(0.0),
            };
        }
        let small = |k: &str, def: u8| int(k).map_or(def, |v| v.clamp(0, 255) as u8);
        CompSettings {
            system: int("B116").map_or(d.system, |v| v != 0),
            control: int("B117").map_or(d.control, |v| v != 0),
            hole_corrector: int("B124").map_or(d.hole_corrector, |v| v.max(0) as u32),
            outside_corrector: int("B125").map_or(d.outside_corrector, |v| v.max(0) as u32),
            common_cut: small("B191", d.common_cut),
            by_quality: int("B748").is_some_and(|v| v != 0),
            quality,
            modes: [small("B749", 0), small("B750", 0), small("B751", 0)],
            lead_mode: small("B752", 0),
            joint_mode: small("B753", 0),
            corner_code: int("A017").map_or(d.corner_code, |v| v as i32),
        }
    }

    pub fn corners(&self) -> CornerMode {
        CornerMode::from_code(self.corner_code)
    }

    /// KC5: quality 1–12's row; any other quality uses quality 1's.
    pub fn quality_row(&self, q: usize) -> QualityComp {
        self.quality[if (1..=12).contains(&q) { q - 1 } else { 0 }]
    }

    fn table_empty(&self) -> bool {
        self.quality.iter().all(|q| *q == QualityComp::default())
    }
}

/// The job's choice (Machining → Compensation): None = the machine file's.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KerfChoice {
    /// Control compensation (G41/G42 and the corrector) on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<bool>,
    /// System compensation (the offset path) on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<bool>,
}

/// What one contour gets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContourComp {
    /// The scrap side, from the cutting direction (KC2), flipped by a negative
    /// quality radius (KC5).
    pub side: Side,
    /// The program tells the control the side and the corrector.
    pub control: bool,
    pub corrector: u32,
    /// The plan's path is already offset by `radius`.
    pub system: bool,
    /// Cutter radius, mm (the system offset; shown for control compensation).
    pub radius: f64,
}

impl ContourComp {
    /// The side word and corrector the control must hold for this contour
    /// (None = compensation off).
    pub fn word(&self) -> Option<(Side, u32)> {
        (self.control && self.side != Side::Off).then_some((self.side, self.corrector))
    }
}

/// Below this a radius counts as zero (KC5, KC6).
pub const RADIUS_EPS: f64 = 1e-6;

/// KC1, KC3–KC6: what a cut contour gets. `quality` is the contour's cutting
/// quality number (1–12), `radius` the machine's cutter radius for the sheet's
/// material and thickness, `side` the scrap side from its direction.
///
/// * The machine's two switches (system, control) decide, unless the
///   quality-dependent table is on: then its mode for normal contours does
///   (0 control with the table's corrector, 1 system with the table's radius,
///   2 both); a negative table radius flips the side of the offset.
/// * The job's own choice wins over both (our rule: the window's toggles).
/// * Holes take the hole corrector, outsides the outside one.
/// * No side (open path) = nothing at all; a zero radius = no offset.
pub fn resolve(s: &CompSettings, choice: KerfChoice, inner: bool, quality: usize, radius: f64, side: Side) -> ContourComp {
    let mut c = ContourComp { side, control: s.control, corrector: if inner { s.hole_corrector } else { s.outside_corrector }, system: s.system, radius };
    if s.by_quality && !s.table_empty() {
        let q = s.quality_row(quality);
        let (corr, rad) = if inner { (q.hole_corrector, q.hole_radius) } else { (q.outside_corrector, q.outside_radius) };
        let mode = s.modes[0];
        c.control = mode != 1;
        c.system = mode != 0;
        // A row without a corrector keeps the machine's (our choice).
        if corr > 0 {
            c.corrector = corr;
        }
        if rad.abs() > RADIUS_EPS {
            c.radius = rad.abs();
            if rad < 0.0 && c.system {
                c.side = c.side.flipped();
            }
        }
    }
    if let Some(v) = choice.control {
        c.control = v;
    }
    if let Some(v) = choice.system {
        c.system = v;
    }
    if c.side == Side::Off {
        c.control = false;
        c.system = false;
        c.corrector = 0;
    }
    c
}

/// The machine's cutter radius for a sheet (B002, or its material section's
/// Z004), else half the fallback kerf.
pub fn cutter_radius(mt: &crate::ct1::MachineTech) -> f64 {
    mt.cutter_radius.filter(|r| *r > RADIUS_EPS).unwrap_or(crate::cutoff::KERF_MM / 2.0)
}

/// The real kerf (cut width) for a sheet's material and thickness: twice the
/// machine's cutter radius, else the fallback 0.3 mm. Used where a cut's width
/// matters: remnant and cut-off lines, the simulation.
pub fn kerf_mm(spec: &crate::nest::SheetSpec) -> f64 {
    2.0 * cutter_radius(&crate::ct1::for_spec(spec))
}

/// One line for people: "Kerf 0.30 mm · control (G41/G42, D1/D2)".
pub fn status_line(spec: &crate::nest::SheetSpec) -> String {
    let mt = crate::ct1::for_spec(spec);
    let s = &mt.comp;
    let control = spec.kerf.control.unwrap_or(s.control);
    let system = spec.kerf.system.unwrap_or(s.system);
    let r = cutter_radius(&mt);
    let src = if mt.cutter_radius.is_some() { "" } else { " (default)" };
    let mut parts = vec![];
    if s.by_quality && spec.kerf == KerfChoice::default() {
        parts.push("by cutting quality (machine table)".to_string());
    } else {
        if control {
            parts.push(format!("control (G41/G42, D{}/D{})", s.hole_corrector, s.outside_corrector));
        }
        if system {
            parts.push(format!("system offset ({})", s.corners().label()));
        }
        if parts.is_empty() {
            parts.push("off".into());
        }
    }
    format!("Kerf {:.2} mm{src} · {}", 2.0 * r, parts.join(" + "))
}

// ---------------------------------------------------------------------------
// The offset path (KC8–KC11)
// ---------------------------------------------------------------------------

/// Above this turn (degrees) a vertex is a real corner and takes the corner
/// mode; below it the vertex is part of a curve and its offsets just meet
/// (the app's element rule).
pub const CURVE_TURN_DEG: f64 = crate::nest::ELEMENT_CORNER_DEG;
/// A sharp corner further than this many cutter radii from the drawn corner
/// is chamfered instead (our limit for very acute spikes).
pub const MITER_LIMIT: f64 = 10.0;
/// Round corners are drawn with a point at least every this many degrees.
pub const ARC_STEP_DEG: f64 = 10.0;

#[derive(Clone, Copy, Debug)]
struct Seg {
    a: P,
    b: P,
    d: P,
    /// Unit normal towards the offset side.
    n: P,
    /// Index of the input point where the segment starts.
    v: usize,
}

impl Seg {
    fn start(&self, r: f64) -> P {
        add(self.a, mul(self.n, r))
    }
    fn end(&self, r: f64) -> P {
        add(self.b, mul(self.n, r))
    }
}

/// An offset path and where each input point went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offset {
    pub pts: Vec<P>,
    /// Per input point: index in `pts` of the first point made at it.
    pub at: Vec<usize>,
    /// Inside elements smaller than the cutter radius were left out (KC11).
    pub dropped: usize,
    /// A slot narrower than the kerf lost its end (KC11).
    pub narrow: bool,
}

/// How two offset segments meet: Some(point) when they cross or run on
/// (inside corner or straight on: both are trimmed there), None for an
/// outside corner (a gap the corner mode fills).
fn trim_point(a: &Seg, b: &Seg, r: f64, right: bool) -> Option<(P, bool)> {
    let c = cross(a.d, b.d);
    let toward = if right { -c } else { c };
    let (ae, bs) = (a.end(r), b.start(r));
    let mid = [(ae[0] + bs[0]) / 2.0, (ae[1] + bs[1]) / 2.0];
    // Nearly parallel: straight on, or a U-turn — towards the offset side when
    // the next segment lies on that side (the two walls of a slot).
    if c.abs() < PARALLEL_SIN {
        if dot(a.d, b.d) > 0.0 {
            return Some((mid, false));
        }
        return (dot(sub(b.a, a.b), a.n) > 1e-12).then_some((mid, true));
    }
    if toward < 0.0 {
        return None;
    }
    Some((line_x(ae, a.d, bs, b.d).unwrap_or(mid), false))
}

/// Two segments closer to parallel than about 1° count as parallel.
const PARALLEL_SIN: f64 = 0.0175;

fn turn_deg(a: P, b: P) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Offset a polyline by `r` to one side of its direction (`right`), closed or
/// open (KC8–KC12):
///
/// * every segment is moved `r` sideways;
/// * an inside corner is trimmed where the two offsets cross;
/// * an outside corner gets the corner mode — an arc of radius `r` round the
///   drawn corner, a chamfer (the line touching that arc at its middle), or
///   sharp (the offsets run on until they meet); points inside a curve just
///   meet; `sharp_at` forces a sharp corner at chosen input points;
/// * an inside element the offset swallows (an inside arc smaller than the
///   cutter radius, a slot end narrower than the kerf) is left out and its
///   neighbours joined; the two walls of a slot narrower than the kerf go too;
/// * an open path's ends are offset square to their segment;
/// * points closer than `tol` are merged.
///
/// None when a closed path collapses (a hole or slot no bigger than the kerf).
pub fn offset_path(pts: &[P], closed: bool, r: f64, right: bool, corners: CornerMode, sharp_at: &dyn Fn(usize) -> bool, tol: f64) -> Option<Offset> {
    // Segments, skipping zero-length ones.
    let n = pts.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    let mut segs: Vec<Seg> = vec![];
    for i in 0..count {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if dist(a, b) < 1e-9 {
            continue;
        }
        let d = unit(sub(b, a));
        let nr = if right { [d[1], -d[0]] } else { [-d[1], d[0]] };
        segs.push(Seg { a, b, d, n: nr, v: i });
    }
    if segs.is_empty() || (closed && segs.len() < 3) {
        return None;
    }
    // Leave out the segments the offset turns round (KC11), one at a time.
    let mut act: Vec<usize> = (0..segs.len()).collect();
    let (mut dropped, mut narrow) = (0usize, false);
    loop {
        let m = act.len();
        if (closed && m < 3) || m == 0 {
            return None;
        }
        let mut st: Vec<P> = act.iter().map(|&i| segs[i].start(r)).collect();
        let mut en: Vec<P> = act.iter().map(|&i| segs[i].end(r)).collect();
        let mut walls: Option<(usize, usize)> = None;
        for k in 0..m {
            if !closed && k == 0 {
                continue;
            }
            let p = (k + m - 1) % m;
            if let Some((x, anti)) = trim_point(&segs[act[p]], &segs[act[k]], r, right) {
                en[p] = x;
                st[k] = x;
                if anti && walls.is_none() {
                    walls = Some((p, k));
                }
            }
        }
        // Two walls closer than the kerf meeting head-on: drop the shorter.
        if let Some((p, k)) = walls {
            let len = |j: usize| dist(segs[act[j]].a, segs[act[j]].b);
            let j = if len(p) <= len(k) { p } else { k };
            act.remove(j);
            narrow = true;
            continue;
        }
        let worst = (0..m)
            .map(|k| (k, dot(sub(en[k], st[k]), segs[act[k]].d)))
            .filter(|&(_, l)| l < -1e-9)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((k, _)) = worst {
            act.remove(k);
            dropped += 1;
            continue;
        }
        break;
    }
    let m = act.len();
    let mut out: Vec<P> = vec![];
    let mut at: Vec<(usize, usize)> = vec![];
    if !closed {
        at.push((segs[act[0]].v, 0));
        out.push(segs[act[0]].start(r));
    }
    for k in if closed { 0 } else { 1 }..m {
        let (a, b) = (&segs[act[(k + m - 1) % m]], &segs[act[k]]);
        at.push((b.v, out.len()));
        if let Some((x, _)) = trim_point(a, b, r, right) {
            out.push(x);
            continue;
        }
        let (ae, bs) = (a.end(r), b.start(r));
        let v = b.a;
        if dist(a.b, b.a) > 1e-9 {
            // Neighbours of a dropped element: straight across.
            out.push(ae);
            out.push(bs);
            continue;
        }
        let real = turn_deg(a.d, b.d) > CURVE_TURN_DEG;
        let mode = if sharp_at(b.v) || !real { CornerMode::Sharp } else { corners };
        let chamfer = |out: &mut Vec<P>| {
            let s = add(a.n, b.n);
            let m = if (s[0] * s[0] + s[1] * s[1]).sqrt() < 1e-9 { a.d } else { unit(s) };
            let t = add(v, mul(m, r));
            let td = [-m[1], m[0]];
            out.push(line_x(ae, a.d, t, td).unwrap_or(ae));
            out.push(line_x(bs, b.d, t, td).unwrap_or(bs));
        };
        match mode {
            CornerMode::Sharp => match line_x(ae, a.d, bs, b.d) {
                Some(x) if dist(x, v) <= MITER_LIMIT * r.max(1e-9) => out.push(x),
                _ => chamfer(&mut out),
            },
            CornerMode::Chamfer => chamfer(&mut out),
            CornerMode::Round => {
                let a0 = a.n[1].atan2(a.n[0]);
                let a1 = b.n[1].atan2(b.n[0]);
                let tau = std::f64::consts::TAU;
                let sweep = if right { (a1 - a0).rem_euclid(tau) } else { -(a0 - a1).rem_euclid(tau) };
                let steps = ((sweep.abs().to_degrees() / ARC_STEP_DEG).ceil() as usize).max(1);
                for i in 0..=steps {
                    let t = a0 + sweep * i as f64 / steps as f64;
                    out.push([v[0] + r * t.cos(), v[1] + r * t.sin()]);
                }
            }
        }
    }
    if !closed {
        out.push(segs[act[m - 1]].end(r));
        at.push((segs[act[m - 1]].v + 1, out.len() - 1));
    }
    // Merge points closer than the tolerance (tiny elements, KC11).
    let mut keep: Vec<P> = vec![];
    let mut remap = vec![0usize; out.len()];
    for (i, &p) in out.iter().enumerate() {
        let last = !closed && i + 1 == out.len();
        match keep.last() {
            Some(&q) if dist(p, q) < tol && !last => {}
            Some(&q) if dist(p, q) < tol && last => {
                let k = keep.len() - 1;
                keep[k] = p;
            }
            _ => keep.push(p),
        }
        remap[i] = keep.len() - 1;
    }
    if closed && keep.len() > 1 && dist(keep[0], *keep.last().unwrap()) < tol {
        keep.pop();
        for x in remap.iter_mut() {
            if *x >= keep.len() {
                *x = 0;
            }
        }
    }
    // Every input point → the first output point made at it; points of
    // dropped elements → the next one made.
    let mut idx = vec![usize::MAX; n];
    for &(v, o) in &at {
        if v < n && idx[v] == usize::MAX {
            idx[v] = remap[o];
        }
    }
    if !closed && n > 0 && idx[n - 1] == usize::MAX {
        idx[n - 1] = keep.len().saturating_sub(1);
    }
    for _ in 0..2 {
        for i in (0..n).rev() {
            if idx[i] == usize::MAX {
                let next = if i + 1 < n { idx[i + 1] } else if closed { idx[0] } else { usize::MAX };
                idx[i] = next;
            }
        }
    }
    for x in idx.iter_mut() {
        if *x == usize::MAX {
            *x = 0;
        }
    }
    if closed {
        // A loop that turned inside out (or to nothing) can't be cut.
        let (a_in, a_out) = (signed_area2(pts), signed_area2(&keep));
        let inward = (a_in > 0.0) != right;
        if keep.len() < 3 || a_in.signum() != a_out.signum() || a_out.abs() < 2.0 * tol * tol || (inward && a_out.abs() >= a_in.abs()) || (!inward && a_out.abs() <= a_in.abs()) {
            return None;
        }
    }
    Some(Offset { pts: keep, at: idx, dropped, narrow })
}

// ---------------------------------------------------------------------------
// A whole cut contour under system compensation (KC12–KC15)
// ---------------------------------------------------------------------------

/// How the offset treats leads and joints.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SystemOpts {
    pub corners: CornerMode,
    /// KC13: 0 offset with the contour, 1 move corner leads, 2 move element
    /// leads, 3 move both.
    pub lead_mode: u8,
    /// KC14: 0 offset joint leads with their piece, 1 move them.
    pub joint_mode: u8,
    pub tol: f64,
}

/// Why a contour can't be cut with system compensation, or what it lost.
#[derive(Clone, Debug, PartialEq)]
pub enum Trouble {
    /// A round hole no bigger than the kerf: not cut.
    SmallHole { d: f64 },
    /// The contour is narrower than the kerf everywhere: not cut.
    Collapsed,
    /// A slot narrower than the kerf: its end isn't cut.
    NarrowSlot,
    /// Inside arcs or corners smaller than the cutter radius were cut as a corner.
    SmallInsideArcs(usize),
}

impl Trouble {
    /// Is the whole contour left uncut?
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn not_cut(&self) -> bool {
        matches!(self, Trouble::SmallHole { .. } | Trouble::Collapsed)
    }
    pub fn describe(&self, kerf: f64) -> String {
        match self {
            Trouble::SmallHole { d } => format!("round hole Ø{d:.2} mm is no bigger than the {kerf:.2} mm kerf: not cut"),
            Trouble::Collapsed => format!("narrower than the {kerf:.2} mm kerf everywhere: not cut"),
            Trouble::NarrowSlot => format!("a slot narrower than the {kerf:.2} mm kerf: its end is not cut"),
            Trouble::SmallInsideArcs(k) => format!("{k} inside arc(s)/corner(s) smaller than the cutter radius cut as a corner"),
        }
    }
}

/// Nearest point on a closed polyline: distance along it from pts[0].
fn along(pts: &[P], p: P) -> (f64, f64) {
    let n = pts.len();
    let mut best = (f64::MAX, 0.0);
    let mut acc = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let ab = sub(b, a);
        let l2 = dot(ab, ab);
        let t = if l2 > 0.0 { (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let q = add(a, mul(ab, t));
        let d = dist(p, q);
        if d < best.0 {
            best = (d, acc + l2.sqrt() * t);
        }
        acc += l2.sqrt();
    }
    (best.1, best.0)
}

/// Direction of a closed polyline at distance `d` (the segment there).
fn dir_at(pts: &[P], d: f64) -> P {
    let n = pts.len();
    let mut acc = 0.0;
    for i in 0..n {
        let l = dist(pts[i], pts[(i + 1) % n]);
        if l > 1e-12 && (acc + l >= d || i == n - 1) {
            return unit(sub(pts[(i + 1) % n], pts[i]));
        }
        acc += l;
    }
    [1.0, 0.0]
}

fn translate(path: &[P], by: P) -> Vec<P> {
    path.iter().map(|&p| add(p, by)).collect()
}

/// A lead ending on the contour, offset with the element after its end as one
/// open path: the part up to their join (KC13, KC14).
fn offset_lead_in(lead: &[P], next: P, r: f64, right: bool, o: &SystemOpts) -> Option<Vec<P>> {
    let mut p = lead.to_vec();
    p.push(next);
    let q = offset_path(&p, false, r, right, o.corners, &|_| false, o.tol)?;
    let v = lead.len() - 1;
    let end = q.at.get(v + 1).copied().unwrap_or(q.pts.len() - 1).saturating_sub(1).max(q.at[v]).min(q.pts.len() - 1);
    let head = q.pts[..=end].to_vec();
    (head.len() >= 2).then_some(head)
}

/// A lead leaving the contour, offset with the element before its start: the
/// part from their join on.
fn offset_lead_out(prev: P, lead: &[P], r: f64, right: bool, o: &SystemOpts) -> Option<Vec<P>> {
    let mut p = vec![prev];
    p.extend(lead.iter().copied());
    let q = offset_path(&p, false, r, right, o.corners, &|_| false, o.tol)?;
    let tail = q.pts[q.at[1].min(q.pts.len() - 1)..].to_vec();
    (tail.len() >= 2).then_some(tail)
}

/// KC13 "at a corner": the contour turns at its start point.
fn starts_at_corner(pts: &[P]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let a = unit(sub(pts[0], pts[n - 1]));
    let b = unit(sub(pts[1], pts[0]));
    turn_deg(a, b) > CURVE_TURN_DEG
}

/// KC12–KC15: turn a planned contour (drawn geometry) into the path the
/// beam's centre runs with system compensation, in place:
///
/// * the contour is offset by the cutter radius to the scrap side as one
///   closed loop; a micro-joint gap keeps its ends square to the edge (its
///   ends move sideways with the edge, so on a straight edge its length stays),
///   which is what offsetting each piece between gaps on its own gives;
/// * lead-in and lead-out: with lead mode 0 they are offset with the contour
///   and joined to it by the corner rules (the cut may then end a little before
///   its start, where the lead-out's offset meets the contour's); with 1/2/3 a
///   lead at a corner (1, 3) or on an element (2, 3) is moved unchanged by how
///   far its end point moved, and that corner is made sharp;
/// * joint leads likewise by the joint mode (0 offset, 1 moved);
/// * loops at corners move with their (sharp) corner;
/// * the pierce is the first point of the moved lead-in (KC15).
///
/// Err = the contour can't be cut (a hole or slot no bigger than the kerf);
/// Ok carries what it lost on the way, if anything.
pub fn compensate(c: &mut crate::cnc::Contour, comp: &ContourComp, o: &SystemOpts) -> Result<Option<Trouble>, Trouble> {
    let r = comp.radius;
    if !comp.system || r <= o.tol.max(RADIUS_EPS) || comp.side == Side::Off || c.points.len() < 3 {
        return Ok(None);
    }
    let right = comp.side == Side::Right;
    let nominal = c.points.clone();
    let n = nominal.len();
    let per0 = crate::nest::perimeter(&nominal);
    let has_lead = c.lead_path.len() >= 2;
    let lead_moves = if c.closing {
        o.joint_mode >= 1
    } else if starts_at_corner(&nominal) {
        matches!(o.lead_mode, 1 | 3)
    } else {
        matches!(o.lead_mode, 2 | 3)
    };
    let loop_at: Vec<usize> = c.loops.iter().map(|l| l.0).collect();
    let sharp = |v: usize| (v == 0 && has_lead && lead_moves) || loop_at.contains(&v);
    // A round hole no bigger than the kerf has no path (KC8).
    let inward = (signed_area2(&nominal) > 0.0) != right;
    if let (true, (_, Some((_, rad)))) = (inward, crate::leads::classify(&nominal, true)) {
        if rad <= r + o.tol {
            return Err(Trouble::SmallHole { d: 2.0 * rad });
        }
    }
    let Some(off) = offset_path(&nominal, true, r, right, o.corners, &sharp, o.tol) else {
        return Err(Trouble::Collapsed);
    };
    let lost = if off.narrow { Some(Trouble::NarrowSlot) } else if off.dropped > 0 { Some(Trouble::SmallInsideArcs(off.dropped)) } else { None };
    // The loop from the point made at the contour's start.
    let l0 = off.pts_rot(off.at[0]);
    let start = l0[0];
    let pl = crate::nest::perimeter(&l0);
    // A drawn point at distance d → r to the scrap side of its edge, and its
    // place along the offset loop.
    let map_point = |d: f64| {
        let d = d.clamp(0.0, per0);
        let t = dir_at(&nominal, d);
        let nr = if right { [t[1], -t[0]] } else { [-t[1], t[0]] };
        add(crate::cnc::point_at(&nominal, d), mul(nr, r))
    };
    let map_d = |d: f64| -> f64 {
        if d <= 1e-9 {
            return 0.0;
        }
        if d >= per0 - 1e-9 {
            return pl;
        }
        let (s, _) = along(&l0, map_point(d));
        // Keep the seam on the right side.
        let f = d / per0;
        let s = if s / pl - f > 0.5 { s - pl } else if f - s / pl > 0.5 { s + pl } else { s };
        s.clamp(0.0, pl)
    };
    // Lead-in: moved, or offset with the contour's first element.
    let mut new_start = start;
    if has_lead {
        let offset_lead = if lead_moves {
            None
        } else {
            offset_lead_in(&c.lead_path, nominal[1 % n], r, right, o)
        };
        match offset_lead {
            Some(p) => {
                new_start = *p.last().unwrap();
                c.lead_path = p;
            }
            None => c.lead_path = translate(&c.lead_path, sub(start, nominal[0])),
        }
    }
    // Start the loop where the lead-in joins it.
    let (s0, _) = along(&l0, new_start);
    let l = if dist(new_start, start) > 1e-9 { crate::cnc::start_at_point(l0.clone(), new_start) } else { l0.clone() };
    let shift = |s: f64| if s >= pl - 1e-9 { pl } else { (s - s0).rem_euclid(pl.max(1e-12)) };
    // Joints: their ends move sideways with the edge.
    let old_joints = c.joints.clone();
    let mut joints: Vec<(f64, f64)> = old_joints
        .iter()
        .map(|&(a, b)| {
            let a2 = shift(map_d(a));
            let b2 = shift(map_d(b));
            (a2, if b2 > a2 { b2 } else { (a2 + (b - a)).min(pl) })
        })
        .collect();
    // Joint leads (KC14).
    let mut leads = std::mem::take(&mut c.joint_leads);
    let n_leads = leads.len();
    for (k, (out, back)) in leads.iter_mut().enumerate() {
        let Some(&(a, b)) = old_joints.get(k) else { continue };
        let (pa, pb) = (crate::cnc::point_at(&nominal, a), crate::cnc::point_at(&nominal, b));
        let moved = o.joint_mode >= 1;
        if out.len() >= 2 {
            let by = sub(map_point(a), pa);
            let off_out = (!moved)
                .then(|| {
                    let prev = crate::cnc::point_at(&nominal, (a - 1.0_f64.min(a / 2.0).max(1e-3)).rem_euclid(per0));
                    offset_lead_out(prev, out, r, right, o)
                })
                .flatten();
            *out = off_out.unwrap_or_else(|| translate(out, by));
            joints[k].0 = along(&l, out[0]).0;
        }
        if c.closing && k + 1 == n_leads {
            // The closing joint's lead-in is the contour's lead-in, done above.
            *back = c.lead_path.clone();
            joints[k].1 = pl;
        } else if back.len() >= 2 {
            let by = sub(map_point(b), pb);
            let off_back = (!moved)
                .then(|| {
                    let next = crate::cnc::point_at(&nominal, (b + 1.0_f64.min((per0 - b) / 2.0).max(1e-3)).rem_euclid(per0));
                    offset_lead_in(back, next, r, right, o)
                })
                .flatten();
            *back = off_back.unwrap_or_else(|| translate(back, by));
            let e = along(&l, *back.last().unwrap()).0;
            joints[k].1 = if b >= per0 - 1e-9 { pl } else { e };
        }
        if joints[k].1 <= joints[k].0 {
            joints[k].1 = (joints[k].0 + (b - a)).min(pl);
        }
    }
    c.joint_leads = leads;
    c.joints = joints;
    // Lead-out (KC13) and the overlap before it.
    if !c.closing {
        let ov0 = c.overlap.max(0.0);
        let q = c.lead_out_path.first().copied().unwrap_or(nominal[0]);
        let s_q = if ov0 > 1e-9 { shift(map_d(ov0)) } else { 0.0 };
        let mut end_at = s_q;
        if c.lead_out_path.len() >= 2 {
            let off_out = (!lead_moves)
                .then(|| {
                    let prev = if ov0 > 1e-9 { crate::cnc::point_at(&nominal, (ov0 - 1.0_f64.min(ov0 / 2.0)).max(0.0)) } else { nominal[n - 1] };
                    offset_lead_out(prev, &c.lead_out_path, r, right, o)
                })
                .flatten();
            match off_out {
                Some(p) => {
                    end_at = along(&l, p[0]).0;
                    c.lead_out_path = p;
                }
                None => {
                    let to = crate::cnc::point_at(&l, s_q);
                    c.lead_out_path = translate(&c.lead_out_path, sub(to, q));
                }
            }
        }
        // The cut may stop short of its start (negative overlap).
        c.overlap = if ov0 <= 1e-9 && end_at > pl / 2.0 { end_at - pl } else { end_at };
        if c.overlap.abs() < 1e-9 {
            c.overlap = 0.0;
        }
        if c.lead_out.is_some() {
            c.lead_out = c.lead_out_path.last().copied();
        }
    }
    // Loops move with their corner.
    let loops = std::mem::take(&mut c.loops);
    c.loops = loops
        .into_iter()
        .filter_map(|(vi, pts)| {
            let corner = off.pts.get(*off.at.get(vi)?)?;
            let k = l.iter().position(|p| dist(*p, *corner) < 1e-9)?;
            Some((k, translate(&pts, sub(*corner, nominal[vi]))))
        })
        .collect();
    c.points = l;
    c.lead_in = c.lead_path.first().copied().unwrap_or(c.points[0]);
    Ok(lost)
}

impl Offset {
    /// The offset loop starting at output point `k`.
    fn pts_rot(&self, k: usize) -> Vec<P> {
        let mut v = self.pts.clone();
        if !v.is_empty() {
            let k = k.min(v.len() - 1);
            v.rotate_left(k);
        }
        v
    }
}

#[cfg(test)]
mod kerf_tests {
    use super::*;

    fn near(a: P, b: P) -> bool {
        dist(a, b) < 1e-6
    }
    fn rect(w: f64, h: f64) -> Vec<P> {
        vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
    }
    fn circle(c: P, r: f64, n: usize, ccw: bool) -> Vec<P> {
        let mut v: Vec<P> = (0..n).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        }).collect();
        if !ccw {
            v.reverse();
        }
        v
    }

    #[test]
    fn the_scrap_side_follows_the_cutting_direction() {
        let ccw = rect(100.0, 50.0);
        let mut cw = ccw.clone();
        cw.reverse();
        assert_eq!(side_for(&ccw, false, true), Side::Right, "outside counter-clockwise → G42");
        assert_eq!(side_for(&cw, false, true), Side::Left, "outside clockwise → G41");
        assert_eq!(side_for(&ccw, true, true), Side::Left, "hole counter-clockwise → G41");
        assert_eq!(side_for(&cw, true, true), Side::Right, "hole clockwise → G42");
        assert_eq!(side_for(&ccw, false, false), Side::Off, "marking line → no compensation");
        assert_eq!((Side::Left.word(), Side::Right.word(), Side::Off.word()), ("G41", "G42", "G40"));
    }

    #[test]
    fn correctors_come_from_the_machine_or_its_material_section() {
        let s = CompSettings::default();
        let hole = resolve(&s, KerfChoice::default(), true, 1, 0.12, Side::Left);
        let outside = resolve(&s, KerfChoice::default(), false, 1, 0.12, Side::Right);
        assert_eq!((hole.control, hole.corrector, hole.system), (true, 1, false));
        assert_eq!((outside.word(), outside.system), (Some((Side::Right, 2)), false));
        let m: HashMap<String, String> = [("B116", "ON"), ("B117", "OFF"), ("B124", "7"), ("B125", "8"), ("A017", "12")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let s = CompSettings::from_codes(&m);
        assert!(s.system && !s.control && s.corners() == CornerMode::Sharp);
        let c = resolve(&s, KerfChoice::default(), true, 1, 0.2, Side::Right);
        assert_eq!((c.control, c.system, c.corrector, c.radius), (false, true, 7, 0.2));
        // The job's own choice wins; a marking line never gets a word.
        let c = resolve(&s, KerfChoice { control: Some(true), system: Some(false) }, false, 1, 0.2, Side::Left);
        assert_eq!(c.word(), Some((Side::Left, 8)));
        assert!(!c.system);
        assert_eq!(resolve(&s, KerfChoice::default(), false, 1, 0.2, Side::Off).word(), None);
    }

    #[test]
    fn the_quality_table_picks_corrector_and_radius_and_a_negative_radius_flips_the_side() {
        let mut m: HashMap<String, String> = HashMap::new();
        for (k, v) in [("B748", "ON"), ("B749", "1"), ("B704", "3"), ("B705", "4"), ("B706", "0.10"), ("B707", "-0.15")] {
            m.insert(k.into(), v.into());
        }
        let s = CompSettings::from_codes(&m);
        // Quality 2: holes offset 0.10 on their side, outsides 0.15 on the other side.
        let h = resolve(&s, KerfChoice::default(), true, 2, 0.12, Side::Left);
        assert_eq!((h.system, h.control, h.radius, h.side), (true, false, 0.10, Side::Left));
        let o = resolve(&s, KerfChoice::default(), false, 2, 0.12, Side::Right);
        assert_eq!((o.system, o.radius, o.side), (true, 0.15, Side::Left));
        // Mode 0: control with the table's correctors; quality 1 has none → the machine's.
        m.insert("B749".into(), "0".into());
        let s = CompSettings::from_codes(&m);
        assert_eq!(resolve(&s, KerfChoice::default(), true, 2, 0.12, Side::Left).word(), Some((Side::Left, 3)));
        assert_eq!(resolve(&s, KerfChoice::default(), false, 1, 0.12, Side::Right).word(), Some((Side::Right, 2)));
        // A quality past 12 uses quality 1's row.
        assert_eq!(s.quality_row(13), s.quality_row(1));
    }

    #[test]
    fn an_outside_rectangle_offsets_with_round_sharp_or_chamfered_corners() {
        let r = 0.5;
        let no = |_: usize| false;
        // Sharp: a rectangle 2r bigger each way.
        let o = offset_path(&rect(100.0, 50.0), true, r, true, CornerMode::Sharp, &no, 1e-4).unwrap();
        assert_eq!(o.pts.len(), 4);
        assert!(near(o.pts[0], [-0.5, -0.5]) && near(o.pts[1], [100.5, -0.5]) && near(o.pts[2], [100.5, 50.5]) && near(o.pts[3], [-0.5, 50.5]), "{:?}", o.pts);
        // Round: an arc of radius r round each drawn corner.
        let o = offset_path(&rect(100.0, 50.0), true, r, true, CornerMode::Round, &no, 1e-4).unwrap();
        assert!(near(o.pts[o.at[1]], [100.0, -0.5]), "the arc at (100, 0) starts square below it");
        let arc: Vec<P> = o.pts[o.at[1]..o.at[2]].to_vec();
        assert!(arc.iter().all(|p| (dist(*p, [100.0, 0.0]) - r).abs() < 1e-9), "{arc:?}");
        assert!(near(*arc.last().unwrap(), [100.5, 0.0]));
        // Chamfer: the line touching that arc at its middle, two corners.
        let o = offset_path(&rect(100.0, 50.0), true, r, true, CornerMode::Chamfer, &no, 1e-4).unwrap();
        let k = r * (std::f64::consts::SQRT_2 - 1.0);
        assert!(near(o.pts[o.at[1]], [100.0 + k, -0.5]) && near(o.pts[o.at[1] + 1], [100.5, -k]), "{:?}", &o.pts);
    }

    #[test]
    fn a_hole_offsets_inward_with_its_inside_corners_trimmed() {
        // A square hole cut clockwise: the beam runs on its right, inside.
        let mut h = rect(20.0, 10.0);
        h.reverse();
        let s = side_for(&h, true, true);
        assert_eq!(s, Side::Right);
        let o = offset_path(&h, true, 0.5, true, CornerMode::Round, &|_| false, 1e-4).unwrap();
        assert_eq!(o.pts.len(), 4, "inside corners are trimmed, not rounded: {:?}", o.pts);
        let mut xs: Vec<P> = o.pts.clone();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(near(xs[0], [0.5, 0.5]) && near(xs[3], [19.5, 9.5]), "{xs:?}");
        // A round hole: radius less the cutter radius.
        let c = circle([50.0, 50.0], 5.0, 72, false);
        let o = offset_path(&c, true, 0.12, true, CornerMode::Sharp, &|_| false, 1e-4).unwrap();
        let rmax = o.pts.iter().map(|p| dist(*p, [50.0, 50.0])).fold(0.0, f64::max);
        assert!((rmax - 4.88).abs() < 0.01, "{rmax}");
    }

    #[test]
    fn a_hole_no_bigger_than_the_kerf_or_a_too_narrow_slot_has_no_path() {
        let c = circle([0.0, 0.0], 0.1, 36, false);
        assert!(offset_path(&c, true, 0.12, true, CornerMode::Round, &|_| false, 1e-4).is_none());
        // Slot 0.2 wide, kerf 0.24.
        let mut slot = rect(10.0, 0.2);
        slot.reverse();
        assert!(offset_path(&slot, true, 0.12, true, CornerMode::Round, &|_| false, 1e-4).is_none());
        // Through a planned contour: reported, not cut.
        let mut con = contour(circle([0.0, 0.0], 0.1, 36, false), true);
        let comp = ContourComp { side: Side::Right, control: false, corrector: 1, system: true, radius: 0.12 };
        let e = compensate(&mut con, &comp, &SystemOpts { tol: 1e-3, ..Default::default() }).unwrap_err();
        assert!(e.not_cut() && e.describe(0.24).contains("not cut"), "{e:?}");
    }

    #[test]
    fn a_notch_narrower_than_the_kerf_is_passed_by() {
        // Outside 100 × 50 counter-clockwise with a 0.2 mm wide, 5 mm deep notch up from the bottom.
        let pts = vec![[0.0, 0.0], [50.0, 0.0], [50.0, 5.0], [50.2, 5.0], [50.2, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let o = offset_path(&pts, true, 0.12, true, CornerMode::Sharp, &|_| false, 1e-4).unwrap();
        assert!(o.narrow);
        assert!(o.pts.iter().all(|p| p[1] < 0.0 || p[0] < 0.0 || p[0] > 100.0 || p[1] > 50.0), "the beam never enters the notch: {:?}", o.pts);
    }

    fn contour(points: Vec<P>, inner: bool) -> crate::cnc::Contour {
        crate::cnc::Contour { lead_in: points[0], lead_path: vec![], lead_out: None, lead_out_path: vec![], loops: vec![], points, inner, part_no: 1, joints: vec![], joint_leads: vec![], feed: 1000.0, manual: false, punch: None, overlap: 0.0, closing: false, comp: ContourComp::default(), quality: 1, time: Default::default(), pierces: 0 }
    }

    fn opts(lead_mode: u8) -> SystemOpts {
        SystemOpts { corners: CornerMode::Sharp, lead_mode, joint_mode: 0, tol: 1e-4 }
    }
    const SYS: ContourComp = ContourComp { side: Side::Right, control: false, corrector: 2, system: true, radius: 0.5 };

    #[test]
    fn a_moved_lead_keeps_its_length_and_takes_the_pierce_with_it() {
        // Lead on the middle of the bottom edge, perpendicular from 5 mm below.
        let pts = crate::cnc::start_at_point(rect(100.0, 50.0), [50.0, 0.0]);
        let mut c = contour(pts, false);
        c.lead_path = vec![[50.0, -5.0], [50.0, 0.0]];
        c.lead_out_path = vec![[50.0, 0.0], [50.0, -3.0]];
        c.lead_out = Some([50.0, -3.0]);
        compensate(&mut c, &SYS, &opts(3)).unwrap();
        assert!(near(c.points[0], [50.0, -0.5]), "{:?}", c.points[0]);
        assert_eq!(c.lead_path, vec![[50.0, -5.5], [50.0, -0.5]]);
        assert!(near(c.lead_in, [50.0, -5.5]), "the pierce moves with the lead");
        assert_eq!(c.lead_out, Some([50.0, -3.5]));
        assert_eq!(c.overlap, 0.0);
    }

    #[test]
    fn an_offset_lead_meets_the_offset_edge_and_the_cut_stops_where_the_lead_out_leaves() {
        let pts = crate::cnc::start_at_point(rect(100.0, 50.0), [50.0, 0.0]);
        let mut c = contour(pts, false);
        c.lead_path = vec![[50.0, -5.0], [50.0, 0.0]];
        c.lead_out_path = vec![[50.0, 0.0], [50.0, -3.0]];
        c.lead_out = Some([50.0, -3.0]);
        compensate(&mut c, &SYS, &opts(0)).unwrap();
        // The lead-in runs r to the right of its line (x + 0.5) down to the offset edge.
        assert!(near(c.lead_path[0], [50.5, -5.0]) && near(*c.lead_path.last().unwrap(), [50.5, -0.5]), "{:?}", c.lead_path);
        assert!(near(c.points[0], [50.5, -0.5]));
        // The lead-out leaves r before the drawn start: the cut ends 2r short of its own start.
        assert!(near(c.lead_out_path[0], [49.5, -0.5]), "{:?}", c.lead_out_path);
        assert!((c.overlap + 1.0).abs() < 1e-9, "{}", c.overlap);
        assert!(near(c.lead_in, [50.5, -5.0]));
    }

    #[test]
    fn micro_joint_gaps_move_sideways_with_their_edge() {
        let mut c = contour(rect(100.0, 50.0), false);
        c.joints = vec![(40.0, 41.0), (170.0, 171.0)];
        compensate(&mut c, &SYS, &opts(0)).unwrap();
        // Sharp corners: the loop starts at (-0.5, -0.5); the bottom gap is 1 mm long, 40.5 mm in.
        assert!(near(c.points[0], [-0.5, -0.5]));
        let (a, b) = c.joints[0];
        assert!((a - 40.5).abs() < 1e-9 && (b - a - 1.0).abs() < 1e-9, "{:?}", c.joints);
        let p = crate::cnc::point_at(&c.points, c.joints[1].0);
        assert!(near(p, [80.0, 50.5]), "the top gap's start sits above the drawn one: {p:?}");
    }

    #[test]
    fn joint_leads_offset_with_their_piece_keep_the_bridge_as_wide_as_drawn() {
        // A 1 mm joint at 40–41 on the bottom edge, leads 5 mm straight down.
        let joint = |mode: u8| {
            let mut c = contour(rect(100.0, 50.0), false);
            c.joints = vec![(40.0, 41.0)];
            c.joint_leads = vec![(vec![[40.0, 0.0], [40.0, -5.0]], vec![[41.0, -5.0], [41.0, 0.0]])];
            compensate(&mut c, &SYS, &SystemOpts { joint_mode: mode, ..opts(0) }).unwrap();
            c
        };
        let c = joint(0);
        let (out, back) = &c.joint_leads[0];
        // The beam's centre leaves r before the gap and comes back r after it:
        // the uncut bridge between the two kerfs is still 1 mm.
        assert!(near(out[0], [39.5, -0.5]) && near(*out.last().unwrap(), [39.5, -5.0]), "{out:?}");
        assert!(near(back[0], [41.5, -5.0]) && near(*back.last().unwrap(), [41.5, -0.5]), "{back:?}");
        let (a, b) = c.joints[0];
        assert!((b - a - 2.0).abs() < 1e-9, "the path's gap is the bridge plus a kerf: {:?}", c.joints);
        // Moved instead: the same leads, half a kerf lower, the gap as drawn.
        let c = joint(1);
        assert_eq!(c.joint_leads[0].0, vec![[40.0, -0.5], [40.0, -5.5]]);
        assert!((c.joints[0].1 - c.joints[0].0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_contour_without_system_compensation_is_left_as_drawn() {
        let mut c = contour(rect(100.0, 50.0), false);
        let comp = ContourComp { system: false, control: true, ..SYS };
        compensate(&mut c, &comp, &opts(0)).unwrap();
        assert_eq!(c.points, rect(100.0, 50.0));
    }

    #[test]
    fn a_reversed_contour_flips_its_side() {
        let mut pts = circle([0.0, 0.0], 10.0, 36, true);
        assert_eq!(side_for(&pts, true, true), Side::Left);
        pts.reverse();
        assert_eq!(side_for(&pts, true, true), Side::Right);
    }
}
