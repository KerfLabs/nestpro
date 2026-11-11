//! Machines and how they machine a contour: cutting heads (laser, plasma,
//! oxyfuel, waterjet), combined punch + cut, and punching machines — and the
//! original's standard-shape recognition that decides when a contour can be
//! punched with one tool instead of cut. See MECHANICS.md "Machines and
//! standard shapes".

use crate::nest::P;

/// The machine a sheet is programmed for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum MachineKind {
    #[default]
    Laser,
    Plasma,
    Oxyfuel,
    Waterjet,
    /// Combined machine: punches standard holes, cuts the rest.
    LaserPunch,
    /// Punching machine: one hit for a standard shape, nibbling for the rest.
    Punch,
}

/// The original's machine type (postprocessor library line A001:
/// "0:CUT, 1:CUTPUNCH, 2:PUNCH").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Cut,
    CutPunch,
    Punch,
}

impl Family {
    /// The A001 code.
    pub fn code(self) -> u8 {
        match self {
            Family::Cut => FAMILY_CUT,
            Family::CutPunch => FAMILY_CUTPUNCH,
            Family::Punch => FAMILY_PUNCH,
        }
    }
}

/// A001 codes.
pub const FAMILY_CUT: u8 = 0;
pub const FAMILY_CUTPUNCH: u8 = 1;
pub const FAMILY_PUNCH: u8 = 2;
/// Machine tolerance (A005 in the installed libraries).
pub const MACHINE_TOL: f64 = 0.005;
/// Shape tolerance grows with the shape (imported drawings aren't exact).
pub const STD_TOL_REL: f64 = 0.002;
/// Corners within this many degrees of square count as square.
pub const SQUARE_TOL_DEG: f64 = 0.5;
/// A circle is an arc of 360°; an obround's two arcs are 180° each (MC3).
pub const CIRCLE_SWEEP_DEG: f64 = 360.0;
pub const OBROUND_ARC_DEG: f64 = 180.0;
/// Nibbling: distance between hits along a contour that isn't a standard shape.
pub const NIBBLE_STEP: f64 = 4.0;
/// Seconds per punch hit (for the time estimate).
pub const HIT_SECONDS: f64 = 0.25;

impl MachineKind {
    pub const ALL: [MachineKind; 6] = [MachineKind::Laser, MachineKind::Plasma, MachineKind::Oxyfuel, MachineKind::Waterjet, MachineKind::LaserPunch, MachineKind::Punch];

    pub fn label(self) -> &'static str {
        match self {
            MachineKind::Laser => "Laser 01",
            MachineKind::Plasma => "Plasma 01",
            MachineKind::Oxyfuel => "Oxyfuel 01",
            MachineKind::Waterjet => "Waterjet 01",
            MachineKind::LaserPunch => "Laser-Punch 01",
            MachineKind::Punch => "Punch 01",
        }
    }

    /// What the machine does, in shop words: "Laser", "Plasma" … (MK1).
    pub fn process(self) -> &'static str {
        match self {
            MachineKind::Laser => "Laser",
            MachineKind::Plasma => "Plasma",
            MachineKind::Oxyfuel => "Oxyfuel",
            MachineKind::Waterjet => "Waterjet",
            MachineKind::LaserPunch => "Laser + punch",
            MachineKind::Punch => "Punch",
        }
    }

    /// The key it is saved under in the settings file ("laser", "laserpunch" …).
    pub fn key(self) -> String {
        format!("{self:?}").to_lowercase()
    }

    pub fn from_key(k: &str) -> Option<MachineKind> {
        MachineKind::ALL.into_iter().find(|m| m.key() == k.trim().to_lowercase())
    }

    /// A machine's process guessed from its name, for machines brought in
    /// from the other program that don't say (MK2, estimate): a name with
    /// plasma, oxy, water or punch in it is that; anything else a laser.
    pub fn guess_from_name(name: &str) -> MachineKind {
        let n = name.to_lowercase();
        let punch = n.contains("punch") || n.contains("stanz");
        if punch && (n.contains("laser") || n.contains("combi")) {
            MachineKind::LaserPunch
        } else if punch {
            MachineKind::Punch
        } else if n.contains("plasma") {
            MachineKind::Plasma
        } else if n.contains("oxy") || n.contains("flame") {
            MachineKind::Oxyfuel
        } else if n.contains("water") {
            MachineKind::Waterjet
        } else {
            MachineKind::Laser
        }
    }

    pub fn family(self) -> Family {
        match self {
            MachineKind::LaserPunch => Family::CutPunch,
            MachineKind::Punch => Family::Punch,
            _ => Family::Cut,
        }
    }

    pub fn describe(self) -> &'static str {
        match self.family() {
            Family::Cut => "Cuts every contour with leads, joints and loops",
            Family::CutPunch => "Punches holes that are standard shapes (circle, rectangle, slot) with one hit, cuts everything else",
            Family::Punch => "Punches standard shapes with one hit and nibbles every other contour",
        }
    }
}

/// A standard shape the original recognises (MC3; it tries them in this order: 0 circle, 1 rectangle, 2 obround slot,
/// 3 ring sector).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StdShape {
    Circle { c: P, d: f64 },
    Rect { c: P, w: f64, h: f64, angle: f64 },
    Obround { c: P, l: f64, w: f64, angle: f64 },
}

impl StdShape {
    pub fn centre(&self) -> P {
        match *self {
            StdShape::Circle { c, .. } | StdShape::Rect { c, .. } | StdShape::Obround { c, .. } => c,
        }
    }
    /// Short name with size, as a punch tool would be called.
    pub fn label(&self) -> String {
        match *self {
            StdShape::Circle { d, .. } => format!("Round Ø{d:.1}"),
            StdShape::Rect { w, h, .. } => format!("Rect {w:.1}×{h:.1}"),
            StdShape::Obround { l, w, .. } => format!("Obround {l:.1}×{w:.1}"),
        }
    }
    /// The original's type number for the shape (MC3).
    pub fn code(&self) -> u8 {
        match self {
            StdShape::Circle { .. } => 0,
            StdShape::Rect { .. } => 1,
            StdShape::Obround { .. } => 2,
        }
    }
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn seg_point(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-18 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    dist(p, [a[0] + t * dx, a[1] + t * dy])
}

/// Drop repeated points and points where the outline runs straight on.
fn corners(pts: &[P]) -> Vec<P> {
    let mut v: Vec<P> = vec![];
    for &p in pts {
        if v.last().is_none_or(|q| dist(*q, p) > 1e-9) {
            v.push(p);
        }
    }
    if v.len() > 1 && dist(v[0], v[v.len() - 1]) < 1e-9 {
        v.pop();
    }
    let n = v.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            cross.abs() > 1e-9 * (dist(a, b) * dist(b, c)).max(1e-12)
        })
        .map(|i| v[i])
        .collect()
}

/// Recognise a closed contour as a standard shape, in the original's order:
/// circle, rectangle, obround (MC3). A circle is one
/// full circle (an arc of 360°); a rectangle has four straight sides at right
/// angles and no arcs; an obround has two straight sides joined by two
/// half-circles (arcs of 180°) — every point lies at half the width from the
/// line between the two arc centres.
pub fn recognise(pts: &[P]) -> Option<StdShape> {
    let v = corners(pts);
    let n = v.len();
    if n < 3 {
        return None;
    }
    let (lo, hi) = v.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    let tol = MACHINE_TOL + STD_TOL_REL * size;
    // 0 — circle.
    if n >= 12 {
        let c = [v.iter().map(|p| p[0]).sum::<f64>() / n as f64, v.iter().map(|p| p[1]).sum::<f64>() / n as f64];
        let r = v.iter().map(|&p| dist(p, c)).sum::<f64>() / n as f64;
        // The outline turns once all the way round (360°), never back.
        let turning: f64 = (0..n)
            .map(|i| {
                let (a, b, cc) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
                let (u, w) = ([b[0] - a[0], b[1] - a[1]], [cc[0] - b[0], cc[1] - b[1]]);
                (u[0] * w[1] - u[1] * w[0]).atan2(u[0] * w[0] + u[1] * w[1]).to_degrees().abs()
            })
            .sum();
        if (turning - CIRCLE_SWEEP_DEG).abs() < 1.0 && v.iter().all(|&p| (dist(p, c) - r).abs() <= tol) {
            return Some(StdShape::Circle { c, d: 2.0 * r });
        }
    }
    // 1 — rectangle.
    if n == 4 {
        let square = (0..4).all(|i| {
            let (a, b, c) = (v[(i + 3) % 4], v[i], v[(i + 1) % 4]);
            let (u, w) = ([a[0] - b[0], a[1] - b[1]], [c[0] - b[0], c[1] - b[1]]);
            let cos = (u[0] * w[0] + u[1] * w[1]) / (dist(a, b) * dist(c, b)).max(1e-12);
            cos.abs() <= SQUARE_TOL_DEG.to_radians().sin()
        });
        if square {
            let (w, h) = (dist(v[0], v[1]), dist(v[1], v[2]));
            let angle = crate::nest::norm_angle((v[1][1] - v[0][1]).atan2(v[1][0] - v[0][0]).to_degrees() % 180.0);
            let c = [(v[0][0] + v[2][0]) / 2.0, (v[0][1] + v[2][1]) / 2.0];
            return Some(StdShape::Rect { c, w, h, angle });
        }
    }
    // 2 — obround: the two longest sides are the straight ones.
    if n >= 8 {
        let mut edges: Vec<(f64, usize)> = (0..n).map(|i| (dist(v[i], v[(i + 1) % n]), i)).collect();
        edges.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (e1, e2) = (edges[0].1, edges[1].1);
        let (a1, b1, a2, b2) = (v[e1], v[(e1 + 1) % n], v[e2], v[(e2 + 1) % n]);
        // Opposite, parallel and as long as each other.
        let d1 = [b1[0] - a1[0], b1[1] - a1[1]];
        let d2 = [b2[0] - a2[0], b2[1] - a2[1]];
        let cross = (d1[0] * d2[1] - d1[1] * d2[0]).abs() / (edges[0].0 * edges[1].0).max(1e-12);
        let antiparallel = d1[0] * d2[0] + d1[1] * d2[1] < 0.0;
        if cross < 1e-3 && antiparallel && (edges[0].0 - edges[1].0).abs() <= tol {
            // Arc centres: midpoints across the straight sides' ends.
            let s0 = [(a1[0] + b2[0]) / 2.0, (a1[1] + b2[1]) / 2.0];
            let s1 = [(b1[0] + a2[0]) / 2.0, (b1[1] + a2[1]) / 2.0];
            let r = dist(a1, b2) / 2.0;
            // Each arc runs 180° round its centre: its two ends are opposite.
            let sweep = |c: P, p: P, q: P| {
                let (u, w) = ([p[0] - c[0], p[1] - c[1]], [q[0] - c[0], q[1] - c[1]]);
                (u[0] * w[1] - u[1] * w[0]).atan2(u[0] * w[0] + u[1] * w[1]).to_degrees().abs()
            };
            let half = (sweep(s0, a1, b2) - OBROUND_ARC_DEG).abs() < 1.0 && (sweep(s1, b1, a2) - OBROUND_ARC_DEG).abs() < 1.0;
            if half && r > tol && v.iter().all(|&p| (seg_point(p, s0, s1) - r).abs() <= tol) {
                let c = [(s0[0] + s1[0]) / 2.0, (s0[1] + s1[1]) / 2.0];
                let angle = crate::nest::norm_angle((s1[1] - s0[1]).atan2(s1[0] - s0[0]).to_degrees() % 180.0);
                return Some(StdShape::Obround { c, l: dist(s0, s1) + 2.0 * r, w: 2.0 * r, angle });
            }
        }
    }
    None
}

/// How a punching machine machines one contour.
#[derive(Clone, Debug, PartialEq)]
pub struct Punch {
    /// The tool: a standard shape (one hit at its centre), or None for nibbling.
    pub tool: Option<StdShape>,
    pub hits: Vec<P>,
}

impl Punch {
    pub fn label(&self) -> String {
        match &self.tool {
            Some(s) => format!("PUNCH {}", s.label()),
            None => format!("NIBBLE {} HITS", self.hits.len()),
        }
    }
}

/// Is this contour punched on this machine, and how? A combined machine punches
/// only holes that are standard shapes; a punching machine punches every
/// standard shape and nibbles the other contours.
pub fn punch_for(kind: MachineKind, pts: &[P], inner: bool) -> Option<Punch> {
    let std = recognise(pts);
    match (kind.family(), std) {
        (Family::Cut, _) => None,
        (Family::CutPunch, Some(s)) if inner => Some(Punch { tool: Some(s), hits: vec![s.centre()] }),
        (Family::CutPunch, _) => None,
        (Family::Punch, Some(s)) => Some(Punch { tool: Some(s), hits: vec![s.centre()] }),
        (Family::Punch, None) => Some(Punch { tool: None, hits: nibble(pts) }),
    }
}

/// Hits along a contour every NIBBLE_STEP.
pub fn nibble(pts: &[P]) -> Vec<P> {
    let n = pts.len();
    let per: f64 = (0..n).map(|i| dist(pts[i], pts[(i + 1) % n])).sum();
    let k = (per / NIBBLE_STEP).ceil().max(1.0) as usize;
    (0..k).map(|i| crate::cnc::point_at(pts, per * i as f64 / k as f64)).collect()
}

#[cfg(test)]
mod machine_tests {
    use super::*;
    use crate::nest::circle;

    #[test]
    fn a_machine_says_what_it_does() {
        // MK1: saved by key, shown in shop words.
        for m in MachineKind::ALL {
            assert_eq!(MachineKind::from_key(&m.key()), Some(m));
        }
        assert_eq!(MachineKind::from_key(" Plasma "), Some(MachineKind::Plasma));
        assert_eq!(MachineKind::from_key("lathe"), None);
        assert_eq!(MachineKind::LaserPunch.process(), "Laser + punch");
        // MK2: guessed from a name brought in from the other program.
        assert_eq!(MachineKind::guess_from_name("LC5 B"), MachineKind::Laser);
        assert_eq!(MachineKind::guess_from_name("Table plasma 2"), MachineKind::Plasma);
        assert_eq!(MachineKind::guess_from_name("Oxy table"), MachineKind::Oxyfuel);
        assert_eq!(MachineKind::guess_from_name("Waterjet 01"), MachineKind::Waterjet);
        assert_eq!(MachineKind::guess_from_name("Punch 01"), MachineKind::Punch);
        assert_eq!(MachineKind::guess_from_name("Laser-Punch 01"), MachineKind::LaserPunch);
    }

    #[test]
    fn recognises_the_standard_shapes() {
        match recognise(&circle(10.0, 20.0, 5.0, 32)) {
            Some(StdShape::Circle { c, d }) => {
                assert!((d - 10.0).abs() < 1e-6 && (c[0] - 10.0).abs() < 1e-6 && (c[1] - 20.0).abs() < 1e-6);
            }
            o => panic!("{o:?}"),
        }
        match recognise(&[[0.0, 0.0], [30.0, 0.0], [30.0, 10.0], [0.0, 10.0]]) {
            Some(StdShape::Rect { w, h, .. }) => assert!((w - 30.0).abs() < 1e-9 && (h - 10.0).abs() < 1e-9),
            o => panic!("{o:?}"),
        }
        // A rectangle with a midpoint on one side is still a rectangle.
        assert!(matches!(recognise(&[[0.0, 0.0], [15.0, 0.0], [30.0, 0.0], [30.0, 10.0], [0.0, 10.0]]), Some(StdShape::Rect { .. })));
        // A slot 60 long, 12 wide.
        let p = crate::nest::Part { name: "S".into(), shape: crate::nest::Shape::SlottedPlate, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let slot = &p.geometry().holes[0];
        match recognise(slot) {
            Some(StdShape::Obround { l, w, .. }) => assert!((l - 60.0).abs() < 0.05 && (w - 12.0).abs() < 0.05, "{l} {w}"),
            o => panic!("{o:?}"),
        }
        // A triangle and an L are not standard.
        assert_eq!(recognise(&[[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]), None);
        assert_eq!(recognise(&[[0.0, 0.0], [20.0, 0.0], [20.0, 5.0], [5.0, 5.0], [5.0, 20.0], [0.0, 20.0]]), None);
    }

    #[test]
    fn machines_punch_what_their_family_allows() {
        let hole = circle(0.0, 0.0, 5.0, 32);
        let tri = vec![[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]];
        assert_eq!(MachineKind::Laser.family().code(), FAMILY_CUT);
        assert!(punch_for(MachineKind::Laser, &hole, true).is_none());
        assert!(punch_for(MachineKind::LaserPunch, &hole, true).is_some_and(|p| p.hits.len() == 1));
        assert!(punch_for(MachineKind::LaserPunch, &tri, false).is_none());
        let nib = punch_for(MachineKind::Punch, &tri, false).unwrap();
        assert!(nib.tool.is_none() && nib.hits.len() >= (136.6 / NIBBLE_STEP) as usize);
    }
}
