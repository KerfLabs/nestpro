//! Import and export of part drawings as ASCII DXF.
//!
//! Rules taken from the original (see MECHANICS.md, "DXF"):
//! - joining tolerance 0.01 mm and arc smoothness 0.1 mm
//!   (DF4);
//! - every drawing object is looked up by its **layer** and given a machining
//!   role; the default role (code 13) is a normal cut contour, other layers can
//!   be marking or left out (DF5).
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
/// Fewest straight segments for a full circle, however small (DX2), so a small
/// hole stays round and its arcs can be written as arcs.
pub const ARC_MIN_SEGMENTS: f64 = 16.0;
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
    /// Every contour of the drawing with its state, in contour-number order:
    /// the closed contours inside the outline (holes), the outline, then the
    /// open lines (CS1). `outline`, `holes`, `marks` and the fills are made
    /// from it (`apply_states`). Parts saved before contour states have none;
    /// their outline, holes and marks stay exactly as saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contours: Vec<Contour>,
    /// The cut order set by hand (CO3): the contours (numbers from 0, as
    /// `contours` numbers them) each command added, command by command, in
    /// the order added. The part cuts them first, the outline always last,
    /// and the rest in the automatic order (`engine::cutorder`). Empty: no
    /// order set by hand, as in every part saved before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hand_order: Vec<Vec<usize>>,
}

/// What happens to one contour of a part (CS1, `GLOSSARY.md`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContourState {
    /// Cut through the sheet.
    #[default]
    Cut,
    /// Marked on the surface, not cut through.
    Mark,
    /// Kept on the part and shown, never machined.
    NoCut,
    /// A no-cut line where the part is bent later, shown in its own colour.
    BendLine,
    /// Not on the part at all.
    LeaveOut,
}

impl ContourState {
    pub const ALL: [ContourState; 5] = [ContourState::Cut, ContourState::Mark, ContourState::NoCut, ContourState::BendLine, ContourState::LeaveOut];
    /// The word on screen.
    pub fn label(self) -> &'static str {
        match self {
            ContourState::Cut => "Cut",
            ContourState::Mark => "Mark",
            ContourState::NoCut => "No cut",
            ContourState::BendLine => "Bend line",
            ContourState::LeaveOut => "Leave out",
        }
    }
    /// The agents' word (as saved in the job file).
    pub fn key(self) -> &'static str {
        match self {
            ContourState::Cut => "cut",
            ContourState::Mark => "mark",
            ContourState::NoCut => "no-cut",
            ContourState::BendLine => "bend-line",
            ContourState::LeaveOut => "leave-out",
        }
    }
    /// Shown on the part and the sheet, but never cut, marked, pierced or
    /// timed (CS3).
    pub fn shown_only(self) -> bool {
        matches!(self, ContourState::NoCut | ContourState::BendLine)
    }
}

/// One contour of a part as drawn, with its state (CS1). A closed contour
/// doesn't repeat its first point.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Contour {
    pub pts: Vec<P>,
    pub closed: bool,
    pub state: ContourState,
    /// The cut quality set on this contour by hand, which wins over the
    /// part's size ranges and quality and the machine's size bands (CQ6).
    /// Only a Cut contour has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<crate::nest::CutQuality>,
}

impl Contour {
    /// A contour in `state` with no quality set by hand.
    pub fn new(pts: Vec<P>, closed: bool, state: ContourState) -> Contour {
        Contour { pts, closed, state, quality: None }
    }

    /// The points to draw or mark along: a closed contour back to its start.
    pub fn path(&self) -> Vec<P> {
        let mut pts = self.pts.clone();
        if self.closed && !pts.is_empty() {
            pts.push(pts[0]);
        }
        pts
    }

    /// Its length: around a closed contour, along an open one.
    pub fn length(&self) -> f64 {
        crate::leads::path_len(&self.path())
    }
}

/// A marking line of a part saved before contour states that closes on
/// itself (its last point back on its first): numbered among the holes.
fn closed_mark(m: &[P]) -> bool {
    m.len() > 3 && close(m[0], m[m.len() - 1], JOIN_TOL)
}

/// Where the outline is in a contour list: the last closed contour (CS1
/// numbering: holes first, the outline, then the open lines).
pub fn outline_index(list: &[Contour]) -> Option<usize> {
    list.iter().rposition(|c| c.closed)
}

/// The contours the part cuts, as their places in `list`, in the order the
/// part's geometry has them: the Cut holes, then the outline (CS3). Element,
/// lead and joint contour numbers count in this order.
pub fn cut_order(list: &[Contour]) -> Vec<usize> {
    let o = outline_index(list);
    let mut v: Vec<usize> = (0..list.len()).filter(|&k| Some(k) != o && list[k].closed && list[k].state == ContourState::Cut).collect();
    v.extend(o);
    v
}

impl CustomGeo {
    /// The contours in number order (CS1): the part's own list, or for a part
    /// saved before contour states its holes and outline as Cut and its
    /// marking lines as Mark (closed ones among the holes).
    pub fn contour_list(&self) -> Vec<Contour> {
        if !self.contours.is_empty() {
            return self.contours.clone();
        }
        let mut list: Vec<Contour> = self.holes.iter().map(|h| Contour::new(h.clone(), true, ContourState::Cut)).collect();
        list.extend(self.marks.iter().filter(|m| closed_mark(m)).map(|m| Contour::new(m[..m.len() - 1].to_vec(), true, ContourState::Mark)));
        list.push(Contour::new(self.outline.clone(), true, ContourState::Cut));
        list.extend(self.marks.iter().filter(|m| !closed_mark(m)).map(|m| Contour::new(m.clone(), false, ContourState::Mark)));
        list
    }

    /// Each marking line's contour number (its place in `contour_list`), in
    /// the order of `marks`: the Mark contours in number order, or for a part
    /// saved before contour states its closed lines among the holes and its
    /// open ones after the outline, as `contour_list` numbers them.
    pub fn mark_numbers(&self) -> Vec<usize> {
        if !self.contours.is_empty() {
            return (0..self.contours.len()).filter(|&k| self.contours[k].state == ContourState::Mark).collect();
        }
        let closed = self.marks.iter().filter(|m| closed_mark(m)).count();
        let (mut c, mut o) = (self.holes.len(), self.holes.len() + closed + 1);
        let mut out = vec![];
        for m in &self.marks {
            let k = if closed_mark(m) { &mut c } else { &mut o };
            out.push(*k);
            *k += 1;
        }
        out
    }

    /// The quality set by hand on each contour the part cuts, in the order of
    /// its geometry (holes, then the outline; `cut_order`), `None` where the
    /// part's size ranges, quality or the machine's size bands decide (CQ6).
    /// Empty for a part saved before contour states.
    pub fn cut_qualities(&self) -> Vec<Option<crate::nest::CutQuality>> {
        cut_order(&self.contours).into_iter().map(|k| self.contours[k].quality).collect()
    }

    /// Make the outline, holes, marking lines and fills from the contours'
    /// states (CS3): only Cut contours are cut, Mark contours (open or closed)
    /// become marking lines, the rest are neither. Nothing changes for a part
    /// without a contour list.
    pub fn apply_states(&mut self) {
        let order = cut_order(&self.contours);
        let Some((&o, holes)) = order.split_last() else { return };
        self.outline = self.contours[o].pts.clone();
        self.holes = holes.iter().map(|&k| self.contours[k].pts.clone()).collect();
        self.marks = self.contours.iter().filter(|c| c.state == ContourState::Mark).map(Contour::path).collect();
        self.fills = triangulate(&self.outline);
        self.hole_fills = self.holes.iter().flat_map(|h| triangulate(h)).collect();
    }

    /// The contours drawn but never machined (No cut and Bend line, CS3), in
    /// part coordinates, with their state.
    pub fn shown_only(&self) -> impl Iterator<Item = &Contour> {
        self.contours.iter().filter(|c| c.state.shown_only())
    }
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
    /// Open lines left out because they aren't on the part.
    pub open_pieces: usize,
    /// Open lines kept on the part as Bend line or No cut (CS2).
    pub open_kept: usize,
    pub outside_loops: usize,
    /// Gaps between 0.01 and 0.5 mm that were closed (IM8).
    pub gaps_closed: usize,
    /// Objects drawn twice in the same place, left out once (DX4).
    pub duplicates: usize,
    /// Objects the drawing hides or draws as drafting lines, left out (DX3):
    /// (layer row, objects).
    pub hidden_left_out: Vec<(String, usize)>,
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
    /// Layer rows that start as "Leave out" (DX3), with why: objects on a frozen
    /// or turned-off layer, invisible objects, and objects in a centre, hidden
    /// or other dashed line type ("0 [CENTER]", "HOLES [frozen]"…).
    pub hidden: BTreeMap<String, String>,
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

    /// The role a layer row starts with: "Leave out" for what the drawing hides
    /// or draws as drafting lines (DX3), else Cut.
    pub fn default_role(&self, layer: &str) -> LayerRole {
        if self.hidden.contains_key(layer) {
            LayerRole::Ignore
        } else {
            LayerRole::Cut
        }
    }

    /// Whether a layer row holds lines drawn in a dashed or dotted line type
    /// (DX3's `0 [CENTER]` rows; not frozen, off or invisible ones).
    pub fn dashed(&self, layer: &str) -> bool {
        self.hidden.get(layer).is_some_and(|why| why.contains("line type"))
    }
}

/// A layer named like a bend layer ("BEND", "Bend lines", "BENDING"…), CS2.
pub fn bend_layer(layer: &str) -> bool {
    layer.to_ascii_uppercase().contains("BEND")
}

/// The state an open line on a cut layer starts with (CS2): a Bend line when
/// it is dashed, on a bend layer, or straight and running from edge to edge
/// of the part (both ends within `CLOSE_TOL` of the outline); otherwise No
/// cut. Cutting an open line could split the part, so none starts as Cut.
fn open_line_start(pts: &[P], bendish: bool, mut edge: impl FnMut(P) -> bool) -> ContourState {
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let straight = len > CLOSE_TOL && pts.iter().all(|&p| (((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / len).abs() <= JOIN_TOL);
    if bendish || (straight && edge(a) && edge(b)) {
        ContourState::BendLine
    } else {
        ContourState::NoCut
    }
}

/// A piece's points on a 0.001 mm grid, the same whichever end (or, closed,
/// whichever point) it starts from and whichever way it runs: two objects
/// drawn in the same place have the same key (DX4).
fn piece_key(pts: &[P], closed: bool) -> Vec<(i64, i64)> {
    let q: Vec<(i64, i64)> = pts.iter().map(|p| ((p[0] * 1000.0).round() as i64, (p[1] * 1000.0).round() as i64)).collect();
    let mut q = q;
    if closed && q.len() > 1 && q.first() == q.last() {
        q.pop();
    }
    let forward = |v: &[(i64, i64)]| -> Vec<(i64, i64)> {
        if !closed || v.is_empty() {
            return v.to_vec();
        }
        let k = (0..v.len()).min_by_key(|&i| v[i]).unwrap_or(0);
        v[k..].iter().chain(&v[..k]).copied().collect()
    };
    let a = forward(&q);
    let mut r = q.clone();
    r.reverse();
    let b = forward(&r);
    a.min(b)
}

/// Number of straight segments for an arc so it stays within ARC_CHORD_ERR,
/// and no coarser than ARC_MIN_SEGMENTS to the full turn (DX2).
pub(crate) fn arc_segments(r: f64, sweep: f64) -> usize {
    let fine = std::f64::consts::TAU / ARC_MIN_SEGMENTS;
    let step = if r > ARC_CHORD_ERR { (2.0 * (1.0 - ARC_CHORD_ERR / r).acos()).min(fine) } else { fine };
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

    /// Whether `p` is inside the polygon (even-odd rule), adding the number of
    /// edges looked at to `work` (so callers can hold a whole drawing's tests
    /// to a budget).
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

    /// Whether `p` lies within `tol` of the polygon's edge, adding the edges
    /// looked at to `work`.
    pub(crate) fn near_edge(&self, p: P, tol: f64, work: &mut usize) -> bool {
        if self.rows.is_empty() || p[0] < self.bbox[0] - tol || p[0] > self.bbox[2] + tol || p[1] < self.bbox[1] - tol || p[1] > self.bbox[3] + tol {
            return false;
        }
        let row = |y: f64| (((y - self.lo) / self.band).floor().max(0.0) as usize).min(self.rows.len() - 1);
        (row(p[1] - tol)..=row(p[1] + tol)).any(|r| {
            *work += self.rows[r].len();
            self.rows[r].iter().any(|&(a, b)| crate::nest::seg_point_dist(p, a, b) <= tol)
        })
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

/// `read`, with `unit_mm` for a file that doesn't say its unit (the ERP
/// integration's drawing unit). A file that says its unit is read in it.
pub fn read_in(text: &str, unit_mm: f64) -> Result<Drawing, String> {
    read_with(text, &crate::formats::dxfread::Options { unit_mm, ..Default::default() })
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

/// Where three or more piece ends meet (within 0.01 mm), the ends alone don't
/// say which way a contour goes on: a line across the part ending on the
/// outline's corners could take a corner's place in the outline (CS2). For
/// each group of pieces joined through such a point, the outside boundary is
/// taken as the closed contour, walked from the group's lowest point keeping
/// the outside on the right (always the rightmost turn); the pieces inside it
/// are left to join as usual, so they stay open lines. Gaps up to 0.5 mm in
/// such a group are closed first (IM8), each loose end with a straight piece to
/// the nearest end within 0.5 mm, so a small gap at a corner can't send the
/// walk across the part. Returns the pieces left to join, in drawing order: all
/// of them when no three ends meet anywhere, so such drawings join exactly as
/// before.
fn take_branch_boundaries(open: Vec<Vec<P>>, loops: &mut Vec<Vec<P>>, report: &mut ImportReport) -> Result<Vec<Vec<P>>, String> {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let eps = JOIN_TOL;
    // Piece ends filed into points (nodes).
    let mut nodes: Vec<P> = vec![];
    let mut grid = EndGrid::new(eps);
    let mut ends: Vec<Option<[usize; 2]>> = Vec::with_capacity(open.len());
    for c in &open {
        if c.len() < 2 || !c.iter().all(|p| p[0].is_finite() && p[1].is_finite()) {
            ends.push(None);
            continue;
        }
        let mut pair = [0; 2];
        for (j, p) in [c[0], c[c.len() - 1]].into_iter().enumerate() {
            let found = grid.first(p, |_| true, |n| close(nodes[n], p, eps));
            if grid.work > JOIN_WORK {
                return Err(join_too_complex());
            }
            pair[j] = match found {
                Some(n) => n,
                None => {
                    nodes.push(p);
                    grid.add(p, nodes.len() - 1);
                    nodes.len() - 1
                }
            };
        }
        ends.push(Some(pair));
    }
    let mut at: Vec<Vec<(usize, bool)>> = vec![vec![]; nodes.len()];
    for (k, e) in ends.iter().enumerate() {
        if let Some([a, b]) = *e {
            at[a].push((k, true));
            at[b].push((k, false));
        }
    }
    if at.iter().all(|h| h.len() < 3) {
        return Ok(open);
    }
    let branch_points: Vec<usize> = (0..nodes.len()).filter(|&n| at[n].len() >= 3).collect();
    // IM8 inside the groups: each loose end gets a straight piece to the
    // nearest other end within 0.5 mm (not its own piece's other end); these
    // pieces only exist for the walk.
    let real = open.len();
    let mut open = open;
    {
        let mut near = EndGrid::new(CLOSE_TOL);
        for (n, &p) in nodes.iter().enumerate() {
            near.add(p, n);
        }
        let dist = |a: P, b: P| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let mut gaps: std::collections::BTreeSet<(usize, usize)> = Default::default();
        for f in (0..nodes.len()).filter(|&n| at[n].len() == 1) {
            let (k, from_start) = at[f][0];
            let own = ends[k].map(|e| if from_start { e[1] } else { e[0] });
            let found = near.nearest(nodes[f], |n| n != f && Some(n) != own, |n| dist(nodes[n], nodes[f]));
            if near.work > JOIN_WORK {
                return Err(join_too_complex());
            }
            if let Some((n, _)) = found.filter(|(_, d)| *d <= CLOSE_TOL) {
                gaps.insert((f.min(n), f.max(n)));
            }
        }
        for (a, b) in gaps {
            at[a].push((open.len(), true));
            at[b].push((open.len(), false));
            ends.push(Some([a, b]));
            open.push(vec![nodes[a], nodes[b]]);
        }
    }
    // Groups of pieces joined through their ends; each group's lowest (then
    // leftmost) point.
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    for [a, b] in ends.iter().flatten().copied() {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        parent[ra] = rb;
    }
    let branched: std::collections::BTreeSet<usize> = branch_points.into_iter().map(|n| root(&mut parent, n)).collect();
    let mut lowest: BTreeMap<usize, usize> = BTreeMap::new();
    for n in 0..nodes.len() {
        let r = root(&mut parent, n);
        let low = lowest.entry(r).or_insert(n);
        if nodes[n][1] < nodes[*low][1] || (nodes[n][1] == nodes[*low][1] && nodes[n][0] < nodes[*low][0]) {
            *low = n;
        }
    }
    // The direction a piece leaves a node by, and arrives at its far end by.
    let leave = |(k, from_start): (usize, bool)| {
        let c = &open[k];
        let (p, q) = if from_start { (c[0], c[1]) } else { (c[c.len() - 1], c[c.len() - 2]) };
        (q[1] - p[1]).atan2(q[0] - p[0])
    };
    let arrive = |(k, from_start): (usize, bool)| {
        let c = &open[k];
        let (p, q) = if from_start { (c[c.len() - 2], c[c.len() - 1]) } else { (c[1], c[0]) };
        (q[1] - p[1]).atan2(q[0] - p[0])
    };
    let far = |(k, from_start): (usize, bool)| ends[k].map_or(0, |e| if from_start { e[1] } else { e[0] });
    let mut taken = vec![false; open.len()];
    for r in branched {
        let start = lowest[&r];
        // Walk round the outside, as if arriving at the lowest point from below.
        let (mut v, mut heading, mut came) = (start, FRAC_PI_2, None::<(usize, bool)>);
        let mut walk: Vec<(usize, bool)> = vec![];
        let ok = loop {
            let back = heading + PI;
            let turn = |h: (usize, bool)| match (leave(h) - back).rem_euclid(TAU) {
                a if a < 1e-12 => TAU,
                a => a,
            };
            let rev = came.map(|(k, s)| (k, !s));
            let next = at[v].iter().copied().filter(|h| Some(*h) != rev).min_by(|a, b| turn(*a).total_cmp(&turn(*b))).or(rev);
            let Some(h) = next else { break false };
            if walk.first() == Some(&h) {
                break true;
            }
            walk.push(h);
            if walk.len() > 2 * open.len() + 2 {
                break false;
            }
            (v, heading, came) = (far(h), arrive(h), Some(h));
        };
        // Pieces walked twice stick out of the boundary (or join two parts of
        // it); the ones walked once are the boundary.
        let mut times: BTreeMap<usize, usize> = BTreeMap::new();
        for h in &walk {
            *times.entry(h.0).or_insert(0) += 1;
        }
        let once: Vec<(usize, bool)> = walk.iter().copied().filter(|h| times[&h.0] == 1).collect();
        let joined = |i: usize| far(once[i]) == far((once[(i + 1) % once.len()].0, !once[(i + 1) % once.len()].1));
        if !ok || once.is_empty() || !(0..once.len()).all(joined) {
            continue;
        }
        let mut ring: Vec<P> = vec![];
        for &(k, from_start) in &once {
            let mut c = open[k].clone();
            if !from_start {
                c.reverse();
            }
            let skip = usize::from(!ring.is_empty());
            ring.extend(c.into_iter().skip(skip));
        }
        if ring.len() > 1 && close(ring[0], ring[ring.len() - 1], eps) {
            ring.pop();
        }
        if ring.len() >= 3 {
            loops.push(ring);
            for &(k, _) in &once {
                taken[k] = true;
            }
            report.gaps_closed += once.iter().filter(|h| h.0 >= real).count();
        }
    }
    Ok(open.into_iter().zip(taken).take(real).filter(|(_, t)| !t).map(|(c, _)| c).collect())
}

/// Join open pieces end to end into closed loops (added to `loops`): first
/// within 0.01 mm; then (IM8) only the chains still open get gaps up to 0.5 mm
/// closed, so a contour that closes on its own never loses a piece to a nearby
/// loose line. Piece ends are filed in a grid, so joining stays in step with
/// the drawing's size; the piece chosen is the same as a plain search would
/// choose. Where three or more ends meet, the outside boundary closes first
/// (`take_branch_boundaries`). Returns the chains that stay open.
fn join_pieces(pieces: Vec<Vec<P>>, loops: &mut Vec<Vec<P>>, report: &mut ImportReport) -> Result<Vec<Vec<P>>, String> {
    let mut open = take_branch_boundaries(pieces, loops, report)?;
    let eps = JOIN_TOL;
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
    let mut still_open: Vec<Vec<P>> = vec![];
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
                        still_open.push(chain);
                        break;
                    }
                }
            }
        }
    }
    Ok(still_open)
}

/// Turn a read drawing into one part, using a role per layer (missing = Cut).
pub fn build(d: &Drawing, roles: &BTreeMap<String, LayerRole>) -> Result<(CustomGeo, ImportReport), String> {
    let role = |l: &str| roles.get(l).copied().unwrap_or_else(|| d.default_role(l));
    let mut report = d.report.clone();
    let mut loops: Vec<Vec<P>> = vec![];
    let mut open: Vec<Vec<P>> = vec![];
    // Marking pieces (points, closed), and the open pieces of bend layers and
    // dashed rows (CS2).
    let mut mark_pieces: Vec<(Vec<P>, bool)> = vec![];
    let mut bend_pieces: Vec<Vec<P>> = vec![];
    // DX4: an object drawn twice in the same place is cut (or marked) once,
    // whatever its layer, so a doubled circle isn't pierced and cut twice.
    let mut seen: std::collections::HashSet<(bool, Vec<(i64, i64)>)> = Default::default();
    let mut hidden: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &d.pieces {
        let r = role(&p.layer);
        if r == LayerRole::Ignore {
            if d.hidden.contains_key(&p.layer) {
                *hidden.entry(&p.layer).or_insert(0) += 1;
            }
            continue;
        }
        if !seen.insert((r == LayerRole::Mark, piece_key(&p.pts, p.closed))) {
            report.duplicates += 1;
            continue;
        }
        match r {
            LayerRole::Ignore => {}
            LayerRole::Mark => mark_pieces.push((p.pts.clone(), p.closed)),
            LayerRole::Cut => {
                if p.closed {
                    loops.push(p.pts.clone());
                } else if bend_layer(&p.layer) || d.dashed(&p.layer) {
                    bend_pieces.push(p.pts.clone());
                } else {
                    open.push(p.pts.clone());
                }
            }
        }
    }
    report.hidden_left_out = hidden.into_iter().map(|(l, n)| (l.to_string(), n)).collect();
    let eps = JOIN_TOL;
    // Cut pieces are joined into loops; the bend layers' and dashed rows'
    // pieces only with each other (CS2), so a bend line ending on an outline
    // corner can't take that corner's place in the outline, while a hole drawn
    // dashed and set to Cut still closes.
    let open_lines = join_pieces(open, &mut loops, &mut report)?;
    let bend_lines = join_pieces(bend_pieces, &mut loops, &mut report)?;
    // A loop with a point that isn't a number (a damaged file) can't be an outline.
    let loops: Vec<Vec<P>> = loops.into_iter().filter(|l| l.iter().all(|p| p[0].is_finite() && p[1].is_finite())).map(|l| cleanup(l, eps)).filter(|l| l.len() >= 3).collect();
    if loops.is_empty() {
        return Err("No closed outline on the cut layers (pieces don't join within 0.5 mm).".into());
    }
    let oi = (0..loops.len()).max_by(|&a, &b| area(&loops[a]).total_cmp(&area(&loops[b]))).unwrap();
    let outline = loops[oi].clone();
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
    // CS2: an open line is on the part when the middle of one of its pieces
    // lies inside the outline; the others are left out, like outlines outside
    // the part.
    let mut lines: Vec<Contour> = vec![];
    for (mut pts, bendish) in open_lines.into_iter().map(|l| (l, false)).chain(bend_lines.into_iter().map(|l| (l, true))) {
        if work > HOLE_WORK {
            return Err("This drawing is too complex to sort its holes and lines from its outline in reasonable time (a very jagged outline with very many holes or lines). Nothing was imported; simplify the drawing or split it.".into());
        }
        pts.dedup_by(|a, b| close(*a, *b, eps));
        let mid = |w: &[P]| [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
        if pts.len() < 2 || !pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()) || !pts.windows(2).any(|w| inside.contains_counted(mid(w), &mut work)) {
            report.open_pieces += 1;
            continue;
        }
        let state = open_line_start(&pts, bendish, |p| inside.near_edge(p, CLOSE_TOL, &mut work));
        report.open_kept += 1;
        lines.push(Contour::new(pts, false, state));
    }
    let (mut ox, mut oy, mut mx, mut my) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &outline {
        ox = ox.min(p[0]);
        oy = oy.min(p[1]);
        mx = mx.max(p[0]);
        my = my.max(p[1]);
    }
    // Contour numbers (CS1): holes and other closed contours first, the
    // outline, then the open lines. Closed contours start as Cut, marking
    // layers' as Mark.
    let contour = Contour::new;
    let (closed_marks, open_marks): (Vec<_>, Vec<_>) = mark_pieces.into_iter().filter(|m| !m.0.is_empty()).partition(|m| m.1);
    let mut contours: Vec<Contour> = holes.into_iter().map(|h| contour(h, true, ContourState::Cut)).collect();
    contours.extend(closed_marks.into_iter().map(|(pts, _)| contour(pts, true, ContourState::Mark)));
    contours.push(contour(outline, true, ContourState::Cut));
    contours.extend(lines);
    contours.extend(open_marks.into_iter().map(|(pts, _)| contour(pts, false, ContourState::Mark)));
    for c in &mut contours {
        for p in c.pts.iter_mut() {
            p[0] -= ox;
            p[1] -= oy;
        }
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
    let mut g = CustomGeo { outline: vec![], holes: vec![], fills: vec![], hole_fills: vec![], marks: vec![], texts, w: mx - ox, h: my - oy, contours, hand_order: vec![] };
    g.apply_states();
    Ok((g, report))
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
            assert_eq!(idx.contains_counted(p, &mut 0), plain(p), "{p:?}");
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

    #[test]
    fn an_inch_drawing_comes_in_at_its_real_size() {
        let mm = parse(SAMPLE_DXF).unwrap().0;
        let inch = parse(&format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n{SAMPLE_DXF}")).unwrap().0;
        assert!((inch.w - mm.w * 25.4).abs() < 1e-6 && (inch.h - mm.h * 25.4).abs() < 1e-6, "{} vs {}", inch.w, mm.w);
        // ... and its round holes stay round: within the chord error in mm.
        for h in &inch.holes {
            let n = h.len() as f64;
            let c = [h.iter().map(|q| q[0]).sum::<f64>() / n, h.iter().map(|q| q[1]).sum::<f64>() / n];
            let r = ((h[0][0] - c[0]).powi(2) + (h[0][1] - c[1]).powi(2)).sqrt();
            for k in 0..h.len() {
                let (a, b) = (h[k], h[(k + 1) % h.len()]);
                let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let dev = r - ((mid[0] - c[0]).powi(2) + (mid[1] - c[1]).powi(2)).sqrt();
                assert!(dev <= ARC_CHORD_ERR + 1e-6, "hole r {r:.1}: chord {dev:.3} mm off");
            }
        }
    }
}
