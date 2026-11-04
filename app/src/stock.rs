//! Sheet stock: the sheets and remnants on hand, kept in the Library folder
//! (`Sheets/stock.json`). It follows the original's rules (MECHANICS.md
//! "Sheet stock and remnants"), stored our own way. Lantek keeps sheets and remnants as
//! products in its SQL database (PPRR_PPRR_00000100, stock in DIS_CamQuan).
//!
//! - Three kinds: plain sheets, remnants left by a nesting (they record the
//!   sheet they came from), and remnants a user drew (no origin) (SS3).
//! - Stock goes down when a sheet is given to a nesting and comes back when
//!   the nesting is deleted (SS10). It never goes below 0 unless allowed, and
//!   is capped at 32 500 (SS11).
//! - Automatic nesting skips locked items and items with less stock than it
//!   needs (SS4), can offer remnants first (SS5) and picks the next sheet by
//!   size or priority (SS6).

use crate::nest::P;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Highest quantity an item can hold (the original caps it, SS11).
pub const MAX_QTY: i32 = 32_500;
/// "No limit" in an availability list (SS12). Never stored in the file.
pub const UNLIMITED: i32 = -1;
/// Default thickness / size tolerance when matching stock (mm). The original
/// reads it from its database settings; the value isn't visible (estimated).
pub const TOL: f64 = 0.01;
/// Priority given to new items when none is set. Priority 1 is used first.
pub const DEFAULT_PRIORITY: i32 = 1;
/// Longest reference (remnant references are cut to 40, RE22).
pub const MAX_REF: usize = 40;

const FORMAT: &str = "nestpro-stock";
const VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Sheet,
    /// Left over from a nesting; `origin` names the sheet it came from.
    NestRemnant,
    /// Drawn or entered by a user; no origin.
    UserRemnant,
}

impl Kind {
    pub fn is_remnant(self) -> bool {
        self != Kind::Sheet
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sheet => "Sheet",
            Kind::NestRemnant => "Remnant (nesting)",
            Kind::UserRemnant => "Remnant (user)",
        }
    }
}

/// A remnant's real shape: outer outline plus holes, in mm, with the item's
/// lower-left corner of its box at (0, 0).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub outer: Vec<P>,
    #[serde(default)]
    pub holes: Vec<Vec<P>>,
}

impl Outline {
    /// Net area in mm² (outer less holes).
    pub fn area_mm2(&self) -> f64 {
        crate::nest::area(&self.outer).abs() - self.holes.iter().map(|h| crate::nest::area(h).abs()).sum::<f64>()
    }
    /// Box size (length along X, width along Y).
    pub fn size(&self) -> (f64, f64) {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &self.outer {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        if x1 < x0 { (0.0, 0.0) } else { (x1 - x0, y1 - y0) }
    }
    /// Moved so its box starts at (0, 0).
    pub fn normalised(mut self) -> Self {
        let (mut x0, mut y0) = (f64::MAX, f64::MAX);
        for p in &self.outer {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
        }
        if x0 == f64::MAX {
            return self;
        }
        for p in self.outer.iter_mut().chain(self.holes.iter_mut().flatten()) {
            p[0] -= x0;
            p[1] -= y0;
        }
        self
    }
}

/// "Remnants can be rotated" (manual p. 559): how a remnant is turned when
/// it's loaded for nesting (ST10, RE26).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum RemnantRotation {
    NotAllowed,
    #[default]
    Allowed,
    PreferHorizontal,
    PreferVertical,
}

impl RemnantRotation {
    pub const ALL: [RemnantRotation; 4] = [RemnantRotation::NotAllowed, RemnantRotation::Allowed, RemnantRotation::PreferHorizontal, RemnantRotation::PreferVertical];
    pub fn label(self) -> &'static str {
        match self {
            RemnantRotation::NotAllowed => "Not allow rotations",
            RemnantRotation::Allowed => "Allow rotations",
            RemnantRotation::PreferHorizontal => "Prioritize horizontal",
            RemnantRotation::PreferVertical => "Prioritize vertically",
        }
    }
}

/// How much straight edge each corner of a shape has: the straight lengths
/// lying on the box's left, right, bottom and top sides, summed per corner
/// (ST10). Returns (BL, TL, TR, BR).
fn corner_scores(o: &Outline) -> [f64; 4] {
    let (w, h) = o.size();
    let e = 1e-3;
    let (mut l, mut r, mut b, mut t) = (0.0, 0.0, 0.0, 0.0);
    let n = o.outer.len();
    for i in 0..n {
        let (p, q) = (o.outer[i], o.outer[(i + 1) % n]);
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        if p[0].abs() < e && q[0].abs() < e {
            l += len;
        } else if (p[0] - w).abs() < e && (q[0] - w).abs() < e {
            r += len;
        } else if p[1].abs() < e && q[1].abs() < e {
            b += len;
        } else if (p[1] - h).abs() < e && (q[1] - h).abs() < e {
            t += len;
        }
    }
    [b + l, t + l, t + r, b + r]
}

/// Turn a shape (quarter turns counter-clockwise, then mirror in X if asked)
/// and move its box back to (0, 0).
pub fn turn_outline(o: &Outline, quarters: u8, mirror: bool) -> Outline {
    let mut v = o.clone().normalised();
    for _ in 0..(quarters % 4) {
        let (_, h) = v.size();
        for p in v.outer.iter_mut().chain(v.holes.iter_mut().flatten()) {
            *p = [h - p[1], p[0]];
        }
        v = v.normalised();
    }
    if mirror {
        let (w, _) = v.size();
        for p in v.outer.iter_mut().chain(v.holes.iter_mut().flatten()) {
            p[0] = w - p[0];
        }
        v.outer.reverse();
        for hh in v.holes.iter_mut() {
            hh.reverse();
        }
    }
    v.normalised()
}

/// Turn a remnant so the corner with the most straight edge is at the
/// machine's start corner — bottom-left here, where nesting starts — the way
/// the original does when it loads one (RE26). "Prioritize horizontal /
/// vertically" first lays the long side that way and then only turns it half
/// round. Mirroring only with "Allow turning over".
pub fn orient_for_machine(o: &Outline, rot: RemnantRotation, turn_over: bool) -> Outline {
    if rot == RemnantRotation::NotAllowed || o.outer.len() < 3 {
        return o.clone().normalised();
    }
    let (w, h) = o.size();
    let pre = match rot {
        RemnantRotation::PreferHorizontal if h > w => 1,
        RemnantRotation::PreferVertical if h < w => 1,
        _ => 0,
    };
    let turns: &[u8] = if rot == RemnantRotation::Allowed { &[0, 1, 2, 3] } else { &[0, 2] };
    let mirrors: &[bool] = if turn_over { &[false, true] } else { &[false] };
    let mut best: Option<(f64, Outline)> = None;
    for &m in mirrors {
        for &q in turns {
            let c = turn_outline(o, pre + q, m);
            let s = corner_scores(&c);
            // The bottom-left corner must be (one of) the best.
            let bl = s[0];
            if best.as_ref().is_none_or(|b| bl > b.0 + 1e-6) {
                best = Some((bl, c));
            }
        }
    }
    best.map(|b| b.1).unwrap_or_else(|| o.clone().normalised())
}

/// One stock item (SS1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub reference: String,
    pub kind: Kind,
    /// Index into the app's materials.
    pub material: usize,
    pub thickness: f64,
    /// Length (X) and width (Y) in mm. For a remnant, its outline's box.
    pub length: f64,
    pub width: f64,
    /// Real shape for remnants (None = a full rectangle).
    #[serde(default)]
    pub outline: Option<Outline>,
    pub qty: i32,
    /// 1 is used first when ordering by priority (SS6).
    pub priority: i32,
    /// Locked items are never used by automatic nesting (SS4, SS21).
    #[serde(default)]
    pub locked: bool,
    /// For quotations only: listed only when quoting (SS2).
    #[serde(default)]
    pub quote: bool,
    /// The sheet a nesting remnant was cut from (SS3, SS16).
    #[serde(default)]
    pub origin: String,
    /// The job a nesting remnant came from.
    #[serde(default)]
    pub from_job: String,
    #[serde(default)]
    pub user_data: [String; 3],
    #[serde(default)]
    pub description: String,
    /// Margin kept clear along a remnant's cut edges.
    #[serde(default)]
    pub remnant_margin: f64,
    /// Cost per kg; the remnant and scrap shares of it, in % (SS23).
    #[serde(default)]
    pub price_kg: f64,
    #[serde(default)]
    pub remnant_pct: f64,
    #[serde(default)]
    pub scrap_pct: f64,
    /// When it was added (YYYY-MM-DD).
    #[serde(default)]
    pub created: String,
}

impl Item {
    pub fn sheet(reference: &str, material: usize, thickness: f64, length: f64, width: f64, qty: i32) -> Self {
        Item {
            reference: reference.into(),
            kind: Kind::Sheet,
            material,
            thickness,
            length,
            width,
            outline: None,
            qty: qty.clamp(0, MAX_QTY),
            priority: DEFAULT_PRIORITY,
            locked: false,
            quote: false,
            origin: String::new(),
            from_job: String::new(),
            user_data: Default::default(),
            description: String::new(),
            remnant_margin: 0.0,
            price_kg: 0.0,
            remnant_pct: 0.0,
            scrap_pct: 0.0,
            created: today(),
        }
    }

    /// Real area in m²: the outline's net area for remnants, else L × W (SS15).
    pub fn area_m2(&self) -> f64 {
        match &self.outline {
            Some(o) if o.outer.len() >= 3 => o.area_mm2() / 1e6,
            _ => self.length * self.width / 1e6,
        }
    }

    /// Weight in kg from the real area.
    pub fn weight_kg(&self) -> f64 {
        crate::cnc::weight_kg(self.material, self.area_m2() * 1e6, self.thickness)
    }

    fn same_format(&self, material: usize, thickness: f64, length: f64, width: f64) -> bool {
        self.material == material && (self.thickness - thickness).abs() <= TOL && (self.length - length).abs() <= TOL && (self.width - width).abs() <= TOL
    }
}

/// Which items a list shows or automatic nesting may use (the Criteria window,
/// manual p. 210; the original's stock-list filters, SS2–SS4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Criteria {
    pub min_length: f64,
    pub max_length: f64,
    pub min_width: f64,
    pub max_width: f64,
    /// Empty = any.
    pub user_data: [String; 3],
    pub show_zero_qty: bool,
    pub include_locked: bool,
    pub nest_remnants: bool,
    pub user_remnants: bool,
    pub sheets: bool,
    pub quoting: bool,
}

impl Default for Criteria {
    fn default() -> Self {
        Criteria {
            min_length: 0.0,
            max_length: 0.0,
            min_width: 0.0,
            max_width: 0.0,
            user_data: Default::default(),
            show_zero_qty: false,
            include_locked: false,
            nest_remnants: true,
            user_remnants: true,
            sheets: true,
            quoting: false,
        }
    }
}

impl Criteria {
    /// Every item of the material and thickness, whatever its state (for the
    /// stock window).
    pub fn everything() -> Self {
        Criteria { show_zero_qty: true, include_locked: true, ..Default::default() }
    }

    /// Does the item pass these criteria (any material and thickness)?
    pub fn admits(&self, it: &Item) -> bool {
        let size_ok = |v: f64, lo: f64, hi: f64| (lo <= 0.0 || v >= lo - TOL) && (hi <= 0.0 || v <= hi + TOL);
        let kind_ok = match it.kind {
            Kind::Sheet => self.sheets,
            Kind::NestRemnant => self.nest_remnants,
            Kind::UserRemnant => self.user_remnants,
        };
        kind_ok
            && (self.quoting || !it.quote)
            && (self.show_zero_qty || it.qty > 0)
            && (self.include_locked || !it.locked)
            && size_ok(it.length, self.min_length, self.max_length)
            && size_ok(it.width, self.min_width, self.max_width)
            && self.user_data.iter().zip(&it.user_data).all(|(want, has)| want.is_empty() || want == has)
    }
}

/// The order automatic nesting takes sheets in (Jobs config → Do All → Order of
/// sheets; its three sheet orders, SS6).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Order {
    #[default]
    LargestFirst,
    SmallestFirst,
    /// Priority 1 first.
    ByPriority,
}

impl Order {
    pub const ALL: [Order; 3] = [Order::LargestFirst, Order::SmallestFirst, Order::ByPriority];
    pub fn label(self) -> &'static str {
        match self {
            Order::LargestFirst => "Largest to smallest",
            Order::SmallestFirst => "Smallest to largest",
            Order::ByPriority => "By priority",
        }
    }
}

/// How "used area" is counted when Do All checks a sheet's efficiency
/// (the original's four efficiency modes, SS7).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum EffMode {
    #[default]
    RealArea,
    ExternalArea,
    RectangularArea,
    /// maxX × maxY of the parts, from the sheet origin.
    PartsBox,
}

impl EffMode {
    pub const ALL: [EffMode; 4] = [EffMode::RealArea, EffMode::ExternalArea, EffMode::RectangularArea, EffMode::PartsBox];
    pub fn label(self) -> &'static str {
        match self {
            EffMode::RealArea => "Real area",
            EffMode::ExternalArea => "External area",
            EffMode::RectangularArea => "Rectangular area",
            EffMode::PartsBox => "All parts bounding box",
        }
    }
}

/// How automatic nesting takes sheets from stock and what saving does with
/// what's left (Jobs configuration → Do All, manual pp. 209–213; Sheet
/// Remnants, pp. 556–558). Defaults are estimates where the original's aren't
/// known yet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StockSettings {
    /// Nest on sheets from the stock of the job's material and thickness.
    pub use_stock: bool,
    pub order: Order,
    pub remnants_first: bool,
    /// Which stock items automatic nesting may use.
    pub criteria: Criteria,
    /// When the stock runs out, carry on with the job's sheet size (the
    /// original stops and leaves the rest unnested).
    pub fallback_job_size: bool,
    /// Use items with too little stock anyway (stock may go below 0).
    pub allow_no_stock: bool,
    /// Drop sheets whose efficiency (0..1) is below this; 0 = off (SS7).
    pub min_efficiency: f64,
    pub eff_mode: EffMode,
    /// Sheets with the same material, thickness and size are one item unless on (SS14).
    pub with_references: bool,
    /// Generate remnants when you save.
    pub remnants_on_save: bool,
    /// Generate remnants with the completed sheets.
    pub keep_completed: bool,
    /// Minimum area of remnants (m²).
    pub min_remnant_m2: f64,
    /// Ask for each remnant's reference instead of numbering it.
    pub prompt_remnant_ref: bool,
    /// Remnants can be rotated / allow turning over the remnant (when loaded).
    pub remnant_rotation: RemnantRotation,
    pub turn_over: bool,
    /// The automatic remnant line's shape and options.
    pub line: crate::remline::LineSettings,
    /// Automatic sheet / remnant references and their formats (RF1; off and
    /// empty by default).
    pub auto_sheet_ref: bool,
    pub sheet_ref_format: String,
    pub auto_remnant_ref: bool,
    pub remnant_ref_format: String,
    /// The automatic mark on remnants (RF9–RF10).
    pub mark: crate::remmark::MarkSettings,
    /// "Look for the best sheet format": nest the whole job on each sheet size
    /// and keep the best (UI21).
    pub best_format: bool,
    /// "Search for the best sheet combination" and its minimum efficiency (UI22).
    pub best_combination: bool,
    pub combination_min: f64,
    /// "Maximum time to look for the best sheets" (seconds).
    pub max_seconds: f64,
    /// Put the automatic remnant line on every sheet after nesting
    /// ("Generate remnants after nesting", manual p. 213).
    pub lines_after_nesting: bool,
}

impl Default for StockSettings {
    fn default() -> Self {
        StockSettings {
            use_stock: true,
            order: Order::LargestFirst,
            remnants_first: true,
            criteria: Criteria::default(),
            fallback_job_size: true,
            allow_no_stock: false,
            min_efficiency: 0.0,
            eff_mode: EffMode::RealArea,
            with_references: true,
            remnants_on_save: false,
            keep_completed: false,
            min_remnant_m2: crate::remnant::DEFAULT_MIN_AREA_M2,
            prompt_remnant_ref: false,
            remnant_rotation: RemnantRotation::Allowed,
            turn_over: true,
            line: Default::default(),
            lines_after_nesting: false,
            auto_sheet_ref: false,
            sheet_ref_format: String::new(),
            auto_remnant_ref: false,
            remnant_ref_format: String::new(),
            mark: Default::default(),
            best_format: false,
            best_combination: false,
            combination_min: 0.7,
            max_seconds: 60.0,
        }
    }
}

/// Is a Do All sheet good enough? Efficiency = used ÷ sheet area as a fraction;
/// the sheet is dropped only when it is strictly below the minimum (SS7).
pub fn keeps_sheet(used_mm2: f64, sheet_mm2: f64, min_eff: f64) -> bool {
    if sheet_mm2 <= 0.0 {
        return false;
    }
    used_mm2 / sheet_mm2 >= min_eff
}

/// The stock file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stock {
    pub format: String,
    pub version: u32,
    pub items: Vec<Item>,
    /// Last numbers used for automatic references (base 36), like the
    /// original's CONTCHP.cdr and CONTRET.cdr counters.
    pub sheet_counter: u64,
    pub remnant_counter: u64,
    /// Remnants a job's store replaced (rebuilt for a changed sheet): old
    /// reference → the ones made in its place (none when the sheet now has
    /// no remnant). Undo follows these to the remnants that are really in
    /// stock now (RN6); a remnant deleted by hand is never in here.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub replaced: std::collections::BTreeMap<String, Vec<String>>,
}

impl Default for Stock {
    fn default() -> Self {
        Stock { format: FORMAT.into(), version: VERSION, items: vec![], sheet_counter: 0, remnant_counter: 0, replaced: Default::default() }
    }
}

/// `n` in base 36 with digits 0–9 then A–Z, as the original's counters
/// (sheets `1`…`I`, remnants `…-2H`).
pub fn base36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if n == 0 {
        return "0".into();
    }
    let mut s = vec![];
    while n > 0 {
        s.push(D[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Names Windows keeps for devices; the counters skip them (RF3).
fn is_device_name(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    matches!(u.as_str(), "NUL" | "CON" | "AUX" | "PRN") || ((u.starts_with("COM") || u.starts_with("LPT")) && u.len() == 4 && u.as_bytes()[3].is_ascii_digit() && u.as_bytes()[3] != b'0')
}

/// What a reference format can use (RF4–RF5).
#[derive(Clone, Debug, Default)]
pub struct RefValues {
    pub material: String,
    pub user_data: [String; 3],
    /// The origin sheet (remnants) — [SHT].
    pub sheet: String,
    /// Length along X, width along Y, thickness (mm).
    pub length: f64,
    pub width: f64,
    pub thickness: f64,
    /// Decimals when a number token doesn't give them.
    pub decimals: usize,
}

/// Expand a reference format (RF4–RF6): `[MAT]`, `[UD1]`–`[UD3]`, `[SHT]`,
/// `[AUT]` (the counter, taken only when the format uses it), and the number
/// tokens `[LNGx]`, `[WDTx]`, `[THKx]` — x's first character `1` divides by 10,
/// `2` by 100; with exactly three characters the last is the decimals. Any
/// other `[xxx]` becomes `xxx`. The result keeps its first 40 characters.
pub fn expand_format(format: &str, v: &RefValues, mut counter: impl FnMut() -> String) -> String {
    let mut out = String::new();
    let mut rest = format;
    let mut seen_num: Vec<&str> = vec![];
    while let Some(a) = rest.find('[') {
        out.push_str(&rest[..a]);
        let Some(b) = rest[a..].find(']') else {
            out.push_str(&rest[a..]);
            rest = "";
            break;
        };
        let tok = &rest[a + 1..a + b];
        rest = &rest[a + b + 1..];
        let num = |name: &str, val: f64, seen: &mut Vec<&str>| -> Option<String> {
            let x = tok.strip_prefix(name)?;
            if seen.contains(&name) {
                return None;
            }
            let scale = match x.chars().next() {
                Some('1') => 10.0,
                Some('2') => 100.0,
                _ => 1.0,
            };
            let d = if x.chars().count() == 3 { x.chars().last().and_then(|c| c.to_digit(10)).map_or(v.decimals, |d| d as usize) } else { v.decimals };
            Some(format!("{:.d$}", val / scale, d = d))
        };
        let piece = match tok {
            "MAT" => v.material.clone(),
            "UD1" => v.user_data[0].clone(),
            "UD2" => v.user_data[1].clone(),
            "UD3" => v.user_data[2].clone(),
            "SHT" | "SHT#NestId" => v.sheet.clone(),
            "AUT" | "AUT#ByNest" => counter(),
            _ => {
                let mut r = None;
                for (name, val) in [("LNG", v.length), ("WDT", v.width), ("THK", v.thickness)] {
                    if tok.starts_with(name) {
                        r = num(name, val, &mut seen_num);
                        seen_num.push(name);
                        break;
                    }
                }
                r.unwrap_or_else(|| tok.to_string())
            }
        };
        out.push_str(&piece);
    }
    out.push_str(rest);
    out.chars().take(MAX_REF).collect()
}

pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

impl Stock {
    pub fn file(library_dir: &Path) -> PathBuf {
        library_dir.join("Sheets").join("stock.json")
    }

    /// The stock in a library folder (empty if there's none yet).
    pub fn load(library_dir: &Path) -> Result<Self, String> {
        let f = Self::file(library_dir);
        match std::fs::read_to_string(&f) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", f.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Stock::default()),
            Err(e) => Err(format!("{}: {e}", f.display())),
        }
    }

    /// `load`, re-read only when the file changed (for drawing every frame).
    pub fn load_cached(library_dir: &Path) -> Stock {
        thread_local! {
            static CACHE: std::cell::RefCell<Option<(PathBuf, Option<std::time::SystemTime>, Stock)>> = const { std::cell::RefCell::new(None) };
        }
        let f = Self::file(library_dir);
        let mtime = std::fs::metadata(&f).and_then(|m| m.modified()).ok();
        if let Some(s) = CACHE.with(|c| c.borrow().as_ref().filter(|(p, t, _)| *p == f && *t == mtime).map(|x| x.2.clone())) {
            return s;
        }
        let s = Stock::load(library_dir).unwrap_or_default();
        CACHE.with(|c| *c.borrow_mut() = Some((f, mtime, s.clone())));
        s
    }

    /// Write atomically (temp file, then rename) so a shared folder never sees
    /// half a file.
    pub fn save(&self, library_dir: &Path) -> Result<(), String> {
        let f = Self::file(library_dir);
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let tmp = f.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
    }

    /// Re-read the stock, change it and save it, so two users on a shared
    /// folder don't overwrite each other's changes.
    pub fn update<T>(library_dir: &Path, f: impl FnOnce(&mut Stock) -> Result<T, String>) -> Result<T, String> {
        let mut s = Stock::load(library_dir)?;
        let out = f(&mut s)?;
        s.save(library_dir)?;
        Ok(out)
    }

    pub fn get(&self, reference: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.reference == reference)
    }

    fn get_mut(&mut self, reference: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.reference == reference)
    }

    /// Items of a material and thickness (thickness within the tolerance) that
    /// the criteria admit, sorted by reference (SS2, SS3).
    pub fn list(&self, material: usize, thickness: f64, crit: &Criteria) -> Vec<&Item> {
        let mut v: Vec<&Item> = self.items.iter().filter(|i| i.material == material && (i.thickness - thickness).abs() <= TOL && crit.admits(i)).collect();
        v.sort_by(|a, b| ref_order(&a.reference, &b.reference));
        v
    }

    /// What automatic nesting may use (SS4, SS5): never locked, never with less
    /// stock than `needed` (at least 1) unless `allow_no_stock`; with
    /// `remnants_first`, only remnants as long as any is usable, otherwise
    /// sheets, then remnants. `exclude` holds references already tried.
    pub fn candidates(&self, material: usize, thickness: f64, crit: &Criteria, remnants_first: bool, needed: i32, allow_no_stock: bool, exclude: &[String]) -> Vec<&Item> {
        let crit = Criteria { include_locked: false, show_zero_qty: true, ..crit.clone() };
        let need = needed.max(1);
        let usable: Vec<&Item> = self
            .list(material, thickness, &crit)
            .into_iter()
            .filter(|i| !exclude.iter().any(|e| e == &i.reference))
            .filter(|i| allow_no_stock || i.qty >= need)
            .collect();
        let (rem, sht): (Vec<&Item>, Vec<&Item>) = usable.into_iter().partition(|i| i.kind.is_remnant());
        if remnants_first {
            if rem.is_empty() { sht } else { rem }
        } else {
            sht.into_iter().chain(rem).collect()
        }
    }

    /// The next sheet for automatic nesting (ST2, SS6): largest or
    /// smallest real area (strictly better wins, so ties keep list order), or
    /// the lowest priority number.
    pub fn next_sheet<'a>(&'a self, candidates: &[&'a Item], order: Order) -> Option<&'a Item> {
        let mut best: Option<&Item> = None;
        for &it in candidates {
            let better = match best {
                None => true,
                Some(b) => match order {
                    Order::LargestFirst => it.area_m2() > b.area_m2(),
                    Order::SmallestFirst => it.area_m2() < b.area_m2(),
                    Order::ByPriority => it.priority < b.priority,
                },
            };
            if better {
                best = Some(it);
            }
        }
        best
    }

    /// Take `n` of an item for a nesting (SS10, SS11). Fails when the stock
    /// would go below 0, unless `allow_negative`.
    pub fn take(&mut self, reference: &str, n: i32, allow_negative: bool) -> Result<(), String> {
        let it = self.get_mut(reference).ok_or_else(|| format!("Sheet {reference} isn't in stock."))?;
        let left = it.qty - n;
        if left < 0 && !allow_negative {
            return Err(format!("Only {} of sheet {reference} in stock, {n} needed.", it.qty));
        }
        it.qty = left.min(MAX_QTY);
        Ok(())
    }

    /// Give `n` back (a nesting was deleted), capped at 32 500 (SS10, SS11).
    pub fn give_back(&mut self, reference: &str, n: i32) {
        if let Some(it) = self.get_mut(reference) {
            it.qty = (it.qty + n).min(MAX_QTY);
        }
    }

    /// Add `n` (Add button) or remove it (Remove): at 0 or below the item is
    /// deleted, as the original does (manual p. 535–536).
    pub fn change_qty(&mut self, reference: &str, delta: i32) -> Result<(), String> {
        let it = self.get_mut(reference).ok_or_else(|| format!("Sheet {reference} isn't in stock."))?;
        let q = (it.qty + delta).min(MAX_QTY);
        if q <= 0 {
            self.delete(reference);
        } else {
            it.qty = q;
        }
        Ok(())
    }

    pub fn set_locked(&mut self, reference: &str, locked: bool) {
        if let Some(it) = self.get_mut(reference) {
            it.locked = locked;
        }
    }

    pub fn delete(&mut self, reference: &str) {
        self.items.retain(|i| i.reference != reference);
    }

    /// The next automatic sheet reference (base-36 counter, skipping any that
    /// are taken).
    pub fn next_sheet_ref(&mut self) -> String {
        loop {
            self.sheet_counter += 1;
            let r = base36(self.sheet_counter);
            // Names kept for undo's history are skipped too (RN6).
            if self.get(&r).is_none() && !is_device_name(&r) && !self.in_replaced(&r) {
                return r;
            }
        }
    }

    /// The next remnant counter value (base 36, device names skipped).
    pub fn next_remnant_number(&mut self) -> String {
        loop {
            self.remnant_counter += 1;
            let r = base36(self.remnant_counter);
            if !is_device_name(&r) {
                return r;
            }
        }
    }

    /// A sheet reference from a format (Automatic sheet references), or the
    /// next counter value when the format is empty.
    pub fn sheet_ref_from(&mut self, format: &str, v: &RefValues) -> String {
        if format.trim().is_empty() {
            return self.next_sheet_ref();
        }
        expand_format(format, v, || self.next_sheet_ref())
    }

    /// A remnant reference from a format (Automatic remnant references), or
    /// `<origin>-<counter>` when the format is empty (RF2). The counter moves
    /// on only when the format uses it.
    pub fn remnant_ref_from(&mut self, format: &str, v: &RefValues) -> String {
        if format.trim().is_empty() {
            return self.next_remnant_ref(&v.sheet);
        }
        let r = expand_format(format, v, || self.next_remnant_number());
        // A formatted name kept for undo's history (RN6): the next automatic one.
        if self.reserved(&r) { self.next_remnant_ref(&v.sheet) } else { r }
    }


    /// The next automatic remnant reference: `<origin>-<counter>` (the sample
    /// database's `2-16`, `E-2F`), cut to 40 characters.
    pub fn next_remnant_ref(&mut self, origin: &str) -> String {
        loop {
            let suffix = self.next_remnant_number();
            let head: String = origin.chars().take(MAX_REF.saturating_sub(suffix.len() + 1)).collect();
            let r = if head.is_empty() { format!("R-{suffix}") } else { format!("{head}-{suffix}") };
            if self.get(&r).is_none() && !self.in_replaced(&r) {
                return r;
            }
        }
    }

    /// Add a new sheet (SS13, SS14). An empty reference gets the next number.
    /// Without sheet references, a sheet of the same material, thickness and
    /// size just adds its quantity to the existing one. A taken reference is
    /// refused. Returns the reference used.
    pub fn add_sheet(&mut self, mut item: Item, with_references: bool) -> Result<String, String> {
        item.qty = item.qty.clamp(0, MAX_QTY);
        if !with_references && item.kind == Kind::Sheet {
            if let Some(ex) = self.items.iter_mut().find(|i| i.kind == Kind::Sheet && i.same_format(item.material, item.thickness, item.length, item.width)) {
                ex.qty = (ex.qty + item.qty).min(MAX_QTY);
                return Ok(ex.reference.clone());
            }
        }
        if item.reference.trim().is_empty() {
            item.reference = self.next_sheet_ref();
        }
        if item.reference.chars().count() > MAX_REF {
            return Err(format!("A reference can have at most {MAX_REF} characters."));
        }
        if self.get(&item.reference).is_some() {
            return Err(format!("There's already a sheet called {}.", item.reference));
        }
        if self.reserved(&item.reference) {
            return Err(format!("{} was the reference of a remnant that has since been replaced — pick another reference.", item.reference));
        }
        let r = item.reference.clone();
        self.items.push(item);
        Ok(r)
    }

    /// Is this reference in the replacement history? (Automatic references
    /// skip those, so the history stays whole.)
    fn in_replaced(&self, reference: &str) -> bool {
        self.replaced.contains_key(reference) || self.replaced.values().any(|v| v.iter().any(|r| r == reference))
    }

    /// A name a new item may not take: in the replacement history and not the
    /// item still in stock under it. Undo follows the history by name, so a
    /// new item under such a name would be taken for the old one (RN6).
    pub fn reserved(&self, reference: &str) -> bool {
        self.get(reference).is_none() && self.in_replaced(reference)
    }

    /// Store a remnant a nesting left (SS16–SS18): box size from its outline,
    /// priority and user data copied from the sheet it came from, origin = that
    /// sheet. A remnant from a quotation sheet is stored with 0 and locked. An
    /// existing reference is updated (its quantity grows). The parent sheet's
    /// stock is not touched (it was taken when the nesting got the sheet).
    pub fn add_remnant(&mut self, outline: Outline, parent: &Item, qty: i32, reference: Option<String>, job: &str) -> String {
        let outline = outline.normalised();
        let (length, width) = outline.size();
        // A name kept for undo's history gets the next automatic one (RN6).
        let reference = reference.filter(|r| !r.trim().is_empty() && !self.reserved(r)).unwrap_or_else(|| self.next_remnant_ref(&parent.reference));
        let (qty, locked) = if parent.quote { (0, true) } else { (qty.max(0), false) };
        if let Some(ex) = self.get_mut(&reference) {
            ex.qty = (ex.qty + qty).min(MAX_QTY);
            ex.outline = Some(outline);
            ex.length = length;
            ex.width = width;
            return reference;
        }
        self.items.push(Item {
            reference: reference.clone(),
            kind: Kind::NestRemnant,
            material: parent.material,
            thickness: parent.thickness,
            length,
            width,
            outline: Some(outline),
            qty: qty.min(MAX_QTY),
            priority: parent.priority,
            locked,
            quote: parent.quote,
            origin: parent.reference.clone(),
            from_job: job.into(),
            user_data: parent.user_data.clone(),
            description: String::new(),
            remnant_margin: parent.remnant_margin,
            price_kg: parent.price_kg,
            remnant_pct: parent.remnant_pct,
            scrap_pct: parent.scrap_pct,
            created: today(),
        });
        reference
    }
}

/// References in a natural order: shorter base-36 numbers first, then text.
fn ref_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> Stock {
        let mut s = Stock::default();
        for (r, l, w, q) in [("1", 3000.0, 1500.0, 5), ("2", 2500.0, 1250.0, 2), ("3", 4000.0, 2000.0, 0)] {
            s.add_sheet(Item::sheet(r, 0, 10.0, l, w, q), true).unwrap();
        }
        s
    }

    #[test]
    fn base36_counts_like_the_original() {
        assert_eq!(base36(1), "1");
        assert_eq!(base36(18), "I"); // CONTCHP.cdr "!!!!!!!I" = 18 sheets
        assert_eq!(base36(89), "2H"); // CONTRET.cdr "!!!!!!2H"
        assert_eq!(base36(36), "10");
    }

    #[test]
    fn lists_match_material_thickness_and_criteria() {
        let mut s = stock();
        s.items.push(Item::sheet("X", 1, 10.0, 3000.0, 1500.0, 9));
        s.items.push(Item::sheet("Y", 0, 12.0, 3000.0, 1500.0, 9));
        let all = s.list(0, 10.0, &Criteria::everything());
        assert_eq!(all.iter().map(|i| i.reference.as_str()).collect::<Vec<_>>(), ["1", "2", "3"]);
        // Zero stock is hidden unless asked for; locked items too.
        s.set_locked("2", true);
        let shown = s.list(0, 10.0, &Criteria::default());
        assert_eq!(shown.iter().map(|i| i.reference.as_str()).collect::<Vec<_>>(), ["1"]);
        let crit = Criteria { min_length: 2600.0, show_zero_qty: true, include_locked: true, ..Default::default() };
        assert_eq!(s.list(0, 10.0, &crit).len(), 2);
    }

    #[test]
    fn automatic_nesting_skips_locked_and_short_stock() {
        let mut s = stock();
        s.set_locked("1", true);
        let c = s.candidates(0, 10.0, &Criteria::default(), false, 1, false, &[]);
        assert_eq!(c.iter().map(|i| i.reference.as_str()).collect::<Vec<_>>(), ["2"]);
        // Needing 3 repetitions: sheet 2 (2 in stock) is out.
        assert!(s.candidates(0, 10.0, &Criteria::default(), false, 3, false, &[]).is_empty());
        // Unless running without stock.
        assert_eq!(s.candidates(0, 10.0, &Criteria::default(), false, 3, true, &[]).len(), 2);
    }

    #[test]
    fn remnants_first_offers_only_remnants_while_any_is_usable() {
        let mut s = stock();
        let parent = s.get("1").unwrap().clone();
        let o = Outline { outer: vec![[0.0, 0.0], [800.0, 0.0], [800.0, 1500.0], [0.0, 1500.0]], holes: vec![] };
        let r = s.add_remnant(o, &parent, 1, None, "J1");
        let c = s.candidates(0, 10.0, &Criteria::default(), true, 1, false, &[]);
        assert_eq!(c.iter().map(|i| i.reference.as_str()).collect::<Vec<_>>(), [r.as_str()]);
        let c = s.candidates(0, 10.0, &Criteria::default(), false, 1, false, &[]);
        assert_eq!(c.last().unwrap().reference, r);
        s.take(&r, 1, false).unwrap();
        let c = s.candidates(0, 10.0, &Criteria::default(), true, 1, false, &[]);
        assert_eq!(c.len(), 2); // no usable remnant left: sheets
    }

    #[test]
    fn next_sheet_orders_by_area_or_lowest_priority_number() {
        let mut s = stock();
        s.items[0].priority = 3;
        s.items[1].priority = 2;
        let c = s.candidates(0, 10.0, &Criteria::default(), false, 1, false, &[]);
        assert_eq!(s.next_sheet(&c, Order::LargestFirst).unwrap().reference, "1");
        assert_eq!(s.next_sheet(&c, Order::SmallestFirst).unwrap().reference, "2");
        assert_eq!(s.next_sheet(&c, Order::ByPriority).unwrap().reference, "2");
        // A tie keeps the first in the list.
        s.items[1].priority = 3;
        let c = s.candidates(0, 10.0, &Criteria::default(), false, 1, false, &[]);
        assert_eq!(s.next_sheet(&c, Order::ByPriority).unwrap().reference, "1");
    }

    #[test]
    fn stock_changes_are_bounded() {
        let mut s = stock();
        assert!(s.take("2", 3, false).is_err());
        s.take("2", 2, false).unwrap();
        assert_eq!(s.get("2").unwrap().qty, 0);
        s.give_back("2", 40_000);
        assert_eq!(s.get("2").unwrap().qty, MAX_QTY);
        s.change_qty("1", -5).unwrap();
        assert!(s.get("1").is_none(), "removing down to 0 deletes the sheet");
    }

    #[test]
    fn new_sheets_merge_without_references_and_refuse_duplicates() {
        let mut s = stock();
        let r = s.add_sheet(Item::sheet("", 0, 10.0, 3000.0, 1500.0, 4), false).unwrap();
        assert_eq!(r, "1");
        assert_eq!(s.get("1").unwrap().qty, 9);
        let r = s.add_sheet(Item::sheet("", 0, 10.0, 3000.0, 1500.0, 4), true).unwrap();
        assert_eq!(r, "4", "next free counter");
        assert!(s.add_sheet(Item::sheet("2", 0, 10.0, 1.0, 1.0, 1), true).is_err());
    }

    #[test]
    fn remnants_keep_their_origin_shape_and_real_area() {
        let mut s = stock();
        let mut parent = s.get("1").unwrap().clone();
        parent.priority = 4;
        parent.user_data[0] = "rack A".into();
        // An L-shaped remnant placed away from the origin.
        let o = Outline { outer: vec![[2000.0, 0.0], [3000.0, 0.0], [3000.0, 1500.0], [2500.0, 1500.0], [2500.0, 500.0], [2000.0, 500.0]], holes: vec![] };
        let r = s.add_remnant(o, &parent, 2, None, "J7");
        assert_eq!(r, "1-1");
        let it = s.get(&r).unwrap();
        assert_eq!((it.length, it.width), (1000.0, 1500.0));
        assert!((it.area_m2() - 1.0).abs() < 1e-9, "1000×500 + 500×1000 = 1 m², not the 1.5 m² box");
        assert_eq!((it.priority, it.origin.as_str(), it.qty, it.kind), (4, "1", 2, Kind::NestRemnant));
        assert_eq!(it.user_data[0], "rack A");
        assert_eq!(it.outline.as_ref().unwrap().outer[0], [0.0, 0.0]);
        // From a quotation sheet: 0 and locked.
        parent.quote = true;
        let o = Outline { outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], holes: vec![] };
        let q = s.add_remnant(o, &parent, 3, None, "J7");
        assert_eq!((s.get(&q).unwrap().qty, s.get(&q).unwrap().locked), (0, true));
    }

    #[test]
    fn remnants_are_turned_to_put_the_straightest_corner_at_the_start() {
        // An L missing its bottom-left: the straightest corner is top-right.
        let l = Outline { outer: vec![[400.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0], [0.0, 400.0], [400.0, 400.0]], holes: vec![] };
        let s = corner_scores(&l);
        assert!(s[2] > s[0] && s[2] > s[1] && s[2] > s[3], "{s:?}");
        let o = orient_for_machine(&l, RemnantRotation::Allowed, false);
        let s = corner_scores(&o);
        assert!(s[0] >= s[1] && s[0] >= s[2] && s[0] >= s[3], "bottom-left is now the best: {s:?}");
        assert!((o.area_mm2() - l.area_mm2()).abs() < 1e-6);
        assert_eq!(orient_for_machine(&l, RemnantRotation::NotAllowed, true), l.clone().normalised());
        // Prioritize horizontal: a tall strip is laid down first.
        let tall = Outline { outer: vec![[0.0, 0.0], [300.0, 0.0], [300.0, 1200.0], [0.0, 1200.0]], holes: vec![] };
        let lying = orient_for_machine(&tall, RemnantRotation::PreferHorizontal, false);
        assert_eq!(lying.size(), (1200.0, 300.0));
    }

    #[test]
    fn reference_formats_expand_like_the_original() {
        let v = RefValues { material: "S235".into(), user_data: ["A".into(), String::new(), String::new()], sheet: "7".into(), length: 1234.5, width: 678.0, thickness: 10.0, decimals: 0 };
        let mut s = Stock::default();
        // Formats of the kind shops use: material and sizes, or a numbered remnant.
        assert_eq!(s.sheet_ref_from("PL-[MAT]_[LNG]x[WDT]x[THK]", &v), "PL-S235_1234x678x10");
        assert_eq!(s.sheet_counter, 0, "no [AUT]: the counter isn't used");
        assert_eq!(s.remnant_ref_from("REM_[MAT]_T_[THK]_[AUT]", &v), "REM_S235_T_10_1");
        assert_eq!(s.remnant_counter, 1);
        // ÷10 / ÷100 and decimals; unknown tokens lose their brackets.
        assert_eq!(expand_format("[LNG1x1]/[WDT2x2]/[XYZ]/[UD1]", &v, || "?".into()), "123.5/6.78/XYZ/A");
        // Cut to 40 characters from the end.
        assert_eq!(expand_format(&"X".repeat(50), &v, || "?".into()).len(), 40);
        // No format: origin-counter.
        assert_eq!(s.remnant_ref_from("", &v), "7-2");
        // Device names are skipped.
        assert!(is_device_name("COM1") && is_device_name("con") && !is_device_name("COM0") && !is_device_name("CONX"));
    }

    #[test]
    fn efficiency_drops_only_below_the_minimum() {
        assert!(keeps_sheet(500.0, 1000.0, 0.5));
        assert!(!keeps_sheet(499.0, 1000.0, 0.5));
        assert!(!keeps_sheet(1.0, 0.0, 0.0));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("nestpro-stock-{}", std::process::id()));
        let s = stock();
        s.save(&dir).unwrap();
        assert_eq!(Stock::load(&dir).unwrap(), s);
        let r = Stock::update(&dir, |s| Ok(s.add_sheet(Item::sheet("", 0, 10.0, 1.0, 1.0, 1), true)?)).unwrap();
        assert!(Stock::load(&dir).unwrap().get(&r).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
