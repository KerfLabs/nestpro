//! ERP / MES integrations: the shop's order system talking to the nesting.
//!
//! Lantek sells this as a separate product (Lantek Integra); here it is one
//! layer every connector plugs into. The flow, whatever the system:
//!
//! 1. **Pull** — list the work waiting to be nested ([`Group`]: one material
//!    and the operation it is cut on, with its open job operations).
//! 2. **Import** — fetch the cut list for the chosen job operations
//!    ([`WorkPackage`]) and their drawings, and make a job of them. Each part
//!    remembers the orders it is made for ([`Demand`], `Part::erp`) and the job
//!    remembers where it came from ([`ErpJob`]).
//! 3. **Push** — after nesting, send the nest back ([`NestReport`]: sheets,
//!    parts per sheet, how many go to each job operation, what didn't fit).
//!
//! A new system = a new module implementing [`Connector`] plus a [`Provider`]
//! entry; the settings page, the window and the job links are shared.
//! Lengths here are in mm; a connector converts to whatever its system wants.

pub mod fulcrum;
pub mod http;

use crate::nest::{Part, Placed, SheetSpec};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, TryRecvError};

/// The systems the app can talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Fulcrum,
}

impl Provider {
    pub const ALL: [Provider; 1] = [Provider::Fulcrum];
    pub fn key(self) -> &'static str {
        match self {
            Provider::Fulcrum => "fulcrum",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key().eq_ignore_ascii_case(s.trim()))
    }
    pub fn name(self) -> &'static str {
        match self {
            Provider::Fulcrum => "Fulcrum",
        }
    }
    pub fn about(self) -> &'static str {
        match self {
            Provider::Fulcrum => "FulcrumPro ERP/MES — jobs waiting to be nested, their DXF drawings, and work orders with the nest sent back.",
        }
    }
    pub fn default_url(self) -> &'static str {
        match self {
            Provider::Fulcrum => fulcrum::DEFAULT_URL,
        }
    }
    /// Where the API token is made, for the settings page.
    pub fn token_hint(self) -> &'static str {
        match self {
            Provider::Fulcrum => "Make one in Fulcrum: System Data → Advanced → Public API Setup.",
        }
    }
    /// Environment variable that can hold the token instead of the settings
    /// (the only way to keep it between runs off Windows).
    pub fn token_env(self) -> &'static str {
        match self {
            Provider::Fulcrum => "FULCRUM_API_TOKEN",
        }
    }
}

/// Length unit a system uses for numbers it is sent, or a drawing is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Inch,
    Millimeter,
}

impl Units {
    pub fn key(self) -> &'static str {
        match self {
            Units::Inch => "inch",
            Units::Millimeter => "mm",
        }
    }
    pub fn from_key(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("mm") { Units::Millimeter } else { Units::Inch }
    }
    pub fn label(self) -> &'static str {
        match self {
            Units::Inch => "inches",
            Units::Millimeter => "millimetres",
        }
    }
    /// mm per unit.
    pub fn mm(self) -> f64 {
        match self {
            Units::Inch => 25.4,
            Units::Millimeter => 1.0,
        }
    }
}

/// One connected system's settings (settings.ini `[integration]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Integration {
    pub provider: Provider,
    pub enabled: bool,
    /// API address; empty = the provider's default.
    pub base_url: String,
    /// The API token encrypted for this Windows account (DPAPI), or empty.
    pub token_enc: String,
    /// Units for the sizes, areas and lengths sent back with a nest.
    pub units: Units,
    /// Units of a drawing that doesn't say ($INSUNITS missing or unitless).
    pub drawing_units: Units,
    /// Create work orders already approved (otherwise "needs review").
    pub approve: bool,
    /// Which nesting program the nest is reported as coming from.
    pub software: String,
}

impl Integration {
    pub fn new(provider: Provider) -> Self {
        Integration { provider, enabled: true, base_url: String::new(), token_enc: String::new(), units: Units::Inch, drawing_units: Units::Inch, approve: false, software: "unknown".into() }
    }
    pub fn url(&self) -> String {
        let u = self.base_url.trim().trim_end_matches('/');
        if u.is_empty() { self.provider.default_url().to_string() } else { u.to_string() }
    }
}

/// Where the API token comes from, first match wins: typed this session,
/// the environment variable, then the one remembered (encrypted) in settings.
pub fn token(i: &Integration, session: &str) -> Result<String, String> {
    if !session.trim().is_empty() {
        return Ok(session.trim().to_string());
    }
    if let Ok(t) = std::env::var(i.provider.token_env()) {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    if !i.token_enc.is_empty() {
        return crate::settings::unprotect(&i.token_enc);
    }
    Err(format!("No {} API token yet — add it in Settings → ERP / MES. {}", i.provider.name(), i.provider.token_hint()))
}

/// The token only travels encrypted: https, or plain http to this PC (tests,
/// a local proxy).
pub fn check_url(url: &str) -> Result<(), String> {
    let u = url.trim().to_ascii_lowercase();
    if u.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = u.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or("");
        if host == "localhost" || host == "127.0.0.1" || rest.starts_with("[::1]") {
            return Ok(());
        }
    }
    Err(format!("The API address must start with https:// (got \"{}\") — the token would otherwise be sent unencrypted.", url.trim()))
}

/// The connector for a system, signed in with `token`.
pub fn connector(i: &Integration, token: String) -> Box<dyn Connector> {
    match i.provider {
        Provider::Fulcrum => Box::new(fulcrum::Fulcrum::new(&i.url(), token, i.units, i.approve, &i.software)),
    }
}

// ---------------------------------------------------------------------------
// What is kept on parts and jobs.
// ---------------------------------------------------------------------------

/// One order line a part is made for: a job operation and how many it needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Demand {
    pub provider: Provider,
    pub job_id: String,
    /// Job number / name, for people.
    pub job_name: String,
    /// The system's id for the operation that cuts the part.
    pub op_id: String,
    #[serde(default)]
    pub item_to_make_id: String,
    #[serde(default)]
    pub item_id: String,
    pub qty: u32,
    #[serde(default)]
    pub customer: String,
    /// Production due date (ISO date), used to fill earliest-due orders first.
    #[serde(default)]
    pub due: String,
    /// The drawing the part was made from.
    #[serde(default)]
    pub drawing_id: String,
    #[serde(default)]
    pub drawing_name: String,
}

/// The work order a nest was sent to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkOrderRef {
    pub id: String,
    /// The work-order operation the nest is stored on.
    pub op_id: String,
    #[serde(default)]
    pub name: String,
}

/// A job's link to its system: the material + operation group it came from,
/// and once sent, the work order holding its nest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErpJob {
    pub provider: Provider,
    #[serde(default)]
    pub material_id: String,
    #[serde(default)]
    pub material_name: String,
    #[serde(default)]
    pub operation_id: String,
    #[serde(default)]
    pub operation_name: String,
    #[serde(default)]
    pub work_order: Option<WorkOrderRef>,
    /// When the nest was last sent (seconds since 1970).
    #[serde(default)]
    pub sent: Option<u64>,
    /// The system's item number for the sheet stock, sent with each sheet
    /// so the system knows what material is used (empty = not sent).
    #[serde(default)]
    pub sheet_item: String,
}

// ---------------------------------------------------------------------------
// What connectors return.
// ---------------------------------------------------------------------------

/// A reference to a record: id and display name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ref {
    pub id: String,
    pub name: String,
}

/// Filter for the waiting-work list.
#[derive(Clone, Debug, Default)]
pub struct GroupFilter {
    /// Only groups with job operations still waiting to be nested.
    pub need_nesting: bool,
    /// Only sheet and plate stock (what a laser cuts).
    pub flat_only: bool,
    pub search: String,
}

/// Work for one material on one operation: what waits to be nested and the
/// nests already made.
#[derive(Clone, Debug, Default)]
pub struct Group {
    pub material: Ref,
    pub operation: Ref,
    pub earliest_start: String,
    pub open: Vec<OpenOp>,
    pub nests: Vec<NestRef>,
}

/// A job operation waiting to be nested.
#[derive(Clone, Debug, Default)]
pub struct OpenOp {
    pub job: Ref,
    pub op_id: String,
    pub item_to_make_id: String,
    pub item: Ref,
    pub qty: f64,
    pub customer: String,
    pub sales_order: String,
    pub due: String,
    pub start: String,
    pub rework: bool,
    /// The operation needs more than one material (its group is a guess).
    pub multi_material: bool,
}

/// A nest already stored on a work order.
#[derive(Clone, Debug, Default)]
pub struct NestRef {
    pub work_order: Ref,
    pub op_id: String,
    pub status: String,
    pub op_status: String,
    pub sheets: u32,
    pub parts: u32,
    pub machine_s: f64,
    pub nested: bool,
}

/// The material a part is cut from, as the system describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialInfo {
    pub shape_id: String,
    pub name: String,
    pub material_name: String,
    pub grade: String,
    pub thickness_mm: Option<f64>,
    pub thickness_text: String,
    pub form: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawingRef {
    pub id: String,
    pub file_name: String,
    /// Made for this operation rather than the item in general.
    pub op_specific: bool,
}

/// One part to cut, for one job operation.
#[derive(Clone, Debug)]
pub struct CutPart {
    pub demand: Demand,
    /// How many, as the system stores it (may be fractional).
    pub qty: f64,
    pub item: Ref,
    pub item_number: String,
    pub revision: String,
    pub material: Option<MaterialInfo>,
    pub drawings: Vec<DrawingRef>,
    pub rework: bool,
    pub multi_material: bool,
    /// Already on a work-order nest.
    pub nested_on: Option<String>,
}

impl CutPart {
    /// The drawing to cut from: a DXF made for this operation, else the
    /// item's first DXF.
    pub fn dxf(&self) -> Option<&DrawingRef> {
        let is_dxf = |d: &&DrawingRef| d.file_name.to_lowercase().ends_with(".dxf");
        self.drawings.iter().filter(is_dxf).find(|d| d.op_specific).or_else(|| self.drawings.iter().find(is_dxf))
    }
    /// The part's name: item number (+ revision), else the item's name.
    pub fn part_name(&self) -> String {
        let base = if !self.item_number.trim().is_empty() { self.item_number.trim().to_string() } else if !self.item.name.trim().is_empty() { self.item.name.trim().to_string() } else { "Part".into() };
        if self.revision.trim().is_empty() { base } else { format!("{base} rev {}", self.revision.trim()) }
    }
}

/// The cut list for a set of job operations.
#[derive(Clone, Debug, Default)]
pub struct WorkPackage {
    pub parts: Vec<CutPart>,
    /// Requested operations that gave no part, with why.
    pub skipped: Vec<String>,
}

/// A nest to send: lengths in mm, areas in mm².
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestReport {
    pub machine: String,
    pub parts: Vec<NestPart>,
    pub sheets: Vec<NestSheet>,
    pub unplaced: Vec<Unplaced>,
    /// Things left out of the report, for the person.
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestPart {
    /// Unique within the report; links parts to their sheet placements.
    pub number: String,
    pub qty: u32,
    pub area_mm2: f64,
    pub contour_mm: f64,
    pub item_id: String,
    /// (job id, job operation id, how many) — together `qty`.
    pub per_op: Vec<(String, String, u32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestSheet {
    pub w_mm: f64,
    pub h_mm: f64,
    /// Identical sheets cut this many times.
    pub count: u32,
    /// Machine time for one of them.
    pub runtime_s: f64,
    pub name: String,
    /// (part number, how many on one sheet).
    pub parts: Vec<(String, u32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unplaced {
    pub file_id: String,
    pub file_name: String,
    pub qty: u32,
    pub reason: String,
}

/// What sending a nest did.
#[derive(Clone, Debug, Default)]
pub struct SendResult {
    pub work_order: Option<WorkOrderRef>,
    /// A work order was made this time.
    pub created: bool,
    /// The system's remarks on the stored nest (it is stored even with them).
    pub issues: Vec<String>,
    /// The step that failed after the work order was made, if any.
    pub error: Option<String>,
}

/// What a system can do. Every call blocks; the window runs them on a
/// background thread ([`Task`]).
pub trait Connector: Send {
    /// Check the address and token; describes the token on success.
    fn check(&self) -> Result<String, String>;
    /// Work waiting to be nested.
    fn groups(&self, f: &GroupFilter) -> Result<Vec<Group>, String>;
    /// Cut list for job operations given as (job id, job operation id, item-to-make id).
    fn work_package(&self, ops: &[(String, String, String)]) -> Result<WorkPackage, String>;
    /// A drawing's file.
    fn download(&self, drawing: &DrawingRef) -> Result<Vec<u8>, String>;
    /// Store a nest: makes the work order the first time (from every job
    /// operation in `ops`), then replaces its nest.
    fn send_nest(&self, job_name: &str, link: &ErpJob, ops: &[(String, String)], nest: &NestReport) -> Result<SendResult, String>;
}

// ---------------------------------------------------------------------------
// Background requests.
// ---------------------------------------------------------------------------

/// A request running on its own thread, so the window keeps drawing.
pub struct Task<T> {
    rx: Receiver<Result<T, String>>,
    /// What it is doing, for the window.
    pub what: String,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(what: impl Into<String>, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err("the request stopped unexpectedly".into()));
            let _ = tx.send(r);
        });
        Task { rx, what: what.into() }
    }
    /// The result once it has come.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the request stopped unexpectedly".into())),
        }
    }
}

// ---------------------------------------------------------------------------
// Import helpers.
// ---------------------------------------------------------------------------

/// One part to make from a work package: every job operation needing the
/// same item from the same drawing becomes one part with their total.
#[derive(Clone, Debug)]
pub struct ImportPart {
    pub name: String,
    pub drawing: DrawingRef,
    pub demands: Vec<Demand>,
}

impl ImportPart {
    pub fn qty(&self) -> u32 {
        self.demands.iter().map(|d| d.qty).sum()
    }
}

/// Fractional quantities round up: a part short is worse than one spare.
pub fn whole_qty(q: f64) -> u32 {
    if q.is_finite() && q > 0.0 { (q - 1e-9).ceil().max(1.0) as u32 } else { 0 }
}

/// Group a work package into parts to make, earliest due first within each
/// part. Parts without a DXF, or already nested, are returned as notes.
pub fn plan_import(wp: &WorkPackage) -> (Vec<ImportPart>, Vec<String>) {
    let mut out: Vec<ImportPart> = vec![];
    let mut notes = wp.skipped.clone();
    for cp in &wp.parts {
        let label = format!("{} ({})", cp.part_name(), cp.demand.job_name);
        if let Some(wo) = &cp.nested_on {
            notes.push(format!("{label}: already nested on work-order operation {wo}"));
            continue;
        }
        let Some(dxf) = cp.dxf() else {
            notes.push(format!("{label}: no DXF drawing attached"));
            continue;
        };
        if cp.multi_material {
            notes.push(format!("{label}: the operation needs more than one material — check it belongs on this sheet"));
        }
        if cp.rework {
            notes.push(format!("{label}: rework"));
        }
        let mut d = cp.demand.clone();
        d.qty = whole_qty(cp.qty);
        d.drawing_id = dxf.id.clone();
        d.drawing_name = dxf.file_name.clone();
        if d.qty == 0 {
            notes.push(format!("{label}: quantity 0"));
            continue;
        }
        let key_item = cp.item.id.clone();
        match out.iter_mut().find(|p| p.drawing.id == dxf.id && p.demands.first().is_some_and(|f| f.item_id == key_item)) {
            Some(p) => p.demands.push(d),
            None => out.push(ImportPart { name: cp.part_name(), drawing: dxf.clone(), demands: vec![d] }),
        }
    }
    // Same name, different drawing: keep names unique so parts stay apart.
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for p in &mut out {
        p.demands.sort_by(|a, b| due_key(&a.due).cmp(&due_key(&b.due)));
        let n = seen.entry(p.name.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            p.name = format!("{} ({})", p.name, n);
        }
    }
    (out, notes)
}

/// Undated orders sort last.
fn due_key(s: &str) -> String {
    if s.is_empty() { "~".into() } else { s.to_string() }
}

/// The app's material (index into `cnc::MATERIALS`) for a system's material:
/// grade, then material name, then shape name, through Settings → Materials
/// and its aliases, then by common words.
pub fn pick_material(cfg: &crate::settings::AppSettings, m: &MaterialInfo) -> Option<usize> {
    let names = [&m.grade, &m.material_name, &m.name];
    for n in names {
        if let Some(i) = cfg.material_for(n) {
            return Some(i);
        }
    }
    for n in names {
        for w in n.split(|c: char| !(c.is_alphanumeric() || c == '.')).filter(|w| !w.is_empty()) {
            if let Some(i) = cfg.material_for(w) {
                return Some(i);
            }
        }
    }
    let all = names.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>().join(" ");
    if all.contains("stainless") || all.contains("inox") {
        Some(1)
    } else if all.contains("alum") {
        Some(2)
    } else if all.contains("steel") || all.contains("a36") || all.contains("hrpo") || all.contains("crs") || all.contains("hrs") {
        Some(0)
    } else {
        None
    }
}

/// A DXF's own unit from its header ($INSUNITS: 1 inch, 4 mm, 5 cm, 6 m),
/// or None when it doesn't say.
pub fn dxf_units_mm(text: &str) -> Option<f64> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let i = lines.iter().position(|l| *l == "$INSUNITS")?;
    // Next group: "70" then the value.
    let v: i32 = lines.get(i + 2)?.parse().ok()?;
    match v {
        1 => Some(25.4),
        2 => Some(304.8),
        4 => Some(1.0),
        5 => Some(10.0),
        6 => Some(1000.0),
        _ => None,
    }
}

/// Scale a drawing as read (points, texts) by `f`.
pub fn scale_drawing(d: &mut crate::dxf::Drawing, f: f64) {
    if (f - 1.0).abs() < 1e-12 {
        return;
    }
    for p in &mut d.pieces {
        for q in &mut p.pts {
            q[0] *= f;
            q[1] *= f;
        }
    }
    for t in &mut d.texts {
        t.2[0] *= f;
        t.2[1] *= f;
        t.3 *= f;
    }
}

// ---------------------------------------------------------------------------
// Push: the nest as a report.
// ---------------------------------------------------------------------------

fn sheet_parts(sheet: &[Placed], parts: usize) -> Vec<u32> {
    let mut n = vec![0u32; parts];
    for p in sheet {
        if let Some(c) = n.get_mut(p.part) {
            *c += 1;
        }
    }
    n
}

/// Split `made` parts over the orders, in order, each up to its need; any
/// extra goes to the last order.
pub fn allocate(made: u32, demands: &[Demand]) -> Vec<u32> {
    let mut left = made;
    let mut out: Vec<u32> = demands
        .iter()
        .map(|d| {
            let k = left.min(d.qty);
            left -= k;
            k
        })
        .collect();
    if let Some(last) = out.last_mut() {
        *last += left;
    }
    out
}

/// The nest as the system wants it. `groups` are the identical-sheet groups
/// (first sheet index, how many) and `seconds` each sheet's machine time;
/// each sheet's size is its format's (a stock sheet or remnant), else the
/// job's sheet. Only parts made for this system's orders are reported.
#[allow(clippy::too_many_arguments)]
pub fn nest_report(provider: Provider, job_name: &str, machine: &str, parts: &[Part], sheets: &[Vec<Placed>], formats: &[crate::nest::SheetFormat], groups: &[(usize, usize)], seconds: &[f64], spec: &SheetSpec) -> NestReport {
    let mine = |p: &Part| !p.erp.is_empty() && p.erp.iter().all(|d| d.provider == provider);
    let mut r = NestReport { machine: machine.to_string(), ..Default::default() };
    // Unique part numbers.
    let mut numbers: Vec<String> = vec![];
    for p in parts {
        let mut n = p.name.trim().to_string();
        if n.is_empty() {
            n = "Part".into();
        }
        let base = n.clone();
        let mut k = 1;
        while numbers.contains(&n) {
            k += 1;
            n = format!("{base} #{k}");
        }
        numbers.push(n);
    }
    let placed: Vec<u32> = (0..parts.len()).map(|i| sheets.iter().flatten().filter(|p| p.part == i).count() as u32).collect();
    let others: Vec<&str> = parts.iter().enumerate().filter(|(i, p)| !mine(p) && placed[*i] > 0).map(|(_, p)| p.name.as_str()).collect();
    if !others.is_empty() {
        r.notes.push(format!("{} part kind(s) not from {} left out of the report: {}", others.len(), provider.name(), others.join(", ")));
    }
    for (i, p) in parts.iter().enumerate() {
        if !mine(p) {
            continue;
        }
        let need: u32 = p.erp.iter().map(|d| d.qty).sum();
        if placed[i] > 0 {
            let split = allocate(placed[i], &p.erp);
            r.parts.push(NestPart {
                number: numbers[i].clone(),
                qty: placed[i],
                area_mm2: p.net_area(),
                contour_mm: p.cut_length(),
                item_id: p.erp.first().map_or(String::new(), |d| d.item_id.clone()),
                per_op: p.erp.iter().zip(split).filter(|(_, k)| *k > 0).map(|(d, k)| (d.job_id.clone(), d.op_id.clone(), k)).collect(),
            });
        }
        if placed[i] < need {
            let d = &p.erp[0];
            r.unplaced.push(Unplaced { file_id: d.drawing_id.clone(), file_name: if d.drawing_name.is_empty() { p.name.clone() } else { d.drawing_name.clone() }, qty: need - placed[i], reason: format!("{} of {need} placed", placed[i]) });
        }
    }
    for (g, &(first, count)) in groups.iter().enumerate() {
        let Some(s) = sheets.get(first) else { continue };
        let n = sheet_parts(s, parts.len());
        let on: Vec<(String, u32)> = n.iter().enumerate().filter(|(i, k)| **k > 0 && mine(&parts[*i])).map(|(i, k)| (numbers[i].clone(), *k)).collect();
        if on.is_empty() {
            r.notes.push(format!("Sheet {} has no {} parts and was left out", first + 1, provider.name()));
            continue;
        }
        let f = crate::engine::format_at(formats, spec, first);
        r.sheets.push(NestSheet { w_mm: f.w, h_mm: f.h, count: count as u32, runtime_s: seconds.get(first).copied().unwrap_or(0.0), name: format!("{job_name}-{}", g + 1), parts: on });
    }
    r
}

/// Every (job id, job operation id) the job's parts are made for, once each,
/// in part order.
pub fn job_ops(parts: &[Part], provider: Provider) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = vec![];
    for d in parts.iter().flat_map(|p| p.erp.iter()).filter(|d| d.provider == provider) {
        let k = (d.job_id.clone(), d.op_id.clone());
        if !v.contains(&k) {
            v.push(k);
        }
    }
    v
}

/// Seconds since 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Show an ISO date-time as its date.
pub fn day(s: &str) -> &str {
    s.get(..10).unwrap_or(s)
}

/// A stand-in API server for tests.
#[cfg(test)]
pub(crate) mod mock {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A stand-in Fulcrum on localhost: answers by path, records requests.
    pub fn mock(answers: Vec<(&'static str, u16, String)>) -> (String, Arc<Mutex<Vec<(String, String, String)>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![]));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for stream in l.incoming() {
                let Ok(mut st) = stream else { break };
                let mut rd = BufReader::new(st.try_clone().unwrap());
                let mut first = String::new();
                rd.read_line(&mut first).unwrap();
                let mut len = 0usize;
                let mut auth = String::new();
                loop {
                    let mut h = String::new();
                    rd.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    let lower = h.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = h.trim().to_string();
                    }
                }
                let mut body = vec![0u8; len];
                rd.read_exact(&mut body).unwrap();
                let target = first.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
                let path = target.split('?').next().unwrap().to_string();
                seen2.lock().unwrap().push((target.clone(), auth, String::from_utf8_lossy(&body).to_string()));
                let (code, text) = answers.iter().find(|a| path.ends_with(a.0)).map(|a| (a.1, a.2.clone())).unwrap_or((404, String::new()));
                let _ = write!(st, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (addr, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::Shape;

    fn demand(job: &str, op: &str, qty: u32, due: &str) -> Demand {
        Demand { provider: Provider::Fulcrum, job_id: job.into(), job_name: format!("Job {job}"), op_id: op.into(), item_to_make_id: String::new(), item_id: "item1".into(), qty, customer: String::new(), due: due.into(), drawing_id: "att1".into(), drawing_name: "bracket.dxf".into() }
    }

    fn part(name: &str, erp: Vec<Demand>) -> Part {
        Part { name: name.into(), shape: Shape::Rectangle, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp }
    }

    fn at(part: usize, x: f64) -> Placed {
        Placed { part, x, y: 5.0, w: 100.0, h: 50.0, turns: 0, mirror: false }
    }

    #[test]
    fn made_parts_fill_the_earliest_orders_first_and_extras_go_last() {
        let d = [demand("j1", "o1", 3, ""), demand("j2", "o2", 2, "")];
        assert_eq!(allocate(5, &d), vec![3, 2]);
        assert_eq!(allocate(4, &d), vec![3, 1]);
        assert_eq!(allocate(7, &d), vec![3, 4], "extras go to the last order");
        assert_eq!(allocate(0, &d), vec![0, 0]);
    }

    #[test]
    fn fractional_quantities_round_up() {
        assert_eq!((whole_qty(2.0), whole_qty(2.2), whole_qty(0.3), whole_qty(0.0), whole_qty(f64::NAN)), (2, 3, 1, 0, 0));
    }

    #[test]
    fn the_report_counts_sheets_parts_and_orders() {
        let parts = vec![part("BRK-1", vec![demand("j1", "o1", 3, "2026-11-01"), demand("j2", "o2", 3, "2026-11-09")]), part("Shop scrap", vec![]), part("PLT-2", vec![demand("j3", "o3", 4, "")])];
        // Two identical sheets (2× BRK-1 + 1× shop part each) and one with 1× PLT-2.
        let sheets = vec![vec![at(0, 5.0), at(0, 110.0), at(1, 220.0)], vec![at(0, 5.0), at(0, 110.0), at(1, 220.0)], vec![at(2, 5.0)]];
        let groups = [(0, 2), (2, 1)];
        let spec = SheetSpec { w: 3048.0, h: 1524.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
        let r = nest_report(Provider::Fulcrum, "W45", "Laser", &parts, &sheets, &[], &groups, &[120.0, 120.0, 60.0], &spec);
        assert_eq!(r.parts.len(), 2);
        assert_eq!((r.parts[0].number.as_str(), r.parts[0].qty), ("BRK-1", 4));
        assert_eq!(r.parts[0].per_op, vec![("j1".into(), "o1".into(), 3), ("j2".into(), "o2".into(), 1)]);
        assert!((r.parts[0].area_mm2 - 5000.0).abs() < 1e-6 && (r.parts[0].contour_mm - 300.0).abs() < 1e-6);
        assert_eq!(r.sheets.len(), 2);
        assert_eq!((r.sheets[0].count, r.sheets[0].runtime_s, r.sheets[0].name.as_str()), (2, 120.0, "W45-1"));
        assert_eq!(r.sheets[0].parts, vec![("BRK-1".into(), 2)], "the shop's own part isn't reported");
        // BRK-1 needs 6, 4 placed; PLT-2 needs 4, 1 placed.
        assert_eq!(r.unplaced.iter().map(|u| (u.file_name.as_str(), u.qty)).collect::<Vec<_>>(), vec![("bracket.dxf", 2), ("bracket.dxf", 3)]);
        assert!(r.notes.iter().any(|n| n.contains("Shop scrap")));
        assert_eq!(job_ops(&parts, Provider::Fulcrum), vec![("j1".into(), "o1".into()), ("j2".into(), "o2".into()), ("j3".into(), "o3".into())]);
    }

    #[test]
    fn duplicate_part_names_get_numbered() {
        let parts = vec![part("A", vec![demand("j", "o1", 1, "")]), part("A", vec![demand("j", "o2", 1, "")])];
        let sheets = vec![vec![at(0, 5.0), at(1, 110.0)]];
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default() };
        let r = nest_report(Provider::Fulcrum, "J", "", &parts, &sheets, &[], &[(0, 1)], &[10.0], &spec);
        assert_eq!(r.parts.iter().map(|p| p.number.as_str()).collect::<Vec<_>>(), vec!["A", "A #2"]);
        assert_eq!(r.sheets[0].parts, vec![("A".into(), 1), ("A #2".into(), 1)]);
    }

    fn cut(job: &str, op: &str, item: &str, number: &str, qty: f64, due: &str, drawings: Vec<DrawingRef>) -> CutPart {
        let mut d = demand(job, op, 0, due);
        d.item_id = item.into();
        CutPart { demand: d, qty, item: Ref { id: item.into(), name: format!("{number} name") }, item_number: number.into(), revision: String::new(), material: None, drawings, rework: false, multi_material: false, nested_on: None }
    }

    fn dr(id: &str, f: &str, op: bool) -> DrawingRef {
        DrawingRef { id: id.into(), file_name: f.into(), op_specific: op }
    }

    #[test]
    fn import_merges_one_item_across_jobs_and_skips_parts_without_a_dxf() {
        let wp = WorkPackage {
            parts: vec![
                cut("j2", "o2", "i1", "BRK-1", 3.0, "2026-11-09", vec![dr("a1", "BRK-1.dxf", false)]),
                cut("j1", "o1", "i1", "BRK-1", 2.5, "2026-11-01", vec![dr("a1", "BRK-1.dxf", false)]),
                cut("j1", "o3", "i2", "PLT-2", 1.0, "", vec![dr("p", "PLT-2.pdf", false), dr("a3", "PLT-2-laser.DXF", true), dr("a2", "PLT-2.dxf", false)]),
                cut("j1", "o4", "i3", "NOPE", 1.0, "", vec![dr("p", "x.pdf", false)]),
            ],
            skipped: vec!["j9/o9: notFound".into()],
        };
        let (parts, notes) = plan_import(&wp);
        assert_eq!(parts.len(), 2);
        assert_eq!((parts[0].name.as_str(), parts[0].qty()), ("BRK-1", 6), "3 + 2.5 rounded up");
        assert_eq!(parts[0].demands.iter().map(|d| d.job_id.as_str()).collect::<Vec<_>>(), vec!["j1", "j2"], "earliest due first");
        assert_eq!(parts[1].drawing.id, "a3", "the operation's own DXF wins");
        assert_eq!(notes.len(), 2);
        assert!(notes[1].contains("NOPE") && notes[1].contains("no DXF"));
    }

    #[test]
    fn materials_map_through_grades_aliases_and_words() {
        let cfg = crate::settings::AppSettings::default();
        let m = |grade: &str, name: &str| pick_material(&cfg, &MaterialInfo { grade: grade.into(), material_name: name.into(), ..Default::default() });
        assert_eq!(m("304", "Stainless Steel 304 Sheet 1/4\""), Some(1));
        assert_eq!(m("", "Aluminum 5052 Sheet"), Some(2));
        assert_eq!(m("A36", "Hot Rolled Steel Plate"), Some(0));
        assert_eq!(m("", "Copper"), None);
    }

    #[test]
    fn dxf_units_come_from_the_header() {
        assert_eq!(dxf_units_mm("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n"), Some(25.4));
        assert_eq!(dxf_units_mm("9\n$INSUNITS\n70\n     4\n"), Some(1.0));
        assert_eq!(dxf_units_mm("9\n$INSUNITS\n70\n0\n"), None, "unitless");
        assert_eq!(dxf_units_mm(crate::dxf::SAMPLE_DXF), None);
        let mut d = crate::dxf::read(crate::dxf::SAMPLE_DXF).unwrap();
        let w0 = d.pieces[0].pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        scale_drawing(&mut d, 25.4);
        let w1 = d.pieces[0].pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        assert!((w1 - w0 * 25.4).abs() < 1e-9);
    }

    #[test]
    fn the_token_only_goes_over_https() {
        assert!(check_url("https://api.fulcrumpro.com").is_ok());
        assert!(check_url("http://127.0.0.1:5000").is_ok() && check_url("http://localhost/x").is_ok());
        assert!(check_url("http://api.fulcrumpro.com").is_err());
        assert!(check_url("http://localhost.evil.com").is_err());
        assert!(check_url("ftp://x").is_err());
    }

    #[test]
    fn the_token_comes_from_the_session_first() {
        let i = Integration::new(Provider::Fulcrum);
        assert_eq!(token(&i, "  abc ").unwrap(), "abc");
        // (The environment variable and DPAPI paths need a real environment.)
    }
}
