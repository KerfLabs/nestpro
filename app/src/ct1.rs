//! The machine's technology file (Lantek `MchFiles\<post>.CT1`): plain text
//! lines `Xnnn value ; comment`. The BLM laser's file is PstAdi06.CT1; its
//! lead-in, overlap and micro-joint values are the defaults here (see
//! MECHANICS.md "Machine technology file"). The automatic lead tables by
//! contour class and quality are in `leads::LeadTable` (B200–B295, E000–E909).
//!
//! After the machine's own values the file may hold any number of material ×
//! thickness sections (Z/Y/X/V lines, each closed by `Z999`). The first section
//! that fits the job's material and thickness replaces the machine's lead and
//! micro-joint tables, joint width, part gap, kerf, margins and so on (MECHANICS
//! "Material and thickness sections of the machine file", MTL1–MTL12).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Values the part technology uses, by their CT1 codes.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineTech {
    /// Quality 1's values (corrected in v44 build analysis: quality 1 is the B
    /// block; E000… is quality 2). B200: zone (1–9 keypad) where the lead goes.
    pub zone: u8,
    /// B201: largest corner angle for a lead in a corner, degrees.
    pub corner_max: f64,
    /// B204: shortest element that takes a lead, mm.
    pub min_element: f64,
    /// B244 / B245: lead-in and lead-out distance on other-shaped contours (contour 1), mm.
    pub lead_in: f64,
    pub lead_out: f64,
    /// B246: lead-in angle on contours, degrees.
    pub lead_angle: f64,
    /// B214 / B215: lead-in and lead-out distance in round holes, mm.
    pub hole_lead_in: f64,
    pub hole_lead_out: f64,
    /// Automatic lead tables, every class and quality (B150–B295, E000–E909).
    pub leads: crate::leads::LeadTable,
    /// B156: overlap of the cut past its start before the lead-out, mm.
    pub overlap: f64,
    /// B157: loop radius, mm.
    pub loop_radius: f64,
    /// F000: joint type of quality 1's default record (3 = leads, perpendicular).
    pub joint_type: i32,
    /// F001 / F003: joint lead-in distance and angle (joints by quality 1).
    pub joint_lead: f64,
    pub joint_angle: f64,
    /// F002: joint lead-out distance (0 = no lead-out).
    pub joint_lead_out: f64,
    /// F005: joint uncut length (the gap), mm; 99999.999 there = B164.
    pub joint_gap: f64,
    /// F150: maximum distance between joints, mm (quality 1; corrected in v44
    /// build analysis: F120 is not read).
    pub joint_every: f64,
    /// Per quality (F150 + 4q … F153 + 4q): max distance, min distance to a
    /// corner, min distance between joints on one element, step angle on circles.
    pub joint_dist: [[f64; 4]; 12],
    /// F121 / F122: fewest and most joints per contour (fallbacks of the size classes).
    pub joint_min: u32,
    pub joint_max: u32,
    /// F136–F149 (+ F121/F122): fewest/most joints by contour size.
    pub joint_sizes: crate::nest::AutoJoints,
    /// F133–F135: joint overlap test (> 0 on), shortening per step, tries.
    pub joint_test: i32,
    pub joint_shorten: f64,
    pub joint_tries: i32,
    /// B110–B123: the sheet's automatic order, zone by zone (CZ1–CZ6), with
    /// any change made in Settings → Machines (CO16).
    pub sheet_order: SheetOrder,
    /// B565–B568, B587–B590: how a part's holes are ordered when nobody has
    /// ordered them by hand (CO20, CO14, CO15), with any change made in
    /// Settings → Machines (CO17).
    pub hole_order: HoleOrder,
    /// The order settings changed in Settings → Machines that are used
    /// (CO16, CO17, `OrderChanges::valid`): kept so a material section's
    /// values (`for_job`) take them too, and so reports say where each value
    /// comes from.
    pub order_changes: OrderChanges,
    /// B134: test parts inside holes (nest small parts in the holes of big ones).
    pub parts_in_holes: bool,
    /// B076: smallest hole area offered to other parts, mm² (0 = any).
    pub min_hole_area: f64,
    /// A785: group parts by attributes; A787: each group in its own nesting
    /// zone; A788: gap between the zones, mm.
    pub group_parts: bool,
    pub group_zones: bool,
    pub zone_gap: f64,
    /// Nesting: B057 strip width of the true-shape nester, mm (0 = not in the
    /// file; the nester then uses `nest::BAND_DEFAULT`). The shop CT1s say 13.896.
    pub band_width: f64,
    /// Nesting: B078 "fit the parts to the sheet border": the usable area grows by
    /// half the gap so a part's real edge can reach the margin (shop CT1s: ON).
    pub fit_border: bool,
    /// B002: cutter radius (half the kerf), mm, or the material section's
    /// Z004; None = not in the file (kerf::cutter_radius then falls back).
    pub cutter_radius: Option<f64>,
    /// Kerf compensation: control/system switches, correctors (B124/B125 or
    /// the section's Z024/Z025), the quality table, lead/joint modes, corner
    /// mode (KC1–KC14).
    pub comp: crate::kerf::CompSettings,
    /// B056: the machine's part gap for nesting, mm (MTL9). Never applied by
    /// itself: the sheet's gap stays the user's (see `spacing`).
    pub part_gap: Option<f64>,
    /// A256–A259: sheet margins bottom, top, left, right, mm (MTL9).
    pub margins: Option<[f64; 4]>,
    /// A260: remnant margin, mm.
    pub remnant_margin: Option<f64>,
    /// B096 / B097: a different gap along Y, and that gap, mm.
    pub y_gap: Option<f64>,
    /// B352: default pierce radius, mm (read and shown, not used yet).
    pub pierce_radius: Option<f64>,
    /// A058: default cutting speed, mm/min (or the section's Z003): the
    /// "active" speed of the time calculation (CTM8).
    pub cut_speed: Option<f64>,
    /// A051: rapid speed, mm/min (CTM3).
    pub rapid_speed: Option<f64>,
    /// A063: marking speed of the cutting head, mm/min (CTM9).
    pub mark_speed: Option<f64>,
    /// The machine's time settings and technology table (`.TM1` / `.TT1`, TT1).
    pub speed_files: Arc<crate::techtable::SpeedFiles>,
    /// A093: the machine marks one section per material and thickness as the
    /// default technology (MTL5).
    pub default_rule: bool,
    /// The whole file: machine values and every material × thickness section.
    pub file: Arc<MachineFile>,
    /// The section these values came from (index into `file.sections`); None =
    /// the machine's own values.
    pub section: Option<usize>,
    /// The material key and thickness the values were picked for (None = not
    /// picked for a job: the machine's own values).
    pub picked_for: Option<(String, f64)>,
    /// Fewest/most joints by size edited in the window (Micro-joints →
    /// Settings): they win over the file's, section or not (TEC-9).
    pub sizes_override: Option<crate::nest::AutoJoints>,
    /// The file the values came from (None = built-in values).
    pub source: Option<PathBuf>,
    /// The machine's cut qualities as they show: names (CQ4) and colours (CQ5).
    pub qualities: Arc<crate::qualities::CutQualities>,
}

impl Default for MachineTech {
    /// The values in C:\Lantek\Expert\MchFiles\PstAdi06.CT1 (the BLM laser).
    fn default() -> Self {
        MachineTech {
            zone: 1,
            corner_max: 135.0,
            min_element: 5.0,
            lead_in: 7.0,
            lead_out: 5.0,
            lead_angle: 45.0,
            hole_lead_in: 7.0,
            hole_lead_out: 5.0,
            leads: crate::leads::LeadTable::default(),
            overlap: 0.5,
            loop_radius: 10.0,
            // F000–F005 of the laser files (PSTBUR01/PSTMAZ03.CT1, matching PstAdi06):
            // type 1 there is a corner type; ours defaults to 3 (perpendicular leads,
            // PSTadi02.CT1) so every contour can take joints.
            joint_type: 3,
            joint_lead: 10.0,
            joint_angle: 45.0,
            joint_lead_out: 0.0,
            joint_gap: 5.0,
            joint_every: 100.0,
            joint_dist: [[100.0, 0.0, 0.0, 0.0]; 12],
            joint_min: 0,
            joint_max: 32767,
            joint_sizes: crate::nest::AutoJoints { max: 32767, ..Default::default() },
            joint_test: -1,
            joint_shorten: -1.0,
            joint_tries: -1,
            sheet_order: SheetOrder::default(),
            hole_order: HoleOrder::default(),
            order_changes: OrderChanges::default(),
            parts_in_holes: true,
            min_hole_area: 0.0,
            group_parts: false,
            group_zones: true,
            zone_gap: 0.0,
            // Nesting (B057 isn't known for PstAdi06; B078 ON as in every shop CT1).
            band_width: 0.0,
            fit_border: true,
            cutter_radius: None,
            comp: crate::kerf::CompSettings::default(),
            part_gap: None,
            margins: None,
            remnant_margin: None,
            y_gap: None,
            pierce_radius: None,
            cut_speed: None,
            rapid_speed: None,
            mark_speed: None,
            speed_files: Arc::new(Default::default()),
            default_rule: false,
            file: Arc::new(MachineFile::default()),
            section: None,
            picked_for: None,
            sizes_override: None,
            source: None,
            qualities: Arc::new(Default::default()),
        }
    }
}

static ACTIVE: std::sync::RwLock<Option<MachineTech>> = std::sync::RwLock::new(None);
/// The machine-file keys of the app's materials (Settings → Materials →
/// Machine table), already normalised; None = the built-in ones.
static KEYS: std::sync::RwLock<Option<Vec<String>>> = std::sync::RwLock::new(None);
/// Bumped whenever the machine or the material keys change (empties the cache).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The last values picked for a material and thickness: (generation, material, thickness bits).
#[allow(clippy::type_complexity)]
static PICKED: std::sync::Mutex<Option<((u64, usize, u64), MachineTech)>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    /// A test's own machine (tests run side by side; the shared one stays built-in).
    static LOCAL: std::cell::RefCell<Option<MachineTech>> = const { std::cell::RefCell::new(None) };
}

/// The active machine's technology (set when settings are applied): its own
/// values, before any material × thickness section.
pub fn active() -> MachineTech {
    #[cfg(test)]
    if let Some(t) = LOCAL.with(|l| l.borrow().clone()) {
        return t;
    }
    ACTIVE.read().ok().and_then(|g| g.clone()).unwrap_or_default()
}

pub fn set_active(t: MachineTech) {
    if let Ok(mut g) = ACTIVE.write() {
        *g = Some(t);
    }
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Tests: use this machine on the calling thread only (None = the shared one again).
#[cfg(test)]
pub fn set_thread_active(t: Option<MachineTech>) {
    LOCAL.with(|l| *l.borrow_mut() = t);
}

/// Set the machine-file key of each app material, by id
/// (`engine::material::machine_keys`).
pub fn set_material_keys(keys: Vec<String>) {
    if let Ok(mut g) = KEYS.write() {
        if g.as_ref() != Some(&keys) {
            *g = Some(keys);
            GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// The machine-file key of app material `i` (its id).
pub fn key_of_material(i: usize) -> String {
    if let Some(k) = KEYS.read().ok().and_then(|g| g.as_ref().and_then(|v| v.get(i).cloned())) {
        return k;
    }
    crate::engine::material::default_machine_key(i)
}

/// The active machine's values for a sheet: the first section of its file
/// that fits the sheet's material and thickness, else its own values (MTL1–MTL6).
pub fn for_spec(spec: &crate::nest::SheetSpec) -> MachineTech {
    for_material(spec.material, spec.thickness)
}

/// The active machine's values for app material `material` at `thickness` mm.
pub fn for_material(material: usize, thickness: f64) -> MachineTech {
    let keys = JobKeys::new(&key_of_material(material), thickness);
    #[cfg(test)]
    if let Some(t) = LOCAL.with(|l| l.borrow().clone()) {
        return t.for_job(&keys);
    }
    let id = (GENERATION.load(std::sync::atomic::Ordering::SeqCst), material, thickness.to_bits());
    if let Some((k, t)) = PICKED.lock().ok().and_then(|g| g.clone()) {
        if k == id {
            return t;
        }
    }
    let t = active().for_job(&keys);
    if let Ok(mut g) = PICKED.lock() {
        *g = Some((id, t.clone()));
    }
    t
}

/// The window's micro-joint size classes (Micro-joints → Settings) for the
/// active machine: they win over the file's values from now on.
pub fn set_joint_sizes(sizes: crate::nest::AutoJoints) {
    #[cfg(test)]
    if LOCAL.with(|l| l.borrow().is_some()) {
        LOCAL.with(|l| {
            if let Some(t) = l.borrow_mut().as_mut() {
                t.joint_sizes = sizes;
                t.sizes_override = Some(sizes);
            }
        });
        return;
    }
    let mut a = active();
    a.joint_sizes = sizes;
    a.sizes_override = Some(sizes);
    set_active(a);
}

/// `code → value` for every `Xnnn value ; comment` line outside the material
/// × thickness sections (quotes kept). Section lines are in `parse_file`.
pub fn parse(text: &str) -> HashMap<String, String> {
    parse_file(text).globals
}

/// One `Xnnn value` line: (code, value), or None for comments and other lines.
fn code_line(line: &str) -> Option<(&str, &str)> {
    let line = line.split(';').next().unwrap_or("").trim();
    let mut it = line.splitn(2, char::is_whitespace);
    let (Some(code), Some(val)) = (it.next(), it.next()) else { return None };
    let b = code.as_bytes();
    (b.len() == 4 && b[0].is_ascii_uppercase() && b[1..].iter().all(u8::is_ascii_digit)).then(|| (code, val.trim()))
}

/// A whole machine file: the machine's own values and its material ×
/// thickness sections, in file order.
#[derive(Clone, Default, PartialEq)]
pub struct MachineFile {
    /// Codes outside the sections (A…N blocks); a repeated code keeps its last value.
    pub globals: HashMap<String, String>,
    pub sections: Vec<Section>,
}

impl std::fmt::Debug for MachineFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MachineFile {{ {} codes, {} sections }}", self.globals.len(), self.sections.len())
    }
}

/// One material × thickness section (MTL1, MTL3, MTL4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// Place in the file, from 1.
    pub no: usize,
    /// Z000: the material key (quotes off, short catalogue numbers padded, MTL2);
    /// "*" = every material.
    pub material: String,
    /// Z001 / Z002: thickness from / to, mm (a negative "from" = any thickness).
    pub t_min: f64,
    pub t_max: f64,
    /// Z343: laser gas number; Z366: oxyfuel/plasma gas number (0 = none).
    pub gas: i32,
    pub torch_gas: i32,
    /// Z342 lens, Z344 power, Z367 torch power, Z345 nozzle Ø, Z346 pressure,
    /// Z348 abrasive flow, Z349 mixing tube Ø.
    pub numbers: [f64; 7],
    /// Z347: abrasive; Z354: configuration alias.
    pub abrasive: String,
    pub alias: String,
    /// Z350: this is the default technology for its material and thickness (MTL5).
    pub default: bool,
    /// Z010 / Z011 / Z012 triples: hole size from, to, technology index (MTL10;
    /// read, not used yet).
    pub hole_bands: Vec<(f64, f64, i32)>,
    /// Every V/X/Y/Z line of the section (a repeated code keeps its last value).
    pub codes: HashMap<String, String>,
}

/// Codes of the section's match keys that are numbers (see `Section::numbers`).
const NUMBER_KEYS: [&str; 7] = ["Z342", "Z344", "Z367", "Z345", "Z346", "Z348", "Z349"];

impl Section {
    fn finish(mut self, no: usize) -> Section {
        let txt = |k: &str| self.codes.get(k).map(|v| v.trim().trim_matches('"').to_string()).unwrap_or_default();
        let num = |k: &str| self.codes.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        self.no = no;
        self.material = material_key(&txt("Z000"));
        self.t_min = num("Z001").unwrap_or(-1.0);
        self.t_max = num("Z002").unwrap_or(99999.0);
        self.gas = num("Z343").map_or(0, |v| v as i32);
        self.torch_gas = num("Z366").map_or(0, |v| v as i32);
        for (k, c) in NUMBER_KEYS.iter().enumerate() {
            self.numbers[k] = num(c).unwrap_or(0.0);
        }
        self.abrasive = txt("Z347");
        self.alias = txt("Z354");
        self.default = self.codes.get("Z350").is_some_and(|v| v.trim() == "ON");
        self
    }

    /// "0010 mild steel 0.4–1.6 mm" (the material key in shop words).
    pub fn label(&self) -> String {
        let range = if self.t_min < 0.0 { "any thickness".to_string() } else { format!("{}–{} mm", self.t_min, self.t_max) };
        format!("{} {range}", material_label(&self.material))
    }
}

/// Read a machine file in order (MTL1): the A…N lines are the machine's own
/// values; V/X/Y/Z lines fill one section until `Z999` closes it. A section
/// left open at the end of the file is never tested, so it is dropped.
pub fn parse_file(text: &str) -> MachineFile {
    let mut f = MachineFile::default();
    let mut cur: Option<Section> = None;
    for line in text.lines() {
        let Some((code, val)) = code_line(line) else { continue };
        if !matches!(code.as_bytes()[0], b'V' | b'X' | b'Y' | b'Z') {
            f.globals.insert(code.to_string(), val.to_string());
            continue;
        }
        if code == "Z999" {
            if let Some(s) = cur.take() {
                let no = f.sections.len() + 1;
                f.sections.push(s.finish(no));
            }
            continue;
        }
        let s = cur.get_or_insert_with(Section::default);
        let num = val.parse::<f64>().unwrap_or(0.0);
        match code {
            "Z010" => s.hole_bands.push((num, 0.0, 0)),
            "Z011" => {
                if let Some(b) = s.hole_bands.last_mut() {
                    b.1 = num;
                }
            }
            "Z012" => {
                if let Some(b) = s.hole_bands.last_mut() {
                    b.2 = num as i32;
                }
            }
            _ => {}
        }
        s.codes.insert(code.to_string(), val.to_string());
    }
    f
}

/// A material key as the file and the job compare it (MTL2): quotes and
/// spaces off; a catalogue-number key with a short number (`##MATEXP10##`) is
/// padded to four digits (`##MATEXP0010##`). Our own shorthand: a bare
/// number ("10", "0010") means that catalogue number.
pub fn material_key(text: &str) -> String {
    let t = text.trim().trim_matches('"').trim();
    let digits = |d: &str| (!d.is_empty() && d.len() <= 4 && d.bytes().all(|b| b.is_ascii_digit())).then(|| format!("##MATEXP{:04}##", d.parse::<u32>().unwrap_or(0)));
    if let Some(k) = digits(t) {
        return k;
    }
    if t.starts_with("##MATEXP") && t.len() < 14 && t.len() >= 10 {
        if let Some(k) = digits(&t[8..t.len() - 2]) {
            return k;
        }
    }
    t.to_string()
}

/// Catalogue numbers of the material keys, in shop words (MTL2).
pub const CATALOGUE: [(&str, &str); 7] = [
    ("0010", "mild steel"),
    ("0100", "stainless steel"),
    ("0200", "pure aluminium"),
    ("0210", "aluminium-magnesium"),
    ("0300", "galvanised steel"),
    ("0510", "copper"),
    ("0590", "brass"),
];

/// A material key for people: "0010 mild steel", "any material", or the name.
pub fn material_label(key: &str) -> String {
    if key == "*" {
        return "any material".into();
    }
    match key.strip_prefix("##MATEXP").and_then(|r| r.strip_suffix("##")) {
        Some(n) => match CATALOGUE.iter().find(|c| c.0 == n) {
            Some((_, name)) => format!("{n} {name}"),
            None => n.to_string(),
        },
        None => key.to_string(),
    }
}

/// What the job brings to the section choice (MTL3, MTL4). Our jobs carry no
/// gas, lens, power, nozzle, abrasive or alias: those stay "not given" (0 / ""),
/// which every section accepts (a design choice; FEATURES TEC-10).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobKeys {
    pub material: String,
    pub thickness: f64,
    pub gas: i32,
    pub torch_gas: i32,
    pub numbers: [f64; 7],
    pub abrasive: String,
    pub alias: String,
}

impl JobKeys {
    pub fn new(material: &str, thickness: f64) -> JobKeys {
        JobKeys { material: material_key(material), thickness, ..Default::default() }
    }

    /// Nothing but material and thickness given (the default-technology rule, MTL5).
    pub fn unset(&self) -> bool {
        self.gas == 0 && self.torch_gas == 0 && self.numbers.iter().all(|&v| v <= NOT_GIVEN) && (self.abrasive.is_empty() || self.abrasive == "*") && self.alias.is_empty()
    }
}

/// Thickness slack when matching a section's range, mm (MTL3).
pub const THICKNESS_SLACK: f64 = 0.005;
/// A job number at or below this is "not given" (MTL4).
pub const NOT_GIVEN: f64 = 0.001;
/// Two numbers (lens, nozzle …) match within this (MTL4).
pub const NUMBER_TOL: f64 = 0.01;

/// MTL3: a negative "from" fits any thickness; else from − 0.005 ≤ t ≤ to + 0.005.
pub fn thickness_fits(from: f64, to: f64, t: f64) -> bool {
    from < 0.0 || (from - THICKNESS_SLACK <= t && t <= to + THICKNESS_SLACK)
}

/// Does a section fit the job (MTL2–MTL4)? The material key must be equal
/// (case counts) or the section's "*"; the thickness in range; every other key
/// equal or not given by the job.
pub fn section_fits(s: &Section, k: &JobKeys) -> bool {
    let num_ok = |job: f64, sec: f64| job <= NOT_GIVEN || (job - sec).abs() <= NUMBER_TOL;
    (s.material == "*" || s.material == k.material)
        && thickness_fits(s.t_min, s.t_max, k.thickness)
        && (k.gas == 0 || k.gas == s.gas)
        && (k.torch_gas == 0 || k.torch_gas == s.torch_gas)
        && k.numbers.iter().zip(&s.numbers).all(|(&j, &v)| num_ok(j, v))
        && (k.abrasive.is_empty() || k.abrasive == "*" || k.abrasive == s.abrasive)
        && (k.alias.is_empty() || k.alias == s.alias)
}

/// Which section applies (MTL5): the first that fits, in file order. When the
/// machine marks default technologies (`default_rule`) and the job gives only
/// material and thickness, a fitting section that isn't marked default is only
/// a fallback: the first such one is kept while the search goes on, and a later
/// fitting section marked default wins. None = no section fits (MTL6).
pub fn pick_section(sections: &[Section], k: &JobKeys, default_rule: bool) -> Option<usize> {
    let defaults = default_rule && k.unset();
    let mut fallback = None;
    for (i, s) in sections.iter().enumerate() {
        if !section_fits(s, k) {
            continue;
        }
        if defaults && !s.default {
            fallback = fallback.or(Some(i));
            continue;
        }
        return Some(i);
    }
    fallback
}

/// The machine code a section line stands for (MTL8, MTL9): Y = the lead
/// table of qualities 2–12 (same numbers as E), Z054–Z124 / Z132 / Z305 / Z306 =
/// quality 1's lead table (B200–B271, B294, B295), X = the micro-joint table
/// (F, with fewest/most moved one down), and the single Z values that replace
/// a machine value. None = not used by the app (V records, hole bands, punch
/// and chamfer values, conditional groups).
pub fn machine_code(code: &str) -> Option<String> {
    let n: u32 = code.get(1..)?.parse().ok()?;
    let to = |l: char, v: u32| Some(format!("{l}{v:03}"));
    match code.as_bytes()[0] {
        b'Y' => to('E', n),
        b'X' => match n {
            0..=118 | 129..=197 => to('F', n),
            122 | 123 => to('F', n - 1),
            125..=128 => to('F', n - 2),
            _ => None,
        },
        b'Z' => match n {
            3 => to('A', 58),
            4 => to('B', 2),
            6 => to('B', 56),
            7..=9 => to('A', 250 + n),
            16 => to('A', 59),
            // KC3: the section's corrector numbers for holes and parts.
            24 => to('B', 124),
            25 => to('B', 125),
            26..=53 => to('B', n + 124),
            54..=124 => to('B', n + 146),
            125 => to('B', 178),
            126..=129 => to('B', n + 53),
            130 => to('A', 260),
            131 => to('B', 352),
            132 => to('B', 271),
            301 => to('B', 292),
            305 => to('B', 294),
            306 => to('B', 295),
            335 => to('B', 96),
            336 => to('B', 97),
            _ => None,
        },
        _ => None,
    }
}

/// Machine codes a fitting section replaces as whole tables (MTL7): the
/// automatic lead tables (B200–B271, B294, B295, the E block) and the
/// micro-joint table (F). What the section leaves out takes the built-in
/// values, not the machine's.
fn table_code(code: &str) -> bool {
    let n: u32 = code.get(1..).and_then(|x| x.parse().ok()).unwrap_or(0);
    match code.as_bytes().first() {
        Some(b'E') | Some(b'F') => true,
        Some(b'B') => (200..=271).contains(&n) || n == 294 || n == 295,
        _ => false,
    }
}

/// The machine's codes with a section laid over them (MTL7–MTL9). Z005 fills
/// all four margins before Z007–Z009 set top, left and right.
pub fn codes_with_section(globals: &HashMap<String, String>, s: &Section) -> HashMap<String, String> {
    let mut m: HashMap<String, String> = globals.iter().filter(|(k, _)| !table_code(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    if let Some(v) = s.codes.get("Z005") {
        for c in ["A256", "A257", "A258", "A259"] {
            m.insert(c.into(), v.clone());
        }
    }
    for (k, v) in &s.codes {
        if let Some(g) = machine_code(k) {
            m.insert(g, v.clone());
        }
    }
    m
}

impl MachineTech {
    pub fn from_text(text: &str) -> MachineTech {
        let file = parse_file(text);
        let mut t = MachineTech::from_codes(&file.globals);
        t.file = Arc::new(file);
        t
    }

    /// The values for a job (MTL1–MTL7): the machine's own values with the
    /// fitting section laid over them, or the machine's own when none fits.
    /// The window's joint size classes (TEC-9) win either way.
    pub fn for_job(&self, k: &JobKeys) -> MachineTech {
        let mut t = match pick_section(&self.file.sections, k, self.default_rule) {
            Some(i) => {
                let mut t = MachineTech::from_codes(&codes_with_section(&self.file.globals, &self.file.sections[i]));
                t.file = self.file.clone();
                t.source = self.source.clone();
                t.speed_files = self.speed_files.clone();
                t.qualities = self.qualities.clone();
                t.section = Some(i);
                // Settings → Machines wins over the section too (CO16, CO17).
                t.with_order_changes(self.order_changes)
            }
            None => MachineTech { section: None, ..self.clone() },
        };
        t.picked_for = Some((k.material.clone(), k.thickness));
        if let Some(o) = self.sizes_override {
            t.joint_sizes = o;
            t.sizes_override = Some(o);
        }
        t
    }

    /// The cut quality the applied section's size bands give a contour of
    /// equivalent radius `r` mm (MTL10, TT8): none when the section has no
    /// bands or the first starts above `r`; else the first band with
    /// from ≤ r < to, its technology index turned into a quality.
    pub fn band_quality(&self, r: f64) -> Option<usize> {
        let bands = &self.applied()?.hole_bands;
        if bands.first().is_none_or(|b| b.0 > r) {
            return None;
        }
        bands.iter().find(|b| b.0 <= r && r < b.1).map(|b| crate::techtable::quality_of_index(b.2))
    }

    /// The sheet and hole order the machine cuts with: the machine file's,
    /// with any change made in Settings → Machines (CO16, CO17).
    pub fn order(&self) -> OrderSettings {
        OrderSettings { sheet: self.sheet_order, holes: self.hole_order }
    }

    /// The sheet and hole order the machine file alone gives: what "Use the
    /// machine file" puts back (CO16, CO17).
    pub fn file_order(&self) -> OrderSettings {
        OrderSettings::from_codes(&self.file.globals)
    }

    /// These values with the order settings changed in Settings → Machines
    /// laid over the file's (CO16, CO17).
    pub fn with_order_changes(mut self, c: OrderChanges) -> MachineTech {
        // Only the changes used are kept, so `order_changes` says which
        // values come from Settings → Machines.
        let c = c.valid();
        let o = c.apply(self.order());
        self.sheet_order = o.sheet;
        self.hole_order = o.holes;
        self.order_changes = c;
        self
    }

    /// The machine file sets start bottom off (B111), which the sheet's
    /// order doesn't follow: it starts at the bottom (CZ6, CO28).
    pub fn file_says_top(&self) -> bool {
        self.file.globals.get("B111").is_some_and(|v| v.trim() == "OFF")
    }

    /// The section the values came from.
    pub fn applied(&self) -> Option<&Section> {
        self.section.and_then(|i| self.file.sections.get(i))
    }

    /// One line for the window and the agents: which table the values come from.
    pub fn table_label(&self) -> String {
        match self.applied() {
            Some(s) => format!("Machine table: {}", s.label()),
            None if self.file.sections.is_empty() => "Machine table: global (the machine file has no material sections)".into(),
            None => "Machine table: global (no section for this material/thickness)".into(),
        }
    }

    /// The margin a sheet takes from the machine's four: the widest, so no
    /// side gets less than the machine asks (our choice: the sheet has one margin).
    pub fn sheet_margin(&self) -> Option<f64> {
        self.margins.map(|m| m.iter().copied().fold(0.0, f64::max))
    }

    pub fn from_codes(m: &HashMap<String, String>) -> MachineTech {
        let d = MachineTech::default();
        let f = |k: &str, def: f64| m.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(def);
        let u = |k: &str, def: u32| m.get(k).and_then(|v| v.parse::<i64>().ok()).map_or(def, |v| v.clamp(0, u32::MAX as i64) as u32);
        let i = |k: &str, def: i32| m.get(k).and_then(|v| v.parse::<f64>().ok()).map_or(def, |v| v as i32);
        let num = |k: &str| m.get(k).and_then(|v| v.parse::<f64>().ok());
        let leads = crate::leads::LeadTable::from_codes(m);
        let q1 = leads.quality(1);
        // Joints (AM1, MT11): F0q0–F0q8 = quality q's
        // default record; F150+4q… = its distances; an unset gap (99999.999) = B164.
        let b164 = f("B164", d.joint_gap);
        let gap = f("F005", d.joint_gap);
        let gap = if (gap - crate::leads::UNSET).abs() <= crate::leads::UNSET_TOL { b164 } else { gap };
        let mut joint_dist = d.joint_dist;
        for (q, row) in joint_dist.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = f(&format!("F{}", 150 + 4 * q + j), *v);
            }
        }
        let (jmin, jmax) = (u("F121", d.joint_min), u("F122", d.joint_max));
        let pair = |a: &str, b: &str| [i(a, -1), i(b, -1)];
        let joint_sizes = crate::nest::AutoJoints {
            small: [f("F136", 0.0), f("F137", 0.0)],
            large: [f("F138", 99999.999), f("F139", 99999.999)],
            parts: [pair("F142", "F143"), [jmin as i32, jmax.min(i32::MAX as u32) as i32], pair("F148", "F149")],
            holes: [pair("F140", "F141"), pair("F144", "F145"), pair("F146", "F147")],
            min: jmin,
            max: jmax,
        };
        MachineTech {
            zone: q1.zone.clamp(1, 9) as u8,
            corner_max: q1.corner_max,
            min_element: f("B204", d.min_element),
            lead_in: q1.recs[5].d_in,
            lead_out: q1.recs[5].d_out,
            lead_angle: q1.recs[5].a_in,
            hole_lead_in: q1.recs[0].d_in,
            hole_lead_out: q1.recs[0].d_out,
            overlap: leads.overlap,
            leads,
            loop_radius: f("B157", d.loop_radius),
            joint_type: i("F000", d.joint_type),
            joint_lead: f("F001", d.joint_lead),
            joint_angle: f("F003", d.joint_angle),
            joint_lead_out: f("F002", d.joint_lead_out),
            joint_gap: gap,
            joint_every: joint_dist[0][0],
            joint_dist,
            joint_min: jmin,
            joint_max: jmax,
            joint_sizes,
            joint_test: i("F133", d.joint_test),
            joint_shorten: f("F134", d.joint_shorten),
            joint_tries: i("F135", d.joint_tries),
            sheet_order: SheetOrder::from_codes(m),
            hole_order: HoleOrder::from_codes(m),
            order_changes: OrderChanges::default(),
            parts_in_holes: m.get("B134").map_or(d.parts_in_holes, |v| v == "ON"),
            min_hole_area: f("B076", d.min_hole_area),
            group_parts: m.get("A785").map_or(d.group_parts, |v| v == "ON"),
            group_zones: m.get("A787").map_or(d.group_zones, |v| v == "ON"),
            zone_gap: f("A788", d.zone_gap),
            // Nesting: strip width and fit to the border.
            band_width: f("B057", d.band_width).max(0.0),
            fit_border: m.get("B078").map_or(d.fit_border, |v| v == "ON"),
            cutter_radius: num("B002"),
            comp: crate::kerf::CompSettings::from_codes(m),
            part_gap: num("B056"),
            margins: match (num("A256"), num("A257"), num("A258"), num("A259")) {
                (None, None, None, None) => None,
                (b, t, l, r) => Some([b.unwrap_or(0.0), t.unwrap_or(0.0), l.unwrap_or(0.0), r.unwrap_or(0.0)]),
            },
            remnant_margin: num("A260"),
            y_gap: if m.get("B096").is_some_and(|v| v == "ON") { num("B097") } else { None },
            pierce_radius: num("B352"),
            cut_speed: num("A058"),
            rapid_speed: num("A051"),
            mark_speed: num("A063"),
            speed_files: d.speed_files.clone(),
            default_rule: m.get("A093").is_some_and(|v| v == "ON"),
            file: d.file.clone(),
            section: None,
            picked_for: None,
            sizes_override: None,
            source: None,
            qualities: Arc::new(crate::qualities::CutQualities::from_codes(m)),
        }
    }

    /// Read a CT1 file (Latin-1 text).
    pub fn load(path: &Path) -> Option<MachineTech> {
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut t = MachineTech::from_text(&text);
        t.source = Some(path.to_path_buf());
        Some(t)
    }

    /// The machine's CT1: beside its post library or BLM .CF1/.PS1 (same
    /// name, .CT1), else the BLM post's file in the Lantek folder.
    pub fn for_machine(m: &crate::settings::Machine, lantek_dir: &str) -> MachineTech {
        let mut tries: Vec<PathBuf> = vec![];
        for p in [&m.blm_cf1, &m.blm_ps1, &m.post_lib] {
            if !p.is_empty() {
                let pb = PathBuf::from(p);
                tries.push(pb.with_extension("CT1"));
                tries.push(pb.with_extension("ct1"));
            }
        }
        let dir = if lantek_dir.is_empty() { crate::settings::lantek_default_dir() } else { lantek_dir.to_string() };
        tries.push(Path::new(&dir).join("MchFiles").join("PstAdi06.CT1"));
        let found = tries.iter().find(|p| p.is_file());
        let mut t = found.and_then(|p| MachineTech::load(p)).unwrap_or_default();
        // The qualities' colours are the original's display settings, the same
        // for every machine (CQ5).
        t.qualities = Arc::new(t.qualities.as_ref().clone().with_colours_in(Path::new(&dir)));
        // The machine's CF file names its technology table and time settings
        // (TT1): the BLM post's .CF1, else a .CF<n> post library.
        let cf = [&m.blm_cf1, &m.post_lib].into_iter().filter(|p| !p.is_empty()).map(PathBuf::from).find(|p| {
            let e = p.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
            e.starts_with("CF") && e != "CFF" && p.is_file()
        });
        t.speed_files = Arc::new(crate::techtable::SpeedFiles::find(found.map(|p| p.as_path()), cf.as_deref(), m.gas_id));
        // The sheet and hole order changed for this machine in Settings →
        // Machines win over its file (CO16, CO17).
        t.with_order_changes(m.order)
    }

    /// Joints for a medium-sized part contour of length `per` at one per
    /// `joint_every` mm (F150), within F121–F122: ceil, then made even
    /// (corrected in v44 build analysis: not rounded down, F120 not read).
    pub fn joints_for(&self, per: f64) -> u32 {
        crate::joints::count(per, self.joint_every, self.joint_min, self.joint_max, self.leads.lin_tol).0
    }
}

/// A small made-up machine file with material × thickness sections, for tests.
#[cfg(test)]
pub fn demo_sections_file() -> String {
    // Leads of every contour class (quality 1): distance in/out `d` (Z068/Z069,
    // Z074/Z075 … Z110/Z111) and arc radius in/out d + 0.5 (Z072/Z073 …).
    let leads = |d: f64| (0..8).map(|k| format!("Z{:03} {d}\nZ{:03} {d}\nZ{:03} {}\nZ{:03} {}\n", 68 + 6 * k, 69 + 6 * k, 72 + 6 * k, d + 0.5, 73 + 6 * k, d + 0.5)).collect::<String>();
    let mut t = String::from("# Demo laser\nA093 OFF\nB056 6.0\nA256 7\nA257 7\nA258 7\nA259 7\nB352 1.0\nF005 0.5\nB164 0.5\nB244 7.0\n");
    // Mild steel by its short catalogue number: thin sheet.
    t += "Z000 \"##MATEXP10##\"\nZ001 0.4\nZ002 1.6\nZ004 0.10\nZ005 10\nZ006 5\nZ131 1.5\n";
    t += &leads(2.0);
    t += "Z040 0.30\nX001 2.0\nX005 0.30\nZ999 0\n";
    // Mild steel 1.7–2.1 (a gap to the next range).
    t += "Z000 \"##MATEXP0010##\"\nZ001 1.7\nZ002 2.1\nZ006 5\n";
    t += &leads(2.0);
    t += "X005 0.35\nZ999 0\n";
    // Mild steel 2.2–3.2.
    t += "Z000 \"##MATEXP0010##\"\nZ001 2.2\nZ002 3.2\nZ006 6\nZ005 10\nZ008 12\n";
    t += &leads(3.0);
    t += "X001 3.0\nX005 0.35\nX150 150\nZ999 0\n";
    // Mild steel plate 12.9–20.1.
    t += "Z000 \"##MATEXP0010##\"\nZ001 12.9\nZ002 20.1\nZ004 0.45\nZ005 10\nZ006 15\nZ131 5\nZ033 8\n";
    t += &leads(13.0);
    t += "Y000 1\nY014 13\nX001 12.0\nX005 0.70\nX122 2\nX123 8\nZ999 0\n";
    // Stainless.
    t += "Z000 \"##MATEXP0100##\"\nZ001 0.4\nZ002 20.1\nZ006 9\nX005 0.40\nZ999 0\n";
    t
}

#[cfg(test)]
mod ct1_tests {
    use super::*;

    #[test]
    fn sections_are_read_in_order_and_keep_their_own_values() {
        let f = parse_file(&demo_sections_file());
        assert_eq!(f.sections.len(), 5);
        assert_eq!(f.sections.iter().map(|s| s.no).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
        // The short catalogue number is padded (MTL2).
        assert_eq!(f.sections[0].material, "##MATEXP0010##");
        assert_eq!((f.sections[2].t_min, f.sections[2].t_max), (2.2, 3.2));
        // Codes repeated from section to section don't leak: the third has no Z131 or Z004.
        assert_eq!(f.sections[0].codes.get("Z131").map(String::as_str), Some("1.5"));
        assert!(!f.sections[2].codes.contains_key("Z131") && !f.sections[2].codes.contains_key("Z004"));
        // Section lines are not machine values; machine values stay.
        assert!(!f.globals.contains_key("Z006") && !f.globals.contains_key("X005"));
        assert_eq!(parse(&demo_sections_file()).get("B056").map(String::as_str), Some("6.0"));
        let thick = MachineTech::from_text(&demo_sections_file()).for_job(&JobKeys::new("10", 3.0));
        // Pierce radius and kerf: from the machine, not from the thin-sheet section before it.
        assert_eq!((thick.pierce_radius, thick.cutter_radius), (Some(1.0), None));
        // A section left open at the end of the file is dropped.
        assert_eq!(parse_file("Z000 \"*\"\nZ001 -1\n").sections.len(), 0);
    }

    #[test]
    fn three_mm_mild_steel_picks_its_thickness_range() {
        let m = MachineTech::from_text(&demo_sections_file());
        let t = m.for_job(&JobKeys::new("0010", 3.0));
        assert_eq!(t.section, Some(2));
        assert_eq!(t.table_label(), "Machine table: 0010 mild steel 2.2–3.2 mm");
        assert_eq!((t.lead_in, t.hole_lead_in, t.joint_gap, t.joint_lead, t.joint_every), (3.0, 3.0, 0.35, 3.0, 150.0));
        assert_eq!(t.part_gap, Some(6.0));
        // Z005 fills every margin, then Z008 sets the left one; the sheet takes the widest.
        assert_eq!((t.margins, t.sheet_margin()), (Some([10.0, 10.0, 12.0, 10.0]), Some(12.0)));
        // The range ends count, with 0.005 mm slack (MTL3).
        assert_eq!(m.for_job(&JobKeys::new("0010", 3.205)).section, Some(2));
        assert_eq!(m.for_job(&JobKeys::new("0010", 2.2)).section, Some(2));
        // Stainless has its own.
        assert_eq!(m.for_job(&JobKeys::new("##MATEXP100##", 8.0)).part_gap, Some(9.0));
    }

    #[test]
    fn a_thickness_between_two_ranges_keeps_the_machines_own_values() {
        let m = MachineTech::from_text(&demo_sections_file());
        let t = m.for_job(&JobKeys::new("0010", 1.65));
        assert_eq!(t.section, None);
        assert_eq!(t.table_label(), "Machine table: global (no section for this material/thickness)");
        assert_eq!((t.lead_in, t.part_gap, t.joint_gap), (7.0, Some(6.0), 0.5));
        // An unknown material too.
        assert_eq!(m.for_job(&JobKeys::new("0590", 3.0)).section, None);
        // A file without sections says so.
        assert!(MachineTech::default().for_job(&JobKeys::new("0010", 3.0)).table_label().contains("no material sections"));
    }

    #[test]
    fn a_star_section_fits_every_material_and_any_thickness() {
        let text = "Z000 \"*\"\nZ001 -1\nZ002 0\nZ006 4\nZ999 0\nZ000 \"##MATEXP0010##\"\nZ001 0\nZ002 99\nZ006 9\nZ999 0\n";
        let m = MachineTech::from_text(text);
        for (k, t) in [("0010", 3.0), ("0590", 0.8), ("Copper", 25.0)] {
            let r = m.for_job(&JobKeys::new(k, t));
            // The first fitting section wins, even before a section for the exact material.
            assert_eq!((r.section, r.part_gap), (Some(0), Some(4.0)), "{k}");
        }
        assert_eq!(material_label("*"), "any material");
    }

    #[test]
    fn short_catalogue_numbers_are_padded_and_names_compare_exactly() {
        assert_eq!(material_key("\"##MATEXP10##\""), "##MATEXP0010##");
        assert_eq!(material_key("##MATEXP590##"), "##MATEXP0590##");
        assert_eq!(material_key("##MATEXP0100##"), "##MATEXP0100##");
        assert_eq!(material_key("210"), "##MATEXP0210##");
        assert_eq!(material_key(" AISI304 "), "AISI304");
        assert_eq!(material_label("##MATEXP0300##"), "0300 galvanised steel");
        // A plain-name section fits only the same name, case and all.
        let s = Section { material: "AISI304".into(), t_min: 0.0, t_max: 10.0, ..Default::default() };
        assert!(section_fits(&s, &JobKeys::new("AISI304", 2.0)));
        assert!(!section_fits(&s, &JobKeys::new("aisi304", 2.0)));
    }

    #[test]
    fn the_default_technology_wins_over_an_earlier_fitting_section() {
        // MTL5: three sections fit; with the machine's default rule on, the
        // first is only a fallback and the one marked default wins.
        let sec = |gas: i32, def: bool, gap: f64| format!("Z000 \"##MATEXP0010##\"\nZ001 0.4\nZ002 1.6\nZ006 {gap}\nZ343 {gas}\nZ350 {}\nZ999 0\n", if def { "ON" } else { "OFF" });
        let body = format!("{}{}{}", sec(3, false, 5.0), sec(1, true, 6.0), sec(2, true, 7.0));
        let on = MachineTech::from_text(&format!("A093 ON\n{body}"));
        assert_eq!(on.for_job(&JobKeys::new("0010", 1.0)).part_gap, Some(6.0));
        let off = MachineTech::from_text(&format!("A093 OFF\n{body}"));
        assert_eq!(off.for_job(&JobKeys::new("0010", 1.0)).part_gap, Some(5.0));
        // Only non-default sections fit: the first is kept.
        let only = MachineTech::from_text(&format!("A093 ON\n{}{}", sec(3, false, 5.0), sec(2, false, 8.0)));
        assert_eq!(only.for_job(&JobKeys::new("0010", 1.0)).part_gap, Some(5.0));
        // A job that names a gas takes the first fitting section of that gas.
        let k = JobKeys { gas: 2, ..JobKeys::new("0010", 1.0) };
        assert_eq!(on.for_job(&k).part_gap, Some(7.0));
    }

    #[test]
    fn a_plate_section_replaces_the_lead_and_joint_tables() {
        let m = MachineTech::from_text(&demo_sections_file());
        let t = m.for_job(&JobKeys::new("0010", 15.0));
        // Quality 1 from Z, quality 2 from Y (same numbers as E), quality 3 copies quality 1.
        assert_eq!(t.leads.quality(1).recs[5].d_in, 13.0);
        assert_eq!(t.leads.quality(2).recs[0].d_in, 13.0);
        assert_eq!(t.leads.quality(3).recs[3].d_in, 13.0);
        // Joints: X → F, fewest/most from X122/X123; no X150 = the built-in 100 mm (MTL7).
        assert_eq!((t.joint_gap, t.joint_lead, t.joint_min, t.joint_max, t.joint_every), (0.70, 12.0, 2, 8, 100.0));
        assert_eq!((t.cutter_radius, t.pierce_radius, t.loop_radius, t.part_gap), (Some(0.45), Some(5.0), 8.0, Some(15.0)));
        // The machine's own lead lengths are not mixed in.
        assert_eq!(m.lead_in, 7.0);
    }

    #[test]
    fn reads_the_blm_technology_lines() {
        // Corrected in v44 build analysis: quality 1's contour lead is B244 (E044 is
        // quality 2), the joint distance is F150 (F120 is not read), and the count
        // is rounded up and made even.
        let t = "# DATOS\nB156 0.5                   ; Solapamiento de las entradas y salidas.\nB244 8.0 ; Distancia entrada contornos 1.\nE000 1\nE044 9.0\nF005 2.5 ; Long. no corte\nF120 150.0 ; Distancia entre amarres.\nF150 120.0\nF122 32767\nA009 \"CAJPIE.CAJ\" ; Cajetin\n";
        let m = parse(t);
        assert_eq!(m.get("A009").map(String::as_str), Some("\"CAJPIE.CAJ\""));
        let mt = MachineTech::from_text(t);
        assert_eq!((mt.overlap, mt.lead_in, mt.joint_gap, mt.joint_every, mt.joint_max), (0.5, 8.0, 2.5, 120.0, 32767));
        assert_eq!(mt.leads.quality(2).recs[5].d_in, 9.0);
        // Missing codes keep the PstAdi06.CT1 values.
        assert_eq!((mt.lead_out, mt.joint_lead, mt.zone), (5.0, 10.0, 1));
        // 620 mm at one per 120 mm: ceil 5.2 = 6.
        assert_eq!(mt.joints_for(620.0), 6);
        // An unset gap takes B164.
        assert_eq!(MachineTech::from_text("F005 99999.999\nB164 0.35\n").joint_gap, 0.35);
    }

    #[test]
    fn zones_split_the_sheet_and_hold_parts_by_their_corners() {
        let z = zones(2000.0, 1000.0, &SheetOrder::default());
        assert_eq!(z.len(), 200);
        // Left column first, bottom up (as the real BLM program cuts).
        assert_eq!(z[0], [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(z[1], [0.0, 100.0, 100.0, 200.0]);
        assert_eq!(z[10], [100.0, 0.0, 200.0, 100.0]);
        // CZ4: overlap plus a corner inside.
        assert!(box_in_zone([90.0, 10.0, 150.0, 60.0], [0.0, 0.0, 100.0, 100.0]));
        assert!(!box_in_zone([110.0, 10.0, 150.0, 60.0], [0.0, 0.0, 100.0, 100.0]));
        // A long strip crossing the zone with no corner in it: not in that zone.
        assert!(!box_in_zone([-10.0, 40.0, 300.0, 60.0], [100.0, 0.0, 200.0, 100.0]));
        let zz = zones(200.0, 200.0, &SheetOrder { zones_x: 2, zones_y: 2, zigzag: true, ..SheetOrder::default() });
        assert_eq!(zz, vec![[0.0, 0.0, 100.0, 100.0], [0.0, 100.0, 100.0, 200.0], [100.0, 100.0, 200.0, 200.0], [100.0, 0.0, 200.0, 100.0]]);
    }

    #[test]
    fn a_column_of_parts_is_cut_bottom_up_like_the_real_program() {
        use crate::nest::{Part, Placed, Shape, SheetSpec};
        // 1234X38THXSAMPLE: ORIG(10,10) (10,93.2) … (10,1091.6), in that order.
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 1180.0, h: 73.2, qty: 14, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let mut placed: Vec<Placed> = (0..14).map(|k| Placed { part: 0, x: 10.0, y: 10.0 + 83.2 * k as f64, w: 1180.0, h: 73.2, turns: 0, mirror: false, spin: Default::default() }).collect();
        placed.reverse();
        let spec = SheetSpec { w: 1200.15, h: 1219.2, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let plan = crate::cnc::plan(&parts, &placed, &spec);
        let ys: Vec<f64> = plan.contours.iter().map(|c| placed[c.part_no - 1].y).collect();
        assert!(ys.windows(2).all(|w| w[0] <= w[1]), "{ys:?}");
    }

    #[test]
    fn the_sheet_order_is_read_from_the_machine_file_and_the_sheet_starts_at_the_bottom() {
        // A machine file that sets nothing, and none: the BLM file's values
        // (CZ3), the left column first, bottom up, columns, no zig-zag, 20 × 10.
        let built_in = SheetOrder { zones_x: 20, zones_y: 10, start_left: true, start_bottom: true, horizontal: false, zigzag: false };
        assert_eq!(MachineTech::default().sheet_order, built_in);
        assert_eq!(MachineTech::from_text("").sheet_order, built_in);
        assert_eq!(MachineTech::from_text("# Demo laser\nB244 7.0\n").sheet_order, built_in);
        assert!(!MachineTech::from_text("").file_says_top());
        // A made-up file that sets every one: start bottom off is shown and
        // reported, but the sheet still starts at the bottom (CZ6, CO28).
        let t = MachineTech::from_text("# Demo laser\nB110 OFF\nB111 OFF\nB114 ON\nB115 ON\nB122 4\nB123 0\n");
        assert_eq!(t.sheet_order, SheetOrder { zones_x: 4, zones_y: 0, start_left: false, start_bottom: true, horizontal: true, zigzag: true });
        assert!(t.file_says_top());
        assert!(!MachineTech::from_text("B111 ON\n").file_says_top());
        // A material section doesn't change them.
        let mut text = demo_sections_file();
        text.insert_str(0, "B110 OFF\nB122 4\n");
        let t = MachineTech::from_text(&text).for_job(&JobKeys::new("0010", 3.0));
        assert_eq!((t.section, t.sheet_order.start_left, t.sheet_order.zones_x), (Some(2), false, 4));
        // A change made in Settings → Machines wins over a section too (CO16, CO17).
        let c = OrderChanges { zones_x: Some(7), hole_mode: Some(2), ..Default::default() };
        let t = MachineTech::from_text(&text).with_order_changes(c).for_job(&JobKeys::new("0010", 3.0));
        assert_eq!((t.section, t.sheet_order.zones_x, t.hole_order.mode, t.order_changes), (Some(2), 7, HoleMode::Direction, c));
        assert_eq!(t.file_order().sheet.zones_x, 4, "the file's own value stays the file's");
    }

    #[test]
    fn a_change_wins_over_the_machine_file_and_a_value_the_setting_cant_take_leaves_the_files() {
        let file = MachineTech::from_text("# Demo laser\nB565 2\nB566 3\nB568 0.5\n").order();
        // Nothing changed: the file's values.
        assert_eq!(OrderChanges::default().apply(file), file);
        // Every value changed.
        let c = OrderChanges {
            start_left: Some(false), start_bottom: Some(false), rows: Some(true), zigzag: Some(true), zones_x: Some(3), zones_y: Some(1),
            hole_mode: Some(0), hole_start_corner: Some(1), hole_columns: Some(true), orthogonality_tolerance: Some(2.0),
            small_holes_first: Some(true), big_hole_min_length: Some(40.0), big_hole_min_width: Some(30.0), big_hole_offset: Some(2.5),
        };
        let o = c.apply(file);
        assert_eq!(o.sheet, SheetOrder { zones_x: 3, zones_y: 1, start_left: false, start_bottom: false, horizontal: true, zigzag: true });
        assert_eq!(o.holes, HoleOrder { mode: HoleMode::None, corner: 1, columns: true, ortho_tol: 2.0, small_first: true, big_length: 40.0, big_width: 30.0, big_offset: 2.5, tol: file.holes.tol });
        // `between` gives back exactly those changes, and none for the file's own values.
        assert_eq!(OrderChanges::between(&file, &o), c);
        assert!(OrderChanges::between(&file, &file).is_empty());
        // Values the settings don't have leave the file's; zones are capped.
        let odd = OrderChanges { hole_mode: Some(9), hole_start_corner: Some(5), orthogonality_tolerance: Some(-1.0), big_hole_min_length: Some(f64::NAN), zones_x: Some(100_000), ..Default::default() };
        let o = odd.apply(file);
        assert_eq!((o.holes.mode, o.holes.corner, o.holes.ortho_tol, o.holes.big_length, o.sheet.zones_x), (HoleMode::Direction, 3, 0.5, 0.0, 20));
        assert!(odd.valid().is_empty(), "none of them is used");
        assert_eq!(OrderChanges { zones_x: Some(MAX_ZONES), ..odd }.valid(), OrderChanges { zones_x: Some(MAX_ZONES), ..Default::default() });
        // The settings file is read value by value: a bad one is left out.
        assert_eq!(OrderChanges::read(r#"{"zones_x":-1,"rows":true,"hole_mode":9,"zigzag":"on"}"#), OrderChanges { rows: Some(true), ..Default::default() });
        assert!(OrderChanges::read("[1, 2]").is_empty() && OrderChanges::read("").is_empty());
        // Saved as only the values changed; a line from a newer or older app
        // with keys this one doesn't know still reads.
        assert_eq!(serde_json::to_string(&OrderChanges { rows: Some(true), ..Default::default() }).unwrap(), r#"{"rows":true}"#);
        let read: OrderChanges = serde_json::from_str(r#"{"zigzag":true,"cut_by":"heat"}"#).unwrap();
        assert_eq!(read, OrderChanges { zigzag: Some(true), ..Default::default() });
    }

    #[test]
    fn zones_are_cut_from_each_start_corner_in_rows_or_columns_with_or_without_zig_zag() {
        // A 200 × 200 sheet in 2 × 2 zones: lower left (LL), upper left (UL),
        // lower right (LR), upper right (UR).
        let (ll, ul, lr, ur) = ([0.0, 0.0, 100.0, 100.0], [0.0, 100.0, 100.0, 200.0], [100.0, 0.0, 200.0, 100.0], [100.0, 100.0, 200.0, 200.0]);
        let order = |start_left: bool, start_bottom: bool, horizontal: bool, zigzag: bool| zones(200.0, 200.0, &SheetOrder { zones_x: 2, zones_y: 2, start_left, start_bottom, horizontal, zigzag });
        // Columns, every one run the same way.
        assert_eq!(order(true, true, false, false), vec![ll, ul, lr, ur]);
        assert_eq!(order(false, true, false, false), vec![lr, ur, ll, ul]);
        assert_eq!(order(true, false, false, false), vec![ul, ll, ur, lr]);
        assert_eq!(order(false, false, false, false), vec![ur, lr, ul, ll]);
        // Rows.
        assert_eq!(order(true, true, true, false), vec![ll, lr, ul, ur]);
        assert_eq!(order(false, false, true, false), vec![ur, ul, lr, ll]);
        // Zig-zag: every other column or row runs back.
        assert_eq!(order(true, true, false, true), vec![ll, ul, ur, lr]);
        assert_eq!(order(true, false, true, true), vec![ul, ur, lr, ll]);
        // 0 zones along an axis is one zone across it (CZ1), as is 1.
        for n in [0, 1] {
            let z = zones(300.0, 200.0, &SheetOrder { zones_x: n, zones_y: n, ..SheetOrder::default() });
            assert_eq!(z, vec![[0.0, 0.0, 300.0, 200.0]], "{n} zones");
        }
        // Many: every zone, the first and last where the start corner says.
        let z = zones(1500.0, 1000.0, &SheetOrder { zones_x: 500, zones_y: 400, start_left: false, start_bottom: false, ..SheetOrder::default() });
        assert_eq!(z.len(), 200_000);
        assert_eq!((z[0], z[399], z[400]), ([1497.0, 997.5, 1500.0, 1000.0], [1497.0, 0.0, 1500.0, 2.5], [1494.0, 997.5, 1497.0, 1000.0]));
    }

    #[test]
    fn the_hole_order_settings_are_read_from_the_machine_file_with_the_originals_defaults() {
        // CO20: with no file, and with a file that doesn't set them, By
        // proximity from the corner nearest the head, rows, 1 mm, and Cut
        // small holes first off with every limit 0.
        let want = HoleOrder { mode: HoleMode::Proximity, corner: 0, columns: false, ortho_tol: 1.0, small_first: false, big_length: 0.0, big_width: 0.0, big_offset: 0.0, tol: crate::leads::LIN_TOL };
        assert_eq!(MachineTech::default().hole_order, want);
        assert_eq!(MachineTech::from_text("").hole_order, want);
        assert_eq!(MachineTech::from_text("# Demo laser\nB244 7.0\n").hole_order, want);
        // A made-up machine file that sets every one.
        let demo = "# Demo laser\nA005 0.02\nB565 2 ; order mode of the holes\nB566 3\nB567 2\nB568 0.5\nB587 1\nB588 40.0\nB589 30.0\nB590 2.5\n";
        let got = MachineTech::from_text(demo).hole_order;
        assert_eq!(got, HoleOrder { mode: HoleMode::Direction, corner: 3, columns: true, ortho_tol: 0.5, small_first: true, big_length: 40.0, big_width: 30.0, big_offset: 2.5, tol: 0.02 });
        // Each mode by its number, Cut small holes first written as ON too.
        let mode = |v: &str| MachineTech::from_text(&format!("B565 {v}\n")).hole_order.mode;
        assert_eq!([mode("0"), mode("1"), mode("2"), mode("3")], [HoleMode::None, HoleMode::Proximity, HoleMode::Direction, HoleMode::MinimumHeat]);
        assert!(MachineTech::from_text("B587 ON\n").hole_order.small_first);
        assert!(!MachineTech::from_text("B587 OFF\n").hole_order.small_first);
        // A value the setting doesn't have keeps the default (MAC-61).
        let odd = MachineTech::from_text("B565 7\nB566 9\nB567 5\nB568 -2\n").hole_order;
        assert_eq!((odd.mode, odd.corner, odd.columns, odd.ortho_tol), (HoleMode::Proximity, 0, false, 1.0));
        // The machine's own values: a material section doesn't change them.
        let mut text = demo_sections_file();
        text.insert_str(0, "B565 0\n");
        let t = MachineTech::from_text(&text).for_job(&JobKeys::new("0010", 3.0));
        assert_eq!((t.section, t.hole_order.mode), (Some(2), HoleMode::None));
    }

    #[test]
    fn the_installed_blm_file_matches_the_built_in_values() {
        let p = Path::new(r"C:\Lantek\Expert\MchFiles\PstAdi06.CT1");
        if let Some(t) = MachineTech::load(p) {
            let d = MachineTech::default();
            assert_eq!((t.lead_in, t.lead_out, t.overlap, t.joint_gap, t.joint_every, t.joint_lead), (d.lead_in, d.lead_out, d.overlap, d.joint_gap, d.joint_every, d.joint_lead));
        }
    }
}

/// The sheet's automatic order (CZ1–CZ6, CO27 (c)): the cutting zones, the
/// corner the first one is in and the way they are run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetOrder {
    /// B122 / B123: cutting zones along X (across) and Y (up); 0 = one zone across.
    pub zones_x: u32,
    pub zones_y: u32,
    /// B110: the left column first, else the right.
    pub start_left: bool,
    /// The bottom row first, else the top. The machine file's start bottom
    /// (B111) isn't followed: the sheet starts at the bottom (CZ6, CO28).
    pub start_bottom: bool,
    /// B114: rows (along X) one after another, else columns.
    pub horizontal: bool,
    /// B115: every other row or column runs back.
    pub zigzag: bool,
}

impl Default for SheetOrder {
    /// The BLM machine file's values (CZ3), with the bottom start (CZ6).
    fn default() -> Self {
        SheetOrder { zones_x: 20, zones_y: 10, start_left: true, start_bottom: true, horizontal: false, zigzag: false }
    }
}

impl SheetOrder {
    /// The sheet order a machine file's codes give. A code the file doesn't
    /// set keeps the built-in value.
    pub fn from_codes(m: &HashMap<String, String>) -> SheetOrder {
        let d = SheetOrder::default();
        let u = |k: &str, def: u32| m.get(k).and_then(|v| v.parse::<i64>().ok()).map_or(def, |v| v.clamp(0, u32::MAX as i64) as u32);
        let on = |k: &str, def: bool| m.get(k).map_or(def, |v| v == "ON");
        SheetOrder {
            zones_x: u("B122", d.zones_x),
            zones_y: u("B123", d.zones_y),
            start_left: on("B110", d.start_left),
            // CZ6: B111 isn't followed; the sheet starts at the bottom.
            start_bottom: d.start_bottom,
            horizontal: on("B114", d.horizontal),
            zigzag: on("B115", d.zigzag),
        }
    }

    /// The corner the first zone is in, for people and agents.
    pub fn corner_label(&self) -> &'static str {
        match (self.start_left, self.start_bottom) {
            (true, true) => "lower left",
            (false, true) => "lower right",
            (true, false) => "upper left",
            (false, false) => "upper right",
        }
    }
}

/// Most cutting zones a change in Settings → Machines takes along one axis
/// (own limit, MAC-62).
pub const MAX_ZONES: u32 = 500;

/// A machine's sheet and hole order (CO16, CO17).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderSettings {
    pub sheet: SheetOrder,
    pub holes: HoleOrder,
}

impl OrderSettings {
    /// What a machine file's codes give, before any change in Settings.
    pub fn from_codes(m: &HashMap<String, String>) -> OrderSettings {
        OrderSettings { sheet: SheetOrder::from_codes(m), holes: HoleOrder::from_codes(m) }
    }
}

/// The sheet and hole order settings changed for one machine in Settings →
/// Machines (CO16, CO17), saved with the machine. Each value set wins over
/// the machine file's; None = the file's. Empty = every value from the file
/// ("Use the machine file"). Saved values are read one by one (`read`).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OrderChanges {
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub start_left: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub start_bottom: Option<bool>,
    /// Rows (along X), else columns.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub rows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub zigzag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub zones_x: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub zones_y: Option<u32>,
    /// Order mode of the holes by its number in the machine file (B565).
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub hole_mode: Option<u8>,
    /// By direction's start corner (B566: 0 nearest the head, 1–4).
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub hole_start_corner: Option<u8>,
    /// By direction in columns (B567 2), else rows.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub hole_columns: Option<bool>,
    /// B568, mm.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub orthogonality_tolerance: Option<f64>,
    /// B587.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub small_holes_first: Option<bool>,
    /// B588 / B589 / B590, mm.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub big_hole_min_length: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub big_hole_min_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "one_value")]
    pub big_hole_offset: Option<f64>,
}

/// One saved value of `OrderChanges`: None when it can't be read as one
/// (a word for a switch, a negative count), so the others are still read.
fn one_value<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(T::deserialize(v).ok())
}

/// `wanted` when it isn't `file`.
fn changed<T: PartialEq>(file: T, wanted: T) -> Option<T> {
    (file != wanted).then_some(wanted)
}

/// A length `wanted` when it isn't `file` (to a millionth of a mm).
fn changed_len(file: f64, wanted: f64) -> Option<f64> {
    ((file - wanted).abs() > 1e-6).then_some(wanted)
}

impl OrderChanges {
    /// Nothing changed: every value from the machine file.
    pub fn is_empty(&self) -> bool {
        *self == OrderChanges::default()
    }

    /// The changes as saved in the settings file (CO16, CO17): each value
    /// read on its own, and one that can't be read or that the setting
    /// can't take (`valid`) left out, so the machine's other changes are
    /// kept. A line that isn't a list of values at all gives none.
    pub fn read(text: &str) -> OrderChanges {
        serde_json::from_str::<OrderChanges>(text).map(OrderChanges::valid).unwrap_or_default()
    }

    /// Only the changes the settings can take (CO16, CO17): a mode or start
    /// corner number the setting doesn't have, more than `MAX_ZONES` zones,
    /// a negative or endless length or an endless offset is left out, and
    /// the file's value stays. Reading the settings, laying the changes
    /// over the file (`apply`) and saying where a value comes from
    /// (`MachineTech::order_changes`) all go through it.
    pub fn valid(self) -> OrderChanges {
        let len = |v: Option<f64>| v.filter(|x| x.is_finite() && *x >= 0.0);
        OrderChanges {
            zones_x: self.zones_x.filter(|n| *n <= MAX_ZONES),
            zones_y: self.zones_y.filter(|n| *n <= MAX_ZONES),
            hole_mode: self.hole_mode.filter(|c| HoleMode::from_code(*c).is_some()),
            hole_start_corner: self.hole_start_corner.filter(|c| *c <= 4),
            orthogonality_tolerance: len(self.orthogonality_tolerance),
            big_hole_min_length: len(self.big_hole_min_length),
            big_hole_min_width: len(self.big_hole_min_width),
            big_hole_offset: self.big_hole_offset.filter(|x| x.is_finite()),
            ..self
        }
    }

    /// The order settings with these changes laid over `file`'s; a value
    /// the setting can't take (`valid`) leaves the file's.
    pub fn apply(&self, file: OrderSettings) -> OrderSettings {
        let c = self.valid();
        let (mut s, mut h) = (file.sheet, file.holes);
        s.start_left = c.start_left.unwrap_or(s.start_left);
        s.start_bottom = c.start_bottom.unwrap_or(s.start_bottom);
        s.horizontal = c.rows.unwrap_or(s.horizontal);
        s.zigzag = c.zigzag.unwrap_or(s.zigzag);
        s.zones_x = c.zones_x.unwrap_or(s.zones_x);
        s.zones_y = c.zones_y.unwrap_or(s.zones_y);
        h.mode = c.hole_mode.and_then(HoleMode::from_code).unwrap_or(h.mode);
        h.corner = c.hole_start_corner.unwrap_or(h.corner);
        h.columns = c.hole_columns.unwrap_or(h.columns);
        h.ortho_tol = c.orthogonality_tolerance.unwrap_or(h.ortho_tol);
        h.small_first = c.small_holes_first.unwrap_or(h.small_first);
        h.big_length = c.big_hole_min_length.unwrap_or(h.big_length);
        h.big_width = c.big_hole_min_width.unwrap_or(h.big_width);
        h.big_offset = c.big_hole_offset.unwrap_or(h.big_offset);
        OrderSettings { sheet: s, holes: h }
    }

    /// The changes that make `file`'s settings `wanted`: only the values
    /// that differ, so a value set back to the file's follows the file again.
    pub fn between(file: &OrderSettings, wanted: &OrderSettings) -> OrderChanges {
        let (fs, ws, fh, wh) = (&file.sheet, &wanted.sheet, &file.holes, &wanted.holes);
        OrderChanges {
            start_left: changed(fs.start_left, ws.start_left),
            start_bottom: changed(fs.start_bottom, ws.start_bottom),
            rows: changed(fs.horizontal, ws.horizontal),
            zigzag: changed(fs.zigzag, ws.zigzag),
            zones_x: changed(fs.zones_x, ws.zones_x),
            zones_y: changed(fs.zones_y, ws.zones_y),
            hole_mode: changed(fh.mode.code(), wh.mode.code()),
            hole_start_corner: changed(fh.corner, wh.corner),
            hole_columns: changed(fh.columns, wh.columns),
            orthogonality_tolerance: changed_len(fh.ortho_tol, wh.ortho_tol),
            small_holes_first: changed(fh.small_first, wh.small_first),
            big_hole_min_length: changed_len(fh.big_length, wh.big_length),
            big_hole_min_width: changed_len(fh.big_width, wh.big_width),
            big_hole_offset: changed_len(fh.big_offset, wh.big_offset),
        }
    }

    /// The sheet order is changed (start corner, rows or columns, zig-zag, zones).
    pub fn sheet_changed(&self) -> bool {
        self.start_left.is_some() || self.start_bottom.is_some() || self.rows.is_some() || self.zigzag.is_some() || self.zones_x.is_some() || self.zones_y.is_some()
    }
}

/// Cutting zones of a sheet, in the order they are cut (CZ1: zone width = sheet length ÷ X zones, height =
/// sheet width ÷ Y zones, only when the count is above 0). Each zone is
/// (x0, y0, x1, y1). Columns (or rows when `horizontal`) start on the
/// configured side; with `zigzag` every other one runs back.
pub fn zones(w: f64, h: f64, t: &SheetOrder) -> Vec<[f64; 4]> {
    let nx = t.zones_x.max(1) as usize;
    let ny = t.zones_y.max(1) as usize;
    let (zw, zh) = (if t.zones_x > 0 { w / t.zones_x as f64 } else { w }, if t.zones_y > 0 { h / t.zones_y as f64 } else { h });
    let xs: Vec<usize> = if t.start_left { (0..nx).collect() } else { (0..nx).rev().collect() };
    let ys: Vec<usize> = if t.start_bottom { (0..ny).collect() } else { (0..ny).rev().collect() };
    let mut out = vec![];
    let cell = |i: usize, j: usize| [i as f64 * zw, j as f64 * zh, (i + 1) as f64 * zw, (j + 1) as f64 * zh];
    if t.horizontal {
        for (k, &j) in ys.iter().enumerate() {
            let row: Vec<usize> = if t.zigzag && k % 2 == 1 { xs.iter().rev().copied().collect() } else { xs.clone() };
            out.extend(row.into_iter().map(|i| cell(i, j)));
        }
    } else {
        for (k, &i) in xs.iter().enumerate() {
            let col: Vec<usize> = if t.zigzag && k % 2 == 1 { ys.iter().rev().copied().collect() } else { ys.clone() };
            out.extend(col.into_iter().map(|j| cell(i, j)));
        }
    }
    out
}

/// Is a part's box in a zone? (CZ4):
/// its box must overlap the zone, and one of its box corners must lie in it.
pub fn box_in_zone(b: [f64; 4], z: [f64; 4]) -> bool {
    if z[0] > b[2] || b[0] > z[2] || z[1] > b[3] || b[1] > z[3] {
        return false;
    }
    let inside = |x: f64, y: f64| x >= z[0] && x <= z[2] && y >= z[1] && y <= z[3];
    inside(b[0], b[1]) || inside(b[2], b[3]) || inside(b[0], b[3]) || inside(b[2], b[1])
}

/// Order mode of the holes (B565, CO20): how a part's holes are ordered when
/// nobody has ordered them by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleMode {
    /// 0: the part's own contour order, as before the setting (CO14).
    None,
    /// 1: the nearest hole next (CO21), the original's default.
    Proximity,
    /// 2: in rows or columns from a start corner (CO23, CO24).
    Direction,
    /// 3: Minimum heat; a part's holes in its own contour order (CO24), the
    /// sheet's heat order isn't built (MAC-53).
    MinimumHeat,
}

impl HoleMode {
    /// The mode a machine-file number stands for (B565), if any.
    pub fn from_code(code: u8) -> Option<HoleMode> {
        [HoleMode::None, HoleMode::Proximity, HoleMode::Direction, HoleMode::MinimumHeat].into_iter().find(|m| m.code() == code)
    }

    /// The setting's number in the machine file (B565).
    pub fn code(self) -> u8 {
        match self {
            HoleMode::None => 0,
            HoleMode::Proximity => 1,
            HoleMode::Direction => 2,
            HoleMode::MinimumHeat => 3,
        }
    }

    /// For people and agents.
    pub fn label(self) -> &'static str {
        match self {
            HoleMode::None => "none",
            HoleMode::Proximity => "by proximity",
            HoleMode::Direction => "by direction",
            HoleMode::MinimumHeat => "minimum heat",
        }
    }
}

/// How a part's holes are ordered when nobody has ordered them by hand: the
/// machine file's hole order settings (CO20) and the tolerance they are
/// compared within.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoleOrder {
    /// B565: the order mode.
    pub mode: HoleMode,
    /// B566: the start corner of By direction: 0 the corner of the part's
    /// box nearest the head, 1 lower left, 2 lower right, 3 upper right,
    /// 4 upper left (CO23).
    pub corner: u8,
    /// B567: By direction cuts columns (2), else rows (1).
    pub columns: bool,
    /// B568: orthogonality tolerance, mm: holes whose start-side edges are
    /// this close are in one row or column (CO23).
    pub ortho_tol: f64,
    /// B587: Cut small holes first (CO25).
    pub small_first: bool,
    /// B588 / B589: a big hole's minimum length along X and width along Y, mm.
    pub big_length: f64,
    pub big_width: f64,
    /// B590: how far around a big hole a small hole counts as next to it, mm.
    pub big_offset: f64,
    /// A005: the machine's linear tolerance, mm, that By proximity's
    /// distances and the big-hole test are compared within (CO21, CO25:
    /// estimated, MAC-61).
    pub tol: f64,
}

impl Default for HoleOrder {
    /// The original's defaults (CO20).
    fn default() -> Self {
        HoleOrder { mode: HoleMode::Proximity, corner: 0, columns: false, ortho_tol: 1.0, small_first: false, big_length: 0.0, big_width: 0.0, big_offset: 0.0, tol: crate::leads::LIN_TOL }
    }
}

impl HoleOrder {
    /// The part's own contour order, whatever the file says: what every
    /// part was cut in before the setting (CO14).
    pub const NONE: HoleOrder = HoleOrder { mode: HoleMode::None, corner: 0, columns: false, ortho_tol: 1.0, small_first: false, big_length: 0.0, big_width: 0.0, big_offset: 0.0, tol: crate::leads::LIN_TOL };

    /// The settings a machine file's codes give, with the original's
    /// defaults (CO20). A value the setting doesn't have keeps its default
    /// (MAC-61).
    pub fn from_codes(m: &HashMap<String, String>) -> HoleOrder {
        let d = HoleOrder::default();
        let num = |k: &str| m.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        let mode = match num("B565") {
            Some(v) if v == 0.0 => HoleMode::None,
            Some(v) if v == 1.0 => HoleMode::Proximity,
            Some(v) if v == 2.0 => HoleMode::Direction,
            Some(v) if v == 3.0 => HoleMode::MinimumHeat,
            _ => d.mode,
        };
        let corner = num("B566").filter(|v| v.fract() == 0.0 && (0.0..=4.0).contains(v)).map_or(d.corner, |v| v as u8);
        let columns = num("B567").map_or(d.columns, |v| v == 2.0);
        let ortho_tol = num("B568").filter(|v| *v >= 0.0).unwrap_or(d.ortho_tol);
        let small_first = m.get("B587").map_or(d.small_first, |v| v.trim() == "ON" || v.trim().parse::<f64>().is_ok_and(|x| x != 0.0));
        let len = |k: &str, def: f64| num(k).filter(|v| *v >= 0.0).unwrap_or(def);
        HoleOrder {
            mode,
            corner,
            columns,
            ortho_tol,
            small_first,
            big_length: len("B588", d.big_length),
            big_width: len("B589", d.big_width),
            big_offset: num("B590").unwrap_or(d.big_offset),
            tol: num("A005").filter(|v| *v > 0.0).unwrap_or(d.tol),
        }
    }
}
