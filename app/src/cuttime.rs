//! Machining time the way the original works it out (MECHANICS "Machining
//! time", CTM1–CTM16): the sheet's program is replayed move by move, each
//! move timed with the machine's accelerations (a speed ramp per axis, the
//! slowest axis sets the time), corners slow the head down, and every pierce,
//! head-up and head-down adds its own time. The settings come from the
//! machine's time file (`.TM1`, beside its technology file); without one the
//! original's defaults apply (CTM16), which leave out the accelerations.

use crate::nest::P;
use std::path::Path;

/// "No limit" in the time file (CTM16).
pub const NO_LIMIT: f64 = 99_999.999;

/// One acceleration record (CTM6): for plates from `t_from` to `t_to` mm,
/// accelerations and decelerations per axis in mm/s², top axis speeds in
/// mm/min.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Accel {
    pub t_from: f64,
    pub t_to: f64,
    pub acc: [f64; 2],
    pub dec: [f64; 2],
    pub vmax: [f64; 2],
}

impl Default for Accel {
    /// No record: everything "no limit", so moves take length ÷ feed (CTM16).
    fn default() -> Self {
        Accel { t_from: 0.0, t_to: f64::MAX, acc: [NO_LIMIT; 2], dec: [NO_LIMIT; 2], vmax: [NO_LIMIT; 2] }
    }
}

/// How a pierce is timed (CTM10, TM1 D014).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PierceMode {
    /// D014 = 1: the same time for every pierce (D016 s).
    Fixed,
    /// D014 = 2: D015 seconds per mm of plate.
    PerMm,
    /// D014 = 3: from the technology table (the record's piercing steps).
    Table,
}

/// Where speeds come from (CTM8 D018, CTM9 D017).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpeedSource {
    /// 1: the machine's active speed (A058, or the section's Z003).
    Active,
    /// 2: the technology table, by cut quality.
    Table,
    /// 3: the table, with the table's reduced feeds where they apply.
    TableReduced,
}

/// The machine's time settings (the `.TM1` file, CTM3–CTM16).
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSettings {
    /// A001: rapids run in a straight line (else each axis on its own, CTM3).
    pub rapid_straight: bool,
    pub pierce_mode: PierceMode,
    /// D015: pierce seconds per mm of plate (mode PerMm).
    pub pierce_per_mm: f64,
    /// D016: fixed pierce seconds (mode Fixed).
    pub pierce_fixed: f64,
    /// D167: pierce time on an already pierced or pre-cut point, % of the full one.
    pub precut_pct: f64,
    /// D017 / D018: marking and cutting speed sources.
    pub mark_source: SpeedSource,
    pub cut_source: SpeedSource,
    /// D168: speed through a 90° corner, mm/min. Negative = the technology
    /// table's per-quality value, which isn't decoded: no slow-down then
    /// (FEATURES MAC-21).
    pub v90: f64,
    /// D001 (with D169 = 1): beam-off time, s.
    pub cut_off: f64,
    /// H006 / H007: head up / head down, s.
    pub head_up: f64,
    pub head_down: f64,
    /// O / P / Q acceleration records: rapids, straight cuts, arcs (CTM6).
    pub rapid_acc: Vec<Accel>,
    pub line_acc: Vec<Accel>,
    pub arc_acc: Vec<Accel>,
    /// S001: jerk-limited (S-curve) moves. Timed with plain ramps here (FEATURES MAC-21).
    pub jerk: bool,
    /// N001–N023: factors on each kind of time (CTM14); 1 = as computed.
    pub factors: [f64; 23],
    /// The file the settings came from (None = the original's defaults).
    pub source: Option<std::path::PathBuf>,
}

impl Default for TimeSettings {
    /// The original's values when the machine has no time file (CTM16).
    fn default() -> Self {
        TimeSettings {
            rapid_straight: true,
            pierce_mode: PierceMode::PerMm,
            pierce_per_mm: 1.0,
            pierce_fixed: 0.0,
            precut_pct: 100.0,
            mark_source: SpeedSource::Active,
            cut_source: SpeedSource::Active,
            v90: NO_LIMIT,
            cut_off: 0.0,
            head_up: 0.0,
            head_down: 0.0,
            rapid_acc: vec![],
            line_acc: vec![],
            arc_acc: vec![],
            jerk: false,
            factors: [1.0; 23],
            source: None,
        }
    }
}

/// Factor slots for the kinds of time the app keeps (CTM14): N001 rapids,
/// N002 cutting, N007 marking, N008 pierces.
pub const F_RAPID: usize = 0;
pub const F_CUT: usize = 1;
pub const F_MARK: usize = 6;
pub const F_PIERCE: usize = 7;

impl TimeSettings {
    /// Read a time file (`Xnnn value…` lines, `;` comments). Codes it doesn't
    /// give keep the defaults (CTM16).
    pub fn from_text(text: &str) -> TimeSettings {
        let mut s = TimeSettings::default();
        let mut pierce_mode = 2;
        let mut cutoff_mode = 1;
        let (mut o, mut p, mut q) = (vec![], vec![], vec![]);
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let mut it = line.split_whitespace();
            let Some(code) = it.next() else { continue };
            let b = code.as_bytes();
            if b.len() != 4 || !b[0].is_ascii_uppercase() || !b[1..].iter().all(u8::is_ascii_digit) {
                continue;
            }
            let vals: Vec<&str> = it.collect();
            let Some(&first) = vals.first() else { continue };
            let num = first.parse::<f64>().ok();
            let on = first.eq_ignore_ascii_case("ON");
            let n: usize = code[1..].parse().unwrap_or(0);
            let src = |v: Option<f64>| match v.map(|v| v as i64) {
                Some(2) => SpeedSource::Table,
                Some(3) => SpeedSource::TableReduced,
                _ => SpeedSource::Active,
            };
            match (b[0], n) {
                (b'A', 1) => s.rapid_straight = on,
                (b'D', 1) => s.cut_off = num.unwrap_or(0.0).abs(),
                (b'D', 14) => pierce_mode = num.unwrap_or(2.0) as i64,
                (b'D', 15) => s.pierce_per_mm = num.unwrap_or(1.0),
                (b'D', 16) => s.pierce_fixed = num.unwrap_or(0.0),
                (b'D', 17) => s.mark_source = src(num),
                (b'D', 18) => s.cut_source = src(num),
                (b'D', 167) => s.precut_pct = num.unwrap_or(100.0),
                (b'D', 168) => s.v90 = num.unwrap_or(NO_LIMIT),
                (b'D', 169) => cutoff_mode = num.unwrap_or(1.0) as i64,
                (b'H', 6) => s.head_up = num.unwrap_or(0.0),
                (b'H', 7) => s.head_down = num.unwrap_or(0.0),
                (b'N', 1..=23) => s.factors[n - 1] = num.unwrap_or(1.0),
                (b'S', 1) => s.jerk = num.unwrap_or(0.0) > 0.0,
                (b'O' | b'P' | b'Q', _) => {
                    let v: Vec<f64> = vals.iter().filter_map(|v| v.parse().ok()).collect();
                    if v.len() >= 8 {
                        // A value of 0 or less limits nothing (our reading of an unset field).
                        let lim = |x: f64| if x > 1e-9 { x } else { NO_LIMIT };
                        let r = Accel { t_from: v[0], t_to: v[1], acc: [lim(v[2]), lim(v[3])], dec: [lim(v[4]), lim(v[5])], vmax: [lim(v[6]), lim(v[7])] };
                        match b[0] {
                            b'O' => o.push(r),
                            b'P' => p.push(r),
                            _ => q.push(r),
                        }
                    }
                }
                _ => {}
            }
        }
        s.pierce_mode = match pierce_mode {
            1 => PierceMode::Fixed,
            2 => PierceMode::PerMm,
            _ => PierceMode::Table,
        };
        // CTM11: only the fixed beam-off time applies to a laser (the
        // table's is a waterjet's).
        if cutoff_mode != 1 {
            s.cut_off = 0.0;
        }
        s.rapid_acc = o;
        // CTM6: arcs take the straight-cut record when there is no Q record.
        s.arc_acc = if q.is_empty() { p.clone() } else { q };
        s.line_acc = p;
        s
    }

    /// Read a time file (Latin-1 text).
    pub fn load(path: &Path) -> Option<TimeSettings> {
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut s = TimeSettings::from_text(&text);
        s.source = Some(path.to_path_buf());
        Some(s)
    }

    /// One line for the window and agents: where the time settings came from.
    pub fn label(&self) -> String {
        let name = |p: &std::path::PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match &self.source {
            Some(p) => format!("Time settings: {} (accelerations, corners, head and pierce times)", name(p)),
            None => "Time settings: the machine has no time file; moves take length ÷ speed, 1 s per mm of plate per pierce".into(),
        }
    }

    /// Seconds for one pierce at `thickness` mm (CTM10); `table` is the
    /// technology table's pierce time for the contour's quality, when it has one.
    pub fn pierce_secs(&self, thickness: f64, table: Option<f64>) -> f64 {
        match self.pierce_mode {
            PierceMode::Fixed => self.pierce_fixed,
            PierceMode::PerMm => self.pierce_per_mm * thickness,
            // No table record: our fallback is the per-mm rule (FEATURES MAC-20).
            PierceMode::Table => table.unwrap_or(self.pierce_per_mm * thickness),
        }
    }
}

/// The first record whose thickness range holds `t` (CTM6), else no limits.
pub fn accel_for(list: &[Accel], t: f64) -> Accel {
    list.iter().find(|a| a.t_from <= t && t <= a.t_to).copied().unwrap_or_default()
}

/// Time for one axis to travel `d` mm, cruising at `v`, entering at `v0` and
/// leaving at `v1` (mm/s), with acceleration `a` and deceleration `dc` (mm/s²)
/// (CTM2): a speed ramp up, a cruise and a ramp down, or — when the move is too
/// short to reach `v` — up to the highest speed it can reach and straight down.
pub fn axis_secs(d: f64, v: f64, v0: f64, v1: f64, a: f64, dc: f64) -> f64 {
    if d <= 1e-9 || v <= 1e-9 {
        return 0.0;
    }
    // No (or a nonsense) acceleration limits nothing.
    let a = if a > 1e-9 { a } else { NO_LIMIT };
    let dc = if dc > 1e-9 { dc } else { NO_LIMIT };
    let (v0, v1) = (v0.min(v), v1.min(v));
    let s_acc = (v * v - v0 * v0) / (2.0 * a);
    let s_dec = (v * v - v1 * v1) / (2.0 * dc);
    if s_acc + s_dec <= d {
        return (v - v0) / a + (v - v1) / dc + (d - s_acc - s_dec) / v;
    }
    let peak = ((v0 * v0 * dc + 2.0 * d * a * dc + v1 * v1 * a) / (a + dc)).sqrt();
    // An exit faster than the move can reach: no ramp down (CTM2 lowers the
    // exit instead; `move_secs` passes only reachable exits).
    let peak = peak.max(v0).max(v1);
    (peak - v0) / a + (peak - v1) / dc
}

/// One straight move from `p` to `q` at feed `f` mm/min (CTM2): each axis
/// ramps on its own and the slowest sets the time. `v0`/`v1` are the path
/// speeds in and out (mm/min). Returns (seconds, the exit speed actually
/// reached, mm/min).
pub fn move_secs(p: P, q: P, f: f64, v0: f64, v1: f64, acc: &Accel) -> (f64, f64) {
    let d = [(q[0] - p[0]).abs(), (q[1] - p[1]).abs()];
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if l <= 1e-9 || f <= 1e-9 {
        return (0.0, v0);
    }
    // The feed the axes allow along this direction.
    let mut feed = f;
    for i in 0..2 {
        if d[i] > 1e-9 {
            feed = feed.min(acc.vmax[i] * l / d[i]);
        }
    }
    let v0 = v0.min(feed);
    // The exit can't be faster than the move can speed up to (CTM2: the next
    // move's entry is lowered).
    let mut reach = f64::MAX;
    for i in 0..2 {
        if d[i] > 1e-9 {
            let a_path = acc.acc[i] * l / d[i];
            reach = reach.min((v0 / 60.0).powi(2) + 2.0 * a_path * l);
        }
    }
    let v1 = v1.min(feed).min(reach.sqrt() * 60.0);
    let mut t: f64 = 0.0;
    for i in 0..2 {
        if d[i] > 1e-9 {
            let k = d[i] / l / 60.0;
            t = t.max(axis_secs(d[i], feed * k, v0 * k, v1 * k, acc.acc[i], acc.dec[i]));
        }
    }
    (t, v1)
}

/// The speed the head may keep through a corner where the path turns by
/// `turn` degrees (CTM5): the 90° speed `v90` scaled by how sharp the corner
/// is, never above the feed `f`. A straight-on vertex keeps the feed; a full
/// reversal stops.
pub fn corner_speed(turn: f64, v90: f64, f: f64) -> f64 {
    // No limit, or the table's (negative, not decoded): no slow-down.
    if v90 >= NO_LIMIT * 0.999 || v90 < 0.0 {
        return f;
    }
    let s = (90.0 - turn / 2.0).to_radians().sin();
    if s >= 1.0 - 1e-12 {
        return f;
    }
    if s <= 0.0 {
        return 0.0;
    }
    let r45 = std::f64::consts::FRAC_1_SQRT_2;
    let k = ((s / (1.0 - s)) / (r45 / (1.0 - r45))).sqrt();
    (v90 * k).min(f)
}

/// A vertex that turns by no more than this many degrees is part of a curve
/// (our polylines stand for arcs): the head keeps its speed (FEATURES MAC-21).
pub const TANGENT_DEG: f64 = 20.0;

/// The turn at `b` on the way a → b → c, degrees (0 = straight on).
pub fn turn_deg(a: P, b: P, c: P) -> f64 {
    let u = [b[0] - a[0], b[1] - a[1]];
    let w = [c[0] - b[0], c[1] - b[1]];
    let (lu, lw) = ((u[0] * u[0] + u[1] * u[1]).sqrt(), (w[0] * w[0] + w[1] * w[1]).sqrt());
    if lu <= 1e-9 || lw <= 1e-9 {
        return 0.0;
    }
    let c = ((u[0] * w[0] + u[1] * w[1]) / (lu * lw)).clamp(-1.0, 1.0);
    c.acos().to_degrees()
}

/// The deceleration along the path for a move from `p` to `q`: the axis
/// that limits it sets it (mm/s²).
fn path_dec(p: P, q: P, acc: &Accel) -> f64 {
    let d = [(q[0] - p[0]).abs(), (q[1] - p[1]).abs()];
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (0..2).filter(|&i| d[i] > 1e-9).map(|i| acc.dec[i] * l / d[i]).fold(f64::MAX, f64::min)
}

/// A cut along `path` at feed `f` mm/min, from rest to rest (CTM4, CTM5):
/// each vertex slows the head to its corner speed (vertices that turn by no
/// more than `TANGENT_DEG` keep the feed), the end stops it. Each vertex's
/// speed is also held low enough to brake in time for the slower vertices
/// further on (CTM2). Seconds.
pub fn path_secs(path: &[P], f: f64, v90: f64, acc: &Accel) -> f64 {
    let n = path.len();
    if n < 2 {
        return 0.0;
    }
    // Speed limit at the end of each move (mm/min): its corner, 0 at the end.
    let mut lim: Vec<f64> = (0..n - 1)
        .map(|k| match path.get(k + 2) {
            None => 0.0,
            Some(&c) => {
                let turn = turn_deg(path[k], path[k + 1], c);
                if turn <= TANGENT_DEG { f } else { corner_speed(turn, v90, f) }
            }
        })
        .collect();
    // Look ahead: no faster than the head can brake to the next limit.
    for k in (0..n.saturating_sub(2)).rev() {
        let (p, q) = (path[k + 1], path[k + 2]);
        let l = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        let next = lim[k + 1] / 60.0;
        let reach = (next * next + 2.0 * path_dec(p, q, acc) * l).sqrt() * 60.0;
        lim[k] = lim[k].min(reach);
    }
    let mut t = 0.0;
    let mut v = 0.0;
    for k in 0..n - 1 {
        let (dt, out) = move_secs(path[k], path[k + 1], f, v, lim[k], acc);
        t += dt;
        v = out;
    }
    t
}

/// A rapid from `p` to `q` at `f` mm/min, from rest to rest (CTM3). With
/// `straight` off each axis runs on its own: a 45° move over the shorter
/// axis, then straight on along the longer one.
pub fn rapid_secs(p: P, q: P, f: f64, straight: bool, acc: &Accel) -> f64 {
    if straight {
        return move_secs(p, q, f, 0.0, 0.0, acc).0;
    }
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let m = dx.abs().min(dy.abs());
    let mid = [p[0] + m * dx.signum(), p[1] + m * dy.signum()];
    move_secs(p, mid, f, 0.0, 0.0, acc).0 + move_secs(mid, q, f, 0.0, 0.0, acc).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> TimeSettings {
        TimeSettings::from_text("A001 ON\nD014 3\nD015 1\nD017 2\nD018 2\nD168 3\nD169 1\nH006 0.7\nN001 1\nO001 0 999999 10000 10000 10000 10000 114000 140000\nP001 0 999999 10000 10000 10000 10000 114000 140000\nQ001 0 999999 8000 8000 8000 8000 114000 140000\nS001 1\n")
    }

    #[test]
    fn a_100_mm_cut_at_3000_takes_the_ramps_on_top_of_length_over_feed() {
        // CTM2 worked example: 0.005 + 0.005 + 99.75 / 50 = 2.005 s.
        let s = template();
        let a = accel_for(&s.line_acc, 3.0);
        let t = path_secs(&[[0.0, 0.0], [100.0, 0.0]], 3000.0, s.v90, &a);
        assert!((t - 2.005).abs() < 1e-9, "{t}");
        let t1 = path_secs(&[[0.0, 0.0], [1.0, 0.0]], 3000.0, s.v90, &a);
        assert!((t1 - 0.025).abs() < 1e-9, "{t1}");
    }

    #[test]
    fn a_rapid_is_limited_by_the_slower_axis() {
        // CTM3 worked example: 0 → (300, 400) with the template's O record = 0.433 s.
        let s = template();
        let t = rapid_secs([0.0, 0.0], [300.0, 400.0], NO_LIMIT, true, &accel_for(&s.rapid_acc, 3.0));
        assert!((t - 0.4333).abs() < 1e-3, "{t}");
    }

    #[test]
    fn without_a_time_file_moves_take_length_over_feed() {
        // CTM16: no acceleration records, 1 s per mm per pierce.
        let s = TimeSettings::default();
        let t = path_secs(&[[0.0, 0.0], [100.0, 0.0], [100.0, 50.0]], 3000.0, s.v90, &accel_for(&s.line_acc, 3.0));
        assert!((t - 3.0).abs() < 1e-3, "{t}");
        assert_eq!(s.pierce_secs(5.0, None), 5.0);
    }

    #[test]
    fn corners_slow_the_head_and_arcs_do_not() {
        let s = template();
        let a = accel_for(&s.line_acc, 3.0);
        // CTM5: 90° → the 90° speed, reversal → 0, straight → the feed.
        assert!((corner_speed(90.0, 3.0, 3000.0) - 3.0).abs() < 1e-9);
        assert_eq!(corner_speed(180.0, 3.0, 3000.0), 0.0);
        assert_eq!(corner_speed(0.0, 3.0, 3000.0), 3000.0);
        // A square: four near stops cost more than one straight line of the same length.
        let sq = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0], [0.0, 0.0]];
        let line = [[0.0, 0.0], [400.0, 0.0]];
        assert!(path_secs(&sq, 3000.0, s.v90, &a) > path_secs(&line, 3000.0, s.v90, &a) + 0.01);
        // A circle as 72 facets keeps its speed all the way round.
        let circle: Vec<P> = (0..=72).map(|k| {
            let t = (k as f64) * 5.0f64.to_radians();
            [50.0 * t.cos(), 50.0 * t.sin()]
        }).collect();
        let len: f64 = circle.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
        let t = path_secs(&circle, 3000.0, s.v90, &a);
        assert!(t < len / 50.0 + 0.02, "{t} vs {}", len / 50.0);
    }

    #[test]
    fn the_template_time_file_is_read() {
        let s = template();
        assert_eq!(s.pierce_mode, PierceMode::Table);
        assert_eq!((s.cut_source, s.mark_source), (SpeedSource::Table, SpeedSource::Table));
        assert_eq!(s.head_up, 0.7);
        assert_eq!(s.arc_acc[0].acc, [8000.0, 8000.0]);
        assert!(s.jerk);
        // No Q record: arcs use the straight-cut record (CTM6).
        let p_only = TimeSettings::from_text("P001 0 10 5000 5000 5000 5000 60000 60000\n");
        assert_eq!(p_only.arc_acc, p_only.line_acc);
    }

    #[test]
    fn the_head_brakes_in_time_for_a_corner_after_a_curve() {
        // A long run of gentle bends (kept at the feed), then a tiny segment
        // into a full stop: the braking starts on the bends before it (CTM2).
        let s = template();
        let a = Accel { acc: [500.0; 2], dec: [500.0; 2], ..accel_for(&s.line_acc, 3.0) };
        let mut path: Vec<P> = (0..=40).map(|k| [k as f64 * 2.0, (k % 2) as f64 * 0.2]).collect();
        path.push([80.2, 0.0]);
        let t = path_secs(&path, 3000.0, s.v90, &a);
        // From rest to rest over ~80 mm at 50 mm/s with 500 mm/s² ramps: at
        // least the ramp time 0.1 s each way on top of length ÷ feed.
        let len: f64 = path.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum();
        assert!(t >= len / 50.0 + 0.09, "{t} vs {}", len / 50.0);
    }

    #[test]
    fn zero_values_in_the_time_file_limit_nothing() {
        let s = TimeSettings::from_text("P001 0 999 0 0 0 0 0 0\nD168 -1\nN007 2\n");
        let a = accel_for(&s.line_acc, 3.0);
        let t = path_secs(&[[0.0, 0.0], [100.0, 0.0], [100.0, 50.0]], 3000.0, s.v90, &a);
        assert!(t.is_finite() && (t - 3.0).abs() < 1e-3, "{t}");
        // A negative 90° speed (the table's, not decoded) slows no corner.
        assert_eq!(corner_speed(90.0, s.v90, 3000.0), 3000.0);
        // N007 scales the marking time.
        assert_eq!(s.factors[F_MARK], 2.0);
    }
}
