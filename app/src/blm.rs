//! Programs for the BLM LC5 (Siemens 840D) the way Lantek's BLM post writes
//! them (the "ADIGE (Laser Siemens 840D)" post, settings in
//! MchFiles\PstAdi06.PS1 / .CF1). Per sheet it writes:
//!
//! * `<NEST>.SPF` — the sheet: header (material, sheet size, R-parameters),
//!   `INITNEST`, then per part `ORIG(x,y,angle)` + `N<k> CALL"…_<SUB>_spf"`,
//!   `ENDNEST`, `RET`;
//! * `<NEST>7nn.SPF` — one subprogram per part: `INITPART(1)`, per contour
//!   `NEXT_L(…)`, lead-in, `PROFILE`, the moves (`CORNER` at sharp corners),
//!   `LASTLINE(…)` for the last move, `ENDGEO`, the pre-cut retry loop,
//!   `ENDPART`, `RET`;
//! * `<NEST>LIS.SPF` — the list of the SPF files;
//! * `<NEST>.XML` — the job sheet for the machine (BLM's Italian tags);
//! * `<NEST>.zip` — the SPF files zipped (the post calls an external zip tool; BL1).
//!
//! Lines end in CR LF. See MECHANICS.md "BLM LC5 programs".

use crate::cnc::{Made, SheetPlan};
use crate::nest::{Part, Placed, SheetSpec, P};
use std::fmt::Write as _;

/// Subprogram numbers start here: `<name>` + (700 + part number)
/// (BL3).
pub const SUB_BASE: usize = 700;
/// The post warns above this many subprograms (PS1 Z027).
pub const MAX_SUBPROGRAMS: usize = 96;
/// CORNER is written where the angle between two moves is at most this
/// (PstAdi06.PS1 Z043 "valore dell'angolo massimo su cui inserire il
/// comando"; BL5).
pub const CORNER_MAX_ANGLE_DEG: f64 = 119.9;
/// NEXT_L codes (BL22): marks are written at quality 9 with pierce 0; a cut
/// contour at its own quality with the table's pierce mode for it, or pierce
/// 2 when the table gives none (the example program's code). Cut-off lines
/// keep pierce 2 at quality 3, also in the XML's <Taglio>/<Foratura> flags (BL23).
pub const QUALITY_CUT: usize = 3;
pub const QUALITY_MARK: usize = 9;
pub const PIERCE_CUT: usize = 2;
pub const PIERCE_MARK: usize = 0;
/// Points must lie this close to a circle to be written as one arc (mm) —
/// the machine tolerance of the LC5's post library (PstAdi06.CF1 A005).
pub const ARC_TOL: f64 = 0.005;
/// Largest radius written as an arc; flatter runs stay straight moves (mm).
pub const ARC_MAX_R: f64 = 5000.0;
/// Largest turn between two moves inside an arc (degrees); sharper is a
/// corner, not a curve (a square's corners also lie on a circle).
pub const ARC_MAX_STEP_DEG: f64 = 30.0;
/// Fewest moves an arc must replace.
pub const ARC_MIN_MOVES: usize = 3;

/// How far a move between two fitted points may sit from the arc that
/// replaces it (BL38): a drawing's curves are split within ARC_CHORD_ERR, so a
/// true arc's pieces reach that, plus the fit's usual point tolerance. Fixed:
/// a post library's looser tolerance must not loosen this limit too.
pub const ARC_SPAN_TOL: f64 = crate::dxf::ARC_CHORD_ERR + ARC_TOL;
/// Sheet-trim overruns (PstAdi06.PS1 Z039 "fuoriuscita in negativo" and
/// Z040 "in positivo", 10 mm each here; Z038 = 1 turns the trim on). Used by
/// the post's trim writer per sheet edge (BL12; see final_cut_sub).
pub const TRIM_NEG: f64 = 10.0;
pub const TRIM_POS: f64 = 10.0;
/// The trim cut stops this far before the far edge, then HD_DWN_OFF
/// (BL13).
pub const TRIM_STOP_BEFORE_END: f64 = 10.0;
/// How close to the sheet's low edges (x or y = 0) a trim end counts as on
/// that edge (mm; the high edges are "at or past the sheet size").
pub const TRIM_EDGE_TOL: f64 = 0.5;

/// A contour without a lead-in: how far along its first straight move the
/// compensation word's own move runs before `PROFILE` (PS1 Z028; BL24).
pub const NO_LEAD_COMP_RUN: f64 = 4.0;

/// The BLM post's settings this writer uses, read from the post's own files
/// in the install (MchFiles\PstAdi06.PS1 / .CF1); the constants above when
/// the files aren't there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdiParams {
    /// Z039 / Z040.
    pub trim_neg: f64,
    pub trim_pos: f64,
    /// Z043.
    pub corner_deg: f64,
    /// CF1 A005.
    pub arc_tol: f64,
    /// Z028.
    pub no_lead_run: f64,
}

impl Default for AdiParams {
    fn default() -> Self {
        AdiParams { trim_neg: TRIM_NEG, trim_pos: TRIM_POS, corner_deg: CORNER_MAX_ANGLE_DEG, arc_tol: ARC_TOL, no_lead_run: NO_LEAD_COMP_RUN }
    }
}

/// The value of a `Kxxx value ; comment` line.
fn setting(text: &str, key: &str) -> Option<f64> {
    text.lines().find(|l| l.split_whitespace().next() == Some(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.trim_matches('"').parse().ok())
}

impl AdiParams {
    pub fn parse(ps1: &str, cf1: &str) -> Self {
        let d = AdiParams::default();
        AdiParams {
            trim_neg: setting(ps1, "Z039").unwrap_or(d.trim_neg),
            trim_pos: setting(ps1, "Z040").unwrap_or(d.trim_pos),
            corner_deg: setting(ps1, "Z043").unwrap_or(d.corner_deg),
            arc_tol: setting(cf1, "A005").filter(|v| *v > 0.0).unwrap_or(d.arc_tol),
            no_lead_run: setting(ps1, "Z028").unwrap_or(d.no_lead_run),
        }
    }
}

static ADI: std::sync::RwLock<Option<AdiParams>> = std::sync::RwLock::new(None);

/// The active machine's post settings (set from Settings → Machines); the
/// built-in values until a machine sets them.
pub fn adi() -> AdiParams {
    ADI.read().ok().and_then(|g| *g).unwrap_or_default()
}

pub fn set_adi(p: AdiParams) {
    if let Ok(mut g) = ADI.write() {
        *g = Some(p);
    }
}

impl AdiParams {
    /// Read from a post's .PS1 and .CF1 files (either may be missing).
    pub fn from_files(ps1: &str, cf1: &str) -> Self {
        let rd = |f: &str| if f.is_empty() { String::new() } else { std::fs::read(f).map(|b| b.iter().map(|&c| c as char).collect::<String>()).unwrap_or_default() };
        AdiParams::parse(&rd(ps1), &rd(cf1))
    }
}
/// Name of the cut-off subprogram's part (example: <PartRef>FINAL_CUT).
pub const FINAL_CUT: &str = "FINAL_CUT";
/// Post version written in the header ("Postprocessor version 2.0.0").
pub const POST_VERSION: &str = "2.0.0";
/// Program folder on the control the CALLs point to.
pub const SHEET_DIR: &str = "/_n_wks_dir/_n_sheet_wpd/_n_";

/// What the XML needs that the nesting doesn't know.
#[derive(Clone, Debug, PartialEq)]
pub struct BlmSettings {
    /// Write BLM LC5 programs instead of plain G-code.
    pub on: bool,
    pub machine: String,
    pub job: String,
    pub mat_name: String,
    pub mat_id: String,
    pub gas_id: u32,
    pub gas_name: String,
}

/// Material name and BLM material code for our materials (codes from the
/// PstAdi06.PS1 D-lines: "RST 37-2" 0010, "X5CrNi" 0100, "Al 1050 A" 0200).
pub fn material_codes(material: usize) -> (&'static str, &'static str) {
    match material {
        1 => ("STAINLESS", "0100"),
        2 => ("ALUMINIUM", "0200"),
        _ => ("STEEL", "0010"),
    }
}

impl BlmSettings {
    /// Settings for a machine before a job is open (on for an LC5 / BLM).
    pub fn for_machine(machine: &str) -> Self {
        let m = machine.to_uppercase();
        let (n, id) = material_codes(1);
        BlmSettings { on: m.contains("LC5") || m.contains("BLM"), machine: machine.to_string(), job: String::new(), mat_name: n.into(), mat_id: id.into(), gas_id: 1, gas_name: "Nitrogen".into() }
    }

    pub fn for_spec(spec: &SheetSpec, machine: &str, job: &str) -> Self {
        let (n, id) = material_codes(spec.material);
        let blm = machine.to_uppercase().contains("LC5") || machine.to_uppercase().contains("BLM");
        // The job's gas from the list (GS10); a job without one takes the
        // machine's in `engine::blm_for` (GS11), nitrogen until then, as for
        // a machine with no job open.
        let g = spec.gas.unwrap_or(crate::gas::Gas::Nitrogen);
        BlmSettings { on: blm, machine: machine.to_string(), job: job.to_string(), mat_name: n.into(), mat_id: id.into(), gas_id: g.number(), gas_name: g.name().into() }
    }
}

/// A number the way the post writes coordinates: up to 3
/// decimals, trailing zeros dropped, a whole number keeps its point ("12.").
pub fn coord(v: f64) -> String {
    let s = plain(v);
    if s.contains('.') {
        s
    } else {
        format!("{s}.")
    }
}

/// A number the way ORIG and the header write it (3 decimals,
/// trailing zeros dropped, no point for whole numbers).
pub fn plain(v: f64) -> String {
    let v = if v.abs() < 0.0005 { 0.0 } else { v };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// The nesting's name: job letters and digits, "X", sheet number.
pub fn nest_name(job: &str, sheet_no: usize) -> String {
    let j: String = job.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    format!("{}X{}", if j.is_empty() { "JOB".into() } else { j }, sheet_no)
}

/// A subprogram's name (BL3): the nesting name up to the first
/// ".", cut to 5 characters when it is 6–8 long, then 700 + part number.
pub fn sub_name(nest: &str, k: usize) -> String {
    let mut base: String = nest.split('.').next().unwrap_or("").to_string();
    if (6..=8).contains(&base.chars().count()) {
        base = base.chars().take(5).collect();
    }
    format!("{base}{}", SUB_BASE + k)
}

fn turn_deg(a: P, b: P, c: P) -> f64 {
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [c[0] - b[0], c[1] - b[1]];
    let (lu, lv) = ((u[0] * u[0] + u[1] * u[1]).sqrt(), (v[0] * v[0] + v[1] * v[1]).sqrt());
    if lu < 1e-9 || lv < 1e-9 {
        return 0.0;
    }
    ((u[0] * v[0] + u[1] * v[1]) / (lu * lv)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Is the corner at b sharp enough for CORNER? (angle between the moves —
/// 180° for a straight run — at most CORNER_MAX_ANGLE_DEG).
fn is_corner(a: P, b: P, c: P) -> bool {
    180.0 - turn_deg(a, b, c) <= adi().corner_deg
}

/// One move of a path: a straight move, or an arc to `to` around `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    Line(P),
    Arc { to: P, center: P, ccw: bool },
}

fn cross(a: P, b: P, c: P) -> f64 {
    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
}

/// Does the run pts[i..=j] lie on one circle, turning one way? Returns the
/// circle's centre and the turning direction.
fn arc_fit(pts: &[P], i: usize, j: usize) -> Option<(P, bool)> {
    let (c, r) = crate::partview::circle_through(pts[i], pts[(i + j) / 2], pts[j])?;
    if r > ARC_MAX_R {
        return None;
    }
    let ccw = cross(pts[i], pts[i + 1], pts[i + 2]) > 0.0;
    for k in i..=j {
        let d = ((pts[k][0] - c[0]).powi(2) + (pts[k][1] - c[1]).powi(2)).sqrt();
        // A post library may ask for tighter than ARC_TOL, never looser: BL38's
        // limit holds only with the ends this close to the circle.
        if (d - r).abs() > adi().arc_tol.min(ARC_TOL) {
            return None;
        }
        if k + 2 <= j && (cross(pts[k], pts[k + 1], pts[k + 2]) > 0.0) != ccw {
            return None;
        }
        if k + 2 <= j && turn_deg(pts[k], pts[k + 1], pts[k + 2]) > ARC_MAX_STEP_DEG {
            return None;
        }
        // BL38: the points can all lie on a circle while a long straight between
        // them runs far from it (an edge between two small rounded corners).
        // A straight move is nearest the centre at its middle, furthest at its
        // ends, which the test above has already held to the circle.
        if k < j {
            let mid = [(pts[k][0] + pts[k + 1][0]) / 2.0, (pts[k][1] + pts[k + 1][1]) / 2.0];
            if r - (mid[0] - c[0]).hypot(mid[1] - c[1]) > ARC_SPAN_TOL {
                return None;
            }
        }
    }
    // Less than a full turn.
    let mut sweep = 0.0;
    for k in i..j {
        let a0 = (pts[k][1] - c[1]).atan2(pts[k][0] - c[0]);
        let a1 = (pts[k + 1][1] - c[1]).atan2(pts[k + 1][0] - c[0]);
        let mut d = a1 - a0;
        while d > std::f64::consts::PI {
            d -= std::f64::consts::TAU;
        }
        while d < -std::f64::consts::PI {
            d += std::f64::consts::TAU;
        }
        sweep += d.abs();
    }
    (sweep < std::f64::consts::TAU - 1e-3).then_some((c, ccw))
}

/// The moves from pts[0] through pts[last]: runs of points on one circle
/// become arcs (longest first), the rest straight moves.
pub fn fit_moves(pts: &[P]) -> Vec<Move> {
    let mut out = vec![];
    let mut i = 0;
    let last = pts.len().saturating_sub(1);
    while i < last {
        let mut best = None;
        let mut j = i + ARC_MIN_MOVES;
        while j <= last {
            match arc_fit(pts, i, j) {
                Some(f) => best = Some((j, f)),
                None if best.is_some() => break,
                None => {}
            }
            j += 1;
        }
        match best {
            Some((j, (center, ccw))) => {
                out.push(Move::Arc { to: pts[j], center, ccw });
                i = j;
            }
            None => {
                out.push(Move::Line(pts[i + 1]));
                i += 1;
            }
        }
    }
    out
}

fn move_end(m: &Move) -> P {
    match *m {
        Move::Line(p) => p,
        Move::Arc { to, .. } => to,
    }
}

/// What a path in a subprogram is: a marking line (pierce 0, quality 9) or a
/// cut with its NEXT_L pierce code and quality (BL22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathKind {
    Mark,
    Cut { pierce: usize, quality: usize },
}

impl PathKind {
    /// A cut contour: its own quality and the pierce mode the sheet's
    /// technology table gives it, else pierce 2 (BL22).
    fn of(plan: &SheetPlan, c: &crate::cnc::Contour) -> PathKind {
        PathKind::Cut { pierce: plan.timing.speeds.pierce_mode(c.quality).map_or(PIERCE_CUT, |p| p as usize), quality: c.quality }
    }
    /// A cut-off line's codes.
    const CUT_LINE: PathKind = PathKind::Cut { pierce: PIERCE_CUT, quality: QUALITY_CUT };

    /// NEXT_L's pierce and quality codes.
    fn codes(self) -> (usize, usize) {
        match self {
            PathKind::Mark => (PIERCE_MARK, QUALITY_MARK),
            PathKind::Cut { pierce, quality } => (pierce, quality),
        }
    }
}

/// One path (closed contour or open marking line) in the subprogram's form.
/// `comp` is the contour's compensation word on its first lead-in move
/// (" G41" / " G42", or "" for none: system compensation or off; KC2).
fn write_path(s: &mut String, pts: &[P], closed: bool, lead: &[P], comp: &str, first: bool, kind: PathKind) {
    let mark = kind == PathKind::Mark;
    let mut moves: Vec<P> = pts.to_vec();
    if closed && pts.len() > 1 {
        moves.push(pts[0]);
    }
    // No zero-length moves (a repeated point would be written twice).
    moves.dedup_by(|b, a| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
    if moves.len() < 2 {
        return;
    }
    let has_lead = !mark && lead.len() >= 2;
    let start = if has_lead { lead[0] } else { moves[0] };
    let (pierce, quality) = kind.codes();
    let _ = write!(s, "NEXT_L({},{},{},{},{})\r\n", coord(start[0]), coord(start[1]), if first { 0 } else { 1 }, pierce, quality);
    if has_lead {
        for (i, q) in lead.iter().enumerate().skip(1) {
            let comp = if i == 1 { comp } else { "" };
            let _ = write!(s, "G01{comp} X{} Y{}\r\n", coord(q[0]), coord(q[1]));
        }
    }
    let n = moves.len();
    // Everything but the last move (that one is LASTLINE), arcs as G02/G03
    // with I/J from the arc's start to its centre, as in the LC5 example.
    let body = fit_moves(&moves[..n - 1]);
    if !has_lead && !mark && !comp.is_empty() {
        // No lead-in to carry the compensation word (BL24): it goes on a move
        // Z028 along the first straight (at most half of it) before PROFILE,
        // or on a zero-length move when the contour starts with an arc.
        let a = moves[0];
        let run = adi().no_lead_run;
        let to = match body.first() {
            Some(&Move::Line(b)) if run > 0.1 => {
                let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let t = run.min(len / 2.0) / len;
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            }
            _ => a,
        };
        let _ = write!(s, "G01{comp} X{} Y{}\r\n", coord(to[0]), coord(to[1]));
    }
    s.push_str("PROFILE\r\n");
    let mut from = moves[0];
    let mut idx = 0usize;
    for (k, mv) in body.iter().enumerate() {
        let end = move_end(mv);
        match *mv {
            Move::Line(p) => {
                let _ = write!(s, "G01 X{} Y{}\r\n", coord(p[0]), coord(p[1]));
            }
            Move::Arc { to, center, ccw } => {
                let _ = write!(s, "{} X{} Y{} I{} J{}\r\n", if ccw { "G03" } else { "G02" }, coord(to[0]), coord(to[1]), coord(center[0] - from[0]), coord(center[1] - from[1]));
            }
        }
        // Where this move ends among the points, for the corner test.
        while idx < n - 1 && moves[idx] != end {
            idx += 1;
        }
        let _ = k;
        if idx >= 1 && idx + 1 < n && is_corner(moves[idx - 1], moves[idx], moves[idx + 1]) {
            s.push_str("CORNER\r\n");
        }
        from = end;
    }
    let (a, b) = (moves[n - 2], moves[n - 1]);
    // BL27: what real programs write on nearly every contour; when the post
    // writes a 1 and the lead-in arc's radius instead isn't known (MAC-31).
    let _ = write!(s, "LASTLINE(0,0,0,1,{},{},{},{},0,0)\r\n", coord(a[0]), coord(a[1]), coord(b[0]), coord(b[1]));
}

/// One part's subprogram, in the part's own coordinates (origin at its
/// lowest-left corner, which ORIG places on the sheet).
pub fn subprogram(name: &str, plan: &SheetPlan, part_no: usize, origin: P) -> String {
    let rel = |q: P| [q[0] - origin[0], q[1] - origin[1]];
    let mut s = String::from(";M01\r\nR16=chk_pret\r\nCICLA:\r\nINITPART(1)\r\n");
    let _ = write!(s, ";{name}\r\n");
    let mut first = true;
    // Its marks and contours in its cut order (CO9): with no order set by
    // hand, the marks first.
    for made in plan.part_order(part_no) {
        match made {
            Made::Mark(j) => {
                let p: Vec<P> = plan.marks[j].1.iter().map(|&q| rel(q)).collect();
                write_path(&mut s, &p, false, &[], "", first, PathKind::Mark);
            }
            Made::Contour(k) if plan.contours[k].punch.is_none() => {
                let c = &plan.contours[k];
                let p: Vec<P> = c.points.iter().map(|&q| rel(q)).collect();
                let lead: Vec<P> = c.lead_path.iter().map(|&q| rel(q)).collect();
                // The side from the contour's real direction (KC2): a mirrored part
                // or a clockwise drawing gets the other word. The LC5 post writes no
                // corrector number.
                let comp = c.comp.word().map_or(String::new(), |(side, _)| format!(" {}", side.word()));
                write_path(&mut s, &p, true, &lead, &comp, first, PathKind::of(plan, c));
            }
            Made::Contour(_) => continue,
        }
        first = false;
    }
    s.push_str("ENDGEO\r\nIF R16==1\r\n\t  STOPRE\r\n\t  R16=0\r\n   GOTOB CICLA\r\nENDIF\r\nENDPART\r\nRET\r\n");
    s
}

/// Which sheet edge a trim end is on (BL11): 1 left (x below the
/// tolerance), 2 bottom, 3 right (x at or past the sheet length), 4 top —
/// the later test wins, so 4 > 3 > 2 > 1.
pub fn trim_side(q: P, w: f64, h: f64) -> Option<u8> {
    let mut s = None;
    if q[0] < TRIM_EDGE_TOL {
        s = Some(1);
    }
    if q[1] < TRIM_EDGE_TOL {
        s = Some(2);
    }
    if q[0] >= w - 1e-9 {
        s = Some(3);
    }
    if q[1] >= h - 1e-9 {
        s = Some(4);
    }
    s
}

/// A straight sheet-trim (cut-off) line's subprogram, as the post writes it
/// (BL12–BL13; example 1234X38THXSAMPLE702.SPF). Local coordinates,
/// ORIG at the line's start `a`; `b` is the end. Per the start edge: the
/// approach point is outside by Z039 (left/bottom) or Z040 (right/top), the
/// cut starts inside by Z040 (left/bottom) or Z039 (right/top); NEXT_L gets
/// the start, then (start − approach) and the approach point. The cut runs
/// to TRIM_STOP_BEFORE_END before the end edge, then HD_DWN_OFF, then out
/// past the end by Z040 (right/top) or Z039 (left/bottom). None when an end
/// isn't on a sheet edge.
pub fn final_cut_sub(a: P, b: P, w: f64, h: f64, p: &AdiParams) -> Option<String> {
    let (s0, s1) = (trim_side(a, w, h)?, trim_side(b, w, h)?);
    let rel = |q: P| [q[0] - a[0], q[1] - a[1]];
    let (mut start, mut appr) = (rel(a), rel(a));
    match s0 {
        1 => {
            start[0] += p.trim_pos;
            appr[0] -= p.trim_neg;
        }
        2 => {
            start[1] += p.trim_pos;
            appr[1] -= p.trim_neg;
        }
        3 => {
            start[0] -= p.trim_neg;
            appr[0] += p.trim_pos;
        }
        _ => {
            start[1] -= p.trim_neg;
            appr[1] += p.trim_pos;
        }
    }
    let (mut stop, mut exit) = (rel(b), rel(b));
    let k = TRIM_STOP_BEFORE_END;
    match s1 {
        1 => {
            stop[0] += k;
            exit[0] -= p.trim_neg;
        }
        2 => {
            stop[1] += k;
            exit[1] -= p.trim_neg;
        }
        3 => {
            stop[0] -= k;
            exit[0] += p.trim_pos;
        }
        _ => {
            stop[1] -= k;
            exit[1] += p.trim_pos;
        }
    }
    let d = [start[0] - appr[0], start[1] - appr[1]];
    let mut s = String::from(";M01\r\nR16=chk_pret\r\nCICLA:\r\nINITPART(1)\r\n;\r\n");
    let _ = write!(s, "NEXT_L({},{},0,0,3,1,{},{},{},{})\r\n", plain(start[0]), plain(start[1]), plain(d[0]), plain(d[1]), plain(appr[0]), plain(appr[1]));
    let _ = write!(s, "G01 G40 X{} Y{}\r\nPROFILE\r\n", coord(start[0]), coord(start[1]));
    let _ = write!(s, "G01 X{} Y{}\r\nHD_DWN_OFF\r\nG01 X{} Y{}\r\n", coord(stop[0]), coord(stop[1]), coord(exit[0]), coord(exit[1]));
    s.push_str("ENDGEO\r\nIF R16==1\r\n\t  STOPRE\r\n\t  R16=0\r\n   GOTOB CICLA\r\nENDIF\r\nENDPART\r\nRET\r\n");
    Some(s)
}

/// Any other cut line (several segments, or not edge to edge): one open
/// cut path, no lead-in.
pub fn cut_path_sub(pts: &[P], origin: P) -> String {
    let p: Vec<P> = pts.iter().map(|q| [q[0] - origin[0], q[1] - origin[1]]).collect();
    let mut s = String::from(";M01\r\nR16=chk_pret\r\nCICLA:\r\nINITPART(1)\r\n;\r\n");
    let mut body = String::new();
    write_path(&mut body, &p, false, &[], "", true, PathKind::CUT_LINE);
    s.push_str(&body);
    s.push_str("ENDGEO\r\nIF R16==1\r\n\t  STOPRE\r\n\t  R16=0\r\n   GOTOB CICLA\r\nENDIF\r\nENDPART\r\nRET\r\n");
    s
}

/// Everything written for one sheet.
pub struct BlmSet {
    pub nest: String,
    /// (file name, contents) of the SPF files, in zip order.
    pub spf: Vec<(String, String)>,
    pub xml: String,
    /// The post's warning when there are more than 96 subprograms.
    pub warning: Option<String>,
}

fn bounds(parts: &[Part], sheet: &[Placed]) -> (P, P) {
    let mut lo = [f64::MAX, f64::MAX];
    let mut hi = [f64::MIN, f64::MIN];
    for pl in sheet {
        for q in crate::nest::placed_shape(parts, pl).0 {
            lo = [lo[0].min(q[0]), lo[1].min(q[1])];
            hi = [hi[0].max(q[0]), hi[1].max(q[1])];
        }
    }
    (lo, hi)
}

/// The free margins in the sheet program's header (BL17): the room left at each
/// side once everything written is on the sheet, lead-ins, marking and cut-off
/// lines included. [R23 far X, R24 far Y, R523 near X, R524 near Y]; never below
/// 0, and under 0.1 mm counts as none.
pub fn sheet_margins(plan: &SheetPlan, cuts: &[crate::cutoff::CutLine], w: f64, h: f64) -> [f64; 4] {
    let cuts_pts = plan.contours.iter().filter(|c| c.punch.is_none()).flat_map(|c| c.points.iter().chain(c.lead_path.iter()));
    let all = cuts_pts.chain(plan.marks.iter().flat_map(|m| m.1.iter())).chain(cuts.iter().filter(|c| c.pts.len() >= 2).flat_map(|c| c.pts.iter()));
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in all {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    if lo[0] > hi[0] {
        return [w, h, 0.0, 0.0];
    }
    let room = |v: f64| if v <= 0.1 { 0.0 } else { v };
    [room(w - hi[0]), room(h - hi[1]), room(lo[0]), room(lo[1])]
}

/// Build the BLM program set for one sheet.
pub fn sheet_set(sheet_no: usize, parts: &[Part], sheet: &[Placed], spec: &SheetSpec, plan: &SheetPlan, cuts: &[crate::cutoff::CutLine], set: &BlmSettings) -> BlmSet {
    let nest = nest_name(&set.job, sheet_no);
    let origin_of = |i: usize| bounds(parts, &sheet[i..i + 1]).0;
    // One subprogram per part as machined: copies share one only when their
    // whole path (turn, mirror, start, leads) is the same relative to their
    // corner — a copy whose lead was moved clear of a neighbour gets its own.
    let mut variants: Vec<(usize, String)> = vec![];
    let mut sub_of = vec![0usize; sheet.len()];
    for (i, pl) in sheet.iter().enumerate() {
        let text = subprogram(&parts[pl.part].name, plan, i + 1, origin_of(i));
        sub_of[i] = match variants.iter().position(|v| v.0 == pl.part && v.1 == text) {
            Some(k) => k,
            None => {
                variants.push((pl.part, text));
                variants.len() - 1
            }
        };
    }
    let mut spf = vec![];
    let mut subs = vec![];
    for (k, (part, text)) in variants.iter().enumerate() {
        let name = sub_name(&nest, k + 1);
        let pname = parts[*part].name.clone();
        subs.push((name.clone(), pname, sub_of.iter().filter(|&&x| x == k).count(), *part));
        spf.push((format!("{name}.SPF"), text.clone()));
    }
    let (_, hi) = if sheet.is_empty() { ([0.0, 0.0], [0.0, 0.0]) } else { bounds(parts, sheet) };
    let (mut xmax, mut ymax) = (hi[0].max(0.0), hi[1].max(0.0));
    // Cut lines (remnant last): one subprogram each, called after the parts.
    // (name, ORIG, file contents, length in X, length in Y)
    let mut cut_subs: Vec<(String, P, String, f64, f64)> = vec![];
    let mut ordered: Vec<&crate::cutoff::CutLine> = cuts.iter().filter(|c| c.kind == crate::cutoff::CutKind::Free).collect();
    ordered.extend(cuts.iter().filter(|c| c.kind == crate::cutoff::CutKind::Remnant));
    for c in ordered {
        if c.pts.len() < 2 {
            continue;
        }
        let name = sub_name(&nest, variants.len() + cut_subs.len() + 1);
        let (a, b) = (c.pts[0], *c.pts.last().unwrap());
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let trim = if c.pts.len() == 2 && len > TRIM_STOP_BEFORE_END + adi().trim_pos.max(adi().trim_neg) { final_cut_sub(a, b, spec.w, spec.h, &adi()) } else { None };
        let body = trim.unwrap_or_else(|| cut_path_sub(&c.pts, a));
        let (lo, hi2) = c.pts.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(l, h), q| ([l[0].min(q[0]), l[1].min(q[1])], [h[0].max(q[0]), h[1].max(q[1])]));
        if c.kind == crate::cutoff::CutKind::Remnant {
            xmax = xmax.max(hi2[0].min(spec.w));
            ymax = ymax.max(hi2[1].min(spec.h));
        }
        cut_subs.push((name, a, body, hi2[0] - lo[0], hi2[1] - lo[1]));
    }
    for (name, _, body, _, _) in &cut_subs {
        spf.push((format!("{name}.SPF"), body.clone()));
    }
    // The sheet program.
    let mut m = String::new();
    let _ = write!(m, ";ADILAS\r\n;{},{},{},{}\r\n", set.mat_name, plain(spec.w), plain(spec.h), plain(spec.thickness));
    let _ = write!(m, ";IDMAT={}\r\n;NOMEMAT={}\r\n;ALIASMAT={}\r\n;IDGAS={}\r\n;NOMEGAS={}\r\n", set.mat_id, set.mat_name, set.mat_name, set.gas_id, set.gas_name);
    let _ = write!(m, ";Postprocessor version {POST_VERSION}\r\n;R220=Z16*1000+2*100+0\r\n");
    let _ = write!(m, "R12={}\r\n", plain(spec.thickness));
    for t in 1..=3 {
        let _ = write!(m, "R20{t}=0.0    ;lunghezza barretta utensile {t}\r\n");
    }
    let [r23, r24, r523, r524] = sheet_margins(plan, cuts, spec.w, spec.h);
    let _ = write!(m, "R21={}\r\nR22={}\r\nR23={}\r\nR24={}\r\nR523={}\r\nR524={}\r\nR500=0\r\nINITNEST\r\n", plain(spec.w), plain(spec.h), plain(r23), plain(r24), plain(r523), plain(r524));
    let mut order: Vec<usize> = (0..sheet.len()).collect();
    // Parts in the order they are finished (the outside is cut last, MM2). A
    // part is one call, so a part sitting in another's hole is still called
    // first when a hole of the other was picked before it (CO2).
    order.sort_by_key(|&i| plan.contours.iter().rposition(|c| c.part_no == i + 1).unwrap_or(usize::MAX));
    for (n, &i) in order.iter().enumerate() {
        let o = origin_of(i);
        let _ = write!(m, "ORIG({},{},0)\r\n", plain(o[0]), plain(o[1]));
        let _ = write!(m, "N{} CALL\"{SHEET_DIR}{}_spf\"\r\n", n + 1, sub_name(&nest, sub_of[i] + 1));
    }
    for (k, (name, o, _, _, _)) in cut_subs.iter().enumerate() {
        let _ = write!(m, "ORIG({},{},0)\r\n", plain(o[0]), plain(o[1]));
        let _ = write!(m, "N{} CALL\"{SHEET_DIR}{}_spf\"\r\n", order.len() + k + 1, name);
    }
    m.push_str("ENDNEST\r\nRET\r\n");
    let mut all = vec![(format!("{nest}.SPF"), m)];
    all.extend(spf);
    let lis: String = all.iter().map(|f| format!("{}\r\n", f.0)).collect();
    all.push((format!("{nest}LIS.SPF"), lis));
    // The job sheet (same tags and order as the post's XML).
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<General>\r\n");
    let tag = |x: &mut String, ind: usize, t: &str, v: &str| {
        let _ = write!(x, "{}<{t}>{v}</{t}>\r\n", "\t".repeat(ind));
    };
    tag(&mut x, 1, "NestingName", &nest);
    tag(&mut x, 1, "JobName", &set.job);
    tag(&mut x, 1, "Machine", &set.machine);
    tag(&mut x, 1, "MatName", &set.mat_name);
    tag(&mut x, 1, "MatId", &set.mat_id);
    tag(&mut x, 1, "MatAlias", &set.mat_name);
    tag(&mut x, 1, "Thickness", &plain(spec.thickness));
    tag(&mut x, 1, "Length", &plain(spec.w));
    tag(&mut x, 1, "Width", &plain(spec.h));
    tag(&mut x, 1, "PartXMax", &plain(xmax));
    tag(&mut x, 1, "PartYMax", &plain(ymax));
    tag(&mut x, 1, "Quantity", "1");
    tag(&mut x, 1, "NestingTime", &format!("{}", plan.seconds.round() as i64));
    tag(&mut x, 1, "GasId", &format!("{}", set.gas_id));
    tag(&mut x, 1, "GasName", &set.gas_name);
    tag(&mut x, 1, "TechnoTable", "");
    for u in 1..=5 {
        tag(&mut x, 1, &format!("UserData{u}"), "");
    }
    x.push_str("\t<Files>\r\n");
    for (name, pname, _, _) in &subs {
        x.push_str("\t\t<SubPrg>\r\n");
        tag(&mut x, 3, "CNCSub", name);
        tag(&mut x, 3, "PartRef", pname);
        x.push_str("\t\t</SubPrg>\r\n");
    }
    for (name, ..) in &cut_subs {
        x.push_str("\t\t<SubPrg>\r\n");
        tag(&mut x, 3, "CNCSub", name);
        tag(&mut x, 3, "PartRef", FINAL_CUT);
        x.push_str("\t\t</SubPrg>\r\n");
    }
    x.push_str("\t</Files>\r\n\t<Pezzi>\r\n");
    for (name, pname, qty, pi) in &subs {
        let (w, h) = parts[*pi].bbox();
        x.push_str("\t\t<Istanza>\r\n");
        tag(&mut x, 3, "Name", pname);
        tag(&mut x, 3, "Length", &plain(w));
        tag(&mut x, 3, "Width", &plain(h));
        tag(&mut x, 3, "Quantity", &qty.to_string());
        tag(&mut x, 3, "PieceTime", "0");
        tag(&mut x, 3, "WorkOrder", "");
        tag(&mut x, 3, "Order", "");
        for u in 1..=8 {
            tag(&mut x, 3, &format!("UserData{u}"), "");
        }
        x.push_str("\t\t\t<Executions>\r\n\t\t\t\t<Execution>\r\n");
        tag(&mut x, 5, "Quantity", &qty.to_string());
        x.push_str("\t\t\t\t\t<SubProgram>\r\n");
        tag(&mut x, 6, "CNCSub", &format!("{name}.SPF"));
        x.push_str("\t\t\t\t\t</SubProgram>\r\n\t\t\t\t</Execution>\r\n\t\t\t</Executions>\r\n\t\t</Istanza>\r\n");
    }
    // Cut lines: as in the example, no PieceTime / WorkOrder / Order.
    for (name, _, _, lx, ly) in &cut_subs {
        x.push_str("\t\t<Istanza>\r\n");
        tag(&mut x, 3, "Name", FINAL_CUT);
        tag(&mut x, 3, "Length", &plain(*lx));
        tag(&mut x, 3, "Width", &plain(*ly));
        tag(&mut x, 3, "Quantity", "1");
        for u in 1..=8 {
            tag(&mut x, 3, &format!("UserData{u}"), "");
        }
        x.push_str("\t\t\t<Executions>\r\n\t\t\t\t<Execution>\r\n");
        tag(&mut x, 5, "Quantity", "1");
        x.push_str("\t\t\t\t\t<SubProgram>\r\n");
        tag(&mut x, 6, "CNCSub", &format!("{name}.SPF"));
        x.push_str("\t\t\t\t\t</SubProgram>\r\n\t\t\t\t</Execution>\r\n\t\t\t</Executions>\r\n\t\t</Istanza>\r\n");
    }
    x.push_str("\t</Pezzi>\r\n\t<Utensili>\r\n\t\t<Foratura>\r\n");
    // BL23: every pierce code and quality the sheet's NEXT_L lines use.
    let mut used: Vec<PathKind> = plan.contours.iter().filter(|c| c.punch.is_none()).map(|c| PathKind::of(plan, c)).collect();
    if !plan.marks.is_empty() {
        used.push(PathKind::Mark);
    }
    if !cut_subs.is_empty() {
        used.push(PathKind::CUT_LINE);
    }
    let used: Vec<(usize, usize)> = used.into_iter().map(PathKind::codes).collect();
    for i in 0..=6 {
        let on = used.iter().any(|u| u.0 == i);
        tag(&mut x, 3, &format!("Id{i}"), if on { "1" } else { "0" });
    }
    x.push_str("\t\t</Foratura>\r\n\t\t<Taglio>\r\n");
    for i in 0..=12 {
        let on = used.iter().any(|u| u.1 == i);
        tag(&mut x, 3, &format!("Id{i}"), if on { "1" } else { "0" });
    }
    x.push_str("\t\t</Taglio>\r\n\t</Utensili>\r\n</General>\r\n");
    let warning = (variants.len() > MAX_SUBPROGRAMS).then(|| format!("{nest}: {} subprograms — more than the {MAX_SUBPROGRAMS} the control allows", variants.len()));
    BlmSet { nest, spf: all, xml: x, warning }
}

fn dos_time() -> (u16, u16) {
    // 2000-01-01 00:00 when the clock can't be read; otherwise UTC now.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(946_684_800);
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if mo <= 2 { 1 } else { 0 };
    let date = (((y - 1980).max(0) as u16) << 9) | ((mo as u16) << 5) | d as u16;
    let time = (((rem / 3600) as u16) << 11) | ((((rem % 3600) / 60) as u16) << 5) | (((rem % 60) / 2) as u16);
    (time, date)
}

/// A zip of the files, deflated, no folders (what the post's zip tool makes).
pub fn zip(files: &[(String, String)]) -> Vec<u8> {
    let (time, date) = dos_time();
    let mut out = vec![];
    let mut central = vec![];
    for (name, text) in files {
        let data = text.as_bytes();
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let crc = crc32fast::hash(data);
        let off = out.len() as u32;
        let hdr = |sig: u32, central: bool| {
            let mut h = vec![];
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes()); // made by
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // needed
            h.extend_from_slice(&0u16.to_le_bytes()); // flags
            h.extend_from_slice(&8u16.to_le_bytes()); // deflate
            h.extend_from_slice(&time.to_le_bytes());
            h.extend_from_slice(&date.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // extra
            if central {
                h.extend_from_slice(&0u16.to_le_bytes()); // comment
                h.extend_from_slice(&0u16.to_le_bytes()); // disk
                h.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
                h.extend_from_slice(&0u32.to_le_bytes()); // external attrs
                h.extend_from_slice(&off.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend(hdr(0x0403_4b50, false));
        out.extend_from_slice(&packed);
        central.extend(hdr(0x0201_4b50, true));
    }
    let cd_off = out.len() as u32;
    let n = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Write a sheet's set into `dir`: `<NEST>.XML` and `<NEST>.zip`.
pub fn write_sheet(dir: &std::path::Path, set: &BlmSet) -> std::io::Result<()> {
    std::fs::write(dir.join(format!("{}.XML", set.nest)), &set.xml)?;
    std::fs::write(dir.join(format!("{}.zip", set.nest)), zip(&set.spf))
}

#[cfg(test)]
mod blm_tests {
    use super::*;
    use crate::nest::Shape;

    #[test]
    fn numbers_and_names_follow_the_post() {
        assert_eq!(coord(12.0), "12.");
        assert_eq!(coord(25.84), "25.84");
        assert_eq!(coord(0.0), "0.");
        assert_eq!(plain(10.0), "10");
        assert_eq!(plain(93.2), "93.2");
        assert_eq!(plain(-0.02), "-0.02");
        assert_eq!(sub_name("1234X38THXSAMPLE", 1), "1234X38THXSAMPLE701");
        assert_eq!(sub_name("ABCDEFG", 2), "ABCDE702");
        assert_eq!(nest_name("J-1234 sample", 2), "J1234SAMPLEX2");
    }

    /// How far the arcs and lines of `moves` from `from` stray from the path
    /// through `pts` (the farthest of many samples along each move).
    fn stray(pts: &[P], moves: &[Move]) -> f64 {
        let seg = |p: P, a: P, b: P| {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = dx * dx + dy * dy;
            let t = if l == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l).clamp(0.0, 1.0) };
            ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
        };
        let near = |p: P| pts.windows(2).map(|w| seg(p, w[0], w[1])).fold(f64::MAX, f64::min);
        let mut from = pts[0];
        let mut worst = 0.0f64;
        for m in moves {
            for k in 1..=200 {
                let t = k as f64 / 200.0;
                let p = match *m {
                    Move::Line(e) => [from[0] + (e[0] - from[0]) * t, from[1] + (e[1] - from[1]) * t],
                    Move::Arc { to, center, ccw } => {
                        let r = (from[0] - center[0]).hypot(from[1] - center[1]);
                        let a0 = (from[1] - center[1]).atan2(from[0] - center[0]);
                        let mut a1 = (to[1] - center[1]).atan2(to[0] - center[0]);
                        let tau = std::f64::consts::TAU;
                        if ccw && a1 <= a0 {
                            a1 += tau;
                        }
                        if !ccw && a1 >= a0 {
                            a1 -= tau;
                        }
                        let a = a0 + (a1 - a0) * t;
                        [center[0] + r * a.cos(), center[1] + r * a.sin()]
                    }
                };
                worst = worst.max(near(p));
            }
            from = move_end(m);
        }
        worst
    }

    /// A plate's outline with rounded corners, split the way a drawing is.
    fn rounded_plate(w: f64, h: f64, r: f64) -> Vec<P> {
        let mut pts = vec![];
        for (cx, cy, a0) in [(w - r, r, -90.0f64), (w - r, h - r, 0.0), (r, h - r, 90.0), (r, r, 180.0)] {
            let n = crate::dxf::arc_segments(r, std::f64::consts::FRAC_PI_2);
            for k in 0..=n {
                let a = (a0 + 90.0 * k as f64 / n as f64).to_radians();
                pts.push([cx + r * a.cos(), cy + r * a.sin()]);
            }
        }
        pts.push(pts[0]);
        pts
    }

    #[test]
    fn small_holes_reach_the_machine_as_arcs() {
        // DX2 + BL38: a hole of any size has enough pieces for the arc fit.
        for d in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 10.0] {
            let r = d / 2.0;
            let n = crate::dxf::arc_segments(r, std::f64::consts::TAU);
            assert!(n >= 16, "Ø{d}: {n} pieces");
            let pts: Vec<P> = (0..n).map(|k| {
                let a = k as f64 / n as f64 * std::f64::consts::TAU;
                [r * a.cos(), r * a.sin()]
            }).collect();
            assert!(fit_moves(&pts).iter().any(|m| matches!(m, Move::Arc { .. })), "Ø{d} came out as straight moves");
            assert!(matches!(crate::machine::recognise(&pts), Some(crate::machine::StdShape::Circle { .. })), "Ø{d} isn't seen as round");
        }
    }

    #[test]
    fn margins_are_the_room_left_around_what_is_written() {
        // BL17: lead-ins and marks count; a line to the edge leaves no room.
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let mut plan = crate::cnc::plan(&[], &[], &spec);
        plan.marks.push((1, vec![[20.0, 30.0], [40.0, 30.0]]));
        assert_eq!(sheet_margins(&plan, &[], 1000.0, 500.0), [960.0, 470.0, 20.0, 30.0]);
        let cut = crate::cutoff::CutLine { pts: vec![[0.0, 100.0], [1000.0, 100.0]], kind: crate::cutoff::CutKind::Remnant };
        assert_eq!(sheet_margins(&plan, &[cut], 1000.0, 500.0), [0.0, 400.0, 0.0, 30.0]);
        assert_eq!(sheet_margins(&crate::cnc::plan(&[], &[], &spec), &[], 1000.0, 500.0), [1000.0, 500.0, 0.0, 0.0]);
    }

    #[test]
    fn long_edges_between_small_rounded_corners_stay_straight() {
        // BL38: the points of a long edge and its two neighbours lie on a circle,
        // but the edge itself runs far from it.
        for (w, h) in [(300.0, 150.0), (2400.0, 1200.0)] {
            for r in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 5.0, 25.0] {
                let pts = rounded_plate(w, h, r);
                let off = stray(&pts, &fit_moves(&pts));
                assert!(off < 0.11, "{w} x {h} R{r}: the program strays {off:.2} mm from the part");
            }
        }
    }

    #[test]
    fn a_point_just_inside_the_circle_counts_against_the_move_limit() {
        // BL38: pieces at the full limit, two points 0.005 mm inside the circle.
        let r = 500.0;
        let step = 2.0 * (1.0f64 - 0.10499 / r).acos();
        let pts: Vec<P> = (0..7).map(|k| {
            let a = k as f64 * step;
            let rr = if k == 1 || k == 2 { r - 0.0049 } else { r };
            [rr * a.cos(), rr * a.sin()]
        }).collect();
        let off = stray(&pts, &fit_moves(&pts));
        assert!(off <= ARC_SPAN_TOL + 1e-6, "strays {off:.4} mm");
    }

    #[test]
    fn straight_sided_shapes_stay_straight() {
        // BL38: half of a 24-sided plate of radius 200 has no curve to fit.
        let pts: Vec<P> = (0..=12).map(|k| {
            let a = k as f64 / 24.0 * std::f64::consts::TAU;
            [200.0 * a.cos(), 200.0 * a.sin()]
        }).collect();
        let m = fit_moves(&pts);
        assert!(m.iter().all(|m| matches!(m, Move::Line(_))), "{m:?}");
    }

    #[test]
    fn curves_split_at_the_drawing_tolerance_still_become_arcs() {
        // BL38: a chord of a true arc may sit the full 0.1 mm import error off it.
        for r in [6.0, 40.0, 100.0, 1000.0, 4000.0] {
            let n = crate::dxf::arc_segments(r, std::f64::consts::PI);
            let pts: Vec<P> = (0..=n).map(|k| {
                let a = k as f64 / n as f64 * std::f64::consts::PI;
                [r * a.cos(), r * a.sin()]
            }).collect();
            let m = fit_moves(&pts);
            assert_eq!(m.len(), 1, "R{r} in {n} pieces: {m:?}");
            assert!(matches!(m[0], Move::Arc { .. }), "R{r}: {m:?}");
        }
    }

    #[test]
    fn circles_become_arcs_and_lines_stay_lines() {
        // A 16-point circle of radius 5 around (10,10), then a straight run.
        let mut pts: Vec<P> = (0..=16).map(|k| {
            let a = k as f64 / 16.0 * std::f64::consts::PI; // half circle
            [10.0 + 5.0 * a.cos(), 10.0 + 5.0 * a.sin()]
        }).collect();
        pts.push([0.0, 0.0]);
        pts.push([-10.0, 0.0]);
        let m = fit_moves(&pts);
        match m[0] {
            Move::Arc { to, center, ccw } => {
                assert!((to[0] - 5.0).abs() < 1e-9 && (center[0] - 10.0).abs() < 1e-6 && ccw);
            }
            _ => panic!("expected an arc, got {:?}", m[0]),
        }
        assert_eq!(m[1..], [Move::Line([0.0, 0.0]), Move::Line([-10.0, 0.0])]);
        // A square has no arcs.
        assert!(fit_moves(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]).iter().all(|m| matches!(m, Move::Line(_))));
    }

    #[test]
    fn reads_the_posts_own_settings_and_trims_by_edge() {
        let p = AdiParams::parse("Z039 12.0 ; neg\nZ040 8.0 ; pos\nZ043 100.0 ;\n", "A005 0.01 ;\n");
        assert_eq!(p, AdiParams { trim_neg: 12.0, trim_pos: 8.0, corner_deg: 100.0, arc_tol: 0.01, no_lead_run: NO_LEAD_COMP_RUN });
        assert_eq!(AdiParams::parse("", ""), AdiParams::default());
        assert_eq!(trim_side([0.0, 300.0], 1000.0, 500.0), Some(1));
        assert_eq!(trim_side([300.0, 500.0], 1000.0, 500.0), Some(4));
        assert_eq!(trim_side([300.0, 200.0], 1000.0, 500.0), None);
        // Right edge to left edge with 12 / 8: approach 8 outside on the right,
        // start 12 inside, stop 10 before the left edge, exit 12 past it.
        let s = final_cut_sub([1000.0, 200.0], [0.0, 200.0], 1000.0, 500.0, &p).unwrap();
        assert!(s.contains("NEXT_L(-12,0,0,0,3,1,-20,0,8,0)\r\nG01 G40 X-12. Y0.\r\nPROFILE\r\nG01 X-990. Y0.\r\nHD_DWN_OFF\r\nG01 X-1012. Y0.\r\n"), "{s}");
        assert!(final_cut_sub([300.0, 200.0], [0.0, 200.0], 1000.0, 500.0, &p).is_none());
    }

    #[test]
    fn a_cut_off_line_is_written_like_the_lc5_example() {
        // The example's FINAL_CUT: ORIG(172.4,-0.02), 1219.24 long in +Y.
        // Sheet 1200.15 x 1219.2; the line from the bottom edge to the top.
        let s = final_cut_sub([172.4, -0.02], [172.4, 1219.22], 1200.15, 1219.2, &AdiParams::default()).unwrap();
        assert!(s.contains("INITPART(1)\r\n;\r\nNEXT_L(0,10,0,0,3,1,0,20,0,-10)\r\nG01 G40 X0. Y10.\r\nPROFILE\r\nG01 X0. Y1209.24\r\nHD_DWN_OFF\r\nG01 X0. Y1229.24\r\nENDGEO\r\n"), "{s}");
    }

    #[test]
    fn a_part_in_a_hole_is_called_before_its_frame() {
        // CO2: the frame's empty hole picked first, then its outside. The plan
        // cuts the empty hole, the part in the other hole, then the rest of the
        // frame; the frame's call (all of it) must still follow the part.
        let r = |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let g = crate::dxf::CustomGeo { w: 400.0, h: 200.0, outline: r(0.0, 0.0, 400.0, 200.0), holes: vec![r(20.0, 20.0, 180.0, 180.0), r(220.0, 20.0, 380.0, 180.0)], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], contours: vec![], hand_order: vec![] };
        let p = |name: &str, shape, w, h, custom| Part { name: name.into(), shape, w, h, qty: 1, rotate: false, color: 0, tech: None, custom, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let parts = vec![p("FRAME", Shape::Custom, 400.0, 200.0, Some(std::sync::Arc::new(g))), p("TAB", Shape::Rectangle, 50.0, 50.0, None)];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 400.0, h: 200.0, turns: 0, mirror: false }, Placed { part: 1, x: 285.0, y: 85.0, w: 50.0, h: 50.0, turns: 0, mirror: false }];
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let picks = [crate::cnc::ManualCut { placed: 0, contour: 0, start: [30.0, 100.0], reverse: false, whole: None }, crate::cnc::ManualCut { placed: 0, contour: 2, start: [10.0, 100.0], reverse: false, whole: None }];
        let plan = crate::cnc::plan_manual(&parts, &sheet, &spec, &picks);
        let s = sheet_set(1, &parts, &sheet, &spec, &plan, &[], &BlmSettings::for_spec(&spec, "LC5", "J-1"));
        let main = &s.spf[0].1;
        assert!(main.find("J1X1702_spf").unwrap() < main.find("J1X1701_spf").unwrap(), "{main}");
    }

    #[test]
    fn the_lc5_settings_name_the_jobs_gas_and_never_call_oxygen_2() {
        // GS10, GS11: the job's gas from the list; mild steel isn't oxygen by
        // itself any more (the old fallback numbered it 2).
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: Some(crate::gas::Gas::Oxygen) };
        let s = BlmSettings::for_spec(&spec, "LC5", "J-1");
        assert_eq!((s.gas_id, s.gas_name.as_str()), (3, "Oxygen"));
        let s = BlmSettings::for_spec(&SheetSpec { gas: None, ..spec }, "LC5", "J-1");
        assert_eq!((s.gas_id, s.gas_name.as_str()), (1, "Nitrogen"));
    }

    #[test]
    fn writes_the_lc5_set_and_a_readable_zip() {
        let parts = vec![Part { name: "BLANK".into(), shape: Shape::Rectangle, w: 100.0, h: 50.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let sheet = vec![Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false }, Placed { part: 0, x: 10.0, y: 70.0, w: 100.0, h: 50.0, turns: 0, mirror: false }];
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 10.0, material: 1, thickness: 6.35, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let plan = crate::cnc::plan(&parts, &sheet, &spec);
        let set = BlmSettings::for_spec(&spec, "LC5 YLS6000", "J-1");
        let s = sheet_set(1, &parts, &sheet, &spec, &plan, &[], &set);
        let names: Vec<&str> = s.spf.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, vec!["J1X1.SPF", "J1X1701.SPF", "J1X1LIS.SPF"]);
        let main = &s.spf[0].1;
        assert!(main.starts_with(";ADILAS\r\n;STAINLESS,1000,500,6.35\r\n"));
        assert!(main.contains("INITNEST\r\nORIG(10,10,0)\r\nN1 CALL\"/_n_wks_dir/_n_sheet_wpd/_n_J1X1701_spf\"\r\nORIG(10,70,0)\r\nN2 CALL"));
        assert!(main.ends_with("ENDNEST\r\nRET\r\n"));
        let sub = &s.spf[1].1;
        assert!(sub.starts_with(";M01\r\nR16=chk_pret\r\nCICLA:\r\nINITPART(1)\r\n;BLANK\r\nNEXT_L("));
        assert!(sub.contains("PROFILE\r\n") && sub.contains("LASTLINE(0,0,0,1,") && sub.contains("CORNER\r\n"));
        assert!(sub.ends_with("ENDGEO\r\nIF R16==1\r\n\t  STOPRE\r\n\t  R16=0\r\n   GOTOB CICLA\r\nENDIF\r\nENDPART\r\nRET\r\n"));
        // The list names the sheet and part programs, not itself (as in the LC5 example).
        assert_eq!(s.spf[2].1, "J1X1.SPF\r\nJ1X1701.SPF\r\n");
        assert!(s.warning.is_none());
        assert!(s.xml.contains("<NestingName>J1X1</NestingName>") && s.xml.contains("<CNCSub>J1X1701.SPF</CNCSub>") && s.xml.contains("<Quantity>2</Quantity>"));
        assert!(s.xml.contains("\t\t\t<Id3>1</Id3>") && s.xml.contains("\t\t\t<Id2>1</Id2>"));
        // The zip: local header, deflated entries that unpack to the files.
        let z = zip(&s.spf);
        assert_eq!(&z[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        let mut at = 0;
        for (name, text) in &s.spf {
            let n = u16::from_le_bytes([z[at + 26], z[at + 27]]) as usize;
            let size = u32::from_le_bytes([z[at + 18], z[at + 19], z[at + 20], z[at + 21]]) as usize;
            assert_eq!(&z[at + 30..at + 30 + n], name.as_bytes());
            let data = miniz_oxide::inflate::decompress_to_vec(&z[at + 30 + n..at + 30 + n + size]).unwrap();
            assert_eq!(data, text.as_bytes());
            at += 30 + n + size;
        }
    }
}
