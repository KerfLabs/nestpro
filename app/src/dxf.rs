//! Import and export of part drawings as ASCII DXF.
//!
//! Rules taken from the original (see MECHANICS.md, "DXF"):
//! - joining tolerance 0.01 mm and arc smoothness 0.1 mm
//!   (D4);
//! - every drawing object is looked up by its **layer** and given a machining
//!   role; the default role (code 13) is a normal cut contour, other layers can
//!   be marking or left out (D5).
//!
//! Reads lines, arcs, circles, polylines (with bulges, old and new style),
//! ellipses, splines, texts and block inserts, each in its own plane, from model
//! space (`formats::dxfread`, the same geometry step as DWG). Cut pieces are
//! joined end-to-end into closed loops (gaps up to 0.5 mm closed); the largest
//! loop is the outer contour and the loops inside it are holes. Units are
//! millimetres.

use crate::nest::{area, P};
use std::collections::BTreeMap;

/// Joining tolerance in mm (original default 0.01).
pub const JOIN_TOL: f64 = 0.01;
/// Largest gap between an arc and its straight segments, in mm (original default 0.1).
pub const ARC_CHORD_ERR: f64 = 0.1;
/// Most outline edges the hole tests of one drawing may look at.
const HOLE_WORK: usize = 200_000_000;

/// Gaps up to this size (mm) left after joining are closed with a straight piece
/// ("close contours", on by default with 0.5 mm in every importer, IM8).
pub const CLOSE_TOL: f64 = 0.5;

/// A part shape read from a drawing, moved so its bounding box starts at 0,0.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomGeo {
    pub outline: Vec<P>,
    pub holes: Vec<Vec<P>>,
    /// Triangles filling the outline (for drawing concave shapes).
    pub fills: Vec<Vec<P>>,
    /// Triangles filling each hole.
    pub hole_fills: Vec<Vec<P>>,
    /// Marking lines (from layers set to "Mark"), not cut through.
    pub marks: Vec<Vec<P>>,
    /// Texts from the drawing (TEXT/MTEXT), in part coordinates.
    pub texts: Vec<crate::nest::PartText>,
    pub w: f64,
    pub h: f64,
}

/// What a drawing layer becomes (original: layer → machining code; 13 = cut).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerRole {
    Cut,
    Mark,
    Ignore,
}

impl LayerRole {
    pub const ALL: [LayerRole; 3] = [LayerRole::Cut, LayerRole::Mark, LayerRole::Ignore];
    pub fn label(self) -> &'static str {
        match self {
            LayerRole::Cut => "Cut",
            LayerRole::Mark => "Mark",
            LayerRole::Ignore => "Leave out",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ImportReport {
    pub entities: usize,
    pub skipped: Vec<String>,
    pub open_pieces: usize,
    pub outside_loops: usize,
    /// Gaps between 0.01 and 0.5 mm that were closed (IM8).
    pub gaps_closed: usize,
}

/// One drawn object, already turned into points.
#[derive(Debug, Clone)]
pub struct Piece {
    pub layer: String,
    pub pts: Vec<P>,
    pub closed: bool,
}

/// A drawing as read, before layer roles are applied.
#[derive(Debug, Clone, Default)]
pub struct Drawing {
    pub pieces: Vec<Piece>,
    /// Texts: (layer, text, insertion point, height, angle in degrees).
    pub texts: Vec<(String, String, P, f64, f64)>,
    pub report: ImportReport,
}

impl Drawing {
    /// Layer names with how many objects each has, in name order.
    pub fn layers(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for p in &self.pieces {
            *m.entry(p.layer.clone()).or_insert(0) += 1;
        }
        for t in &self.texts {
            *m.entry(t.0.clone()).or_insert(0) += 1;
        }
        m
    }
}

/// Number of straight segments for an arc so it stays within ARC_CHORD_ERR.
pub(crate) fn arc_segments(r: f64, sweep: f64) -> usize {
    let step = if r > ARC_CHORD_ERR { 2.0 * (1.0 - ARC_CHORD_ERR / r).acos() } else { std::f64::consts::FRAC_PI_2 };
    ((sweep.abs() / step).ceil() as usize).clamp(2, 720)
}

pub(crate) fn arc_points(c: P, r: f64, a0: f64, sweep: f64) -> Vec<P> {
    let n = arc_segments(r, sweep);
    (0..=n)
        .map(|i| {
            let a = a0 + sweep * i as f64 / n as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Piece ends filed by grid cell (`cell` wide), for finding the pieces whose
/// end lies near a point without looking at every piece. Each cell keeps its
/// entries in number order and skips used-up ones from the front, so even many
/// ends in one cell are found quickly.
pub(crate) struct EndGrid {
    cell: f64,
    map: std::collections::HashMap<(i64, i64), (usize, Vec<usize>)>,
    /// Entries looked at so far (callers hold a drawing to `JOIN_WORK`).
    pub(crate) work: usize,
}

/// Most piece ends one drawing's joining may look at; past that the drawing is
/// refused whole rather than holding up the window.
pub(crate) const JOIN_WORK: usize = 200_000_000;

/// The message for a drawing whose joining went past `JOIN_WORK`.
pub(crate) fn join_too_complex() -> String {
    "This drawing is too complex to join its lines into contours in reasonable time (very many line ends crowded together). Nothing was imported; simplify the drawing or split it.".into()
}

impl EndGrid {
    pub(crate) fn new(cell: f64) -> Self {
        EndGrid { cell, map: std::collections::HashMap::new(), work: 0 }
    }
    fn key(&self, p: P) -> (i64, i64) {
        ((p[0] / self.cell).floor() as i64, (p[1] / self.cell).floor() as i64)
    }
    /// File entry `k` at `p`; entries must be added in increasing number order.
    pub(crate) fn add(&mut self, p: P, k: usize) {
        if p[0].is_finite() && p[1].is_finite() {
            let key = self.key(p);
            self.map.entry(key).or_default().1.push(k);
        }
    }
    /// The lowest-numbered entry near `p` that is still `alive` and `near`.
    pub(crate) fn first(&mut self, p: P, alive: impl Fn(usize) -> bool, near: impl Fn(usize) -> bool) -> Option<usize> {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        let (cx, cy) = self.key(p);
        let mut best: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some((start, v)) = self.map.get_mut(&(cx + dx, cy + dy)) {
                    while *start < v.len() && !alive(v[*start]) {
                        *start += 1;
                    }
                    for &k in &v[*start..] {
                        if best.is_some_and(|b| k >= b) {
                            break;
                        }
                        self.work += 1;
                        if alive(k) && near(k) {
                            best = Some(k);
                            break;
                        }
                    }
                }
            }
        }
        best
    }
    /// The `alive` entry near `p` with the smallest distance `d`; ties go to the
    /// lowest number.
    pub(crate) fn nearest(&mut self, p: P, alive: impl Fn(usize) -> bool, d: impl Fn(usize) -> f64) -> Option<(usize, f64)> {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        let (cx, cy) = self.key(p);
        let mut best: Option<(usize, f64)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some((start, v)) = self.map.get_mut(&(cx + dx, cy + dy)) {
                    while *start < v.len() && !alive(v[*start]) {
                        *start += 1;
                    }
                    for &k in &v[*start..] {
                        self.work += 1;
                        if alive(k) {
                            let dk = d(k);
                            if best.is_none_or(|(bk, bd)| dk < bd || (dk == bd && k < bk)) {
                                best = Some((k, dk));
                            }
                        }
                    }
                }
            }
        }
        best
    }
}

/// A polygon's edges filed by horizontal band, for quick point-in tests.
pub(crate) struct PolyIndex {
    lo: f64,
    band: f64,
    rows: Vec<Vec<(P, P)>>,
    bbox: [f64; 4],
}

impl PolyIndex {
    pub(crate) fn new(poly: &[P]) -> Self {
        let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in poly {
            bbox = [bbox[0].min(p[0]), bbox[1].min(p[1]), bbox[2].max(p[0]), bbox[3].max(p[1])];
        }
        let n = poly.len();
        let h = (bbox[3] - bbox[1]).max(1e-9);
        // About 8 edges a band; fewer bands when long edges would be filed many
        // times. The filing is counted first and stored only once it fits in
        // 16 entries an edge, so a jagged outline can't claim memory on its own.
        let rows_of = |nrows: usize, a: P, b: P| {
            let band = h / nrows as f64;
            let r0 = (((a[1].min(b[1]) - bbox[1]) / band).floor().max(0.0) as usize).min(nrows - 1);
            let r1 = (((a[1].max(b[1]) - bbox[1]) / band).floor().max(0.0) as usize).min(nrows - 1);
            (r0, r1)
        };
        let mut nrows = (n / 8).clamp(1, 4096);
        while nrows > 1 {
            let budget = 16 * n.max(1);
            let mut filed = 0usize;
            for i in 0..n {
                let (r0, r1) = rows_of(nrows, poly[i], poly[(i + n - 1) % n]);
                filed += r1 - r0 + 1;
                if filed > budget {
                    break;
                }
            }
            if filed <= budget {
                break;
            }
            nrows = (nrows / 4).max(1);
        }
        let band = h / nrows as f64;
        let mut rows: Vec<Vec<(P, P)>> = vec![vec![]; nrows];
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + n - 1) % n]);
            let (r0, r1) = rows_of(nrows, a, b);
            for r in rows.iter_mut().take(r1 + 1).skip(r0) {
                r.push((a, b));
            }
        }
        PolyIndex { lo: bbox[1], band, rows, bbox }
    }

    /// Whether `p` is inside the polygon (even-odd rule).
    pub(crate) fn contains(&self, p: P) -> bool {
        self.contains_counted(p, &mut 0)
    }

    /// `contains`, adding the number of edges looked at to `work` (so callers
    /// can hold a whole drawing's tests to a budget).
    pub(crate) fn contains_counted(&self, p: P, work: &mut usize) -> bool {
        if self.rows.is_empty() || p[0] < self.bbox[0] || p[0] > self.bbox[2] || p[1] < self.bbox[1] || p[1] > self.bbox[3] {
            return false;
        }
        let r = (((p[1] - self.lo) / self.band).floor().max(0.0) as usize).min(self.rows.len() - 1);
        *work += self.rows[r].len();
        let mut inside = false;
        for &(a, b) in &self.rows[r] {
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
                inside = !inside;
            }
        }
        inside
    }
}

fn close(a: P, b: P, eps: f64) -> bool {
    (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps
}

pub fn signed_area(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn cleanup(mut pts: Vec<P>, eps: f64) -> Vec<P> {
    pts.dedup_by(|a, b| close(*a, *b, eps));
    if pts.len() > 1 && close(pts[0], pts[pts.len() - 1], eps) {
        pts.pop();
    }
    pts
}

/// Ear-clipping triangulation of a simple polygon.
pub fn triangulate(poly: &[P]) -> Vec<Vec<P>> {
    // Ear clipping over a linked list of corners. Only corners that turn the
    // wrong way (reflex) can lie inside an ear, so only they are tested; the
    // fills are for drawing only, so a shape that would take too long gets
    // none rather than holding up the import.
    let mut pts: Vec<P> = poly.to_vec();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let n = pts.len();
    if n < 3 {
        return vec![];
    }
    let cross = |a: P, b: P, c: P| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    let mut reflex: Vec<usize> = (0..n).filter(|&i| cross(pts[prev[i]], pts[i], pts[next[i]]) <= 1e-12).collect();
    let mut tris = Vec::with_capacity(n.saturating_sub(2));
    let (mut left, mut i, mut tried, mut work, mut clips) = (n, 0usize, 0usize, 0usize, 0usize);
    const WORK: usize = 20_000_000;
    while left > 3 {
        if tried > left {
            break; // no ear in a whole turn: a damaged shape, stop
        }
        let (a, b, c) = (prev[i], i, next[i]);
        let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
        let mut ear = cross(pa, pb, pc) > 1e-12;
        if ear {
            for &k in &reflex {
                work += 1;
                if !alive[k] || k == a || k == b || k == c {
                    continue;
                }
                let p = pts[k];
                if cross(pa, pb, p) >= 0.0 && cross(pb, pc, p) >= 0.0 && cross(pc, pa, p) >= 0.0 {
                    ear = false;
                    break;
                }
            }
        }
        if work > WORK {
            return vec![];
        }
        if ear {
            tris.push(vec![pa, pb, pc]);
            alive[b] = false;
            next[a] = c;
            prev[c] = a;
            left -= 1;
            tried = 0;
            clips += 1;
            if clips % 64 == 0 {
                reflex.retain(|&k| alive[k]);
            }
            i = a;
        } else {
            i = next[i];
            tried += 1;
        }
    }
    if left == 3 {
        let (a, b) = (i, next[i]);
        tris.push(vec![pts[a], pts[b], pts[next[b]]]);
    }
    tris
}

/// Read every drawable object of a DXF, keeping its layer: lines, arcs, circles,
/// polylines (with bulges), ellipses, splines, texts and block inserts, in each
/// object's own plane, model space only (JOB-20, DX1; the same geometry step as DWG).
pub fn read(text: &str) -> Result<Drawing, String> {
    read_with(text, &crate::formats::dxfread::Options::default())
}

/// `read` with the reading choices (paper space, blocks, invisible objects).
pub fn read_with(text: &str, opts: &crate::formats::dxfread::Options) -> Result<Drawing, String> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut pairs: Vec<(i32, &str)> = Vec::with_capacity(lines.len() / 2);
    let mut i = 0;
    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].parse::<i32>() {
            pairs.push((code, lines[i + 1]));
        }
        i += 2;
    }
    crate::formats::dxfread::read(&pairs, opts)
}

/// Turn a read drawing into one part, using a role per layer (missing = Cut).
pub fn build(d: &Drawing, roles: &BTreeMap<String, LayerRole>) -> Result<(CustomGeo, ImportReport), String> {
    let role = |l: &str| roles.get(l).copied().unwrap_or(LayerRole::Cut);
    let mut report = d.report.clone();
    let mut loops: Vec<Vec<P>> = vec![];
    let mut open: Vec<Vec<P>> = vec![];
    let mut marks: Vec<Vec<P>> = vec![];
    for p in &d.pieces {
        match role(&p.layer) {
            LayerRole::Ignore => {}
            LayerRole::Mark => {
                let mut pts = p.pts.clone();
                if p.closed {
                    pts.push(pts[0]);
                }
                marks.push(pts);
            }
            LayerRole::Cut => {
                if p.closed {
                    loops.push(p.pts.clone());
                } else {
                    open.push(p.pts.clone());
                }
            }
        }
    }
    let eps = JOIN_TOL;
    // Join open cut pieces into loops: first end to end within 0.01 mm; then
    // (IM8) only the chains still open get gaps up to 0.5 mm closed, so a contour
    // that closes on its own never loses a piece to a nearby loose line. Piece
    // ends are filed in a grid, so joining stays in step with the drawing's size;
    // the piece chosen is the same as a plain search would choose.
    let mut left: Vec<Vec<P>> = vec![];
    {
        let mut ends = EndGrid::new(eps);
        for (k, c) in open.iter().enumerate() {
            ends.add(c[0], k);
            ends.add(*c.last().unwrap(), k);
        }
        let mut alive = vec![true; open.len()];
        let mut top = open.len();
        loop {
            // The last piece still unused starts the next chain.
            while top > 0 && !alive[top - 1] {
                top -= 1;
            }
            if top == 0 {
                break;
            }
            top -= 1;
            alive[top] = false;
            let mut chain = std::mem::take(&mut open[top]);
            loop {
                let end = *chain.last().unwrap();
                if chain.len() > 2 && close(end, chain[0], eps) {
                    loops.push(chain);
                    break;
                }
                let next = ends.first(end, |k| alive[k], |k| close(open[k][0], end, eps) || close(*open[k].last().unwrap(), end, eps));
                if ends.work > JOIN_WORK {
                    return Err(join_too_complex());
                }
                match next {
                    Some(n) => {
                        alive[n] = false;
                        let mut c = std::mem::take(&mut open[n]);
                        if !close(c[0], end, eps) {
                            c.reverse();
                        }
                        chain.extend(c.into_iter().skip(1));
                    }
                    None => {
                        left.push(chain);
                        break;
                    }
                }
            }
        }
    }
    let dist = |a: P, b: P| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    {
        let mut ends = EndGrid::new(CLOSE_TOL);
        for (k, c) in left.iter().enumerate() {
            ends.add(c[0], k);
            ends.add(*c.last().unwrap(), k);
        }
        let mut alive = vec![true; left.len()];
        let mut top = left.len();
        loop {
            while top > 0 && !alive[top - 1] {
                top -= 1;
            }
            if top == 0 {
                break;
            }
            top -= 1;
            alive[top] = false;
            let mut chain = std::mem::take(&mut left[top]);
            loop {
                // Close to the nearest end of another open chain, or back to the chain's
                // own start when that is nearer (a lone chain closes on itself).
                let end = *chain.last().unwrap();
                let own = if chain.len() > 2 { dist(end, chain[0]) } else { f64::MAX };
                let other = ends.nearest(end, |k| alive[k], |k| dist(left[k][0], end).min(dist(*left[k].last().unwrap(), end)));
                if ends.work > JOIN_WORK {
                    return Err(join_too_complex());
                }
                match other {
                    Some((n, d)) if d <= CLOSE_TOL && d < own => {
                        alive[n] = false;
                        let mut c = std::mem::take(&mut left[n]);
                        if dist(c[0], end) > dist(*c.last().unwrap(), end) {
                            c.reverse();
                        }
                        if d > eps {
                            report.gaps_closed += 1;
                        }
                        chain.extend(c);
                    }
                    _ if own <= CLOSE_TOL => {
                        if own > eps {
                            report.gaps_closed += 1;
                        }
                        loops.push(chain);
                        break;
                    }
                    _ => {
                        report.open_pieces += 1;
                        break;
                    }
                }
            }
        }
    }
    // A loop with a point that isn't a number (a damaged file) can't be an outline.
    let loops: Vec<Vec<P>> = loops.into_iter().filter(|l| l.iter().all(|p| p[0].is_finite() && p[1].is_finite())).map(|l| cleanup(l, eps)).filter(|l| l.len() >= 3).collect();
    if loops.is_empty() {
        return Err("No closed outline on the cut layers (pieces don't join within 0.5 mm).".into());
    }
    let oi = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).total_cmp(&area(&loops[b]))).unwrap();
    let mut outline = loops[oi].clone();
    let mut holes = vec![];
    // The outline's edges filed by band: each hole test looks at the few edges
    // at its height, not the whole outline.
    let inside = PolyIndex::new(&outline);
    // A deliberately jagged outline puts most edges in every band; past this
    // many edge checks the drawing is refused whole rather than holding up the
    // window (IM8, DW15).
    let mut work = 0usize;
    for (k, l) in loops.iter().enumerate() {
        if k == oi {
            continue;
        }
        if work > HOLE_WORK {
            return Err("This drawing is too complex to sort its holes from its outline in reasonable time (a very jagged outline with very many holes). Nothing was imported; simplify the drawing or split it.".into());
        }
        if inside.contains_counted(l[0], &mut work) {
            holes.push(l.clone());
        } else {
            report.outside_loops += 1;
        }
    }
    let (mut ox, mut oy, mut mx, mut my) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &outline {
        ox = ox.min(p[0]);
        oy = oy.min(p[1]);
        mx = mx.max(p[0]);
        my = my.max(p[1]);
    }
    let shift = |l: &mut Vec<P>| {
        for p in l.iter_mut() {
            p[0] -= ox;
            p[1] -= oy;
        }
    };
    shift(&mut outline);
    for h in &mut holes {
        shift(h);
    }
    for m in &mut marks {
        shift(m);
    }
    // Texts on a Mark layer are marked; texts on a cut layer stay drawing notes
    // (text can't be cut through); left-out layers drop theirs.
    let texts = d
        .texts
        .iter()
        .filter_map(|(layer, text, at, height, angle)| {
            let kind = match role(layer) {
                LayerRole::Ignore => return None,
                LayerRole::Mark => crate::nest::TextKind::Mark,
                LayerRole::Cut => crate::nest::TextKind::Note,
            };
            Some(crate::nest::PartText { text: text.clone(), at: [at[0] - ox, at[1] - oy], height: *height, angle: *angle, curve: 0.0, kind })
        })
        .collect();
    let fills = triangulate(&outline);
    let hole_fills = holes.iter().flat_map(|h| triangulate(h)).collect();
    Ok((CustomGeo { outline, holes, fills, hole_fills, marks, texts, w: mx - ox, h: my - oy }, report))
}

/// Read a DXF and build it with every layer cut.
#[allow(dead_code)]
pub fn parse(text: &str) -> Result<(CustomGeo, ImportReport), String> {
    build(&read(text)?, &BTreeMap::new())
}

/// Write a part as DXF: outer contour and holes on layer CUT, marking on MARK.
pub fn write(outline: &[P], holes: &[Vec<P>], marks: &[Vec<P>]) -> String {
    use std::fmt::Write;
    let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let mut poly = |pts: &[P], layer: &str, closed: bool| {
        let _ = write!(s, "0\nLWPOLYLINE\n8\n{layer}\n90\n{}\n70\n{}\n", pts.len(), if closed { 1 } else { 0 });
        for p in pts {
            let _ = write!(s, "10\n{:.4}\n20\n{:.4}\n", p[0], p[1]);
        }
    };
    poly(outline, "CUT", true);
    for h in holes {
        poly(h, "CUT", true);
    }
    for m in marks {
        poly(m, "MARK", false);
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

/// A small drawing used for the sample job: a plate with a rounded end, two
/// holes, and a part number to be marked (layer MARK).
pub const SAMPLE_DXF: &str = "0\nSECTION\n2\nENTITIES\n\
0\nLWPOLYLINE\n8\nCUT\n90\n6\n70\n1\n10\n0\n20\n0\n10\n180\n20\n0\n42\n1\n10\n180\n20\n120\n10\n110\n20\n120\n10\n90\n20\n80\n10\n0\n20\n80\n\
0\nCIRCLE\n8\nCUT\n10\n180\n20\n60\n40\n18\n\
0\nCIRCLE\n8\nCUT\n10\n30\n20\n40\n40\n8\n\
0\nLWPOLYLINE\n8\nMARK\n90\n4\n70\n0\n10\n60\n20\n20\n10\n60\n20\n50\n10\n75\n20\n50\n10\n75\n20\n20\n\
0\nENDSEC\n0\nEOF\n";

/// Roles for the sample drawing's layers.
pub fn sample_roles() -> BTreeMap<String, LayerRole> {
    BTreeMap::from([("MARK".to_string(), LayerRole::Mark)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_circle_make_a_plate_with_a_hole() {
        let dxf = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n10\n10\n20\n10\n11\n110\n21\n10\n\
0\nLINE\n10\n110\n20\n60\n11\n110\n21\n10\n\
0\nLINE\n10\n110\n20\n60\n11\n10\n21\n60\n\
0\nLINE\n10\n10\n20\n60\n11\n10\n21\n10\n\
0\nCIRCLE\n10\n60\n20\n35\n40\n10\n\
0\nTEXT\n1\nlabel\n\
0\nENDSEC\n0\nEOF\n";
        let (g, rep) = parse(dxf).unwrap();
        assert!((g.w - 100.0).abs() < 1e-6 && (g.h - 50.0).abs() < 1e-6);
        assert_eq!(g.holes.len(), 1);
        assert!((area(&g.outline) - 5000.0).abs() < 1.0);
        assert_eq!(rep.open_pieces, 0);
        let tri: f64 = g.fills.iter().map(|t| area(t)).sum();
        assert!((tri - 5000.0).abs() < 1.0);
    }

    #[test]
    fn small_gaps_are_closed_and_wide_ones_stay_open() {
        // IM8: the second line starts 0.05 mm from where the first ends (> 0.01 mm,
        // within 0.5 mm): the gap is closed and reported.
        let tri = |gap: &str| {
            format!(
                "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0\n20\n0\n11\n100\n21\n0\n0\nLINE\n10\n{gap}\n20\n0\n11\n100\n21\n50\n0\nLINE\n10\n100\n20\n50\n11\n0\n21\n0\n0\nENDSEC\n0\nEOF\n"
            )
        };
        let (g, rep) = parse(&tri("100.05")).unwrap();
        assert_eq!((rep.gaps_closed, rep.open_pieces), (1, 0));
        assert!((g.w - 100.05).abs() < 1e-9);
        // 0.6 mm is more than 0.5 mm: the pieces stay open.
        assert!(parse(&tri("100.6")).is_err());
    }

    #[test]
    fn a_jagged_outline_indexes_without_huge_memory_and_answers_right() {
        // A 100 000-tooth saw: every tooth spans the full height.
        let n = 50_000;
        let mut saw: Vec<P> = vec![[0.0, -10.0]];
        for k in 0..n {
            saw.push([k as f64, 100.0]);
            saw.push([k as f64 + 0.5, 0.0]);
        }
        saw.push([n as f64, -10.0]);
        let idx = PolyIndex::new(&saw);
        let filed: usize = idx.rows.iter().map(Vec::len).sum();
        assert!(filed <= 16 * saw.len(), "{filed}");
        let plain = |p: P| {
            let mut inside = false;
            let m = saw.len();
            for i in 0..m {
                let (a, b) = (saw[i], saw[(i + m - 1) % m]);
                if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
                    inside = !inside;
                }
            }
            inside
        };
        for p in [[10.25, 50.0], [10.75, 99.0], [3.0, -5.0], [3.2, 0.0], [-1.0, 5.0], [100.0, 100.0]] {
            assert_eq!(idx.contains(p), plain(p), "{p:?}");
        }
    }

    #[test]
    fn big_drawings_build_in_step_with_their_size() {
        // A 20 000-corner round outline, 5 000 separate loose lines and 5 000
        // small holes: each step finds its pieces through an index, so this
        // takes well under a second even in a debug build.
        let t0 = std::time::Instant::now();
        let n = 20_000;
        let ring: Vec<P> = (0..n).map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            [500.0 + 400.0 * a.cos(), 500.0 + 400.0 * a.sin()]
        }).collect();
        assert_eq!(triangulate(&ring).len(), n - 2);
        let mut d = Drawing::default();
        d.pieces.push(Piece { layer: "0".into(), pts: ring, closed: true });
        for k in 0..5_000 {
            let (x, y) = (300.0 + (k % 70) as f64 * 6.0, 300.0 + (k / 70) as f64 * 6.0);
            d.pieces.push(Piece { layer: "0".into(), pts: vec![[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]], closed: true });
            d.pieces.push(Piece { layer: "0".into(), pts: vec![[2000.0 + k as f64 * 3.0, 0.0], [2001.0 + k as f64 * 3.0, 0.0]], closed: false });
        }
        let (g, rep) = build(&d, &BTreeMap::new()).unwrap();
        assert_eq!((g.holes.len(), rep.open_pieces), (5_000, 5_000));
        assert!(t0.elapsed().as_secs_f64() < 20.0, "{:?}", t0.elapsed());
    }

    #[test]
    fn arcs_stay_within_the_chord_error() {
        let pts = arc_points([0.0, 0.0], 50.0, 0.0, std::f64::consts::TAU);
        for w in pts.windows(2) {
            let mid = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
            let dev = 50.0 - (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
            assert!(dev <= ARC_CHORD_ERR + 1e-9, "deviation {dev}");
        }
    }

    #[test]
    fn sample_with_bulge_and_mark_layer() {
        let d = read(SAMPLE_DXF).unwrap();
        assert_eq!(d.layers().len(), 2);
        let (g, _) = build(&d, &sample_roles()).unwrap();
        assert_eq!(g.holes.len(), 2);
        assert_eq!(g.marks.len(), 1);
        assert!((g.w - 240.0).abs() < 0.5, "w = {}", g.w);
        // Leaving the marking layer out drops the mark.
        let (g2, _) = build(&d, &BTreeMap::from([("MARK".to_string(), LayerRole::Ignore)])).unwrap();
        assert!(g2.marks.is_empty());
    }

    #[test]
    fn export_reads_back_the_same_part() {
        let (g, _) = build(&read(SAMPLE_DXF).unwrap(), &sample_roles()).unwrap();
        let text = write(&g.outline, &g.holes, &g.marks);
        let d = read(&text).unwrap();
        let (g2, _) = build(&d, &BTreeMap::from([("MARK".to_string(), LayerRole::Mark)])).unwrap();
        assert_eq!(g2.holes.len(), g.holes.len());
        assert!((area(&g2.outline) - area(&g.outline)).abs() < 0.5);
        assert_eq!(g2.marks.len(), 1);
    }

    #[test]
    fn not_a_dxf_is_refused() {
        assert!(parse("hello").is_err());
    }
}
