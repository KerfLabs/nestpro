//! The machine's laser technology table (the `.TT1` file next to its machine
//! files; MECHANICS "Machining time and speeds", TT1–TT10): one record per
//! material, thickness range and gas (lens, power, bevel angle and an alias
//! can narrow it too), holding the cutting speed and pierce time of each of
//! the 12 cut qualities and one marking speed. The time file (`cuttime`)
//! says whether times use the table or the machine's own default speeds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cuttime::{TimeSettings, NO_LIMIT};

/// One record of the table (TT2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    /// A000: material key, as the machine file compares it (MTL2).
    pub material: String,
    /// A001 / A002: thickness from / to, mm.
    pub t_from: f64,
    pub t_to: f64,
    /// A003 lens, A004 power, A005 gas number (0 = any).
    pub lens: f64,
    pub power: f64,
    pub gas: u32,
    /// A006 / A007: bevel angle from / to.
    pub angle_from: f64,
    pub angle_to: f64,
    /// A008 = 1: the default record for its material and thickness.
    pub default: bool,
    /// A009: configuration alias ("" = none).
    pub alias: String,
    /// Every B/C/D line of the record, by code.
    pub codes: HashMap<String, f64>,
}

impl Record {
    fn num(&self, code: &str) -> Option<f64> {
        self.codes.get(code).copied()
    }

    /// D(50·(q−1)+n): field `n` of quality `q` (TT3).
    fn qfield(&self, q: usize, n: usize) -> Option<f64> {
        (1..=12).contains(&q).then(|| self.num(&format!("D{:03}", 50 * (q - 1) + n))).flatten()
    }

    /// Cutting speed of quality `q`, mm/min (TT6); None when the record has
    /// none (the original then cuts in no time: our caller falls back).
    pub fn cut_speed(&self, q: usize) -> Option<f64> {
        self.qfield(q, 7).filter(|&v| v > 0.001)
    }

    /// The post's pierce mode for quality `q` (BL22): field 26 of the
    /// quality's block (1 smooth, 2 fast, 4 booster); None when the record
    /// has none.
    pub fn pierce_mode(&self, q: usize) -> Option<u32> {
        self.qfield(q, 26).filter(|&v| v >= 0.0).map(|v| v.round() as u32)
    }

    /// Marking speed, mm/min: C009, the same for every quality (TT6).
    pub fn mark_speed(&self) -> Option<f64> {
        self.num("C009").filter(|&v| v > 0.001)
    }

    /// Pierce time of quality `q`, s (TT5): its pierce time, the second pierce
    /// term when the machine shows it, and the time of each pierce type the
    /// quality uses (the decimal digits of its pierce-type field, ten-thousands
    /// = type 1 … units = type 5) when the machine shows pierce types.
    pub fn pierce_secs(&self, q: usize, fields: &Fields) -> f64 {
        let q = if (1..=12).contains(&q) { q } else { 1 };
        let mut t = self.qfield(q, 10).unwrap_or(0.0);
        if fields.pierce_second {
            t += self.qfield(q, 11).unwrap_or(0.0);
        }
        if fields.pierce_types {
            let types = self.qfield(q, 15).unwrap_or(0.0).max(0.0) as u64;
            for k in 1..=5u32 {
                if (types / 10u64.pow(5 - k)) % 10 != 0 {
                    t += self.num(&format!("B{:03}", 100 * (k - 1) + 1)).unwrap_or(0.0);
                }
            }
        }
        t
    }

    /// "stainless 2.7–3.2 mm, gas 1" for people.
    pub fn label(&self) -> String {
        let gas = if self.gas == 0 { String::new() } else { format!(", gas {}", self.gas) };
        format!("{} {}–{} mm{gas}{}", crate::ct1::material_label(&self.material), self.t_from, self.t_to, if self.default { " (default)" } else { "" })
    }
}

/// Which optional fields the machine shows (its CF file's `Tnnn "…" ON|OFF`
/// lines, TT4). Without a CF file every field counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fields {
    /// T412: the second pierce-time term.
    pub pierce_second: bool,
    /// T408: pierce types (and their times in the B block).
    pub pierce_types: bool,
}

impl Default for Fields {
    fn default() -> Self {
        Fields { pierce_second: true, pierce_types: true }
    }
}

/// What a job brings to the record choice (TT9). Our jobs carry a gas (the
/// machine's) but no lens, power, angle or alias: those accept any record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys {
    pub material: String,
    pub thickness: f64,
    /// 0 = no gas given.
    pub gas: u32,
}

/// How the record was chosen (TT9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Every key fits. (The original's second choice, every key but the
    /// configuration alias, never arises: our jobs carry no alias.)
    Exact,
    /// The machine's default record for the material and thickness (the last one).
    Default,
}

/// The whole table: the laser block's records in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub records: Vec<Record>,
    pub source: Option<PathBuf>,
}

impl Table {
    /// Read the table's text (TT2): `W000` opens a machine-type block (3 =
    /// laser; other blocks are skipped); each `A000` starts a record that runs
    /// to the next one.
    pub fn from_text(text: &str) -> Table {
        let mut t = Table::default();
        let mut laser = true;
        let mut cur: Option<Record> = None;
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('#') {
                continue;
            }
            let mut it = line.splitn(2, char::is_whitespace);
            let (Some(code), Some(val)) = (it.next(), it.next()) else { continue };
            let b = code.as_bytes();
            if b.len() != 4 || !b[0].is_ascii_uppercase() || !b[1..].iter().all(u8::is_ascii_digit) {
                continue;
            }
            let val = val.trim();
            let num = val.split_whitespace().next().and_then(|v| v.parse::<f64>().ok());
            if code == "W000" {
                if let Some(r) = cur.take() {
                    t.records.push(r);
                }
                laser = num.is_none_or(|v| v as i64 == 3);
                continue;
            }
            if !laser {
                continue;
            }
            if code == "A000" {
                if let Some(r) = cur.take() {
                    t.records.push(r);
                }
                cur = Some(Record { material: crate::ct1::material_key(val), ..Default::default() });
                continue;
            }
            let Some(r) = cur.as_mut() else { continue };
            let n = num.unwrap_or(0.0);
            match code {
                // A001 also sets "to" (TT2).
                "A001" => {
                    r.t_from = n;
                    r.t_to = n;
                }
                "A002" => r.t_to = n,
                "A003" => r.lens = n,
                "A004" => r.power = n,
                "A005" => r.gas = n.max(0.0) as u32,
                "A006" => {
                    r.angle_from = n;
                    r.angle_to = n;
                }
                "A007" => r.angle_to = n,
                "A008" => r.default = n as i64 == 1,
                "A009" => r.alias = val.trim_matches('"').trim().to_string(),
                _ if matches!(b[0], b'B' | b'C' | b'D') => {
                    if let Some(v) = num {
                        r.codes.insert(code.to_string(), v);
                    }
                }
                _ => {}
            }
        }
        if let Some(r) = cur.take() {
            t.records.push(r);
        }
        t
    }

    pub fn load(path: &Path) -> Option<Table> {
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut t = Table::from_text(&text);
        t.source = Some(path.to_path_buf());
        Some(t)
    }

    /// The record for a job (TT9): the first that fits every key; else the
    /// last default record
    /// for the material and thickness; else none. Thickness ranges are
    /// inclusive with 0.005 mm slack and never stretched to the nearest one.
    pub fn pick(&self, k: &Keys) -> Option<(usize, Pick)> {
        let mut default = None;
        for (i, r) in self.records.iter().enumerate() {
            let base = (r.material == k.material || r.material == "*") && crate::ct1::thickness_fits(r.t_from, r.t_to, k.thickness);
            if !base {
                continue;
            }
            // A record gas of 0 takes any job. Our jobs carry no lens, power,
            // angle or alias, which fit every record (our choice, like TEC-10),
            // so a record that fits the gas fits fully.
            if r.gas == 0 || r.gas == k.gas {
                return Some((i, Pick::Exact));
            }
            if r.default {
                default = Some(i);
            }
        }
        default.map(|i| (i, Pick::Default))
    }
}

/// The cut quality of a technology index (TT8): 1 → 2, 2 → 3, 10–12 → 4–6,
/// 100–102 → 7–9, 110–112 → 10–12, anything else → 1.
pub fn quality_of_index(i: i32) -> usize {
    match i {
        1 => 2,
        2 => 3,
        10..=12 => (i - 6) as usize,
        100..=102 => (i - 93) as usize,
        110..=112 => (i - 100) as usize,
        _ => 1,
    }
}

/// Field flags from a CF file's `Tnnn "label" ON|OFF value` lines (TT4).
pub fn fields_from_cf(text: &str) -> Fields {
    let mut f = Fields::default();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("");
        let mut it = line.split_whitespace();
        let Some(code) = it.next() else { continue };
        if !matches!(code, "T408" | "T412") {
            continue;
        }
        // The flag is the first ON/OFF word after the quoted label.
        let rest = line.splitn(2, char::is_whitespace).nth(1).unwrap_or("");
        let after = rest.rsplit('"').next().unwrap_or(rest);
        let on = after.split_whitespace().next().map(|w| w.eq_ignore_ascii_case("ON"));
        if let Some(on) = on {
            match code {
                "T408" => f.pierce_types = on,
                _ => f.pierce_second = on,
            }
        }
    }
    f
}

/// The machine's speed files: its time settings, its technology table and
/// which optional fields its CF file shows (TT1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpeedFiles {
    pub time: TimeSettings,
    pub table: Option<Table>,
    pub fields: Fields,
    /// The gas the machine cuts with (Settings → Machines), for the record choice.
    pub gas: u32,
}

/// A file named in a CF file (`A019 "PSTADI02.tt1"`), looked for beside the
/// CF file and then beside the technology file; `"xxxxxxxx.xxx"` = none (TT1).
fn named_in_cf(cf: &HashMap<String, String>, code: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let name = cf.get(code)?.trim().trim_matches('"').trim().to_string();
    if name.is_empty() || name.eq_ignore_ascii_case("xxxxxxxx.xxx") {
        return None;
    }
    // Only the file name counts (the original's names are relative).
    let base = name.rsplit(['\\', '/']).next().unwrap_or(&name).to_string();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().eq_ignore_ascii_case(&base) {
                    return Some(e.path());
                }
            }
        }
    }
    None
}

/// The file beside `ct1` with the same name and extension `TT` / `TM` plus
/// the technology file's own suffix (`PSTADI02.CT1` → `PSTADI02.TT1`).
fn beside(ct1: &Path, two: &str) -> Option<PathBuf> {
    let ext = ct1.extension()?.to_string_lossy().to_string();
    let suffix = ext.get(2..).unwrap_or("1");
    let stem = ct1.file_stem()?.to_string_lossy().to_string();
    let want = format!("{stem}.{two}{suffix}");
    let dir = ct1.parent()?;
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(&want)).map(|e| e.path())
}

impl SpeedFiles {
    /// The speed files of a machine whose technology file is `ct1` and whose
    /// CF file (post library) is `cf`: the names in the CF file (A019 table,
    /// A026 time settings), else the technology file's name with `.TT`/`.TM`.
    pub fn find(ct1: Option<&Path>, cf: Option<&Path>, gas: u32) -> SpeedFiles {
        let cf_text = cf.and_then(|p| std::fs::read(p).ok()).map(|b| b.iter().map(|&c| c as char).collect::<String>());
        let cf_codes = cf_text.as_deref().map(crate::ct1::parse).unwrap_or_default();
        let mut dirs: Vec<PathBuf> = vec![];
        for p in [cf, ct1].into_iter().flatten() {
            if let Some(d) = p.parent() {
                dirs.push(d.to_path_buf());
            }
        }
        let tt = named_in_cf(&cf_codes, "A019", &dirs).or_else(|| if cf_codes.contains_key("A019") { None } else { ct1.and_then(|c| beside(c, "TT")) });
        let tm = named_in_cf(&cf_codes, "A026", &dirs).or_else(|| if cf_codes.contains_key("A026") { None } else { ct1.and_then(|c| beside(c, "TM")) });
        SpeedFiles {
            time: tm.and_then(|p| TimeSettings::load(&p)).unwrap_or_default(),
            table: tt.and_then(|p| Table::load(&p)),
            fields: cf_text.as_deref().map(fields_from_cf).unwrap_or_default(),
            gas,
        }
    }
}

/// Where a sheet's speeds came from, for the window and agents.
#[derive(Clone, Debug, PartialEq)]
pub enum SpeedSource {
    /// The technology table's record (how it was picked).
    Table(String, Pick),
    /// The machine's default cutting speed (A058, or its section's Z003).
    Machine,
    /// No machine speed at all: the app's typical laser speeds (estimate).
    Estimate,
}

/// A sheet's speeds and pierce times, resolved once (CTM8–CTM10, TT5, TT6).
#[derive(Clone, Debug, PartialEq)]
pub struct Speeds {
    /// Cutting speed by quality 1–12 (index 0 unused), mm/min.
    pub cut: [f64; 13],
    pub mark: f64,
    pub rapid: f64,
    /// Pierce time by quality 1–12, s.
    pub pierce: [f64; 13],
    /// The post's pierce mode by quality 1–12 from the picked record, used
    /// by the LC5 whatever the time file says (BL22).
    pub pierce_mode: [Option<u32>; 13],
    pub source: SpeedSource,
    /// Some qualities had no table speed and took the fallback (our choice, FEATURES MAC-20).
    pub fell_back: bool,
}

/// Rapid speed when the machine file gives none (A051), mm/min (estimate).
pub const RAPID_DEFAULT: f64 = 60000.0;

impl Speeds {
    /// `active`: the machine's own cutting speed (A058 / Z003) or, when it has
    /// none, our estimate for the material and thickness. `active_mark`: its
    /// marking speed (A063), `rapid`: its rapid speed (A051).
    pub fn resolve(files: &SpeedFiles, keys: &Keys, active: Option<f64>, estimate: f64, active_mark: Option<f64>, rapid: Option<f64>) -> Speeds {
        use crate::cuttime::SpeedSource as Src;
        let ts = &files.time;
        // A machine speed of 0 is no speed (like an empty table field, TT6).
        let active = active.filter(|&v| v > 0.001);
        let active_mark = active_mark.filter(|&v| v > 0.001);
        let picked = files.table.as_ref().and_then(|t| t.pick(keys).map(|(i, how)| (&t.records[i], how)));
        let fallback = active.unwrap_or(estimate);
        let mut cut = [fallback; 13];
        let mut pierce = [ts.pierce_secs(keys.thickness, None); 13];
        let mut pierce_mode = [None; 13];
        let mut fell_back = false;
        let use_table = matches!(ts.cut_source, Src::Table | Src::TableReduced);
        if let Some((r, _)) = picked {
            for q in 1..=12 {
                if use_table {
                    match r.cut_speed(q) {
                        Some(v) => cut[q] = v,
                        None => fell_back |= q <= 3,
                    }
                }
                pierce[q] = ts.pierce_secs(keys.thickness, Some(r.pierce_secs(q, &files.fields)));
                pierce_mode[q] = r.pierce_mode(q);
            }
        } else if use_table {
            fell_back = true;
        }
        let mark_fallback = active_mark.unwrap_or(fallback * crate::cnc::MARK_SPEED_FACTOR);
        let mark = match (ts.mark_source, picked) {
            (Src::Active, _) => mark_fallback,
            (_, Some((r, _))) => r.mark_speed().unwrap_or(mark_fallback),
            _ => mark_fallback,
        };
        let source = match picked {
            Some((r, how)) if use_table => SpeedSource::Table(r.label(), how),
            _ if active.is_some() => SpeedSource::Machine,
            _ => SpeedSource::Estimate,
        };
        Speeds { cut, mark, rapid: rapid.filter(|&v| v > 0.001).unwrap_or(RAPID_DEFAULT), pierce, pierce_mode, source, fell_back }
    }

    pub fn cut_for(&self, q: usize) -> f64 {
        self.cut.get(q).copied().filter(|_| q >= 1).unwrap_or(self.cut[1]).min(NO_LIMIT)
    }

    pub fn pierce_for(&self, q: usize) -> f64 {
        self.pierce.get(q).copied().filter(|_| q >= 1).unwrap_or(self.pierce[1])
    }

    /// The table's pierce mode for quality `q` (BL22); None without one.
    pub fn pierce_mode(&self, q: usize) -> Option<u32> {
        self.pierce_mode.get(q).copied().flatten().filter(|_| q >= 1)
    }

    /// One line for the window and agents.
    pub fn label(&self) -> String {
        let fb = if self.fell_back { " (qualities without a table speed use the machine's default speed)" } else { "" };
        match &self.source {
            SpeedSource::Table(r, Pick::Default) => format!("Speeds: technology table, default record {r}{fb}"),
            SpeedSource::Table(r, _) => format!("Speeds: technology table, {r}{fb}"),
            SpeedSource::Machine => "Speeds: the machine's default cutting speed".into(),
            SpeedSource::Estimate => "Speeds: typical laser speeds (estimate; the machine file gives none)".into(),
        }
    }
}

#[cfg(test)]
pub fn demo_table() -> &'static str {
    "#- LASER -#\nW000 3\n\
     A000 \"##MATEXP0100##\"\nA001 2.7\nA002 3.2\nA005 1\nA008 0\nB001 5\nC009 7500\n\
     D007 1500\nD010 0\nD057 5000\nD060 0.2\nD107 9500\nD110 0.3\nD115 10000\n\
     A000 \"##MATEXP0010##\"\nA001 2.7\nA002 3.2\nA005 3\nA008 0\nC009 3000\nD007 900\nD010 0.05\nD057 2900\nD107 4000\n\
     A000 \"##MATEXP0010##\"\nA001 2.7\nA002 3.2\nA005 1\nA008 1\nC009 7500\nD007 1500\nD057 5000\nD107 10000\n\
     W000 4\nA000 \"##MATEXP0100##\"\nA001 0\nA002 99\nD007 1\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(m: &str, t: f64, gas: u32) -> Keys {
        Keys { material: crate::ct1::material_key(m), thickness: t, gas }
    }

    #[test]
    fn the_table_is_read_by_record_and_quality() {
        let t = Table::from_text(demo_table());
        // The waterjet block (W000 4) is not the laser's.
        assert_eq!(t.records.len(), 3);
        let r = &t.records[0];
        assert_eq!((r.t_from, r.t_to, r.gas), (2.7, 3.2, 1));
        // TT3: quality q's speed is D(50(q−1)+7).
        assert_eq!((r.cut_speed(1), r.cut_speed(2), r.cut_speed(3)), (Some(1500.0), Some(5000.0), Some(9500.0)));
        assert_eq!(r.cut_speed(4), None);
        assert_eq!(r.mark_speed(), Some(7500.0));
    }

    #[test]
    fn the_record_is_the_first_full_fit_else_the_default() {
        let t = Table::from_text(demo_table());
        // Stainless with nitrogen: the first record.
        assert_eq!(t.pick(&keys("100", 3.0, 1)), Some((0, Pick::Exact)));
        // Mild steel with oxygen (gas 3): the oxygen record, before the default.
        assert_eq!(t.pick(&keys("10", 3.0, 3)), Some((1, Pick::Exact)));
        // Mild steel with a gas no record has: the default record.
        assert_eq!(t.pick(&keys("10", 3.0, 2)), Some((2, Pick::Default)));
        // Stainless with that gas: nothing (no default record).
        assert_eq!(t.pick(&keys("100", 3.0, 2)), None);
        // No nearest thickness: 3.3 mm is outside 2.7–3.2 (+0.005).
        assert_eq!(t.pick(&keys("100", 3.3, 1)), None);
        assert_eq!(t.pick(&keys("100", 3.204, 1)), Some((0, Pick::Exact)));
    }

    #[test]
    fn pierce_time_adds_the_terms_the_machine_shows() {
        let t = Table::from_text(demo_table());
        let r = &t.records[0];
        // q2: its pierce time only.
        assert!((r.pierce_secs(2, &Fields::default()) - 0.2).abs() < 1e-9);
        // q3 uses pierce type 1 (ten-thousands digit): + B001.
        assert!((r.pierce_secs(3, &Fields::default()) - 5.3).abs() < 1e-9);
        assert!((r.pierce_secs(3, &Fields { pierce_types: false, pierce_second: true }) - 0.3).abs() < 1e-9);
        let f = fields_from_cf("T408 \"\" OFF 0\nT412 \"Second\" ON 0\n");
        assert_eq!(f, Fields { pierce_types: false, pierce_second: true });
    }

    #[test]
    fn speeds_follow_the_time_file_switches() {
        let table = Table::from_text(demo_table());
        let k = keys("100", 3.0, 1);
        // Time file says tables (D018 2, D017 2, D014 3).
        let files = SpeedFiles { time: TimeSettings::from_text("D014 3\nD017 2\nD018 2\n"), table: Some(table.clone()), fields: Fields::default(), gas: 1 };
        let s = Speeds::resolve(&files, &k, Some(3000.0), 4500.0, Some(4800.0), Some(40000.0));
        assert_eq!((s.cut_for(1), s.cut_for(3)), (1500.0, 9500.0));
        assert_eq!(s.mark, 7500.0);
        assert_eq!(s.rapid, 40000.0);
        assert!((s.pierce_for(2) - 0.2).abs() < 1e-9);
        assert!(matches!(s.source, SpeedSource::Table(_, Pick::Exact)));
        // Quality 4 has no table speed: the machine's default speed (ours).
        assert_eq!(s.cut_for(4), 3000.0);
        // No time file: the machine's own speeds and 1 s per mm per pierce (CTM16).
        let plain = SpeedFiles { table: Some(table), ..Default::default() };
        let s = Speeds::resolve(&plain, &k, Some(3000.0), 4500.0, Some(4800.0), None);
        assert_eq!((s.cut_for(3), s.mark, s.rapid, s.pierce_for(1)), (3000.0, 4800.0, RAPID_DEFAULT, 3.0));
        assert_eq!(s.source, SpeedSource::Machine);
        // No machine speed either: our estimate.
        let s = Speeds::resolve(&SpeedFiles::default(), &k, None, 4500.0, None, None);
        assert_eq!((s.cut_for(1), s.source.clone()), (4500.0, SpeedSource::Estimate));
        // A machine speed of 0 is none (TT6): our estimate, never a feed of 0.
        let s = Speeds::resolve(&SpeedFiles::default(), &k, Some(0.0), 4500.0, Some(0.0), None);
        assert_eq!((s.cut_for(1), s.mark, s.source.clone()), (4500.0, 9000.0, SpeedSource::Estimate));
    }
}
