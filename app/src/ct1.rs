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
    /// B122 / B123: cutting zones along X and Y (the cut order goes zone by zone).
    pub zones_x: u32,
    pub zones_y: u32,
    /// B110: start at the left (cutting); start at the bottom, as the real
    /// BLM program does (B111 not followed, see `from_text`).
    pub start_left: bool,
    pub start_bottom: bool,
    /// B114 / B115: run along X first (else along Y), and zig-zag.
    pub horizontal: bool,
    pub zigzag: bool,
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
            zones_x: 20,
            zones_y: 10,
            start_left: true,
            start_bottom: true,
            horizontal: false,
            zigzag: false,
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

/// Set the machine-file key of each app material: what the user typed in
/// Settings → Materials (normalised by `material_key`), or the material's
/// name in programs when that is empty (MTL2: an empty internal reference
/// keeps the material's name).
pub fn set_material_keys(refs: &[String], names: &[String]) {
    let keys: Vec<String> = (0..refs.len().max(names.len()))
        .map(|i| {
            let r = refs.get(i).map(|s| material_key(s)).unwrap_or_default();
            if r.is_empty() { names.get(i).map(|n| n.trim().to_string()).unwrap_or_default() } else { r }
        })
        .collect();
    if let Ok(mut g) = KEYS.write() {
        if g.as_ref() != Some(&keys) {
            *g = Some(keys);
            GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// The machine-file key of app material `i` (index into `cnc::MATERIALS`).
pub fn key_of_material(i: usize) -> String {
    if let Some(k) = KEYS.read().ok().and_then(|g| g.as_ref().and_then(|v| v.get(i).cloned())) {
        return k;
    }
    crate::settings::default_material_refs().get(i).map(|r| material_key(r)).unwrap_or_default()
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
                t
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
            zones_x: u("B122", d.zones_x),
            zones_y: u("B123", d.zones_y),
            start_left: m.get("B110").map_or(d.start_left, |v| v == "ON"),
            // B111 is OFF in PstAdi06.CT1, yet the real program 1234X38THXSAMPLE
            // cuts from the bottom up (ORIG Y 10 → 1091.6): the bottom start is kept.
            start_bottom: d.start_bottom,
            horizontal: m.get("B114").map_or(d.horizontal, |v| v == "ON"),
            zigzag: m.get("B115").map_or(d.zigzag, |v| v == "ON"),
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
        t
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
        let t = MachineTech::default();
        let z = zones(2000.0, 1000.0, &t);
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
        let zz = zones(200.0, 200.0, &MachineTech { zones_x: 2, zones_y: 2, zigzag: true, ..MachineTech::default() });
        assert_eq!(zz, vec![[0.0, 0.0, 100.0, 100.0], [0.0, 100.0, 100.0, 200.0], [100.0, 100.0, 200.0, 200.0], [100.0, 0.0, 200.0, 100.0]]);
    }

    #[test]
    fn a_column_of_parts_is_cut_bottom_up_like_the_real_program() {
        use crate::nest::{Part, Placed, Shape, SheetSpec};
        // 1234X38THXSAMPLE: ORIG(10,10) (10,93.2) … (10,1091.6), in that order.
        let parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 1180.0, h: 73.2, qty: 14, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        let mut placed: Vec<Placed> = (0..14).map(|k| Placed { part: 0, x: 10.0, y: 10.0 + 83.2 * k as f64, w: 1180.0, h: 73.2, turns: 0, mirror: false }).collect();
        placed.reverse();
        let spec = SheetSpec { w: 1200.15, h: 1219.2, margin: 10.0, gap: 10.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let plan = crate::cnc::plan(&parts, &placed, &spec);
        let ys: Vec<f64> = plan.contours.iter().map(|c| placed[c.part_no - 1].y).collect();
        assert!(ys.windows(2).all(|w| w[0] <= w[1]), "{ys:?}");
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

/// Cutting zones of a sheet, in the order they are cut (CZ1: zone width = sheet length ÷ X zones, height =
/// sheet width ÷ Y zones, only when the count is above 0). Each zone is
/// (x0, y0, x1, y1). Columns (or rows when `horizontal`) start on the
/// configured side; with `zigzag` every other one runs back.
pub fn zones(w: f64, h: f64, t: &MachineTech) -> Vec<[f64; 4]> {
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
