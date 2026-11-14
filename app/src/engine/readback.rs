//! Reading an LC5 program set back before it goes out (BL39). The written text
//! is parsed on its own, with nothing shared with the writer, and every cut is
//! compared with the path it was planned to follow, one against the other in
//! both directions. A set that doesn't match is refused: an error in the writer
//! must never reach the machine looking like a finished program.

use crate::blm::BlmSet;
use crate::cnc::{Made, SheetPlan};
use crate::cutoff::{CutKind, CutLine};
use crate::nest::{Part, Placed, P};
use std::collections::HashMap;

/// How far a written cut may run from its planned path (mm): the arc fit's own
/// allowance (BL38) and the rounding of written coordinates.
pub fn tolerance() -> f64 {
    crate::blm::ARC_SPAN_TOL + 0.02
}
/// A move is split no finer than this while looking for where it leaves the
/// other path (mm), so what the check lets through is at most this much past
/// its tolerance.
const MIN_SPAN: f64 = 0.02;
/// How far a circle may flatten when an arc is rebuilt for the comparison (mm).
const ARC_SAMPLE_ERR: f64 = 0.002;
/// An arc's start and end must be this close to the same radius (mm).
const ARC_RADIUS_TOL: f64 = 0.02;
/// How close a written start must be to a planned one to be that part (mm).
const START_TOL: f64 = 0.01;

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn inside<'a>(line: &'a str, head: &str) -> Option<&'a str> {
    line.strip_prefix(head)?.split(')').next()
}

fn numbers(list: &str, line: &str) -> Result<Vec<f64>, String> {
    list.split(',').map(|v| v.trim().parse::<f64>().map_err(|_| format!("can't read \"{line}\""))).collect()
}

/// The points along a written arc, without its start.
fn arc_path(from: P, to: P, i: f64, j: f64, ccw: bool) -> Result<Vec<P>, String> {
    let c = [from[0] + i, from[1] + j];
    let (r0, r1) = (dist(from, c), dist(to, c));
    if (r0 - r1).abs() > ARC_RADIUS_TOL || r0 <= 0.0 {
        return Err(format!("an arc to X{:.3} Y{:.3} doesn't end on its own circle ({r0:.3} and {r1:.3} mm)", to[0], to[1]));
    }
    let a0 = (from[1] - c[1]).atan2(from[0] - c[0]);
    let mut a1 = (to[1] - c[1]).atan2(to[0] - c[0]);
    let tau = std::f64::consts::TAU;
    if ccw && a1 <= a0 {
        a1 += tau;
    }
    if !ccw && a1 >= a0 {
        a1 -= tau;
    }
    let step = if r0 > ARC_SAMPLE_ERR { 2.0 * (1.0 - ARC_SAMPLE_ERR / r0).acos() } else { tau / 4.0 };
    let n = (((a1 - a0).abs() / step).ceil() as usize).clamp(1, 4000);
    let mut v: Vec<P> = (1..n)
        .map(|k| {
            let t = k as f64 / n as f64;
            let (r, a) = (r0 + (r1 - r0) * t, a0 + (a1 - a0) * t);
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect();
    v.push(to);
    Ok(v)
}

/// One cut: its path from the pierce on, and the compensation word it runs on.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    pub pts: Vec<P>,
    pub comp: Option<&'static str>,
}

/// The cuts of one written subprogram, in order.
pub fn written_paths(text: &str) -> Result<Vec<Cut>, String> {
    let mut out: Vec<Cut> = vec![];
    let mut cur: Option<P> = None;
    for raw in text.lines() {
        let l = raw.trim();
        if let Some(a) = inside(l, "NEXT_L(") {
            let v = numbers(a, l)?;
            if v.len() < 2 {
                return Err(format!("can't read \"{l}\""));
            }
            cur = Some([v[0], v[1]]);
            out.push(Cut { pts: vec![[v[0], v[1]]], comp: None });
        } else if let Some(a) = inside(l, "LASTLINE(") {
            let v = numbers(a, l)?;
            let (Some(from), Some(cut)) = (cur, out.last_mut()) else { return Err(format!("\"{l}\" before any pierce")) };
            if v.len() != 10 {
                return Err(format!("can't read \"{l}\""));
            }
            if dist(from, [v[4], v[5]]) > START_TOL {
                return Err(format!("\"{l}\" doesn't start where the cut before it ended"));
            }
            let to = [v[6], v[7]];
            if v[3] == 2.0 || v[3] == 3.0 {
                cut.pts.extend(arc_path(from, to, v[8], v[9], v[3] == 3.0)?);
            } else {
                cut.pts.push(to);
            }
            cur = Some(to);
        } else if l.starts_with("G01") || l.starts_with("G02") || l.starts_with("G03") {
            let (Some(from), Some(cut)) = (cur, out.last_mut()) else { return Err(format!("\"{l}\" before any pierce")) };
            let mut to = from;
            let (mut i, mut j) = (0.0, 0.0);
            for w in l.split_whitespace().skip(1) {
                match w {
                    "G41" => cut.comp = Some("G41"),
                    "G42" => cut.comp = Some("G42"),
                    _ => {}
                }
                let Some(field) = w.get(..1) else { continue };
                if matches!(field, "X" | "Y" | "I" | "J") {
                    let v: f64 = w[1..].parse().map_err(|_| format!("can't read \"{l}\""))?;
                    match field {
                        "X" => to[0] = v,
                        "Y" => to[1] = v,
                        "I" => i = v,
                        _ => j = v,
                    }
                }
            }
            if l.starts_with("G01") {
                cut.pts.push(to);
            } else {
                cut.pts.extend(arc_path(from, to, i, j, l.starts_with("G03"))?);
            }
            cur = Some(to);
        }
    }
    Ok(out)
}

/// A path's segments filed by place, so "is anything of it this close?" looks
/// at a few segments, however long the path.
struct Near<'a> {
    pts: &'a [P],
    cell: f64,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl<'a> Near<'a> {
    fn new(pts: &'a [P], tol: f64) -> Near<'a> {
        let len: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        let cell = (len / pts.len().max(1) as f64).clamp(1.0, 50.0).max(tol * 2.0);
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, w) in pts.windows(2).enumerate() {
            let n = (dist(w[0], w[1]) / (cell / 2.0)).ceil() as usize;
            for s in 0..=n.max(1) {
                let t = s as f64 / n.max(1) as f64;
                let (cx, cy) = (((w[0][0] + (w[1][0] - w[0][0]) * t) / cell).floor() as i64, ((w[0][1] + (w[1][1] - w[0][1]) * t) / cell).floor() as i64);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let v = cells.entry((cx + dx, cy + dy)).or_default();
                        if v.last() != Some(&(k as u32)) {
                            v.push(k as u32);
                        }
                    }
                }
            }
        }
        Near { pts, cell, cells }
    }

    fn seg(&self, p: P, k: usize) -> f64 {
        let (a, b) = (self.pts[k], self.pts[k + 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx * dx + dy * dy;
        let t = if l == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0) };
        dist(p, [a[0] + t * dx, a[1] + t * dy])
    }

    fn within(&self, p: P, tol: f64) -> bool {
        let key = ((p[0] / self.cell).floor() as i64, (p[1] / self.cell).floor() as i64);
        self.cells.get(&key).is_some_and(|v| v.iter().any(|&k| self.seg(p, k as usize) <= tol))
    }

    /// Is one segment of the path close to both a and b? Then all of a–b is:
    /// the distance to a segment can't rise and fall back along a straight line.
    fn covers(&self, a: P, b: P, tol: f64) -> bool {
        let key = ((a[0] / self.cell).floor() as i64, (a[1] / self.cell).floor() as i64);
        self.cells.get(&key).is_some_and(|v| v.iter().any(|&k| self.seg(a, k as usize) <= tol && self.seg(b, k as usize) <= tol))
    }

    /// The first point of a–b that isn't within `tol`, and how far it is.
    fn stray(&self, a: P, b: P, tol: f64) -> Option<(P, f64)> {
        if self.covers(a, b, tol) {
            return None;
        }
        if dist(a, b) <= MIN_SPAN {
            return [a, b].into_iter().find(|&p| !self.within(p, tol)).map(|p| (p, self.distance(p)));
        }
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        self.stray(a, mid, tol).or_else(|| self.stray(mid, b, tol))
    }

    /// The true distance, for the message when a point is out.
    fn distance(&self, p: P) -> f64 {
        (0..self.pts.len().saturating_sub(1)).map(|k| self.seg(p, k)).fold(f64::MAX, f64::min)
    }
}

/// The first point of `a` that is further than `tol` from `b`, and how far.
fn first_stray(a: &[P], b: &[P], tol: f64) -> Option<(P, f64)> {
    if b.len() < 2 {
        return a.first().map(|&p| (p, f64::MAX));
    }
    let near = Near::new(b, tol);
    if let [only] = a {
        return (!near.within(*only, tol)).then(|| (*only, near.distance(*only)));
    }
    a.windows(2).find_map(|w| near.stray(w[0], w[1], tol))
}

/// Twice the area a path encloses, the sign saying which way round it runs.
fn turned_area(p: &[P]) -> f64 {
    (0..p.len()).map(|i| p[i][0] * p[(i + 1) % p.len()][1] - p[(i + 1) % p.len()][0] * p[i][1]).sum()
}

/// The point a share `f` of the way along a path.
fn along(p: &[P], f: f64) -> P {
    let total: f64 = p.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut left = total * f;
    for w in p.windows(2) {
        let d = dist(w[0], w[1]);
        if left <= d && d > 0.0 {
            let t = left / d;
            return [w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t];
        }
        left -= d;
    }
    p[p.len() - 1]
}

/// Do two paths over the same ground visit it in another order? An arc is a
/// little longer than its chords, hence the slack. For paths whose enclosed area
/// can't say: a figure-eight encloses nothing either way round.
fn other_order(w: &[P], e: &[P]) -> bool {
    let len = |p: &[P]| p.windows(2).map(|s| dist(s[0], s[1])).sum::<f64>();
    let slack = 1.0 + 0.02 * len(w).max(len(e));
    (1..10).any(|k| dist(along(w, k as f64 / 10.0), along(e, k as f64 / 10.0)) > slack)
}

/// What's wrong with one written cut against its plan, in words.
fn compare(w: &Cut, e: &Cut, tol: f64) -> Option<String> {
    if let Some((p, d)) = first_stray(&w.pts, &e.pts, tol) {
        return Some(format!("the written cut runs {d:.2} mm from the planned one near X{:.1} Y{:.1}", p[0], p[1]));
    }
    if let Some((p, d)) = first_stray(&e.pts, &w.pts, tol) {
        return Some(format!("the planned cut is {d:.2} mm from anything written near X{:.1} Y{:.1}", p[0], p[1]));
    }
    if w.comp != e.comp {
        return Some(format!("it is written with compensation {} where {} is planned, so the part would come out a kerf off size", w.comp.unwrap_or("off"), e.comp.unwrap_or("off")));
    }
    // The same path the other way round cuts on the wrong side.
    let (aw, ae) = (turned_area(&w.pts), turned_area(&e.pts));
    // An area smaller than the tolerance times the length is just the difference
    // between arcs and their chords, and says nothing about direction.
    let length: f64 = e.pts.windows(2).map(|s| dist(s[0], s[1])).sum();
    let reversed = if aw.abs().min(ae.abs()) > tol * length { aw * ae < 0.0 } else { other_order(&w.pts, &e.pts) };
    let (lw, le) = (w.pts.last(), e.pts.last());
    if reversed || matches!((lw, le), (Some(&a), Some(&b)) if dist(a, b) > tol) {
        return Some("it is written running the other way round from the plan".into());
    }
    None
}

/// What a part should cut, absolute on the sheet, in the writer's order: its
/// marks and contours in its cut order (CO9; with no order set by hand, the
/// marks first), each contour from the pierce, closed. A path the writer has to
/// leave out (no length) isn't expected.
fn planned_paths(plan: &SheetPlan, part_no: usize) -> Vec<Cut> {
    let same = |a: &P, b: &P| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    let mut out = vec![];
    for made in plan.part_order(part_no) {
        let c = match made {
            Made::Mark(j) => {
                let mut p = plan.marks[j].1.clone();
                p.dedup_by(|a, b| same(a, b));
                if p.len() >= 2 {
                    out.push(Cut { pts: p, comp: None });
                }
                continue;
            }
            Made::Contour(k) if plan.contours[k].punch.is_none() => &plan.contours[k],
            Made::Contour(_) => continue,
        };
        let mut body = c.points.clone();
        if body.len() > 1 {
            body.push(c.points[0]);
        }
        body.dedup_by(|a, b| same(a, b));
        if body.len() < 2 {
            continue;
        }
        let mut p = if c.lead_path.len() >= 2 { c.lead_path.clone() } else { vec![] };
        p.extend(body);
        p.dedup_by(|a, b| same(a, b));
        out.push(Cut { pts: p, comp: c.comp.word().map(|(side, _)| side.word()) });
    }
    out
}

/// What the LC5 files can't carry yet (MAC-30): a part planned with these would
/// be cut differently from what the screen and the times show.
fn unsupported(parts: &[Part], sheet: &[Placed], plan: &SheetPlan) -> Vec<String> {
    let mut seen: Vec<(String, &str)> = vec![];
    for c in &plan.contours {
        let name = sheet.get(c.part_no.wrapping_sub(1)).and_then(|pl| parts.get(pl.part)).map_or("?", |p| p.name.as_str());
        for (has, what) in [(!c.joints.is_empty(), "micro-joints"), (!c.loops.is_empty(), "corner loops"), (c.punch.is_some(), "punched holes")] {
            if has && !seen.iter().any(|s| s.0 == name && s.1 == what) {
                seen.push((name.to_string(), what));
            }
        }
    }
    seen.into_iter().map(|(n, w)| format!("part {n} has {w}, which LC5 programs can't carry yet; take them off the part first")).collect()
}

/// The cut-off lines the writer puts in the set: both kinds, and only with some
/// length (a line from a point to itself has nothing to cut).
fn written_cut_lines(cuts: &[CutLine]) -> Vec<&CutLine> {
    let by = |kind| cuts.iter().filter(move |c| c.kind == kind && c.pts.iter().any(|q| dist(*q, c.pts[0]) > 1e-6));
    by(CutKind::Free).chain(by(CutKind::Remnant)).collect()
}

/// How far a written cut-off strays from its line (MAC-42: its ends run past the
/// sheet edge, so only the line it runs on is compared, not where it stops).
fn cut_line_stray(w: &[P], line: &CutLine, tol: f64) -> Option<f64> {
    if let [a, b] = line.pts[..] {
        let l = dist(a, b);
        let off = |p: &P| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / l;
        return w.iter().map(off).find(|d| *d > tol);
    }
    first_stray(w, &line.pts, tol).map(|h| h.1)
}

/// Everything wrong with a written set, in shop terms. Empty = it cuts what
/// was planned.
pub fn check_set(set: &BlmSet, parts: &[Part], sheet: &[Placed], plan: &SheetPlan, cuts: &[CutLine]) -> Vec<String> {
    let mut issues = unsupported(parts, sheet, plan);
    let tol = tolerance();
    let name_of = |i: usize| sheet.get(i).and_then(|pl| parts.get(pl.part)).map_or("?", |p| p.name.as_str());
    let main = set.spf.iter().find(|f| f.0.eq_ignore_ascii_case(&format!("{}.SPF", set.nest)));
    let Some((_, main)) = main else {
        issues.push("the set has no sheet program".into());
        return issues;
    };
    let mut planned: Vec<Option<Vec<Cut>>> = (0..sheet.len()).map(|i| Some(planned_paths(plan, i + 1)).filter(|p| !p.is_empty())).collect();
    let mut lines: Vec<Option<&CutLine>> = written_cut_lines(cuts).into_iter().map(Some).collect();
    let mut memo: HashMap<String, Vec<Cut>> = HashMap::new();
    let mut orig = ([0.0, 0.0], 0.0f64);
    for raw in main.lines() {
        let l = raw.trim();
        if let Some(a) = inside(l, "ORIG(") {
            match numbers(a, l) {
                Ok(v) if v.len() == 3 => orig = ([v[0], v[1]], v[2]),
                _ => issues.push(format!("can't read \"{l}\"")),
            }
            continue;
        }
        if !l.contains("CALL\"") {
            continue;
        }
        let stem = l.rsplit("_n_").next().unwrap_or("").trim_end_matches('"').trim_end_matches("_spf").to_uppercase();
        if !memo.contains_key(&stem) {
            let Some(file) = set.spf.iter().find(|f| f.0.to_uppercase() == format!("{stem}.SPF")) else {
                issues.push(format!("the sheet program calls {stem}, which isn't in the set"));
                continue;
            };
            match written_paths(&file.1) {
                Ok(p) => {
                    memo.insert(stem.clone(), p);
                }
                Err(e) => {
                    issues.push(format!("{stem}: {e}"));
                    memo.insert(stem.clone(), vec![]);
                    continue;
                }
            }
        }
        let (s, c) = orig.1.to_radians().sin_cos();
        let place = |p: &P| [orig.0[0] + p[0] * c - p[1] * s, orig.0[1] + p[0] * s + p[1] * c];
        let written: Vec<Cut> = memo[&stem].iter().map(|w| Cut { pts: w.pts.iter().map(place).collect(), comp: w.comp }).collect();
        let Some(first) = written.first().and_then(|w| w.pts.first()).copied() else { continue };
        // Which part this call is: one that starts there. Two parts can share
        // a start, so the one it fully matches wins.
        let fails = |want: &[Cut]| -> Option<String> {
            if want.len() != written.len() {
                return Some(format!("it is written with {} cuts, but {} are planned", written.len(), want.len()));
            }
            written.iter().zip(want).enumerate().find_map(|(k, (w, e))| compare(w, e, tol).map(|m| format!("cut {}: {m}", k + 1)))
        };
        let starts: Vec<usize> = (0..planned.len()).filter(|&i| planned[i].as_ref().is_some_and(|e| dist(e[0].pts[0], first) <= START_TOL)).collect();
        if let Some(&i) = starts.iter().find(|&&i| fails(planned[i].as_deref().unwrap_or(&[])).is_none()).or(starts.first()) {
            if let Some(m) = fails(planned[i].as_deref().unwrap_or(&[])) {
                issues.push(format!("part {} (sheet position {}): {m}", name_of(i), i + 1));
            }
            planned[i] = None;
            continue;
        }
        // Not a part: it must be a planned cut-off, placed at that line's start.
        let line = lines.iter().position(|e| e.is_some_and(|e| dist(e.pts[0], orig.0) <= START_TOL));
        let Some(k) = line else {
            issues.push(format!("the sheet program calls {stem} at X{:.1} Y{:.1}, which is no planned part or cut-off line", orig.0[0], orig.0[1]));
            continue;
        };
        if let Some(line) = lines[k].take() {
            let d = written.iter().find_map(|w| cut_line_stray(&w.pts, line, tol));
            if let Some(d) = d {
                issues.push(format!("the cut-off line from X{:.1} Y{:.1} is written {d:.2} mm off its planned line", orig.0[0], orig.0[1]));
            }
            // Compensation would move it off the line the numbers show.
            if written.iter().any(|w| w.comp.is_some()) {
                issues.push(format!("the cut-off line from X{:.1} Y{:.1} is written with compensation on", orig.0[0], orig.0[1]));
            }
        }
    }
    for (i, left) in planned.iter().enumerate() {
        if left.is_some() {
            issues.push(format!("part {} (sheet position {}) isn't in the sheet program", name_of(i), i + 1));
        }
    }
    for line in lines.iter().flatten() {
        issues.push(format!("the cut-off line from X{:.1} Y{:.1} isn't in the sheet program", line.pts[0][0], line.pts[0][1]));
    }
    issues
}

/// The refusal message for the operator.
pub fn refusal(sheet_no: usize, issues: &[String]) -> String {
    let shown: Vec<&str> = issues.iter().take(3).map(String::as_str).collect();
    let more = issues.len().saturating_sub(shown.len());
    let rest = if more > 0 { format!(" (and {more} more)") } else { String::new() };
    format!("The LC5 program for sheet {sheet_no} wouldn't cut what's planned, so nothing was exported: {}{rest}.", shown.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::{Part, Placed, Shape, SheetSpec};

    fn spec() -> SheetSpec {
        SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None, pick: Default::default() }
    }

    fn job() -> (Vec<Part>, Vec<Placed>, SheetPlan, BlmSet) {
        let parts = vec![Part { name: "DEMO-001".into(), shape: Shape::Disc, w: 200.0, h: 200.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let placed = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 200.0, h: 200.0, turns: 0, mirror: false, spin: Default::default() }, Placed { part: 0, x: 300.0, y: 10.0, w: 200.0, h: 200.0, turns: 0, mirror: false, spin: Default::default() }];
        let plan = crate::cnc::plan(&parts, &placed, &spec());
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &[], &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        (parts, placed, plan, set)
    }

    fn edit(set: &mut BlmSet, sub: bool, f: impl Fn(&str) -> String) {
        let file = set.spf.iter_mut().find(|x| x.0.contains("70") == sub && !x.0.contains("LIS")).unwrap();
        file.1 = f(&file.1);
    }

    #[test]
    fn a_set_that_cuts_the_plan_passes() {
        let (parts, placed, plan, set) = job();
        assert_eq!(check_set(&set, &parts, &placed, &plan, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_bent_edge_a_moved_part_or_a_lost_cut_are_all_found() {
        let (parts, placed, plan, mut bad) = job();
        // BL39: an arc swapped the other way round bulges off the part.
        edit(&mut bad, true, |t| if t.contains("G03") { t.replacen("G03", "G02", 1) } else { t.replacen("G02", "G03", 1) });
        let out = check_set(&bad, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("DEMO-001") && m.contains(" mm from ")), "{out:?}");

        // A placement shifted 1 mm is no longer that part.
        let (_, _, _, mut moved) = job();
        edit(&mut moved, false, |t| t.replacen("ORIG(", "ORIG(1", 1));
        let out = check_set(&moved, &parts, &placed, &plan, &[]);
        assert!(!out.is_empty(), "a moved part must not pass");

        // A sheet program that lost a part's call.
        let (_, _, _, mut lost) = job();
        edit(&mut lost, false, |t| t.lines().filter(|l| !l.starts_with("N2 CALL")).collect::<Vec<_>>().join("\r\n"));
        let out = check_set(&lost, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("isn't in the sheet program")), "{out:?}");
    }

    #[test]
    fn a_small_shift_inside_the_part_is_found_though_another_cut_runs_nearby() {
        // One cut is compared with its own plan: a copy of the outline 1 mm off
        // doesn't pass because some other path on the sheet is close.
        let square = |x: f64| vec![[x, 0.0], [x + 100.0, 0.0], [x + 100.0, 100.0], [x, 100.0], [x, 0.0]];
        assert!(first_stray(&square(1.0), &square(0.0), tolerance()).is_some());
        assert!(first_stray(&square(0.05), &square(0.0), tolerance()).is_none());
    }

    #[test]
    fn a_missing_or_extra_short_move_is_found() {
        // Corners alone would match: all four lie on both paths.
        let sq = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]];
        let open = &sq[..4];
        assert!(first_stray(&sq, open, tolerance()).is_some(), "the closing move is missing");
        let cross = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0]];
        assert!(first_stray(&cross, &sq, tolerance()).is_some(), "a diagonal across the square isn't planned");
    }

    #[test]
    fn a_long_straight_cut_over_many_small_ones_passes_quickly() {
        let fine: Vec<P> = (0..=30000).map(|k| [k as f64 * 0.1, 0.0]).collect();
        assert!(first_stray(&[[0.0, 0.0], [3000.0, 0.0]], &fine, tolerance()).is_none());
        assert!(first_stray(&fine, &[[0.0, 0.0], [3000.0, 0.0]], tolerance()).is_none());
    }

    #[test]
    fn the_wrong_compensation_or_direction_is_found() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0], [0.0, 0.0]];
        let e = Cut { pts: sq.clone(), comp: Some("G41") };
        assert_eq!(compare(&e.clone(), &e, tolerance()), None);
        assert!(compare(&Cut { pts: sq.clone(), comp: Some("G42") }, &e, tolerance()).unwrap().contains("compensation"));
        let back: Vec<P> = sq.iter().rev().copied().collect();
        assert!(compare(&Cut { pts: back, comp: Some("G41") }, &e, tolerance()).unwrap().contains("other way round"));
        // A figure-eight encloses nothing, either way round.
        let eight = vec![[0.0, 0.0], [100.0, 100.0], [100.0, 0.0], [0.0, 100.0], [0.0, 0.0], [-100.0, 100.0], [-100.0, 0.0], [0.0, 100.0], [0.0, 0.0]];
        let e8 = Cut { pts: eight.clone(), comp: None };
        assert_eq!(compare(&e8.clone(), &e8, tolerance()), None);
        let back8: Vec<P> = eight.iter().rev().copied().collect();
        assert!(compare(&Cut { pts: back8, comp: None }, &e8, tolerance()).unwrap().contains("other way round"));
    }

    #[test]
    fn a_part_called_in_place_of_a_cut_off_line_is_found() {
        // BL39: the counts balance, but that call isn't the planned line.
        let (parts, placed, plan, _) = job();
        let line = CutLine { pts: vec![[0.0, 300.0], [1000.0, 300.0]], kind: CutKind::Remnant };
        let cuts = vec![line.clone()];
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        assert_eq!(check_set(&set, &parts, &placed, &plan, &cuts), Vec::<String>::new());
        let mut bad = set;
        let main = bad.spf.iter_mut().find(|f| !f.0.contains("70") && !f.0.contains("LIS")).unwrap();
        let part_call = main.1.lines().find(|l| l.contains("CALL")).unwrap().to_string();
        let last_call = main.1.lines().filter(|l| l.contains("CALL")).last().unwrap().to_string();
        main.1 = main.1.replace(&last_call, &part_call);
        let out = check_set(&bad, &parts, &placed, &plan, &cuts);
        assert!(out.iter().any(|m| m.contains("cut-off")), "{out:?}");
    }

    #[test]
    fn an_s_shaped_mark_with_nearly_equal_lobes_passes() {
        // Arcs and chords enclose a little more or less: no sign of direction.
        let lobe = |r: f64, cx: f64, up: bool| -> Vec<P> {
            (0..=32).map(|k| {
                let a = std::f64::consts::PI * (1.0 - k as f64 / 32.0);
                [cx + r * a.cos(), if up { r * a.sin() } else { -r * a.sin() }]
            }).collect()
        };
        let mut planned = lobe(10.0, 10.0, true);
        planned.extend(lobe(10.002, 30.002, false).into_iter().skip(1));
        let w = Cut { pts: planned.iter().map(|p| [p[0], p[1] + 0.001]).collect(), comp: None };
        assert_eq!(compare(&w, &Cut { pts: planned, comp: None }, tolerance()), None);
    }

    #[test]
    fn a_cut_off_line_written_with_compensation_is_found() {
        let (parts, placed, plan, _) = job();
        let cuts = vec![CutLine { pts: vec![[0.0, 300.0], [1000.0, 300.0]], kind: CutKind::Remnant }];
        let mut set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        let file = set.spf.iter_mut().find(|f| f.1.contains("HD_DWN_OFF")).unwrap();
        file.1 = file.1.replace("G01 G40", "G01 G41");
        let out = check_set(&set, &parts, &placed, &plan, &cuts);
        assert!(out.iter().any(|m| m.contains("compensation on")), "{out:?}");
    }

    #[test]
    fn a_cut_line_with_no_length_is_not_expected() {
        let (parts, placed, plan, _) = job();
        let cuts = vec![CutLine { pts: vec![[900.0, 400.0], [900.0, 400.0]], kind: CutKind::Free }];
        let set = crate::blm::sheet_set(1, &parts, &placed, &spec(), &plan, &cuts, &crate::blm::BlmSettings::for_spec(&spec(), "LC5", "J"));
        assert_eq!(check_set(&set, &parts, &placed, &plan, &cuts), Vec::<String>::new());
    }

    #[test]
    fn punched_holes_are_refused() {
        let (parts, placed, mut plan, set) = job();
        plan.contours[0].punch = Some(crate::machine::Punch { tool: None, hits: vec![] });
        let out = check_set(&set, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("punched holes")), "{out:?}");
    }

    #[test]
    fn planned_micro_joints_are_refused_until_the_files_can_carry_them() {
        // MAC-30: they'd be cut free with no warning otherwise.
        let (parts, placed, mut plan, set) = job();
        plan.contours[0].joints.push((10.0, 0.5));
        let out = check_set(&set, &parts, &placed, &plan, &[]);
        assert!(out.iter().any(|m| m.contains("DEMO-001") && m.contains("micro-joints")), "{out:?}");
    }

    #[test]
    fn unreadable_lines_and_arcs_off_their_circle_are_reported() {
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nG01 Xabc Y1\r\n").is_err());
        assert!(written_paths("G01 X1 Y1\r\n").is_err());
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nG03 X10 Y0 I1 J0\r\n").is_err());
        assert!(written_paths("NEXT_L(0,0,0,2,3)\r\nLASTLINE(0,0,0,1,5,5,6,6,0,0)\r\n").is_err());
        let p = written_paths("NEXT_L(0,0,0,2,3)\r\nG01 X10\r\nG03 X20 Y0 I5 J0\r\nLASTLINE(0,0,0,1,20,0,30,0,0,0)\r\n").unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].pts.last(), Some(&[30.0, 0.0]));
        // Counter-clockwise from left to right runs under the centre.
        assert!(p[0].pts.iter().any(|q| (q[1] + 5.0).abs() < 0.01), "{:?}", &p[0].pts[..4]);
    }
}
