//! Turning DWG entities into the points of a `dxf::Drawing` (DW6–DW12): object
//! coordinate systems, block inserts, ellipses and splines.

use super::objects::{Kind, Obj, Spline, V3};
use crate::dxf::{arc_segments, Drawing, Piece, ARC_CHORD_ERR};
use crate::nest::P;
use std::collections::HashMap;
use std::f64::consts::TAU;

/// 3D affine transform: columns x, y, z and the origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat {
    pub x: V3,
    pub y: V3,
    pub z: V3,
    pub o: V3,
}

impl Mat {
    pub const ID: Mat = Mat { x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], z: [0.0, 0.0, 1.0], o: [0.0; 3] };

    pub fn apply(&self, p: V3) -> V3 {
        let mut r = self.o;
        for i in 0..3 {
            r[i] += self.x[i] * p[0] + self.y[i] * p[1] + self.z[i] * p[2];
        }
        r
    }

    pub fn dir(&self, v: V3) -> V3 {
        let mut r = [0.0; 3];
        for i in 0..3 {
            r[i] = self.x[i] * v[0] + self.y[i] * v[1] + self.z[i] * v[2];
        }
        r
    }

    /// self ∘ other (apply `other` first).
    pub fn then(&self, other: &Mat) -> Mat {
        Mat { x: self.dir(other.x), y: self.dir(other.y), z: self.dir(other.z), o: self.apply(other.o) }
    }

    /// Largest stretch in the drawing plane, used so arcs stay within the chord error
    /// after scaling.
    pub fn scale(&self) -> f64 {
        let a = (self.x[0].powi(2) + self.x[1].powi(2)).sqrt();
        let b = (self.y[0].powi(2) + self.y[1].powi(2)).sqrt();
        a.max(b).max(1e-9)
    }

    /// Object coordinate system of an extrusion direction (the arbitrary-axis rule):
    /// 0,0,-1 mirrors X (DW7).
    pub fn ocs(n: V3) -> Mat {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !(len > 1e-12) || (n[0].abs() < 1e-12 && n[1].abs() < 1e-12 && n[2] > 0.0) {
            return Mat::ID;
        }
        let z = [n[0] / len, n[1] / len, n[2] / len];
        let ax = if z[0].abs() < 1.0 / 64.0 && z[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], z) } else { cross([0.0, 0.0, 1.0], z) };
        let x = norm(ax);
        let y = norm(cross(z, x));
        Mat { x, y, z, o: [0.0; 3] }
    }
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: V3) -> V3 {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-15 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        a
    }
}

fn xy(p: V3) -> P {
    [p[0], p[1]]
}

/// Points of an arc in its own plane (x, y at height z), with enough segments that
/// the chord error stays within ARC_CHORD_ERR after a stretch of `scale`.
pub fn arc_pts(c: V3, r: f64, a0: f64, sweep: f64, scale: f64) -> Vec<V3> {
    let n = arc_segments(r * scale, sweep);
    (0..=n)
        .map(|i| {
            let a = a0 + sweep * i as f64 / n as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin(), c[2]]
        })
        .collect()
}

/// Polyline vertices with bulges (in the polyline's plane) to points; stops
/// once it has more than `limit` points (the caller then refuses the drawing).
pub fn bulge_poly(verts: &[([f64; 2], f64)], closed: bool, z: f64, scale: f64, limit: usize) -> Vec<V3> {
    let n = verts.len();
    if n == 0 {
        return vec![];
    }
    let segs = if closed { n } else { n - 1 };
    let mut pts = vec![[verts[0].0[0], verts[0].0[1], z]];
    for s in 0..segs {
        if pts.len() > limit {
            // Cut short: more than `limit` points go back as they are (see spline_pts).
            return pts;
        }
        let (p1, b) = verts[s];
        let p2 = verts[(s + 1) % n].0;
        let dx = p2[0] - p1[0];
        let dy = p2[1] - p1[1];
        let len = (dx * dx + dy * dy).sqrt();
        if b.abs() < 1e-9 || len < 1e-12 || !b.is_finite() {
            pts.push([p2[0], p2[1], z]);
            continue;
        }
        let theta = 4.0 * b.atan();
        let mid = [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0];
        let d = (len / 2.0) / (theta / 2.0).tan();
        let c = [mid[0] - dy / len * d, mid[1] + dx / len * d, z];
        let r = ((p1[0] - c[0]).powi(2) + (p1[1] - c[1]).powi(2)).sqrt();
        let a0 = (p1[1] - c[1]).atan2(p1[0] - c[0]);
        let mut a = arc_pts(c, r, a0, theta, scale);
        let last = a.len() - 1;
        a[last] = [p2[0], p2[1], z];
        pts.extend(a.into_iter().skip(1));
    }
    if closed && pts.len() > 1 {
        pts.pop();
    }
    pts
}

/// A counter-clockwise sweep from the end angle minus the start angle: a sweep
/// of `min` or less goes once more round (0 = a full turn). Any size of input
/// takes the same few steps (a damaged file's huge angle can't keep it turning);
/// `None` when it isn't a number.
pub fn ccw_sweep(sweep: f64, min: f64) -> Option<f64> {
    if !sweep.is_finite() {
        return None;
    }
    if sweep > min {
        return Some(sweep);
    }
    let s = sweep.rem_euclid(TAU);
    Some(if s <= min { s + TAU } else { s })
}

/// Points along an ellipse given in world coordinates (DW9).
pub fn ellipse_pts(c: V3, major: V3, ext: V3, ratio: f64, t0: f64, t1: f64, scale: f64) -> (Vec<V3>, bool) {
    let n = norm(ext);
    let minor = {
        let m = cross(n, major);
        [m[0] * ratio, m[1] * ratio, m[2] * ratio]
    };
    let Some(sweep) = ccw_sweep(t1 - t0, 1e-9).filter(|_| t0.is_finite()) else { return (vec![], false) };
    let full = (sweep - TAU).abs() < 1e-6 || sweep > TAU;
    let sweep = sweep.min(TAU);
    let rmaj = (major[0].powi(2) + major[1].powi(2) + major[2].powi(2)).sqrt();
    // Segment count from the sharpest bend (radius b²/a at the major axis ends).
    let rmin = rmaj * ratio.abs().max(1e-6);
    let tight = (rmin * rmin / rmaj.max(1e-12)).max(1e-6);
    let mut k = arc_segments(tight * scale, sweep).max(arc_segments(rmaj * scale, sweep));
    k = k.clamp(8, 2000);
    let mut pts: Vec<V3> = (0..=k)
        .map(|i| {
            let t = t0 + sweep * i as f64 / k as f64;
            let (s, co) = t.sin_cos();
            [c[0] + major[0] * co + minor[0] * s, c[1] + major[1] * co + minor[1] * s, c[2] + major[2] * co + minor[2] * s]
        })
        .collect();
    if full {
        pts.pop();
    }
    (pts, full)
}

/// A point on a NURBS curve by de Boor's algorithm.
fn nurbs_at(s: &Spline, t: f64) -> V3 {
    let p = s.degree;
    let n = s.ctrl.len();
    let k = &s.knots;
    // Knot span by binary search (the knots never go down): sampling a long
    // spline stays linear in its length.
    let span = (p + k[p + 1..n].partition_point(|&x| x <= t)).min(n - 1);
    let mut d: Vec<[f64; 4]> = (0..=p)
        .map(|j| {
            let i = span + j - p;
            // Weights are positive (spline_ok) and scaled so the largest is 1:
            // used as they are, their ratios shape the curve.
            let w = s.weights.get(i).copied().unwrap_or(1.0);
            let c = s.ctrl[i];
            [c[0] * w, c[1] * w, c[2] * w, w]
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = span + j - p;
            let den = k[i + p + 1 - r] - k[i];
            let a = if den.abs() < 1e-15 { 0.0 } else { (t - k[i]) / den };
            for q in 0..4 {
                d[j][q] = (1.0 - a) * d[j - 1][q] + a * d[j][q];
            }
        }
    }
    let w = d[p][3];
    [d[p][0] / w, d[p][1] / w, d[p][2] / w]
}

fn dist_to_seg(p: V3, a: V3, b: V3) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((ap[0] - ab[0] * t).powi(2) + (ap[1] - ab[1] * t).powi(2)).sqrt()
}

/// Sample `f` on [t0, t1], splitting until the curve is within `tol` of its
/// chords; stops adding once `out` has more than `limit` points.
fn adaptive(f: &dyn Fn(f64) -> V3, t0: f64, t1: f64, tol: f64, out: &mut Vec<V3>, depth: u32, limit: usize) {
    if out.len() > limit {
        return;
    }
    let a = f(t0);
    let b = f(t1);
    let tm = (t0 + t1) / 2.0;
    let m = f(tm);
    let q1 = f(t0 + (t1 - t0) * 0.25);
    let q3 = f(t0 + (t1 - t0) * 0.75);
    let err = dist_to_seg(m, a, b).max(dist_to_seg(q1, a, b)).max(dist_to_seg(q3, a, b));
    if depth < 14 && err > tol {
        adaptive(f, t0, tm, tol, out, depth + 1, limit);
        adaptive(f, tm, t1, tol, out, depth + 1, limit);
    } else {
        out.push(b);
    }
}

/// Points along a spline (DW10): control-point splines are evaluated exactly;
/// splines stored only as fit points get a smooth cubic through the points.
/// A spline that can be drawn: fit points (two or more), or control points with
/// a degree below their count, one knot more than control points plus degree,
/// knots that never go down, and one weight per control point.
pub fn spline_ok(s: &Spline) -> bool {
    let n = s.ctrl.len();
    if n == 0 {
        return s.fit.len() >= 2;
    }
    n >= 2
        && s.degree >= 1
        && s.degree < n
        && s.knots.len() == n + s.degree + 1
        && s.knots[n] > s.knots[s.degree]
        && s.knots.windows(2).all(|w| w[1] >= w[0] - 1e-9 * (s.knots[n] - s.knots[s.degree]))
        && s.weights.len() == n
        && s.weights.iter().all(|w| *w > 0.0 && w.is_finite())
}

/// A spline's points within the chord error; stops once it has more than
/// `limit` points (the caller then refuses the drawing).
pub fn spline_pts(s: &Spline, scale: f64, limit: usize) -> (Vec<V3>, bool) {
    let tol = ARC_CHORD_ERR / scale.max(1e-9);
    let mut out = vec![];
    let n = s.ctrl.len();
    if n >= 2 && s.knots.len() == n + s.degree + 1 && s.degree < n && s.knots[n] > s.knots[s.degree] {
        // Knots scaled to 0..1 over the drawn part, so the tolerances below are
        // relative: a spline with tiny knot values draws like any other.
        let (k0, k1) = (s.knots[s.degree], s.knots[n]);
        // Weights scaled the same way (only their ratios matter).
        let wmax = s.weights.iter().copied().fold(0.0, f64::max);
        let weights = if wmax > 0.0 { s.weights.iter().map(|w| w / wmax).collect() } else { s.weights.clone() };
        let s = &Spline { knots: s.knots.iter().map(|k| (k - k0) / (k1 - k0)).collect(), weights, ..s.clone() };
        let (lo, hi) = (0.0, 1.0);
        if hi > lo && s.knots.windows(2).all(|w| w[1] >= w[0] - 1e-12) {
            let f = |t: f64| nurbs_at(s, t);
            out.push(f(lo));
            let mut prev = lo;
            for &k in &s.knots[s.degree + 1..=n] {
                if out.len() > limit {
                    break;
                }
                // Every span with any length at all is drawn, however narrow.
                if k > prev {
                    // Split each knot span in four first so S-bends are not missed.
                    for q in 0..4 {
                        let a = prev + (k - prev) * q as f64 / 4.0;
                        let b = prev + (k - prev) * (q + 1) as f64 / 4.0;
                        adaptive(&f, a, b, tol, &mut out, 0, limit);
                    }
                    prev = k;
                }
            }
        }
    }
    if out.is_empty() {
        let pts = if s.fit.len() >= 2 { &s.fit } else { &s.ctrl };
        if pts.len() < 2 {
            return (vec![], false);
        }
        if s.fit.len() >= 3 {
            out = fit_cubic(&s.fit, s.beg_tan, s.end_tan, tol, limit);
        } else {
            out = pts.clone();
        }
    }
    // Cut short at the limit: hand back more than `limit` points as they are, so
    // the caller refuses the drawing (dropping the closing point could hide it).
    if out.len() > limit {
        return (out, false);
    }
    let closed = out.len() > 2 && {
        let (a, b) = (out[0], out[out.len() - 1]);
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 1e-6
    };
    if closed {
        out.pop();
    }
    let closed = closed || (s.closed && out.len() > 2);
    (out, closed)
}

/// Cubic through fit points, chord-length parameters, with the given end tangents
/// (or natural ends when they are zero).
fn fit_cubic(p: &[V3], t0: V3, t1: V3, tol: f64, limit: usize) -> Vec<V3> {
    let n = p.len();
    let mut u = vec![0.0; n];
    for i in 1..n {
        let d = ((p[i][0] - p[i - 1][0]).powi(2) + (p[i][1] - p[i - 1][1]).powi(2) + (p[i][2] - p[i - 1][2]).powi(2)).sqrt();
        u[i] = u[i - 1] + d.max(1e-9);
    }
    let len = u[n - 1];
    let has = |t: V3| t.iter().any(|v| v.abs() > 1e-12);
    // Second derivatives per axis (tridiagonal solve).
    let mut m = vec![[0.0; 3]; n];
    for ax in 0..3 {
        let y: Vec<f64> = p.iter().map(|q| q[ax]).collect();
        let mut a = vec![0.0; n];
        let mut b = vec![0.0; n];
        let mut c = vec![0.0; n];
        let mut d = vec![0.0; n];
        for i in 1..n - 1 {
            let h0 = u[i] - u[i - 1];
            let h1 = u[i + 1] - u[i];
            a[i] = h0;
            b[i] = 2.0 * (h0 + h1);
            c[i] = h1;
            d[i] = 6.0 * ((y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0);
        }
        let h0 = u[1] - u[0];
        if has(t0) {
            let tl = (t0[0] * t0[0] + t0[1] * t0[1] + t0[2] * t0[2]).sqrt();
            b[0] = 2.0 * h0;
            c[0] = h0;
            d[0] = 6.0 * ((y[1] - y[0]) / h0 - t0[ax] / tl);
        } else {
            b[0] = 1.0;
        }
        let hn = u[n - 1] - u[n - 2];
        if has(t1) {
            let tl = (t1[0] * t1[0] + t1[1] * t1[1] + t1[2] * t1[2]).sqrt();
            a[n - 1] = hn;
            b[n - 1] = 2.0 * hn;
            d[n - 1] = 6.0 * (t1[ax] / tl - (y[n - 1] - y[n - 2]) / hn);
        } else {
            b[n - 1] = 1.0;
        }
        // Thomas algorithm.
        for i in 1..n {
            let w = a[i] / b[i - 1];
            b[i] -= w * c[i - 1];
            d[i] -= w * d[i - 1];
        }
        let mut x = vec![0.0; n];
        x[n - 1] = d[n - 1] / b[n - 1];
        for i in (0..n - 1).rev() {
            x[i] = (d[i] - c[i] * x[i + 1]) / b[i];
        }
        for i in 0..n {
            m[i][ax] = if x[i].is_finite() { x[i] } else { 0.0 };
        }
    }
    let f = |t: f64| -> V3 {
        let t = t.clamp(0.0, len);
        // The interval by binary search (u never goes down): linear in the
        // number of fit points overall.
        let i = u[1..n - 1].partition_point(|&x| x < t).min(n - 2);
        let h = u[i + 1] - u[i];
        let a = (u[i + 1] - t) / h;
        let b = (t - u[i]) / h;
        let mut r = [0.0; 3];
        for ax in 0..3 {
            r[ax] = a * p[i][ax] + b * p[i + 1][ax] + ((a * a * a - a) * m[i][ax] + (b * b * b - b) * m[i + 1][ax]) * h * h / 6.0;
        }
        r
    };
    let mut out = vec![p[0]];
    for i in 0..n - 1 {
        if out.len() > limit {
            break;
        }
        adaptive(&f, u[i], u[i + 1], tol, &mut out, 0, limit);
    }
    out
}

/// Clean MTEXT/TEXT codes to plain text (DW11).
pub fn plain_text(raw: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        match ch {
            '\\' if i + 1 < cs.len() => {
                let c = cs[i + 1];
                i += 2;
                match c {
                    'P' | 'X' | '~' => out.push(' '),
                    '\\' | '{' | '}' => out.push(c),
                    'U' if cs.get(i) == Some(&'+') => {
                        let hex: String = cs.iter().skip(i + 1).take(4).collect();
                        if let Some(u) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            out.push(u);
                            i += 5;
                        }
                    }
                    'S' => {
                        // stacked text a^b or a/b: keep as a/b
                        while i < cs.len() && cs[i] != ';' {
                            out.push(if cs[i] == '^' || cs[i] == '#' { '/' } else { cs[i] });
                            i += 1;
                        }
                        i += 1;
                    }
                    'A' | 'C' | 'c' | 'F' | 'f' | 'H' | 'h' | 'Q' | 'q' | 'T' | 't' | 'W' | 'w' | 'p' => {
                        while i < cs.len() && cs[i] != ';' {
                            i += 1;
                        }
                        i += 1;
                    }
                    _ => {} // \L \l \O \o \K \k and others: formatting only
                }
                continue;
            }
            '{' | '}' => {}
            '%' if cs.get(i + 1) == Some(&'%') && i + 2 < cs.len() => {
                match cs[i + 2].to_ascii_lowercase() {
                    'c' => out.push('Ø'),
                    'd' => out.push('°'),
                    'p' => out.push('±'),
                    '%' => out.push('%'),
                    _ => {}
                }
                i += 3;
                continue;
            }
            c => out.push(c),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Default)]
pub struct Db {
    pub objs: HashMap<u64, Obj>,
    pub order: Vec<u64>,
    pub layers: HashMap<u64, String>,
    /// Block definition handle → (name, base point).
    pub blocks: HashMap<u64, (String, V3)>,
    /// Owner handle → owned entity handles, in file order.
    pub owned: HashMap<u64, Vec<u64>>,
    /// Layer name (upper case) → its state in the drawing (frozen, off, line type), DX3.
    /// Only the DXF reader fills it; a layer not listed is on, in a solid line.
    pub layer_info: HashMap<String, LayerInfo>,
    /// Object handle → its own line type, when it isn't BYLAYER (DX3).
    pub ltypes: HashMap<u64, String>,
    /// Line types (upper case) drawn with dashes or dots, from the drawing's
    /// line type table; `None` when it has none (every type but CONTINUOUS
    /// then counts as broken).
    pub broken: Option<std::collections::HashSet<String>>,
}

/// A layer as the drawing's layer table has it (DX3).
#[derive(Default, Clone, Debug)]
pub struct LayerInfo {
    pub frozen: bool,
    pub off: bool,
    pub ltype: String,
}

/// Why the objects of a layer row start as "Leave out" (DX3): the drawing
/// hides them, or draws them in a centre / hidden / dashed line type. Ours.
pub fn hidden_reason(frozen: bool, off: bool, invisible: bool, ltype: Option<&str>) -> Vec<String> {
    let mut tags = vec![];
    if let Some(t) = ltype {
        tags.push(t.to_string());
    }
    if frozen {
        tags.push("frozen".into());
    } else if off {
        tags.push("off".into());
    }
    if invisible {
        tags.push("invisible".into());
    }
    tags
}

pub struct Ctx<'a> {
    pub db: &'a Db,
    pub d: Drawing,
    pub explode: bool,
    pub skip_invisible: bool,
    pub points: usize,
    pub visits: usize,
    pub depth_hit: bool,
    /// The drawing went over the point, object or block-grid limit: geometry
    /// was left out, so the import must be refused, not finished short.
    pub over: bool,
    skip_seen: std::collections::HashSet<String>,
    /// Bytes of text made so far (texts in blocks are made once per insert).
    text_bytes: usize,
    /// Inside a block insert: the insert's line type (for BYBLOCK objects),
    /// and whether the insert is on a frozen layer or invisible, which hides
    /// all it holds (DX3).
    in_ltype: Option<String>,
    in_frozen: bool,
    in_invisible: bool,
}

/// Most bytes of text and layer names one drawing may make.
const MAX_TEXT_BYTES: usize = 200 << 20;

/// A layer name as AutoCAD allows it: at most 255 characters.
fn short_name(n: &str) -> String {
    n.chars().take(255).collect()
}

impl<'a> Ctx<'a> {
    pub fn new(db: &'a Db, explode: bool, skip_invisible: bool) -> Ctx<'a> {
        Ctx { db, d: Drawing::default(), explode, skip_invisible, points: 0, visits: 0, depth_hit: false, over: false, skip_seen: Default::default(), text_bytes: 0, in_ltype: None, in_frozen: false, in_invisible: false }
    }

    /// Why the drawing can't be imported whole, if it can't.
    pub fn refusal(&self) -> Option<String> {
        self.over.then(|| {
            format!(
                "This drawing is bigger than the import can take whole (more than {} million points or {} million objects, blocks nested more than 16 deep, a block grid of more than 10 000 copies, or more than 200 MB of text and layer names). \
                 Nothing was imported, so no contour goes missing. Simplify it (fewer spline or arc points, explode only what's needed) or split it, then import again.",
                MAX_POINTS / 1_000_000,
                MAX_VISITS / 1_000_000
            )
        })
    }
}

const MAX_DEPTH: u32 = 16;
const MAX_POINTS: usize = 5_000_000;
const MAX_VISITS: usize = 2_000_000;

impl<'a> Ctx<'a> {
    fn layer_name(&mut self, o: &Obj, inherit: Option<&str>) -> String {
        let own = o.layer.and_then(|h| self.db.layers.get(&h)).map(|n| short_name(n)).unwrap_or_else(|| "0".to_string());
        let name = match inherit {
            // DW8: objects on layer 0 inside a block take the insert's layer.
            Some(l) if own == "0" => l.to_string(),
            _ => own,
        };
        // Every piece keeps its layer's name: their bytes count toward a budget,
        // so long names copied through many inserts can't fill memory (DW15).
        self.text_bytes = self.text_bytes.saturating_add(name.len());
        if self.text_bytes > MAX_TEXT_BYTES {
            self.over = true;
        }
        name
    }

    /// The object's line type in force: its own, its layer's (BYLAYER, the
    /// default) or the enclosing insert's (BYBLOCK), upper case.
    fn ltype_of(&self, o: &Obj, layer: &str) -> String {
        let own = self.db.ltypes.get(&o.handle).map(|t| t.to_ascii_uppercase());
        match own.as_deref() {
            None | Some("BYLAYER") | Some("") => self.info(layer).map(|l| l.ltype.to_ascii_uppercase()).unwrap_or_default(),
            Some("BYBLOCK") => self.in_ltype.clone().unwrap_or_default(),
            Some(t) => t.to_string(),
        }
    }

    fn info(&self, layer: &str) -> Option<&LayerInfo> {
        if self.db.layer_info.is_empty() {
            return None;
        }
        self.db.layer_info.get(&layer.to_uppercase())
    }

    /// Whether a line type is drawn with dashes or dots (a centre line, a
    /// hidden line…): such objects are drafting, not cut (DX3).
    fn broken(&self, t: &str) -> bool {
        if t.is_empty() || t == "CONTINUOUS" || t == "BYLAYER" || t == "BYBLOCK" {
            return false;
        }
        self.db.broken.as_ref().is_none_or(|b| b.contains(t))
    }

    /// The layer row an object goes on (DX3): its layer, or "<layer> [CENTER]",
    /// "<layer> [frozen]", "<layer> [CENTER/invisible]"… for objects the drawing hides
    /// or draws as drafting lines. Those rows start as "Leave out".
    fn row(&mut self, o: &Obj, layer: &str) -> String {
        let info = self.info(layer).cloned().unwrap_or_default();
        let t = self.ltype_of(o, layer);
        let lt = self.broken(&t).then_some(t.as_str());
        let tags = hidden_reason(info.frozen || self.in_frozen, info.off, o.invisible || self.in_invisible, lt);
        if tags.is_empty() {
            return layer.to_string();
        }
        // Joined by "/" (agents list layers split by commas).
        let row = format!("{layer} [{}]", tags.join("/"));
        if !self.d.hidden.contains_key(&row) {
            let mut why = vec![];
            if let Some(t) = lt {
                why.push(format!("drawn in the {t} line type"));
            }
            if info.frozen || self.in_frozen {
                why.push("layer frozen in the drawing".into());
            } else if info.off {
                why.push("layer turned off in the drawing".into());
            }
            if o.invisible || self.in_invisible {
                why.push("invisible in the drawing".into());
            }
            self.d.hidden.insert(row.clone(), why.join(", "));
        }
        row
    }

    /// How many more points the drawing may have.
    fn room(&self) -> usize {
        MAX_POINTS.saturating_sub(self.points)
    }

    fn push(&mut self, layer: String, pts: Vec<V3>, closed: bool, m: &Mat) {
        if self.points.saturating_add(pts.len()) > MAX_POINTS {
            self.over = true;
            return;
        }
        if pts.len() < 2 {
            return;
        }
        self.points += pts.len();
        let pts: Vec<P> = pts.into_iter().map(|p| xy(m.apply(p))).collect();
        if pts.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return;
        }
        self.d.pieces.push(Piece { layer, pts, closed });
    }

    /// Name a kind of object left out, once. Past 50 kinds the rest are only
    /// counted (a file full of made-up object names can't slow the import).
    fn skipped(&mut self, name: &str) {
        if self.skip_seen.contains(name) {
            return;
        }
        self.skip_seen.insert(name.to_string());
        if self.skip_seen.len() <= 50 {
            self.d.report.skipped.push(name.to_string());
        } else if self.skip_seen.len() == 51 {
            self.d.report.skipped.push("other kinds of objects".into());
        }
    }

    /// Vertices of a polyline in order.
    /// A polyline's vertices in order; `None` when one it names isn't in the
    /// file (or isn't a vertex), so the polyline is damaged (DW5) — never drawn
    /// with a corner missing.
    fn vertices(&mut self, o: &Obj) -> Option<Vec<&'a Obj>> {
        let Kind::Poly { owned, first, last, .. } = &o.kind else { return Some(vec![]) };
        let db: &'a Db = self.db;
        let is_vertex = |h: &u64| db.objs.get(h).filter(|v| matches!(v.kind, Kind::Vertex { .. }));
        if !owned.is_empty() {
            // Listed vertices count toward the work budget too (inserts repeat them).
            self.visits = self.visits.saturating_add(owned.len());
            if self.visits > MAX_VISITS {
                self.over = true;
                return None;
            }
            return owned.iter().map(is_vertex).collect();
        }
        let mut list: Vec<u64> = vec![];
        {
            if let (Some(f), Some(l)) = (first, last) {
                // Each step counts toward the drawing's work budget, and a chain
                // that comes back to a vertex it passed is damaged at once.
                let mut h = *f;
                let mut seen = std::collections::HashSet::new();
                loop {
                    if !seen.insert(h) {
                        return None;
                    }
                    list.push(h);
                    self.visits += 1;
                    if h == *l {
                        break;
                    }
                    if self.visits > MAX_VISITS {
                        self.over = true;
                        return None;
                    }
                    match db.objs.get(&h).and_then(|v| v.next) {
                        Some(n) if db.objs.get(&n).is_some_and(|v| matches!(v.kind, Kind::Vertex { .. })) => h = n,
                        _ => break,
                    }
                }
                // A chain that doesn't reach its last vertex has one missing.
                if list.last() != Some(l) {
                    return None;
                }
                return list.iter().map(is_vertex).collect();
            }
        }
        // Neither list nor chain: the vertices the file files under the polyline.
        list = db.owned.get(&o.handle).cloned().unwrap_or_default();
        self.visits = self.visits.saturating_add(list.len());
        if self.visits > MAX_VISITS {
            self.over = true;
            return None;
        }
        Some(list.iter().filter_map(is_vertex).collect())
    }

    pub fn entity(&mut self, o: &Obj, m: &Mat, inherit: Option<&str>, depth: u32) {
        // A drawing whose blocks insert each other many times over stops here.
        if self.over {
            return;
        }
        self.visits += 1;
        if self.visits > MAX_VISITS {
            self.over = true;
            return;
        }
        if self.skip_invisible && o.invisible {
            return;
        }
        let lname = self.layer_name(o, inherit);
        let layer = self.row(o, &lname);
        let sc = m.scale();
        let mut counted = true;
        match &o.kind {
            Kind::Line { a, b } => self.push(layer, vec![*a, *b], false, m),
            Kind::Circle { c, r, ext } => {
                if *r > 0.0 && r.is_finite() {
                    let mut p = arc_pts(*c, *r, 0.0, TAU, sc);
                    p.pop();
                    self.push(layer, p, true, &m.then(&Mat::ocs(*ext)));
                } else {
                    // DW5: a circle with no radius is damaged: named, not drawn.
                    self.skipped("damaged CIRCLE");
                }
            }
            Kind::Arc { c, r, a0, a1, ext } => {
                let sweep = ccw_sweep(a1 - a0, 0.0).filter(|_| a0.is_finite());
                if let (true, Some(sweep)) = (*r > 0.0 && r.is_finite(), sweep) {
                    self.push(layer, arc_pts(*c, *r, *a0, sweep.min(TAU), sc), false, &m.then(&Mat::ocs(*ext)));
                } else {
                    self.skipped("damaged ARC");
                }
            }
            Kind::LwPoly { closed, elev, ext, verts } => {
                let pts = bulge_poly(verts, *closed, *elev, sc, self.room());
                self.push(layer, pts, *closed, &m.then(&Mat::ocs(*ext)));
            }
            Kind::Poly { three_d, closed, spline_fit, elev, ext, .. } => {
                let Some(vs) = self.vertices(o) else {
                    self.skipped("damaged POLYLINE");
                    return;
                };
                let verts: Vec<([f64; 2], f64, f64)> = vs
                    .into_iter()
                    .filter_map(|v| match v.kind {
                        // DW12: on a smoothed polyline, the frame points are not drawn.
                        Kind::Vertex { flags, pt, bulge } if !(*spline_fit && flags & 16 != 0) => Some(([pt[0], pt[1]], bulge, pt[2])),
                        _ => None,
                    })
                    .collect();
                if *three_d {
                    let pts: Vec<V3> = verts.iter().map(|v| [v.0[0], v.0[1], v.2]).collect();
                    self.push(layer, pts, *closed && verts.len() > 2, m);
                } else {
                    let vb: Vec<([f64; 2], f64)> = verts.iter().map(|v| (v.0, v.1)).collect();
                    let pts = bulge_poly(&vb, *closed, *elev, sc, self.room());
                    self.push(layer, pts, *closed, &m.then(&Mat::ocs(*ext)));
                }
            }
            Kind::Ellipse { c, major, ext, ratio, t0, t1 } => {
                // An ellipse needs a major axis with length, a ratio above 0 and a
                // plane; otherwise it is damaged (named, DW5) rather than a few
                // points on top of each other that quietly vanish.
                let len = (major[0] * major[0] + major[1] * major[1] + major[2] * major[2]).sqrt();
                let plane = (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt();
                if !(len > 1e-9 && len.is_finite() && *ratio > 1e-9 && ratio.is_finite() && plane > 1e-12 && plane.is_finite()) {
                    self.skipped("damaged ELLIPSE");
                } else {
                    let (pts, full) = ellipse_pts(*c, *major, *ext, *ratio, *t0, *t1, sc);
                    self.push(layer, pts, full, m);
                }
            }
            Kind::Spline(s) => {
                if !spline_ok(s) {
                    // DW10: a spline whose knots, weights or degree don't fit its
                    // control points is damaged: named, never drawn as its control
                    // polygon.
                    self.skipped("damaged SPLINE");
                } else {
                    let (pts, closed) = spline_pts(s, sc, self.room());
                    if pts.len() < 2 {
                        self.skipped("damaged SPLINE");
                    } else {
                        self.push(layer, pts, closed, m);
                    }
                }
            }
            Kind::Text { pt, ext, rot, h, s } => {
                let mm = m.then(&Mat::ocs(*ext));
                self.text(layer, s, *pt, [rot.cos(), rot.sin(), 0.0], *h, &mm);
            }
            Kind::MText { pt, xdir, h, s } => {
                let d = if xdir.iter().all(|v| v.abs() < 1e-12) { [1.0, 0.0, 0.0] } else { *xdir };
                self.text(layer, s, *pt, d, *h, m);
            }
            Kind::Insert { block, pt, scale, rot, ext, cols, rows, dc, dr } => {
                counted = false;
                if !self.explode {
                    return;
                }
                if depth >= MAX_DEPTH {
                    // What lies deeper would be missing: refuse the drawing (DW8, DW15).
                    self.depth_hit = true;
                    self.over = true;
                    return;
                }
                // An insert of a block the file doesn't define (or an external
                // reference) would lose its contours: named in the notes. A block
                // that is defined but empty draws nothing, as it should.
                let Some((_, base)) = self.db.blocks.get(block).cloned() else {
                    self.skipped("block inserts whose block isn't in the file");
                    return;
                };
                let Some(list) = self.db.owned.get(block).cloned() else { return };
                let (sn, cs) = rot.sin_cos();
                let (cols, rows) = ((*cols).max(1) as usize, (*rows).max(1) as usize);
                // A grid bigger than the limit is refused whole, not cut short.
                if cols * rows > 10_000 {
                    self.over = true;
                    return;
                }
                // DX3: what the insert hides or draws in its line type, its contents do.
                let ins_lt = self.ltype_of(o, &lname);
                let saved = (self.in_ltype.replace(ins_lt), self.in_frozen, self.in_invisible);
                self.in_frozen |= self.info(&lname).is_some_and(|l| l.frozen);
                self.in_invisible |= o.invisible;
                for ri in 0..rows {
                    for ci in 0..cols {
                        // Over a limit: stop expanding at once (the drawing is refused).
                        if self.over {
                            return;
                        }
                        // DW8: insert point, rotation and scale, then the block's base point.
                        let off = [ci as f64 * dc, ri as f64 * dr];
                        let o2 = [pt[0] + cs * off[0] - sn * off[1], pt[1] + sn * off[0] + cs * off[1], pt[2]];
                        let local = Mat {
                            x: [cs * scale[0], sn * scale[0], 0.0],
                            y: [-sn * scale[1], cs * scale[1], 0.0],
                            z: [0.0, 0.0, scale[2]],
                            o: o2,
                        };
                        let shift = Mat { o: [-base[0], -base[1], -base[2]], ..Mat::ID };
                        let mm = m.then(&Mat::ocs(*ext)).then(&local).then(&shift);
                        for h in &list {
                            if self.over {
                                return;
                            }
                            if let Some(child) = self.db.objs.get(h) {
                                if matches!(child.kind, Kind::Vertex { .. } | Kind::BlockHeader { .. } | Kind::Layer { .. } | Kind::Other) {
                                    continue;
                                }
                                self.entity(child, &mm, Some(&lname), depth + 1);
                            }
                        }
                    }
                }
                (self.in_ltype, self.in_frozen, self.in_invisible) = saved;
            }
            Kind::Skip { name, annotation } => {
                counted = false;
                if !annotation {
                    let n = name.clone();
                    self.skipped(&n);
                }
            }
            Kind::Vertex { .. } | Kind::Layer { .. } | Kind::BlockHeader { .. } | Kind::Other => counted = false,
        }
        if counted {
            self.d.report.entities += 1;
        }
    }

    fn text(&mut self, layer: String, raw: &str, pt: V3, dir: V3, h: f64, m: &Mat) {
        // Texts copied through block inserts count toward a budget of their own:
        // past it the drawing is refused (DW15), before any copy is made.
        self.text_bytes = self.text_bytes.saturating_add(raw.len());
        if self.text_bytes > MAX_TEXT_BYTES {
            self.over = true;
            return;
        }
        let s = plain_text(raw);
        if s.is_empty() {
            return;
        }
        let p = m.apply(pt);
        let d = m.dir(dir);
        let up = m.dir([-dir[1], dir[0], 0.0]);
        let hs = (up[0] * up[0] + up[1] * up[1]).sqrt();
        let h = (h * if hs > 1e-9 { hs } else { 1.0 }).max(0.1);
        let ang = d[1].atan2(d[0]).to_degrees();
        if p[0].is_finite() && p[1].is_finite() {
            self.d.texts.push((layer, s, [p[0], p[1]], if h.is_finite() { h } else { 5.0 }, if ang.is_finite() { ang } else { 0.0 }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_polyline_cut_at_the_limit_stays_over_it() {
        // Cut short, the closing point isn't dropped, so the caller sees more
        // points than the limit and refuses (no triangle from a pentagon).
        let v: Vec<([f64; 2], f64)> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [5.0, 15.0], [0.0, 10.0]].iter().map(|p| (*p, 0.0)).collect();
        assert!(bulge_poly(&v, true, 0.0, 1.0, 3).len() > 3);
        assert_eq!(bulge_poly(&v, true, 0.0, 1.0, usize::MAX).len(), 5);
    }

    #[test]
    fn huge_or_endless_angles_end_at_once() {
        // A damaged file's arc angle: no endless turning, no geometry from nonsense.
        assert_eq!(ccw_sweep(f64::NEG_INFINITY, 0.0), None);
        assert_eq!(ccw_sweep(f64::NAN, 0.0), None);
        let s = ccw_sweep(-1e300, 0.0).unwrap();
        assert!(s > 0.0 && s <= TAU);
        assert_eq!(ccw_sweep(0.0, 0.0), Some(TAU));
        assert!((ccw_sweep(-TAU / 4.0, 0.0).unwrap() - 3.0 * TAU / 4.0).abs() < 1e-12);
        let (p, _) = ellipse_pts([0.0; 3], [20.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 0.0, -1e300, 1.0);
        assert!(p.len() <= 2001);
    }

    #[test]
    fn minus_z_extrusion_mirrors_x() {
        let m = Mat::ocs([0.0, 0.0, -1.0]);
        let p = m.apply([10.0, 5.0, 0.0]);
        assert!((p[0] + 10.0).abs() < 1e-12 && (p[1] - 5.0).abs() < 1e-12);
        assert_eq!(Mat::ocs([0.0, 0.0, 1.0]), Mat::ID);
    }

    #[test]
    fn arcs_and_bulges_stay_within_the_chord_error() {
        let pts = arc_pts([0.0, 0.0, 0.0], 50.0, 0.0, TAU / 4.0, 1.0);
        for w in pts.windows(2) {
            let mid = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
            assert!(50.0 - (mid[0].hypot(mid[1])) <= ARC_CHORD_ERR + 1e-9);
        }
        // Bulge 1 = half circle to the left of travel: (0,0)->(10,0) passes below
        // (counter-clockwise around (5,0)).
        let p = bulge_poly(&[([0.0, 0.0], 1.0), ([10.0, 0.0], 0.0)], false, 0.0, 1.0, usize::MAX);
        assert!(p.iter().any(|q| q[1] < -4.9));
        assert_eq!(p.last().unwrap()[..2], [10.0, 0.0]);
    }

    #[test]
    fn ellipse_full_and_partial() {
        let (p, full) = ellipse_pts([0.0; 3], [20.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 0.0, TAU, 1.0);
        assert!(full);
        assert!(p.iter().all(|q| ((q[0] / 20.0).powi(2) + (q[1] / 10.0).powi(2) - 1.0).abs() < 1e-9));
        assert!(p.iter().any(|q| (q[1] - 10.0).abs() < 0.2));
        let (p, full) = ellipse_pts([0.0; 3], [20.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 0.0, TAU / 4.0, 1.0);
        assert!(!full);
        assert!((p.last().unwrap()[1] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn spline_through_control_points_matches_a_quadratic() {
        // Clamped quadratic Bézier (0,0) (10,20) (20,0): peak at (10,10).
        let s = Spline { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], ctrl: vec![[0.0, 0.0, 0.0], [10.0, 20.0, 0.0], [20.0, 0.0, 0.0]], weights: vec![1.0; 3], ..Default::default() };
        let (p, closed) = spline_pts(&s, 1.0, usize::MAX);
        assert!(!closed);
        assert_eq!(p[0], [0.0, 0.0, 0.0]);
        assert!((p.last().unwrap()[0] - 20.0).abs() < 1e-9);
        let top = p.iter().map(|q| q[1]).fold(0.0, f64::max);
        assert!((top - 10.0).abs() < 0.1);
        // Rational quadratic with w = cos 45° on the middle point is a quarter circle.
        let s = Spline { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], ctrl: vec![[10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]], weights: vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0], ..Default::default() };
        let (p, _) = spline_pts(&s, 1.0, usize::MAX);
        assert!(p.iter().all(|q| (q[0].hypot(q[1]) - 10.0).abs() < 1e-9));
    }

    #[test]
    fn tiny_weights_and_narrow_spans_keep_their_shape() {
        // A quarter circle of radius 100 with weights 1e-14 × (1, √½, 1).
        let w = 1e-14;
        let s = Spline { degree: 2, knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], ctrl: vec![[100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]], weights: vec![w, w * std::f64::consts::FRAC_1_SQRT_2, w], ..Default::default() };
        let (p, _) = spline_pts(&s, 1.0, usize::MAX);
        assert!(p.iter().all(|q| (q[0].hypot(q[1]) - 100.0).abs() < 1e-6));
        // A degree-1 spline whose 100 mm excursion sits in spans 3e-14 wide.
        let s = Spline {
            degree: 1,
            knots: vec![0.0, 0.0, 1e-14, 2e-14, 3e-14, 1.0, 1.0],
            ctrl: vec![[0.0, 0.0, 0.0], [0.0, 100.0, 0.0], [100.0, 100.0, 0.0], [100.0, 0.0, 0.0], [200.0, 0.0, 0.0]],
            weights: vec![1.0; 5],
            ..Default::default()
        };
        let (p, _) = spline_pts(&s, 1.0, usize::MAX);
        assert!(p.iter().any(|q| q[1] > 99.0), "{p:?}");
    }

    #[test]
    fn tiny_knot_values_draw_like_any_other() {
        let mk = |e: f64| Spline { degree: 2, knots: vec![0.0, 0.0, 0.0, e, e, e], ctrl: vec![[0.0, 0.0, 0.0], [10.0, 20.0, 0.0], [20.0, 0.0, 0.0]], weights: vec![1.0; 3], ..Default::default() };
        let (a, _) = spline_pts(&mk(1.0), 1.0, usize::MAX);
        let (b, _) = spline_pts(&mk(1e-13), 1.0, usize::MAX);
        assert!(spline_ok(&mk(1e-13)));
        assert_eq!(a.len(), b.len());
        assert!(b.iter().zip(&a).all(|(p, q)| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6));
    }

    #[test]
    fn fit_point_spline_passes_through_its_points() {
        let fit = vec![[0.0, 0.0, 0.0], [10.0, 5.0, 0.0], [20.0, 0.0, 0.0], [30.0, -5.0, 0.0]];
        let s = Spline { degree: 3, fit: fit.clone(), ..Default::default() };
        let (p, _) = spline_pts(&s, 1.0, usize::MAX);
        for f in &fit {
            assert!(p.iter().any(|q| (q[0] - f[0]).abs() < 1e-6 && (q[1] - f[1]).abs() < 1e-6));
        }
    }

    #[test]
    fn mtext_codes_are_removed() {
        assert_eq!(plain_text("{\\fArial|b1;PART \\H2.5;A}\\PQTY 2"), "PART A QTY 2");
        assert_eq!(plain_text("%%c20 \\U+00B0"), "Ø20 °");
        assert_eq!(plain_text("1\\S1^2;\""), "11/2\"");
    }
}
