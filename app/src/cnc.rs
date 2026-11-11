//! Cutting sequence, cost estimate and a simple ISO (G-code) machine program
//! for one nested sheet. Speeds, pierce times and the time of every move come
//! from the machine's files (`techtable`, `cuttime`).

use crate::leads::LeadRec;
use crate::nest::{perimeter, CutQuality, LeadType, Part, PartTech, Placed, SheetSpec, P};

pub const MATERIALS: [&str; 3] = ["Mild steel", "Stainless steel", "Aluminium"];
pub const THICKNESSES: [f64; 7] = [1.0, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0];
const DENSITY: [f64; 3] = [7.85, 8.0, 2.70];

/// Typical fibre-laser cutting speed (mm/min), used only when the machine
/// file gives no speed at all (estimate).
pub fn cut_speed(material: usize, t: f64) -> f64 {
    let table: [[f64; 7]; 3] = [
        [20000.0, 9000.0, 5000.0, 3000.0, 1800.0, 1300.0, 800.0],
        [22000.0, 10000.0, 4500.0, 2000.0, 900.0, 600.0, 300.0],
        [25000.0, 12000.0, 6000.0, 2500.0, 1000.0, 700.0, 400.0],
    ];
    // A thickness between two in the list (any can be typed, TH1) is read
    // in a straight line between them; outside the list, the nearest end.
    let row = &table[material.min(2)];
    let t = t.clamp(THICKNESSES[0], THICKNESSES[THICKNESSES.len() - 1]);
    let i = THICKNESSES.iter().position(|&x| x >= t - 1e-9).unwrap_or(THICKNESSES.len() - 1);
    if i == 0 || (THICKNESSES[i] - t).abs() < 1e-9 {
        return row[i];
    }
    let f = (t - THICKNESSES[i - 1]) / (THICKNESSES[i] - THICKNESSES[i - 1]);
    row[i - 1] + f * (row[i] - row[i - 1])
}

pub fn weight_kg(material: usize, area_mm2: f64, t: f64) -> f64 {
    area_mm2 * t * DENSITY[material.min(2)] / 1.0e6
}

pub struct Contour {
    /// Pierce point (start of the lead-in path).
    pub lead_in: P,
    /// Lead-in path from the pierce point to points[0] (line, line+arc or arc).
    pub lead_path: Vec<P>,
    /// End of the lead-out (None = no lead-out) …
    pub lead_out: Option<P>,
    /// … and its whole path from the overlap point (straight, arc or arc + straight).
    pub lead_out_path: Vec<P>,
    /// Loops at sharp corners: (vertex index, extra points cut after that vertex).
    pub loops: Vec<(usize, Vec<P>)>,
    pub points: Vec<P>,
    pub inner: bool,
    pub part_no: usize,
    /// Micro-joints as (start, end) distances along the contour from points[0].
    pub joints: Vec<(f64, f64)>,
    /// Per joint, when the joint type has leads: (lead-out, lead-in) paths.
    pub joint_leads: Vec<(Vec<P>, Vec<P>)>,
    /// Cut by hand (picked in manual machining).
    pub manual: bool,
    /// Feed rate for this contour (mm/min), from the part's cut quality.
    pub feed: f64,
    /// Punched instead of cut (combined and punching machines): the tool and
    /// its hits.
    pub punch: Option<crate::machine::Punch>,
    /// Cut past the start by this much before the lead-out (CT1 B156), mm.
    pub overlap: f64,
    /// The contour starts at its first joint's lead-in and ends at that
    /// joint's lead-out (rule MJ5): the
    /// last joint closes the contour.
    pub closing: bool,
    /// Kerf compensation (KC1–KC6): the scrap side, the control's corrector,
    /// and whether `points`, the leads and the joints are already the offset
    /// path of the beam's centre (system compensation).
    pub comp: crate::kerf::ContourComp,
    /// The cut quality 1–12 the machine's tables use for this contour (AL1, TT8).
    pub quality: usize,
    /// Its machining time by kind, rapid to it not included (CTM1–CTM12).
    pub time: TimeSplit,
    /// Its pierces: one per beam run (CTM10); 0 when punched.
    pub pierces: usize,
    /// Which of its part's contours it is (cut holes first, outline last, as
    /// `PartTech::starts` and `lead_edits` count them).
    pub contour: usize,
    /// The lead record (0–7, by contour class) its leads were built from.
    pub lead_rec: usize,
    /// Its lead-in or lead-out end was placed by hand (LE1).
    pub lead_hand: bool,
    /// Where its lead-in and lead-out meet the drawn contour, and their ends
    /// (LE1, LE2): what a drag and part-info measure from, compensation or not.
    pub lead_spots: [crate::leads::LeadSpot; 2],
}

/// Point at distance `d` along the closed polyline `pts` (starting at pts[0]).
pub fn point_at(pts: &[P], d: f64) -> P {
    let n = pts.len();
    let mut acc = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let l = dist(a, b);
        if acc + l >= d || i == n - 1 {
            let t = if l > 0.0 { ((d - acc) / l).clamp(0.0, 1.0) } else { 0.0 };
            return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        }
        acc += l;
    }
    pts[0]
}

/// Points along a closed polyline at distances that never go backwards: one
/// pass over the edges for a whole walk (`point_at` starts from the
/// beginning every time).
struct Walker<'a> {
    pts: &'a [P],
    edge: usize,
    start: f64,
}

impl<'a> Walker<'a> {
    fn new(pts: &'a [P]) -> Self {
        Walker { pts, edge: 0, start: 0.0 }
    }

    /// The point at distance `d` from pts[0]; `d` at least the last asked.
    fn at(&mut self, d: f64) -> P {
        let n = self.pts.len();
        while self.edge + 1 < n {
            let l = dist(self.pts[self.edge], self.pts[(self.edge + 1) % n]);
            if self.start + l >= d {
                break;
            }
            self.start += l;
            self.edge += 1;
        }
        let (a, b) = (self.pts[self.edge], self.pts[(self.edge + 1) % n]);
        let l = dist(a, b);
        let t = if l > 0.0 { ((d - self.start) / l).clamp(0.0, 1.0) } else { 0.0 };
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}

/// Sharp outer corners (interior angle at most this) get a loop (estimate).
pub const LOOP_MAX_ANGLE_DEG: f64 = 95.0;

fn signed_area2(pts: &[P]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum()
}

/// Interior angle (degrees) at vertex i, and whether the vertex is convex.
pub fn corner_at(pts: &[P], i: usize) -> (f64, bool) {
    let n = pts.len();
    let (pa, v, pb) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
    let a = norm([v[0] - pa[0], v[1] - pa[1]]);
    let b = norm([pb[0] - v[0], pb[1] - v[1]]);
    let cross = a[0] * b[1] - a[1] * b[0];
    let dot = (a[0] * b[0] + a[1] * b[1]).clamp(-1.0, 1.0);
    let turn = dot.acos().to_degrees(); // 0 = straight on
    let ccw = signed_area2(pts) >= 0.0;
    let convex = if ccw { cross > 0.0 } else { cross < 0.0 };
    let interior = if convex { 180.0 - turn } else { 180.0 + turn };
    (interior, convex)
}

/// Nearest point on the closed polyline to `p`: (edge index, point, distance along the contour).
fn project_on(pts: &[P], p: P) -> (usize, P, f64) {
    let n = pts.len();
    let mut best = (0usize, pts[0], f64::MAX, 0.0);
    let mut acc = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let l2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        let d = dist(p, q);
        if d < best.2 {
            best = (i, q, d, acc + l2.sqrt() * t);
        }
        acc += l2.sqrt();
    }
    (best.0, best.1, best.3)
}

/// Make the contour start at the point nearest `p` (inserting it if needed) —
/// used for lead-ins picked on an element.
pub fn start_at_point(mut pts: Vec<P>, p: P) -> Vec<P> {
    let (i, q, _) = project_on(&pts, p);
    let n = pts.len();
    let k = if dist(q, pts[i]) < 1e-6 {
        i
    } else if dist(q, pts[(i + 1) % n]) < 1e-6 {
        (i + 1) % n
    } else {
        pts.insert(i + 1, q);
        i + 1
    };
    pts.rotate_left(k);
    pts
}

/// Marking runs faster than cutting (estimate).
pub const MARK_SPEED_FACTOR: f64 = 2.0;

/// Lead angle against the contour, in degrees (original default 45°).
pub const LEAD_ANGLE_DEG: f64 = 45.0;

/// Zone 5 ("a given point") for automatic leads: the part-level caller passes
/// (0, 0); we use the sheet origin.
pub const LEAD_ZONE5_POINT: P = [0.0, 0.0];

/// The technology a part without its own is cut with: the machine's
/// automatic leads by contour class, no joints.
pub fn auto_tech() -> PartTech {
    PartTech { lead_in: 0.0, lead_out: 0.0, micro_joints: 0, joint_width: 0.0, quality: None, lead_type: LeadType::Machine, corner_start: true, position: 0, ..PartTech::default() }
}

/// Part `idx`'s machining on its own, at its drawing's origin (the Part view,
/// part-info): no sheet around it, so no sheet edge moves or shortens its
/// leads (VP3 applies on a sheet only).
pub fn plan_part(parts: &[Part], idx: usize, spec: &SheetSpec) -> SheetPlan {
    let (w, h) = parts[idx].bbox();
    let placed = Placed { part: idx, x: 0.0, y: 0.0, w, h, turns: 0, mirror: false };
    plan(parts, &[placed], &SheetSpec { w: 0.0, h: 0.0, ..spec.clone() })
}

/// The eight lead records a part uses: the machine's tables (`LeadType::Machine`)
/// or one of the original's types with the part's own distances (our mapping:
/// Line → 5 inclined, Arc → 8 tangent arc in, LineArc → 13, Perpendicular → 1,
/// Corner → 10). Other shapes fall back to a perpendicular lead (contour 2).
pub fn lead_recs(tech: &PartTech, lq: &crate::leads::LeadQuality) -> [LeadRec; 8] {
    let (din, dout) = (tech.lead_in.max(0.0), tech.lead_out.max(0.0));
    let a = LEAD_ANGLE_DEG;
    let one = match tech.lead_type {
        LeadType::Machine => return lq.recs,
        LeadType::Line => LeadRec::new(5, (din, dout), (a, a), (0.0, 0.0)),
        LeadType::Arc => LeadRec::new(8, (din, dout), (a, a), (din, 0.0)),
        LeadType::LineArc => LeadRec::new(13, (din / 2.0, dout / 2.0), (0.0, 0.0), (din / 2.0, dout / 2.0)),
        LeadType::Perpendicular => LeadRec::new(1, (din, dout), (90.0, 90.0), (0.0, 0.0)),
        LeadType::Corner => LeadRec::new(10, (din, dout), (a, a), (0.0, 0.0)),
    };
    let one = LeadRec { overlap: tech.overlap, ..one };
    let mut r = [one; 8];
    r[6] = LeadRec { kind: 1, ..one };
    r[7].kind = 0;
    r
}

fn bbox_of(pts: &[P]) -> [f64; 4] {
    pts.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// Does a lead (anchored on contour `ci` of placed part `i` at `anchor`) cross a
/// contour, or start inside material? `all` = test every part on the sheet
/// (the original's lead overlap test, rule AL11); otherwise only its own part.
#[allow(clippy::too_many_arguments)]
fn lead_hits(path: &[P], anchor: P, i: usize, ci: usize, inner: bool, obst: &[([f64; 4], Vec<(Vec<P>, bool)>)], all: bool, tol: f64) -> bool {
    if path.len() < 2 {
        return false;
    }
    let lb = bbox_of(path);
    let free = if dist(path[0], anchor) > dist(path[path.len() - 1], anchor) { path[0] } else { path[path.len() - 1] };
    let trim = (10.0 * tol).max(0.05);
    for (j, (b, cs)) in obst.iter().enumerate() {
        if j != i && (!all || b[0] > lb[2] + tol || lb[0] > b[2] + tol || b[1] > lb[3] + tol || lb[1] > b[3] + tol) {
            continue;
        }
        let polys: Vec<&[P]> = cs.iter().enumerate().filter(|(k, _)| j != i || *k != ci).map(|(_, c)| c.0.as_slice()).collect();
        if crate::leads::path_hits(path, anchor, &polys, trim) {
            return true;
        }
        if j == i {
            // Its own contour: a polyline only approximates arcs, so a lead arriving
            // tangentially may graze the chords next to its foot. Those first
            // 2 segment lengths (at most half the lead) don't count (our tolerance).
            let own = &cs[ci].0;
            let n = own.len();
            let seg = (0..n).map(|k| (own[k], own[(k + 1) % n])).map(|(a, b)| (dist(a, b), seg_dist(anchor, a, b))).min_by(|x, y| x.1.total_cmp(&y.1)).map_or(0.0, |x| x.0);
            let own_trim = (2.0 * seg).min(crate::leads::path_len(path) / 2.0).max(trim);
            if crate::leads::path_hits(path, anchor, &[own.as_slice()], own_trim) {
                return true;
            }
        }
        // The free end must lie in scrap: outside every part's material; a hole's
        // lead inside its own hole.
        let outline = &cs[cs.len() - 1].0;
        let in_hole = cs[..cs.len() - 1].iter().any(|h| crate::nest::point_in_poly(&h.0, free));
        if j == i && inner {
            if !crate::nest::point_in_poly(&cs[ci].0, free) {
                return true;
            }
        } else if crate::nest::point_in_poly(outline, free) && !in_hole {
            return true;
        }
    }
    false
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, [a[0] + ab[0] * t, a[1] + ab[1] * t])
}

/// Elements as (start, end) distances along the contour, from the break list.
fn element_spans(breaks: &[f64], per: f64) -> Vec<(f64, f64)> {
    if breaks.is_empty() {
        return vec![(0.0, per)];
    }
    let mut v: Vec<f64> = breaks.to_vec();
    if v[0] > 1e-9 {
        v.insert(0, 0.0);
    }
    v.push(per);
    v.windows(2).filter(|w| w[1] - w[0] > 1e-9).map(|w| (w[0], w[1])).collect()
}

/// Number of polyline segments between two distances along the contour.
fn segment_count(pts: &[P], s0: f64, s1: f64) -> usize {
    let n = pts.len();
    let mut acc = 0.0;
    let mut k = 0;
    for i in 0..n {
        let l = dist(pts[i], pts[(i + 1) % n]);
        if acc + l > s0 + 1e-9 && acc < s1 - 1e-9 {
            k += 1;
        }
        acc += l;
    }
    k
}

/// The vertex at distance `d` along the contour (nearest).
fn vertex_at_distance(pts: &[P], d: f64) -> usize {
    let n = pts.len();
    let mut acc = 0.0;
    let mut best = (0, f64::MAX);
    for i in 0..n {
        if (acc - d).abs() < best.1 {
            best = (i, (acc - d).abs());
        }
        acc += dist(pts[i], pts[(i + 1) % n]);
    }
    if (acc - d).abs() < best.1 {
        best = (0, 0.0);
    }
    best.0
}

/// Cut length of a planned contour: the contour less its micro-joint gaps,
/// plus its leads, loops, joint leads and overlap (a negative overlap = the
/// cut stops that much before its start, KC13).
pub fn contour_cut_len(c: &Contour) -> f64 {
    let per = perimeter(&c.points);
    let uncut: f64 = c.joints.iter().map(|j| j.1 - j.0).sum();
    let lead = if c.closing { 0.0 } else { crate::leads::path_len(&c.lead_path) };
    let joint_leads: f64 = c.joint_leads.iter().map(|(o, i)| crate::leads::path_len(o) + crate::leads::path_len(i)).sum();
    let loops: f64 = c
        .loops
        .iter()
        .map(|(i, l)| {
            let mut prev = c.points[*i];
            l.iter()
                .map(|q| {
                    let d = dist(prev, *q);
                    prev = *q;
                    d
                })
                .sum::<f64>()
        })
        .sum();
    per - uncut + lead + crate::leads::path_len(&c.lead_out_path) + loops + joint_leads + c.overlap
}

/// The beam's path for one contour, as the program cuts it (CTM1): each run
/// starts with a pierce and ends with the beam off; the head jumps from the
/// end of one run to the start of the next across a micro-joint. Follows
/// `program_mm`'s walk (lead-in, contour, joints with or without their own
/// leads, loops, overlap, lead-out).
pub fn beam_runs(c: &Contour) -> Vec<Vec<P>> {
    let mut runs: Vec<Vec<P>> = vec![];
    let mut run: Vec<P> = if c.lead_path.len() >= 2 { c.lead_path.clone() } else { vec![c.lead_in] };
    let n = c.points.len();
    if n == 0 {
        return vec![];
    }
    if dist(*run.last().unwrap(), c.points[0]) > 1e-6 {
        run.push(c.points[0]);
    }
    let mut events: Vec<f64> = vec![];
    let mut vertex_at: Vec<f64> = vec![0.0];
    let mut acc = 0.0;
    for k in 0..n {
        acc += dist(c.points[k], c.points[(k + 1) % n]);
        events.push(acc);
        vertex_at.push(acc);
    }
    for j in &c.joints {
        events.push(j.0);
        events.push(j.1);
    }
    if c.overlap < 0.0 {
        let end = acc + c.overlap;
        events.retain(|&e| e < end - 1e-6);
        events.push(end);
    }
    events.sort_by(|a, b| a.partial_cmp(b).unwrap());
    events.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut prev = 0.0;
    let mut prev_pt = c.points[0];
    let mut walk = Walker::new(&c.points);
    let mut beam_on = true;
    let mut done_joint: Option<usize> = None;
    let mut ended = false;
    for d in events {
        let p = walk.at(d);
        let mid = (prev + d) / 2.0;
        let joint_k = c.joints.iter().position(|j| mid > j.0 && mid < j.1);
        if joint_k.is_some() && joint_k == done_joint {
            (prev, prev_pt) = (d, p);
            continue;
        }
        if let Some((out, back)) = joint_k.and_then(|k| c.joint_leads.get(k)).filter(|_| beam_on) {
            done_joint = joint_k;
            run.extend(out.iter().skip(1).copied());
            if c.closing && joint_k == Some(c.joints.len() - 1) {
                ended = true;
                break;
            }
            runs.push(std::mem::take(&mut run));
            run = back.iter().take(2).copied().collect();
            (prev, prev_pt) = (d, p);
            continue;
        }
        if joint_k.is_some() {
            if beam_on {
                runs.push(std::mem::take(&mut run));
                beam_on = false;
            }
        } else {
            if !beam_on {
                // Pierce again where the gap ends.
                run = vec![prev_pt];
                beam_on = true;
            }
            run.push(p);
            for (vi, pts) in &c.loops {
                if (vertex_at[*vi] - d).abs() < 1e-6 {
                    run.extend(pts.iter().copied());
                }
            }
        }
        (prev, prev_pt) = (d, p);
    }
    if !ended && beam_on {
        if c.overlap > 0.0 {
            run.push(point_at(&c.points, c.overlap));
        }
        run.extend(c.lead_out_path.iter().skip(1).copied());
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

/// The kinds of time in a sheet's machining (CTM14), seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimeSplit {
    pub cut: f64,
    pub mark: f64,
    pub rapid: f64,
    pub pierce: f64,
    /// Head up / down and beam off.
    pub head: f64,
}

impl std::ops::AddAssign for TimeSplit {
    fn add_assign(&mut self, o: TimeSplit) {
        self.cut += o.cut;
        self.mark += o.mark;
        self.rapid += o.rapid;
        self.pierce += o.pierce;
        self.head += o.head;
    }
}

impl TimeSplit {
    pub fn total(&self) -> f64 {
        self.cut + self.mark + self.rapid + self.pierce + self.head
    }
}

pub struct SheetPlan {
    pub contours: Vec<Contour>,
    /// Contours system compensation can't cut (fully or in part), our check:
    /// (part number, hole?, what).
    pub kerf_issues: Vec<(usize, bool, String)>,
    /// Marking lines per part number (from drawing layers set to "Mark").
    pub marks: Vec<(usize, Vec<P>)>,
    /// Per marking line: how many contours are cut before it (program order).
    pub mark_before: Vec<usize>,
    pub mark_len: f64,
    pub cut_len: f64,
    pub rapid_len: f64,
    pub pierces: usize,
    pub seconds: f64,
    /// `seconds` by kind (CTM14).
    pub times: TimeSplit,
    /// How this sheet is timed: its speeds and the machine's time settings.
    pub timing: Timing,
}

/// How a sheet's machining is timed (MECHANICS "Machining time and speeds"):
/// its speeds (the machine's technology table or default speeds, CTM8–CTM10)
/// and the machine's time settings (accelerations, corners, head, CTM2–CTM12).
#[derive(Clone, Debug)]
pub struct Timing {
    pub speeds: crate::techtable::Speeds,
    pub settings: crate::cuttime::TimeSettings,
    rapid_acc: crate::cuttime::Accel,
    line_acc: crate::cuttime::Accel,
}

impl Timing {
    pub fn for_spec(spec: &SheetSpec, mt: &crate::ct1::MachineTech) -> Timing {
        let files = &mt.speed_files;
        let keys = crate::techtable::Keys { material: crate::ct1::key_of_material(spec.material), thickness: spec.thickness, gas: files.gas };
        let speeds = crate::techtable::Speeds::resolve(files, &keys, mt.cut_speed, cut_speed(spec.material, spec.thickness), mt.mark_speed, mt.rapid_speed);
        let settings = files.time.clone();
        let rapid_acc = crate::cuttime::accel_for(&settings.rapid_acc, spec.thickness);
        let line_acc = crate::cuttime::accel_for(&settings.line_acc, spec.thickness);
        Timing { speeds, settings, rapid_acc, line_acc }
    }

    /// The cutting speed of a contour of quality `q`: the machine's (CTM8);
    /// with no machine speed at all, our typical laser speeds × the quality's
    /// factor (CQ3, estimate).
    pub fn feed(&self, q: CutQuality) -> f64 {
        match self.speeds.source {
            crate::techtable::SpeedSource::Estimate => self.speeds.cut_for(1) * q.speed_factor(),
            _ => self.speeds.cut_for(q.number()),
        }
    }

    /// A rapid move (CTM3), seconds.
    pub fn rapid(&self, a: P, b: P) -> f64 {
        crate::cuttime::rapid_secs(a, b, self.speeds.rapid, self.settings.rapid_straight, &self.rapid_acc) * self.settings.factors[crate::cuttime::F_RAPID]
    }

    /// A marking path (CTM9) with the head going down before and up after, seconds.
    pub fn mark(&self, path: &[P]) -> TimeSplit {
        let s = &self.settings;
        TimeSplit { mark: crate::cuttime::path_secs(path, self.speeds.mark, s.v90, &self.line_acc) * s.factors[crate::cuttime::F_MARK], head: s.head_down + s.head_up, ..Default::default() }
    }

    /// One contour (CTM4, CTM5, CTM10–CTM12): head down, then each beam run —
    /// pierce, cut with the corner slow-downs, beam off — with the jumps over
    /// micro-joints between runs (head kept down), and the head up at the end.
    /// Also returns the contour's pierce count (its beam runs).
    pub fn contour(&self, c: &Contour) -> (TimeSplit, usize) {
        let s = &self.settings;
        let mut t = TimeSplit::default();
        if let Some(p) = &c.punch {
            t.cut = p.hits.len() as f64 * crate::machine::HIT_SECONDS;
            for w in p.hits.windows(2) {
                t.rapid += self.rapid(w[0], w[1]);
            }
            return (t, 0);
        }
        let runs = beam_runs(c);
        let mut last: Option<P> = None;
        for run in &runs {
            if let Some(e) = last {
                t.rapid += self.rapid(e, run[0]);
            }
            t += self.run(c, run);
            last = run.last().copied();
        }
        if !runs.is_empty() {
            t.head += s.head_down + s.head_up;
        }
        (t, runs.len())
    }

    /// One beam run of a contour (CTM10, CTM11): its pierce, the cut along
    /// it, and the beam going off at its end. Head down / up and the jumps
    /// between runs are the contour's (`contour`).
    pub fn run(&self, c: &Contour, run: &[P]) -> TimeSplit {
        let s = &self.settings;
        TimeSplit {
            pierce: self.speeds.pierce_for(c.quality) * s.factors[crate::cuttime::F_PIERCE],
            cut: crate::cuttime::path_secs(run, c.feed, s.v90, &self.line_acc) * s.factors[crate::cuttime::F_CUT],
            head: s.cut_off,
            ..Default::default()
        }
    }
}

/// Where the beam stops on a contour: the end of its last run (after the
/// lead-out, the overlap or the closing joint's lead-out). The next rapid
/// starts here.
pub fn beam_end(c: &Contour) -> P {
    beam_runs(c).last().and_then(|r| r.last().copied()).unwrap_or(c.lead_in)
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn norm(v: P) -> P {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l]
}

/// Order parts nearest-first from the machine origin; inside each part cut the
/// holes first and the outer contour last so the part never drops early.
pub fn plan(parts: &[Part], placed: &[Placed], spec: &SheetSpec) -> SheetPlan {
    plan_manual(parts, placed, spec, &[])
}

/// A contour cut by hand (the original's manual machining, rule MM1: contours
/// labelled ##CNTMAN / ##MANINT / ##MANCOM). `contour` counts the part's holes
/// first, the outer contour last; `start` is where it was picked (sheet mm).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ManualCut {
    pub placed: usize,
    pub contour: usize,
    pub start: P,
    pub reverse: bool,
}

/// Contours of a placed part on the sheet: holes first, then the outer contour.
pub fn sheet_contours(part: &Part, pl: &Placed) -> Vec<(Vec<P>, bool)> {
    let g = part.geometry();
    let mut list: Vec<(Vec<P>, bool)> = g.holes.iter().map(|h| (h.iter().map(|&p| pl.map(p)).collect::<Vec<P>>(), true)).collect();
    list.push((g.outline.iter().map(|&p| pl.map(p)).collect(), false));
    list
}

/// The contour nearest a point within `tol` (sheet mm): (placed index, contour, point on it).
/// Like picking a contour on the sheet (rule MM4).
pub fn nearest_contour(parts: &[Part], placed: &[Placed], p: P, tol: f64) -> Option<(usize, usize, P)> {
    let mut best: Option<(usize, usize, P, f64)> = None;
    for (i, pl) in placed.iter().enumerate() {
        if p[0] < pl.x - tol || p[0] > pl.x + pl.w + tol || p[1] < pl.y - tol || p[1] > pl.y + pl.h + tol {
            continue;
        }
        for (ci, (pts, _)) in sheet_contours(&parts[pl.part], pl).iter().enumerate() {
            let (_, q, _) = project_on(pts, p);
            let d = dist(p, q);
            if d <= tol && best.is_none_or(|b| d < b.3) {
                best = Some((i, ci, q, d));
            }
        }
    }
    best.map(|b| (b.0, b.1, b.2))
}

/// The cutting jobs in order: manual picks first, as picked. Before a part's outer
/// contour, any of its holes not cut yet go first (a hole is every contour not
/// labelled ##EXTCUT, rule MM2, and holes are cut before
/// the outside so the part doesn't drop). A contour is cut once
/// (rule MM3). A picked hole, or a hole pulled in by a picked outside, first
/// cuts every part sitting in it, whole, so none drops with its scrap (CO2).
/// Then the rest automatically.
pub fn cut_jobs(parts: &[Part], placed: &[Placed], order: &[usize], manual: &[ManualCut]) -> Vec<CutJob> {
    let mut jobs: Vec<CutJob> = vec![];
    let mut busy = vec![false; placed.len()];
    for m in manual {
        if m.placed >= placed.len() || m.contour >= contour_count(parts, placed, m.placed) || job_done(&jobs, m.placed, m.contour) {
            continue;
        }
        if m.contour + 1 == contour_count(parts, placed, m.placed) {
            // The outside: the whole part, holes first (MM2).
            cut_whole(parts, placed, manual, m.placed, &mut jobs, &mut busy);
        } else {
            busy[m.placed] = true;
            pull_in(parts, placed, manual, m.placed, m.contour, &mut jobs, &mut busy);
            busy[m.placed] = false;
            jobs.push((m.placed, m.contour, Some((m.start, m.reverse))));
        }
    }
    for &i in order {
        for ci in 0..contour_count(parts, placed, i) {
            if !job_done(&jobs, i, ci) {
                jobs.push((i, ci, None));
            }
        }
    }
    jobs
}

/// One cut: placed part, contour (holes first, outside last), and for a
/// picked contour where it starts and whether it runs the other way.
pub type CutJob = (usize, usize, Option<(P, bool)>);

fn contour_count(parts: &[Part], placed: &[Placed], i: usize) -> usize {
    parts[placed[i].part].geometry().holes.len() + 1
}

fn job_done(jobs: &[CutJob], i: usize, ci: usize) -> bool {
    jobs.iter().any(|j| j.0 == i && j.1 == ci)
}

/// What is left of placed part `i`: each hole after the parts sitting in it,
/// then the outside. A contour picked by hand keeps its start and direction.
fn cut_whole(parts: &[Part], placed: &[Placed], manual: &[ManualCut], i: usize, jobs: &mut Vec<CutJob>, busy: &mut [bool]) {
    busy[i] = true;
    let n = contour_count(parts, placed, i);
    for ci in 0..n {
        if job_done(jobs, i, ci) {
            continue;
        }
        if ci + 1 < n {
            pull_in(parts, placed, manual, i, ci, jobs, busy);
        }
        let pick = manual.iter().find(|m| m.placed == i && m.contour == ci).map(|m| (m.start, m.reverse));
        jobs.push((i, ci, pick));
    }
    busy[i] = false;
}

/// CO2: every part sitting in hole `h` of placed part `i` is cut whole before
/// that hole, or it drops with the hole's scrap uncut. `busy` marks the parts
/// being cut, so parts moved over each other by hand can't loop.
fn pull_in(parts: &[Part], placed: &[Placed], manual: &[ManualCut], i: usize, h: usize, jobs: &mut Vec<CutJob>, busy: &mut [bool]) {
    let hole = sheet_contours(&parts[placed[i].part], &placed[i]).swap_remove(h).0;
    let lo = hole.iter().fold([f64::MAX; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
    let hi = hole.iter().fold([f64::MIN; 2], |a, p| [a[0].max(p[0]), a[1].max(p[1])]);
    for (j, pl) in placed.iter().enumerate() {
        if j == i || busy[j] {
            continue;
        }
        // Parts never overlap, so one point of the outline in the hole puts the part in it.
        let Some(&first) = parts[pl.part].geometry().outline.first() else { continue };
        let q = pl.map(first);
        if q[0] >= lo[0] && q[0] <= hi[0] && q[1] >= lo[1] && q[1] <= hi[1] && crate::nest::point_in_poly(&hole, q) {
            cut_whole(parts, placed, manual, j, jobs, busy);
        }
    }
}

/// Where a cut contour's quality comes from (CQ1, CQ6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityFrom {
    /// Set on the contour by hand (CQ6).
    Hand,
    /// The part's own size-range row that holds the contour (CT1).
    SizeRange,
    /// The part's own quality.
    Part,
    /// The machine section's size band (MTL10, TT8).
    SizeBand,
    /// Nothing sets one: quality 3 (estimate).
    Fallback,
}

impl QualityFrom {
    /// For people and agents (part-info, Cut technology by size).
    pub fn label(self) -> &'static str {
        match self {
            QualityFrom::Hand => "by hand",
            QualityFrom::SizeRange => "part size range",
            QualityFrom::Part => "part",
            QualityFrom::SizeBand => "machine size band",
            QualityFrom::Fallback => "default",
        }
    }
}

/// A closed contour's size as the size ranges and bands measure it: the
/// radius of the circle with the same area, mm (CT1).
pub fn contour_radius(pts: &[P]) -> f64 {
    crate::nest::equivalent_radius(signed_area2(pts) / 2.0)
}

/// The cut quality of a closed contour `pts` of a part with technology
/// `tech` on the machine's tables `mt` (CQ1): the quality set on the contour
/// by hand, `hand` (CQ6); else the part's own size-range row for the
/// contour's equivalent radius; else the part's own quality; else, when the
/// part has no size ranges of its own, the machine's size band; else quality 3.
pub fn contour_quality(tech: &PartTech, mt: &crate::ct1::MachineTech, pts: &[P], hand: Option<CutQuality>) -> (CutQuality, QualityFrom) {
    if let Some(q) = hand {
        return (q, QualityFrom::Hand);
    }
    let r_eq = contour_radius(pts);
    if let Some(q) = crate::nest::quality_for_radius(&tech.cut_ranges, r_eq, 0.0) {
        return (q, QualityFrom::SizeRange);
    }
    if let Some(q) = tech.quality {
        return (q, QualityFrom::Part);
    }
    let own_ranges = tech.cut_ranges.iter().any(|c| c.on);
    match mt.band_quality(r_eq).and_then(CutQuality::new) {
        Some(q) if !own_ranges => (q, QualityFrom::SizeBand),
        _ => (CutQuality::FALLBACK, QualityFrom::Fallback),
    }
}

/// Like `plan`, with some contours cut by hand first.
pub fn plan_manual(parts: &[Part], placed: &[Placed], spec: &SheetSpec, manual: &[ManualCut]) -> SheetPlan {
    let mut left: Vec<usize> = (0..placed.len()).collect();
    let mut order = Vec::new();
    let mut at: P = [0.0, 0.0];
    // A part nested inside another part's hole is cut before that part,
    // or it would drop with the scrap of the hole. With parts in holes of
    // parts in holes (rings in rings) the deepest goes first.
    let depth = |i: usize| {
        let p = &placed[i];
        placed.iter().enumerate().filter(|(j, o)| *j != i && o.contains([p.x, p.y]) && o.contains([p.x + p.w, p.y + p.h])).count()
    };
    let inside_other = |i: usize| depth(i) > 0;
    // Zone by zone (rules CZ4, CZ5): the parts of each zone, nearest first,
    // with parts sitting in another part's hole before the rest.
    // The machine's tables for this sheet's material and thickness (MTL1–MTL7).
    let mt = crate::ct1::for_spec(spec);
    let zones = crate::ct1::zones(spec.w, spec.h, &mt);
    let mut zi = 0;
    while !left.is_empty() {
        while zi < zones.len() && !left.iter().any(|&i| crate::ct1::box_in_zone(placed[i].bbox4(), zones[zi])) {
            zi += 1;
        }
        let in_zone = |i: usize| zi >= zones.len() || crate::ct1::box_in_zone(placed[i].bbox4(), zones[zi]);
        let (k, _) = left
            .iter()
            .enumerate()
            .filter(|&(_, &i)| in_zone(i) || inside_other(i))
            .map(|(k, &i)| {
                let p = &placed[i];
                let first = -(depth(i) as f64) * 1e9;
                (k, first + dist(at, [p.x, p.y]))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        let i = left.remove(k);
        at = [placed[i].x + placed[i].w, placed[i].y];
        order.push(i);
    }

    // Speeds and time settings for this sheet (CTM1–CTM16, TT1–TT10).
    let timing = Timing::for_spec(spec, &mt);
    let mut times = TimeSplit::default();
    let mut contours = Vec::new();
    let mut cut_len = 0.0;
    let mut rapid_len = 0.0;
    let mut marks: Vec<(usize, Vec<P>)> = vec![];
    let mut mark_len = 0.0;
    let mut mark_before: Vec<usize> = vec![];
    let mut pos: P = [0.0, 0.0];
    let jobs = cut_jobs(parts, placed, &order, manual);
    let mut marked = vec![false; placed.len()];
    let auto_tech = auto_tech();
    let tol = mt.leads.lin_tol;
    // Kerf compensation for this sheet's material and thickness (KC1–KC14).
    let cutter_r = crate::kerf::cutter_radius(&mt);
    let sys_opts = crate::kerf::SystemOpts { corners: mt.comp.corners(), lead_mode: mt.comp.lead_mode, joint_mode: mt.comp.joint_mode, tol: tol.clamp(1e-4, 0.01) };
    let mut kerf_issues: Vec<(usize, bool, String)> = vec![];
    // Every placed part's contours (holes first, outline last), for the leads' collision test.
    let obst: Vec<([f64; 4], Vec<(Vec<P>, bool)>)> = placed.iter().map(|pl| (pl.bbox4(), sheet_contours(&parts[pl.part], pl))).collect();
    // Each part's qualities set by hand, by cut contour (CQ6).
    let hand: Vec<Vec<Option<CutQuality>>> = parts.iter().map(Part::cut_qualities).collect();
    for (i, ci, man) in jobs {
        let pl = &placed[i];
        let part = &parts[pl.part];
        // Parts without their own technology get the machine's automatic leads
        // (tables by contour class) and no joints.
        let tech = part.tech.unwrap_or(auto_tech);
        let (mut pts, inner) = obst[i].1[ci].clone();
        // The contour's cut quality (CQ1, CQ6), which picks its leads (AL1),
        // kerf row (KC5), joint distance, speed (CTM8) and LC5 codes (BL22).
        let (cq, _) = contour_quality(&tech, &mt, &pts, hand[pl.part].get(ci).copied().flatten());
        let qn = cq.number();
        let feed = timing.feed(cq);
        let lq = mt.leads.quality(qn);
        // Marking first, while the part is still held by the sheet.
        if !marked[i] {
            marked[i] = true;
        if let Some(c) = &part.custom {
            for m in &c.marks {
                let pts: Vec<P> = m.iter().map(|&p| pl.map(p)).collect();
                if pts.len() < 2 {
                    continue;
                }
                rapid_len += dist(pos, pts[0]);
                let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                mark_len += l;
                times.rapid += timing.rapid(pos, pts[0]);
                times += timing.mark(&pts);
                pos = *pts.last().unwrap();
                mark_before.push(contours.len());
                marks.push((i + 1, pts));
            }
        }
        // Marking and special texts are marked with the marking technology;
        // notes are not machined (rule T1).
        let material = crate::cnc::MATERIALS.get(spec.material).copied().unwrap_or("");
        for t in part.texts.iter().filter(|t| t.kind.machined()) {
            let txt = t.resolved(part, material, spec.thickness, i + 1);
            for pts in t.sheet_strokes(&txt, pl, crate::nest::TEXT_ANGLE_STEP) {
                rapid_len += dist(pos, pts[0]);
                let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                mark_len += l;
                times.rapid += timing.rapid(pos, pts[0]);
                times += timing.mark(&pts);
                pos = *pts.last().unwrap();
                mark_before.push(contours.len());
                marks.push((i + 1, pts));
            }
        }
        }
        {
            // A manual pick sets the start and direction; then a lead moved by
            // hand (LE4); then a lead-in picked on an element; otherwise the
            // automatic placement by contour class.
            let edit = tech.lead_edit(ci).filter(|_| man.is_none());
            let picked = tech.starts.iter().flatten().find(|s| s.0 as usize == ci).map(|s| pl.map(s.1)).filter(|_| edit.is_none());
            let (class, circle) = crate::leads::classify(&pts, inner);
            let recs = lead_recs(&tech, &lq);
            let zone = lq.zone_for(inner, (tech.position >= 1).then_some(tech.position as i32));
            let auto = tech.corner_start && man.is_none() && picked.is_none() && edit.is_none() && pts.len() >= 3;
            let mut placement = None;
            if let Some((sp, rev)) = man {
                if rev {
                    pts.reverse();
                }
                pts = start_at_point(pts, sp);
            } else if let Some(e) = edit {
                pts = start_at_point(pts, pl.map(e.at));
            } else if let Some(mp) = picked {
                pts = start_at_point(pts, mp);
            } else if auto {
                placement = crate::leads::place(&pts, inner, class, circle, &lq, &recs, zone, LEAD_ZONE5_POINT, tol, mt.leads.ang_tol);
                if let Some(p) = placement {
                    pts = start_at_point(pts, p.at);
                }
            }
            // Punching / combined machines: a standard shape is one hit, other
            // contours on a punching machine are nibbled (see machine.rs).
            if let Some(p) = crate::machine::punch_for(spec.machine, &pts, inner) {
                if let Some(&h) = p.hits.first() {
                    times.rapid += timing.rapid(pos, h);
                }
                for &h in &p.hits {
                    rapid_len += dist(pos, h);
                    pos = h;
                }
                let first = p.hits.first().copied().unwrap_or(pts[0]);
                let mut con = Contour { lead_in: first, lead_path: vec![], lead_out: None, lead_out_path: vec![], loops: vec![], points: pts, inner, part_no: i + 1, joints: vec![], joint_leads: vec![], feed, manual: man.is_some(), punch: Some(p), overlap: 0.0, closing: false, comp: Default::default(), quality: qn, time: TimeSplit::default(), pierces: 0, contour: ci, lead_rec: class.rec(), lead_hand: false, lead_spots: Default::default() };
                (con.time, con.pierces) = timing.contour(&con);
                times += con.time;
                contours.push(con);
                continue;
            }
            // The lead record: the placement's, else the one a lead moved by
            // hand was built from (LE4), else the class's. An automatic
            // placement that found nothing gets no lead (AL3); a corner
            // type away from a corner is built perpendicular (our choice).
            let lead_rec = placement.map_or(edit.map_or(class.rec(), |e| (e.rec as usize).min(7)), |p| p.rec);
            let mut rec = recs[lead_rec];
            if auto && placement.is_none() {
                rec.kind = 0;
            }
            let at_corner = |q: &[P]| (corner_at(q, 0).0 - 180.0).abs() > crate::nest::ELEMENT_CORNER_DEG;
            if matches!(rec.kind, 10 | 11) && !placement.is_some_and(|p| p.corner) && !at_corner(&pts) {
                rec.kind = 1;
            }
            let hand = edit.map_or_else(crate::leads::Hand::default, |e| crate::leads::Hand { lead_in: e.lead_in, lead_out: e.lead_out });
            let mut geo = crate::leads::build_hand(&pts, inner, &rec, circle, lq.limit_to_centre, tol, hand);
            // Collision test (B185, AL11): automatic leads move along the
            // contour (B187 tries) and/or shorten (B186 per step, floor
            // max(2·tol, B295)) until clear of every contour on the sheet. Without
            // the test (or for picked leads) a lead is still shortened when it
            // crosses its own part (our safety rule). A lead moved by hand
            // keeps its start and is only shortened, against every part when
            // the machine tests leads (LE5).
            let full = (auto || hand.is_some()) && mt.leads.test > 0;
            // VP3: a lead that leaves the sheet collides with its edge, so it
            // moves or shortens like one that hits a part (ours; MAC-46).
            let off_sheet = |path: &[P]| spec.w > 0.0 && spec.h > 0.0 && path.iter().any(|q| q[0] < -0.01 || q[1] < -0.01 || q[0] > spec.w + 0.01 || q[1] > spec.h + 0.01);
            let hits = |path: &[P], anchor: P, all: bool| off_sheet(path) || lead_hits(path, anchor, i, ci, inner, &obst, all, tol);
            let floor = (2.0 * tol).max(lq.min_lead);
            let (mv, sh) = if full && auto { crate::leads::test_steps(mt.leads.test) } else { (false, true) };
            let bad = |g: &crate::leads::LeadGeo, q: &[P]| hits(&g.lead_in, q[0], full) || (!g.lead_out.is_empty() && hits(&g.lead_out, g.lead_out[0], full));
            if mv && bad(&geo, &pts) {
                let per0 = perimeter(&pts);
                let tries = mt.leads.tries.max(1);
                for k in 1..tries {
                    let p2 = start_at_point(pts.clone(), point_at(&pts, per0 * k as f64 / tries as f64));
                    let mut r2 = rec;
                    if matches!(r2.kind, 10 | 11) && !at_corner(&p2) {
                        r2.kind = 1;
                    }
                    let g2 = crate::leads::build(&p2, inner, &r2, circle, lq.limit_to_centre, tol);
                    if !bad(&g2, &p2) {
                        pts = p2;
                        geo = g2;
                        break;
                    }
                }
            }
            if sh {
                let a = pts[0];
                geo.lead_in = crate::leads::shorten_until(&geo.lead_in, a, mt.leads.shorten, floor, |p| !hits(p, a, full));
                if let Some(&o) = geo.lead_out.first() {
                    geo.lead_out = crate::leads::shorten_until(&geo.lead_out, o, mt.leads.shorten, floor, |p| !hits(p, o, full));
                }
            }
            // The leads as built on the drawn contour, before joints or kerf
            // compensation move them (LE1, LE2).
            let lead_spots = crate::leads::spots(&pts, inner, circle, &geo, tol);
            let mut path = geo.lead_in.clone();
            let mut lead_len = crate::leads::path_len(&path);
            let mut lead_in = path[0];
            let per = perimeter(&pts);
            let mut overlap = geo.overlap;
            let mut lead_out_path = geo.lead_out.clone();
            let mut lead_out = lead_out_path.last().copied();
            // Micro-joints. Automatic (one per `joint_every` mm, F150): every contour
            // the joint type reaches, holes too (AM3); a fixed
            // number: the outside only (our manual setting).
            let jcode = tech.joint_type.code();
            let gap = tech.joint_width;
            let with_leads = tech.joint_type.has_leads();
            let (reached, corner_mode) = crate::joints::reaches(jcode, circle.is_some());
            let b4 = bbox_of(&pts);
            let (n_joints, clamped) = if tech.joint_every > 0.0 {
                if reached {
                    let (lo, hi) = mt.joint_sizes.limits(b4[2] - b4[0], b4[3] - b4[1], inner);
                    crate::joints::count(per, tech.joint_every, lo, hi, tol)
                } else {
                    (0, false)
                }
            } else if inner {
                (0, false)
            } else {
                (tech.micro_joints, false)
            };
            let breaks = element_breaks(&pts);
            let mut spans: Vec<((f64, f64), bool)> = vec![];
            if n_joints > 0 && gap > 0.0 {
                if corner_mode {
                    for sp in corner_joint_spans(&pts, n_joints, gap) {
                        spans.push((sp, true));
                    }
                } else {
                    let dist_q = mt.joint_dist[qn.clamp(1, 12) - 1];
                    let w = crate::joints::Walk { every: tech.joint_every.max(per / n_joints as f64), corner_min: dist_q[1], between_min: dist_q[2], tol };
                    let elems: Vec<(f64, f64)> = if circle.is_some() { vec![(0.0, per)] } else { element_spans(&breaks, per) };
                    let lens: Vec<f64> = elems.iter().map(|e| e.1 - e.0).collect();
                    for mut c in crate::joints::walk(&lens, n_joints, clamped, &w) {
                        // On a circle the joint's angle snaps to the step (F153, AM4).
                        if let (Some((cc, _)), true) = (circle, dist_q[3] > 0.0 && dist_q[3] >= mt.leads.ang_tol) {
                            let q = point_at(&pts, c);
                            let th = (q[1] - cc[1]).atan2(q[0] - cc[0]).to_degrees();
                            let th = ((th / dist_q[3]).round() * dist_q[3]).to_radians();
                            let r = dist(q, cc);
                            c = project_on(&pts, [cc[0] + r * th.cos(), cc[1] + r * th.sin()]).2;
                        }
                        let k = elems.iter().position(|e| c >= e.0 - 1e-9 && c <= e.1 + 1e-9).unwrap_or(0);
                        let (s0, s1) = elems[k];
                        // A joint on an element end moves 2 × the gap along it (AM4).
                        if c - s0 <= tol && s1 - s0 > 2.0 * gap {
                            c = s0 + 2.0 * gap;
                        }
                        let line = circle.is_none() && segment_count(&pts, s0, s1) == 1;
                        match crate::joints::fit(c, s0, s1, line, gap, with_leads, tol) {
                            crate::joints::Fit::Refused => {}
                            crate::joints::Fit::Edge(a, b) => spans.push(((a, b), false)),
                            crate::joints::Fit::Corner(at) if with_leads => {
                                let vi = vertex_at_distance(&pts, at);
                                let ang = corner_at(&pts, vi).0;
                                spans.push((crate::joints::corner_span(at, ang, gap), true));
                            }
                            crate::joints::Fit::Corner(at) => {
                                let sp = if (at - s0).abs() < 1e-9 { (s0, s0 + gap) } else { (s1 - gap, s1) };
                                spans.push((sp, false));
                            }
                        }
                    }
                }
            }
            // Micro-joints picked on elements of this contour.
            let jw = tech.joint_width.max(0.1);
            for jp in tech.joint_picks.iter().flatten().filter(|j| j.0 as usize == ci) {
                let (_, _, s) = project_on(&pts, pl.map(jp.1));
                let (a, b) = ((s - jw / 2.0).max(0.0), (s + jw / 2.0).min(per));
                if b > a {
                    if let Some(f) = fit_joint(&breaks, per, a, b) {
                        spans.push((f, false));
                    }
                }
            }
            // Spans across the contour's start (the lead point) are refused.
            spans.retain(|((a, b), _)| *a >= -1e-9 && *b <= per + 1e-9 && b > a);
            spans.sort_by(|x, y| x.0 .0.partial_cmp(&y.0 .0).unwrap());
            spans.dedup_by(|x, y| x.0 .0 < y.0 .1 + 1e-6);
            // A joint too near the lead-in point or a joint already placed is
            // refused (rules MJ1, MJ2). The lead point's size is the lead-out length
            // (estimated: the original's field for it isn't known).
            let mut near: Vec<(P, f64)> = if lead_len > 0.0 || tech.lead_in > 0.0 { vec![(pts[0], tech.lead_out.max(0.0))] } else { vec![] };
            let spans: Vec<((f64, f64), bool)> = spans
                .into_iter()
                .filter(|&((a, b), _)| {
                    let c = point_at(&pts, (a + b) / 2.0);
                    let ok = !crate::nest::joint_too_near(c, b - a, &near);
                    if ok {
                        near.push((c, b - a));
                    }
                    ok
                })
                .collect();
            let mut joints: Vec<(f64, f64)> = spans.iter().map(|s| s.0).collect();
            // Joint leads (AM6): free ends D off the contour in the scrap, leaning
            // away from the gap; no lead-out when its D ≤ 2·tol. Without the
            // machine's joint overlap test (F133 ≤ 0) outer leads stay inside 0.8 ×
            // the part gap (our safety rule); with it they shorten by F134 per try.
            let mut joint_leads: Vec<(Vec<P>, Vec<P>)> = if with_leads {
                let reach = if inner || mt.joint_test > 0 { f64::MAX } else { spec.gap * crate::nest::JOINT_LEAD_REACH };
                let d_in = tech.joint_lead.min(reach).max(0.0);
                let d_out = tech.joint_lead_out.unwrap_or(tech.joint_lead).min(reach).max(0.0);
                let full = mt.joint_test > 0;
                let step = if mt.joint_shorten > 0.0 && mt.joint_shorten < 1.0 { mt.joint_shorten } else { mt.leads.shorten };
                spans
                    .iter()
                    .map(|&((a, b), corner)| {
                        let forms = crate::joints::lead_forms(crate::joints::type_at(jcode, corner));
                        let (o, bk) = joint_lead_paths(&pts, inner, a, b, d_in, d_out, tech.joint_angle, forms, tol);
                        let o = if o.len() > 1 { crate::leads::shorten_until(&o, o[0], step, floor, |p| !hits(p, o[0], full)) } else { o };
                        let foot = bk[1];
                        let bk = crate::leads::shorten_until(&bk, foot, step, floor, |p| !hits(p, foot, full));
                        (o, bk)
                    })
                    .collect()
            } else {
                vec![]
            };
            // A contour with joints that have leads starts at its first joint's
            // lead-in (rule MJ5):
            // no separate lead-in; the cut ends at that joint's lead-out.
            let closing = !inner && man.is_none() && picked.is_none() && edit.is_none() && !joint_leads.is_empty();
            if closing {
                let (ja, jb) = joints[0];
                let w = jb - ja;
                let sp = point_at(&pts, jb);
                pts = start_at_point(pts, sp);
                let per2 = perimeter(&pts);
                for j in joints.iter_mut().skip(1) {
                    *j = ((j.0 - jb).rem_euclid(per2), (j.1 - jb).rem_euclid(per2));
                }
                joints[0] = (per2 - w, per2);
                joints.rotate_left(1);
                joint_leads.rotate_left(1);
                let back = joint_leads.last().map(|l| l.1.clone()).unwrap_or_default();
                path = back;
                lead_in = path[0];
                lead_len = 0.0; // counted with the joint leads
                lead_out = None;
                lead_out_path.clear();
                overlap = 0.0;
            }
            // Loops at sharp convex outer corners: run past the corner, swing round
            // outside the part and come back in along the next side.
            let mut loops: Vec<(usize, Vec<P>)> = vec![];
            if tech.loops && !inner {
                let n = pts.len();
                for i in 1..n {
                    let (ang, cvx) = corner_at(&pts, i);
                    if cvx && ang <= LOOP_MAX_ANGLE_DEG {
                        let v = pts[i];
                        let ia = norm([v[0] - pts[i - 1][0], v[1] - pts[i - 1][1]]);
                        let ob = norm([pts[(i + 1) % n][0] - v[0], pts[(i + 1) % n][1] - v[1]]);
                        let s = tech.loop_size;
                        let out = norm([ia[0] - ob[0], ia[1] - ob[1]]);
                        loops.push((
                            i,
                            vec![
                                [v[0] + ia[0] * s, v[1] + ia[1] * s],
                                [v[0] + out[0] * s * 1.4, v[1] + out[1] * s * 1.4],
                                [v[0] - ob[0] * s, v[1] - ob[1] * s],
                                v,
                            ],
                        ));
                    }
                }
            }
            // Kerf compensation (KC1–KC6): the side from the real cutting
            // direction, the corrector, and — with system compensation — the
            // offset path in place of the drawn one (KC8–KC15).
            let side = crate::kerf::side_for(&pts, inner, true);
            let comp = crate::kerf::resolve(&mt.comp, spec.kerf, inner, qn, cutter_r, side);
            let mut con = Contour { lead_in, lead_path: path, lead_out, lead_out_path, loops, points: pts, inner, part_no: i + 1, joints, joint_leads, feed, manual: man.is_some(), punch: None, overlap, closing, comp, quality: qn, time: TimeSplit::default(), pierces: 0, contour: ci, lead_rec, lead_hand: hand.is_some(), lead_spots };
            match crate::kerf::compensate(&mut con, &comp, &sys_opts) {
                Err(t) => {
                    // Not cut at all: no pierce, no time (our check, KC18).
                    kerf_issues.push((i + 1, inner, t.describe(2.0 * comp.radius)));
                    continue;
                }
                Ok(Some(t)) => kerf_issues.push((i + 1, inner, t.describe(2.0 * comp.radius))),
                Ok(None) => {}
            }
            let open_leads = con.joint_leads.len() - usize::from(con.closing);
            for (o, b) in con.joint_leads.iter().take(open_leads) {
                rapid_len += dist(*o.last().unwrap(), b[0]);
            }
            let len = contour_cut_len(&con);
            times.rapid += timing.rapid(pos, con.lead_in);
            rapid_len += dist(pos, con.lead_in);
            cut_len += len;
            (con.time, con.pierces) = timing.contour(&con);
            times += con.time;
            // The next rapid starts where the beam stopped (after the
            // lead-out, the overlap, or the closing joint's lead-out).
            pos = beam_end(&con);
            contours.push(con);
        }
    }
    // Every beam-on is a pierce, joint re-pierces included (CTM10).
    let pierces: usize = contours.iter().map(|c| c.pierces).sum();
    SheetPlan { contours, kerf_issues, marks, mark_before, mark_len, cut_len, rapid_len, pierces, seconds: times.total(), times, timing }
}

/// Write the sheet's machine program as ISO G-code.
#[cfg_attr(not(test), allow(dead_code))]
pub fn program(sheet_no: usize, parts: &[Part], placed: &[Placed], spec: &SheetSpec, plan: &SheetPlan) -> String {
    program_lines(sheet_no, parts, placed, spec, plan, &[], &crate::post::PostConfig::default())
}

/// The program plus cut lines that belong to no part (free paths, then the
/// remnant line), cut after every part.
pub fn program_lines(sheet_no: usize, parts: &[Part], placed: &[Placed], spec: &SheetSpec, plan: &SheetPlan, lines: &[(String, Vec<P>)], post: &crate::post::PostConfig) -> String {
    // Written in mm with N10, N20… then put into the machine's post form.
    post.apply(&program_mm(sheet_no, parts, placed, spec, plan, lines, post.circles_one_move))
}

fn program_mm(sheet_no: usize, parts: &[Part], placed: &[Placed], spec: &SheetSpec, plan: &SheetPlan, lines: &[(String, Vec<P>)], circles_one_move: bool) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let mut n = 10;
    let mut line = |s: &mut String, t: String| {
        let _ = writeln!(s, "N{n} {t}");
        n += 10;
    };
    let _ = writeln!(
        s,
        "( SHEET {} - {:.0} x {:.0} x {} mm {} )",
        sheet_no,
        spec.w,
        spec.h,
        spec.thickness,
        MATERIALS[spec.material.min(2)].to_uppercase()
    );
    let _ = writeln!(s, "( {} PARTS, {} PIERCES, CUT {:.0} MM )", placed.len(), plan.pierces, plan.cut_len);
    let _ = writeln!(s, "( MACHINE {} - TYPE {} )", spec.machine.label().to_uppercase(), spec.machine.family().code());
    if let Some(c) = plan.contours.iter().find(|c| c.comp.control || c.comp.system) {
        let how = match (c.comp.control, c.comp.system) {
            (true, true) => "CONTROL G41/G42 AND OFFSET PATH",
            (true, false) => "CONTROL G41/G42",
            _ => "OFFSET PATH",
        };
        let _ = writeln!(s, "( KERF {:.2} MM - {how} )", 2.0 * c.comp.radius);
    }
    line(&mut s, "G90 G21 G17".into());
    // The first contour's speed (CTM8); every contour and marking with
    // another speed sets its own.
    let mut last_f = plan.contours.first().map_or(plan.timing.speeds.cut_for(1), |c| c.feed);
    line(&mut s, format!("F{last_f:.0}"));
    let feed_word = |last_f: &mut f64, f: f64| {
        if (f - *last_f).abs() < 0.5 {
            String::new()
        } else {
            *last_f = f;
            format!(" F{f:.0}")
        }
    };
    // Control compensation (KC4, KC17): the side word and corrector go on the
    // first lead-in move and stay on while the next contour needs the same; G40
    // goes on the rapid to whatever needs none (a contour without it, marking,
    // a free cut line, the end). The same word is never written twice in a row.
    let mut active: Option<(crate::kerf::Side, u32)> = None;
    let g0 = |active: &mut Option<(crate::kerf::Side, u32)>, p: P| {
        let off = if active.take().is_some() { "G40 " } else { "" };
        format!("G0 {off}X{:.2} Y{:.2}", p[0], p[1])
    };
    let mut last_part = 0;
    let mut marked: Vec<usize> = vec![];
    for c in &plan.contours {
        if c.manual {
            let _ = writeln!(s, "( MANUAL CUT )");
        }
        if c.part_no != last_part {
            let pl = &placed[c.part_no - 1];
            let _ = writeln!(s, "( PART {} : {}{} )", c.part_no, parts[pl.part].name, match (pl.angle(), pl.mirror) { (0, false) => String::new(), (a, m) => format!(" ROT{a}{}", if m { " MIRROR" } else { "" }) });
            last_part = c.part_no;
            let first_visit = !marked.contains(&c.part_no);
            marked.push(c.part_no);
            for (_, m) in plan.marks.iter().filter(|m| m.0 == c.part_no && first_visit) {
                line(&mut s, g0(&mut active, m[0]));
                line(&mut s, "M10 ( MARK ON )".into());
                for (k, q) in m.iter().skip(1).enumerate() {
                    let f = if k == 0 { feed_word(&mut last_f, plan.timing.speeds.mark) } else { String::new() };
                    line(&mut s, format!("G1 X{:.2} Y{:.2}{f}", q[0], q[1]));
                }
                line(&mut s, "M11 ( MARK OFF )".into());
            }
        }
        if let Some(p) = &c.punch {
            // One hit per standard shape, a row of hits when nibbling.
            let _ = writeln!(s, "( {} )", p.label());
            for h in &p.hits {
                line(&mut s, g0(&mut active, *h));
                line(&mut s, "M12 ( HIT )".into());
            }
            continue;
        }
        let want = c.comp.word();
        if want.is_some() {
            line(&mut s, format!("G0 X{:.2} Y{:.2}", c.lead_in[0], c.lead_in[1]));
        } else {
            line(&mut s, g0(&mut active, c.lead_in));
        }
        line(&mut s, format!("M07 ( PIERCE{} )", if c.inner { " INNER" } else { "" }));
        let mut comp_word = match want {
            Some((side, d)) if want != active => {
                active = want;
                format!("{} D{d} ", side.word())
            }
            _ => String::new(),
        };
        if c.lead_path.len() < 2 {
            // No lead-in: the contour's speed on a block of its own before it.
            let f = feed_word(&mut last_f, c.feed);
            if !f.is_empty() {
                line(&mut s, f.trim_start().to_string());
            }
        } else {
            // The lead-in's first move carries it.
            last_f = c.feed;
        }
        if c.lead_path.len() < 2 && !comp_word.is_empty() {
            // No lead-in: the word on a block of its own before the contour.
            line(&mut s, comp_word.trim_end().to_string());
            comp_word.clear();
        }
        // Lead-in path (line, line + arc, arc or from the hole centre).
        for (k, q) in c.lead_path.iter().enumerate().skip(1) {
            if k == 1 {
                line(&mut s, format!("G1 {comp_word}X{:.2} Y{:.2} F{:.0} ( LEAD-IN )", q[0], q[1], c.feed));
            } else {
                line(&mut s, format!("G1 X{:.2} Y{:.2}", q[0], q[1]));
            }
        }
        // A full circle in one move (post setting A024), when nothing interrupts it.
        let full_circle = if circles_one_move && c.joints.is_empty() && c.loops.is_empty() {
            match crate::machine::recognise(&c.points) {
                Some(crate::machine::StdShape::Circle { c: cc, .. }) => Some(cc),
                _ => None,
            }
        } else {
            None
        };
        if let Some(cc) = full_circle {
            let st = c.points[0];
            let g = if signed_area2(&c.points) >= 0.0 { "G3" } else { "G2" };
            line(&mut s, format!("{g} X{:.2} Y{:.2} I{:.2} J{:.2} ( FULL CIRCLE )", st[0], st[1], cc[0] - st[0], cc[1] - st[1]));
        } else {
        // Walk the contour; inside a micro-joint the beam is off and the head jumps across.
        let n = c.points.len();
        let mut events: Vec<f64> = vec![];
        let mut vertex_at: Vec<f64> = vec![0.0];
        let mut acc = 0.0;
        for k in 0..n {
            acc += dist(c.points[k], c.points[(k + 1) % n]);
            events.push(acc);
            vertex_at.push(acc);
        }
        for j in &c.joints {
            events.push(j.0);
            events.push(j.1);
        }
        // With system compensation the cut may stop short of its start (KC13).
        if c.overlap < 0.0 {
            let end = acc + c.overlap;
            events.retain(|&e| e < end - 1e-6);
            events.push(end);
        }
        events.sort_by(|a, b| a.partial_cmp(b).unwrap());
        events.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let mut prev = 0.0;
        let mut walk = Walker::new(&c.points);
        let mut beam_on = true;
        let mut done_joint: Option<usize> = None;
        for d in events {
            let p = walk.at(d);
            let mid = (prev + d) / 2.0;
            let joint_k = c.joints.iter().position(|j| mid > j.0 && mid < j.1);
            let in_joint = joint_k.is_some();
            // A joint with leads: lead out into the scrap, beam off, jump over,
            // pierce in the scrap and lead back in to the gap's end.
            if joint_k.is_some() && joint_k == done_joint {
                prev = d;
                continue;
            }
            if let Some((out, back)) = joint_k.and_then(|k| c.joint_leads.get(k)).filter(|_| beam_on) {
                done_joint = joint_k;
                // At the gap's start: the lead-out (if any).
                for q in out.iter().skip(1) {
                    line(&mut s, format!("G1 X{:.2} Y{:.2} ( JOINT LEAD-OUT )", q[0], q[1]));
                }
                // The closing joint: the contour ends here.
                if c.closing && joint_k == Some(c.joints.len() - 1) {
                    break;
                }
                line(&mut s, "M08 ( MICRO-JOINT )".into());
                line(&mut s, format!("G0 X{:.2} Y{:.2}", back[0][0], back[0][1]));
                line(&mut s, "M07 ( PIERCE JOINT )".into());
                line(&mut s, format!("G1 X{:.2} Y{:.2} ( JOINT LEAD-IN )", back[1][0], back[1][1]));
                prev = d;
                continue;
            }
            if in_joint && beam_on {
                line(&mut s, "M08 ( MICRO-JOINT )".into());
                beam_on = false;
            } else if !in_joint && !beam_on {
                line(&mut s, "M07".into());
                beam_on = true;
            }
            line(&mut s, format!("{} X{:.2} Y{:.2}", if in_joint { "G0" } else { "G1" }, p[0], p[1]));
            // A loop at this corner: cut it before carrying on along the next side.
            if beam_on {
                for (vi, pts) in &c.loops {
                    if (vertex_at[*vi] - d).abs() < 1e-6 {
                        let _ = writeln!(s, "( LOOP )");
                        for q in pts {
                            line(&mut s, format!("G1 X{:.2} Y{:.2}", q[0], q[1]));
                        }
                    }
                }
            }
            prev = d;
        }
        }
        if c.overlap > 0.0 {
            let q = point_at(&c.points, c.overlap);
            line(&mut s, format!("G1 X{:.2} Y{:.2} ( OVERLAP )", q[0], q[1]));
        }
        for (k, q) in c.lead_out_path.iter().enumerate().skip(1) {
            line(&mut s, format!("G1 X{:.2} Y{:.2}{}", q[0], q[1], if k == 1 { " ( LEAD-OUT )" } else { "" }));
        }
        line(&mut s, "M08".into());
    }
    for (label, pts) in lines {
        let Some(first) = pts.first() else { continue };
        let mark = label.ends_with("MARK");
        let _ = writeln!(s, "( {label} )");
        line(&mut s, g0(&mut active, *first));
        line(&mut s, if mark { "M10 ( MARK ON )".into() } else { "M07 ( PIERCE )".into() });
        // Its own speed, not the last contour's: marking, or the plain cut.
        let mut f = feed_word(&mut last_f, if mark { plan.timing.speeds.mark } else { plan.timing.speeds.cut_for(1) });
        for q in pts.iter().skip(1) {
            line(&mut s, format!("G1 X{:.2} Y{:.2}{f}", q[0], q[1]));
            f.clear();
        }
        line(&mut s, if mark { "M11 ( MARK OFF )".into() } else { "M08".into() });
    }
    line(&mut s, g0(&mut active, [0.0, 0.0]));
    line(&mut s, "M30".into());
    s
}

// ---------------------------------------------------------------------------
// Micro-joints (see MECHANICS.md J1–J8)
// ---------------------------------------------------------------------------

/// Distances along the contour where an element (a line, or a run of short
/// segments = an arc) starts: the vertices that turn more than
/// `nest::ELEMENT_CORNER_DEG`, or where a straight meets an arc, and the
/// contour's start (`leads::is_break`).
pub fn element_breaks(pts: &[P]) -> Vec<f64> {
    let n = pts.len();
    let mut out = vec![];
    let mut acc = 0.0;
    for i in 0..n {
        if crate::leads::is_break(pts, i) {
            out.push(acc);
        }
        acc += dist(pts[i], pts[(i + 1) % n]);
    }
    out
}

/// The element (start, end distance) holding distance `d` along the contour.
fn element_around(breaks: &[f64], per: f64, d: f64) -> (f64, f64) {
    if breaks.is_empty() {
        return (0.0, per); // a circle: one element all round
    }
    let mut s = *breaks.last().unwrap() - per;
    for &b in breaks {
        if b > d {
            return (s, b);
        }
        s = b;
    }
    (s, breaks[0] + per)
}

/// Fit a gap on one element (rule J4: both ends of the gap must lie on the same
/// element) that is longer than the gap (rule AM5: refused when `len ≤ S`; the
/// old "10 × the gap" was the nano-joint size, 10 × tolerance, J5). A gap running
/// past the element's end slides flush against it (AM5); `None` if the element is
/// too short.
pub fn fit_joint(breaks: &[f64], per: f64, a: f64, b: f64) -> Option<(f64, f64)> {
    let w = b - a;
    let c = (a + b) / 2.0;
    let (s, e) = element_around(breaks, per, c);
    if e - s <= w {
        return None;
    }
    let a2 = a.max(s).min(e - w);
    let a2 = if a2 < 0.0 { a2 + per } else { a2 };
    (a2 + w <= per + 1e-9).then_some((a2, a2 + w))
}

/// Joints on convex corners (the original's corner joint types), straddling
/// each corner (rule AM5; corrected: not placed just after it).
pub fn corner_joint_spans(pts: &[P], n: u32, w: f64) -> Vec<(f64, f64)> {
    let count = pts.len();
    let mut corners = vec![];
    let mut acc = 0.0;
    for i in 0..count {
        let (ang, cvx) = corner_at(pts, i);
        if cvx && ang < 180.0 - crate::nest::ELEMENT_CORNER_DEG {
            corners.push((acc, ang));
        }
        acc += dist(pts[i], pts[(i + 1) % count]);
    }
    if corners.is_empty() || n == 0 {
        return vec![];
    }
    // The start corner holds the lead-in, where a joint would be refused
    // (rule MJ2): use the other corners when there are enough.
    let corners: Vec<(f64, f64)> = if corners.len() > n as usize && corners[0].0 < 1e-9 { corners[1..].to_vec() } else { corners };
    let n = (n as usize).min(corners.len());
    (0..n).map(|k| corners[k * corners.len() / n]).map(|(d, ang)| crate::joints::corner_span(d, ang, w)).collect()
}

/// A joint's lead-out (from the gap's start out into the scrap) and lead-in
/// (from the scrap back to the gap's end), see `joints::lead_paths` (AM6):
/// D off the contour, leaning away from the gap by D / tan(angle) for the
/// inclined forms; the lead-out is just the gap start when D_out ≤ 2·tol.
#[allow(clippy::too_many_arguments)]
pub fn joint_lead_paths(pts: &[P], inner: bool, a: f64, b: f64, d_in: f64, d_out: f64, angle: f64, forms: (bool, bool), tol: f64) -> (Vec<P>, Vec<P>) {
    let pa = point_at(pts, a);
    let pb = point_at(pts, b);
    let ta = crate::leads::dir_at(pts, a, true);
    let tb = crate::leads::dir_at(pts, b, false);
    let na = crate::leads::scrap_normal(pts, inner, ta);
    let nb = crate::leads::scrap_normal(pts, inner, tb);
    crate::joints::lead_paths(pa, ta, na, pb, tb, nb, d_in, d_out, angle, forms, tol)
}

/// Edit a joint's lead by dragging its free end (rule J8): the lead keeps its foot at the gap; its angle from the contour is
/// kept between 0° and 90° (the original limits it to −90…90 and falls back to 0),
/// and its length is the distance to the pointer. Returns (length, angle).
pub fn edit_joint_lead(foot: P, back_tangent: P, scrap_normal: P, pointer: P) -> (f64, f64) {
    let v = [pointer[0] - foot[0], pointer[1] - foot[1]];
    let along = v[0] * back_tangent[0] + v[1] * back_tangent[1];
    let across = v[0] * scrap_normal[0] + v[1] * scrap_normal[1];
    let len = (along * along + across * across).sqrt();
    let mut ang = across.atan2(along).to_degrees();
    if !(-90.0..=90.0).contains(&ang) {
        ang = 0.0;
    }
    (len, ang.clamp(0.0, 90.0))
}

#[cfg(test)]
mod joint_tests {
    use super::*;
    use crate::nest::{JointType, Part, PartTech, Placed, Shape, SheetSpec};

    fn plan_for(tech: PartTech) -> SheetPlan {
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: Some(tech), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let pl = Placed { part: 0, x: 50.0, y: 50.0, w: 200.0, h: 100.0, turns: 0, mirror: false };
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        plan(&parts, &[pl], &spec)
    }

    #[test]
    fn fallback_speed_follows_any_thickness() {
        // TH1: a typed thickness between two in the list is read between them.
        assert_eq!(cut_speed(0, 3.0), 5000.0);
        assert_eq!(cut_speed(0, 4.0), 4000.0);
        assert_eq!(cut_speed(0, 0.5), 20000.0, "thinner than the list: its first value");
        assert_eq!(cut_speed(0, 40.0), 800.0, "thicker: its last");
        let quarter = cut_speed(0, 6.35);
        assert!(quarter < 3000.0 && quarter > 1800.0, "{quarter}");
    }

    #[test]
    fn joint_types_follow_the_original_numbering() {
        assert_eq!([1, 2, 7, 8, 3, 6, 9, 11, 12, -1].map(JointType::category), [1, 1, 2, 2, 4, 4, 4, 4, 0, 0]);
        assert!(JointType::in_corner(JointType::Corner.code()) && !JointType::in_corner(JointType::Leads.code()));
        assert_eq!(JointType::category(JointType::Gap.code()), 4);
        let d = PartTech::default();
        assert_eq!((d.joint_type, d.joint_lead, d.joint_angle, d.joint_width), (JointType::Gap, 10.0, 45.0, 0.5));
    }

    #[test]
    fn a_gap_slides_off_a_corner_and_needs_a_long_enough_element() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let breaks = element_breaks(&sq);
        assert_eq!(breaks, vec![0.0, 100.0, 150.0, 250.0]);
        // Straddling the corner at 100: moved wholly onto the edge holding its middle.
        assert_eq!(fit_joint(&breaks, 300.0, 98.5, 100.5), Some((98.0, 100.0)));
        assert_eq!(fit_joint(&breaks, 300.0, 99.0, 101.0), Some((100.0, 102.0)));
        // Corrected (J5): the element only has to be longer
        // than the gap — a 6 mm gap fits the 50 mm side; a 60 mm one doesn't.
        assert_eq!(fit_joint(&breaks, 300.0, 120.0, 126.0), Some((120.0, 126.0)));
        assert_eq!(fit_joint(&breaks, 300.0, 100.0, 160.0), None);
        assert!(fit_joint(&breaks, 300.0, 40.0, 46.0).is_some());
    }

    #[test]
    fn joints_with_leads_cut_out_into_the_scrap_and_re_pierce() {
        let t = PartTech { micro_joints: 4, joint_type: JointType::Leads, ..PartTech::default() };
        let p = plan_for(t);
        let c = &p.contours[0];
        assert_eq!(c.joints.len(), 4);
        assert_eq!(c.joint_leads.len(), 4);
        // The contour starts at its first joint's lead-in and ends at that
        // joint's lead-out (MJ5): 4 pierces, not 1 + 4.
        assert!(c.closing);
        assert_eq!(p.pierces, 4);
        assert_eq!(c.lead_in, c.joint_leads[3].1[0]);
        assert!((c.joints[3].1 - perimeter(&c.points)).abs() < 1e-6, "the closing joint ends the contour");
        let body = [[50.0, 50.0], [250.0, 50.0], [250.0, 150.0], [50.0, 150.0]];
        for (out, back) in &c.joint_leads {
            assert!(!crate::nest::point_in_poly(&body, out[1]), "lead-out ends in the scrap");
            assert!(!crate::nest::point_in_poly(&body, back[0]), "lead-in starts in the scrap");
            // Kept inside the 5 mm part gap.
            let reach = |q: P| (q[0] - 50.0).min(250.0 - q[0]).min(q[1] - 50.0).min(150.0 - q[1]).abs();
            assert!(reach(out[1]) <= 5.0 * crate::nest::JOINT_LEAD_REACH + 1e-6);
        }
        let plain = plan_for(PartTech { micro_joints: 4, ..PartTech::default() });
        assert!(p.cut_len > plain.cut_len);
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: Some(t), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let pl = Placed { part: 0, x: 50.0, y: 50.0, w: 200.0, h: 100.0, turns: 0, mirror: false };
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let prog = program(1, &parts, &[pl], &spec, &p);
        assert_eq!(prog.matches("JOINT LEAD-OUT").count(), 4);
        assert_eq!(prog.matches("PIERCE JOINT").count(), 3);
        assert!(!prog.contains("( LEAD-OUT )"), "the closing joint's lead-out ends the cut");
    }

    #[test]
    fn overlap_cuts_past_the_start_and_joints_space_by_distance() {
        let base = plan_for(PartTech { lead_out: 0.0, ..PartTech::default() });
        let ov = plan_for(PartTech { lead_out: 0.0, overlap: 0.5, ..PartTech::default() });
        assert!((ov.cut_len - base.cut_len - 0.5).abs() < 1e-6);
        assert_eq!(ov.contours[0].overlap, 0.5);
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: Some(PartTech { overlap: 0.5, ..PartTech::default() }), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let pl = Placed { part: 0, x: 50.0, y: 50.0, w: 200.0, h: 100.0, turns: 0, mirror: false };
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        assert!(program(1, &parts, &[pl], &spec, &plan(&parts, &[pl], &spec)).contains("( OVERLAP )"));
        // 600 mm round at one per 100 mm (CT1 F150): 6 joints.
        let every = plan_for(PartTech { joint_every: 100.0, micro_joints: 0, ..PartTech::default() });
        assert_eq!(every.contours[0].joints.len(), 6);
    }

    #[test]
    fn joints_near_the_lead_or_each_other_are_refused() {
        use crate::nest::joint_too_near;
        // MJ2: refused when distance <= own width + other's size.
        assert!(joint_too_near([0.0, 2.5], 0.5, &[([0.0, 0.0], 2.0)]));
        assert!(!joint_too_near([0.0, 2.6], 0.5, &[([0.0, 0.0], 2.0)]));
        // Two joints picked on the same spot of the long side: only one is kept.
        let mut t = PartTech::default();
        t.joint_picks[0] = Some((0, [100.0, 0.0]));
        t.joint_picks[1] = Some((0, [100.6, 0.0]));
        assert_eq!(plan_for(t).contours[0].joints.len(), 1);
        t.joint_picks[1] = Some((0, [140.0, 0.0]));
        assert_eq!(plan_for(t).contours[0].joints.len(), 2);
    }

    #[test]
    fn small_holes_take_their_own_quality() {
        use crate::nest::{equivalent_radius, quality_for_radius, CutQuality, CutRange};
        // A circle of radius 4 has equivalent radius 4.
        assert!((equivalent_radius(std::f64::consts::PI * 16.0) - 4.0).abs() < 1e-9);
        let q = |n| CutQuality::new(n).unwrap();
        let rows = [CutRange { on: true, min_r: 0.0, max_r: 5.0, quality: q(1) }, CutRange { on: true, min_r: 5.0, max_r: 50.0, quality: q(3) }, CutRange::default()];
        assert_eq!(quality_for_radius(&rows, 4.0, 0.0), Some(q(1)));
        assert_eq!(quality_for_radius(&rows, 5.0, 0.0), Some(q(3)), "upper limit not included");
        assert_eq!(quality_for_radius(&rows, 60.0, 0.0), None);
        let late = [CutRange { on: true, min_r: 10.0, max_r: 20.0, quality: q(1) }, CutRange::default(), CutRange::default()];
        assert_eq!(quality_for_radius(&late, 4.0, 0.0), None, "first row starts above the radius");
        // In the plan: the part's outside (r ≈ 79.8) keeps quality 3, a quality-1 row slows a matching contour.
        let t = PartTech { cut_ranges: [CutRange { on: true, min_r: 0.0, max_r: 100.0, quality: q(1) }, CutRange::default(), CutRange::default()], ..PartTech::default() };
        let (slow, normal) = (plan_for(t), plan_for(PartTech::default()));
        assert!(slow.contours[0].feed < normal.contours[0].feed);
    }

    #[test]
    fn corner_joints_straddle_corners() {
        // Corrected (AM5): a corner joint straddles the corner.
        let p = plan_for(PartTech { micro_joints: 2, joint_type: JointType::Corner, ..PartTech::default() });
        let c = &p.contours[0];
        assert_eq!(c.joints.len(), 2);
        let breaks = element_breaks(&c.points);
        for j in &c.joints {
            let mid = (j.0 + j.1) / 2.0;
            assert!(breaks.iter().any(|b| (b - mid).abs() < 1e-6), "joint {j:?} centred on a corner {breaks:?}");
            assert!((j.1 - j.0 - 0.5 * 2.0f64.sqrt()).abs() < 1e-6, "S / sin 45° across a 90° corner");
        }
    }

    #[test]
    fn automatic_joints_are_centred_on_the_sides() {
        // 200 × 100 from its corner, one per 100 mm: 2 on each long side, 1 in
        // the middle of each short side (AM4).
        let p = plan_for(PartTech { joint_every: 100.0, ..PartTech::default() });
        let c = &p.contours[0];
        let mids: Vec<f64> = c.joints.iter().map(|j| (j.0 + j.1) / 2.0).collect();
        let want = [50.0, 150.0, 250.0, 350.0, 450.0, 550.0];
        assert!(mids.len() == 6 && mids.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{mids:?}");
    }

    #[test]
    fn holes_get_automatic_joints_too() {
        // Plate 200 × 200 with a Ø80 hole (251 mm round): ceil 2.5 = 3 → 4 joints;
        // the outside (800 mm): 8. A fixed count stays on the outside only.
        let parts = vec![Part { name: "H".into(), shape: Shape::PlateHole, w: 200.0, h: 200.0, qty: 1, rotate: false, color: 0, tech: Some(PartTech { joint_every: 100.0, ..PartTech::default() }), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let pl = Placed { part: 0, x: 50.0, y: 50.0, w: 200.0, h: 200.0, turns: 0, mirror: false };
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let p = plan(&parts, &[pl], &spec);
        let hole = p.contours.iter().find(|c| c.inner).unwrap();
        let outer = p.contours.iter().find(|c| !c.inner).unwrap();
        assert_eq!((hole.joints.len(), outer.joints.len()), (4, 8));
        let mut fixed = parts.clone();
        fixed[0].tech = Some(PartTech { micro_joints: 4, ..PartTech::default() });
        let p = plan(&fixed, &[pl], &spec);
        assert!(p.contours.iter().find(|c| c.inner).unwrap().joints.is_empty());
    }

    #[test]
    fn machine_leads_reach_the_program_and_keep_clear_of_neighbours() {
        // Parts without technology get the machine's tables: a rectangle's lead is a
        // 7 mm corner lead extending its first side (laser template B209/B232).
        let r = |x: f64| Placed { part: 0, x, y: 20.0, w: 100.0, h: 50.0, turns: 0, mirror: false };
        let parts = vec![Part { name: "R".into(), shape: Shape::Rectangle, w: 100.0, h: 50.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 3.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let alone = plan(&parts, &[r(200.0)], &spec);
        let c = &alone.contours[0];
        assert_eq!(c.points[0], [200.0, 20.0]);
        assert!((c.lead_in[0] - 193.0).abs() < 1e-6 && (c.lead_in[1] - 20.0).abs() < 1e-6, "{:?}", c.lead_in);
        let prog = program(1, &parts, &[r(200.0)], &spec, &alone);
        assert!(prog.contains("( LEAD-IN )") && prog.contains("( LEAD-OUT )"));
        // A neighbour 3 mm to the left: the 7 mm lead would cut into it, so it is
        // moved along the contour (B185 = 3) and stays clear.
        let both = [Placed { y: 10.0, ..r(97.0) }, r(200.0)];
        let p = plan(&parts, &both, &spec);
        let left: Vec<P> = vec![[97.0, 10.0], [197.0, 10.0], [197.0, 60.0], [97.0, 60.0]];
        assert!(crate::leads::path_hits(&c.lead_path, c.points[0], &[&left], 0.05), "the unmoved lead would hit it");
        for c in p.contours.iter().filter(|c| c.part_no == 2) {
            assert!(!crate::leads::path_hits(&c.lead_path, c.points[0], &[&left], 0.05), "{:?}", c.lead_path);
            assert!(!crate::nest::point_in_poly(&left, c.lead_in));
        }
    }

    #[test]
    fn dragging_a_joint_lead_sets_its_length_and_angle() {
        let foot = [0.0, 0.0];
        let (l, a) = edit_joint_lead(foot, [-1.0, 0.0], [0.0, 1.0], [-3.0, 3.0]);
        assert!((l - 18f64.sqrt()).abs() < 1e-9 && (a - 45.0).abs() < 1e-9);
        let (_, a) = edit_joint_lead(foot, [-1.0, 0.0], [0.0, 1.0], [0.0, 4.0]);
        assert!((a - 90.0).abs() < 1e-9);
        // Pointing forward over the gap: outside −90…90, the original falls back to 0.
        let (_, a) = edit_joint_lead(foot, [-1.0, 0.0], [0.0, 1.0], [3.0, -1.0]);
        assert_eq!(a, 0.0);
    }
}

#[cfg(test)]
mod manual_tests {
    use super::*;
    use crate::nest::{Part, Placed, Shape, SheetSpec};

    fn setup() -> (Vec<Part>, Vec<Placed>, SheetSpec) {
        let p = |shape, w, h| Part { name: "P".into(), shape, w, h, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let parts = vec![p(Shape::PlateHole, 200.0, 200.0), p(Shape::Rectangle, 100.0, 50.0)];
        let placed = vec![
            Placed { part: 0, x: 10.0, y: 10.0, w: 200.0, h: 200.0, turns: 0, mirror: false },
            Placed { part: 1, x: 300.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false },
        ];
        (parts, placed, SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() })
    }

    #[test]
    fn picking_finds_the_nearest_contour() {
        let (parts, placed, _) = setup();
        let (i, ci, q) = nearest_contour(&parts, &placed, [10.5, 100.0], 2.0).unwrap();
        assert_eq!((i, ci), (0, 1), "the plate's outside");
        assert!((q[0] - 10.0).abs() < 1e-6);
        let (i, ci, _) = nearest_contour(&parts, &placed, [400.0, 30.0], 2.0).unwrap();
        assert_eq!((i, ci), (1, 0));
        assert!(nearest_contour(&parts, &placed, [700.0, 300.0], 2.0).is_none());
    }

    #[test]
    fn manual_picks_go_first_holes_before_the_outside_and_once_only() {
        let (parts, placed, _) = setup();
        let outer = ManualCut { placed: 0, contour: 1, start: [10.0, 100.0], reverse: false };
        let small = ManualCut { placed: 1, contour: 0, start: [400.0, 30.0], reverse: true };
        let jobs = cut_jobs(&parts, &placed, &[0, 1], &[small, outer, outer]);
        let order: Vec<(usize, usize, bool)> = jobs.iter().map(|j| (j.0, j.1, j.2.is_some())).collect();
        // The small part, then the plate's hole (automatic, inserted), then its outside.
        assert_eq!(order, vec![(1, 0, true), (0, 0, false), (0, 1, true)]);
    }

    /// A 600 plate with a Ø240 hole, a 50 square in the hole and one beside it.
    fn frame() -> (Vec<Part>, Vec<Placed>, SheetSpec) {
        let (mut parts, _, spec) = setup();
        parts[0].w = 600.0;
        parts[0].h = 600.0;
        parts[1].w = 50.0;
        parts[1].h = 50.0;
        let at = |part, x, y, w, h| Placed { part, x, y, w, h, turns: 0, mirror: false };
        (parts, vec![at(0, 10.0, 10.0, 600.0, 600.0), at(1, 285.0, 285.0, 50.0, 50.0), at(1, 700.0, 10.0, 50.0, 50.0)], spec)
    }

    #[test]
    fn a_picked_frame_cuts_the_part_in_its_hole_first() {
        // CO2: picking the plate's outside or its hole cuts the part sitting
        // in the hole first, and a pick on that part keeps its start.
        let (parts, placed, _) = frame();
        let outer = ManualCut { placed: 0, contour: 1, start: [10.0, 300.0], reverse: false };
        let hole = ManualCut { placed: 0, contour: 0, start: [430.0, 310.0], reverse: false };
        let inner = ManualCut { placed: 1, contour: 0, start: [285.0, 300.0], reverse: true };
        let order = |m: &[ManualCut]| cut_jobs(&parts, &placed, &[1, 0, 2], m).iter().map(|j| (j.0, j.1, j.2.is_some())).collect::<Vec<_>>();
        assert_eq!(order(&[outer, inner]), vec![(1, 0, true), (0, 0, false), (0, 1, true), (2, 0, false)]);
        assert_eq!(order(&[hole]), vec![(1, 0, false), (0, 0, true), (0, 1, false), (2, 0, false)]);
    }

    #[test]
    fn verify_flags_a_part_cut_after_the_hole_it_sits_in() {
        let (parts, placed, spec) = frame();
        let late = |t: &String| t.contains("sitting in it (2)") && t.contains("drop first");
        let pick = ManualCut { placed: 0, contour: 1, start: [10.0, 300.0], reverse: false };
        let mut p = plan_manual(&parts, &placed, &spec, &[pick]);
        assert!(!crate::drawing::verify_machining(&p).iter().any(late));
        // The part in the hole moved to the end, after the plate's hole.
        let k = p.contours.iter().position(|c| c.part_no == 2).unwrap();
        let c = p.contours.remove(k);
        p.contours.push(c);
        let probs = crate::drawing::verify_machining(&p);
        assert!(probs.iter().any(late), "{probs:?}");
    }

    #[test]
    fn the_plan_starts_where_picked_and_can_cut_the_other_way() {
        let (parts, placed, spec) = setup();
        let auto = plan(&parts, &placed, &spec);
        let pick = ManualCut { placed: 1, contour: 0, start: [350.0, 10.0], reverse: false };
        let p = plan_manual(&parts, &placed, &spec, &[pick]);
        let c = &p.contours[0];
        assert!(c.manual && c.part_no == 2);
        assert!((c.points[0][0] - 350.0).abs() < 1e-6 && (c.points[0][1] - 10.0).abs() < 1e-6);
        assert_eq!(p.contours.len(), auto.contours.len());
        let rev = plan_manual(&parts, &placed, &spec, &[ManualCut { reverse: true, ..pick }]);
        let a = signed_area2(&c.points);
        let b = signed_area2(&rev.contours[0].points);
        assert!(a * b < 0.0, "reversed direction");
        let prog = program(1, &parts, &placed, &spec, &p);
        assert_eq!(prog.matches("MANUAL CUT").count(), 1);
    }
}

#[cfg(test)]
mod kerf_plan_tests {
    //! Kerf compensation through the cut plan and the programs (KC1–KC15).
    use super::*;
    use crate::kerf::Side;
    use crate::nest::{Part, Placed, Shape, SheetSpec};

    fn part(shape: Shape, w: f64, h: f64, custom: Option<crate::dxf::CustomGeo>) -> Part {
        Part { name: "P".into(), shape, w, h, qty: 1, rotate: false, color: 0, tech: None, custom: custom.map(std::sync::Arc::new), rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }
    fn at(x: f64, y: f64, w: f64, h: f64, mirror: bool) -> Placed {
        Placed { part: 0, x, y, w, h, turns: 0, mirror }
    }
    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() }
    }
    /// A drawing part from outline and holes as drawn (direction kept).
    fn drawn(outline: Vec<P>, holes: Vec<Vec<P>>) -> Part {
        let g = crate::dxf::CustomGeo { w: 100.0, h: 60.0, outline, holes, fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], contours: vec![], hand_order: vec![] };
        part(Shape::Custom, 100.0, 60.0, Some(g))
    }
    fn sides(p: &SheetPlan) -> Vec<(bool, Side)> {
        p.contours.iter().map(|c| (c.inner, c.comp.side)).collect()
    }
    fn machine(text: &str) {
        crate::ct1::set_thread_active(Some(crate::ct1::MachineTech::from_text(text)));
    }

    #[test]
    fn a_mirrored_part_is_compensated_on_the_other_side() {
        crate::ct1::set_thread_active(None);
        let parts = vec![part(Shape::PlateHole, 100.0, 60.0, None)];
        let plain = plan(&parts, &[at(10.0, 10.0, 100.0, 60.0, false)], &spec());
        let mirrored = plan(&parts, &[at(10.0, 10.0, 100.0, 60.0, true)], &spec());
        // Built-in shapes run counter-clockwise: hole left (G41), outside right (G42).
        assert_eq!(sides(&plain), vec![(true, Side::Left), (false, Side::Right)]);
        // Mirrored they run clockwise: the scrap is on the other side.
        assert_eq!(sides(&mirrored), vec![(true, Side::Right), (false, Side::Left)]);
        let prog = program(1, &parts, &[at(10.0, 10.0, 100.0, 60.0, true)], &spec(), &mirrored);
        assert!(prog.contains("G42 D1") && prog.contains("G41 D2"), "{prog}");
        // The BLM post: the same sides, no corrector number.
        let sub = crate::blm::subprogram("P", &mirrored, 1, [10.0, 10.0]);
        let words: Vec<&str> = sub.lines().filter_map(|l| l.split_whitespace().nth(1).filter(|w| w.starts_with("G4"))).collect();
        assert_eq!(words, vec!["G42", "G41"], "{sub}");
        let sub = crate::blm::subprogram("P", &plain, 1, [10.0, 10.0]);
        let words: Vec<&str> = sub.lines().filter_map(|l| l.split_whitespace().nth(1).filter(|w| w.starts_with("G4"))).collect();
        assert_eq!(words, vec!["G41", "G42"], "{sub}");
    }

    #[test]
    fn a_clockwise_drawing_is_compensated_on_its_scrap_side() {
        crate::ct1::set_thread_active(None);
        // Outline drawn clockwise, hole drawn clockwise.
        let outline = vec![[0.0, 0.0], [0.0, 60.0], [100.0, 60.0], [100.0, 0.0]];
        let hole = vec![[40.0, 20.0], [40.0, 40.0], [60.0, 40.0], [60.0, 20.0]];
        let parts = vec![drawn(outline, vec![hole])];
        let p = plan(&parts, &[at(10.0, 10.0, 100.0, 60.0, false)], &spec());
        assert_eq!(sides(&p), vec![(true, Side::Right), (false, Side::Left)]);
        let prog = program(1, &parts, &[at(10.0, 10.0, 100.0, 60.0, false)], &spec(), &p);
        assert!(prog.contains("G42 D1") && prog.contains("G41 D2"), "{prog}");
    }

    #[test]
    fn a_contour_reversed_by_hand_flips_its_side() {
        crate::ct1::set_thread_active(None);
        let parts = vec![part(Shape::Rectangle, 100.0, 60.0, None)];
        let pl = [at(10.0, 10.0, 100.0, 60.0, false)];
        let fwd = plan_manual(&parts, &pl, &spec(), &[ManualCut { placed: 0, contour: 0, start: [60.0, 10.0], reverse: false }]);
        let rev = plan_manual(&parts, &pl, &spec(), &[ManualCut { placed: 0, contour: 0, start: [60.0, 10.0], reverse: true }]);
        assert_eq!((fwd.contours[0].comp.side, rev.contours[0].comp.side), (Side::Right, Side::Left));
    }

    #[test]
    fn control_words_are_written_once_per_change_and_cancelled_at_the_end() {
        crate::ct1::set_thread_active(None);
        let parts = vec![part(Shape::PlateHole, 100.0, 60.0, None)];
        let pl = [at(10.0, 10.0, 100.0, 60.0, false), Placed { part: 0, ..at(150.0, 10.0, 100.0, 60.0, false) }];
        let p = plan(&parts, &pl, &spec());
        let prog = program(1, &parts, &pl, &spec(), &p);
        // hole, outside, hole, outside: the word changes every contour.
        assert_eq!(prog.matches("G41 D1").count(), 2, "{prog}");
        assert_eq!(prog.matches("G42 D2").count(), 2);
        assert_eq!(prog.matches("G40").count(), 1, "one cancel, on the way home: {prog}");
        assert!(prog.contains("G0 G40 X0.00 Y0.00"), "{prog}");
        // The word rides on the first lead-in move.
        assert!(prog.lines().any(|l| l.contains("G1 G41 D1 X") && l.contains("LEAD-IN")), "{prog}");
        // Two plain rectangles: one word for both.
        let parts = vec![part(Shape::Rectangle, 100.0, 60.0, None)];
        let p = plan(&parts, &pl, &spec());
        let prog = program(1, &parts, &pl, &spec(), &p);
        assert_eq!(prog.matches("G42 D2").count(), 1, "{prog}");
        assert!(prog.contains("( KERF 0.30 MM - CONTROL G41/G42 )"));
    }

    #[test]
    fn a_material_section_sets_its_own_correctors_and_cutter_radius() {
        machine("B002 0.12\nB124 1\nB125 2\nZ000 \"*\"\nZ001 -1\nZ002 99\nZ004 0.2\nZ024 5\nZ025 6\nZ999 0\n");
        let parts = vec![part(Shape::PlateHole, 100.0, 60.0, None)];
        let p = plan(&parts, &[at(10.0, 10.0, 100.0, 60.0, false)], &spec());
        let c: Vec<(u32, f64)> = p.contours.iter().map(|c| (c.comp.corrector, c.comp.radius)).collect();
        assert_eq!(c, vec![(5, 0.2), (6, 0.2)]);
        assert!((crate::kerf::kerf_mm(&spec()) - 0.4).abs() < 1e-12);
        crate::ct1::set_thread_active(None);
    }

    #[test]
    fn system_compensation_cuts_the_offset_path_and_moves_the_pierce() {
        machine("B116 ON\nB117 OFF\nB002 0.5\nA017 2\n");
        let parts = vec![part(Shape::Rectangle, 100.0, 60.0, None)];
        let pl = [at(10.0, 10.0, 100.0, 60.0, false)];
        let p = plan(&parts, &pl, &spec());
        let c = &p.contours[0];
        assert!(c.comp.system && !c.comp.control);
        // The beam's centre runs 0.5 mm outside the part, sharp corners.
        let (lo, hi) = c.points.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
        assert!((lo[0] - 9.5).abs() < 1e-9 && (lo[1] - 9.5).abs() < 1e-9 && (hi[0] - 110.5).abs() < 1e-9 && (hi[1] - 70.5).abs() < 1e-9, "{lo:?} {hi:?}");
        // The pierce moved off the drawn lead with it, out in the scrap.
        assert_eq!(c.lead_in, c.lead_path[0]);
        assert!(!crate::nest::point_in_poly(&c.points, c.lead_in));
        // Time and length follow the longer path; the program has no G41/G42.
        let nominal = { crate::ct1::set_thread_active(None); plan(&parts, &pl, &spec()) };
        assert!(p.cut_len > nominal.cut_len + 2.0, "{} vs {}", p.cut_len, nominal.cut_len);
        machine("B116 ON\nB117 OFF\nB002 0.5\nA017 2\n");
        let prog = program(1, &parts, &pl, &spec(), &p);
        assert!(!prog.contains("G41") && !prog.contains("G42") && !prog.contains("G40"), "{prog}");
        assert!(prog.contains("( KERF 1.00 MM - OFFSET PATH )"));
        assert!(prog.contains("X9.50 Y9.50") || prog.contains("X110.50 Y9.50"), "{prog}");
        // The job can switch it off again: the drawn path, control words back.
        let sp = SheetSpec { kerf: crate::kerf::KerfChoice { control: Some(true), system: Some(false) }, ..spec() };
        let p2 = plan(&parts, &pl, &sp);
        assert_eq!(p2.contours[0].comp.word(), Some((Side::Right, 2)));
        assert!(p2.contours[0].points.iter().all(|q| q[0] >= 10.0 - 1e-9 && q[1] >= 10.0 - 1e-9));
        crate::ct1::set_thread_active(None);
    }

    #[test]
    fn a_hole_no_bigger_than_the_kerf_is_reported_and_not_cut() {
        machine("B116 ON\nB117 OFF\nB002 0.15\n");
        let tiny: Vec<P> = (0..24).map(|k| {
            let t = std::f64::consts::TAU * k as f64 / 24.0;
            [50.0 + 0.1 * t.cos(), 30.0 + 0.1 * t.sin()]
        }).collect();
        let outline = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]];
        let parts = vec![drawn(outline, vec![tiny])];
        let p = plan(&parts, &[at(10.0, 10.0, 100.0, 60.0, false)], &spec());
        assert_eq!(p.contours.len(), 1, "only the outside is cut");
        assert_eq!(p.pierces, 1);
        assert_eq!(p.kerf_issues.len(), 1);
        assert!(p.kerf_issues[0].1 && p.kerf_issues[0].2.contains("not cut"), "{:?}", p.kerf_issues);
        crate::ct1::set_thread_active(None);
    }

    /// A 200 × 100 plate with a 6 mm hole, as one part without technology of its own.
    fn plate_with_hole(material: usize, thickness: f64) -> SheetPlan {
        let (parts, placed, spec) = plate_job(material, thickness);
        plan(&parts, &placed, &spec)
    }

    fn plate_job(material: usize, thickness: f64) -> (Vec<Part>, Vec<Placed>, SheetSpec) {
        let mut part = Part { name: "P".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let hole: Vec<P> = (0..48).map(|k| {
            let a = k as f64 * std::f64::consts::TAU / 48.0;
            [100.0 + 3.0 * a.cos(), 50.0 + 3.0 * a.sin()]
        }).collect();
        part.custom = Some(std::sync::Arc::new(crate::dxf::CustomGeo { outline: vec![[0.0, 0.0], [200.0, 0.0], [200.0, 100.0], [0.0, 100.0]], holes: vec![hole], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: 200.0, h: 100.0, contours: vec![], hand_order: vec![] }));
        part.shape = Shape::Custom;
        let pl = Placed { part: 0, x: 50.0, y: 50.0, w: 200.0, h: 100.0, turns: 0, mirror: false };
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material, thickness, level: 3, machine: Default::default(), kerf: Default::default() };
        (vec![part], vec![pl], spec)
    }

    #[test]
    fn times_come_from_the_machines_table_and_time_file() {
        // Stainless 3 mm, nitrogen (gas 1); the section's size bands send the
        // hole (Ø6, r 3) to quality 2 and the outside to quality 3 (MTL10, TT8).
        let mut mt = crate::ct1::MachineTech::from_text("A051 40000\nA058 3000\nZ000 \"##MATEXP0100##\"\nZ001 2.7\nZ002 3.2\nZ010 0\nZ011 2\nZ012 0\nZ010 2\nZ011 5\nZ012 1\nZ010 5\nZ011 99999\nZ012 2\nZ999 0\n");
        let time = crate::cuttime::TimeSettings::from_text("D014 3\nD017 2\nD018 2\nD168 3\nH006 0.7\nP001 0 999999 10000 10000 10000 10000 114000 140000\nO001 0 999999 10000 10000 10000 10000 114000 140000\n");
        mt.speed_files = std::sync::Arc::new(crate::techtable::SpeedFiles { time, table: Some(crate::techtable::Table::from_text(crate::techtable::demo_table())), fields: Default::default(), gas: 1 });
        crate::ct1::set_thread_active(Some(mt));
        let p = plate_with_hole(1, 3.0);
        let feeds: Vec<(bool, usize, f64)> = p.contours.iter().map(|c| (c.inner, c.quality, c.feed)).collect();
        assert_eq!(feeds, vec![(true, 2, 5000.0), (false, 3, 9500.0)]);
        // Pierce times by quality (TT5): 0.2 s for the hole, 0.3 s + pierce type 1 (5 s) outside.
        assert!((p.times.pierce - 5.5).abs() < 1e-9, "{:?}", p.times);
        // Head up after each contour (H006).
        assert!((p.times.head - 1.4).abs() < 1e-9, "{:?}", p.times);
        // The outside's cut takes its length at 9500 mm/min plus the corner stops.
        let out = &p.contours[1];
        let len = contour_cut_len(out);
        assert!(out.time.cut > len / 9500.0 * 60.0 && out.time.cut < len / 9500.0 * 60.0 + 0.5, "{} vs {}", out.time.cut, len / 9500.0 * 60.0);
        assert!((p.seconds - p.times.total()).abs() < 1e-9);
        assert!(matches!(p.timing.speeds.source, crate::techtable::SpeedSource::Table(..)));
        // Every contour is cut at its own speed, with or without a lead-in.
        let (parts, placed, spec) = plate_job(1, 3.0);
        let mut bare = plan(&parts, &placed, &spec);
        for c in &mut bare.contours {
            c.lead_path.clear();
            c.lead_in = c.points[0];
        }
        let prog = program_mm(1, &parts, &placed, &spec, &bare, &[], false);
        let feeds: Vec<&str> = prog.lines().filter_map(|l| l.split_whitespace().find(|w| w.starts_with('F') && w[1..].parse::<f64>().is_ok())).collect();
        assert_eq!(feeds, vec!["F5000", "F9500"], "{prog}");
        // A part with size ranges of its own keeps its quality where none of
        // its ranges fits: the machine's bands don't apply to it (MTL10).
        let (mut parts, placed, spec) = plate_job(1, 3.0);
        let mut ranges: [crate::nest::CutRange; 3] = Default::default();
        ranges[0] = crate::nest::CutRange { on: true, min_r: 0.0, max_r: 5.0, quality: CutQuality::new(1).unwrap() };
        parts[0].tech = Some(PartTech { cut_ranges: ranges, ..PartTech::default() });
        let own = plan(&parts, &placed, &spec);
        let q: Vec<usize> = own.contours.iter().map(|c| c.quality).collect();
        assert_eq!(q, vec![1, CutQuality::FALLBACK.number()]);
        // No table record for 5 mm: the machine's default speed for every contour.
        let p5 = plate_with_hole(1, 5.0);
        assert!(p5.contours.iter().all(|c| c.feed == 3000.0));
        assert_eq!(p5.timing.speeds.source, crate::techtable::SpeedSource::Machine);
        crate::ct1::set_thread_active(None);
    }

    #[test]
    fn the_forward_walk_finds_the_same_points() {
        // A contour with a plain joint and a joint with leads: the beam runs
        // match a walk that measures every point from the start.
        let mut p = plate_with_hole(0, 3.0);
        let mut c = p.contours.remove(1);
        c.joints = vec![(150.0, 151.0), (400.0, 401.0)];
        c.joint_leads = vec![];
        let runs = beam_runs(&c);
        assert_eq!(runs.len(), 3);
        assert!(dist(runs[1][0], point_at(&c.points, 151.0)) < 1e-9 && dist(runs[2][0], point_at(&c.points, 401.0)) < 1e-9);
        assert!(dist(*runs[0].last().unwrap(), point_at(&c.points, 150.0)) < 1e-9);
    }

    #[test]
    fn the_simulation_lasts_as_long_as_the_sheet() {
        let p = plate_with_hole(0, 3.0);
        let total: f64 = crate::sheetsim::ops(&p, &["P".into()]).iter().map(|o| o.secs).sum();
        assert!((total - p.seconds).abs() < 1e-6, "{total} vs {}", p.seconds);
    }

    #[test]
    fn the_installers_template_machine_gives_its_table_speeds() {
        // The template laser of the original's installer (PSTadi02), when this
        // checkout has it: its CF file names the table and the time file.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendor/installer/V44_SP01_METAL/Expert/Exe/DBIMPORT/D200004/FilesCfg");
        let (ct1, cf) = (dir.join("PSTadi02.CT1"), dir.join("PSTadi02.CF1"));
        let Some(mut mt) = crate::ct1::MachineTech::load(&ct1) else { return };
        mt.speed_files = std::sync::Arc::new(crate::techtable::SpeedFiles::find(Some(&ct1), Some(&cf), 1));
        assert!(mt.speed_files.table.as_ref().is_some_and(|t| t.records.len() > 50));
        assert_eq!(mt.speed_files.time.pierce_mode, crate::cuttime::PierceMode::Table);
        crate::ct1::set_thread_active(Some(mt));
        let p = plate_with_hole(1, 3.0);
        // Stainless 2.7–3.2 mm with nitrogen: 5000 mm/min for the Ø6 hole's
        // band, 9500 outside; this record's pierce time is 0.
        let feeds: Vec<f64> = p.contours.iter().map(|c| c.feed).collect();
        assert_eq!(feeds, vec![5000.0, 9500.0], "{}", p.timing.speeds.label());
        assert_eq!(p.times.pierce, 0.0);
        crate::ct1::set_thread_active(None);
    }

    #[test]
    fn rings_in_rings_are_cut_from_the_inside_out() {
        crate::ct1::set_thread_active(None);
        // A frame in a frame's hole with a part in its hole: the innermost
        // first, or the middle frame's hole drops with the uncut part in it.
        let parts = vec![part(Shape::Rectangle, 300.0, 300.0, None)];
        let placed = vec![at(10.0, 10.0, 300.0, 300.0, false), at(50.0, 50.0, 200.0, 200.0, false), at(100.0, 100.0, 50.0, 50.0, false)];
        let p = plan(&parts, &placed, &spec());
        let mut order: Vec<usize> = p.contours.iter().map(|c| c.part_no).collect();
        order.dedup();
        assert_eq!(order, vec![3, 2, 1]);
    }

    #[test]
    fn remnant_marks_and_lines_run_at_their_own_speed() {
        crate::ct1::set_thread_active(None);
        let parts = vec![part(Shape::Rectangle, 100.0, 60.0, None)];
        let placed = vec![at(10.0, 10.0, 100.0, 60.0, false)];
        let mut p = plan(&parts, &placed, &spec());
        for c in &mut p.contours {
            c.feed = 1234.0;
        }
        let lines = vec![("REMNANT MARK".to_string(), vec![[200.0, 0.0], [200.0, 10.0]]), ("REMNANT LINE".to_string(), vec![[300.0, 0.0], [300.0, 500.0]])];
        let prog = program_mm(1, &parts, &placed, &spec(), &p, &lines, false);
        let tail = &prog[prog.find("( REMNANT MARK )").unwrap()..];
        let feeds: Vec<&str> = tail.lines().filter_map(|l| l.split_whitespace().find(|w| w.starts_with('F'))).collect();
        assert_eq!(feeds, vec![format!("F{:.0}", p.timing.speeds.mark), format!("F{:.0}", p.timing.speeds.cut_for(1))], "{prog}");
    }

    #[test]
    fn blm_without_a_lead_in_still_compensates_and_copies_keep_their_own_leads() {
        crate::ct1::set_thread_active(None);
        let parts = vec![part(Shape::Rectangle, 100.0, 100.0, None)];
        let placed = vec![at(50.0, 50.0, 100.0, 100.0, false), at(250.0, 50.0, 100.0, 100.0, false)];
        let mut p = plan(&parts, &placed, &spec());
        // BL24: no lead-in, control compensation on: the word goes on a short
        // move along the first straight before PROFILE.
        let mut bare = plan(&parts, &placed, &spec());
        for c in &mut bare.contours {
            c.lead_path.clear();
            c.comp.control = true;
            c.comp.side = Side::Right;
        }
        let sub = crate::blm::subprogram("P", &bare, 1, [50.0, 50.0]);
        let at_profile = sub.find("PROFILE").unwrap();
        assert!(sub[..at_profile].contains("G01 G42 X"), "{sub}");
        // Two copies whose leads differ get a subprogram each.
        let set = crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J");
        let one = crate::blm::sheet_set(1, &parts, &placed, &spec(), &p, &[], &set);
        assert_eq!(one.spf.iter().filter(|f| f.0.contains("70")).count(), 1, "same leads: one subprogram");
        let c2 = p.contours.iter_mut().find(|c| c.part_no == 2).unwrap();
        c2.lead_path[0][0] -= 3.0;
        let two = crate::blm::sheet_set(1, &parts, &placed, &spec(), &p, &[], &set);
        assert_eq!(two.spf.iter().filter(|f| f.0.contains("70")).count(), 2, "{:?}", two.spf.iter().map(|f| &f.0).collect::<Vec<_>>());
    }
}
#[cfg(test)]
mod edge_lead_tests {
    use crate::nest::*;

    fn plate(corner_start: bool) -> Part {
        let t = PartTech { lead_type: LeadType::Machine, corner_start, ..Default::default() };
        Part { name: "DEMO-001".into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty: 1, rotate: true, color: 0, tech: Some(t), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn a_lead_that_would_leave_the_sheet_moves_or_shortens() {
        // VP3: the sheet's edge is an obstacle for leads, like another part.
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 0.0, gap: 5.0, material: 0, thickness: 5.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let on = |q: &P| q[0] >= -0.01 && q[1] >= -0.01 && q[0] <= spec.w + 0.01 && q[1] <= spec.h + 0.01;
        // Flush in the sheet's corner, an automatic lead moves to a free side.
        let flush = vec![Placed { part: 0, x: 0.0, y: 0.0, w: 100.0, h: 100.0, turns: 0, mirror: false }];
        let plan = super::plan(&[plate(true)], &flush, &spec);
        let c = &plan.contours[0];
        assert!(c.lead_path.iter().all(on) && crate::leads::path_len(&c.lead_path) > 5.0, "{:?}", c.lead_path);
        // 3 mm in from the edges, a fixed-position lead is shortened to fit.
        let near = vec![Placed { part: 0, x: 3.0, y: 3.0, w: 100.0, h: 100.0, turns: 0, mirror: false }];
        let c = &super::plan(&[plate(false)], &near, &spec).contours[0];
        assert!(c.lead_path.iter().all(on), "{:?}", c.lead_path);
        assert!(crate::drawing::pierces_off_sheet(&super::plan(&[plate(false)], &near, &spec), spec.w, spec.h).is_empty());
        // Flush with no room at all, it can't be: the check names it.
        let off = crate::drawing::pierces_off_sheet(&super::plan(&[plate(false)], &flush, &spec), spec.w, spec.h);
        assert_eq!(off.len(), 1, "{off:?}");
    }
}

#[cfg(test)]
mod hand_lead_tests {
    use super::*;
    use crate::nest::{LeadEdit, Shape};

    fn part(shape: Shape, w: f64, h: f64, tech: Option<PartTech>) -> Part {
        Part { name: "H".into(), shape, w, h, qty: 1, rotate: false, color: 0, tech, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() }
    }

    fn with_edit(e: LeadEdit) -> PartTech {
        let mut t = auto_tech();
        t.lead_edits[0] = Some(e);
        t
    }

    fn polar_in(c: &Contour) -> (f64, f64) {
        let (_, circle) = crate::leads::classify(&c.points, c.inner);
        crate::leads::polar_of(crate::leads::in_frame(&c.points, c.inner, circle), true, c.lead_path[0])
    }

    #[test]
    fn a_part_on_its_own_keeps_its_leads_where_the_tables_put_them() {
        // Zone 1 puts a rectangle's corner lead at its bottom-left corner. On
        // its own (Part view, part-info) no sheet edge moves it away from there.
        let parts = vec![part(Shape::Rectangle, 200.0, 100.0, None)];
        let p = plan_part(&parts, 0, &spec());
        let c = &p.contours[0];
        assert!(dist(c.points[0], [0.0, 0.0]) < 1e-9, "{:?}", c.points[0]);
        assert!((crate::leads::path_len(&c.lead_path) - 7.0).abs() < 1e-6);
        // At the sheet's own corner the same lead would leave the sheet (VP3), so it moves.
        let pl = Placed { part: 0, x: 0.0, y: 0.0, w: 200.0, h: 100.0, turns: 0, mirror: false };
        assert!(dist(plan(&parts, &[pl], &spec()).contours[0].points[0], [0.0, 0.0]) > 1.0);
    }

    #[test]
    fn a_lead_moved_by_hand_keeps_its_start_and_lean_when_the_part_is_turned_or_mirrored() {
        // Plate with a hole: contour 0 the hole, 1 the outline. The outline's
        // lead-in swung round its bottom-left corner to −45° (LE1, LE2).
        let e = LeadEdit { contour: 1, at: [0.0, 0.0], rec: 3, lead_in: Some((10.0, -45.0)), lead_out: None };
        let parts = vec![part(Shape::PlateHole, 200.0, 150.0, Some(with_edit(e)))];
        for (turns, mirror) in [(0, false), (1, false), (2, true), (3, true)] {
            let (w, h) = if turns % 2 == 1 { (150.0, 200.0) } else { (200.0, 150.0) };
            let pl = Placed { part: 0, x: 300.0, y: 300.0, w, h, turns, mirror };
            let p = plan(&parts, &[pl], &spec());
            let c = p.contours.iter().find(|c| c.contour == 1).unwrap();
            assert!(c.lead_hand && c.lead_rec == 3);
            assert!(dist(c.points[0], pl.map([0.0, 0.0])) < 1e-6, "turns {turns} mirror {mirror}: {:?}", c.points[0]);
            let (l, a) = polar_in(c);
            assert!((l - 10.0).abs() < 1e-6 && (a + 45.0).abs() < 1e-6, "turns {turns} mirror {mirror}: {l} {a}");
            // The pierce is in the scrap, off the part.
            let (o, _) = crate::nest::placed_shape(&parts, &pl);
            assert!(!crate::nest::point_in_poly(&o, c.lead_in));
            // The hole is untouched: still the automatic lead.
            assert!(!p.contours.iter().find(|c| c.contour == 0).unwrap().lead_hand);
        }
    }

    #[test]
    fn a_lead_moved_by_hand_is_shortened_not_moved_when_it_would_hit_a_neighbour() {
        // A 20 mm lead-in square off the right side, toward a part 5 mm away (LE5).
        let e = LeadEdit { contour: 0, at: [100.0, 50.0], rec: 3, lead_in: Some((20.0, 90.0)), lead_out: None };
        let parts = vec![part(Shape::Rectangle, 100.0, 100.0, Some(with_edit(e))), part(Shape::Rectangle, 100.0, 100.0, None)];
        let a = Placed { part: 0, x: 100.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false };
        let b = Placed { part: 1, x: 205.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false };
        let p = plan(&parts, &[a, b], &spec());
        let c = p.contours.iter().find(|c| c.part_no == 1).unwrap();
        assert!(dist(c.points[0], [200.0, 150.0]) < 1e-6, "the start stays: {:?}", c.points[0]);
        let l = crate::leads::path_len(&c.lead_path);
        assert!(l < 5.0 && l > 0.0, "{l}");
        // On its own the part keeps the full 20 mm.
        assert!((crate::leads::path_len(&plan_part(&parts, 0, &spec()).contours[0].lead_path) - 20.0).abs() < 1e-6);
    }
}
