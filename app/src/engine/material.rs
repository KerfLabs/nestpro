//! The materials, by id: the one place that knows a material's name, weight
//! per volume, name and code in machine programs, machine-file key and other
//! names. Everything else asks here by material id.
//!
//! Today the list is the app's first three materials (0 mild steel,
//! 1 stainless steel, 2 aluminium) with what this PC's Settings → Materials
//! says about them. Files store the id, so a job, part or stock item saved
//! before keeps its material (spec #142, invariant 1).

use crate::settings::AppSettings;

/// A material's id, as jobs, parts and stock items store it.
pub type Id = usize;

/// The kind of metal a material is: it sets the stock pictures' look and the
/// estimated cutting speeds used when the machine file gives none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    MildSteel,
    Stainless,
    Aluminium,
}

/// One of the app's first three materials and its starting values.
struct Builtin {
    name: &'static str,
    kind: Kind,
    /// Weight per volume, g/cm³.
    density: f64,
    /// Name and code in machine programs (BLM codes 0010, 0100, 0200).
    program: (&'static str, &'static str),
    /// Key in the machine file's material × thickness sections (MTL2): mild
    /// steel 0010, stainless 0100, aluminium 0210 (the aluminium-magnesium
    /// grades its other names list; pure aluminium is 0200 — our choice).
    machine_ref: &'static str,
    /// Other names it goes by (MA6; common grades and shop names, estimated).
    other_names: &'static str,
    /// Short code in made-up sheet references ("ST3-2000x1000").
    short_code: &'static str,
}

const OLD_THREE: [Builtin; 3] = [
    Builtin { name: "Mild steel", kind: Kind::MildSteel, density: 7.85, program: ("STEEL", "0010"), machine_ref: "0010", other_names: "Mild steel, S235, S235JR, S275, S355, DC01, CR, HR, MS", short_code: "ST" },
    Builtin { name: "Stainless steel", kind: Kind::Stainless, density: 8.0, program: ("STAINLESS", "0100"), machine_ref: "0100", other_names: "Stainless, 304, 304L, 316, 316L, AISI304, AISI316, 1.4301, INOX, SS", short_code: "SS" },
    Builtin { name: "Aluminium", kind: Kind::Aluminium, density: 2.70, program: ("ALUMINIUM", "0200"), machine_ref: "0210", other_names: "Aluminum, AL, 5052, 5754, 6061, AlMg3", short_code: "AL" },
];

fn builtin(id: Id) -> Option<&'static Builtin> {
    OLD_THREE.get(id)
}

/// JOB-86 (estimated): an id past the list (a hand-edited or newer file)
/// keeps the values it always had where one was always given: those of the
/// last material.
fn nearest(id: Id) -> &'static Builtin {
    &OLD_THREE[id.min(OLD_THREE.len() - 1)]
}

/// Every material's id, in list order.
pub fn ids() -> Vec<Id> {
    (0..OLD_THREE.len()).collect()
}

/// How many materials the list has.
pub fn count() -> usize {
    OLD_THREE.len()
}

pub fn exists(id: Id) -> bool {
    builtin(id).is_some()
}

/// The material's name as the app shows it ("Stainless steel").
pub fn name(id: Id) -> Option<String> {
    builtin(id).map(|b| b.name.to_string())
}

/// The material's name, or `fallback` for an id that isn't in the list.
pub fn name_or(id: Id, fallback: &str) -> String {
    name(id).unwrap_or_else(|| fallback.to_string())
}

/// The material's name; an id past the list gives the last material's (as
/// the program header, the stock window and the import notes always did).
pub fn nearest_name(id: Id) -> String {
    nearest(id).name.to_string()
}

/// Every material's name, in list order.
pub fn names() -> Vec<String> {
    OLD_THREE.iter().map(|b| b.name.to_string()).collect()
}

pub fn kind(id: Id) -> Kind {
    nearest(id).kind
}

/// Short code for a sheet reference made up from material, thickness and
/// size; an id past the list gives the last material's.
pub fn short_code(id: Id) -> &'static str {
    nearest(id).short_code
}

/// Weight per volume, g/cm³.
pub fn density(id: Id) -> f64 {
    nearest(id).density
}

/// The material's name and code in machine programs before Settings say
/// otherwise; an id that isn't in the list is written as mild steel.
pub fn default_program(id: Id) -> (String, String) {
    let (n, c) = builtin(id).unwrap_or(&OLD_THREE[0]).program;
    (n.to_string(), c.to_string())
}

/// The material's name and code in machine programs (Settings → Materials).
pub fn program(cfg: &AppSettings, id: Id) -> (String, String) {
    cfg.materials.get(id).cloned().unwrap_or_else(|| default_program(id))
}

/// The name in programs, or the material's own name when that is empty (the
/// material text on remnant marks and stock references).
pub fn program_name_or_name(cfg: &AppSettings, id: Id) -> String {
    cfg.materials.get(id).map(|x| x.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| nearest_name(id))
}

/// The machine-file key a material has before Settings say otherwise.
pub fn default_machine_key(id: Id) -> String {
    builtin(id).map(|b| crate::ct1::material_key(b.machine_ref)).unwrap_or_default()
}

/// Each material's machine-file key, by id: what the user typed in Settings →
/// Materials (normalised), or the material's name in programs when that is
/// empty (MTL2: an empty internal reference keeps the material's name).
pub fn machine_keys(cfg: &AppSettings) -> Vec<String> {
    (0..cfg.material_refs.len().max(cfg.materials.len()))
        .map(|i| {
            let r = cfg.material_refs.get(i).map(|s| crate::ct1::material_key(s)).unwrap_or_default();
            if r.is_empty() { cfg.materials.get(i).map(|n| n.0.trim().to_string()).unwrap_or_default() } else { r }
        })
        .collect()
}

fn other_names(cfg: &AppSettings, id: Id) -> impl Iterator<Item = String> + '_ {
    cfg.material_aliases.get(id).into_iter().flat_map(|a| a.split(',').map(|x| x.trim().to_lowercase()))
}

/// The material a name from outside means (an import, a program, a file
/// name): the material's own name, its name in programs, its code or one of
/// its other names, without case. As in the original (MA6), another name
/// gives its main material; an unknown name gives None.
pub fn find(cfg: &AppSettings, text: &str) -> Option<Id> {
    let n = text.trim().to_lowercase();
    if n.is_empty() {
        return None;
    }
    ids().into_iter().find(|&i| {
        name(i).is_some_and(|m| m.to_lowercase() == n)
            || cfg.materials.get(i).is_some_and(|(p, c)| p.to_lowercase() == n || c.to_lowercase() == n)
            || other_names(cfg, i).any(|x| x == n)
    })
}

/// A look-alike: an exact name or code first, then a name that contains the
/// text or the other way round (as the XML import reads its files).
pub fn look_alike(cfg: &AppSettings, text: &str) -> Option<Id> {
    crate::xmlimport::material_index(text, &names(), &cfg.materials)
}

/// The material a material text names: `find`; then a grade that starts with
/// another name holding a digit ("S355J2" → the material called S355); then
/// a look-alike.
pub fn like(cfg: &AppSettings, text: &str) -> Option<Id> {
    let t = text.trim().to_lowercase();
    find(cfg, text)
        .or_else(|| ids().into_iter().find(|&i| other_names(cfg, i).any(|x| x.len() >= 3 && x.chars().any(|c| c.is_ascii_digit()) && t.starts_with(&x))))
        .or_else(|| look_alike(cfg, text))
}

// This PC's Settings → Materials (the settings file's material lines).

/// Names and codes in programs, by id, for a new install.
pub fn default_pc_programs() -> Vec<(String, String)> {
    ids().into_iter().map(default_program).collect()
}

/// Other names, by id, for a new install.
pub fn default_pc_other_names() -> Vec<String> {
    OLD_THREE.iter().map(|b| b.other_names.to_string()).collect()
}

/// Machine-file keys as typed, by id, for a new install.
pub fn default_pc_machine_refs() -> Vec<String> {
    OLD_THREE.iter().map(|b| b.machine_ref.to_string()).collect()
}

/// Put Settings → Materials back as a new install has it.
pub fn reset_pc(cfg: &mut AppSettings) {
    cfg.materials = default_pc_programs();
    cfg.material_aliases = default_pc_other_names();
    cfg.material_refs = default_pc_machine_refs();
}

/// One material's row of Settings → Materials, to edit.
pub struct PcRow<'a> {
    pub program_name: &'a mut String,
    pub program_code: &'a mut String,
    pub machine_ref: &'a mut String,
    pub other_names: &'a mut String,
}

/// The row of material `id` (a row the settings file lacks gets the
/// defaults first).
pub fn pc_row(cfg: &mut AppSettings, id: Id) -> PcRow<'_> {
    while cfg.materials.len() <= id {
        cfg.materials.push(default_program(cfg.materials.len()));
    }
    while cfg.material_aliases.len() <= id {
        cfg.material_aliases.push(builtin(cfg.material_aliases.len()).map(|b| b.other_names.to_string()).unwrap_or_default());
    }
    while cfg.material_refs.len() <= id {
        cfg.material_refs.push(builtin(cfg.material_refs.len()).map(|b| b.machine_ref.to_string()).unwrap_or_default());
    }
    let (n, c) = &mut cfg.materials[id];
    PcRow { program_name: n, program_code: c, machine_ref: &mut cfg.material_refs[id], other_names: &mut cfg.material_aliases[id] }
}

/// The settings file's material lines.
pub fn write_pc(cfg: &AppSettings, s: &mut String, esc: impl Fn(&str) -> String) {
    for (k, (n, id)) in cfg.materials.iter().enumerate() {
        s.push_str(&format!("material{k} = {}|{}\n", esc(n), esc(id)));
    }
    for (k, a) in cfg.material_aliases.iter().enumerate() {
        s.push_str(&format!("material_alias{k} = {}\n", esc(a)));
    }
    for (k, r) in cfg.material_refs.iter().enumerate() {
        s.push_str(&format!("material_ref{k} = {}\n", esc(r)));
    }
}

/// Read one `[general]` line of the settings file if it is a material line
/// (a line for a material past the list is ignored). False when it isn't one.
pub fn read_pc(cfg: &mut AppSettings, k: &str, v: String) -> bool {
    if let Some(i) = k.strip_prefix("material_ref") {
        if let Some(r) = i.parse::<usize>().ok().and_then(|i| cfg.material_refs.get_mut(i)) {
            *r = v;
        }
    } else if let Some(i) = k.strip_prefix("material_alias") {
        if let Some(a) = i.parse::<usize>().ok().and_then(|i| cfg.material_aliases.get_mut(i)) {
            *a = v;
        }
    } else if let Some(i) = k.strip_prefix("material") {
        if let (Some(m), Some((n, id))) = (i.parse::<usize>().ok().and_then(|i| cfg.materials.get_mut(i)), v.split_once('|')) {
            *m = (n.to_string(), id.to_string());
        }
    } else {
        return false;
    }
    true
}

/// What this PC's material settings are, as text that changes when they do
/// (to know when an import's preview must be read again).
pub fn fingerprint(cfg: &AppSettings) -> String {
    format!("{:?}|{:?}", cfg.materials, cfg.material_aliases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_three_answer_by_id() {
        assert_eq!(ids(), vec![0, 1, 2]);
        assert_eq!(names(), vec!["Mild steel", "Stainless steel", "Aluminium"]);
        assert_eq!(name(1).as_deref(), Some("Stainless steel"));
        assert_eq!((density(0), density(1), density(2)), (7.85, 8.0, 2.70));
        assert_eq!((kind(0), kind(1), kind(2)), (Kind::MildSteel, Kind::Stainless, Kind::Aluminium));
        assert_eq!(default_program(0), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(default_program(2), ("ALUMINIUM".to_string(), "0200".to_string()));
        assert_eq!(default_machine_key(2), "##MATEXP0210##");
        assert_eq!((short_code(0), short_code(1), short_code(2)), ("ST", "SS", "AL"));
    }

    #[test]
    fn an_id_past_the_list_has_no_name_and_keeps_each_old_fallback() {
        // JOB-86 (estimated): no name; where a value was always given, the
        // same value as before (weights and cutting estimates of the last
        // material, programs as mild steel, no machine-file key).
        assert!(!exists(3) && exists(2));
        assert_eq!(name(7), None);
        assert_eq!(name_or(7, "?"), "?");
        assert_eq!(nearest_name(7), "Aluminium");
        assert_eq!((density(7), kind(7)), (2.70, Kind::Aluminium));
        assert_eq!(short_code(7), "AL");
        assert_eq!(default_program(7), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(program(&AppSettings::default(), 7), ("STEEL".to_string(), "0010".to_string()));
        assert_eq!(default_machine_key(7), "");
        assert_eq!(program_name_or_name(&AppSettings::default(), 7), "Aluminium");
    }

    #[test]
    fn before_settings_exist_the_defaults_answer() {
        let cfg = AppSettings::parse("");
        assert_eq!(program(&cfg, 1), ("STAINLESS".to_string(), "0100".to_string()));
        assert_eq!(machine_keys(&cfg), vec!["##MATEXP0010##", "##MATEXP0100##", "##MATEXP0210##"]);
        assert_eq!(find(&cfg, "aisi316"), Some(1));
        assert_eq!(find(&cfg, "STEEL"), Some(0), "the name in programs");
        assert_eq!(program_name_or_name(&cfg, 2), "ALUMINIUM");
    }

    #[test]
    fn renamed_materials_and_extra_other_names_come_from_this_pcs_settings() {
        let cfg = AppSettings::parse("[general]\nmaterial1 = DEMO-INOX|0101\nmaterial_alias1 = DEMO-316, DEMO-A2\nmaterial_ref1 = DEMO\nmaterial2 = |0200\nmaterial_ref2 =\n");
        assert_eq!(program(&cfg, 1), ("DEMO-INOX".to_string(), "0101".to_string()));
        assert_eq!((find(&cfg, " demo-a2 "), find(&cfg, "0101"), find(&cfg, "AISI316")), (Some(1), Some(1), None));
        assert_eq!(name(1).as_deref(), Some("Stainless steel"), "the name shown stays the app's");
        // An empty machine-file key is the name in programs (MTL2); an empty
        // name in programs gives the material's own name on remnant marks.
        assert_eq!(machine_keys(&cfg), vec!["##MATEXP0010##", "DEMO", ""]);
        assert_eq!(program_name_or_name(&cfg, 2), "Aluminium");
        // A grade that starts with another name holding a digit, then a look-alike.
        assert_eq!(like(&cfg, "DEMO-316L"), Some(1));
        assert_eq!(like(&cfg, "aluminium 5754"), Some(2));
        assert_eq!(like(&cfg, "DEMO-COPPER"), None);
    }

    #[test]
    fn this_pcs_rows_round_trip_and_reset() {
        let mut cfg = AppSettings::default();
        let row = pc_row(&mut cfg, 0);
        *row.program_name = "DEMO-MS".into();
        row.other_names.push_str(", DEMO-A36");
        let back = AppSettings::parse(&cfg.to_text());
        assert_eq!(program(&back, 0).0, "DEMO-MS");
        assert_eq!(find(&back, "demo-a36"), Some(0));
        assert_eq!(fingerprint(&back), fingerprint(&cfg));
        reset_pc(&mut cfg);
        assert_eq!(program(&cfg, 0).0, "STEEL");
        assert_eq!(find(&cfg, "demo-a36"), None);
    }
}
