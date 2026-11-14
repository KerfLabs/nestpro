//! Reading a ByStar Fiber laser job (`.bvc`, BV1–BV20) as it is: a zip of a
//! job file (`JobInfo.xml`: parts, sheets, nests) and one cut program per nest
//! (`<nest>.ccp`, a list of machine commands), each nest's parts as
//! subprograms placed by position and rotation.
//!
//! This reader shares no code with any program writer: it is also the
//! independent check a written `.bvc` is read back with before it goes out
//! (spec #122). It keeps what the file says — every cut with its operation
//! mode, lead-in, lead-out, joints and pierce — in millimetres; `nesting`
//! turns that into the app's parts and sheets.

use crate::dxf::{Contour, ContourState, CustomGeo};
use crate::nest::{CutQuality, Part, Placed, P};
use std::collections::BTreeMap;

/// Millimetres in an inch: an `English` file's lengths are inches (BV2, BV8).
pub const MM_PER_INCH: f64 = 25.4;
/// A cut-out whose path ends this close to where it started (mm) is closed.
pub const CLOSE_TOL: f64 = 0.05;
/// An arc sweeping less than this (degrees) is drawn as a straight line.
pub const FLAT_SWEEP_DEG: f64 = 1e-9;
/// Arcs are drawn with a point at least every this many degrees.
pub const ARC_STEP_DEG: f64 = 5.0;

// ---------------------------------------------------------------------------
// XML: the files are plain serializer output (BV1a): elements and
// attributes, no namespace. Order and unknown fields are not checked.
// ---------------------------------------------------------------------------

/// One XML element with its attributes and child elements (text is ignored:
/// the format keeps everything in attributes).
#[derive(Debug, Default)]
pub struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub kids: Vec<El>,
}

impl El {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|a| a.0 == k).map(|a| a.1.as_str())
    }
    pub fn kid(&self, name: &str) -> Option<&El> {
        self.kids.iter().find(|e| e.name == name)
    }
    pub fn kids_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.kids.iter().filter(move |e| e.name == name)
    }
    /// The element's `Id`, for messages.
    fn id(&self) -> String {
        self.attr("Id").map(|i| format!(" (Id {i})")).unwrap_or_default()
    }
}

fn unescape(s: &str) -> Result<String, String> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('&') {
        out.push_str(&rest[..a]);
        let b = rest[a..].find(';').ok_or_else(|| format!("'&' without ';' in \"{s}\""))? + a;
        let ent = &rest[a + 1..b];
        let c = match ent {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let n = if let Some(h) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                    u32::from_str_radix(h, 16).ok()
                } else {
                    ent.strip_prefix('#').and_then(|d| d.parse().ok())
                };
                n.and_then(char::from_u32).ok_or_else(|| format!("unknown entity &{ent};"))?
            }
        };
        out.push(c);
        rest = &rest[b + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Parse an XML document into its root element.
pub fn parse_xml(text: &str) -> Result<El, String> {
    let s = text.strip_prefix('\u{feff}').unwrap_or(text);
    let b = s.as_bytes();
    let line = |at: usize| s[..at.min(s.len())].matches('\n').count() + 1;
    let mut stack: Vec<El> = vec![];
    let mut root: Option<El> = None;
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            if !b[i].is_ascii_whitespace() && stack.is_empty() {
                return Err(format!("line {}: text outside the document's element", line(i)));
            }
            i += 1;
            continue;
        }
        let skip_to = |from: usize, end: &str| s[from..].find(end).map(|k| from + k + end.len());
        if s[i..].starts_with("<?") {
            i = skip_to(i, "?>").ok_or_else(|| format!("line {}: unclosed <?", line(i)))?;
            continue;
        }
        if s[i..].starts_with("<!--") {
            i = skip_to(i, "-->").ok_or_else(|| format!("line {}: unclosed comment", line(i)))?;
            continue;
        }
        if s[i..].starts_with("<![CDATA[") {
            i = skip_to(i, "]]>").ok_or_else(|| format!("line {}: unclosed CDATA", line(i)))?;
            continue;
        }
        if s[i..].starts_with("<!") {
            i = skip_to(i, ">").ok_or_else(|| format!("line {}: unclosed <!", line(i)))?;
            continue;
        }
        if s[i..].starts_with("</") {
            let e = skip_to(i, ">").ok_or_else(|| format!("line {}: unclosed end tag", line(i)))?;
            let name = s[i + 2..e - 1].trim();
            let open = stack.pop().ok_or_else(|| format!("line {}: </{name}> closes nothing", line(i)))?;
            if open.name != name {
                return Err(format!("line {}: </{name}> closes <{}>", line(i), open.name));
            }
            match stack.last_mut() {
                Some(p) => p.kids.push(open),
                None if root.is_none() => root = Some(open),
                None => return Err(format!("line {}: a second document element", line(i))),
            }
            i = e;
            continue;
        }
        // A start tag: its name, attributes, and whether it closes itself.
        let mut j = i + 1;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'/' && b[j] != b'>' {
            j += 1;
        }
        let mut el = El { name: s[i + 1..j].to_string(), ..Default::default() };
        if el.name.is_empty() {
            return Err(format!("line {}: a tag without a name", line(i)));
        }
        let closed;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            match b.get(j) {
                None => return Err(format!("line {}: <{}> never ends", line(i), el.name)),
                Some(b'>') => {
                    closed = false;
                    j += 1;
                    break;
                }
                Some(b'/') if b.get(j + 1) == Some(&b'>') => {
                    closed = true;
                    j += 2;
                    break;
                }
                _ => {}
            }
            let k0 = j;
            while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let key = s[k0..j].to_string();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if key.is_empty() || b.get(j) != Some(&b'=') {
                return Err(format!("line {}: <{}> has a broken attribute", line(j), el.name));
            }
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let q = *b.get(j).ok_or_else(|| format!("line {}: <{}> never ends", line(i), el.name))?;
            if q != b'"' && q != b'\'' {
                return Err(format!("line {}: {key} on <{}> has no quotes", line(j), el.name));
            }
            let v0 = j + 1;
            let v1 = s[v0..].find(q as char).map(|k| v0 + k).ok_or_else(|| format!("line {}: {key} on <{}> never ends", line(j), el.name))?;
            el.attrs.push((key, unescape(&s[v0..v1]).map_err(|e| format!("line {}: {e}", line(j)))?));
            j = v1 + 1;
        }
        if closed {
            match stack.last_mut() {
                Some(p) => p.kids.push(el),
                None if root.is_none() => root = Some(el),
                None => return Err(format!("line {}: a second document element", line(i))),
            }
        } else {
            stack.push(el);
        }
        i = j;
    }
    if let Some(open) = stack.last() {
        return Err(format!("the file ends inside <{}>", open.name));
    }
    root.ok_or_else(|| "no document element".to_string())
}

/// A number as the format writes it: invariant culture, `INF`, `-INF`, `NaN`.
fn number(el: &El, k: &str) -> Result<Option<f64>, String> {
    let Some(v) = el.attr(k) else { return Ok(None) };
    let t = v.trim();
    let n = match t {
        "INF" => f64::INFINITY,
        "-INF" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => t.parse::<f64>().ok().filter(|_| !t.is_empty() && !t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E')).ok_or_else(|| format!("<{}>{}: {k}=\"{v}\" isn't a number", el.name, el.id()))?,
    };
    Ok(Some(n))
}

fn num_or(el: &El, k: &str, d: f64) -> Result<f64, String> {
    Ok(number(el, k)?.unwrap_or(d))
}

fn int_or(el: &El, k: &str, d: i64) -> Result<i64, String> {
    match el.attr(k) {
        None => Ok(d),
        Some(v) => v.trim().parse::<i64>().map_err(|_| format!("<{}>{}: {k}=\"{v}\" isn't a whole number", el.name, el.id())),
    }
}

/// A position that must be there and be a real number.
fn coord(el: &El, k: &str) -> Result<f64, String> {
    let v = number(el, k)?.ok_or_else(|| format!("<{}>{} has no {k}", el.name, el.id()))?;
    if !v.is_finite() {
        return Err(format!("<{}>{}: {k} is {v}, not a position", el.name, el.id()));
    }
    Ok(v)
}

fn choice<'a>(el: &El, k: &str, names: &[&'a str], d: &'a str) -> Result<&'a str, String> {
    match el.attr(k) {
        None => Ok(d),
        Some(v) => names.iter().find(|n| **n == v.trim()).copied().ok_or_else(|| format!("<{}>{}: {k}=\"{v}\" isn't one of {}", el.name, el.id(), names.join(", "))),
    }
}

// ---------------------------------------------------------------------------
// The cut program (`.ccp`, BV6–BV8).
// ---------------------------------------------------------------------------

/// One move along a cut: straight, or an arc to its end point with a signed
/// sweep in degrees, counter-clockwise positive (BV7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(P),
    Arc(P, f64),
}

impl Seg {
    pub fn end(&self) -> P {
        match *self {
            Seg::Line(p) | Seg::Arc(p, _) => p,
        }
    }
}

/// Which side of the part a cut-out is (`BeginCutOut Contour=`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    #[default]
    Unknown,
    Interior,
    Exterior,
}

/// Kerf compensation side (`SetToolCorrection`), done by the machine (BV8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Correction {
    #[default]
    None,
    Left,
    Right,
}

/// How a pierce is made (`SetPiercingMode`, BV12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Piercing {
    #[default]
    Normal,
    Fine,
    Drilled,
    NoTime,
}

/// A path: where it starts and its moves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub start: P,
    pub segs: Vec<Seg>,
}

impl Path {
    pub fn end(&self) -> P {
        self.segs.last().map_or(self.start, Seg::end)
    }
    /// The path as points, arcs drawn with a point at least every
    /// `ARC_STEP_DEG` and within the drawing tolerance.
    pub fn points(&self) -> Vec<P> {
        let mut out = vec![self.start];
        let mut at = self.start;
        for s in &self.segs {
            match *s {
                Seg::Line(p) => out.push(p),
                Seg::Arc(p, sweep) => out.extend(arc_points(at, p, sweep)),
            }
            at = s.end();
        }
        out
    }
}

/// The centre of the arc from `a` to `b` sweeping `sweep` degrees: on the
/// chord's normal, (chord / 2) / tan(sweep / 2) from its middle (BV7). None
/// for an arc too flat to have one, or one that ends where it starts.
pub fn arc_centre(a: P, b: P, sweep: f64) -> Option<P> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let c = d[0].hypot(d[1]);
    if sweep.abs() < FLAT_SWEEP_DEG || c < 1e-12 || !sweep.is_finite() {
        return None;
    }
    let h = (c / 2.0) / (sweep.to_radians() / 2.0).tan();
    let n = [-d[1] / c, d[0] / c];
    Some([(a[0] + b[0]) / 2.0 + n[0] * h, (a[1] + b[1]) / 2.0 + n[1] * h])
}

/// Points along an arc from `a` to `b` sweeping `sweep` degrees (not
/// including `a`, ending exactly on `b`).
pub fn arc_points(a: P, b: P, sweep: f64) -> Vec<P> {
    let Some(c) = arc_centre(a, b, sweep) else { return vec![b] };
    let r = (a[0] - c[0]).hypot(a[1] - c[1]);
    let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let sw = sweep.to_radians();
    let n = ((sweep.abs() / ARC_STEP_DEG).ceil() as usize).max(crate::dxf::arc_segments(r, sw.abs())).max(1);
    let mut v: Vec<P> = (1..n).map(|k| {
        let t = a0 + sw * k as f64 / n as f64;
        [c[0] + r * t.cos(), c[1] + r * t.sin()]
    }).collect();
    v.push(b);
    v
}

/// One cut-out (`BeginCutOut` … `EndCutOut`) with what belongs to it: the
/// lead-in before it, the lead-out after it and the joints on it (BV7,
/// BV16). Only `path` is the part's shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cut {
    /// The operation mode (the machine's technology column) it is cut in;
    /// None when no `SetOperationMode` came before it.
    pub mode: Option<i64>,
    pub side: Side,
    pub correction: Correction,
    /// Where the beam pierced, and how.
    pub pierce: Option<(P, Piercing)>,
    pub lead_in: Option<Path>,
    pub path: Path,
    pub lead_out: Option<Path>,
    /// Micro-joint markers: points on the path; the machine leaves the gap
    /// (BV16).
    pub joints: Vec<P>,
    /// The head is lifted after it (`ExecHighPositioning`, BV15).
    pub lift: bool,
    /// Points along the path where height sensing was switched off / on (BV14).
    pub sensing: Vec<(P, bool)>,
}

impl Cut {
    pub fn closed(&self) -> bool {
        let (a, b) = (self.path.start, self.path.end());
        !self.path.segs.is_empty() && (a[0] - b[0]).hypot(a[1] - b[1]) < CLOSE_TOL
    }
}

/// A subprogram: one part's cuts, or a cut-off's (BV6).
#[derive(Clone, Debug, Default)]
pub struct Sub {
    pub name: String,
    pub cuts: Vec<Cut>,
}

/// A subprogram placed on the sheet: turned `rotation` degrees
/// counter-clockwise about its own origin, then moved by `x`, `y` (BV6).
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// The subprogram, by its place in `Program::subs`.
    pub sub: usize,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

/// A cut program read, lengths in mm.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub inch: bool,
    pub thickness: f64,
    /// The sheet (`ApplicationFlat`), 0 when not given.
    pub sheet: [f64; 2],
    pub subs: Vec<Sub>,
    pub calls: Vec<Call>,
    /// Commands this reader doesn't know, by name, and how many: skipped.
    pub unknown: BTreeMap<String, usize>,
    /// Commands it knows but doesn't use for the nest (tube, sorting, dwell…).
    pub unread: BTreeMap<String, usize>,
}

/// Commands of the format that don't change a flat nest's shape (BV1e):
/// read and passed over.
const PASSED_OVER: &[&str] = &[
    "SetClamps", "SetProjectionMode", "SetRepositionArea", "SetToolSetup", "SetIndexMark", "ExecDwell", "ExecBonding", "ExecExternalProcess",
    "ExecOptionalStop", "ExecOptionalStopWithoutLifting", "BeginCorner", "EndCorner",
];
/// Commands of the format for tubes (rotary axis): not read on a flat nest.
const TUBE: &[&str] = &["RotaxTransitionTo"];

#[derive(PartialEq, Clone, Copy)]
enum Bracket {
    Out,
    LeadIn,
    CutOut,
    /// A lead-out after its cut-out (the machine's CAM, BV7).
    LeadOut,
    /// A lead-out inside its cut-out, before `EndCutOut` (the original's
    /// post, BV34).
    LeadOutIn,
}

/// The commands of one subprogram as cuts (BV7): moves inside
/// `BeginCutOut`/`EndCutOut` are the cut, moves in the lead brackets (or,
/// without brackets, between the pierce and the cut-out, or after it) are its
/// leads, never part of its shape.
fn read_cuts(cmds: &El, k: f64, p: &mut Program) -> Result<Vec<Cut>, String> {
    let mut cuts: Vec<Cut> = vec![];
    let (mut pos, mut mode, mut corr, mut pmode) = ([0.0, 0.0], None, Correction::None, Piercing::Normal);
    let mut pierced: Option<(P, Piercing)> = None;
    let mut lead: Option<Path> = None;
    let mut cur: Option<Cut> = None;
    let mut br = Bracket::Out;
    // Whether the last cut-out's lead-out may still follow (no pierce since).
    let mut after_cut = false;
    for c in &cmds.kids {
        let xy = |c: &El| -> Result<P, String> { Ok([coord(c, "X")? * k, coord(c, "Y")? * k]) };
        let mv = match c.name.as_str() {
            "LineTo" | "CornerTransitionTo" => Some(Seg::Line(xy(c)?)),
            "ArcTo" => {
                let sw = num_or(c, "SweepAngle", 0.0)?;
                if !sw.is_finite() {
                    return Err(format!("<ArcTo>{}: SweepAngle is {sw}", c.id()));
                }
                let to = xy(c)?;
                if sw.abs() >= FLAT_SWEEP_DEG && (to[0] - pos[0]).hypot(to[1] - pos[1]) < 1e-9 {
                    // An arc back to where it starts has no centre to draw it by.
                    *p.unread.entry("arc that ends where it starts".into()).or_default() += 1;
                    continue;
                }
                Some(if sw.abs() < FLAT_SWEEP_DEG { Seg::Line(to) } else { Seg::Arc(to, sw) })
            }
            _ => None,
        };
        if let Some(s) = mv {
            match br {
                Bracket::CutOut => cur.as_mut().unwrap().path.segs.push(s),
                Bracket::LeadOutIn => cur.as_mut().unwrap().lead_out.get_or_insert(Path { start: pos, segs: vec![] }).segs.push(s),
                Bracket::LeadIn => lead.get_or_insert(Path { start: pos, segs: vec![] }).segs.push(s),
                Bracket::LeadOut => cuts.last_mut().unwrap().lead_out.get_or_insert(Path { start: pos, segs: vec![] }).segs.push(s),
                Bracket::Out if after_cut => cuts.last_mut().unwrap().lead_out.get_or_insert(Path { start: pos, segs: vec![] }).segs.push(s),
                Bracket::Out => lead.get_or_insert(Path { start: pos, segs: vec![] }).segs.push(s),
            }
            pos = s.end();
            continue;
        }
        match c.name.as_str() {
            "MoveLineTo" | "MoveArcTo" => {
                pos = xy(c)?;
                match br {
                    // A rapid can't be part of a cut: the cut-out goes on from here.
                    Bracket::CutOut => *p.unread.entry("rapid inside a cut-out".into()).or_default() += 1,
                    // After a rapid, moves no longer lead out of the last cut.
                    Bracket::Out => after_cut = false,
                    _ => {}
                }
            }
            "SetOperationMode" => mode = Some(int_or(c, "Value", 0)?),
            "SetToolCorrection" => {
                corr = match choice(c, "Value", &["None", "Left", "Right"], "None")? {
                    "Left" => Correction::Left,
                    "Right" => Correction::Right,
                    _ => Correction::None,
                }
            }
            "SetPiercingMode" => {
                pmode = match choice(c, "Value", &["Normal", "Fine", "Drilled", "NoTime"], "Normal")? {
                    "Fine" => Piercing::Fine,
                    "Drilled" => Piercing::Drilled,
                    "NoTime" => Piercing::NoTime,
                    _ => Piercing::Normal,
                }
            }
            "ExecPiercing" => {
                if let Some(cut) = cur.as_mut().filter(|c| c.pierce.is_none()) {
                    cut.pierce = Some((pos, pmode));
                } else {
                    pierced = Some((pos, pmode));
                    after_cut = false;
                }
            }
            "BeginLeadIn" => {
                br = Bracket::LeadIn;
                after_cut = false;
                lead = Some(Path { start: pos, segs: vec![] });
            }
            "EndLeadIn" => br = Bracket::Out,
            "EndLeadOut" => br = if br == Bracket::LeadOutIn { Bracket::CutOut } else { Bracket::Out },
            "BeginCutOut" => {
                if let Some(open) = cur.take() {
                    cuts.push(open);
                }
                let side = match choice(c, "Contour", &["Unknown", "Interior", "Exterior"], "Unknown")? {
                    "Interior" => Side::Interior,
                    "Exterior" => Side::Exterior,
                    _ => Side::Unknown,
                };
                cur = Some(Cut { mode, side, pierce: pierced.take(), lead_in: lead.take().filter(|l| !l.segs.is_empty()), path: Path { start: pos, segs: vec![] }, ..Default::default() });
                br = Bracket::CutOut;
                after_cut = false;
            }
            "EndCutOut" => {
                if let Some(mut cut) = cur.take() {
                    cut.correction = corr;
                    cuts.push(cut);
                    after_cut = true;
                }
                br = Bracket::Out;
            }
            "BeginLeadOut" => {
                br = if cur.is_some() {
                    Bracket::LeadOutIn
                } else if !cuts.is_empty() {
                    Bracket::LeadOut
                } else {
                    *p.unread.entry("lead-out without a cut-out".into()).or_default() += 1;
                    Bracket::LeadIn
                };
            }
            "ExecMicrojoint" => match cur.as_mut().filter(|_| br == Bracket::CutOut) {
                Some(cut) => cut.joints.push(pos),
                None => *p.unread.entry("micro-joint outside a cut-out".into()).or_default() += 1,
            },
            "SetSensingSensor" => {
                let on = choice(c, "Value", &["Off", "On"], "On")? == "On";
                if let Some(cut) = cur.as_mut() {
                    cut.sensing.push((pos, on));
                }
            }
            "ExecHighPositioning" => {
                if let Some(cut) = cuts.last_mut().filter(|_| cur.is_none()) {
                    cut.lift = true;
                }
            }
            "CallSubProgram" => *p.unread.entry("subprogram called from a subprogram".into()).or_default() += 1,
            n if PASSED_OVER.contains(&n) => {}
            n if TUBE.contains(&n) => *p.unread.entry(n.to_string()).or_default() += 1,
            n => *p.unknown.entry(n.to_string()).or_default() += 1,
        }
    }
    if let Some(open) = cur.take() {
        // A cut-out the program never ends: kept as cut so far.
        *p.unread.entry("cut-out without EndCutOut".into()).or_default() += 1;
        cuts.push(Cut { correction: corr, ..open });
    }
    Ok(cuts)
}

/// Read a cut program (`.ccp`): its unit, thickness and sheet, each
/// subprogram's cuts and the main program's placements (BV6–BV8), lengths in
/// mm. A call to a subprogram that isn't there is refused (BV1b).
pub fn parse_program(text: &str) -> Result<Program, String> {
    let root = parse_xml(text)?;
    if root.name != "CuttingControlProgram" {
        return Err(format!("it isn't a cut program (its root is <{}>)", root.name));
    }
    let inch = choice(&root, "MeasurementSystem", &["Metric", "English", "Imperial"], "Metric")? != "Metric";
    let k = if inch { MM_PER_INCH } else { 1.0 };
    let mut p = Program { inch, thickness: num_or(&root, "Thickness", 0.0)? * k, ..Default::default() };
    if let Some(a) = root.kid("ApplicationFlat") {
        p.sheet = [num_or(a, "SheetWidth", 0.0)? * k, num_or(a, "SheetHeight", 0.0)? * k];
    }
    if root.kid("ApplicationTube").is_some() {
        return Err("it is a tube program; only flat sheets open here".into());
    }
    let mut seen: Vec<String> = vec![];
    for s in root.kid("SubPrograms").map(|s| s.kids_named("SubProgram").collect::<Vec<_>>()).unwrap_or_default() {
        let name = s.attr("Name").unwrap_or("").to_string();
        if seen.contains(&name.to_lowercase()) {
            return Err(format!("it has two subprograms named '{name}'"));
        }
        seen.push(name.to_lowercase());
        let cuts = match s.kid("ControlCommands") {
            Some(cmds) => read_cuts(cmds, k, &mut p).map_err(|e| format!("subprogram '{name}': {e}"))?,
            None => vec![],
        };
        p.subs.push(Sub { name, cuts });
    }
    if let Some(main) = root.kid("ControlCommands") {
        for c in &main.kids {
            match c.name.as_str() {
                "CallSubProgram" => {
                    let name = c.attr("SubProgramName").unwrap_or("");
                    // Names are matched ignoring case (BV1b).
                    let sub = p.subs.iter().position(|s| s.name.eq_ignore_ascii_case(name)).ok_or_else(|| format!("it places subprogram '{name}', which isn't in it"))?;
                    let rotation = num_or(c, "Rotation", 0.0)?;
                    if !rotation.is_finite() {
                        return Err(format!("<CallSubProgram>{}: Rotation is {rotation}", c.id()));
                    }
                    p.calls.push(Call { sub, x: coord(c, "X")? * k, y: coord(c, "Y")? * k, rotation });
                }
                "LineTo" | "ArcTo" | "MoveLineTo" | "MoveArcTo" | "CornerTransitionTo" => *p.unread.entry("move in the main program".into()).or_default() += 1,
                n if is_known(n) => {}
                n => *p.unknown.entry(n.to_string()).or_default() += 1,
            }
        }
    }
    Ok(p)
}

/// Every command the format has (BV1e and the model's list).
fn is_known(n: &str) -> bool {
    const ALL: &[&str] = &[
        "MoveLineTo", "MoveArcTo", "LineTo", "ArcTo", "CornerTransitionTo", "RotaxTransitionTo", "CallSubProgram", "SubProgram", "BeginCutOut", "EndCutOut", "BeginLeadIn",
        "EndLeadIn", "BeginLeadOut", "EndLeadOut", "BeginCorner", "EndCorner", "ExecPiercing", "ExecMicrojoint", "ExecHighPositioning", "ExecDwell", "ExecBonding",
        "ExecExternalProcess", "ExecOptionalStop", "ExecOptionalStopWithoutLifting", "SetOperationMode", "SetPiercingMode", "SetToolCorrection", "SetSensingSensor", "SetClamps",
        "SetProjectionMode", "SetRepositionArea", "SetToolSetup", "SetIndexMark",
    ];
    ALL.contains(&n)
}

// ---------------------------------------------------------------------------
// The job file (`JobInfo.xml`, BV2–BV5).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartInfo {
    pub id: i64,
    pub name: String,
    /// How many are ordered.
    pub quantity: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialInfo {
    pub id: i64,
    /// Sheet size, mm.
    pub size: [f64; 2],
}

/// One part's subprogram in a nest, and how many times the nest places it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanPart {
    pub part_id: i64,
    pub quantity: i64,
    pub sub: String,
}

/// One nest (BV5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanInfo {
    /// Its cut program's name (`<name>.ccp`).
    pub name: String,
    /// How many sheets are cut with it.
    pub multiplicity: i64,
    pub material_id: i64,
    pub parts: Vec<PlanPart>,
}

/// The job file read, lengths in mm.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobInfo {
    pub name: String,
    /// The material as written (free text, e.g. "304 SS").
    pub material: String,
    pub thickness: f64,
    pub inch: bool,
    /// The cutting gas as written (e.g. "N2"); empty when not given.
    pub gas: String,
    pub parts: Vec<PartInfo>,
    pub materials: Vec<MaterialInfo>,
    pub plans: Vec<PlanInfo>,
}

/// Read the job file. A nest naming a sheet or a part that isn't in it is
/// refused, as are two parts or sheets with one id (BV1b).
pub fn parse_job_info(text: &str) -> Result<JobInfo, String> {
    let root = parse_xml(text)?;
    if root.name != "JobInfo" {
        return Err(format!("it isn't a job file (its root is <{}>)", root.name));
    }
    let inch = choice(&root, "MeasurementSystem", &["Metric", "English", "Imperial"], "Metric")? != "Metric";
    let k = if inch { MM_PER_INCH } else { 1.0 };
    let mut j = JobInfo {
        name: root.attr("Name").unwrap_or("").to_string(),
        material: root.attr("Material").unwrap_or("").to_string(),
        thickness: num_or(&root, "Thickness", 0.0)? * k,
        inch,
        gas: root.attr("ProcessGas").unwrap_or("").to_string(),
        ..Default::default()
    };
    for p in root.kid("PartInfos").into_iter().flat_map(|e| e.kids_named("PartInfo")) {
        let id = int_or(p, "Id", 0)?;
        if j.parts.iter().any(|q| q.id == id) {
            return Err(format!("it has two parts with id {id}"));
        }
        j.parts.push(PartInfo { id, name: p.attr("Name").unwrap_or("").to_string(), quantity: int_or(p, "Quantity", 0)? });
    }
    for m in root.kid("MaterialInfos").into_iter().flat_map(|e| e.kids_named("MaterialInfo")) {
        let id = int_or(m, "Id", 0)?;
        if j.materials.iter().any(|q| q.id == id) {
            return Err(format!("it has two sheets with id {id}"));
        }
        j.materials.push(MaterialInfo { id, size: [num_or(m, "SizeX", 0.0)? * k, num_or(m, "SizeY", 0.0)? * k] });
    }
    for pl in root.kid("PlanInfos").into_iter().flat_map(|e| e.kids_named("PlanInfo")) {
        let name = pl.attr("Name").unwrap_or("").to_string();
        let material_id = int_or(pl, "MaterialId", 0)?;
        if !j.materials.iter().any(|m| m.id == material_id) {
            return Err(format!("nest '{name}' is on sheet id {material_id}, which the job file doesn't list"));
        }
        let mut plan = PlanInfo { name: name.clone(), multiplicity: int_or(pl, "Multiplicity", 1)?, material_id, parts: vec![] };
        for pp in pl.kid("PlanPartInfos").into_iter().flat_map(|e| e.kids_named("PlanPartInfo")) {
            let part_id = int_or(pp, "PartId", 0)?;
            if !j.parts.iter().any(|p| p.id == part_id) {
                return Err(format!("nest '{name}' places part id {part_id}, which the job file doesn't list"));
            }
            plan.parts.push(PlanPart { part_id, quantity: int_or(pp, "Quantity", 0)?, sub: pp.attr("SubProgramName").unwrap_or("").to_string() });
        }
        j.plans.push(plan);
    }
    Ok(j)
}

// ---------------------------------------------------------------------------
// The container (BV1).
// ---------------------------------------------------------------------------

/// One nest: its entry in the job file and its cut program.
#[derive(Clone, Debug)]
pub struct Nest {
    pub plan: PlanInfo,
    pub program: Program,
}

/// A `.bvc` read as it is.
#[derive(Clone, Debug)]
pub struct Bvc {
    pub job: JobInfo,
    pub nests: Vec<Nest>,
    pub notes: Vec<String>,
}

/// Read a `.bvc` file's bytes: the job file and every nest's cut program.
/// Entries are found by their name without folders, ignoring case (BV1).
pub fn read(bytes: &[u8]) -> Result<Bvc, String> {
    let files = crate::progimport::read_zip(bytes).map_err(|e| if e == "not a zip file" { "it isn't a ByStar Fiber job (.bvc): not a zip file".to_string() } else { format!("it isn't a readable zip ({e})") })?;
    let leaf = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_lowercase();
    let text = |b: &[u8]| String::from_utf8_lossy(b).to_string();
    let info = files.iter().find(|f| leaf(&f.0) == "jobinfo.xml").ok_or("it has no job file (JobInfo.xml), so it isn't a ByStar Fiber job (.bvc)")?;
    let job = parse_job_info(&text(&info.1)).map_err(|e| format!("its job file (JobInfo.xml) doesn't read: {e}"))?;
    if job.plans.is_empty() {
        return Err("its job file lists no nests (PlanInfo)".into());
    }
    let mut nests = vec![];
    for plan in &job.plans {
        let want = format!("{}.ccp", plan.name).to_lowercase();
        let f = files.iter().find(|f| leaf(&f.0) == want).ok_or_else(|| format!("nest '{}' has no cut program ({}.ccp) in it", plan.name, plan.name))?;
        let program = parse_program(&text(&f.1)).map_err(|e| format!("the cut program {}.ccp doesn't read: {e}", plan.name))?;
        // Each part the job file puts in this nest must be cut by a
        // subprogram the nest's cut program has (names ignoring case, BV1b).
        for pp in &plan.parts {
            if !program.subs.iter().any(|s| s.name.eq_ignore_ascii_case(&pp.sub)) {
                let part = job.parts.iter().find(|q| q.id == pp.part_id).map(|q| q.name.as_str()).unwrap_or("");
                return Err(format!(
                    "its job file says part '{part}' (id {}) is cut in nest '{}' by subprogram '{}', which that nest's cut program ({}.ccp) doesn't have, so the part's count and shape can't be trusted. Write the job again from the CAM",
                    pp.part_id, plan.name, pp.sub, plan.name
                ));
            }
        }
        nests.push(Nest { plan: plan.clone(), program });
    }
    let mut notes = vec![];
    let extra: Vec<&str> = files.iter().filter(|f| leaf(&f.0).ends_with(".ccp") && !job.plans.iter().any(|p| leaf(&f.0) == format!("{}.ccp", p.name).to_lowercase())).map(|f| f.0.as_str()).collect();
    if !extra.is_empty() {
        notes.push(format!("cut program(s) the job file doesn't list, not opened: {}", extra.join(", ")));
    }
    let mut unknown: BTreeMap<String, usize> = BTreeMap::new();
    let mut unread: BTreeMap<String, usize> = BTreeMap::new();
    for n in &nests {
        for (k, v) in &n.program.unknown {
            *unknown.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &n.program.unread {
            *unread.entry(k.clone()).or_default() += v;
        }
    }
    let list = |m: &BTreeMap<String, usize>| m.iter().map(|(k, v)| format!("{k} ×{v}")).collect::<Vec<_>>().join(", ");
    if !unknown.is_empty() {
        notes.push(format!("skipped {} command(s) this reader doesn't know: {}", unknown.values().sum::<usize>(), list(&unknown)));
    }
    if !unread.is_empty() {
        notes.push(format!("not read: {}", list(&unread)));
    }
    Ok(Bvc { job, nests, notes })
}

// ---------------------------------------------------------------------------
// As the app's parts and sheets (BV9–BV20).
// ---------------------------------------------------------------------------

/// What an operation mode (the machine's technology column) becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeAs {
    Quality(CutQuality),
    Mark,
    /// Not one of the machine's eight columns.
    Unknown,
}

/// An operation mode as a cut quality: the post's quality-to-mode rule with
/// its default switch (BV32, B015 off: quality 2 → mode 1, 3–7 → 3–7, the
/// others → 0) taken the other way. Mode 0 → quality 1 (the lowest of those
/// that give 0), 1 → 2, 3–7 → 3–7; mode 2 (engraving) → Mark (BV9).
pub fn mode_as(mode: i64) -> ModeAs {
    match mode {
        2 => ModeAs::Mark,
        0 => ModeAs::Quality(CutQuality::new(1).unwrap()),
        1 => ModeAs::Quality(CutQuality::new(2).unwrap()),
        3..=7 => ModeAs::Quality(CutQuality::new(mode as usize).unwrap()),
        _ => ModeAs::Unknown,
    }
}

fn rot(p: P, deg: f64) -> P {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn area(p: &[P]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() * 0.5
}

/// A part subprogram made into a part's drawing, in its own coordinates.
#[derive(Clone, Debug)]
pub struct SubPart {
    pub geo: CustomGeo,
    /// Where the part's (0, 0) is in the subprogram's coordinates.
    pub origin: P,
    /// Each cut contour's start (where its lead meets it), by its place in the
    /// part's cut order (holes first, the outline last), part coordinates.
    pub starts: Vec<(usize, P)>,
    /// Micro-joints: (cut contour, point), part coordinates.
    pub joints: Vec<(usize, P)>,
    /// Open cut paths kept as no-cut lines (the app cuts closed contours).
    pub open_cuts: usize,
    /// Contours whose mode isn't one of the machine's columns.
    pub unknown_modes: Vec<i64>,
    /// Each contour's cut path as the file gives it (lines and true arcs),
    /// in the order of `geo.contours`, part coordinates.
    pub paths: Vec<Path>,
}

/// A subprogram as a part: its closed cut-outs the outline (the exterior
/// one, else the biggest) and holes, each with the cut quality of its mode;
/// engraving (mode 2) as Mark; leads and joints kept apart from the shape.
/// None when it cuts no closed contour (a cut-off).
pub fn sub_part(sub: &Sub) -> Option<SubPart> {
    let cut = |c: &Cut| c.mode.map(mode_as) != Some(ModeAs::Mark);
    let closed: Vec<usize> = (0..sub.cuts.len()).filter(|&k| cut(&sub.cuts[k]) && sub.cuts[k].closed()).collect();
    if closed.is_empty() {
        return None;
    }
    let pts = |c: &Cut| {
        let mut v = c.path.points();
        if c.closed() {
            v.pop();
        }
        v
    };
    let big = |ks: &[usize]| ks.iter().copied().max_by(|&a, &b| area(&pts(&sub.cuts[a])).abs().total_cmp(&area(&pts(&sub.cuts[b])).abs()));
    let ext: Vec<usize> = closed.iter().copied().filter(|&k| sub.cuts[k].side == Side::Exterior).collect();
    let oi = big(&ext).or_else(|| big(&closed))?;
    let outline = pts(&sub.cuts[oi]);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in &outline {
        lo = [lo[0].min(q[0]), lo[1].min(q[1])];
        hi = [hi[0].max(q[0]), hi[1].max(q[1])];
    }
    let sh = |q: P| [q[0] - lo[0], q[1] - lo[1]];
    let empty = CustomGeo { outline: vec![], holes: vec![], fills: vec![], hole_fills: vec![], marks: vec![], texts: vec![], w: 0.0, h: 0.0, contours: vec![], hand_order: vec![] };
    let mut out = SubPart { geo: empty, origin: lo, starts: vec![], joints: vec![], open_cuts: 0, unknown_modes: vec![], paths: vec![] };
    // Contours in the app's numbering (CS1): closed ones inside the outline,
    // the outline, then the open lines; each with the cut it came from.
    let mut list: Vec<(Contour, usize)> = vec![];
    let mut open: Vec<(Contour, usize)> = vec![];
    for (k, c) in sub.cuts.iter().enumerate() {
        if k == oi || c.path.segs.is_empty() {
            continue;
        }
        let p: Vec<P> = pts(c).into_iter().map(sh).collect();
        let mark = c.mode.map(mode_as) == Some(ModeAs::Mark);
        let state = if mark {
            ContourState::Mark
        } else if c.closed() {
            ContourState::Cut
        } else {
            out.open_cuts += 1;
            ContourState::NoCut
        };
        let mut ct = Contour::new(p, c.closed(), state);
        if state == ContourState::Cut {
            ct.quality = quality(c, &mut out.unknown_modes);
        }
        if c.closed() { list.push((ct, k)) } else { open.push((ct, k)) }
    }
    let mut o = Contour::new(outline.iter().map(|&q| sh(q)).collect(), true, ContourState::Cut);
    o.quality = quality(&sub.cuts[oi], &mut out.unknown_modes);
    list.push((o, oi));
    list.extend(open);
    let contours: Vec<Contour> = list.iter().map(|c| c.0.clone()).collect();
    out.paths = list
        .iter()
        .map(|c| {
            let p = &sub.cuts[c.1].path;
            let segs = p.segs.iter().map(|s| match *s {
                Seg::Line(q) => Seg::Line(sh(q)),
                Seg::Arc(q, sw) => Seg::Arc(sh(q), sw),
            });
            Path { start: sh(p.start), segs: segs.collect() }
        })
        .collect();
    for (ci, &k) in crate::dxf::cut_order(&contours).iter().enumerate() {
        let c = &sub.cuts[list[k].1];
        out.starts.push((ci, sh(c.path.start)));
        out.joints.extend(c.joints.iter().map(|&j| (ci, sh(j))));
    }
    out.geo.contours = contours;
    (out.geo.w, out.geo.h) = (hi[0] - lo[0], hi[1] - lo[1]);
    out.geo.apply_states();
    Some(out)
}

/// Two subprograms of one part are the same version when their contours
/// (shape, state, cut quality) and their cut starts and joints all agree
/// within this (mm).
pub const SAME_TOL: f64 = 0.01;

/// Distance from `q` to the move from `a` along `s` (a true arc for an arc).
fn seg_dist(q: P, a: P, s: &Seg) -> f64 {
    let line = |b: P| {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l2 = d[0] * d[0] + d[1] * d[1];
        let t = if l2 > 0.0 { (((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
        (q[0] - a[0] - t * d[0]).hypot(q[1] - a[1] - t * d[1])
    };
    match *s {
        Seg::Line(b) => line(b),
        Seg::Arc(b, sweep) => {
            let Some(c) = arc_centre(a, b, sweep) else { return line(b) };
            let r = (a[0] - c[0]).hypot(a[1] - c[1]);
            let a0 = (a[1] - c[1]).atan2(a[0] - c[0]);
            let ang = (q[1] - c[1]).atan2(q[0] - c[0]);
            let tau = std::f64::consts::TAU;
            let along = if sweep > 0.0 { (ang - a0).rem_euclid(tau) } else { (a0 - ang).rem_euclid(tau) };
            if along <= sweep.abs().to_radians() {
                ((q[0] - c[0]).hypot(q[1] - c[1]) - r).abs()
            } else {
                (q[0] - a[0]).hypot(q[1] - a[1]).min((q[0] - b[0]).hypot(q[1] - b[1]))
            }
        }
    }
}

/// Points along a path to compare it by: its start, and each move's end
/// and middle (an arc's on the arc).
fn path_samples(p: &Path) -> Vec<P> {
    let mut v = vec![p.start];
    let mut at = p.start;
    for s in &p.segs {
        let mid = match *s {
            Seg::Line(b) => [(at[0] + b[0]) / 2.0, (at[1] + b[1]) / 2.0],
            Seg::Arc(b, sweep) => match arc_centre(at, b, sweep) {
                Some(c) => {
                    let r = rot([at[0] - c[0], at[1] - c[1]], sweep / 2.0);
                    [c[0] + r[0], c[1] + r[1]]
                }
                None => [(at[0] + b[0]) / 2.0, (at[1] + b[1]) / 2.0],
            },
        };
        v.push(mid);
        at = s.end();
        v.push(at);
    }
    v
}

/// Whether two paths are the same shape within `tol`, however their moves
/// are split: every start, end and middle of each move of one lies within
/// `tol` of the other, both ways.
pub fn same_shape(a: &Path, b: &Path, tol: f64) -> bool {
    let near = |q: P, p: &Path| {
        let mut at = p.start;
        let mut best = (q[0] - at[0]).hypot(q[1] - at[1]);
        for s in &p.segs {
            best = best.min(seg_dist(q, at, s));
            at = s.end();
        }
        best <= tol
    };
    path_samples(a).into_iter().all(|q| near(q, b)) && path_samples(b).into_iter().all(|q| near(q, a))
}

/// Whether two subprograms cut a part the same way: the same contours in
/// the same order, each the same shape (however its moves are split) with
/// the same state and cut quality, and the same cut starts and joints, all
/// within `SAME_TOL`.
pub fn same_version(a: &SubPart, b: &SubPart) -> bool {
    let near = |p: &P, q: &P| (p[0] - q[0]).hypot(p[1] - q[1]) <= SAME_TOL;
    let marks = |x: &[(usize, P)], y: &[(usize, P)]| x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.0 == q.0 && near(&p.1, &q.1));
    let (ca, cb) = (&a.geo.contours, &b.geo.contours);
    ca.len() == cb.len()
        && a.paths.len() == ca.len()
        && b.paths.len() == cb.len()
        && ca.iter().zip(cb).all(|(x, y)| x.state == y.state && x.closed == y.closed && x.quality == y.quality)
        && a.paths.iter().zip(&b.paths).all(|(x, y)| same_shape(x, y, SAME_TOL))
        && marks(&a.starts, &b.starts)
        && marks(&a.joints, &b.joints)
}

fn quality(c: &Cut, unknown: &mut Vec<i64>) -> Option<CutQuality> {
    match c.mode.map(mode_as) {
        Some(ModeAs::Quality(q)) => Some(q),
        _ => {
            unknown.push(c.mode.unwrap_or(-1));
            None
        }
    }
}

/// One nest as the app's sheet(s).
#[derive(Clone, Debug)]
pub struct Sheet {
    pub w: f64,
    pub h: f64,
    /// How many identical sheets are cut with it (at least 1).
    pub count: usize,
    pub placed: Vec<Placed>,
    /// Cut-off paths (subprograms that cut no closed contour), sheet
    /// coordinates, joined where one cut goes on along the same line (BV14).
    pub cutoffs: Vec<Vec<P>>,
}

/// A `.bvc` as the app's parts and sheets.
#[derive(Clone, Debug)]
pub struct Nesting {
    pub name: String,
    pub material: String,
    pub thickness: f64,
    pub gas: String,
    pub inch: bool,
    pub parts: Vec<Part>,
    pub sheets: Vec<Sheet>,
    pub notes: Vec<String>,
}

/// The cut-off pieces of a placed subprogram, joined: two straight cuts on
/// one line that meet (the cut to the edge and the re-pierce on, BV14) are one.
fn join_cutoffs(paths: Vec<Vec<P>>) -> Vec<Vec<P>> {
    let straight = |p: &Vec<P>| {
        let (a, b) = (p[0], *p.last().unwrap());
        let l = (b[0] - a[0]).hypot(b[1] - a[1]);
        l > 1e-9 && p.iter().all(|q| ((q[0] - a[0]) * (b[1] - a[1]) - (q[1] - a[1]) * (b[0] - a[0])).abs() / l < CLOSE_TOL)
    };
    let mut out: Vec<Vec<P>> = vec![];
    for p in paths.into_iter().filter(|p| p.len() >= 2) {
        if !straight(&p) {
            out.push(p);
            continue;
        }
        let (a, b) = (p[0], *p.last().unwrap());
        let on_line = |l: &Vec<P>, q: P| {
            let (u, v) = (l[0], *l.last().unwrap());
            let len = (v[0] - u[0]).hypot(v[1] - u[1]);
            ((q[0] - u[0]) * (v[1] - u[1]) - (q[1] - u[1]) * (v[0] - u[0])).abs() / len < CLOSE_TOL
        };
        let touches = |l: &Vec<P>| {
            let (u, v) = (l[0], *l.last().unwrap());
            [u, v].iter().any(|e| [a, b].iter().any(|f| (e[0] - f[0]).hypot(e[1] - f[1]) < CLOSE_TOL))
        };
        match out.iter_mut().find(|l| l.len() == 2 && on_line(l, a) && on_line(l, b) && touches(l)) {
            Some(l) => {
                // The two ends farthest apart along the line.
                let pts = [l[0], l[1], a, b];
                let d = [l[1][0] - l[0][0], l[1][1] - l[0][1]];
                let t = |q: &P| (q[0] - l[0][0]) * d[0] + (q[1] - l[0][1]) * d[1];
                let lo = *pts.iter().min_by(|x, y| t(x).total_cmp(&t(y))).unwrap();
                let hi = *pts.iter().max_by(|x, y| t(x).total_cmp(&t(y))).unwrap();
                *l = vec![lo, hi];
            }
            None => out.push(vec![a, b]),
        }
    }
    out
}

/// Turn a read `.bvc` into parts and sheets. Each part subprogram becomes a
/// part (one per part id of the job file; its quantity the ordered one), each
/// placement a placed part at its position and rotation, each nest a sheet or
/// as many identical sheets as it is run, and each cut-off its cut lines.
/// Part colours start at `color0`.
pub fn nesting(b: &Bvc, color0: usize) -> Result<Nesting, String> {
    let mut parts: Vec<Part> = vec![];
    // Each part made: its key (part id, or subprogram for one the job file
    // doesn't list), the subprogram it was made from, its index.
    let mut known: Vec<(String, SubPart, usize)> = vec![];
    let mut notes = b.notes.clone();
    let mut sheets = vec![];
    let (mut open_cuts, mut unknown_modes, mut dropped_starts, mut dropped_joints) = (0usize, BTreeMap::<i64, usize>::new(), 0usize, 0usize);
    for n in &b.nests {
        let p = &n.program;
        let mat = b.job.materials.iter().find(|m| m.id == n.plan.material_id).map(|m| m.size).unwrap_or_default();
        let (w, h) = if p.sheet[0] > 0.0 && p.sheet[1] > 0.0 { (p.sheet[0], p.sheet[1]) } else { (mat[0], mat[1]) };
        let subs: Vec<Option<SubPart>> = p.subs.iter().map(sub_part).collect();
        let mut sheet = Sheet { w, h, count: n.plan.multiplicity.max(1) as usize, placed: vec![], cutoffs: vec![] };
        if p.inch != b.job.inch {
            notes.push(format!("cut program {} is in {}, the job file in {}: each read in its own unit", n.plan.name, if p.inch { "inch" } else { "mm" }, if b.job.inch { "inch" } else { "mm" }));
        }
        if n.plan.multiplicity < 1 {
            notes.push(format!("nest {} is run {} time(s): opened as one sheet", n.plan.name, n.plan.multiplicity));
        }
        for call in &p.calls {
            let sub = &p.subs[call.sub];
            let place = |q: P| {
                let r = rot(q, call.rotation);
                [r[0] + call.x, r[1] + call.y]
            };
            let Some(sp) = &subs[call.sub] else {
                // A cut-off: its cuts on the sheet.
                let paths = sub.cuts.iter().filter(|c| c.mode.map(mode_as) != Some(ModeAs::Mark)).map(|c| c.path.points().into_iter().map(place).collect()).collect();
                sheet.cutoffs.extend(join_cutoffs(paths));
                continue;
            };
            let link = n.plan.parts.iter().find(|pp| pp.sub.eq_ignore_ascii_case(&sub.name));
            let key = match link {
                Some(l) => format!("id:{}", l.part_id),
                None => format!("sub:{}:{}", n.plan.name, sub.name),
            };
            // A part cut the same way again is the same part; one whose shape
            // or cut settings differ in this nest is another version of it.
            let idx = match known.iter().find(|k| k.0 == key && same_version(&k.1, sp)) {
                Some(k) => k.2,
                None => {
                    let info = link.and_then(|l| b.job.parts.iter().find(|q| q.id == l.part_id));
                    let base = info.map(|i| i.name.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| sub.name.clone());
                    let name = if known.iter().any(|k| k.0 == key) { format!("{base} ({})", n.plan.name) } else { base.clone() };
                    if info.is_none() {
                        notes.push(format!("subprogram {} of nest {} has no part in the job file: opened as part {name}", sub.name, n.plan.name));
                    }
                    open_cuts += sp.open_cuts;
                    for m in &sp.unknown_modes {
                        *unknown_modes.entry(*m).or_default() += 1;
                    }
                    let mut tech = crate::cnc::auto_tech();
                    for &(ci, at) in &sp.starts {
                        match tech.starts.iter_mut().find(|s| s.is_none()) {
                            Some(s) => *s = Some((ci as u8, at)),
                            None => dropped_starts += 1,
                        }
                    }
                    if !sp.joints.is_empty() {
                        // The file's joints are markers; the machine sets the
                        // gap (BV16). The app's own joint width stands in.
                        tech.joint_width = crate::nest::PartTech::default().joint_width;
                    }
                    for &(ci, at) in &sp.joints {
                        match tech.joint_picks.iter_mut().find(|s| s.is_none()) {
                            Some(s) => *s = Some((ci as u8, at)),
                            None => dropped_joints += 1,
                        }
                    }
                    let g = sp.geo.clone();
                    parts.push(Part {
                        name,
                        shape: crate::nest::Shape::Custom,
                        w: g.w,
                        h: g.h,
                        qty: info.map(|i| i.quantity.max(0) as u32).unwrap_or(0),
                        rotate: true,
                        color: color0 + parts.len(),
                        tech: Some(tech),
                        custom: Some(std::sync::Arc::new(g)),
                        rots: Default::default(),
                        texts: vec![],
                        group: String::new(),
                        erp: vec![],
                    });
                    known.push((key, sp.clone(), parts.len() - 1));
                    parts.len() - 1
                }
            };
            // The part's own (0, 0) is the subprogram's `origin`; turned and
            // moved as the call says, then the app's placement made to match.
            let part = &parts[idx];
            let probe = crate::nest::oriented(part, idx, call.rotation, false);
            let anchor = part.geometry().outline.first().copied().unwrap_or([0.0, 0.0]);
            let want = place([anchor[0] + sp.origin[0], anchor[1] + sp.origin[1]]);
            let got = probe.map(anchor);
            sheet.placed.push(Placed { x: want[0] - got[0], y: want[1] - got[1], ..probe });
        }
        sheets.push(sheet);
    }
    // A part with no ordered quantity, or cut in more than one version: as
    // many of each as the nests place (JOB-74).
    let placed = |k: usize| sheets.iter().map(|s| s.count * s.placed.iter().filter(|p| p.part == k).count()).sum::<usize>() as u32;
    let mut keys: Vec<&String> = vec![];
    for k in &known {
        if !keys.contains(&&k.0) {
            keys.push(&k.0);
        }
    }
    for key in keys {
        let versions: Vec<usize> = known.iter().filter(|k| &k.0 == key).map(|k| k.2).collect();
        if versions.len() > 1 {
            let ordered = parts[versions[0]].qty;
            for &k in &versions {
                parts[k].qty = placed(k);
            }
            let names = versions.iter().map(|&k| format!("{} ×{}", parts[k].name, parts[k].qty)).collect::<Vec<_>>().join(", ");
            notes.push(format!("{} is cut {} different ways (shape or cut settings) in different nests: opened as {} parts with the quantities their nests place, {names} (ordered {ordered})", parts[versions[0]].name, versions.len(), versions.len()));
        }
    }
    for k in 0..parts.len() {
        if parts[k].qty == 0 {
            parts[k].qty = placed(k);
        }
    }
    if parts.is_empty() {
        return Err("its nests place no parts".into());
    }
    if open_cuts > 0 {
        notes.push(format!("{open_cuts} open cut line(s) on parts came in as no-cut lines (only closed contours are cut)"));
    }
    if !unknown_modes.is_empty() {
        let l = unknown_modes.iter().map(|(m, c)| if *m < 0 { format!("no mode ×{c}") } else { format!("mode {m} ×{c}") }).collect::<Vec<_>>().join(", ");
        notes.push(format!("contours with no known operation mode get the machine's automatic cut quality: {l}"));
    }
    if dropped_starts > 0 || dropped_joints > 0 {
        notes.push(format!("{dropped_starts} cut start(s) and {dropped_joints} micro-joint(s) beyond what a part keeps (8 starts, 12 joints) left to the automatic plan"));
    }
    // The job file's thickness, else the first cut program's.
    let thickness = if b.job.thickness > 0.0 { b.job.thickness } else { b.nests.first().map_or(0.0, |n| n.program.thickness) };
    Ok(Nesting { name: b.job.name.clone(), material: b.job.material.clone(), thickness, gas: b.job.gas.clone(), inch: b.job.inch, parts, sheets, notes })
}

#[cfg(test)]
mod bvc_tests {
    use super::*;

    #[test]
    fn arcs_go_the_way_their_sweep_says() {
        // A quarter turn counter-clockwise about the origin, and clockwise
        // about (1, 1), between the same two points.
        let c = arc_centre([1.0, 0.0], [0.0, 1.0], 90.0).unwrap();
        assert!(c[0].abs() < 1e-12 && c[1].abs() < 1e-12, "{c:?}");
        let c = arc_centre([1.0, 0.0], [0.0, 1.0], -90.0).unwrap();
        assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 1.0).abs() < 1e-12, "{c:?}");
        // The long way round (270°) has its centre on the other side.
        let c = arc_centre([1.0, 0.0], [0.0, 1.0], 270.0).unwrap();
        assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 1.0).abs() < 1e-12, "{c:?}");
        let pts = arc_points([10.0, 0.0], [-10.0, 0.0], 180.0);
        assert!(pts.iter().all(|q| (q[0].hypot(q[1]) - 10.0).abs() < 1e-9 && q[1] >= -1e-9), "upper half: {pts:?}");
        assert_eq!(*pts.last().unwrap(), [-10.0, 0.0]);
    }

    #[test]
    fn a_sweep_near_zero_is_a_line_and_one_near_a_full_turn_a_circle() {
        assert_eq!(arc_points([0.0, 0.0], [10.0, 0.0], 1e-12), vec![[10.0, 0.0]]);
        // A tiny sweep that still has a centre: the points stay on the chord.
        let p = arc_points([0.0, 0.0], [10.0, 0.0], 1e-6);
        assert!(p.iter().all(|q| q[1].abs() < 1e-6));
        // 359.999° around (0, 0) from (10, 0): nearly a whole circle, radius 10.
        let s: f64 = 359.999;
        let end = [10.0 * s.to_radians().cos(), 10.0 * s.to_radians().sin()];
        let p = arc_points([10.0, 0.0], end, s);
        assert!(p.iter().all(|q| (q[0].hypot(q[1]) - 10.0).abs() < 1e-6), "on the circle");
        let ys: Vec<f64> = p.iter().map(|q| q[1]).collect();
        assert!(ys.iter().cloned().fold(f64::MIN, f64::max) > 9.99 && ys.iter().cloned().fold(f64::MAX, f64::min) < -9.99, "all the way round");
        // The same the other way (-359.999°).
        let end = [10.0 * (-s).to_radians().cos(), 10.0 * (-s).to_radians().sin()];
        let p = arc_points([10.0, 0.0], end, -s);
        assert!(p.iter().all(|q| (q[0].hypot(q[1]) - 10.0).abs() < 1e-6));
    }

    #[test]
    fn xml_reads_attributes_entities_and_refuses_broken_files() {
        let e = parse_xml("\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<!-- c -->\r\n<A x=\"1\" y='a &amp; b'>\r\n  <B />\r\n  <C k=\"&#65;&#x42;\"></C>\r\n</A>\r\n").unwrap();
        assert_eq!((e.name.as_str(), e.attr("x"), e.attr("y")), ("A", Some("1"), Some("a & b")));
        assert_eq!(e.kids.len(), 2);
        assert_eq!(e.kid("C").unwrap().attr("k"), Some("AB"));
        assert!(parse_xml("<A><B></A>").unwrap_err().contains("closes <B>"));
        assert!(parse_xml("<A><B/>").unwrap_err().contains("ends inside <A>"));
        assert!(parse_xml("<A x=1/>").is_err());
        assert!(parse_xml("").is_err());
    }

    #[test]
    fn numbers_read_as_the_format_writes_them() {
        let e = parse_xml("<L X=\"1.2246467991473532E-16\" Y=\"-0\" Z=\"INF\" F=\"1,5\" />").unwrap();
        assert_eq!(number(&e, "X").unwrap(), Some(1.2246467991473532E-16));
        assert_eq!(number(&e, "Y").unwrap(), Some(0.0));
        assert_eq!(number(&e, "Z").unwrap(), Some(f64::INFINITY));
        assert!(number(&e, "F").is_err(), "a comma is not the decimal point");
        assert!(coord(&e, "Z").is_err(), "INF is no position");
        assert!(coord(&e, "Q").is_err(), "a move needs its position");
    }

    #[test]
    fn modes_become_qualities_and_engraving_mark() {
        let q = |n| ModeAs::Quality(CutQuality::new(n).unwrap());
        assert_eq!([0, 1, 3, 4, 5, 6, 7].map(mode_as), [q(1), q(2), q(3), q(4), q(5), q(6), q(7)]);
        assert_eq!(mode_as(2), ModeAs::Mark);
        assert_eq!(mode_as(8), ModeAs::Unknown);
        assert_eq!(mode_as(-1), ModeAs::Unknown);
    }

    #[test]
    fn leads_and_joints_stay_off_the_shape() {
        // A 50 x 20 outline with a lead-in from outside, a joint part-way,
        // and a lead-out; an unbracketed lead on a hole.
        let text = r#"<CuttingControlProgram MeasurementSystem="Metric" Thickness="2"><SubPrograms><SubProgram Name="S"><ControlCommands>
<SetOperationMode Value="3" /><MoveLineTo X="10" Y="10" /><SetPiercingMode Value="Normal" /><ExecPiercing /><LineTo X="12" Y="10" />
<BeginCutOut Contour="Interior" /><SetToolCorrection Value="Left" /><LineTo X="12" Y="14" /><LineTo X="16" Y="14" /><LineTo X="16" Y="10" /><LineTo X="12" Y="10" /><EndCutOut /><LineTo X="13" Y="11" />
<SetOperationMode Value="0" /><MoveLineTo X="-5" Y="0" /><BeginLeadIn /><SetPiercingMode Value="Normal" /><ExecPiercing /><LineTo X="0" Y="0" /><EndLeadIn />
<BeginCutOut Contour="Exterior" /><SetToolCorrection Value="Right" /><LineTo X="50" Y="0" /><ExecMicrojoint /><LineTo X="50" Y="20" /><LineTo X="0" Y="20" /><LineTo X="0" Y="0" /><EndCutOut />
<BeginLeadOut /><LineTo X="-3" Y="-3" /><EndLeadOut /><SetToolCorrection Value="None" />
</ControlCommands></SubProgram></SubPrograms><ControlCommands><CallSubProgram X="0" Y="0" Rotation="0" SubProgramName="s" /></ControlCommands></CuttingControlProgram>"#;
        let p = parse_program(text).unwrap();
        assert_eq!(p.calls.len(), 1, "the call finds its subprogram whatever the case");
        let cuts = &p.subs[0].cuts;
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[0].lead_in.as_ref().unwrap().segs, vec![Seg::Line([12.0, 10.0])], "unbracketed lead-in");
        assert_eq!(cuts[0].lead_out.as_ref().unwrap().segs, vec![Seg::Line([13.0, 11.0])], "unbracketed lead-out");
        assert_eq!((cuts[0].correction, cuts[1].correction), (Correction::Left, Correction::Right));
        assert_eq!(cuts[1].pierce, Some(([-5.0, 0.0], Piercing::Normal)));
        assert_eq!(cuts[1].joints, vec![[50.0, 0.0]]);
        let sp = sub_part(&p.subs[0]).unwrap();
        assert_eq!((sp.geo.w, sp.geo.h, sp.geo.holes.len()), (50.0, 20.0, 1), "no lead stretches the part");
        assert!(sp.geo.outline.iter().all(|q| q[0] >= 0.0 && q[1] >= 0.0));
        assert_eq!(sp.starts, vec![(0, [12.0, 10.0]), (1, [0.0, 0.0])]);
        assert_eq!(sp.joints, vec![(1, [50.0, 0.0])]);
    }

    #[test]
    fn a_lead_out_inside_the_cut_out_stays_off_the_shape_and_imperial_is_inch() {
        // The original's post writes the lead-out brackets before EndCutOut
        // (BV34) and labels inch programs "Imperial" (BV25).
        let text = r#"<CuttingControlProgram MeasurementSystem="Imperial" Thickness="0.125"><SubPrograms><SubProgram Name="S"><ControlCommands>
<SetOperationMode Value="0" /><MoveLineTo X="-0.2" Y="0" /><BeginLeadIn /><SetPiercingMode Value="Fine" /><ExecPiercing /><LineTo X="0" Y="0" /><EndLeadIn />
<BeginCutOut Contour="Exterior" /><SetToolCorrection Value="Right" /><LineTo X="2" Y="0" /><LineTo X="2" Y="1" /><LineTo X="0" Y="1" /><LineTo X="0" Y="0" />
<BeginLeadOut /><LineTo X="-0.1" Y="-0.1" /><EndLeadOut /><EndCutOut /><SetToolCorrection Value="None" />
<SetOperationMode Value="3" /><MoveLineTo X="0.5" Y="0.5" /><BeginCutOut Contour="Interior" /><LineTo X="0.75" Y="0.5" /><LineTo X="0.75" Y="0.75" /><LineTo X="0.5" Y="0.5" /><EndCutOut />
</ControlCommands></SubProgram></SubPrograms><ControlCommands><CallSubProgram X="1" Y="1" Rotation="0" SubProgramName="S" /></ControlCommands></CuttingControlProgram>"#;
        let p = parse_program(text).unwrap();
        assert!(p.inch && (p.thickness - 3.175).abs() < 1e-9 && (p.calls[0].x - 25.4).abs() < 1e-9);
        let cuts = &p.subs[0].cuts;
        assert_eq!(cuts.len(), 2, "the lead-out doesn't become a cut or the next lead-in");
        assert!(cuts[0].closed(), "the outline still closes");
        assert_eq!(cuts[0].lead_out.as_ref().unwrap().segs, vec![Seg::Line([-2.54, -2.54])]);
        assert!(cuts[1].lead_in.is_none());
        let sp = sub_part(&p.subs[0]).unwrap();
        assert!((sp.geo.w - 50.8).abs() < 1e-9 && (sp.geo.h - 25.4).abs() < 1e-9, "{} x {}", sp.geo.w, sp.geo.h);
    }

    #[test]
    fn a_shape_split_into_more_moves_is_the_same_shape() {
        let rect = |pts: &[P]| Path { start: pts[0], segs: pts[1..].iter().map(|&p| Seg::Line(p)).collect() };
        let four = rect(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0], [0.0, 0.0]]);
        let five = rect(&[[0.0, 0.0], [50.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0], [0.0, 0.0]]);
        assert!(same_shape(&four, &five, SAME_TOL), "an extra point along a side");
        let taller = rect(&[[0.0, 0.0], [100.0, 0.0], [100.0, 60.02], [0.0, 60.02], [0.0, 0.0]]);
        assert!(!same_shape(&four, &taller, SAME_TOL), "0.02 mm taller is another shape");
        // A half circle as one arc, and as two quarter arcs.
        let one = Path { start: [10.0, 0.0], segs: vec![Seg::Arc([-10.0, 0.0], 180.0)] };
        let two = Path { start: [10.0, 0.0], segs: vec![Seg::Arc([0.0, 10.0], 90.0), Seg::Arc([-10.0, 0.0], 90.0)] };
        assert!(same_shape(&one, &two, SAME_TOL));
        let other_way = Path { start: [10.0, 0.0], segs: vec![Seg::Arc([-10.0, 0.0], -180.0)] };
        assert!(!same_shape(&one, &other_way, SAME_TOL), "the lower half isn't the upper half");
    }

    #[test]
    fn refuses_a_call_to_a_missing_subprogram_and_a_tube() {
        let e = parse_program(r#"<CuttingControlProgram><SubPrograms /><ControlCommands><CallSubProgram X="0" Y="0" Rotation="0" SubProgramName="Gone" /></ControlCommands></CuttingControlProgram>"#).unwrap_err();
        assert!(e.contains("'Gone'"), "{e}");
        assert!(parse_program("<JobInfo />").unwrap_err().contains("isn't a cut program"));
        assert!(parse_program(r#"<CuttingControlProgram MeasurementSystem="Furlongs" />"#).unwrap_err().contains("MeasurementSystem"));
    }
}
