//! Cutting sequence, cost estimate and a simple ISO (G-code) machine program
//! for one nested sheet. Speeds and pierce times are typical estimates.

use crate::leads::LeadRec;
use crate::nest::{perimeter, CutQuality, LeadType, Part, PartTech, Placed, SheetSpec, P};

pub const MATERIALS: [&str; 3] = ["Mild steel", "Stainless steel", "Aluminium"];
pub const THICKNESSES: [f64; 7] = [1.0, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0];
const DENSITY: [f64; 3] = [7.85, 8.0, 2.70];
pub const RAPID_MM_MIN: f64 = 60000.0;

/// Estimated cutting speed (mm/min) for a fibre laser.
pub fn cut_speed(material: usize, t: f64) -> f64 {
    let table: [[f64; 7]; 3] = [
        [20000.0, 9000.0, 5000.0, 3000.0, 1800.0, 1300.0, 800.0],
        [22000.0, 10000.0, 4500.0, 2000.0, 900.0, 600.0, 300.0],
        [25000.0, 12000.0, 6000.0, 2500.0, 1000.0, 700.0, 400.0],
    ];
    let i = THICKNESSES.iter().position(|&x| (x - t).abs() < 1e-6).unwrap_or(2);
    table[material.min(2)][i]
}

/// Estimated time (s) to pierce once.
pub fn pierce_time(t: f64) -> f64 {
    0.3 + 0.12 * t * t / 2.0
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

pub struct SheetPlan {
    pub contours: Vec<Contour>,
    /// Marking lines per part number (from drawing layers set to "Mark").
    pub marks: Vec<(usize, Vec<P>)>,
    pub mark_len: f64,
    pub cut_len: f64,
    pub rapid_len: f64,
    pub pierces: usize,
    pub seconds: f64,
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
/// (rule MM3). Then the rest automatically.
pub fn cut_jobs(parts: &[Part], placed: &[Placed], order: &[usize], manual: &[ManualCut]) -> Vec<(usize, usize, Option<(P, bool)>)> {
    let count = |i: usize| parts[placed[i].part].geometry().holes.len() + 1;
    let mut jobs: Vec<(usize, usize, Option<(P, bool)>)> = vec![];
    let done = |jobs: &Vec<(usize, usize, Option<(P, bool)>)>, i: usize, ci: usize| jobs.iter().any(|j| j.0 == i && j.1 == ci);
    for m in manual {
        if m.placed >= placed.len() || m.contour >= count(m.placed) || done(&jobs, m.placed, m.contour) {
            continue;
        }
        let outer = m.contour == count(m.placed) - 1;
        if outer {
            for h in 0..count(m.placed) - 1 {
                if !done(&jobs, m.placed, h) {
                    jobs.push((m.placed, h, None));
                }
            }
        }
        jobs.push((m.placed, m.contour, Some((m.start, m.reverse))));
    }
    for &i in order {
        for ci in 0..count(i) {
            if !done(&jobs, i, ci) {
                jobs.push((i, ci, None));
            }
        }
    }
    jobs
}

/// Like `plan`, with some contours cut by hand first.
pub fn plan_manual(parts: &[Part], placed: &[Placed], spec: &SheetSpec, manual: &[ManualCut]) -> SheetPlan {
    let mut left: Vec<usize> = (0..placed.len()).collect();
    let mut order = Vec::new();
    let mut at: P = [0.0, 0.0];
    // A part nested inside another part's hole is cut before that part,
    // or it would drop with the scrap of the hole.
    let inside_other = |i: usize| {
        let p = &placed[i];
        placed.iter().enumerate().any(|(j, o)| j != i && o.contains([p.x, p.y]) && o.contains([p.x + p.w, p.y + p.h]))
    };
    // Zone by zone (rules CZ4, CZ5): the parts of each zone, nearest first,
    // with parts sitting in another part's hole before the rest.
    let mt = crate::ct1::active();
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
                let first = if inside_other(i) { 0.0 } else { 1e9 };
                (k, first + dist(at, [p.x, p.y]))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        let i = left.remove(k);
        at = [placed[i].x + placed[i].w, placed[i].y];
        order.push(i);
    }

    let base_speed = cut_speed(spec.material, spec.thickness);
    let mut contours = Vec::new();
    let mut cut_len = 0.0;
    let mut rapid_len = 0.0;
    let mut cut_secs = 0.0;
    let mut marks: Vec<(usize, Vec<P>)> = vec![];
    let mut mark_len = 0.0;
    let mut joint_pierces = 0usize;
    let mut pos: P = [0.0, 0.0];
    let jobs = cut_jobs(parts, placed, &order, manual);
    let mut marked = vec![false; placed.len()];
    // Parts without their own technology: the machine's automatic leads, no joints.
    let auto_tech = PartTech { lead_in: 0.0, lead_out: 0.0, micro_joints: 0, joint_width: 0.0, quality: CutQuality::Normal, lead_type: LeadType::Machine, corner_start: true, position: 0, ..PartTech::default() };
    let tol = mt.leads.lin_tol;
    // Every placed part's contours (holes first, outline last), for the leads' collision test.
    let obst: Vec<([f64; 4], Vec<(Vec<P>, bool)>)> = placed.iter().map(|pl| (pl.bbox4(), sheet_contours(&parts[pl.part], pl))).collect();
    for (i, ci, man) in jobs {
        let pl = &placed[i];
        let part = &parts[pl.part];
        // Parts without their own technology get the machine's automatic leads
        // (tables by contour class) and no joints.
        let tech = part.tech.unwrap_or(auto_tech);
        let feed = base_speed * tech.quality.speed_factor();
        let (mut pts, inner) = obst[i].1[ci].clone();
        // Cut quality by contour size: the first matching range for the
        // contour's equivalent radius (rule CT1).
        let r_eq = crate::nest::equivalent_radius(signed_area2(&pts) / 2.0);
        let cq = crate::nest::quality_for_radius(&tech.cut_ranges, r_eq, 0.0);
        let feed = cq.map_or(feed, |q| base_speed * q.speed_factor());
        // The lead table of the contour's quality (rule AL1).
        let qn = crate::leads::quality_number(cq.unwrap_or(tech.quality));
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
                cut_secs += l / (base_speed * MARK_SPEED_FACTOR) * 60.0;
                pos = *pts.last().unwrap();
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
                cut_secs += l / (base_speed * MARK_SPEED_FACTOR) * 60.0;
                pos = *pts.last().unwrap();
                marks.push((i + 1, pts));
            }
        }
        }
        {
            // A manual pick sets the start and direction; then a lead-in picked on
            // an element; otherwise the automatic placement by contour class.
            let picked = tech.starts.iter().flatten().find(|s| s.0 as usize == ci).map(|s| pl.map(s.1));
            let (class, circle) = crate::leads::classify(&pts, inner);
            let recs = lead_recs(&tech, &lq);
            let zone = lq.zone_for(inner, (tech.position >= 1).then_some(tech.position as i32));
            let auto = tech.corner_start && man.is_none() && picked.is_none() && pts.len() >= 3;
            let mut placement = None;
            if let Some((sp, rev)) = man {
                if rev {
                    pts.reverse();
                }
                pts = start_at_point(pts, sp);
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
                for &h in &p.hits {
                    rapid_len += dist(pos, h);
                    pos = h;
                }
                cut_secs += p.hits.len() as f64 * crate::machine::HIT_SECONDS;
                let first = p.hits.first().copied().unwrap_or(pts[0]);
                contours.push(Contour { lead_in: first, lead_path: vec![], lead_out: None, lead_out_path: vec![], loops: vec![], points: pts, inner, part_no: i + 1, joints: vec![], joint_leads: vec![], feed, manual: man.is_some(), punch: Some(p), overlap: 0.0, closing: false });
                continue;
            }
            // The lead record: the placement's, else the class's. An automatic
            // placement that found nothing gets no lead (AL3); a corner
            // type away from a corner is built perpendicular (our choice).
            let mut rec = recs[placement.map_or(class.rec(), |p| p.rec)];
            if auto && placement.is_none() {
                rec.kind = 0;
            }
            let at_corner = |q: &[P]| (corner_at(q, 0).0 - 180.0).abs() > crate::nest::ELEMENT_CORNER_DEG;
            if matches!(rec.kind, 10 | 11) && !placement.is_some_and(|p| p.corner) && !at_corner(&pts) {
                rec.kind = 1;
            }
            let mut geo = crate::leads::build(&pts, inner, &rec, circle, lq.limit_to_centre, tol);
            // Collision test (B185, AL11): automatic leads move along the
            // contour (B187 tries) and/or shorten (B186 per step, floor
            // max(2·tol, B295)) until clear of every contour on the sheet. Without
            // the test (or for picked leads) a lead is still shortened when it
            // crosses its own part (our safety rule).
            let full = auto && mt.leads.test > 0;
            let hits = |path: &[P], anchor: P, all: bool| lead_hits(path, anchor, i, ci, inner, &obst, all, tol);
            let floor = (2.0 * tol).max(lq.min_lead);
            let (mv, sh) = if full { crate::leads::test_steps(mt.leads.test) } else { (false, true) };
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
            let start = pts[0];
            let mut path = geo.lead_in.clone();
            let mut lead_len = crate::leads::path_len(&path);
            let mut lead_in = path[0];
            let per = perimeter(&pts);
            let mut overlap = geo.overlap;
            let mut lead_out_path = geo.lead_out.clone();
            let mut out_len = crate::leads::path_len(&lead_out_path);
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
            let uncut: f64 = joints.iter().map(|j| j.1 - j.0).sum();
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
            let closing = !inner && man.is_none() && picked.is_none() && !joint_leads.is_empty();
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
                out_len = 0.0;
                lead_out = None;
                lead_out_path.clear();
                overlap = 0.0;
            }
            let joint_lead_len: f64 = joint_leads.iter().map(|(o, i)| crate::leads::path_len(o) + crate::leads::path_len(i)).sum();
            // Each joint with leads re-pierces in the scrap (the closing joint's
            // pierce is the contour's own).
            joint_pierces += joint_leads.len() - usize::from(closing);
            for (o, i) in joint_leads.iter().take(joint_leads.len() - usize::from(closing)) {
                rapid_len += dist(*o.last().unwrap(), i[0]);
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
            let loop_len: f64 = loops
                .iter()
                .map(|(i, l)| {
                    let mut prev_p = pts[*i];
                    l.iter()
                        .map(|q| {
                            let d = dist(prev_p, *q);
                            prev_p = *q;
                            d
                        })
                        .sum::<f64>()
                })
                .sum();
            let len = per - uncut + lead_len + out_len + loop_len + joint_lead_len + overlap;
            rapid_len += dist(pos, lead_in);
            cut_len += len;
            cut_secs += len / feed * 60.0;
            pos = if closing { joint_leads.last().map_or(start, |l| *l.0.last().unwrap()) } else { lead_out.unwrap_or(start) };
            contours.push(Contour { lead_in, lead_path: path, lead_out, lead_out_path, loops, points: pts, inner, part_no: i + 1, joints, joint_leads, feed, manual: man.is_some(), punch: None, overlap, closing });
        }
    }
    let pierces = contours.iter().filter(|c| c.punch.is_none()).count() + joint_pierces;
    let seconds = cut_secs + rapid_len / RAPID_MM_MIN * 60.0 + pierces as f64 * pierce_time(spec.thickness);
    SheetPlan { contours, marks, mark_len, cut_len, rapid_len, pierces, seconds }
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
    line(&mut s, "G90 G21 G17".into());
    line(&mut s, format!("F{:.0}", cut_speed(spec.material, spec.thickness)));
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
                line(&mut s, format!("G0 X{:.2} Y{:.2}", m[0][0], m[0][1]));
                line(&mut s, "M10 ( MARK ON )".into());
                for q in m.iter().skip(1) {
                    line(&mut s, format!("G1 X{:.2} Y{:.2}", q[0], q[1]));
                }
                line(&mut s, "M11 ( MARK OFF )".into());
            }
        }
        if let Some(p) = &c.punch {
            // One hit per standard shape, a row of hits when nibbling.
            let _ = writeln!(s, "( {} )", p.label());
            for h in &p.hits {
                line(&mut s, format!("G0 X{:.2} Y{:.2}", h[0], h[1]));
                line(&mut s, "M12 ( HIT )".into());
            }
            continue;
        }
        line(&mut s, format!("G0 X{:.2} Y{:.2}", c.lead_in[0], c.lead_in[1]));
        line(&mut s, format!("M07 ( PIERCE{} )", if c.inner { " INNER" } else { "" }));
        // Lead-in path (line, line + arc, arc or from the hole centre).
        for (k, q) in c.lead_path.iter().enumerate().skip(1) {
            if k == 1 {
                line(&mut s, format!("G1 X{:.2} Y{:.2} F{:.0} ( LEAD-IN )", q[0], q[1], c.feed));
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
        events.sort_by(|a, b| a.partial_cmp(b).unwrap());
        events.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let mut prev = 0.0;
        let mut beam_on = true;
        let mut done_joint: Option<usize> = None;
        for d in events {
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
            let p = point_at(&c.points, d);
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
        let _ = writeln!(s, "( {label} )");
        line(&mut s, format!("G0 X{:.2} Y{:.2}", first[0], first[1]));
        line(&mut s, "M07 ( PIERCE )".into());
        for q in pts.iter().skip(1) {
            line(&mut s, format!("G1 X{:.2} Y{:.2}", q[0], q[1]));
        }
        line(&mut s, "M08".into());
    }
    line(&mut s, "G0 X0 Y0".into());
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
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
        plan(&parts, &[pl], &spec)
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
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
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
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
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
        let rows = [CutRange { on: true, min_r: 0.0, max_r: 5.0, quality: CutQuality::Fine }, CutRange { on: true, min_r: 5.0, max_r: 50.0, quality: CutQuality::Rough }, CutRange::default()];
        assert_eq!(quality_for_radius(&rows, 4.0, 0.0), Some(CutQuality::Fine));
        assert_eq!(quality_for_radius(&rows, 5.0, 0.0), Some(CutQuality::Rough), "upper limit not included");
        assert_eq!(quality_for_radius(&rows, 60.0, 0.0), None);
        let late = [CutRange { on: true, min_r: 10.0, max_r: 20.0, quality: CutQuality::Fine }, CutRange::default(), CutRange::default()];
        assert_eq!(quality_for_radius(&late, 4.0, 0.0), None, "first row starts above the radius");
        // In the plan: the part's outside (r ≈ 79.8) keeps Normal, a fine row slows a matching contour.
        let t = PartTech { cut_ranges: [CutRange { on: true, min_r: 0.0, max_r: 100.0, quality: CutQuality::Fine }, CutRange::default(), CutRange::default()], ..PartTech::default() };
        let (fine, normal) = (plan_for(t), plan_for(PartTech::default()));
        assert!(fine.contours[0].feed < normal.contours[0].feed);
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
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
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
        let spec = SheetSpec { w: 1000.0, h: 1000.0, margin: 5.0, gap: 3.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
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
        (parts, placed, SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() })
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
