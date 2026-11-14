//! The sheet stock's part in a job, shared by the window and the agent
//! operations: which stock sheets Do All takes and in what order (SS4–SS7,
//! SS12), taking sheets from stock and giving them back as a job's nestings
//! change (SS10, SS11), and storing the remnants its remnant lines leave (RE1).
//! The stock itself (items, counters, file) is `crate::stock`.

use crate::cutoff::{CutKind, CutLine};
use crate::library::Library;
use crate::nest::{self, FormatSlot, NestResult, Part, Placed, SheetFormat, SheetPick, SheetSpec};
use crate::stock::{self, EffMode, Item, Stock, StockSettings};
use std::collections::BTreeMap;
use std::path::Path;

/// What nesting from stock needs to know besides the parts and the sheet.
pub struct StockUse<'a> {
    /// The library folder the stock file lives in.
    pub dir: std::path::PathBuf,
    /// The Do All / Sheet Remnants options.
    pub st: &'a StockSettings,
    /// What the job holds from stock as of its last save (counts as free
    /// again for this job, SS12).
    pub held: &'a BTreeMap<String, i32>,
    /// Stock references never to nest on (the remnants this job cut itself).
    pub exclude: Vec<String>,
}

/// The sheets nesting may use, in the order it takes them (SS4–SS6, SS12):
/// stock items of the job's material and thickness — remnants first if set,
/// then by the chosen order — each with what's in stock plus what this job
/// already holds; then the job's sheet size when stock may run out. With a
/// size picked for the job, only the stock's sheets of that size (ST18).
/// None when stock isn't used or has nothing for this job.
pub fn stock_plan(spec: &SheetSpec, u: &StockUse, exclude: &[String]) -> Option<Vec<FormatSlot>> {
    let st = u.st;
    if !st.use_stock {
        return None;
    }
    let stock = Stock::load(&u.dir).ok()?;
    let items = only_picked(usable_items(&stock, spec, u, exclude), spec);
    if items.is_empty() {
        // Nothing in stock for this job: the job's sheet size only when the
        // options allow it, else no sheets (never a sheet that isn't there).
        return (!st.fallback_job_size).then(Vec::new);
    }
    let mut plan: Vec<FormatSlot> = items
        .iter()
        .map(|it| {
            let mut f = SheetFormat::from_stock(it);
            // A remnant is turned for the machine as it's loaded (RE26).
            if let Some(o) = f.outline.as_ref().filter(|_| f.remnant) {
                let o = stock::orient_for_machine(o, st.remnant_rotation, st.turn_over);
                (f.w, f.h) = o.size();
                f.outline = Some(o);
            }
            FormatSlot { format: f, qty: if st.allow_no_stock { None } else { Some(it.qty.max(0) as usize) } }
        })
        .collect();
    if st.fallback_job_size {
        plan.push(FormatSlot { format: SheetFormat::rect(spec.w, spec.h), qty: None });
    }
    Some(plan)
}

/// The items Nest all may take when a size was picked for the job (ST18):
/// whole sheets of that size only; otherwise all of them.
fn only_picked(items: Vec<Item>, spec: &SheetSpec) -> Vec<Item> {
    if spec.pick != SheetPick::Size {
        return items;
    }
    items.into_iter().filter(|i| !i.kind.is_remnant() && spec.is_size(i.length, i.width)).collect()
}

/// The stock items Do All may take for this job, in the order it takes them
/// (SS4–SS6, SS12): each with what's in stock plus what this job already
/// holds; remnants first when the options say so.
fn usable_items(stock: &Stock, spec: &SheetSpec, u: &StockUse, exclude: &[String]) -> Vec<Item> {
    let st = u.st;
    let held = |r: &str| u.held.get(r).copied().unwrap_or(0);
    // What this job holds is available to it again (SS12).
    let mut s = stock.clone();
    for it in s.items.iter_mut() {
        it.qty = (it.qty + held(&it.reference)).min(stock::MAX_QTY);
    }
    let excl: Vec<String> = exclude.iter().chain(u.exclude.iter()).cloned().collect();
    let cands = s.candidates(spec.material, spec.thickness, &st.criteria, false, 1, st.allow_no_stock, &excl);
    let (rem, sht): (Vec<&Item>, Vec<&Item>) = cands.iter().partition(|i| i.kind.is_remnant());
    let order = |mut v: Vec<&Item>| -> Vec<Item> {
        let mut out = vec![];
        while let Some(n) = s.next_sheet(&v, st.order) {
            out.push(n.clone());
            v.retain(|i| i.reference != n.reference);
        }
        out
    };
    if st.remnants_first {
        order(rem).into_iter().chain(order(sht)).collect()
    } else {
        order(cands.clone())
    }
}

/// What Nest all will nest on, said before nesting (the Sheet tab's "Nest
/// on" line, the Home tab and agents' `nest-on`): the same choice `nest_all`
/// and `stock_plan` make, so the job's typed sheet size is never shown as the
/// sheet when the stock will be used first.
#[derive(Clone, Debug, PartialEq)]
pub enum NestOnMode {
    /// From the stock, then the job's sheet size when it runs out (STK-10).
    StockThenSize,
    /// From the stock only: what doesn't fit stays unnested.
    StockOnly,
    /// The job's sheet size: the Do All options don't use the stock.
    StockOff,
    /// The job's sheet size: part groups nest in their own zones, without
    /// stock (STK-11).
    Grouped,
    /// The job's sheet size: the stock has nothing for this material and thickness.
    NoneInStock,
    /// Nothing: the stock has nothing for this job and the options don't fall
    /// back to the job's sheet size.
    NoSheets,
    /// The job's sheet size: the Library's stock can't be reached or read (LB11).
    Unreachable(String),
    /// The job's sheet size: this PC has no Library yet, so no stock.
    NoLibrary,
}

/// One stocked sheet size and how many of it Do All may take.
#[derive(Clone, Debug, PartialEq)]
pub struct StockSize {
    pub length: f64,
    pub width: f64,
    pub qty: i32,
}

/// What Nest all will nest on (see `NestOnMode`), with what the stock holds
/// for the job.
#[derive(Clone, Debug, PartialEq)]
pub struct NestOn {
    pub mode: NestOnMode,
    /// Sheets and remnants Do All may take (quantities; locked items and
    /// what the criteria leave out aren't counted).
    pub sheets: i32,
    pub remnants: i32,
    /// The stocked sheet sizes (not remnants), largest first — every size,
    /// also when one was picked, for the size list.
    pub sizes: Vec<StockSize>,
    /// A size was picked for the job (ST18): Nest all nests only on it, and
    /// `sheets` counts the stock's sheets of that size.
    pub picked: bool,
}

impl NestOn {
    /// The mode's key for agents.
    pub fn key(&self) -> &'static str {
        match self.mode {
            NestOnMode::StockThenSize => "stock-then-size",
            NestOnMode::StockOnly => "stock-only",
            NestOnMode::StockOff => "size-stock-off",
            NestOnMode::Grouped => "size-groups",
            NestOnMode::NoneInStock => "size-none-in-stock",
            NestOnMode::NoSheets => "nothing",
            NestOnMode::Unreachable(_) => "size-stock-unreachable",
            NestOnMode::NoLibrary => "size-no-library",
        }
    }

    /// Does the stock come first?
    pub fn from_stock(&self) -> bool {
        matches!(self.mode, NestOnMode::StockThenSize | NestOnMode::StockOnly)
    }

    /// Worth a warning colour: nothing to nest on, or the stock can't be read.
    pub fn warn(&self) -> bool {
        matches!(self.mode, NestOnMode::NoSheets | NestOnMode::Unreachable(_))
    }

    /// "14 sheets · 2 remnants" (what's in stock for the job).
    pub fn counts(&self) -> String {
        if self.picked {
            return format!("{} in stock", self.sheets);
        }
        let n = |k: i32, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
        match (self.sheets, self.remnants) {
            (s, 0) => n(s, "sheet", "sheets"),
            (0, r) => n(r, "remnant", "remnants"),
            (s, r) => format!("{} · {}", n(s, "sheet", "sheets"), n(r, "remnant", "remnants")),
        }
    }

    /// One short line: where the sheets come from; `size` is the job's
    /// sheet size as shown ("78.74 × 39.37").
    pub fn line(&self, size: &str) -> String {
        if self.picked {
            match &self.mode {
                NestOnMode::StockThenSize => return format!("{size} only: {} first, then more", self.counts()),
                NestOnMode::StockOnly => return format!("{size} only, from stock ({})", self.counts()),
                NestOnMode::NoneInStock => return format!("{size} only (none in stock)"),
                NestOnMode::NoSheets => return format!("Nothing: no {size} in stock"),
                _ => {}
            }
        }
        match &self.mode {
            NestOnMode::StockThenSize => format!("Stock first ({}), then {size}", self.counts()),
            NestOnMode::StockOnly => format!("Stock only ({})", self.counts()),
            NestOnMode::StockOff => format!("{size} (stock not used)"),
            NestOnMode::Grouped => format!("{size} (groups don't use stock)"),
            NestOnMode::NoneInStock => format!("{size} (none in stock)"),
            NestOnMode::NoSheets => "Nothing: none in stock".into(),
            NestOnMode::Unreachable(_) => format!("{size} (stock not reachable)"),
            NestOnMode::NoLibrary => format!("{size} (no Library yet)"),
        }
    }

    /// Shorter, for the Home tab: "From stock: 14 sheets", or "78.74 × 39.37 in sheet".
    pub fn short(&self, size: &str) -> String {
        if self.picked {
            match &self.mode {
                NestOnMode::StockThenSize | NestOnMode::StockOnly => return format!("{size} sheet · {}", self.counts()),
                NestOnMode::NoSheets => return format!("No {size} in stock"),
                _ => {}
            }
        }
        match &self.mode {
            NestOnMode::StockThenSize | NestOnMode::StockOnly => format!("From stock: {}", self.counts()),
            NestOnMode::NoSheets => "None in stock".into(),
            NestOnMode::Unreachable(_) => "Stock not reachable".into(),
            _ => format!("{size} sheet"),
        }
    }

    /// Why, and what to change, in a sentence or two.
    pub fn explain(&self) -> String {
        if self.picked {
            let back = "Pick \"Any size in stock\" in the size list ▾ on the Sheet tab to let Nest all take any size in stock again.";
            match &self.mode {
                NestOnMode::StockThenSize => {
                    return format!("This size was picked for the job, so Nest all nests only on it: the {} for this material and thickness first, then more of this size when they run out. {back}", self.counts());
                }
                NestOnMode::StockOnly => {
                    return format!("This size was picked for the job, so Nest all nests only on it, and only on the {} (the Do All options don't go on to sheets that aren't in stock): parts that don't fit stay unnested. {back}", self.counts());
                }
                NestOnMode::NoneInStock => return format!("This size was picked for the job, so Nest all nests only on it. None of it is in stock for this material and thickness. {back}"),
                NestOnMode::NoSheets => {
                    return format!("This size was picked for the job, but none of it is in stock for this material and thickness, and the Do All options don't go on to sheets that aren't in stock: add it to stock, pick a size that is, or turn that option on (Sheet stock → Options). {back}");
                }
                _ => {}
            }
        }
        match &self.mode {
            NestOnMode::StockThenSize => format!(
                "Nest all takes the sheets and remnants in stock for this material and thickness first ({}), then this job's sheet size when they run out. Change it in Sheet stock → Options.",
                self.counts()
            ),
            NestOnMode::StockOnly => format!(
                "Nest all uses only the sheets and remnants in stock for this material and thickness ({}); parts that don't fit on them stay unnested. Change it in Sheet stock → Options.",
                self.counts()
            ),
            NestOnMode::StockOff => "Nest all nests on this job's sheet size: the Do All options don't use the sheet stock (Sheet stock → Options).".into(),
            NestOnMode::Grouped => "With Group by attribute on and several groups, each group is nested in its own zone on this job's sheet size; the sheet stock isn't used.".into(),
            NestOnMode::NoneInStock => "Nothing in stock for this material and thickness, so Nest all nests on this job's sheet size.".into(),
            NestOnMode::NoSheets => "Nothing in stock for this material and thickness, and the Do All options don't fall back to the job's sheet size: add sheets to stock, or turn that option on (Sheet stock → Options).".into(),
            NestOnMode::Unreachable(e) => format!("Nest all would take sheets from stock, but the stock can't be read, so it nests on this job's sheet size. {e}"),
            NestOnMode::NoLibrary => format!("Nest all nests on this job's sheet size: {}", crate::library::unreachable(&crate::library::default_dir())),
        }
    }
}

/// What Nest all will nest on for this job (see `NestOn`). `grouped`: Group
/// by attribute is on and the parts have several groups. Reads the stock
/// file, so the window asks only when something changed.
pub fn nest_on(spec: &SheetSpec, u: &StockUse, grouped: bool) -> NestOn {
    let picked = spec.pick == SheetPick::Size;
    let none = |mode| NestOn { mode, sheets: 0, remnants: 0, sizes: vec![], picked };
    if grouped {
        return none(NestOnMode::Grouped);
    }
    if !u.st.use_stock {
        return none(NestOnMode::StockOff);
    }
    // This PC's own default folder that was never made: no stock yet, not a
    // share that's down.
    if !u.dir.is_dir() && u.dir == crate::library::default_dir() {
        return none(NestOnMode::NoLibrary);
    }
    if let Err(e) = crate::library::usable(&u.dir) {
        return none(NestOnMode::Unreachable(e));
    }
    let stock = match Stock::load(&u.dir) {
        Ok(s) => s,
        Err(e) => return none(NestOnMode::Unreachable(format!("The stock file can't be read: {e}"))),
    };
    let all = usable_items(&stock, spec, u, &[]);
    let mut sizes: Vec<StockSize> = vec![];
    for it in all.iter().filter(|i| !i.kind.is_remnant()) {
        let (l, w) = (it.length, it.width);
        match sizes.iter_mut().find(|s| (s.length - l).abs() < nest::SIZE_TOL && (s.width - w).abs() < nest::SIZE_TOL) {
            Some(s) => s.qty += it.qty.max(0),
            None => sizes.push(StockSize { length: l, width: w, qty: it.qty.max(0) }),
        }
    }
    sizes.sort_by(|a, b| (b.length * b.width).total_cmp(&(a.length * a.width)));
    let items = only_picked(all, spec);
    if items.is_empty() {
        return NestOn { sizes, ..none(if u.st.fallback_job_size { NestOnMode::NoneInStock } else { NestOnMode::NoSheets }) };
    }
    let (rem, sht): (Vec<&Item>, Vec<&Item>) = items.iter().partition(|i| i.kind.is_remnant());
    let count = |v: &[&Item]| v.iter().map(|i| i.qty.max(0)).sum::<i32>();
    NestOn {
        mode: if u.st.fallback_job_size { NestOnMode::StockThenSize } else { NestOnMode::StockOnly },
        sheets: count(&sht),
        remnants: count(&rem),
        sizes,
        picked,
    }
}

/// Said after Do All when it would have nested from the sheet stock but the
/// Library can't be reached (LB11): it nested on the job's sheet instead.
pub fn stock_unreachable_note(u: &StockUse) -> Option<String> {
    if !u.st.use_stock {
        return None;
    }
    crate::library::usable(&u.dir).err().map(|e| format!(" The sheet stock wasn't used: {e}"))
}

/// Nest from stock, dropping stock sheets below the minimum efficiency and
/// nesting again without them (SS7), with the Do All searches the options ask
/// for (ST13): the best sheet combination (each sheet used less than its
/// minimum efficiency is tried in the other sizes, UI22) and the best sheet
/// format (the whole job on each stocked size in turn, UI21), within the
/// maximum search time. None when stock isn't used or has nothing for this job.
pub fn nest_from_stock(parts: &[Part], spec: &SheetSpec, u: &StockUse) -> Option<NestResult> {
    let st = u.st;
    let start = std::time::Instant::now();
    let out_of_time = || start.elapsed().as_secs_f64() > st.max_seconds.max(1.0);
    let combo = st.best_combination.then_some(st.combination_min);
    let mut exclude: Vec<String> = vec![];
    let mut res = None;
    for _ in 0..8 {
        let plan = stock_plan(spec, u, &exclude)?;
        if plan.is_empty() && res.is_some() {
            // Every other sheet is dropped too: keep the last nesting.
            break;
        }
        let mut r = nest::nest_plan_opt(parts, spec, &plan, combo);
        if st.best_format {
            r = best_format(parts, spec, &plan, combo, r, &out_of_time);
        }
        let low: Vec<String> = r
            .sheets
            .iter()
            .zip(&r.formats)
            .filter(|(s, f)| !f.stock_ref.is_empty() && st.min_efficiency > 0.0 && !stock::keeps_sheet(used_area(parts, s, f, st.eff_mode), f.area_mm2(), st.min_efficiency))
            .map(|(_, f)| f.stock_ref.clone())
            .collect();
        res = Some(r);
        if low.is_empty() || out_of_time() {
            break;
        }
        exclude.extend(low);
    }
    res
}

/// How good a whole-job result is for "best sheet format" (UI21): every part
/// placed first, then less waste (sheet area less part area, within 1 mm²),
/// then the higher worst-sheet efficiency.
fn format_score(parts: &[Part], r: &NestResult) -> (bool, f64, f64) {
    let need: usize = parts.iter().map(|p| p.qty as usize).sum();
    let placed: usize = r.sheets.iter().map(|s| s.len()).sum();
    let net = |s: &[Placed]| s.iter().map(|p| parts[p.part].net_area()).sum::<f64>();
    let waste: f64 = r.sheets.iter().zip(&r.formats).map(|(s, f)| f.area_mm2() - net(s)).sum();
    let worst = r.sheets.iter().zip(&r.formats).map(|(s, f)| net(s) / f.area_mm2().max(1.0)).fold(1.0, f64::min);
    (placed >= need, -waste, worst)
}

fn format_better(a: &(bool, f64, f64), b: &(bool, f64, f64)) -> bool {
    (a.0 && !b.0) || (a.0 == b.0 && (a.1 > b.1 + 1.0 || ((a.1 - b.1).abs() <= 1.0 && a.2 > b.2)))
}

/// "Look for the best sheet format" (UI21): nest the whole job on each stocked
/// size (with the job's sheet after it when stock may run out) and keep the
/// best of those and `first` (the plan in stock order). Sizes not reached
/// before the time limit are skipped.
fn best_format(parts: &[Part], spec: &SheetSpec, plan: &[FormatSlot], combo: Option<f64>, first: NestResult, out_of_time: &dyn Fn() -> bool) -> NestResult {
    let key = |f: &SheetFormat| (f.w, f.h, f.remnant);
    let fallback = plan.last().filter(|s| s.format.stock_ref.is_empty()).cloned();
    let mut best_s = format_score(parts, &first);
    let mut best = first;
    let mut seen: Vec<(f64, f64, bool)> = vec![];
    for slot in plan.iter().filter(|s| !s.format.stock_ref.is_empty()) {
        let k = key(&slot.format);
        if seen.contains(&k) || out_of_time() {
            continue;
        }
        seen.push(k);
        let mut only: Vec<FormatSlot> = plan.iter().filter(|s| !s.format.stock_ref.is_empty() && key(&s.format) == k).cloned().collect();
        only.extend(fallback.clone());
        let t = nest::nest_plan_opt(parts, spec, &only, combo);
        let s = format_score(parts, &t);
        if format_better(&s, &best_s) {
            best_s = s;
            best = t;
        }
    }
    best
}

/// "Used area" of a sheet the way the efficiency check counts it (SS7).
pub fn used_area(parts: &[Part], sheet: &[Placed], f: &SheetFormat, mode: EffMode) -> f64 {
    match mode {
        EffMode::RealArea => sheet.iter().map(|p| parts[p.part].net_area()).sum(),
        EffMode::ExternalArea => sheet.iter().map(|p| nest::area(&nest::placed_shape(parts, p).0).abs()).sum(),
        EffMode::RectangularArea => sheet.iter().map(|p| p.w * p.h).sum(),
        EffMode::PartsBox => {
            let mx = sheet.iter().map(|p| p.x + p.w).fold(0.0, f64::max).min(f.w);
            let my = sheet.iter().map(|p| p.y + p.h).fold(0.0, f64::max).min(f.h);
            mx * my
        }
    }
}

/// The stock sheets the nested sheets are, per reference (empty sheets too:
/// a sheet given to a nesting is taken, SS10).
pub fn sheets_used(formats: &[SheetFormat], n_sheets: usize) -> BTreeMap<String, i32> {
    let mut used: BTreeMap<String, i32> = BTreeMap::new();
    for f in formats.iter().take(n_sheets) {
        if !f.stock_ref.is_empty() {
            *used.entry(f.stock_ref.clone()).or_default() += 1;
        }
    }
    used
}

/// Why a job's sheets weren't taken from stock.
#[derive(Clone, Debug, PartialEq)]
pub enum NotTaken {
    /// A sheet the job is nested on is gone: who took it and when (SV10).
    Gone(String),
    /// The stock couldn't be changed (missing sheet, busy Library…).
    Failed(String),
}

impl std::fmt::Display for NotTaken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotTaken::Gone(e) | NotTaken::Failed(e) => f.write_str(e),
        }
    }
}

/// Take the sheets the job now uses from stock and give back the ones it no
/// longer uses (SS10, SS11), bringing `held` up to date, as `lib`'s user and
/// PC (SV9). Returns a note ("" when nothing changed). Nothing changes when a
/// sheet it needs is gone; the refusal says who took it (SV10).
pub fn reconcile(lib: &Library, formats: &[SheetFormat], n_sheets: usize, held: &mut BTreeMap<String, i32>, allow_neg: bool) -> Result<String, NotTaken> {
    let used = sheets_used(formats, n_sheets);
    if &used == held {
        return Ok(String::new());
    }
    // Each reference once (one in both lists would otherwise move twice).
    let refs: std::collections::BTreeSet<String> = used.keys().chain(held.keys()).cloned().collect();
    let before = held.clone();
    let (mut taken, mut back) = (0, 0);
    let mut gone = None;
    Stock::update(lib, |s| {
        for r in &refs {
            let d = used.get(r).copied().unwrap_or(0) - before.get(r).copied().unwrap_or(0);
            if d > 0 {
                if let Err(e) = s.take(r, d, allow_neg) {
                    gone = s.get(r).and_then(|it| it.gone_note(d, &lib.clock));
                    return Err(e);
                }
                taken += d;
            } else if d < 0 {
                s.give_back(r, -d);
                back += -d;
            }
        }
        Ok(())
    })
    .map_err(|e| gone.map_or(NotTaken::Failed(e), NotTaken::Gone))?;
    *held = used;
    let mut v = vec![];
    if taken > 0 {
        v.push(format!("{taken} sheet(s) taken from stock"));
    }
    if back > 0 {
        v.push(format!("{back} given back"));
    }
    Ok(v.join(", "))
}

/// A job's nested sheets as the remnant generation sees them.
pub struct JobSheets<'a> {
    pub name: &'a str,
    pub parts: &'a [Part],
    pub spec: &'a SheetSpec,
    pub sheets: &'a [Vec<Placed>],
    pub cut_lines: &'a [Vec<CutLine>],
    /// The job material's name as programs call it ([MAT], mark #400).
    pub material_name: String,
}

/// The material's name from the settings (the built-in name when none is set).
pub fn material_name(cfg: &crate::settings::AppSettings, m: usize) -> String {
    cfg.materials.get(m).map(|x| x.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| crate::cnc::MATERIALS[m.min(crate::cnc::MATERIALS.len() - 1)].to_string())
}

/// What a reference format can use for a remnant of sheet `i` (RF4–RF5).
fn ref_values(j: &JobSheets, f: &SheetFormat, i: usize, parent: Option<&Item>, o: &crate::stock::Outline) -> stock::RefValues {
    let (l, w) = o.size();
    stock::RefValues { material: j.material_name.clone(), user_data: parent.map(|p| p.user_data.clone()).unwrap_or_default(), sheet: origin_of(j, f, i), length: l, width: w, thickness: j.spec.thickness, decimals: 0 }
}

/// The next reference for a new remnant of sheet `i`: the automatic remnant
/// format when it's on, else `<origin>-<number>` (RF2).
pub fn next_ref(s: &mut Stock, j: &JobSheets, f: &SheetFormat, i: usize, parent: Option<&Item>, o: &crate::stock::Outline, st: &StockSettings) -> String {
    let v = ref_values(j, f, i, parent, o);
    let fmt = if st.auto_remnant_ref { st.remnant_ref_format.as_str() } else { "" };
    s.remnant_ref_from(fmt, &v)
}

/// The references sheet `i`'s remnant pieces have, or will get when stored.
pub fn remnant_refs(stock: &Stock, j: &JobSheets, formats: &[SheetFormat], i: usize, pieces: &[crate::stock::Outline], st: &StockSettings) -> Vec<String> {
    let f = formats.get(i).cloned().unwrap_or_else(|| SheetFormat::rect(j.spec.w, j.spec.h));
    let lines = j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice());
    let sheet = j.sheets.get(i).map_or(&[][..], |v| v.as_slice());
    let rebuild = stored_state(stock, &f, lines, sheet) == Stored::Rebuild;
    if !rebuild && f.made.len() == pieces.len() {
        return f.made.clone();
    }
    // What the next store names them: the sheets up to this one, in store
    // order (the sheets after it don't change its names).
    let upto = (i + 1).min(j.sheets.len());
    let head = JobSheets { name: j.name, parts: j.parts, spec: j.spec, sheets: &j.sheets[..upto], cut_lines: &j.cut_lines[..upto.min(j.cut_lines.len())], material_name: j.material_name.clone() };
    let named: Vec<String> = rows_from(stock, &head, &formats[..upto.min(formats.len())], st, false).into_iter().filter(|r| r.sheet == i).map(|r| r.reference).collect();
    if named.len() == pieces.len() {
        return named;
    }
    // Not stored by the next save (storing is off for it): a likely name.
    let f = &f;
    let mut s = stock.clone();
    if rebuild {
        // As storing will: the sheet's old remnants go and keep their names (RN6).
        for r in &f.made {
            s.delete(r);
            s.replaced.insert(r.clone(), vec![]);
        }
    }
    let parent = s.get(&f.stock_ref).cloned();
    pieces.iter().map(|o| next_ref(&mut s, j, f, i, parent.as_ref(), o, st)).collect()
}

/// The automatic marks on sheet `i`'s remnants, in sheet coordinates (RF9–RF10;
/// cutting machines only). Engraved before the remnant line.
pub fn remnant_marks(stock: &Stock, j: &JobSheets, formats: &[SheetFormat], i: usize, st: &StockSettings) -> Vec<Vec<crate::nest::P>> {
    let ms = &st.mark;
    if !ms.on || j.spec.machine == crate::machine::MachineKind::Punch {
        return vec![];
    }
    let lines = j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice());
    if !lines.iter().any(|l| l.kind == CutKind::Remnant) {
        return vec![];
    }
    let Some(sheet) = j.sheets.get(i) else { return vec![] };
    let f = formats.get(i).cloned().unwrap_or_else(|| SheetFormat::rect(j.spec.w, j.spec.h));
    let pieces = crate::remnant::sheet_remnants_cached(j.parts, sheet, &f, lines, crate::kerf::kerf_mm(&j.spec));
    let refs = remnant_refs(stock, j, formats, i, &pieces, st);
    let mut out = vec![];
    for (k, o) in pieces.iter().enumerate() {
        let (l, w) = o.size();
        let v = crate::remmark::MarkValues {
            reference: refs.get(k).cloned().unwrap_or_default(),
            length: l,
            width: w,
            area_m2: o.area_mm2() / 1e6,
            weight_kg: crate::cnc::weight_kg(j.spec.material, o.area_mm2(), j.spec.thickness),
            sheet: origin_of(j, &f, i),
            sheet_user_data: Default::default(),
            material: j.material_name.clone(),
            thickness: j.spec.thickness,
            date: stock::today(),
            time: String::new(),
        };
        let text = crate::remmark::expand_text(&ms.text, &v);
        if let Some(st) = crate::remmark::place(o, &text, ms) {
            out.extend(st);
        }
    }
    out
}

/// Every sheet's remnant marks (for exporting all programs).
pub fn all_remnant_marks(dir: &Path, j: &JobSheets, formats: &[SheetFormat], st: &StockSettings) -> Vec<Vec<Vec<crate::nest::P>>> {
    if !st.mark.on {
        return vec![];
    }
    let stock = Stock::load_cached(dir);
    (0..j.sheets.len()).map(|i| remnant_marks(&stock, j, formats, i, st)).collect()
}

/// A fingerprint of what a sheet's stored remnants were cut from
/// (`SheetFormat::made_from`, ST12): its remnant lines — and, on a sheet
/// without one (a completed sheet kept whole, its parts cut out), where its
/// parts are, since moving or re-nesting them changes that remnant (RN4).
/// Always odd; 0 = stored before fingerprints were kept.
pub fn sheet_fingerprint(lines: &[CutLine], sheet: &[Placed]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut has_line = false;
    for l in lines.iter().filter(|l| l.kind == CutKind::Remnant) {
        has_line = true;
        for p in &l.pts {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    if !has_line {
        for c in sheet {
            (c.part, c.x.to_bits(), c.y.to_bits(), c.turns, c.mirror).hash(&mut h);
            // A free angle (HM14) only when there is one, so sheets saved
            // before free angles keep their fingerprint.
            if !c.spin.is_none() {
                c.spin.deg.to_bits().hash(&mut h);
            }
        }
    }
    h.finish() | 1
}

/// What older files stored for a sheet without a remnant line (the lines
/// only, so nothing): read as "unchanged", as 0 is.
fn legacy_no_line_fingerprint() -> u64 {
    use std::hash::Hasher;
    std::collections::hash_map::DefaultHasher::new().finish() | 1
}

/// Mark sheet remnants as cut from something else, so the next store
/// rebuilds them (replacing the old ones while still unused, ST12): after a
/// sheet is nested again (RN4).
pub fn remnants_outdated(f: &mut SheetFormat) {
    if !f.made.is_empty() {
        f.made_from = OUTDATED;
    }
}

/// Never a real fingerprint (those are odd).
const OUTDATED: u64 = 2;

/// Undo or redo past a store (RN6): the stock isn't rolled back. A restored
/// sheet whose stored remnants a later store replaced (recorded in
/// `Stock::replaced`) takes over what replaced them — followed through every
/// later replacement to what is in stock now — marked out of date, so the
/// next store rebuilds them for the restored layout instead of leaving them
/// behind untracked; replaced by nothing, its remnants are made again. A
/// remnant missing for any other reason (deleted by hand) is left alone, and
/// a remnant another restored sheet still holds is never taken over.
pub fn follow_stock(stock: &Stock, restored: &mut [SheetFormat]) {
    // Each history entry is expanded once (`seen`), so shared descendants
    // and any cycle cost nothing extra, however long the chain.
    fn resolve(stock: &Stock, r: &str, seen: &mut Vec<String>, out: &mut Vec<String>) {
        if seen.iter().any(|s| s == r) {
            return;
        }
        seen.push(r.to_string());
        match stock.replaced.get(r) {
            Some(next) if stock.get(r).is_none() => {
                for n in next {
                    resolve(stock, n, seen, out);
                }
            }
            _ => out.push(r.to_string()),
        }
    }
    let held: Vec<String> = restored.iter().flat_map(|f| f.made.iter()).filter(|r| stock.get(r).is_some()).cloned().collect();
    let mut taken: Vec<String> = vec![];
    for f in restored.iter_mut() {
        if !f.made.iter().any(|r| stock.get(r).is_none() && stock.replaced.contains_key(r)) {
            continue;
        }
        let mut now = vec![];
        for r in &f.made {
            if stock.get(r).is_some() {
                if !now.contains(r) {
                    now.push(r.clone());
                }
                continue;
            }
            let (mut seen, mut found) = (vec![], vec![]);
            resolve(stock, r, &mut seen, &mut found);
            for n in found {
                let live = stock.get(&n).is_some();
                // Exclusive: never one another restored sheet holds or this
                // pass already gave out.
                if live && (held.contains(&n) || taken.contains(&n)) {
                    continue;
                }
                if !now.contains(&n) {
                    if live {
                        taken.push(n.clone());
                    }
                    // A chain ending at a remnant deleted by hand keeps that
                    // reference, so it isn't remade (as before).
                    now.push(n);
                }
            }
        }
        f.made = now;
        f.made_from = if f.made.is_empty() { 0 } else { OUTDATED };
    }
}

/// One sheet's remnants to store: the new outlines and the remnants stored
/// from it before that go first (its remnant lines changed).
#[derive(Clone, Debug)]
pub struct Pending {
    pub sheet: usize,
    pub pieces: Vec<stock::Outline>,
    pub replace: Vec<String>,
}

/// One new remnant with a suggested reference (the naming step, UI14).
#[derive(Clone, Debug)]
pub struct NameRow {
    pub sheet: usize,
    pub piece: usize,
    pub reference: String,
    pub w: f64,
    pub h: f64,
    pub area_m2: f64,
}

/// The remnants storing would make now (RE1, ST12): the sheets with a remnant
/// line, or with "completed sheets" every nested sheet. A sheet whose remnants
/// are stored already is skipped while its remnant lines are unchanged; when
/// they changed, its old remnants are replaced — unless one of them has been
/// used since: then they stay, and the sheet is in the second list.
/// `formats` must have one per sheet.
/// What storing will do with the remnants already stored from a sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stored {
    /// None stored yet.
    None,
    /// Stored from what the sheet still is: left as they are.
    Unchanged,
    /// The sheet changed but one of them is already used: kept (ST12).
    KeptUsed,
    /// The sheet changed: they are replaced by new ones (ST12, RN4).
    Rebuild,
}

/// One rule for storing and for previewing names (remnant marks), so the
/// two never disagree.
pub fn stored_state(stock_now: &Stock, f: &SheetFormat, lines: &[CutLine], sheet: &[Placed]) -> Stored {
    if f.made.is_empty() {
        return Stored::None;
    }
    let has_line = lines.iter().any(|l| l.kind == CutKind::Remnant);
    if f.made_from == sheet_fingerprint(lines, sheet) || f.made_from == 0 || (!has_line && f.made_from == legacy_no_line_fingerprint()) {
        // Unchanged (or stored before fingerprints were kept).
        return Stored::Unchanged;
    }
    if !f.made.iter().all(|r| stock_now.get(r).is_some_and(|it| it.qty >= 1)) {
        return Stored::KeptUsed;
    }
    Stored::Rebuild
}

pub fn pending_remnants(stock_now: &Stock, j: &JobSheets, formats: &[SheetFormat], st: &StockSettings, force_lines: bool) -> (Vec<Pending>, Vec<usize>) {
    let mut todo = vec![];
    let mut kept_used = vec![];
    for i in 0..j.sheets.len() {
        let Some(f) = formats.get(i) else { continue };
        if j.sheets[i].is_empty() {
            continue;
        }
        let lines = j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice());
        let replace = match stored_state(stock_now, f, lines, &j.sheets[i]) {
            Stored::Unchanged => continue,
            Stored::KeptUsed => {
                kept_used.push(i);
                continue;
            }
            Stored::Rebuild => f.made.clone(),
            Stored::None => vec![],
        };
        let has_line = lines.iter().any(|l| l.kind == CutKind::Remnant);
        let want = (has_line && (st.remnants_on_save || force_lines)) || st.keep_completed;
        let pieces = if want { crate::remnant::sheet_remnants(j.parts, &j.sheets[i], f, lines, crate::kerf::kerf_mm(&j.spec), st.min_remnant_m2, st.keep_completed && !has_line) } else { vec![] };
        if !pieces.is_empty() || !replace.is_empty() {
            todo.push(Pending { sheet: i, pieces, replace });
        }
    }
    (todo, kept_used)
}

/// The sheet a remnant comes from, for its origin and suggested reference.
fn origin_of(j: &JobSheets, f: &SheetFormat, i: usize) -> String {
    if f.stock_ref.is_empty() { format!("{}-{:02}", j.name, i + 1) } else { f.stock_ref.clone() }
}

/// The new remnants storing would make, each with the reference it would get
/// (the rows of the naming window, UI14; `stock-book status` for agents).
pub fn remnant_rows(dir: &Path, j: &JobSheets, formats: &[SheetFormat], st: &StockSettings, force_lines: bool) -> Vec<NameRow> {
    rows_from(&Stock::load(dir).unwrap_or_default(), j, formats, st, force_lines)
}

/// `remnant_rows` on a stock already loaded: storing simulated on a copy,
/// sheet by sheet in store order, so every sheet's names follow the ones
/// the sheets before it take (RN6).
fn rows_from(stock: &Stock, j: &JobSheets, formats: &[SheetFormat], st: &StockSettings, force_lines: bool) -> Vec<NameRow> {
    let mut s = stock.clone();
    let (todo, _) = pending_remnants(&s, j, formats, st, force_lines);
    let mut rows = vec![];
    for p in &todo {
        for r in &p.replace {
            s.delete(r);
            s.replaced.insert(r.clone(), vec![]);
        }
        let f = &formats[p.sheet];
        let parent = s.get(&f.stock_ref).cloned();
        for (k, o) in p.pieces.iter().enumerate() {
            let (w, h) = o.size();
            let reference = next_ref(&mut s, j, f, p.sheet, parent.as_ref(), o, st);
            rows.push(NameRow { sheet: p.sheet, piece: k, reference, w, h, area_m2: o.area_mm2() / 1e6 });
        }
    }
    rows
}

/// What storing remnants did: how many were stored, and the sheets whose
/// changed remnant lines left their (already used) remnants as they were.
#[derive(Clone, Debug, Default)]
pub struct Made {
    pub count: usize,
    pub kept_used: Vec<usize>,
}

impl Made {
    pub fn kept_note(&self) -> String {
        if self.kept_used.is_empty() {
            return String::new();
        }
        format!("sheet(s) {} changed, but their remnants are already in use — they were kept", self.kept_used.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", "))
    }
}

/// Store the remnants `pending_remnants` finds (RE1, ST12) and note them in the
/// sheet's `made` / `made_from`. `force_lines`: store remnant lines' remnants
/// even when "generate remnants when you save" is off (an explicit booking).
/// `names`: references given for (sheet, piece) (Prompt for references, UI14);
/// the others are numbered.
pub fn make_remnants(lib: &Library, j: &JobSheets, formats: &mut Vec<SheetFormat>, st: &StockSettings, force_lines: bool, names: Option<&BTreeMap<(usize, usize), String>>) -> Result<Made, String> {
    while formats.len() < j.sheets.len() {
        formats.push(SheetFormat::rect(j.spec.w, j.spec.h));
    }
    let stock_now = Stock::load(&lib.dir).unwrap_or_default();
    let (todo, kept_used) = pending_remnants(&stock_now, j, formats, st, force_lines);
    let mut out = Made { count: 0, kept_used };
    if todo.is_empty() {
        return Ok(out);
    }
    let (material, thickness) = (j.spec.material, j.spec.thickness);
    let fs = formats.clone();
    let made: Vec<(usize, Vec<String>)> = Stock::update(lib, |s| {
        let mut out = vec![];
        for p in &todo {
            for r in &p.replace {
                s.delete(r);
                // Kept from now on, so no replacement takes its name (RN6);
                // the real successors are recorded below.
                s.replaced.insert(r.clone(), vec![]);
            }
            let f = &fs[p.sheet];
            // The sheet it came from: its stock item, or a stand-in for a
            // sheet that wasn't taken from stock.
            let parent = s.get(&f.stock_ref).cloned().unwrap_or_else(|| {
                let mut it = Item::sheet(&format!("{}-{:02}", j.name, p.sheet + 1), material, thickness, f.w, f.h, 0);
                it.priority = stock::DEFAULT_PRIORITY;
                it
            });
            let refs = p
                .pieces
                .iter()
                .enumerate()
                .map(|(k, o)| {
                    // A given name, else the format (or <origin>-<number>).
                    let r = names.and_then(|m| m.get(&(p.sheet, k)).cloned()).unwrap_or_else(|| next_ref(s, j, f, p.sheet, Some(&parent), o, st));
                    s.add_remnant(o.clone(), &parent, 1, Some(r), j.name)
                })
                .collect::<Vec<String>>();
            // Remember what replaced what, for undo (RN6).
            for r in &p.replace {
                s.replaced.insert(r.clone(), refs.clone());
            }
            out.push((p.sheet, refs));
        }
        Ok(out)
    })?;
    for (i, refs) in made {
        out.count += refs.len();
        let fp = sheet_fingerprint(j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice()), &j.sheets[i]);
        formats[i].made_from = if refs.is_empty() { 0 } else { fp };
        formats[i].made = refs;
    }
    Ok(out)
}

/// Before a job is written: put new remnants in stock and settle what the job
/// holds (what the window does on every save), as `lib`'s user and PC (SV9).
/// Returns the notes ("" when nothing changed).
#[allow(clippy::too_many_arguments)]
pub fn on_save(lib: &Library, j: &JobSheets, formats: &mut Vec<SheetFormat>, held: &mut BTreeMap<String, i32>, st: &StockSettings, force_lines: bool, names: Option<&BTreeMap<(usize, usize), String>>) -> String {
    let mut notes = vec![];
    // The sheets first: remnants of a sheet that couldn't be taken from
    // stock would be remnants of a sheet that isn't there (SV7).
    let (taken, stock_note) = match reconcile(lib, formats, j.sheets.len(), held, st.allow_no_stock) {
        Ok(s) => (true, s),
        // Another PC took a sheet this job is nested on (SV10).
        Err(NotTaken::Gone(e)) => (false, format!("{e}, so nothing was taken from stock (remnants not stored): nest again (Do All) to use what's in stock now")),
        Err(NotTaken::Failed(e)) => (false, format!("stock not updated: {e}; remnants not stored")),
    };
    if taken && (st.remnants_on_save || st.keep_completed || force_lines) {
        match make_remnants(lib, j, formats, st, force_lines, names) {
            Ok(m) => {
                if m.count > 0 {
                    notes.push(format!("{} remnant(s) put in stock", m.count));
                }
                let k = m.kept_note();
                if !k.is_empty() {
                    notes.push(k);
                }
            }
            Err(e) => notes.push(format!("remnants not stored: {e}")),
        }
    }
    if !stock_note.is_empty() {
        notes.push(stock_note);
    }
    notes.join("; ")
}

/// Forget a job's stock booking, leaving the stock as it is: it holds no
/// sheets and stored no remnants. A copy of a job then takes its own sheets
/// when it's saved, and the original keeps its own (LB9).
pub fn forget_booking(formats: &mut [SheetFormat], held: &mut BTreeMap<String, i32>) {
    held.clear();
    for f in formats.iter_mut() {
        f.made.clear();
        f.made_from = 0;
    }
}

/// Undo a job's stock booking: every sheet it holds back into stock, and the
/// remnants it stored taken out again while nothing else has used them (their
/// quantity is still the 1 they were stored with). Returns (sheets given back,
/// remnants removed, remnants left because they were used elsewhere).
pub fn release(lib: &Library, formats: &mut [SheetFormat], held: &mut BTreeMap<String, i32>) -> Result<(i32, usize, Vec<String>), String> {
    let made: Vec<String> = formats.iter().flat_map(|f| f.made.iter().cloned()).collect();
    let give: Vec<(String, i32)> = held.iter().map(|(r, n)| (r.clone(), *n)).collect();
    let (back, removed, kept) = Stock::update(lib, |s| {
        let mut back = 0;
        for (r, n) in &give {
            s.give_back(r, *n);
            back += n;
        }
        let (mut removed, mut kept) = (0, vec![]);
        for r in &made {
            match s.get(r).map(|it| it.qty) {
                Some(q) if q >= 1 => {
                    s.change_qty(r, -1)?;
                    removed += 1;
                }
                Some(_) => kept.push(r.clone()),
                None => {}
            }
        }
        Ok((back, removed, kept))
    })?;
    held.clear();
    for f in formats.iter_mut() {
        f.made.clear();
        f.made_from = 0;
    }
    Ok((back, removed, kept))
}

/// Is one more of stock item `reference` free for this job: in stock, plus
/// what the job holds, less the job's sheets already of it (`replacing`: a
/// sheet being changed, which no longer counts)?
pub fn one_free(item: &Item, held: &BTreeMap<String, i32>, formats: &[SheetFormat], n_sheets: usize, replacing: Option<usize>) -> bool {
    let free = item.qty + held.get(&item.reference).copied().unwrap_or(0);
    let used = formats.iter().take(n_sheets).enumerate().filter(|(k, f)| Some(*k) != replacing && f.stock_ref == item.reference).count() as i32;
    used < free
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Library in `dir`, as sam on PROG-1 changes it.
    fn as_sam(dir: &Path) -> Library {
        Library::at(dir, &crate::shared::Who::new("sam", "PROG-1"), &crate::shared::Clock::pc())
    }

    #[test]
    fn undo_follows_only_recorded_replacements() {
        // RN6
        let mut st = Stock::default();
        for r in ["Ra", "Rc", "Rb3"] {
            st.items.push(Item::sheet(r, 0, 3.0, 500.0, 500.0, 1));
        }
        st.replaced.insert("Rb".into(), vec!["Rb2".into()]);
        st.replaced.insert("Rb2".into(), vec!["Rb3".into()]);
        let fmt = |refs: &[&str], fp: u64| SheetFormat { made: refs.iter().map(|r| r.to_string()).collect(), made_from: fp, ..SheetFormat::rect(2000.0, 1000.0) };
        // A sheet deleted, then B re-nested twice: undo restores A, B, C.
        // B follows its own chain, never C's remnant by position.
        let mut r = vec![fmt(&["Ra"], 11), fmt(&["Rb"], 13), fmt(&["Rc"], 15)];
        follow_stock(&st, &mut r);
        assert_eq!((r[0].made.clone(), r[0].made_from), (vec!["Ra".to_string()], 11));
        assert_eq!((r[1].made.clone(), r[1].made_from), (vec!["Rb3".to_string()], OUTDATED));
        assert_eq!((r[2].made.clone(), r[2].made_from), (vec!["Rc".to_string()], 15));
        // Deleted by hand (no record): left alone, so it isn't remade.
        let mut r = vec![fmt(&["Gone"], 17)];
        follow_stock(&st, &mut r);
        assert_eq!((r[0].made.clone(), r[0].made_from), (vec!["Gone".to_string()], 17));
        // Replaced by nothing: made again.
        st.replaced.insert("Rn".into(), vec![]);
        let mut r = vec![fmt(&["Rn"], 19)];
        follow_stock(&st, &mut r);
        assert_eq!((r[0].made.is_empty(), r[0].made_from), (true, 0));
        // A remnant another restored sheet holds is never taken over.
        st.replaced.insert("Rx".into(), vec!["Rc".into()]);
        let mut r = vec![fmt(&["Rx"], 21), fmt(&["Rc"], 15)];
        follow_stock(&st, &mut r);
        assert_eq!((r[0].made.is_empty(), r[0].made_from), (true, 0));
        assert_eq!(r[1].made, vec!["Rc".to_string()]);
        // A long history of two-remnant sheets: every step is followed (no
        // depth limit) and each entry expanded once (no blow-up).
        let mut st = Stock::default();
        for k in 0..100 {
            let next = vec![format!("L{}a", k + 1), format!("L{}b", k + 1)];
            st.replaced.insert(format!("L{k}a"), next.clone());
            st.replaced.insert(format!("L{k}b"), next);
        }
        for r in ["L100a", "L100b"] {
            st.items.push(Item::sheet(r, 0, 3.0, 500.0, 500.0, 1));
        }
        let mut r = vec![fmt(&["L0a", "L0b"], 23)];
        let t = std::time::Instant::now();
        follow_stock(&st, &mut r);
        assert!(t.elapsed().as_secs() < 2);
        assert_eq!(r[0].made, vec!["L100a".to_string(), "L100b".to_string()]);
        // Names in the history are never given to a new item (A → B → C,
        // C live): not the replaced one, not the middle one, so undo can't
        // adopt an unrelated item and the chain to C stays whole.
        let mut st = Stock::default();
        st.replaced.insert("A".into(), vec!["B".into()]);
        st.replaced.insert("B".into(), vec!["C".into()]);
        st.items.push(Item::sheet("C", 0, 3.0, 500.0, 500.0, 1));
        for name in ["A", "B"] {
            assert!(st.add_sheet(Item::sheet(name, 0, 3.0, 500.0, 500.0, 1), true).unwrap_err().contains("replaced"));
        }
        let parent = Item::sheet("S", 0, 3.0, 2000.0, 1000.0, 1);
        let sq = crate::stock::Outline { outer: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]], holes: vec![] };
        let got = st.add_remnant(sq.clone(), &parent, 1, Some("B".into()), "DEMO-001");
        assert_ne!(got, "B", "a remnant asked to be called B gets another name");
        let v = crate::stock::RefValues { sheet: "S".into(), ..Default::default() };
        assert_ne!(st.remnant_ref_from("A", &v), "A", "a fixed format landing on a kept name too");
        let mut r = vec![fmt(&["A"], 25)];
        follow_stock(&st, &mut r);
        assert_eq!((r[0].made.clone(), r[0].made_from), (vec!["C".to_string()], OUTDATED));
        // Automatic sheet numbers skip kept names: blank references keep
        // working, through the same stock update the window and agents use.
        let dir = std::env::temp_dir().join(format!("nestpro-reserved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::library::create(&dir).unwrap();
        let mut st = Stock::default();
        st.replaced.insert("1".into(), vec!["R".into()]);
        st.save(&dir).unwrap();
        for _ in 0..2 {
            let got = Stock::update(&as_sam(&dir), |s| s.add_sheet(Item::sheet("", 0, 3.0, 2000.0, 1000.0, 1), true)).unwrap();
            assert_ne!(got, "1");
        }
        assert_eq!(Stock::load(&dir).unwrap().items.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
        // Automatic references skip names still in the history.
        let mut st = Stock::default();
        let first = st.next_remnant_ref("S");
        st.replaced.insert("X".into(), vec![format!("S-{}", crate::stock::base36(st.remnant_counter + 1))]);
        let second = st.next_remnant_ref("S");
        assert!(first != second && !st.replaced["X"].contains(&second), "{first} {second}");
    }

    #[test]
    fn a_rebuilt_remnant_never_takes_its_predecessors_name() {
        // RN6: a fixed reference format ("R"), the remnant rebuilt, the new
        // one deleted by hand, then undo: it must not be remade.
        let dir = std::env::temp_dir().join(format!("nestpro-selfloop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::library::create(&dir).unwrap();
        Stock::default().save(&dir).unwrap();
        let part = Part { name: "P".into(), shape: nest::Shape::Rectangle, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let st = StockSettings { keep_completed: true, auto_remnant_ref: true, remnant_ref_format: "R".into(), ..Default::default() };
        let parts = [part];
        let store = |sheet: Placed, formats: &mut Vec<SheetFormat>| {
            let sheets = [vec![sheet]];
            let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: &[], material_name: String::new() };
            make_remnants(&as_sam(&dir), &j, formats, &st, false, None).unwrap()
        };
        let p = Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false, spin: Default::default() };
        let mut formats = vec![SheetFormat::rect(1000.0, 500.0)];
        store(p, &mut formats);
        assert_eq!(formats[0].made, vec!["R".to_string()]);
        let first = formats.clone();
        store(Placed { x: 400.0, ..p }, &mut formats);
        let second = formats[0].made.clone();
        assert_eq!(second.len(), 1);
        assert_ne!(second[0], "R", "the replacement gets its own name");
        let stock = Stock::load(&dir).unwrap();
        assert_eq!(stock.replaced["R"], second, "R → its real successor, no self-loop");
        // Deleted by hand, then undo to the first layout.
        Stock::update(&as_sam(&dir), |s| {
            s.delete(&second[0]);
            Ok(())
        })
        .unwrap();
        let stock = Stock::load(&dir).unwrap();
        let mut restored = first;
        follow_stock(&stock, &mut restored);
        let sheets = [vec![p]];
        let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: &[], material_name: String::new() };
        let (todo, _) = pending_remnants(&stock, &j, &restored, &st, false);
        assert!(todo.is_empty(), "the hand-deleted remnant is not remade");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marks_naming_and_storing_agree_on_a_rebuilt_remnant() {
        // RN6: one remnant "R" stored from a remnant line; the sheet changes
        // (still one piece). Marks, the naming suggestion and the stored name
        // must all be the same new name, never the outgoing "R".
        let dir = std::env::temp_dir().join(format!("nestpro-marknames-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::library::create(&dir).unwrap();
        Stock::default().save(&dir).unwrap();
        let part = Part { name: "P".into(), shape: nest::Shape::Rectangle, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let st = StockSettings { remnants_on_save: true, auto_remnant_ref: true, remnant_ref_format: "R".into(), ..Default::default() };
        let parts = [part];
        let line = |x: f64| vec![CutLine { pts: vec![[x, 0.0], [x, 500.0]], kind: CutKind::Remnant }];
        let p = Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false, spin: Default::default() };
        let sheets = [vec![p]];
        let mut formats = vec![SheetFormat::rect(1000.0, 500.0)];
        let lines1 = [line(300.0)];
        let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: &lines1, material_name: String::new() };
        make_remnants(&as_sam(&dir), &j, &mut formats, &st, false, None).unwrap();
        assert_eq!(formats[0].made, vec!["R".to_string()]);
        // The line moves: still one piece, to be rebuilt.
        let lines2 = [line(400.0)];
        let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: &lines2, material_name: String::new() };
        let stock = Stock::load(&dir).unwrap();
        let pieces = crate::remnant::sheet_remnants(&parts, &sheets[0], &formats[0], &lines2[0], crate::kerf::kerf_mm(&spec), st.min_remnant_m2, false);
        assert_eq!(pieces.len(), 1);
        let marked = remnant_refs(&stock, &j, &formats, 0, &pieces, &st);
        let suggested: Vec<String> = remnant_rows(&dir, &j, &formats, &st, false).into_iter().map(|r| r.reference).collect();
        make_remnants(&as_sam(&dir), &j, &mut formats, &st, false, None).unwrap();
        assert_ne!(marked, vec!["R".to_string()], "marks don't engrave the outgoing name");
        assert_eq!(marked, suggested);
        assert_eq!(marked, formats[0].made, "what is engraved is what is stored");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marks_on_several_sheets_follow_the_store_order() {
        // RN6: two sheets both storing (new or rebuilt) remnants with
        // automatic numbers: each sheet's preview is what storing gives it.
        let dir = std::env::temp_dir().join(format!("nestpro-marknames2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::library::create(&dir).unwrap();
        Stock::default().save(&dir).unwrap();
        let part = Part { name: "P".into(), shape: nest::Shape::Rectangle, w: 100.0, h: 50.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let st = StockSettings { remnants_on_save: true, ..Default::default() };
        let parts = [part];
        let line = |x: f64| vec![CutLine { pts: vec![[x, 0.0], [x, 500.0]], kind: CutKind::Remnant }];
        let p = Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false, spin: Default::default() };
        let sheets = [vec![p], vec![p]];
        let mut formats = vec![SheetFormat::rect(1000.0, 500.0), SheetFormat::rect(1000.0, 500.0)];
        let check = |lines: &[Vec<CutLine>], formats: &mut Vec<SheetFormat>| {
            let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: lines, material_name: String::new() };
            let stock = Stock::load(&dir).unwrap();
            let marked: Vec<Vec<String>> = (0..2)
                .map(|i| {
                    let pieces = crate::remnant::sheet_remnants(&parts, &sheets[i], &formats[i], &lines[i], crate::kerf::kerf_mm(&spec), st.min_remnant_m2, false);
                    remnant_refs(&stock, &j, formats, i, &pieces, &st)
                })
                .collect();
            make_remnants(&as_sam(&dir), &j, formats, &st, false, None).unwrap();
            assert_eq!(marked, vec![formats[0].made.clone(), formats[1].made.clone()]);
            assert_ne!(formats[0].made, formats[1].made);
        };
        check(&[line(300.0), line(300.0)], &mut formats);
        // Both lines move: both rebuilt, numbered on from the counter.
        check(&[line(400.0), line(400.0)], &mut formats);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn completed_sheet_remnants_notice_parts_moving() {
        // RN4: without a remnant line the stored remnant is the sheet less
        // its parts, so moving them must make it be rebuilt.
        let p = Placed { part: 0, x: 10.0, y: 10.0, w: 100.0, h: 50.0, turns: 0, mirror: false, spin: Default::default() };
        let moved = Placed { x: 300.0, ..p };
        assert_ne!(sheet_fingerprint(&[], &[p]), sheet_fingerprint(&[], &[moved]));
        let line = [CutLine { pts: vec![[900.0, 0.0], [900.0, 500.0]], kind: CutKind::Remnant }];
        assert_eq!(sheet_fingerprint(&line, &[p]), sheet_fingerprint(&line, &[moved]), "a remnant line alone decides");
        let part = Part { name: "R".into(), shape: nest::Shape::Rectangle, w: 100.0, h: 50.0, qty: 1, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let spec = SheetSpec { w: 1000.0, h: 500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default(), pick: Default::default() };
        let st = StockSettings { keep_completed: true, ..Default::default() };
        let stock = Stock::default();
        let parts = [part];
        let pending = |sheet: Vec<Placed>, made_from: u64| {
            let sheets = [sheet];
            let j = JobSheets { name: "DEMO-001", parts: &parts, spec: &spec, sheets: &sheets, cut_lines: &[], material_name: String::new() };
            let f = SheetFormat { made: vec!["R-1".into()], made_from, ..SheetFormat::rect(1000.0, 500.0) };
            let (todo, kept) = pending_remnants(&stock, &j, &[f], &st, false);
            !todo.is_empty() || !kept.is_empty()
        };
        let fp = sheet_fingerprint(&[], &[p]);
        assert!(!pending(vec![p], fp), "unchanged: nothing to redo");
        assert!(pending(vec![moved], fp), "parts moved: the remnant is out of date");
        assert!(!pending(vec![moved], legacy_no_line_fingerprint()), "older files stay quiet");
        let mut f = SheetFormat { made: vec!["R-1".into()], made_from: legacy_no_line_fingerprint(), ..SheetFormat::rect(1000.0, 500.0) };
        remnants_outdated(&mut f);
        assert!(pending(vec![moved], f.made_from), "a re-nested sheet is redone even from an older file");
    }

    #[test]
    fn reconciling_moves_each_reference_once() {
        let dir = std::env::temp_dir().join(format!("nestpro-stockflow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::library::create(&dir).unwrap();
        let mut s = Stock::default();
        s.add_sheet(Item::sheet("S1", 0, 3.0, 1500.0, 750.0, 5), true).unwrap();
        s.save(&dir).unwrap();
        let f = SheetFormat::from_stock(s.get("S1").unwrap());
        // One sheet held already, a second one added: only that one is taken.
        let mut held: BTreeMap<String, i32> = [("S1".to_string(), 1)].into();
        Stock::update(&as_sam(&dir), |s| s.take("S1", 1, false)).unwrap();
        let note = reconcile(&as_sam(&dir), &[f.clone(), f.clone()], 2, &mut held, false).unwrap();
        assert_eq!(note, "1 sheet(s) taken from stock");
        assert_eq!(Stock::load(&dir).unwrap().get("S1").unwrap().qty, 3);
        // Back to one: one given back.
        reconcile(&as_sam(&dir), &[f], 1, &mut held, false).unwrap();
        assert_eq!((Stock::load(&dir).unwrap().get("S1").unwrap().qty, held["S1"]), (4, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
