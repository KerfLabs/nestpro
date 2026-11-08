//! IGES drawings (JOB-1).
//!
//! Rules (see MECHANICS.md, "IGES import"):
//! - IG1: the file is the fixed 80-column layout (Start, Global, Directory,
//!   Parameter, Terminate sections); free-format parameters, Hollerith strings.
//! - IG2: the Global section's units flag (and unit name) converts every length
//!   to millimetres.
//! - IG3: read curves: line (110), circular arc (100), composite curve (102),
//!   conic arc (104, ellipses), copious data / polylines (106 forms 11–13, 63),
//!   parametric spline (112), rational B-spline (126), general note (212, as text),
//!   subfigure instances (408 of 308, exploded). Points, dimensions and other
//!   annotation are left out like in DXF; surfaces and solids are left out and
//!   reported.
//! - IG4: transformation matrices (124) are applied, nested ones included; a
//!   matrix that stands the drawing on edge is turned so it lies flat. Drawing
//!   views (410/404) are not used: entities are drawn in model space.
//! - IG5: the level number is the layer.
//! - IG6: hidden (blanked) entities, parameter-space curves and entities that are
//!   only parts of another entity are not drawn on their own (no duplicates).
//! - IG7: splines become straight segments within the arc smoothness (0.1 mm) and
//!   no longer than 10 mm each; arcs use the arc smoothness.
//! - IG8: a drawing that is flat but not in the XY plane is laid flat.

use super::{Meta, ReadPart};
use crate::dxf::{ARC_CHORD_ERR, Drawing, Piece};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Options {
    /// IG7: longest straight segment a spline becomes, in mm (original default 10).
    pub spline_segment: f64,
    /// IG6: leave out entities marked hidden (blanked).
    pub skip_hidden: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { spline_segment: 10.0, skip_hidden: true }
    }
}

type V3 = [f64; 3];

/// A 3 × 4 transformation: rotation/scale rows and a translation column.
#[derive(Clone, Copy, Debug)]
struct Xf {
    r: [[f64; 3]; 3],
    t: V3,
}

impl Xf {
    const ID: Xf = Xf { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };
    fn pt(&self, p: V3) -> V3 {
        let v = self.vec(p);
        [v[0] + self.t[0], v[1] + self.t[1], v[2] + self.t[2]]
    }
    fn vec(&self, p: V3) -> V3 {
        let r = &self.r;
        [
            r[0][0] * p[0] + r[0][1] * p[1] + r[0][2] * p[2],
            r[1][0] * p[0] + r[1][1] * p[1] + r[1][2] * p[2],
            r[2][0] * p[0] + r[2][1] * p[1] + r[2][2] * p[2],
        ]
    }
    /// `self` after `inner` (inner is applied first).
    fn after(&self, inner: &Xf) -> Xf {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.r[i][k] * inner.r[k][j]).sum();
            }
        }
        Xf { r, t: self.pt(inner.t) }
    }
    fn scale_xy(&self) -> f64 {
        let a = self.vec([1.0, 0.0, 0.0]);
        (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
    }
}

/// A directory entry (two 80-column records).
#[derive(Clone, Debug, Default)]
struct De {
    typ: i32,
    level: i64,
    xform: usize,
    blank: u32,
    sub: u32,
    use_: u32,
    form: i32,
}

enum Item {
    Curve { pts: Vec<V3>, closed: bool, layer: String },
    Note { text: String, at: V3, dir: V3, up: V3, layer: String },
}

impl Item {
    fn map(&mut self, x: &Xf) {
        match self {
            Item::Curve { pts, .. } => {
                for p in pts.iter_mut() {
                    *p = x.pt(*p);
                }
            }
            Item::Note { at, dir, up, .. } => {
                *at = x.pt(*at);
                *dir = x.vec(*dir);
                *up = x.vec(*up);
            }
        }
    }
}

struct File {
    des: HashMap<usize, De>,
    order: Vec<usize>,
    params: HashMap<usize, Vec<String>>,
    /// Millimetres per file unit (IG2).
    mm: f64,
    opts: Options,
    skipped: BTreeMap<&'static str, usize>,
    /// Entities being expanded right now (a composite that holds itself stops).
    active: Vec<usize>,
    /// Entities expanded and points made so far: a small file can't make an
    /// endless amount of work or geometry.
    visits: usize,
    points: usize,
    /// A limit was reached: part of the drawing is missing, so the import is
    /// refused whole (no part comes in short of a contour).
    over: bool,
    /// Bytes of note text made so far (a note in a subfigure is made once per
    /// instance), held to `MAX_TEXT_BYTES`.
    text_bytes: usize,
    /// Each entity's damage check, done once.
    damage_seen: std::cell::RefCell<HashMap<(usize, i32), Option<&'static str>>>,
}

const MAX_TEXT_BYTES: usize = 200 << 20;

const MAX_VISITS: usize = 500_000;
const MAX_POINTS: usize = 5_000_000;

/// Read an IGES file's flat drawing as one part.
pub fn read(bytes: &[u8], opts: &Options) -> Result<Vec<ReadPart>, String> {
    let f = File::parse(bytes, opts)?;
    let mut f = f;
    let drawing = f.drawing()?;
    Ok(vec![ReadPart { drawing, meta: Meta::default(), mark_layers: vec![] }])
}

/// Split free-format parameter data into fields (Hollerith strings kept whole).
fn fields(s: &str, pd: char, rd: char) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    let mut cur = String::new();
    // Whether the field so far is spaces, digits, spaces (a Hollerith count
    // before an `H`), and that count — kept as each character comes, so no
    // character is looked at twice.
    #[derive(PartialEq)]
    enum Lead {
        Blank,
        Digits,
        After,
        Other,
    }
    let (mut lead, mut count) = (Lead::Blank, 0usize);
    while i < c.len() {
        let ch = c[i];
        if ch == pd || ch == rd {
            out.push(cur.trim().to_string());
            cur.clear();
            (lead, count) = (Lead::Blank, 0);
            i += 1;
            if ch == rd {
                return out;
            }
            continue;
        }
        if ch == 'H' && matches!(lead, Lead::Digits | Lead::After) {
            let n: usize = count;
            let end = (i + 1 + n).min(c.len());
            cur = c[i + 1..end].iter().collect();
            i = end;
            // Skip anything up to the next delimiter.
            while i < c.len() && c[i] != pd && c[i] != rd {
                i += 1;
            }
            out.push(cur.clone());
            cur.clear();
            (lead, count) = (Lead::Blank, 0);
            if i < c.len() {
                let d = c[i];
                i += 1;
                if d == rd {
                    return out;
                }
            }
            continue;
        }
        lead = match (lead, ch.is_ascii_digit(), ch.is_whitespace()) {
            (Lead::Blank, _, true) => Lead::Blank,
            (Lead::Blank | Lead::Digits, true, _) => {
                count = count.saturating_mul(10).saturating_add(ch.to_digit(10).unwrap_or(0) as usize);
                Lead::Digits
            }
            (Lead::Digits | Lead::After, _, true) => Lead::After,
            _ => Lead::Other,
        };
        cur.push(ch);
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn num(s: &str) -> Option<f64> {
    let t = s.trim().replace(['D', 'd'], "E");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().or_else(|| {
        // "1." / ".5" / "1.E3" forms.
        let t2 = if t.ends_with('.') { format!("{t}0") } else { t.replace(".E", ".0E") };
        t2.parse().ok()
    })
}

/// IG2: millimetres per unit for the Global section's units flag / name.
fn unit_mm(flag: i64, name: &str) -> f64 {
    let by_name = |n: &str| match n.trim().to_ascii_uppercase().as_str() {
        "IN" | "INCH" | "INCHES" => Some(25.4),
        "MM" => Some(1.0),
        "FT" => Some(304.8),
        "MI" => Some(1_609_344.0),
        "M" => Some(1000.0),
        "KM" => Some(1_000_000.0),
        "MIL" => Some(0.0254),
        "UM" | "MICRON" => Some(0.001),
        "CM" => Some(10.0),
        "UIN" => Some(0.0000254),
        _ => None,
    };
    match flag {
        1 => 25.4,
        2 => 1.0,
        4 => 304.8,
        5 => 1_609_344.0,
        6 => 1000.0,
        7 => 1_000_000.0,
        8 => 0.0254,
        9 => 0.001,
        10 => 10.0,
        11 => 0.0000254,
        _ => by_name(name).unwrap_or(1.0),
    }
}

fn dist(a: V3, b: V3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Segments for an arc of radius `r_mm` and sweep `sweep` within the arc smoothness.
fn arc_segments(r_mm: f64, sweep: f64) -> usize {
    crate::dxf::arc_segments(r_mm, sweep)
}

impl File {
    fn parse(bytes: &[u8], opts: &Options) -> Result<File, String> {
        let text = String::from_utf8_lossy(bytes);
        let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
        // Some writers leave out line breaks: 80-column records back to back.
        if lines.iter().filter(|l| !l.is_empty()).count() <= 1 && text.len() >= 160 {
            let t: Vec<char> = text.trim_end().chars().collect();
            lines = t.chunks(80).map(|c| c.iter().collect()).collect();
        }
        let mut g = String::new();
        let mut d_lines: Vec<(usize, String)> = vec![];
        let mut p_lines: Vec<(usize, String)> = vec![];
        let mut compressed = false;
        for l in &lines {
            let ch: Vec<char> = l.chars().collect();
            if ch.len() < 73 {
                continue;
            }
            let data: String = ch[..72].iter().collect();
            let seq = ch[73.min(ch.len())..].iter().collect::<String>().trim().parse::<usize>().unwrap_or(0);
            match ch[72] {
                'G' => g.push_str(&data),
                'D' => d_lines.push((seq, data)),
                'P' => p_lines.push((seq, data)),
                'C' => compressed = true,
                _ => {}
            }
        }
        if compressed && d_lines.is_empty() {
            return Err("This IGES file is in the compressed layout, which can't be read. Save it again as a normal (fixed 80-column) IGES file.".into());
        }
        if d_lines.is_empty() {
            return Err("This is not an IGES file (no directory section was found). Check the file, or export the drawing as IGES or DXF again.".into());
        }
        // Global section: delimiters, then units (fields 14 and 15).
        let gt = g.trim_start();
        let (mut pd, mut rd) = (',', ';');
        let mut rest = gt;
        if let Some(r) = rest.strip_prefix("1H") {
            pd = r.chars().next().unwrap_or(',');
            rest = &r[pd.len_utf8()..];
            rest = rest.strip_prefix(pd).unwrap_or(rest);
        } else {
            rest = rest.strip_prefix(',').unwrap_or(rest);
        }
        if let Some(r) = rest.trim_start().strip_prefix("1H") {
            rd = r.chars().next().unwrap_or(';');
        }
        let gf = fields(gt, pd, rd);
        let gnum = |i: usize| gf.get(i - 1).and_then(|s| num(s));
        let flag = gnum(14).unwrap_or(2.0) as i64;
        let mm = unit_mm(flag, gf.get(14).map(String::as_str).unwrap_or(""));

        let mut des = HashMap::new();
        let mut order = vec![];
        let field = |s: &str, k: usize| -> String { s.chars().skip(k * 8).take(8).collect::<String>() };
        let int = |s: &str, k: usize| -> i64 { field(s, k).trim().parse::<i64>().unwrap_or(0) };
        let mut i = 0;
        while i + 1 < d_lines.len() {
            let (seq, a) = &d_lines[i];
            let (_, b) = &d_lines[i + 1];
            i += 2;
            let status: Vec<u32> = {
                let s = field(a, 8);
                let s: Vec<char> = format!("{:0>8}", s.trim().replace(' ', "0")).chars().collect();
                // By characters, not bytes: a stray non-ASCII byte must not split one.
                (0..4).map(|k| s.iter().skip(k * 2).take(2).collect::<String>().parse().unwrap_or(0)).collect()
            };
            let de = De {
                typ: int(a, 0) as i32,
                level: int(a, 4),
                xform: int(a, 6).max(0) as usize,
                blank: status[0],
                sub: status[1],
                use_: status[2],
                form: int(b, 4) as i32,
            };
            des.insert(*seq, de);
            order.push(*seq);
        }
        // Parameter data: columns 1–64, grouped by the directory pointer in 66–72.
        let mut raw: HashMap<usize, String> = HashMap::new();
        for (_, l) in &p_lines {
            let data: String = l.chars().take(64).collect();
            let back = l.chars().skip(64).take(8).collect::<String>().trim().parse::<usize>().unwrap_or(0);
            raw.entry(back).or_default().push_str(&data);
        }
        let params = raw.into_iter().map(|(k, v)| (k, fields(&v, pd, rd))).collect();
        Ok(File { des, order, params, mm, opts: opts.clone(), skipped: BTreeMap::new(), active: vec![], visits: 0, points: 0, over: false, text_bytes: 0, damage_seen: Default::default() })
    }

    fn p(&self, seq: usize) -> &[String] {
        self.params.get(&seq).map(|v| v.as_slice()).unwrap_or(&[])
    }
    /// Parameter `i` (1 = first after the entity type).
    fn f(&self, seq: usize, i: usize) -> f64 {
        self.p(seq).get(i).and_then(|s| num(s)).unwrap_or(0.0)
    }
    fn ptr(&self, seq: usize, i: usize) -> usize {
        self.f(seq, i).abs() as usize
    }
    /// A count at parameter `i` of things that each take `per` parameters from
    /// `start` on, cut to what the record really holds: a damaged count can't
    /// make up points that aren't there.
    fn count(&self, seq: usize, i: usize, start: usize, per: usize) -> usize {
        let have = self.p(seq).len().saturating_sub(start) / per.max(1);
        (self.f(seq, i).max(0.0) as usize).min(have)
    }

    /// IG4: the transformation of a 124 entity, following its own parent matrix.
    fn xform(&self, seq: usize, depth: u32) -> Xf {
        if seq == 0 || depth > 16 {
            return Xf::ID;
        }
        let Some(de) = self.des.get(&seq) else { return Xf::ID };
        if de.typ != 124 {
            return Xf::ID;
        }
        let v: Vec<f64> = (1..=12).map(|i| self.f(seq, i)).collect();
        let own = Xf { r: [[v[0], v[1], v[2]], [v[4], v[5], v[6]], [v[8], v[9], v[10]]], t: [v[3], v[7], v[11]] };
        self.xform(de.xform, depth + 1).after(&own)
    }

    /// IG4: a transformation pointer that leads to whole 124 entities all the way
    /// up (0 = none). One pointing at an entity the file doesn't have, at another
    /// kind of entity, at a damaged matrix or round in a circle can't place the
    /// geometry, which is then left out and named (never drawn unmoved).
    fn xform_ok(&self, seq: usize, depth: u32) -> bool {
        if seq == 0 {
            return true;
        }
        depth <= 16 && self.des.get(&seq).is_some_and(|d| d.typ == 124 && self.damage(seq, 124).is_none() && self.xform_ok(d.xform, depth + 1))
    }

    /// IG4: the matrix an entity is drawn with. A matrix that stands the drawing
    /// on edge (its third row is the X or Y axis) has its columns turned round so
    /// the drawing lies in the XY plane again.
    fn entity_xform(&self, seq: usize) -> Xf {
        let mut x = self.xform(seq, 0);
        let r3 = x.r[2];
        let is = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let shift = |row: [f64; 3], by: usize| [row[by % 3], row[(by + 1) % 3], row[(by + 2) % 3]];
        let by = if is(r3[0], 1.0) && is(r3[1], 0.0) && is(r3[2], 0.0) {
            1
        } else if is(r3[0], 0.0) && is(r3[1], 1.0) && is(r3[2], 0.0) {
            2
        } else {
            0
        };
        if by > 0 {
            for row in x.r.iter_mut() {
                *row = shift(*row, by);
            }
        }
        x
    }

    fn layer(level: i64, parent: &str) -> String {
        if level == 0 && !parent.is_empty() { parent.to_string() } else { level.abs().to_string() }
    }

    fn skip(&mut self, what: &'static str) {
        *self.skipped.entry(what).or_insert(0) += 1;
    }

    /// The drawing items of one entity, with its own matrix applied. `k` is mm per
    /// unit at this depth (for the arc smoothness); `parent` the inherited layer.
    /// An entity's drawing items. One that is already being expanded higher up
    /// (a composite or subfigure holding itself) gives nothing, and the whole
    /// file has a budget of entities and points.
    fn items(&mut self, seq: usize, k: f64, parent: &str, depth: u32) -> Vec<Item> {
        if self.visits >= MAX_VISITS || self.points >= MAX_POINTS {
            self.over = true;
            return vec![];
        }
        if depth > 24 {
            self.over = true;
            return vec![];
        }
        // A composite or subfigure inside itself: the loop draws nothing more.
        if self.active.contains(&seq) {
            return vec![];
        }
        self.visits += 1;
        self.active.push(seq);
        let out = self.items_of(seq, k, parent, depth);
        self.active.pop();
        self.points += out.iter().map(|it| if let Item::Curve { pts, .. } = it { pts.len() } else { 1 }).sum::<usize>();
        if self.points > MAX_POINTS {
            self.over = true;
        }
        out
    }

    /// IG3: an entity whose numbers can't be read, or that holds fewer values
    /// than it says, is damaged: left out and named in the notes, never drawn
    /// with zeros or with points missing. Returns the reason, if any.
    fn damage(&self, seq: usize, typ: i32) -> Option<&'static str> {
        // Checked once per entity: one referenced many times isn't rescanned.
        if let Some(r) = self.damage_seen.borrow().get(&(seq, typ)) {
            return *r;
        }
        let r = self.damage_of(seq, typ);
        self.damage_seen.borrow_mut().insert((seq, typ), r);
        r
    }

    fn damage_of(&self, seq: usize, typ: i32) -> Option<&'static str> {
        let p = self.p(seq);
        let numeric = matches!(typ, 100 | 102 | 104 | 106 | 110 | 112 | 124 | 126 | 408);
        if numeric && p.iter().skip(1).any(|v| !v.trim().is_empty() && num(v).is_none_or(|x| !x.is_finite())) {
            return Some("damaged entities");
        }
        let n = |i: usize| self.f(seq, i).max(0.0) as usize;
        let need = match typ {
            102 => 2usize.saturating_add(n(1)),
            106 => {
                let w = match self.f(seq, 1) as i64 {
                    1 => 2,
                    3 => 6,
                    _ => 3,
                };
                (if w == 2 { 4usize } else { 3 }).saturating_add(n(2).saturating_mul(w))
            }
            112 => 6usize.saturating_add(n(4).saturating_mul(13)),
            126 => {
                let (kk, m) = (self.f(seq, 1) as i64, self.f(seq, 2) as i64);
                if kk < 0 || m < 1 || m > kk.max(1) || m > 30 {
                    return Some("damaged entities");
                }
                let kk = kk as usize;
                let need = 7usize.saturating_add(kk + m as usize + 2).saturating_add(kk.saturating_add(1).saturating_mul(4)).saturating_add(2);
                // Knots that go down or weights that aren't positive draw nonsense.
                if need <= p.len() {
                    let nk = kk + m as usize + 2;
                    let knots: Vec<f64> = (0..nk).map(|i| self.f(seq, 7 + i)).collect();
                    // A polynomial spline (PROP3 = 1) has equal weights and they
                    // aren't used; a rational one needs them positive.
                    let polynomial = self.f(seq, 5) == 1.0;
                    let weights_ok = polynomial || (0..=kk).all(|i| self.f(seq, 7 + nk + i) > 0.0);
                    if knots.windows(2).any(|w| w[1] < w[0]) || !weights_ok {
                        return Some("damaged entities");
                    }
                }
                need
            }
            212 => 2usize.saturating_add(n(1).saturating_mul(12)),
            308 => 4usize.saturating_add(n(3)),
            // Fixed-size entities: every coordinate written (a value may be left
            // empty, which is 0, but not cut off at the end).
            100 => 8,
            104 => 12,
            110 => 7,
            124 => 13,
            408 => 2,
            _ => 0,
        };
        (need > p.len()).then_some("damaged entities")
    }

    fn items_of(&mut self, seq: usize, k: f64, parent: &str, depth: u32) -> Vec<Item> {
        // A composite member or subfigure that points to an entity the file
        // doesn't have would lose its contours: named (pointer 0 means none).
        let Some(de) = self.des.get(&seq).cloned() else {
            if seq != 0 {
                self.skip("references to entities that aren't in the file");
            }
            return vec![];
        };
        if let Some(why) = self.damage(seq, de.typ) {
            self.skip(why);
            return vec![];
        }
        if !self.xform_ok(de.xform, 0) {
            self.skip("entities whose placement (matrix) isn't in the file");
            return vec![];
        }
        // A spline of more than 100 000 pieces is over the import's limit: the
        // file is refused whole.
        if (de.typ == 112 && self.f(seq, 4) > 100_000.0) || (de.typ == 126 && self.f(seq, 1) > 100_000.0) {
            self.over = true;
            return vec![];
        }
        let layer = if de.level < 0 {
            // A list of levels (406 form 1): use the first one.
            let p = de.level.unsigned_abs() as usize;
            Self::layer(self.f(p, 2) as i64, parent)
        } else {
            Self::layer(de.level, parent)
        };
        let xf = self.entity_xform(de.xform);
        let kk = k * xf.scale_xy().max(1e-12);
        let tol = ARC_CHORD_ERR / kk;
        let mut out: Vec<Item> = vec![];
        let curve = |pts: Vec<V3>, closed: bool| Item::Curve { pts, closed, layer: layer.clone() };
        match de.typ {
            110 => {
                if de.form == 0 {
                    let v: Vec<f64> = (1..=6).map(|i| self.f(seq, i)).collect();
                    out.push(curve(vec![[v[0], v[1], v[2]], [v[3], v[4], v[5]]], false));
                }
            }
            100 => {
                let v: Vec<f64> = (1..=7).map(|i| self.f(seq, i)).collect();
                let (z, c, s, e) = (v[0], [v[1], v[2]], [v[3], v[4]], [v[5], v[6]]);
                let r = ((s[0] - c[0]).powi(2) + (s[1] - c[1]).powi(2)).sqrt();
                if r > 0.0 {
                    let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
                    let a1 = (e[1] - c[1]).atan2(e[0] - c[0]);
                    // IG3: start and end within 0.01 mm make a full circle.
                    let full = ((s[0] - e[0]).powi(2) + (s[1] - e[1]).powi(2)).sqrt() * kk <= crate::dxf::JOIN_TOL;
                    let mut sweep = a1 - a0;
                    while sweep <= 0.0 {
                        sweep += std::f64::consts::TAU;
                    }
                    if full {
                        sweep = std::f64::consts::TAU;
                    }
                    let n = arc_segments(r * kk, sweep);
                    let mut pts: Vec<V3> = (0..=n)
                        .map(|i| {
                            let a = a0 + sweep * i as f64 / n as f64;
                            [c[0] + r * a.cos(), c[1] + r * a.sin(), z]
                        })
                        .collect();
                    pts[0] = [s[0], s[1], z];
                    if full {
                        pts.pop();
                    } else {
                        pts[n] = [e[0], e[1], z];
                    }
                    out.push(curve(pts, full));
                } else {
                    // An arc with no radius: damaged, named.
                    self.skip("damaged entities");
                }
            }
            104 => out.extend(self.conic(seq, de.form, kk, &layer)),
            106 => {
                let ip = self.f(seq, 1) as i64;
                let n = match ip {
                    1 => self.count(seq, 2, 4, 2),
                    3 => self.count(seq, 2, 3, 6),
                    _ => self.count(seq, 2, 3, 3),
                };
                match de.form {
                    // IG3: point lists are drawn joined up, like polylines.
                    0..=3 | 11..=13 | 63 => {
                        let mut pts = vec![];
                        if ip == 1 {
                            let z = self.f(seq, 3);
                            for i in 0..n {
                                pts.push([self.f(seq, 4 + 2 * i), self.f(seq, 5 + 2 * i), z]);
                            }
                        } else {
                            let w = if ip == 3 { 6 } else { 3 };
                            for i in 0..n {
                                let b = 3 + w * i;
                                pts.push([self.f(seq, b), self.f(seq, b + 1), self.f(seq, b + 2)]);
                            }
                        }
                        let mut closed = de.form == 63;
                        if pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) <= 1e-9 {
                            pts.pop();
                            closed = true;
                        }
                        if pts.len() >= 2 {
                            out.push(curve(pts, closed));
                        }
                    }
                    // Forms 20 and up are annotation (centre lines, hatching…).
                    _ => {}
                }
            }
            112 => {
                let pts = self.spline112(seq, tol, kk, MAX_POINTS.saturating_sub(self.points));
                if pts.len() >= 2 {
                    out.push(curve(pts, false));
                }
            }
            126 => {
                let pts = self.nurbs(seq, tol, kk, MAX_POINTS.saturating_sub(self.points));
                if pts.len() >= 2 {
                    out.push(curve(pts, false));
                }
            }
            102 => {
                let n = self.count(seq, 1, 2, 1);
                let mut pts: Vec<V3> = vec![];
                for i in 0..n {
                    let c = self.ptr(seq, 2 + i);
                    for it in self.items(c, kk, &layer, depth + 1) {
                        if let Item::Curve { pts: mut q, closed, .. } = it {
                            if closed && !q.is_empty() {
                                q.push(q[0]);
                            }
                            if let (Some(last), Some(first)) = (pts.last(), q.first()) {
                                // Constituents may be stored either way round.
                                if pts.len() == q.len().min(pts.len()) && i == 1 && dist(*last, *first) > dist(pts[0], *first).min(dist(*last, q[q.len() - 1])) {
                                    if dist(pts[0], *first) < dist(*last, q[q.len() - 1]) {
                                        pts.reverse();
                                    } else {
                                        q.reverse();
                                    }
                                } else if dist(*last, q[q.len() - 1]) < dist(*last, *first) {
                                    q.reverse();
                                }
                                if dist(*pts.last().unwrap(), q[0]) <= 1e-9 * (1.0 + q[0][0].abs()) {
                                    q.remove(0);
                                }
                            }
                            pts.extend(q);
                        }
                    }
                }
                let mut closed = false;
                if pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) * kk <= crate::dxf::JOIN_TOL {
                    pts.pop();
                    closed = true;
                }
                if pts.len() >= 2 {
                    out.push(curve(pts, closed));
                }
            }
            212 => {
                // Every line of the note (the record holds them all, `damage`);
                // each copy's bytes count toward the file's text budget first.
                let ns = self.count(seq, 1, 2, 12);
                for s in 0..ns {
                    let b = 2 + s * 12;
                    let ht = self.f(seq, b + 2);
                    let ang = self.f(seq, b + 5);
                    let at = [self.f(seq, b + 8), self.f(seq, b + 9), self.f(seq, b + 10)];
                    let len = self.p(seq).get(b + 11).map_or(0, |t| t.len());
                    self.text_bytes = self.text_bytes.saturating_add(len);
                    if self.text_bytes > MAX_TEXT_BYTES {
                        self.over = true;
                        return vec![];
                    }
                    let text = self.p(seq).get(b + 11).map(|t| t.trim().to_string()).unwrap_or_default();
                    if !text.is_empty() {
                        out.push(Item::Note {
                            text,
                            at,
                            dir: [ang.cos(), ang.sin(), 0.0],
                            up: [-ang.sin() * ht, ang.cos() * ht, 0.0],
                            layer: layer.clone(),
                        });
                    }
                }
            }
            408 => {
                let def = self.ptr(seq, 1);
                let t = [self.f(seq, 2), self.f(seq, 3), self.f(seq, 4)];
                let s = match self.p(seq).get(5).and_then(|v| num(v)) {
                    Some(s) if s != 0.0 => s,
                    _ => 1.0,
                };
                if !self.des.get(&def).is_some_and(|d| d.typ == 308) {
                    self.skip("references to entities that aren't in the file");
                } else {
                    if let Some(why) = self.damage(def, 308) {
                        self.skip(why);
                        return vec![];
                    }
                    let n = self.count(def, 3, 4, 1);
                    let place = Xf { r: [[s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, s]], t };
                    // Every member: the file-wide budget refuses a file too big.
                    for i in 0..n {
                        let m = self.ptr(def, 4 + i);
                        for mut it in self.items(m, kk * s.abs(), &layer, depth + 1) {
                            it.map(&place);
                            out.push(it);
                        }
                    }
                }
            }
            0 | 116 | 124 | 202..=230 | 302..=322 | 402..=430 => {}
            108 | 114 | 118 | 120 | 122 | 128 | 140 | 143 | 144 | 190..=198 => self.skip("surfaces"),
            141 | 142 => self.skip("curves on surfaces"),
            150..=186 | 502..=514 => self.skip("solids"),
            _ => self.skip("other entities"),
        }
        for it in &mut out {
            it.map(&xf);
        }
        out
    }

    /// Conic arc (104). Ellipses are drawn; parabolas and hyperbolas as chords.
    fn conic(&mut self, seq: usize, _form: i32, k: f64, layer: &str) -> Vec<Item> {
        let v: Vec<f64> = (1..=11).map(|i| self.f(seq, i)).collect();
        let (a, b, c, d, e, f, z) = (v[0], v[1], v[2], v[3], v[4], v[5], v[6]);
        let (s, en) = ([v[7], v[8]], [v[9], v[10]]);
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            self.skip("parabolas/hyperbolas (drawn as straight lines)");
            return vec![Item::Curve { pts: vec![[s[0], s[1], z], [en[0], en[1], z]], closed: false, layer: layer.to_string() }];
        }
        // Centre, axis direction and semi-axes of the ellipse.
        let det = 4.0 * a * c - b * b;
        let x0 = (b * e - 2.0 * c * d) / det;
        let y0 = (b * d - 2.0 * a * e) / det;
        let th = 0.5 * b.atan2(a - c);
        let (ct, st) = (th.cos(), th.sin());
        let ap = a * ct * ct + b * ct * st + c * st * st;
        let cp = a * st * st - b * ct * st + c * ct * ct;
        let fp = a * x0 * x0 + b * x0 * y0 + c * y0 * y0 + d * x0 + e * y0 + f;
        if ap == 0.0 || cp == 0.0 || -fp / ap <= 0.0 || -fp / cp <= 0.0 {
            // No real ellipse (e.g. x² + y² + 1 = 0): damaged, named.
            self.skip("damaged entities");
            return vec![];
        }
        let (ra, rb) = ((-fp / ap).sqrt(), (-fp / cp).sqrt());
        let param = |p: [f64; 2]| {
            let (dx, dy) = (p[0] - x0, p[1] - y0);
            let (u, w) = (dx * ct + dy * st, -dx * st + dy * ct);
            (w / rb).atan2(u / ra)
        };
        let (t0, t1) = (param(s), param(en));
        let full = ((s[0] - en[0]).powi(2) + (s[1] - en[1]).powi(2)).sqrt() <= 1e-9 * ra.max(1.0);
        let mut sweep = t1 - t0;
        while sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
        }
        if full {
            sweep = std::f64::consts::TAU;
        }
        // Segment count from the tighter end of the ellipse.
        let rmin = ra.min(rb).max(1e-12);
        let rcurv = (ra.max(rb).powi(2) / rmin).max(rmin);
        let n = (arc_segments(rmin.min(rcurv) * k, sweep) * 2).clamp(4, 2000);
        let mut pts: Vec<V3> = (0..=n)
            .map(|i| {
                let t = t0 + sweep * i as f64 / n as f64;
                let (u, w) = (ra * t.cos(), rb * t.sin());
                [x0 + u * ct - w * st, y0 + u * st + w * ct, z]
            })
            .collect();
        if full {
            pts.pop();
        } else {
            pts[0] = [s[0], s[1], z];
            pts[n] = [en[0], en[1], z];
        }
        vec![Item::Curve { pts, closed: full, layer: layer.to_string() }]
    }

    /// Parametric spline (112): cubic segments over the break points.
    /// Parametric spline (112); stops after more than `room` points (refused).
    fn spline112(&self, seq: usize, tol: f64, k: f64, room: usize) -> Vec<V3> {
        // n segments take n + 1 break points and 12 coefficients each.
        let n = self.p(seq).len().saturating_sub(6).checked_div(13).map_or(0, |have| self.count(seq, 4, 0, 1).min(have));
        if n == 0 || n > 100_000 {
            return vec![];
        }
        let t: Vec<f64> = (0..=n).map(|i| self.f(seq, 5 + i)).collect();
        let base = 5 + n + 1;
        let max_seg = self.opts.spline_segment / k;
        let mut pts: Vec<V3> = vec![];
        for i in 0..n {
            let c: Vec<f64> = (0..12).map(|j| self.f(seq, base + 12 * i + j)).collect();
            let len = t[i + 1] - t[i];
            let ev = |u: f64| {
                let s = u * len;
                [
                    c[0] + s * (c[1] + s * (c[2] + s * c[3])),
                    c[4] + s * (c[5] + s * (c[6] + s * c[7])),
                    c[8] + s * (c[9] + s * (c[10] + s * c[11])),
                ]
            };
            if pts.len() > room {
                break;
            }
            let seg = flatten(&ev, 0.0, 1.0, tol, max_seg, room - pts.len());
            let skip = if pts.is_empty() { 0 } else { 1 };
            pts.extend(seg.into_iter().skip(skip));
        }
        pts
    }

    /// Rational B-spline curve (126).
    /// Rational B-spline (126); stops after more than `room` points (refused).
    fn nurbs(&self, seq: usize, tol: f64, k: f64, room: usize) -> Vec<V3> {
        let kk = self.f(seq, 1) as i64;
        let m = self.f(seq, 2) as i64;
        // kk + 1 control points take at least 4 parameters each (weight, x, y, z).
        if kk < 0 || m < 1 || kk > 100_000 || m > 30 || m > kk || (kk as usize + 1) * 4 > self.p(seq).len() {
            return vec![];
        }
        let (kk, m) = (kk as usize, m as usize);
        let nk = kk + m + 2;
        let knots: Vec<f64> = (0..nk).map(|i| self.f(seq, 7 + i)).collect();
        let wb = 7 + nk;
        let w: Vec<f64> = (0..=kk).map(|i| self.f(seq, wb + i)).collect();
        let cb = wb + kk + 1;
        let cp: Vec<V3> = (0..=kk).map(|i| [self.f(seq, cb + 3 * i), self.f(seq, cb + 3 * i + 1), self.f(seq, cb + 3 * i + 2)]).collect();
        let vb = cb + 3 * (kk + 1);
        let (mut v0, mut v1) = (self.f(seq, vb), self.f(seq, vb + 1));
        let (lo, hi) = (knots[m], knots[kk + 1]);
        if !(v0 < v1) {
            (v0, v1) = (lo, hi);
        }
        let v0 = v0.max(lo);
        let v1 = v1.min(hi);
        if !(v0 < v1) {
            return vec![];
        }
        let ev = |u: f64| -> V3 {
            // de Boor in homogeneous coordinates.
            // Knot span by binary search (the knots never go down, see `damage`):
            // sampling stays linear in the spline's length.
            let span = (m + knots[m + 1..=kk].partition_point(|&x| x <= u)).min(kk);
            let mut d: Vec<[f64; 4]> = (0..=m)
                .map(|j| {
                    let i = span - m + j;
                    let wi = if w[i] == 0.0 { 1.0 } else { w[i] };
                    [cp[i][0] * wi, cp[i][1] * wi, cp[i][2] * wi, wi]
                })
                .collect();
            for r in 1..=m {
                for j in (r..=m).rev() {
                    let i = span - m + j;
                    let den = knots[i + m + 1 - r] - knots[i];
                    let a = if den.abs() < 1e-300 { 0.0 } else { (u - knots[i]) / den };
                    for c in 0..4 {
                        d[j][c] = (1.0 - a) * d[j - 1][c] + a * d[j][c];
                    }
                }
            }
            let h = d[m];
            let wq = if h[3].abs() < 1e-300 { 1.0 } else { h[3] };
            [h[0] / wq, h[1] / wq, h[2] / wq]
        };
        // Flatten span by span so corners at repeated knots are kept.
        let mut breaks: Vec<f64> = vec![v0];
        for &t in &knots {
            if t > v0 && t < v1 && t > *breaks.last().unwrap() + 1e-12 {
                breaks.push(t);
            }
        }
        breaks.push(v1);
        let max_seg = self.opts.spline_segment / k;
        let mut pts: Vec<V3> = vec![];
        for w2 in breaks.windows(2) {
            if pts.len() > room {
                break;
            }
            let seg = flatten(&ev, w2[0], w2[1], tol, max_seg, room - pts.len());
            let skip = if pts.is_empty() { 0 } else { 1 };
            pts.extend(seg.into_iter().skip(skip));
        }
        pts
    }

    /// Top-level entities in model space (IG4), as one drawing.
    fn drawing(&mut self) -> Result<Drawing, String> {
        // IG6: entities used inside others are drawn through them only.
        let mut child: HashSet<usize> = HashSet::new();
        for (&seq, de) in &self.des {
            match de.typ {
                102 => {
                    let n = self.count(seq, 1, 2, 1);
                    child.extend((0..n).map(|i| self.ptr(seq, 2 + i)));
                }
                308 => {
                    let n = self.count(seq, 3, 4, 1);
                    child.extend((0..n).map(|i| self.ptr(seq, 4 + i)));
                }
                // Boundary curves of surfaces are not drawing lines (IG6): only the
                // pointer fields of each entity, never its flags or counts.
                141 => {
                    // TYPE, PREF, SPTR, N, then per boundary CRVPT, SENSE, K, PSCPT×K.
                    child.insert(self.ptr(seq, 3));
                    let mut at = 5;
                    for _ in 0..self.count(seq, 4, 5, 3) {
                        child.insert(self.ptr(seq, at));
                        let k = (self.f(seq, at + 2).max(0.0) as usize).min(self.p(seq).len());
                        child.extend((0..k).map(|j| self.ptr(seq, at + 3 + j)));
                        at = at.saturating_add(3 + k);
                    }
                }
                // CRTN, SPTR, BPTR, CPTR, PREF.
                142 => child.extend([self.ptr(seq, 2), self.ptr(seq, 3), self.ptr(seq, 4)]),
                // TYPE, SPTR, N, BDPT×N.
                143 => {
                    child.insert(self.ptr(seq, 2));
                    child.extend((0..self.count(seq, 3, 4, 1)).map(|i| self.ptr(seq, 4 + i)));
                }
                // PTS, N1, N2, PTO, PTI×N2.
                144 => {
                    child.extend([self.ptr(seq, 1), self.ptr(seq, 4)]);
                    child.extend((0..self.count(seq, 3, 5, 1)).map(|i| self.ptr(seq, 5 + i)));
                }
                _ => {}
            }
        }
        let order = self.order.clone();
        let mut items: Vec<Item> = vec![];
        let mut counted = 0;
        for seq in order {
            let de = self.des[&seq].clone();
            if child.contains(&seq) || (self.opts.skip_hidden && de.blank == 1) || de.sub == 1 || de.sub == 3 || de.use_ == 5 || de.typ == 308 {
                continue;
            }
            let its = self.items(seq, self.mm, "", 0);
            if its.is_empty() {
                continue;
            }
            counted += 1;
            items.extend(its);
        }
        // IG8: a flat drawing standing on edge is laid into the XY plane.
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for it in &items {
            if let Item::Curve { pts, .. } = it {
                for p in pts {
                    for a in 0..3 {
                        lo[a] = lo[a].min(p[a]);
                        hi[a] = hi[a].max(p[a]);
                    }
                }
            }
        }
        let ext: Vec<f64> = (0..3).map(|a| (hi[a] - lo[a]).max(0.0)).collect();
        let big = ext.iter().cloned().fold(0.0, f64::max);
        let axes = if big > 0.0 && ext[0].min(ext[1]) <= 1e-6 * big && ext[2] > 1e-6 * big {
            if ext[0] <= ext[1] { [1, 2] } else { [0, 2] }
        } else {
            [0, 1]
        };
        let mm = self.mm;
        let flat = |p: V3| [p[axes[0]] * mm, p[axes[1]] * mm];
        let mut d = Drawing::default();
        for it in items {
            match it {
                Item::Curve { pts, closed, layer } => {
                    let mut q: Vec<[f64; 2]> = pts.into_iter().map(flat).collect();
                    q.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
                    let mut closed = closed;
                    if q.len() > 2 && (q[0][0] - q[q.len() - 1][0]).abs() < 1e-9 && (q[0][1] - q[q.len() - 1][1]).abs() < 1e-9 {
                        q.pop();
                        closed = true;
                    }
                    if q.len() >= 2 {
                        let closed = closed && q.len() > 2;
                        d.pieces.push(Piece { layer, pts: q, closed });
                    }
                }
                Item::Note { text, at, dir, up, layer } => {
                    let a = flat(at);
                    let dv = [dir[axes[0]], dir[axes[1]]];
                    let h = (up[0] * up[0] + up[1] * up[1] + up[2] * up[2]).sqrt() * mm;
                    if h > 0.0 {
                        d.texts.push((layer, text, a, h, dv[1].atan2(dv[0]).to_degrees()));
                    }
                }
            }
        }
        if self.over {
            return Err("This IGES drawing is bigger than the import can take whole (more than 5 million points, 500 000 entities or 200 MB of note text). \
                        Nothing was imported, so no contour goes missing. Simplify or split it, or export it as DXF."
                .into());
        }
        d.report.entities = counted;
        d.report.skipped = self.skipped.iter().map(|(k, v)| format!("{k} ({v})")).collect();
        if d.pieces.is_empty() {
            if self.skipped.contains_key("surfaces") || self.skipped.contains_key("solids") {
                return Err("This IGES file holds a 3D model (surfaces or solids), not a flat drawing. \
                            Export the part's flat outline (lines and arcs) as IGES or DXF instead."
                    .into());
            }
            return Err("No lines, arcs or curves were found in this IGES file.".into());
        }
        Ok(d)
    }
}

/// IG7: points along a curve within `tol` of it, no segment longer than `max_seg`.
/// Points along `f` within `tol` and no further apart than `max_seg`; stops
/// adding once it has more than `limit` points.
fn flatten(f: &dyn Fn(f64) -> V3, t0: f64, t1: f64, tol: f64, max_seg: f64, limit: usize) -> Vec<V3> {
    #[allow(clippy::too_many_arguments)]
    fn rec(f: &dyn Fn(f64) -> V3, a: f64, pa: V3, b: f64, pb: V3, tol: f64, max_seg: f64, depth: u32, out: &mut Vec<V3>, limit: usize) {
        if out.len() > limit {
            return;
        }
        let m = 0.5 * (a + b);
        let pm = f(m);
        let q1 = f(a + 0.25 * (b - a));
        let q3 = f(a + 0.75 * (b - a));
        let chord_dev = |p: V3, t: f64| {
            let c = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t, pa[2] + (pb[2] - pa[2]) * t];
            dist(p, c)
        };
        let dev = chord_dev(pm, 0.5).max(chord_dev(q1, 0.25)).max(chord_dev(q3, 0.75));
        if depth >= 14 || (dev <= tol && dist(pa, pb) <= max_seg) {
            out.push(pb);
            return;
        }
        rec(f, a, pa, m, pm, tol, max_seg, depth + 1, out, limit);
        rec(f, m, pm, b, pb, tol, max_seg, depth + 1, out, limit);
    }
    let mut out = vec![f(t0)];
    // Start from a few pieces so S-bends are not missed.
    let n = 4;
    let mut prev = (t0, out[0]);
    for i in 1..=n {
        let t = t0 + (t1 - t0) * i as f64 / n as f64;
        let p = f(t);
        rec(f, prev.0, prev.1, t, p, tol, max_seg, 0, &mut out, limit);
        prev = (t, p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Lay out an IGES file from global fields and (type, level, xform, status, form, params).
    fn make(global: &str, ents: &[(i32, i64, usize, &str, i32, &str)]) -> String {
        let rec = |data: &str, s: char, n: usize| format!("{data:<72}{s}{n:>7}\n");
        let mut out = rec("DEMO IGES", 'S', 1);
        let gl: Vec<String> = global.as_bytes().chunks(72).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
        for (i, g) in gl.iter().enumerate() {
            out.push_str(&rec(g, 'G', i + 1));
        }
        let mut d = String::new();
        let mut p = String::new();
        let mut pl = 1;
        for (i, (t, lev, xf, st, form, par)) in ents.iter().enumerate() {
            let de = 2 * i + 1;
            let chunks: Vec<String> = par.as_bytes().chunks(64).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
            d.push_str(&rec(&format!("{t:>8}{pl:>8}{:>8}{:>8}{lev:>8}{:>8}{xf:>8}{:>8}{st:>8}", 0, 1, 0, 0), 'D', de));
            d.push_str(&rec(&format!("{t:>8}{:>8}{:>8}{:>8}{form:>8}", 0, 0, chunks.len()), 'D', de + 1));
            for c in &chunks {
                p.push_str(&format!("{c:<64}{de:>8}P{pl:>7}\n"));
                pl += 1;
            }
        }
        out.push_str(&d);
        out.push_str(&p);
        out.push_str(&rec("S      1G      1D      1P      1", 'T', 1));
        out
    }

    const MM: &str = "1H,,1H;,4HDEMO,8HDEMO.IGS,4HDEMO,4HDEMO,32,38,6,308,15,4HDEMO,1.0,2,2HMM,1,0.01,13H260101.120000,0.001,1000.0,4HDEMO,4HDEMO,11,0;";
    const INCH: &str = "1H,,1H;,4HDEMO,8HDEMO.IGS,4HDEMO,4HDEMO,32,38,6,308,15,4HDEMO,1.0,1,4HINCH,1,0.01,13H260101.120000,0.001,1000.0,4HDEMO,4HDEMO,11,0;";

    fn geo(text: &str) -> crate::dxf::CustomGeo {
        let parts = read(text.as_bytes(), &Options::default()).unwrap();
        crate::dxf::build(&parts[0].drawing, &BTreeMap::new()).unwrap().0
    }

    fn square(level: i64) -> Vec<(i32, i64, usize, &'static str, i32, &'static str)> {
        vec![
            (110, level, 0, "00000000", 0, "110,0.,0.,0.,100.,0.,0.;"),
            (110, level, 0, "00000000", 0, "110,100.,0.,0.,100.,50.,0.;"),
            (110, level, 0, "00000000", 0, "110,100.,50.,0.,0.,50.,0.;"),
            (110, level, 0, "00000000", 0, "110,0.,50.,0.,0.,0.,0.;"),
        ]
    }

    #[test]
    fn lines_and_a_full_circle_make_a_plate_with_a_hole() {
        let mut e = square(3);
        e.push((100, 3, 0, "00000000", 0, "100,0.,50.,25.,60.,25.,60.,25.;"));
        let g = geo(&make(MM, &e));
        assert!((g.w - 100.0).abs() < 1e-9 && (g.h - 50.0).abs() < 1e-9);
        assert_eq!(g.holes.len(), 1);
        let a = crate::nest::area(&g.holes[0]);
        assert!((a - std::f64::consts::PI * 100.0).abs() < 0.02 * a, "{a}");
    }

    #[test]
    fn counts_larger_than_the_record_are_cut_to_it() {
        // A copious-data entity claiming a trillion points (with two real ones)
        // and a composite curve claiming a trillion members: read at once.
        let mut e = square(3);
        e.push((106, 3, 0, "00000000", 1, "106,1,1000000000000,0.,10.,10.,20.,10.;"));
        e.push((102, 3, 0, "00000000", 0, "102,1000000000000,1;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let longest = parts[0].drawing.pieces.iter().map(|p| p.pts.len()).max().unwrap();
        assert!(longest < 1000, "{longest}");
    }

    #[test]
    fn composites_that_hold_themselves_end_at_once() {
        // Entity 9 (a composite) lists itself twice and a line; entity 11 lists 9 three times.
        let mut e = square(3);
        e.push((102, 3, 0, "00000000", 0, "102,3,9,9,1;"));
        e.push((102, 3, 0, "00000000", 0, "102,3,9,9,9;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let total: usize = parts[0].drawing.pieces.iter().map(|p| p.pts.len()).sum();
        assert!(total < 100, "{total}");
    }

    #[test]
    fn damaged_entities_are_named_not_drawn_short() {
        // A closed point list (106 form 63) saying 4 corners but holding 3, and a
        // line with an end that isn't a number: both left out and named.
        let mut e = square(3);
        e.push((106, 3, 0, "00000000", 63, "106,1,4,0.,10.,10.,20.,10.,20.,20.;"));
        e.push((110, 3, 0, "00000000", 0, "110,0.,0.,0.,ABC,0.,0.;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts[0].drawing.pieces.len(), 4);
        assert!(parts[0].drawing.report.skipped.iter().any(|s| s.contains("damaged entities (2)")), "{:?}", parts[0].drawing.report.skipped);
    }

    #[test]
    fn references_to_missing_entities_and_impossible_ellipses_are_named() {
        let mut e = square(3);
        e.push((408, 3, 0, "00000000", 0, "408,999;"));
        e.push((102, 3, 0, "00000000", 0, "102,1,999;"));
        e.push((104, 3, 0, "00000000", 1, "104,1.,0.,1.,0.,0.,1.,0.,1.,0.,1.,0.;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let sk = &parts[0].drawing.report.skipped;
        assert!(sk.iter().any(|s| s.contains("aren't in the file")), "{sk:?}");
        assert!(sk.iter().any(|s| s.contains("damaged entities")), "{sk:?}");
    }

    #[test]
    fn surface_flags_are_not_taken_for_pointers() {
        // A trimmed surface (144) whose outer-boundary flag N1 = 1 must not hide
        // the circle that happens to be entity 1.
        let e = vec![
            (100, 1, 0, "00000000", 0, "100,0.,0.,0.,50.,0.,50.,0.;"),
            (100, 1, 0, "00000000", 0, "100,0.,0.,0.,10.,0.,10.,0.;"),
            (144, 1, 0, "00000000", 0, "144,0,1,0,0;"),
        ];
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts[0].drawing.pieces.len(), 2);
    }

    #[test]
    fn a_placement_that_isnt_in_the_file_is_named_not_ignored() {
        // A hole whose matrix pointer (999) leads nowhere: left out and named,
        // not drawn at its unmoved position.
        let mut e = square(3);
        e.push((100, 3, 999, "00000000", 0, "100,0.,50.,25.,60.,25.,60.,25.;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts[0].drawing.pieces.len(), 4);
        assert!(parts[0].drawing.report.skipped.iter().any(|s| s.contains("placement")), "{:?}", parts[0].drawing.report.skipped);
    }

    #[test]
    fn hollerith_fields_split_right_and_quickly() {
        assert_eq!(fields("110,3HA,C,  2Hxy, 1 2H,5;", ',', ';'), vec!["110", "A,C", "xy", "1 2H", "5"]);
        // 60 000 spaces then 60 000 H's: each character is looked at once.
        let field = format!("{}{};", " ".repeat(60_000), "H".repeat(60_000));
        let t0 = std::time::Instant::now();
        assert_eq!(fields(&field, ',', ';').len(), 1);
        assert!(t0.elapsed().as_secs_f64() < 2.0, "{:?}", t0.elapsed());
    }

    #[test]
    fn a_cut_off_arc_is_named() {
        let mut e = square(3);
        e.push((100, 3, 0, "00000000", 0, "100;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        assert!(parts[0].drawing.report.skipped.iter().any(|s| s.contains("damaged entities")), "{:?}", parts[0].drawing.report.skipped);
    }

    #[test]
    fn inch_files_are_converted_to_millimetres() {
        let g = geo(&make(INCH, &square(0)));
        assert!((g.w - 2540.0).abs() < 1e-6);
    }

    #[test]
    fn levels_become_layers() {
        let mut e = square(1);
        e.push((110, 7, 0, "00000000", 0, "110,10.,10.,0.,20.,10.,0.;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let names: Vec<String> = parts[0].drawing.layers().keys().cloned().collect();
        assert_eq!(names, vec!["1", "7"]);
    }

    #[test]
    fn transformation_matrices_are_applied_and_nested() {
        // Matrix 1 (DE 1): move +1000 in x; matrix 2 (DE 3): rotate 90° then matrix 1.
        let mut e = vec![
            (124, 0, 0, "00000000", 0, "124,1.,0.,0.,1000.,0.,1.,0.,0.,0.,0.,1.,0.;"),
            (124, 0, 1, "00000000", 0, "124,0.,-1.,0.,0.,1.,0.,0.,0.,0.,0.,1.,0.;"),
        ];
        for (t, l, _, s, f, p) in square(0) {
            e.push((t, l, 3, s, f, p));
        }
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let d = &parts[0].drawing;
        // (100, 0) rotated 90° is (0, 100), then moved to (1000, 100).
        let p = d.pieces[0].pts[1];
        assert!((p[0] - 1000.0).abs() < 1e-9 && (p[1] - 100.0).abs() < 1e-9, "{p:?}");
        let g = crate::dxf::build(d, &BTreeMap::new()).unwrap().0;
        assert!((g.w - 50.0).abs() < 1e-9 && (g.h - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_matrix_that_stands_the_drawing_on_edge_is_turned_flat() {
        // Third row (1,0,0): drawn as is the part would be edge-on (all y equal).
        let mut e = vec![(124, 0, 0, "00000000", 0, "124,0.,-1.,0.,900.,0.,0.,-1.,1300.,1.,0.,0.,-140.;")];
        for (t, l, _, s, f, p) in square(0) {
            e.push((t, l, 1, s, f, p));
        }
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let d = &parts[0].drawing;
        // Columns turned: x' = -x + 900, y' = -y + 1300 (the part turned 180°).
        let p = d.pieces[0].pts[1];
        assert!((p[0] - 800.0).abs() < 1e-9 && (p[1] - 1300.0).abs() < 1e-9, "{p:?}");
        let g = crate::dxf::build(d, &BTreeMap::new()).unwrap().0;
        assert!((g.w - 100.0).abs() < 1e-9 && (g.h - 50.0).abs() < 1e-9);
    }

    #[test]
    fn hidden_and_dependent_entities_are_not_drawn_twice() {
        // A composite curve (102) of four lines: the lines are physically dependent.
        let mut e: Vec<(i32, i64, usize, &str, i32, &str)> = vec![];
        for (t, l, x, _, f, p) in square(0) {
            e.push((t, l, x, "00010000", f, p));
        }
        e.push((102, 0, 0, "00000000", 0, "102,4,1,3,5,7;"));
        // A hidden extra line.
        e.push((110, 0, 0, "01000000", 0, "110,0.,0.,0.,500.,500.,0.;"));
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        assert_eq!(parts[0].drawing.pieces.len(), 1);
        assert!(parts[0].drawing.pieces[0].closed);
        let g = crate::dxf::build(&parts[0].drawing, &BTreeMap::new()).unwrap().0;
        assert!((crate::nest::area(&g.outline) - 5000.0).abs() < 1e-6);
    }

    #[test]
    fn splines_ellipses_and_subfigures_are_read() {
        // Rational B-spline: a quarter circle of radius 100 (degree 2, weights 1, √½, 1).
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let nurbs = format!("126,2,2,1,0,1,0,0.,0.,0.,1.,1.,1.,1.,{w},1.,100.,0.,0.,100.,100.,0.,0.,100.,0.,0.,1.,0.,0.,1.;");
        let nurbs: &'static str = Box::leak(nurbs.into_boxed_str());
        let e = vec![
            (126, 0, 0, "00000000", 0, nurbs),
            // Ellipse x²/50² + y²/25² = 1 (full).
            (104, 0, 0, "00000000", 1, "104,1.,0.,4.,0.,0.,-2500.,0.,50.,0.,50.,0.;"),
            // Subfigure: one line, placed twice the size at (1000, 0).
            (110, 0, 0, "00000200", 0, "110,0.,0.,0.,10.,0.,0.;"),
            (308, 0, 0, "00000200", 0, "308,0,4HDEMO,1,5;"),
            (408, 0, 0, "00000000", 0, "408,7,1000.,0.,0.,2.;"),
        ];
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let d = &parts[0].drawing;
        assert_eq!(d.pieces.len(), 3, "{:?}", d.pieces.iter().map(|p| p.pts.len()).collect::<Vec<_>>());
        for p in &d.pieces[0].pts {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 100.0).abs() <= ARC_CHORD_ERR + 1e-6, "r = {r}");
        }
        for w in d.pieces[0].pts.windows(2) {
            let l = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            assert!(l <= 10.0 + 1e-9);
        }
        assert!(d.pieces[1].closed);
        let a = crate::nest::area(&d.pieces[1].pts);
        assert!((a - std::f64::consts::PI * 50.0 * 25.0).abs() < 20.0, "{a}");
        let sub = &d.pieces[2].pts;
        assert!((sub[0][0] - 1000.0).abs() < 1e-9 && (sub[1][0] - 1020.0).abs() < 1e-9);
    }

    #[test]
    fn a_drawing_on_edge_is_laid_flat_and_notes_become_text() {
        // The square drawn in the XZ plane.
        let e = vec![
            (110, 0, 0, "00000000", 0, "110,0.,5.,0.,100.,5.,0.;"),
            (110, 0, 0, "00000000", 0, "110,100.,5.,0.,100.,5.,50.;"),
            (110, 0, 0, "00000000", 0, "110,100.,5.,50.,0.,5.,50.;"),
            (110, 0, 0, "00000000", 0, "110,0.,5.,50.,0.,5.,0.;"),
            (212, 0, 0, "00000000", 0, "212,1,8,40.,5.,1,0.,0.,0,0,10.,5.,10.,8HDEMO-001;"),
        ];
        let parts = read(make(MM, &e).as_bytes(), &Options::default()).unwrap();
        let g = crate::dxf::build(&parts[0].drawing, &BTreeMap::new()).unwrap().0;
        assert!((g.w - 100.0).abs() < 1e-9 && (g.h - 50.0).abs() < 1e-9);
        assert_eq!(parts[0].drawing.texts.len(), 1);
        assert_eq!(parts[0].drawing.texts[0].1, "DEMO-001");
    }

    #[test]
    fn hollerith_fields_may_hold_delimiters() {
        let f = fields("212,1,3,4HA,B;,7.5D1;", ',', ';');
        assert_eq!(f, vec!["212", "1", "3", "A,B;", "7.5D1"]);
        assert_eq!(num("7.5D1"), Some(75.0));
        assert_eq!(num("1."), Some(1.0));
    }

    #[test]
    fn surfaces_only_and_junk_are_refused_in_plain_words() {
        let e = vec![(128, 0, 0, "00000000", 0, "128,1,1,1,1,0,0,1,0,0;")];
        let err = read(make(MM, &e).as_bytes(), &Options::default()).unwrap_err();
        assert!(err.contains("3D model"), "{err}");
        assert!(read(b"hello", &Options::default()).is_err());
        let _ = read(&make(MM, &square(0)).as_bytes()[..300], &Options::default());
    }

    /// Reads every sample IGES that ships with the original program, if installed.
    #[test]
    fn samples_from_the_original_install_are_read() {
        let dir = std::env::var("LANTEK_DIR").unwrap_or_else(|_| r"C:\Lantek\Expert".to_string());
        let dir = std::path::Path::new(&dir).join("Samples").join("IGES");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("igs") || x.eq_ignore_ascii_case("iges")) {
                let bytes = std::fs::read(&p).unwrap();
                let parts = read(&bytes, &Options::default()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                crate::dxf::build(&parts[0].drawing, &BTreeMap::new()).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            }
        }
    }
}
