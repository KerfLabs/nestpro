//! The headless engine: one job and the rules that change it, without a window.
//!
//! The GUI (`App`), the command line (`cli.rs`) and the MCP server (`mcp.rs`)
//! all go through the functions here, so a rule lives in one place and the
//! agent interfaces keep up with the GUI. `ops.rs` lists every operation an
//! agent can run (the registry the CLI and the MCP tools are built from);
//! `render.rs` draws a sheet or a part to PNG/SVG so an agent can look at it.

pub mod arrange;
pub mod contours;
pub mod imports;
pub mod librarycheck;
pub mod ops;
pub mod readback;
pub mod render;
pub mod session;
pub mod stockflow;

use crate::cnc::{self, ManualCut, SheetPlan};
use crate::cutoff::{self, CutKind, CutLine};
use crate::dxf;
use crate::jobfile::{self, JobFile};
use crate::nest::{self, NestResult, Part, PartTech, Placed, SheetFormat, Shape, SheetSpec, P};
use crate::settings::{self, AppSettings};
use crate::stock::Stock;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The machine context: settings, the machine's post and technology file.
// ---------------------------------------------------------------------------

/// What the active machine brings: its post-processor, BLM output, CT1 values.
pub struct Ctx {
    pub cfg: AppSettings,
    /// The library folder: the sheet stock lives there (`stock::Stock::file`),
    /// shared with the window.
    pub library_dir: PathBuf,
    pub post: crate::post::PostConfig,
    pub blm: crate::blm::BlmSettings,
    pub tech: crate::ct1::MachineTech,
    /// Who the agent works for (this PC's user, marked as an agent) and the
    /// clock it goes by: what it saves to the Library records them (LB5).
    pub who: crate::shared::Who,
    pub clock: crate::shared::Clock,
}

/// The post library of a machine (built-in values when it has none or it can't be read).
pub fn load_post(m: &settings::Machine) -> crate::post::PostConfig {
    if m.post_lib.is_empty() { Default::default() } else { crate::post::PostConfig::load(Path::new(&m.post_lib)).unwrap_or_default() }
}

/// Make `m` the machine the rules use: its BLM post values and CT1 technology
/// file (cutting zones, lead-ins, joints, grouping). Shared with the GUI's
/// Settings → Apply.
pub fn install_machine(m: &settings::Machine, lantek_dir: &str) -> crate::ct1::MachineTech {
    crate::blm::set_adi(crate::blm::AdiParams::from_files(&m.blm_ps1, &m.blm_cf1));
    let t = crate::ct1::MachineTech::for_machine(m, lantek_dir);
    crate::ct1::set_active(t.clone());
    t
}

/// Tell the rules which machine-file key each material has (Settings →
/// Materials → Machine table, MTL2). Shared with the GUI's Settings → Apply.
pub fn install_material_keys(cfg: &AppSettings) {
    let names: Vec<String> = cfg.materials.iter().map(|m| m.0.clone()).collect();
    crate::ct1::set_material_keys(&cfg.material_refs, &names);
}

/// The machine's part gap and sheet margin for a sheet's material and
/// thickness (MTL9; the margin is the widest of the machine's four), each None
/// when the machine file has none.
pub fn machine_spacing(spec: &SheetSpec) -> (Option<f64>, Option<f64>) {
    let t = crate::ct1::for_spec(spec);
    (t.part_gap, t.sheet_margin())
}

/// "Use machine values": take the machine's part gap and margin for the
/// sheet's material and thickness into the sheet. Our choice: the sheet's
/// spacing is never changed by itself when the material or thickness
/// changes; this is the one-click action (window and agents). Says what it set.
pub fn use_machine_spacing(spec: &mut SheetSpec, gap: bool, margin: bool) -> Result<String, String> {
    let (g, m) = machine_spacing(spec);
    let mut said = vec![];
    if let (true, Some(g)) = (gap, g) {
        spec.gap = g;
        said.push(format!("part gap {g} mm"));
    }
    if let (true, Some(m)) = (margin, m) {
        spec.margin = m;
        said.push(format!("margin {m} mm"));
    }
    if said.is_empty() {
        return Err(format!(
            "The machine file has no part gap or margins for {} {} mm ({}); the sheet keeps gap {} mm and margin {} mm.",
            cnc::MATERIALS.get(spec.material).copied().unwrap_or("?"),
            spec.thickness,
            crate::ct1::for_spec(spec).table_label(),
            spec.gap,
            spec.margin
        ));
    }
    Ok(format!("Machine spacing: {}.", said.join(", ")))
}

/// BLM output switches of a machine.
pub fn blm_base(m: &settings::Machine) -> crate::blm::BlmSettings {
    let mut b = crate::blm::BlmSettings::for_machine(&m.name);
    b.on = m.output == settings::Output::BlmSiemens;
    b.gas_id = m.gas_id;
    b.gas_name = m.gas_name.clone();
    b
}

/// BLM settings with the job's name, material and thickness filled in
/// (material name and code from Settings → Materials).
pub fn blm_for(spec: &SheetSpec, base: &crate::blm::BlmSettings, job_name: &str, cfg: &AppSettings) -> crate::blm::BlmSettings {
    let b = crate::blm::BlmSettings::for_spec(spec, &base.machine, job_name);
    let (mat_name, mat_id) = cfg.materials.get(spec.material).cloned().unwrap_or((b.mat_name.clone(), b.mat_id.clone()));
    crate::blm::BlmSettings { on: base.on, gas_id: base.gas_id, gas_name: base.gas_name.clone(), mat_name, mat_id, ..b }
}

impl Ctx {
    /// The saved settings and their active machine.
    pub fn load() -> Ctx {
        Ctx::from_settings(AppSettings::load())
    }

    pub fn from_settings(cfg: AppSettings) -> Ctx {
        let m = cfg.machine();
        let tech = install_machine(&m, &cfg.lantek_dir);
        install_material_keys(&cfg);
        let library_dir = if cfg.library_dir.is_empty() { crate::library::default_dir() } else { PathBuf::from(&cfg.library_dir) };
        Ctx { post: load_post(&m), blm: blm_base(&m), tech, library_dir, cfg, who: crate::shared::Who::this_pc().as_agent(), clock: crate::shared::Clock::pc() }
    }

    /// Built-in values only (tests).
    #[cfg(test)]
    pub fn builtin() -> Ctx {
        // Built-in machine values only, never the tables of a Lantek install on
        // this PC (LANTEK_DIR or C:\Lantek\Expert): they would be set for every
        // test running at the same time.
        let cfg = AppSettings { lantek_dir: std::env::temp_dir().join("nestpro_test_no_lantek").to_string_lossy().into_owned(), ..AppSettings::default() };
        let mut c = Ctx::from_settings(cfg);
        // Never the shop's real stock: an empty library of the test's own.
        c.library_dir = std::env::temp_dir().join(format!("nestpro_test_nolib_{}_{:?}", std::process::id(), std::thread::current().id()));
        c
    }

    /// The Library folder as this agent works on it (its user and clock),
    /// without setting anything up: stock changes are signed with it (SV9).
    pub fn library_at(&self) -> crate::library::Library {
        crate::library::Library::at(&self.library_dir, &self.who, &self.clock)
    }

    /// The sheet stock (empty when there is none yet).
    pub fn stock(&self) -> Result<Stock, String> {
        Stock::load(&self.library_dir)
    }

    /// Nesting from stock for a job (what it holds counts as free again; the
    /// remnants it cut itself are never nested on).
    pub fn stock_use<'a>(&'a self, job: &'a JobFile) -> stockflow::StockUse<'a> {
        stockflow::StockUse { dir: self.library_dir.clone(), st: &self.cfg.stock, held: &job.held, exclude: job.formats.iter().flat_map(|f| f.made.iter().cloned()).collect() }
    }

    pub fn export_dir(&self) -> PathBuf {
        if self.cfg.export_dir.is_empty() { crate::library::documents_dir() } else { PathBuf::from(&self.cfg.export_dir) }
    }
}

// ---------------------------------------------------------------------------
// Parts.
// ---------------------------------------------------------------------------

/// Size a new built-in shape starts with.
pub fn default_size(shape: Shape) -> (f64, f64) {
    match shape {
        Shape::Rectangle | Shape::PlateHole => (200.0, 150.0),
        Shape::Strip => (600.0, 60.0),
        Shape::LBracket => (200.0, 160.0),
        Shape::Disc => (180.0, 180.0),
        Shape::Flange => (220.0, 220.0),
        Shape::Triangle => (160.0, 160.0),
        Shape::SlottedPlate => (400.0, 100.0),
        Shape::Custom => (100.0, 100.0),
    }
}

/// A built-in shape part with the GUI's defaults (5 off, may turn 90°).
pub fn shape_part(shape: Shape, n: usize, color: usize) -> Part {
    let (w, h) = default_size(shape);
    Part { name: format!("{} {}", shape.label(), n), shape, w, h, qty: 5, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
}

/// Next free colour for a new part.
pub fn next_color(parts: &[Part]) -> usize {
    parts.iter().map(|p| p.color + 1).max().unwrap_or(0)
}

/// The app material a material text names: its name, code or an alias; then a
/// grade that starts with an alias holding a digit ("S355J2" → the material
/// with alias S355); then a look-alike name (as the XML import does).
pub fn material_like(cfg: &AppSettings, text: &str) -> Option<usize> {
    let t = text.trim().to_lowercase();
    cfg.material_for(text)
        .or_else(|| {
            (0..cnc::MATERIALS.len()).find(|&i| {
                cfg.material_aliases.get(i).is_some_and(|a| {
                    a.split(',').map(|x| x.trim().to_lowercase()).any(|x| x.len() >= 3 && x.chars().any(|c| c.is_ascii_digit()) && t.starts_with(&x))
                })
            })
        })
        .or_else(|| crate::xmlimport::material_index(text, &cnc::MATERIALS, &cfg.materials))
}

/// What a drawing's import left out or changed, for the user (every format's
/// skipped objects, open pieces, closed gaps, outlines outside the part).
pub fn import_notes(name: &str, rep: &dxf::ImportReport) -> Vec<String> {
    let mut notes = vec![];
    if !rep.skipped.is_empty() {
        notes.push(format!("{name}: ignored {}", rep.skipped.join(", ")));
    }
    if rep.open_pieces > 0 {
        notes.push(format!("{name}: {} open piece(s) off the part left out", rep.open_pieces));
    }
    if rep.open_kept > 0 {
        notes.push(format!("{name}: {} open line(s) kept on the part as Bend line or No cut, never cut; set them in Cut technology", rep.open_kept));
    }
    if rep.gaps_closed > 0 {
        notes.push(format!("{name}: {} gap(s) up to 0.5 mm closed", rep.gaps_closed));
    }
    if rep.outside_loops > 0 {
        notes.push(format!("{name}: {} outline(s) outside the part left out", rep.outside_loops));
    }
    if rep.duplicates > 0 {
        notes.push(format!("{name}: {} object(s) drawn twice in the same place, cut once", rep.duplicates));
    }
    if !rep.hidden_left_out.is_empty() {
        let rows: Vec<String> = rep.hidden_left_out.iter().map(|(l, n)| format!("{n} on '{l}'")).collect();
        notes.push(format!("{name}: left out what the drawing hides or draws as centre / dashed lines ({}); set the layer to Cut to cut it", rows.join(", ")));
    }
    notes
}

/// "material …, thickness … mm" where part data (from a file name or a file)
/// doesn't match the job; `None` when it does or says nothing.
fn spec_mismatch(cfg: &AppSettings, spec: &SheetSpec, material: Option<&str>, thickness: Option<f64>) -> Option<String> {
    let mut info = vec![];
    if let Some(m) = material.filter(|m| !m.trim().is_empty()) {
        if material_like(cfg, m) != Some(spec.material) {
            info.push(format!("material {}", m.trim()));
        }
    }
    if let Some(t) = thickness.filter(|t| *t > 0.0 && (t - spec.thickness).abs() > 1e-6) {
        info.push(format!("thickness {t} mm"));
    }
    (!info.is_empty()).then(|| format!("{} (the job is {} {} mm)", info.join(", "), cnc::MATERIALS.get(spec.material).copied().unwrap_or(""), spec.thickness))
}

/// Build a part from a drawing with the given layer roles and the part data its
/// file carries (DSTV header, MEC record…). `file` is the file's name (without
/// extension): Settings → name format reads part data from it. `name` is the
/// part's own name — the same as `file` for a one-part file, else the numbered
/// or file-given name of one part of it, which is then the reference as it is.
/// The quantity is the file's, then the file name's, then 1; the reference gets
/// the prefix/suffix (at most 40 characters). Returns the part and notes about
/// what was left out or doesn't match the job.
#[allow(clippy::too_many_arguments)]
pub fn part_from_read(cfg: &AppSettings, spec: &SheetSpec, file: &str, name: &str, d: &dxf::Drawing, meta: &crate::formats::Meta, roles: &BTreeMap<String, dxf::LayerRole>, color: usize) -> Result<(Part, Vec<String>), String> {
    let (g, rep) = dxf::build(d, roles).map_err(|e| format!("{name}: {e}"))?;
    let mut notes = import_notes(name, &rep);
    // Part data from the file name (v44 "Text definition in the file name") —
    // the real file name, never a part's numbered name ("DEMO-2" isn't quantity 2).
    let fd = crate::filename::parse_format(&cfg.name_format).ok().and_then(|f| crate::filename::read(&f, file));
    let one_part = name == file;
    let (reference, qty) = match fd {
        Some(fd) => {
            if let Some(m) = spec_mismatch(cfg, spec, fd.material.as_deref(), fd.thickness) {
                notes.push(format!("{name}: the name says {m}"));
            }
            let reference = if one_part { fd.reference.unwrap_or_else(|| file.to_string()) } else { name.to_string() };
            (reference, meta.qty.or(fd.quantity).unwrap_or(1).max(1))
        }
        None => (name.to_string(), meta.qty.unwrap_or(1).max(1)),
    };
    if let Some(m) = spec_mismatch(cfg, spec, meta.material.as_deref(), meta.thickness) {
        notes.push(format!("{name}: the file says {m}"));
    }
    // Prefix + name + suffix, at most 40 characters (FN5).
    let reference = crate::filename::part_reference(&cfg.ref_prefix, &reference, &cfg.ref_suffix);
    let part = Part {
        name: reference,
        shape: Shape::Custom,
        w: g.w,
        h: g.h,
        qty,
        rotate: true,
        color,
        tech: None,
        texts: g.texts.clone(),
        group: String::new(),
        erp: vec![],
        custom: Some(std::sync::Arc::new(g)),
        rots: Default::default(),
    };
    Ok((part, notes))
}

/// Take part `idx` out of the job; placed copies go with it and later parts
/// keep their placements.
pub fn remove_part(parts: &mut Vec<Part>, sheets: &mut [Vec<Placed>], idx: usize) {
    parts.remove(idx);
    for s in sheets.iter_mut() {
        s.retain(|p| p.part != idx);
        for p in s.iter_mut() {
            if p.part > idx {
                p.part -= 1;
            }
        }
    }
}

/// Insert a part at `idx`; placements of later parts move up one.
pub fn insert_part(parts: &mut Vec<Part>, sheets: &mut [Vec<Placed>], idx: usize, part: Part) {
    parts.insert(idx, part);
    for s in sheets.iter_mut() {
        for p in s.iter_mut() {
            if p.part >= idx {
                p.part += 1;
            }
        }
    }
}

/// Name for a copy of a part with technology: "Name (1)", "(2)"…
/// (T2: the change goes on a copy so the original keeps its technology).
pub fn copy_name(parts: &[Part], base: &str) -> String {
    (1..10).map(|k| format!("{base} ({k})")).find(|n| !parts.iter().any(|p| &p.name == n)).unwrap_or_else(|| format!("{base} copy"))
}

/// How often a part is placed on the sheets.
pub fn placed_count(sheets: &[Vec<Placed>], part: usize) -> usize {
    sheets.iter().flatten().filter(|p| p.part == part).count()
}

// ---------------------------------------------------------------------------
// Nesting and placing.
// ---------------------------------------------------------------------------

/// Nest every part (Do All): with several part groups (and grouping on), each
/// group in its own zone (CT1 A785/A787, zone gap A788); otherwise from the
/// sheet stock when it has sheets for this job (`stock`, see stockflow), else
/// on the job's sheet size — then every part settles against its neighbours'
/// real contours (not on remnants: compaction assumes a rectangular sheet).
pub fn nest_all(parts: &[Part], spec: &SheetSpec, group_nest: bool, stock: Option<&stockflow::StockUse>) -> NestResult {
    let mt = crate::ct1::active();
    if group_nest && nest::part_groups(parts).len() > 1 {
        let mut r = nest::nest_grouped(parts, spec, if mt.group_zones { mt.zone_gap } else { 0.0 });
        r.normalise(spec);
        return r;
    }
    let mut r = stock.and_then(|u| stockflow::nest_from_stock(parts, spec, u)).unwrap_or_else(|| nest::nest(parts, spec));
    r.normalise(spec);
    for (s, f) in r.sheets.iter_mut().zip(&r.formats) {
        if f.outline.is_none() {
            nest::compact_contours(parts, s, &spec.for_format(f));
        }
    }
    r
}

/// Compact on a remnant: it slides parts toward the lower-left corner of the
/// sheet's box, where a shaped remnant may have no material (CP1).
pub const COMPACT_REMNANT: &str = "Compact works on whole rectangular sheets only — on a remnant it could slide parts off its outline. Move them by hand, or Nest sheet.";

/// Re-nest one sheet (RN1): only the parts already on it, onto the same sheet
/// (same format, remnant outline included), by the same automatic level as
/// Nest all, then compacted against the real contours on a rectangular sheet.
/// With grouping on and several groups on the sheet, each group gets its own
/// zone as in Nest all (RN3; no compaction, so the zones stay apart). The
/// other sheets are not touched. `Err` (and nothing changes, RN2) when the
/// nester can't fit them all back on that one sheet.
pub fn renest_sheet(parts: &[Part], spec: &SheetSpec, sheet: &[Placed], f: &SheetFormat, group_nest: bool) -> Result<Vec<Placed>, String> {
    if sheet.is_empty() {
        return Err("That sheet has no parts to nest.".into());
    }
    // The job's parts with each quantity = how many of it are on this sheet,
    // so placements keep the job's part numbers.
    let mut only: Vec<Part> = parts.to_vec();
    for (i, p) in only.iter_mut().enumerate() {
        p.qty = sheet.iter().filter(|c| c.part == i).count() as u32;
    }
    let on_sheet: Vec<Part> = only.iter().filter(|p| p.qty > 0).cloned().collect();
    let zoned = group_nest && nest::part_groups(&on_sheet).len() > 1;
    let mut out = if zoned {
        if f.outline.is_some() {
            return Err("This remnant holds parts of several groups — groups are nested in zones on whole sheets only. Turn Group by attribute off, or move them by hand.".into());
        }
        let mt = crate::ct1::active();
        let r = nest::nest_grouped(&only, &spec.for_format(f), if mt.group_zones { mt.zone_gap } else { 0.0 });
        if r.sheets.len() > 1 {
            return Err("With each group in its own zone the sheet's parts don't all fit back on it — the sheet was left as it was.".into());
        }
        r.sheets.into_iter().next().unwrap_or_default()
    } else {
        nest::nest_plan(&only, spec, &[nest::FormatSlot { format: f.clone(), qty: Some(1) }]).sheets.into_iter().next().unwrap_or_default()
    };
    if out.len() < sheet.len() {
        return Err(format!("Only {} of the sheet's {} parts fit when nested again — the sheet was left as it was.", out.len(), sheet.len()));
    }
    if !zoned && f.outline.is_none() {
        nest::compact_contours(parts, &mut out, &spec.for_format(f));
    }
    Ok(out)
}

/// Is a part inside a sheet of this format: within its margin and, on a
/// remnant, inside its real shape?
pub fn fits_format(spec: &SheetSpec, f: &SheetFormat, c: &Placed) -> bool {
    let spec = spec.for_format(f);
    if !spec.fits(c) {
        return false;
    }
    match f.outline.as_ref().filter(|o| o.outer.len() >= 3) {
        Some(o) => crate::region::box_inside(o, spec.margin, c.x, c.y, c.w, c.h),
        None => true,
    }
}

/// Is a part inside a sheet of this format, judged by its real outline on a
/// remnant: true-shape nesting may tuck a part's box over a remnant's notch
/// while its outline stays inside (RE26).
pub fn part_fits_format(parts: &[Part], spec: &SheetSpec, f: &SheetFormat, c: &Placed) -> bool {
    if fits_format(spec, f, c) {
        return true;
    }
    let spec = spec.for_format(f);
    match f.outline.as_ref().filter(|o| o.outer.len() >= 3) {
        Some(o) if spec.fits(c) => crate::region::poly_inside(o, spec.margin, &nest::placed_shape(parts, c).0),
        _ => false,
    }
}

/// Inside the sheet of format `f` (its margin, a remnant's real shape) and
/// (with collision on) clear of every other part on the sheet by the part
/// gap; `skip` is the part being moved.
#[allow(clippy::too_many_arguments)]
pub fn placement_ok(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, f: &SheetFormat, skip: Option<usize>, cand: &Placed, collision: bool) -> bool {
    if !part_fits_format(parts, spec, f, cand) {
        return false;
    }
    if !collision {
        return true;
    }
    let gap = spec.gap - 1e-6;
    // Round parts are tested with their real circle (C4).
    sheet.iter().enumerate().all(|(j, o)| Some(j) == skip || !nest::clash_exact(parts, cand, o, gap))
}

/// The placed part turned to its next allowed quarter turn, or with `back` to
/// the previous one (QR3: the quick ring's "Turn back").
pub fn step_turn(part: &Part, old: Placed, back: bool) -> Result<Placed, &'static str> {
    if !part.rotate {
        return Err("This part is not allowed to rotate.");
    }
    let step = |k: u8| if back { (old.turns + 4 - k) % 4 } else { (old.turns + k) % 4 };
    let Some(k) = (1..4u8).find(|&k| nest::turn_allowed(part, step(k), old.mirror)) else {
        return Err("No other rotation is allowed for this part.");
    };
    let mut c = old;
    c.turns = step(k);
    if k % 2 == 1 {
        std::mem::swap(&mut c.w, &mut c.h);
    }
    Ok(c)
}

/// Footprint of a part turned `turns` × 90°.
pub fn footprint(part: &Part, turns: u8) -> (f64, f64) {
    let (w, h) = part.bbox();
    if turns % 2 == 1 { (h, w) } else { (w, h) }
}

/// First free spot for a part on a sheet, lowest then leftmost (the original's
/// Semi-Automatic "nest one part only").
#[allow(clippy::too_many_arguments)]
pub fn free_spot(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, f: &SheetFormat, part: usize, turns: u8, mirror: bool, collision: bool) -> Option<Placed> {
    let (fw, fh) = footprint(&parts[part], turns);
    let (gap, m) = (spec.gap, spec.margin);
    let mut xs = vec![m];
    let mut ys = vec![m];
    for o in sheet {
        xs.push(o.x + o.w + gap);
        ys.push(o.y + o.h + gap);
    }
    let mut best: Option<Placed> = None;
    for &y in &ys {
        for &x in &xs {
            let c = Placed { part, x, y, w: fw, h: fh, turns, mirror };
            if placement_ok(parts, sheet, spec, f, None, &c, collision) && best.is_none_or(|b| (c.y, c.x) < (b.y, b.x)) {
                best = Some(c);
            }
        }
    }
    best
}


/// (usage of the whole sheet %, usage up to the remnant cut %, X of that cut)
/// of a sheet of format `f` in a job with sheet settings `spec`. A remnant
/// counts by its real area, not its box (SS7, SS15).
pub fn sheet_usage(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, f: &SheetFormat) -> (f64, f64, f64) {
    let spec = spec.for_format(f);
    let area: f64 = sheet.iter().map(|p| parts[p.part].net_area()).sum();
    let used_x = sheet.iter().map(|p| p.x + p.w).fold(0.0, f64::max);
    let cut_x = (used_x + spec.gap).min(spec.w);
    let sheet_area = f.area_mm2().max(1.0);
    ((area / sheet_area * 100.0).min(100.0), (area / (cut_x * spec.h).max(1.0) * 100.0).min(100.0), cut_x)
}

// ---------------------------------------------------------------------------
// Technology.
// ---------------------------------------------------------------------------

/// One change to a part's technology — the Technology tab's buttons.
#[derive(Clone, Debug)]
pub enum TechEdit {
    /// Automatic leads by contour class (leads.rs), with this lead type
    /// (`Machine` = the machine's tables) and keypad position.
    AutoLeads { lead_type: nest::LeadType, position: u8 },
    /// Lead-in, lead-out, overlap and position from the machine's CT1
    /// (E044, E045, B156, E000).
    MachineLeads(crate::ct1::MachineTech),
    DeleteLeads,
    /// Automatic micro-joints (joints.rs): one per `every` mm of each contour,
    /// holes too — by default the machine's maximum distance for the part's
    /// quality (`dist`, CT1 F150) — within the machine's size-class limits
    /// (`ct1::MachineTech::joint_sizes`, applied when the cut plan is made).
    /// A part without a joint gap takes the machine's (`gap`, F005/B164);
    /// with `gap_wins` (the values come from a material × thickness section)
    /// every part takes it — our choice: the plate's table decides the gap.
    AutoJoints { dist: [[f64; 4]; 12], gap: f64, every: Option<f64>, gap_wins: bool },
    DeleteJoints,
    Loops(bool),
    /// The part's own technology with the defaults.
    Own,
    /// Back to the sheet's automatic technology.
    Delete,
}

impl TechEdit {
    /// Automatic micro-joints from the machine's values for the job (`ct1::for_spec`):
    /// the joint distance by quality and the joint gap (MTL7).
    pub fn auto_joints(mt: &crate::ct1::MachineTech, every: Option<f64>) -> TechEdit {
        TechEdit::AutoJoints { dist: mt.joint_dist, gap: mt.joint_gap, every, gap_wins: mt.section.is_some() }
    }

    pub fn apply(&self, t: &mut Option<PartTech>, _bbox: (f64, f64)) {
        match self {
            TechEdit::AutoLeads { lead_type, position } => {
                let mut x = t.unwrap_or_default();
                x.corner_start = true;
                x.lead_type = *lead_type;
                x.position = *position;
                if x.lead_in <= 0.0 {
                    x.lead_in = PartTech::default().lead_in;
                }
                *t = Some(x);
            }
            TechEdit::MachineLeads(mt) => {
                let mut x = t.unwrap_or_default();
                x.lead_in = mt.lead_in;
                x.lead_out = mt.lead_out;
                x.overlap = mt.overlap;
                x.position = mt.zone;
                *t = Some(x);
            }
            TechEdit::DeleteLeads => {
                if let Some(x) = t.as_mut() {
                    x.lead_in = 0.0;
                    x.lead_out = 0.0;
                    x.corner_start = false;
                    // Machine-table leads ignore the lengths: back to a plain type.
                    x.lead_type = nest::LeadType::Line;
                    x.starts = [None; 8];
                    x.lead_edits = [None; nest::LEAD_EDITS];
                } else {
                    *t = Some(PartTech { lead_in: 0.0, lead_out: 0.0, ..Default::default() });
                }
            }
            TechEdit::AutoJoints { dist, gap, every, gap_wins } => {
                let mut x = t.unwrap_or_default();
                if let Some(e) = every {
                    x.joint_every = e.max(0.0);
                } else if x.joint_every <= 0.0 {
                    // The part's own quality, else the outline's usual one (CQ1).
                    let q = x.quality.unwrap_or(crate::nest::CutQuality::FALLBACK).number();
                    x.joint_every = dist[q - 1][0];
                }
                x.micro_joints = 0;
                if x.joint_width <= 0.0 || (*gap_wins && *gap > 0.0) {
                    x.joint_width = *gap;
                }
                *t = Some(x);
            }
            TechEdit::DeleteJoints => {
                if let Some(x) = t.as_mut() {
                    x.micro_joints = 0;
                    // Automatic joints are spaced by distance: that goes too.
                    x.joint_every = 0.0;
                    x.joint_picks = [None; 12];
                }
            }
            TechEdit::Loops(on) => {
                if *on {
                    let mut x = t.unwrap_or_default();
                    x.loops = true;
                    *t = Some(x);
                } else if let Some(x) = t.as_mut() {
                    x.loops = false;
                }
            }
            TechEdit::Own => {
                if t.is_none() {
                    *t = Some(PartTech::default());
                }
            }
            TechEdit::Delete => *t = None,
        }
    }
}

/// Add a text along the bottom of a part, centred and shrunk to fit.
pub fn add_text(part: &mut Part, txt: &str, height: f64, kind: nest::TextKind) {
    let (w, h) = part.bbox();
    let mut ht = height.min(h * 0.25);
    let wid = crate::font::width(txt, ht);
    if wid > w * 0.8 {
        ht *= w * 0.8 / wid;
    }
    let wid = crate::font::width(txt, ht);
    let at = [(w - wid) / 2.0, h * 0.08];
    part.texts.push(nest::PartText { text: txt.to_string(), at, height: ht, angle: 0.0, curve: 0.0, kind });
}

/// Lead-in on element `elem` of the part (numbered as `nest::elements_of`):
/// the contour starts by the third of a line that was picked, otherwise at the
/// element's middle (LP1–LP3), if the element is long enough.
pub fn lead_on_element(part: &mut Part, elem: usize, pick: Option<P>) -> Result<String, String> {
    let elems = nest::elements_of(&part.geometry());
    let e = elems.get(elem).ok_or_else(|| format!("The part has {} element(s); there is no element {}.", elems.len(), elem + 1))?;
    let (ci, kind, len) = (e.contour, e.kind, e.length);
    let min = part.tech.map_or(5.0, |t| t.lead_in).max(1.0);
    if len < min {
        return Err(format!("That {} is {len:.1} mm — too short for a {min:.1} mm lead.", kind.label().to_lowercase()));
    }
    let overlap = part.tech.map_or(0.0, |t| t.overlap);
    let Some(at) = e.lead_point(pick, overlap, nest::LEAD_LINEAR_TOL) else {
        return Err(format!("That {} is too short for the lead and its {overlap} mm overlap.", kind.label().to_lowercase()));
    };
    let where_ = match (kind, pick) {
        (nest::ElemKind::Line, Some(p)) => {
            let (a, b) = (e.pts[0], e.pts[e.pts.len() - 1]);
            let r = (((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1])) / (e.length * e.length)).clamp(0.0, 1.0);
            if r <= nest::LEAD_PICK_LOW { "near the start of" } else if r < nest::LEAD_PICK_HIGH { "in the middle of" } else { "near the end of" }
        }
        _ => "in the middle of",
    };
    let mut t = part.tech.unwrap_or_default();
    for s in t.starts.iter_mut() {
        if s.is_some_and(|x| x.0 as usize == ci) {
            *s = None;
        }
    }
    if let Some(slot) = t.starts.iter_mut().find(|s| s.is_none()) {
        *slot = Some((ci as u8, at));
    }
    // A new start replaces a lead moved by hand on that contour (LE6).
    t.clear_lead_edit(ci);
    part.tech = Some(t);
    Ok(format!("Lead-in placed {where_} the {} ({len:.1} mm).", kind.label().to_lowercase()))
}

// ---------------------------------------------------------------------------
// Leads moved by hand (LE1–LE7)
// ---------------------------------------------------------------------------

/// Which end of a contour's lead: the lead-in's pierce or the lead-out's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeadEnd {
    In,
    Out,
}

impl LeadEnd {
    pub fn label(self) -> &'static str {
        match self {
            LeadEnd::In => "lead-in",
            LeadEnd::Out => "lead-out",
        }
    }
}

/// A contour's leads as they are cut (LE1): each end as (distance, angle)
/// from where it meets the contour, the angles it may be given, and whether
/// it was placed by hand. `None` = the contour has no such lead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeadEnds {
    pub lead_in: Option<(f64, f64)>,
    pub lead_out: Option<(f64, f64)>,
    pub in_angles: (f64, f64),
    pub out_angles: (f64, f64),
    pub hand: bool,
}

/// A planned contour's lead ends (LE1), measured on the drawn contour as the
/// plan built them, so kerf compensation (KC8) doesn't change the numbers. A
/// punched contour, or one that starts at its first micro-joint's lead-in
/// (MJ5), has none of its own.
pub fn lead_ends(c: &cnc::Contour) -> LeadEnds {
    let own = c.punch.is_none() && !c.closing;
    let [si, so] = c.lead_spots;
    LeadEnds { lead_in: si.end.filter(|_| own), lead_out: so.end.filter(|_| own), in_angles: crate::leads::hand_angles(si.open), out_angles: crate::leads::hand_angles(so.open), hand: c.lead_hand }
}

/// The number part-info, the window and messages give contour `ci` (CS1).
pub fn contour_number(part: &Part, ci: usize) -> usize {
    dxf::cut_order(&contours::list(part)).get(ci).copied().unwrap_or(ci) + 1
}

/// Drag the free end of contour `ci`'s lead-in (its pierce) or lead-out to
/// `pointer`, part mm (LE2): the GUI's drag, frame by frame. The message
/// gives lengths in `unit`, the window's (NST-16).
pub fn drag_lead_end(parts: &mut [Part], idx: usize, spec: &SheetSpec, ci: usize, end: LeadEnd, pointer: P, unit: crate::units::LenUnit) -> Result<String, String> {
    place_lead_end(parts, idx, spec, ci, end, unit, |frame, open, _| Ok(crate::leads::hand_polar(frame, end == LeadEnd::In, open, pointer)))
}

/// Give contour `ci`'s lead-in pierce or lead-out end a length (mm) and/or
/// an angle (degrees from the contour, into the scrap) by hand (LE1, LE2);
/// what isn't given stays as the lead is cut now.
pub fn set_lead_end(parts: &mut [Part], idx: usize, spec: &SheetSpec, ci: usize, end: LeadEnd, len: Option<f64>, angle: Option<f64>) -> Result<String, String> {
    let no = contour_number(&parts[idx], ci);
    place_lead_end(parts, idx, spec, ci, end, crate::units::LenUnit::Mm, |_, open, now| {
        let (lo, hi) = crate::leads::hand_angles(open);
        let what = end.label();
        let len = match (len, now) {
            (Some(l), _) => l,
            (None, Some((l, _))) => l,
            (None, None) => return Err(format!("Contour {no} has no {what} yet: give its length too.")),
        };
        if !len.is_finite() || len < crate::leads::HAND_MIN_LEN {
            return Err(format!("A {what} moved by hand is at least {} mm long; {len} mm is too short. To cut without it, delete the leads (leads mode=delete).", crate::leads::HAND_MIN_LEN));
        }
        let a = match (angle, now) {
            (Some(a), _) => {
                if !(lo - 1e-9..=hi + 1e-9).contains(&a) {
                    return Err(format!("The {what} on contour {no} can lean {lo:.0}° to {hi:.0}° from the contour (the scrap side; 90° = square off it). {a}° would put it into the part."));
                }
                a
            }
            (None, Some((_, a))) => a,
            (None, None) => 90.0,
        };
        Ok(crate::leads::hand_clamp(len, a, open))
    })
}

fn place_lead_end(parts: &mut [Part], idx: usize, spec: &SheetSpec, ci: usize, end: LeadEnd, unit: crate::units::LenUnit, pick: impl FnOnce((P, P, P), f64, Option<(f64, f64)>) -> Result<(f64, f64), String>) -> Result<String, String> {
    let part = parts.get(idx).ok_or("No such part.")?;
    let no = contour_number(part, ci);
    let plan = cnc::plan_part(parts, idx, spec);
    let Some(c) = plan.contours.iter().find(|c| c.contour == ci) else {
        return Err(format!("Contour {no} isn't cut (see part-info for the cut contours), so it has no lead."));
    };
    if c.punch.is_some() {
        return Err(format!("Contour {no} is punched on this machine: it has no leads."));
    }
    if c.closing {
        return Err(format!("Contour {no} starts at its first micro-joint's lead-in: drag that joint's lead instead, or delete the joints first."));
    }
    // Measured on the drawn contour, not the offset path (KC8).
    let spot = c.lead_spots[usize::from(end == LeadEnd::Out)];
    let (len, ang) = pick(spot.frame, spot.open, spot.end)?;
    // The lead's foot: the drawn contour's start.
    let g = part.geometry();
    let own = if ci < g.holes.len() { g.holes[ci].clone() } else { g.outline.clone() };
    let mut t = part.tech.unwrap_or_else(cnc::auto_tech);
    let mut e = t.lead_edit(ci).unwrap_or(nest::LeadEdit { contour: ci as u8, at: cnc::start_at_point(own, c.lead_spots[0].frame.0)[0], rec: c.lead_rec as u8, lead_in: None, lead_out: None });
    match end {
        LeadEnd::In => e.lead_in = Some((len, ang)),
        LeadEnd::Out => e.lead_out = Some((len, ang)),
    }
    let slot = t.lead_edits.iter().position(|s| s.is_some_and(|x| x.contour as usize == ci)).or_else(|| t.lead_edits.iter().position(|s| s.is_none()));
    let Some(k) = slot else {
        return Err(format!("This part already has {} leads moved by hand — put one back first (double-click its end, or lead-edit mode=reset).", nest::LEAD_EDITS));
    };
    t.lead_edits[k] = Some(e);
    // The lead now starts where it was moved; a start picked on an element gives way (LE6).
    for s in t.starts.iter_mut() {
        if s.is_some_and(|x| x.0 as usize == ci) {
            *s = None;
        }
    }
    parts[idx].tech = Some(t);
    // Say what is cut now: a lead that would cross the part is shortened (LE5).
    let plan = cnc::plan_part(parts, idx, spec);
    let cut = plan.contours.iter().find(|c| c.contour == ci).map(lead_ends);
    let got = cut.and_then(|x| if end == LeadEnd::In { x.lead_in } else { x.lead_out });
    let short = got.filter(|g| g.0 < len - 0.05).map_or(String::new(), |g| format!(" — cut {}: the rest would cross the part", unit.len(g.0, 1)));
    Ok(format!("Contour {no}'s {} moved by hand: {} at {ang:.0}° from the contour{short}.", end.label(), unit.len(len, 1)))
}

/// Put contour `ci`'s lead (or, with `None`, every lead of the part) back to
/// what the plan makes of it (LE6). Returns how many were moved by hand.
pub fn reset_lead_edits(part: &mut Part, ci: Option<usize>) -> usize {
    let Some(t) = part.tech.as_mut() else { return 0 };
    match ci {
        Some(ci) => usize::from(t.clear_lead_edit(ci)),
        None => {
            let n = t.lead_edits.iter().flatten().count();
            t.lead_edits = [None; nest::LEAD_EDITS];
            n
        }
    }
}

/// A micro-joint in the middle of each element listed; returns how many fitted.
pub fn joints_on_elements(part: &mut Part, picks: &[(usize, P)]) -> usize {
    let mut t = part.tech.unwrap_or_default();
    let mut placed = 0;
    for &(ci, mid) in picks {
        if let Some(slot) = t.joint_picks.iter_mut().find(|s| s.is_none()) {
            *slot = Some((ci as u8, mid));
            placed += 1;
        }
    }
    part.tech = Some(t);
    placed
}

// ---------------------------------------------------------------------------
// Machining: cut order, cut lines, plans, verification, programs.
// ---------------------------------------------------------------------------

/// Sheet `k`'s format; the job's sheet size when it has none recorded.
pub fn format_at(formats: &[SheetFormat], spec: &SheetSpec, k: usize) -> SheetFormat {
    formats.get(k).cloned().unwrap_or_else(|| SheetFormat::rect(spec.w, spec.h))
}

/// Cutting plan of every sheet (manual picks first, the rest automatically),
/// each with its own sheet size.
pub fn plans(parts: &[Part], sheets: &[Vec<Placed>], formats: &[SheetFormat], spec: &SheetSpec, manual: &[Vec<ManualCut>]) -> Vec<SheetPlan> {
    sheets.iter().enumerate().map(|(k, s)| cnc::plan_manual(parts, s, &spec.for_format(&format_at(formats, spec, k)), manual.get(k).map_or(&[][..], |v| v.as_slice()))).collect()
}

/// The ISO program of one sheet (sheet numbers from 1; `spec` with that
/// sheet's size), cut lines last; `bridge` is the remnant line's edge offset.
#[allow(clippy::too_many_arguments)]
pub fn program(sheet_no: usize, parts: &[Part], sheet: &[Placed], spec: &SheetSpec, plan: &SheetPlan, cuts: &[CutLine], marks: &[Vec<crate::nest::P>], post: &crate::post::PostConfig, bridge: f64) -> String {
    cnc::program_lines(sheet_no, parts, sheet, spec, plan, &program_blocks(cuts, marks, spec, bridge), post)
}

/// A sheet's extra program blocks: free cut paths, then the remnant marks
/// (RF9), then the remnant line last so the remnant only comes loose at the end.
pub fn program_blocks(cuts: &[CutLine], marks: &[Vec<crate::nest::P>], spec: &SheetSpec, bridge: f64) -> Vec<(String, Vec<crate::nest::P>)> {
    let mut v = cutoff::gcode_lines(cuts, spec, bridge);
    let at = v.iter().position(|(l, _)| l == "REMNANT LINE").unwrap_or(v.len());
    v.splice(at..at, marks.iter().map(|s| ("REMNANT MARK".to_string(), s.clone())));
    v
}

/// Add a cut line to a sheet's list; a new remnant line replaces the old one
/// (one remnant per sheet). Refused when it would cut through a part.
pub fn add_cut_line(parts: &[Part], sheet: &[Placed], list: &mut Vec<CutLine>, line: CutLine) -> Result<(), String> {
    if let Some(i) = cutoff::path_hits_part(parts, sheet, &line.pts) {
        return Err(format!("That line would cut through part #{} — draw it through the gaps.", i + 1));
    }
    if line.kind == CutKind::Remnant {
        list.retain(|l| l.kind != CutKind::Remnant);
    }
    list.push(line);
    Ok(())
}

/// A drawn path: a line from one sheet edge to the opposite one separates a
/// remnant, anything else is a free cut line.
pub fn classify_path(pts: Vec<P>, spec: &SheetSpec) -> CutLine {
    let kind = if cutoff::remnant_region(&pts, spec).is_some() { CutKind::Remnant } else { CutKind::Free };
    CutLine { pts, kind }
}

/// The remnants a sheet's remnant lines cut off (sheet coordinates, largest
/// first): the sheet less the cut strip and the parts (remnant.rs).
/// `kerf` is the cut's width (`kerf::kerf_mm`: the machine's, else 0.3 mm).
pub fn remnant_pieces(parts: &[Part], sheet: &[Placed], f: &SheetFormat, cuts: &[CutLine], kerf: f64) -> Vec<crate::stock::Outline> {
    if !cuts.iter().any(|l| l.kind == CutKind::Remnant) {
        return vec![];
    }
    crate::remnant::sheet_remnants_cached(parts, sheet, f, cuts, kerf)
}

/// (width, height, area) of the largest remnant the sheet's remnant lines cut off.
pub fn remnant_info(parts: &[Part], sheet: &[Placed], f: &SheetFormat, cuts: &[CutLine], kerf: f64) -> Option<(f64, f64, f64)> {
    let r = remnant_pieces(parts, sheet, f, cuts, kerf).into_iter().next()?;
    let (w, h) = r.size();
    Some((w, h, r.area_mm2()))
}

/// Pick the contour nearest `p` (sheet mm) as the next manual cut. Returns the
/// status line; a contour is cut once (MM3).
pub fn manual_pick(parts: &[Part], sheet: &[Placed], list: &mut Vec<ManualCut>, p: P, tol: f64, reverse: bool) -> Result<String, String> {
    let Some((i, ci, q)) = cnc::nearest_contour(parts, sheet, p, tol) else {
        return Err("No contour there — pick on a part's edge or a hole.".into());
    };
    let name = parts[sheet[i].part].name.clone();
    let holes = parts[sheet[i].part].geometry().holes.len();
    let what = if ci == holes { "outside".to_string() } else { format!("hole {}", ci + 1) };
    if list.iter().any(|m| m.placed == i && m.contour == ci) {
        return Err(format!("The {what} of '{name}' (#{}) is already in the manual cut — undo the pick to change it.", i + 1));
    }
    list.push(ManualCut { placed: i, contour: ci, start: q, reverse });
    let note = if ci == holes && holes > 0 { " (its holes first)" } else { "" };
    Ok(format!("Manual cut {}: {what} of '{name}' (#{}){}{note}.", list.len(), i + 1, if reverse { ", reversed" } else { "" }))
}

/// Verify Machining on one sheet: pierces in the scrap, holes first, leads,
/// parts inside the sheet, and (when asked) the safe distance between parts.
pub fn verify_sheet(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, plan: &SheetPlan, safe: Option<f64>) -> Vec<crate::v44::Issue> {
    use crate::v44::*;
    let mut issues: Vec<Issue> = crate::drawing::verify_machining(plan)
        .into_iter()
        .map(|t| Issue { code: if t.contains("drop first") { ERR_PARTS_FALL } else if t.contains("kerf") { ERR_KERF } else { OWN_CHECK }, text: t, at: None })
        .collect();
    issues.extend(no_lead_issues(plan));
    issues.extend(out_of_zone_issues(parts, sheet, spec.w, spec.h));
    issues.extend(stop_issues(parts, sheet, spec, plan));
    if let Some(need) = safe {
        issues.extend(safe_distance_issues(parts, sheet, need));
    }
    issues
}

/// What stops an export of the sheet (JOB-41, MAC-46; VP1, VP2): a part whose
/// drawing would be cut wrong (an outline that crosses itself, a hole outside
/// the outline or over another hole), and a lead-in that pierces off the sheet.
pub fn stop_issues(parts: &[Part], sheet: &[Placed], spec: &SheetSpec, plan: &SheetPlan) -> Vec<crate::v44::Issue> {
    use crate::v44::*;
    let mut out = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for pl in sheet {
        if !seen.insert(pl.part) {
            continue;
        }
        let Some(p) = parts.get(pl.part) else { continue };
        let probs = crate::drawing::cut_problems(p);
        if !probs.is_empty() {
            out.push(Issue { code: OWN_PART, text: format!("part '{}': {} (Verify Part; fix the drawing and import it again)", p.name, probs.join("; ")), at: None });
        }
    }
    for (k, i, q) in crate::drawing::pierces_off_sheet(plan, spec.w, spec.h) {
        let name = sheet.get(i).and_then(|pl| parts.get(pl.part)).map_or("?", |p| p.name.as_str());
        out.push(Issue {
            code: ERR_OUT_OF_ZONE,
            text: format!("contour {k} of '{name}' pierces off the sheet at {:.2}, {:.2} (its lead-in is longer than the room to the edge; raise the sheet margin above the lead length, shorten the lead or move the part in)", q[0], q[1]),
            at: Some((q, q)),
        });
    }
    out
}

/// The refusal for an export (or program) that `stop_issues` stops.
pub fn stop_refusal(sheet_no: usize, issues: &[crate::v44::Issue]) -> String {
    let list: Vec<&str> = issues.iter().take(5).map(|i| i.text.as_str()).collect();
    let more = if issues.len() > 5 { format!(" (+{} more)", issues.len() - 5) } else { String::new() };
    format!("Export stopped: sheet {sheet_no} would be cut wrong — {}{more}.", list.join("; "))
}

/// Distance under which two parts count as touching (mm).
pub const TOUCH: f64 = 0.01;

/// Pairs of parts on a sheet that touch or overlap by their real contours
/// (QR6). Only possible after moving parts with collision checking off; the
/// CNC program would cut them through each other. A job nested with no part
/// gap is meant to have touching parts, so nothing is reported for it.
pub fn overlapping(parts: &[Part], sheet: &[Placed], spec: &SheetSpec) -> Vec<(usize, usize)> {
    let need = spec.gap.min(TOUCH);
    if need <= 0.0 {
        return vec![];
    }
    let mut out = vec![];
    for i in 0..sheet.len() {
        for j in i + 1..sheet.len() {
            if nest::clash_exact(parts, &sheet[i], &sheet[j], need) {
                out.push((i, j));
            }
        }
    }
    out
}

/// "sheet 2: #3 Bracket and #5 Plate overlap" when parts on sheet `k` (from 0)
/// touch or overlap (QR6).
pub fn sheet_overlap_note(parts: &[Part], k: usize, sheet: &[Placed], spec: &SheetSpec) -> Option<String> {
    let pairs = overlapping(parts, sheet, spec);
    let &(i, j) = pairs.first()?;
    let more = if pairs.len() > 1 { format!(" (+{} more pair(s))", pairs.len() - 1) } else { String::new() };
    Some(format!("sheet {}: #{} {} and #{} {} overlap{more}", k + 1, i + 1, parts[sheet[i].part].name, j + 1, parts[sheet[j].part].name))
}

/// One warning for every sheet with overlapping parts, or None when all are
/// clear (QR6).
pub fn overlap_warning(parts: &[Part], sheets: &[Vec<Placed>], spec_at: impl Fn(usize) -> SheetSpec) -> Option<String> {
    let lines: Vec<String> = sheets.iter().enumerate().filter_map(|(k, s)| sheet_overlap_note(parts, k, s, &spec_at(k))).collect();
    (!lines.is_empty()).then(|| format!("parts overlap and would be cut through each other — {}", lines.join("; ")))
}

/// What an export wrote.
pub struct Exported {
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub warning: Option<String>,
}

/// File-name-safe job name.
pub fn safe_name(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// Programs an earlier export of this job wrote that this one didn't (`kept`:
/// a sheet that's gone, or the other program format) are removed, so nobody
/// runs a sheet that no longer exists (EXP1).
fn remove_stale_programs(dir: &Path, job_name: &str, post: &crate::post::PostConfig, blm: Option<&crate::blm::BlmSettings>, kept: &[PathBuf]) -> std::io::Result<()> {
    let blm_job = blm.map_or(job_name, |b| b.job.as_str());
    // One locked file doesn't keep the others: every one is tried.
    let mut first_error = None;
    for k in 1..=999 {
        let nest = crate::blm::nest_name(blm_job, k);
        for f in [dir.join(post.file_name(job_name, k)), dir.join(format!("{nest}.zip")), dir.join(format!("{nest}.XML"))] {
            if !kept.contains(&f) && f.is_file() {
                if let Err(e) = std::fs::remove_file(&f) {
                    first_error.get_or_insert(e);
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// BL39: an export that stops takes the job's earlier programs with it, so they
/// can't be run by mistake; says which way that went.
fn remove_programs_note(dir: &Path, job_name: &str, post: &crate::post::PostConfig, blm: Option<&crate::blm::BlmSettings>) -> String {
    match remove_stale_programs(dir, job_name, post, blm, &[]) {
        Ok(()) => " No program of this job is left in that folder.".into(),
        Err(e) => format!(" Some of this job's earlier programs in {} couldn't be removed ({e}): delete them before anyone runs them.", dir.display()),
    }
}

/// Write every sheet's machine program (G-code, or BLM LC5 sets) and the CSV
/// parts report into `base\<job>`.
#[allow(clippy::too_many_arguments)]
pub fn export(base: &Path, job_name: &str, parts: &[Part], sheets: &[Vec<Placed>], formats: &[SheetFormat], plans: &[SheetPlan], job_spec: &SheetSpec, cuts: &[Vec<CutLine>], marks: &[Vec<Vec<crate::nest::P>>], post: &crate::post::PostConfig, blm: Option<&crate::blm::BlmSettings>, bridge: f64) -> Result<Exported, String> {
    let dir = base.join(safe_name(job_name));
    if sheets.is_empty() {
        // The job's earlier programs would still be there to run.
        let _ = remove_stale_programs(&dir, job_name, post, blm, &[]);
        return Err("Nothing to export — nest first.".into());
    }
    // JOB-41, MAC-46: a part drawn so it would be cut wrong, or a lead-in that
    // pierces off the sheet, stops the export before anything is written.
    for (k, (s, pl)) in sheets.iter().zip(plans).enumerate() {
        let spec = &job_spec.for_format(&format_at(formats, job_spec, k));
        let issues = stop_issues(parts, s, spec, pl);
        if !issues.is_empty() {
            return Err(format!("{}{}", stop_refusal(k + 1, &issues), remove_programs_note(&dir, job_name, post, blm)));
        }
    }
    // BL39: every LC5 set is read back before anything is written; a set that
    // wouldn't cut the plan stops the whole export, and the job's earlier
    // programs go too, so they can't be run by mistake.
    let mut sets = vec![];
    if let Some(b) = blm.filter(|b| b.on) {
        for (k, (s, pl)) in sheets.iter().zip(plans).enumerate() {
            let spec = &job_spec.for_format(&format_at(formats, job_spec, k));
            let c = cuts.get(k).map_or(&[][..], |v| v.as_slice());
            let set = crate::blm::sheet_set(k + 1, parts, s, spec, pl, c, b);
            let issues = readback::check_set(&set, parts, s, pl, c);
            if !issues.is_empty() {
                return Err(format!("{}{}", readback::refusal(k + 1, &issues), remove_programs_note(&dir, job_name, post, blm)));
            }
            sets.push(set);
        }
    }
    let mut files = vec![];
    let mut warning = None;
    let res = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        for (k, (s, pl)) in sheets.iter().zip(plans).enumerate() {
            let spec = &job_spec.for_format(&format_at(formats, job_spec, k));
            let c = cuts.get(k).map_or(&[][..], |v| v.as_slice());
            if let Some(set) = sets.get(k) {
                // The LC5 reads BLM's SPF set (zipped) and its XML job sheet.
                crate::blm::write_sheet(&dir, set)?;
                files.push(dir.join(format!("{}.zip", set.nest)));
                files.push(dir.join(format!("{}.XML", set.nest)));
                if set.warning.is_some() {
                    warning = set.warning.clone();
                }
                continue;
            }
            let f = dir.join(post.file_name(job_name, k + 1));
            std::fs::write(&f, program(k + 1, parts, s, spec, pl, c, marks.get(k).map_or(&[][..], |v| v.as_slice()), post, bridge))?;
            files.push(f);
        }
        remove_stale_programs(&dir, job_name, post, blm, &files)?;
        let mut csv = String::from("reference,shape,width_mm,height_mm,quantity,nested\n");
        for (i, p) in parts.iter().enumerate() {
            csv.push_str(&format!("{},{},{:.1},{:.1},{},{}\n", p.name.replace(',', " "), p.shape.label(), p.w, p.h, p.qty, placed_count(sheets, i)));
        }
        let f = dir.join("parts.csv");
        std::fs::write(&f, csv)?;
        files.push(f);
        Ok(())
    })();
    // A write that fails partway must not leave a mix of new and old files.
    res.map_err(|e| format!("Export failed: {e}.{}", remove_programs_note(&dir, job_name, post, blm)))?;
    // QR6: warn (the programs are still written) when parts overlap.
    if let Some(o) = overlap_warning(parts, sheets, |k| job_spec.for_format(&format_at(formats, job_spec, k))) {
        warning = Some(match warning {
            Some(w) => format!("{o}; and {w}"),
            None => o,
        });
    }
    Ok(Exported { dir, files, warning })
}

/// Drop the empty sheets (and their formats, cut lines and hand cuts), except
/// `keep`. Formats are filled in first (the job's sheet size) so they stay in
/// step with the sheets.
pub fn drop_empty_sheets(sheets: &mut Vec<Vec<Placed>>, formats: &mut Vec<SheetFormat>, spec: &SheetSpec, cuts: &mut Vec<Vec<CutLine>>, manual: &mut Vec<Vec<ManualCut>>, keep: Option<usize>) -> bool {
    let mask: Vec<bool> = sheets.iter().enumerate().map(|(k, s)| !s.is_empty() || Some(k) == keep).collect();
    if mask.iter().all(|m| *m) {
        return false;
    }
    fn filt<T>(v: &mut Vec<T>, mask: &[bool]) {
        let mut k = 0;
        v.retain(|_| {
            let ok = mask.get(k).copied().unwrap_or(true);
            k += 1;
            ok
        });
    }
    formats.truncate(sheets.len());
    while formats.len() < sheets.len() {
        formats.push(SheetFormat::rect(spec.w, spec.h));
    }
    filt(formats, &mask);
    filt(sheets, &mask);
    filt(cuts, &mask);
    filt(manual, &mask);
    true
}

// ---------------------------------------------------------------------------
// The job file as the engine's job.
// ---------------------------------------------------------------------------

impl JobFile {
    pub fn new_job(name: &str, spec: SheetSpec) -> JobFile {
        JobFile { format: jobfile::FORMAT.into(), version: jobfile::VERSION, name: name.into(), week: String::new(), parts: vec![], spec, sheets: vec![], formats: vec![], held: Default::default(), cut_lines: vec![], method: String::new(), manual_cuts: vec![], stale: false, booked: false, erp: None, saved: None }
    }

    pub fn plans(&self) -> Vec<SheetPlan> {
        plans(&self.parts, &self.sheets, &self.formats, &self.spec, &self.manual_cuts)
    }

    /// Sheet `k`'s format (size, remnant outline, stock reference).
    pub fn format(&self, k: usize) -> SheetFormat {
        format_at(&self.formats, &self.spec, k)
    }

    /// The job's settings with sheet `k`'s own size.
    pub fn spec_at(&self, k: usize) -> SheetSpec {
        self.spec.for_format(&self.format(k))
    }

    /// Is a part inside sheet `k` (its margin, a remnant's real shape)?
    pub fn fits(&self, k: usize, c: &Placed) -> bool {
        part_fits_format(&self.parts, &self.spec, &self.format(k), c)
    }

    /// One format per sheet (older files have none: the job's sheet size).
    pub fn normalise_formats(&mut self) {
        self.formats.truncate(self.sheets.len());
        while self.formats.len() < self.sheets.len() {
            self.formats.push(SheetFormat::rect(self.spec.w, self.spec.h));
        }
    }

    /// Add an empty sheet of this format; returns its index.
    pub fn push_sheet(&mut self, f: SheetFormat) -> usize {
        self.normalise_formats();
        self.sheets.push(vec![]);
        self.formats.push(f);
        self.sheets.len() - 1
    }

    /// Take sheet `k` out (its parts are no longer nested).
    pub fn remove_sheet(&mut self, k: usize) {
        self.normalise_formats();
        if k < self.sheets.len() {
            self.sheets.remove(k);
            self.formats.remove(k);
        }
        if k < self.cut_lines.len() {
            self.cut_lines.remove(k);
        }
        if k < self.manual_cuts.len() {
            self.manual_cuts.remove(k);
        }
    }

    /// The job's sheets as the stock's remnant generation sees them.
    pub fn stock_sheets(&self, cfg: &AppSettings) -> stockflow::JobSheets<'_> {
        stockflow::JobSheets { name: &self.name, parts: &self.parts, spec: &self.spec, sheets: &self.sheets, cut_lines: &self.cut_lines, material_name: stockflow::material_name(cfg, self.spec.material) }
    }

    /// Bring the stock in step with the job (what the window does on save):
    /// store new remnants (remaking those of sheets whose remnant lines
    /// changed, ST12), take/give back sheets. `force_lines`: store the remnant
    /// lines' remnants even when the save option is off.
    pub fn book_stock(&mut self, ctx: &Ctx, force_lines: bool) -> String {
        self.book_stock_named(ctx, force_lines, None)
    }

    /// `book_stock` with the new remnants' references given for (sheet, piece)
    /// (the window's Prompt for references, UI14).
    pub fn book_stock_named(&mut self, ctx: &Ctx, force_lines: bool, names: Option<&BTreeMap<(usize, usize), String>>) -> String {
        self.normalise_formats();
        let (mut formats, mut held) = (std::mem::take(&mut self.formats), std::mem::take(&mut self.held));
        let note = stockflow::on_save(&ctx.library_at(), &self.stock_sheets(&ctx.cfg), &mut formats, &mut held, &ctx.cfg.stock, force_lines, names);
        (self.formats, self.held) = (formats, held);
        self.booked = true;
        note
    }

    pub fn cuts(&self, k: usize) -> &[CutLine] {
        self.cut_lines.get(k).map_or(&[][..], |v| v.as_slice())
    }

    pub fn cuts_mut(&mut self, k: usize) -> &mut Vec<CutLine> {
        if self.cut_lines.len() <= k {
            self.cut_lines.resize(k + 1, vec![]);
        }
        &mut self.cut_lines[k]
    }

    pub fn manual_mut(&mut self, k: usize) -> &mut Vec<ManualCut> {
        if self.manual_cuts.len() <= k {
            self.manual_cuts.resize(k + 1, vec![]);
        }
        &mut self.manual_cuts[k]
    }

    /// Remove the empty sheets (with their cut lines and hand cuts).
    pub fn tidy(&mut self) {
        drop_empty_sheets(&mut self.sheets, &mut self.formats, &self.spec, &mut self.cut_lines, &mut self.manual_cuts, None);
    }

    /// A placement changed: the sheet's hand-picked cut order no longer fits it.
    pub fn sheet_changed(&mut self, k: usize) {
        if let Some(m) = self.manual_cuts.get_mut(k) {
            m.clear();
        }
    }

    pub fn clear_nestings(&mut self) {
        self.sheets.clear();
        self.formats.clear();
        self.cut_lines.clear();
        self.manual_cuts.clear();
        self.method.clear();
    }
}

/// A new job's sheet with the part gap and edge margin from Settings → Nesting (HM12).
pub fn with_prefs(spec: &SheetSpec, cfg: &AppSettings) -> SheetSpec {
    SheetSpec { gap: cfg.nesting.part_gap, margin: cfg.nesting.edge_margin, ..spec.clone() }
}

/// The default sheet of a new job: 2000 × 1000 mm, 3 mm mild steel, level 3.
pub fn default_spec() -> SheetSpec {
    SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() }
}

#[cfg(test)]
mod tests;
