//! The sheet stock's part in a job, shared by the window and the agent
//! operations: which stock sheets Do All takes and in what order (SS4–SS7,
//! SS12), taking sheets from stock and giving them back as a job's nestings
//! change (SS10, SS11), and storing the remnants its remnant lines leave (RE1).
//! The stock itself (items, counters, file) is `crate::stock`.

use crate::cutoff::{CutKind, CutLine};
use crate::nest::{self, FormatSlot, NestResult, Part, Placed, SheetFormat, SheetSpec};
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
/// already holds; then the job's sheet size when stock may run out.
/// None when stock isn't used or has nothing for this job.
pub fn stock_plan(spec: &SheetSpec, u: &StockUse, exclude: &[String]) -> Option<Vec<FormatSlot>> {
    let st = u.st;
    if !st.use_stock {
        return None;
    }
    let stock = Stock::load(&u.dir).ok()?;
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
    let items: Vec<Item> = if st.remnants_first {
        order(rem).into_iter().chain(order(sht)).collect()
    } else {
        order(cands.clone())
    };
    if items.is_empty() {
        return None;
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

/// Take the sheets the job now uses from stock and give back the ones it no
/// longer uses (SS10, SS11), bringing `held` up to date. Returns a note ("" when
/// nothing changed).
pub fn reconcile(dir: &Path, formats: &[SheetFormat], n_sheets: usize, held: &mut BTreeMap<String, i32>, allow_neg: bool) -> Result<String, String> {
    let used = sheets_used(formats, n_sheets);
    if &used == held {
        return Ok(String::new());
    }
    // Each reference once (one in both lists would otherwise move twice).
    let refs: std::collections::BTreeSet<String> = used.keys().chain(held.keys()).cloned().collect();
    let before = held.clone();
    let (mut taken, mut back) = (0, 0);
    Stock::update(dir, |s| {
        for r in &refs {
            let d = used.get(r).copied().unwrap_or(0) - before.get(r).copied().unwrap_or(0);
            if d > 0 {
                s.take(r, d, allow_neg)?;
                taken += d;
            } else if d < 0 {
                s.give_back(r, -d);
                back += -d;
            }
        }
        Ok(())
    })?;
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
}

/// A fingerprint of a sheet's remnant lines, to tell whether they changed
/// since its remnants were stored (`SheetFormat::made_from`, ST12).
pub fn lines_fingerprint(lines: &[CutLine]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in lines.iter().filter(|l| l.kind == CutKind::Remnant) {
        for p in &l.pts {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
        }
    }
    h.finish() | 1
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
pub fn pending_remnants(stock_now: &Stock, j: &JobSheets, formats: &[SheetFormat], st: &StockSettings, force_lines: bool) -> (Vec<Pending>, Vec<usize>) {
    let mut todo = vec![];
    let mut kept_used = vec![];
    for i in 0..j.sheets.len() {
        let Some(f) = formats.get(i) else { continue };
        if j.sheets[i].is_empty() {
            continue;
        }
        let lines = j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice());
        let mut replace = vec![];
        if !f.made.is_empty() {
            if f.made_from == lines_fingerprint(lines) || f.made_from == 0 {
                // Unchanged (or stored before fingerprints were kept).
                continue;
            }
            if !f.made.iter().all(|r| stock_now.get(r).is_some_and(|it| it.qty >= 1)) {
                kept_used.push(i);
                continue;
            }
            replace = f.made.clone();
        }
        let has_line = lines.iter().any(|l| l.kind == CutKind::Remnant);
        let want = (has_line && (st.remnants_on_save || force_lines)) || st.keep_completed;
        let pieces = if want { crate::remnant::sheet_remnants(j.parts, &j.sheets[i], f, lines, crate::cutoff::KERF_MM, st.min_remnant_m2, st.keep_completed && !has_line) } else { vec![] };
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
    let mut s = Stock::load(dir).unwrap_or_default();
    let (todo, _) = pending_remnants(&s, j, formats, st, force_lines);
    let mut rows = vec![];
    for p in &todo {
        for r in &p.replace {
            s.delete(r);
        }
        let origin = origin_of(j, &formats[p.sheet], p.sheet);
        for (k, o) in p.pieces.iter().enumerate() {
            let (w, h) = o.size();
            let reference = s.next_remnant_ref(&origin);
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
pub fn make_remnants(dir: &Path, j: &JobSheets, formats: &mut Vec<SheetFormat>, st: &StockSettings, force_lines: bool, names: Option<&BTreeMap<(usize, usize), String>>) -> Result<Made, String> {
    while formats.len() < j.sheets.len() {
        formats.push(SheetFormat::rect(j.spec.w, j.spec.h));
    }
    let stock_now = Stock::load(dir).unwrap_or_default();
    let (todo, kept_used) = pending_remnants(&stock_now, j, formats, st, force_lines);
    let mut out = Made { count: 0, kept_used };
    if todo.is_empty() {
        return Ok(out);
    }
    let (material, thickness) = (j.spec.material, j.spec.thickness);
    let fs = formats.clone();
    let made: Vec<(usize, Vec<String>)> = Stock::update(dir, |s| {
        let mut out = vec![];
        for p in &todo {
            for r in &p.replace {
                s.delete(r);
            }
            let f = &fs[p.sheet];
            // The sheet it came from: its stock item, or a stand-in for a
            // sheet that wasn't taken from stock.
            let parent = s.get(&f.stock_ref).cloned().unwrap_or_else(|| {
                let mut it = Item::sheet(&format!("{}-{:02}", j.name, p.sheet + 1), material, thickness, f.w, f.h, 0);
                it.priority = stock::DEFAULT_PRIORITY;
                it
            });
            let refs = p.pieces.iter().enumerate().map(|(k, o)| s.add_remnant(o.clone(), &parent, 1, names.and_then(|m| m.get(&(p.sheet, k)).cloned()), j.name)).collect();
            out.push((p.sheet, refs));
        }
        Ok(out)
    })?;
    for (i, refs) in made {
        out.count += refs.len();
        let fp = lines_fingerprint(j.cut_lines.get(i).map_or(&[][..], |v| v.as_slice()));
        formats[i].made_from = if refs.is_empty() { 0 } else { fp };
        formats[i].made = refs;
    }
    Ok(out)
}

/// Before a job is written: put new remnants in stock and settle what the job
/// holds (what the window does on every save). Returns the notes ("" when
/// nothing changed).
#[allow(clippy::too_many_arguments)]
pub fn on_save(dir: &Path, j: &JobSheets, formats: &mut Vec<SheetFormat>, held: &mut BTreeMap<String, i32>, st: &StockSettings, force_lines: bool, names: Option<&BTreeMap<(usize, usize), String>>) -> String {
    let mut notes = vec![];
    if st.remnants_on_save || st.keep_completed || force_lines {
        match make_remnants(dir, j, formats, st, force_lines, names) {
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
    match reconcile(dir, formats, j.sheets.len(), held, st.allow_no_stock) {
        Ok(s) if !s.is_empty() => notes.push(s),
        Ok(_) => {}
        Err(e) => notes.push(format!("stock not updated: {e}")),
    }
    notes.join("; ")
}

/// Undo a job's stock booking: every sheet it holds back into stock, and the
/// remnants it stored taken out again while nothing else has used them (their
/// quantity is still the 1 they were stored with). Returns (sheets given back,
/// remnants removed, remnants left because they were used elsewhere).
pub fn release(dir: &Path, formats: &mut [SheetFormat], held: &mut BTreeMap<String, i32>) -> Result<(i32, usize, Vec<String>), String> {
    let made: Vec<String> = formats.iter().flat_map(|f| f.made.iter().cloned()).collect();
    let give: Vec<(String, i32)> = held.iter().map(|(r, n)| (r.clone(), *n)).collect();
    let (back, removed, kept) = Stock::update(dir, |s| {
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

    #[test]
    fn reconciling_moves_each_reference_once() {
        let dir = std::env::temp_dir().join(format!("nestpro-stockflow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Stock::default();
        s.add_sheet(Item::sheet("S1", 0, 3.0, 1500.0, 750.0, 5), true).unwrap();
        s.save(&dir).unwrap();
        let f = SheetFormat::from_stock(s.get("S1").unwrap());
        // One sheet held already, a second one added: only that one is taken.
        let mut held: BTreeMap<String, i32> = [("S1".to_string(), 1)].into();
        Stock::update(&dir, |s| s.take("S1", 1, false)).unwrap();
        let note = reconcile(&dir, &[f.clone(), f.clone()], 2, &mut held, false).unwrap();
        assert_eq!(note, "1 sheet(s) taken from stock");
        assert_eq!(Stock::load(&dir).unwrap().get("S1").unwrap().qty, 3);
        // Back to one: one given back.
        reconcile(&dir, &[f], 1, &mut held, false).unwrap();
        assert_eq!((Stock::load(&dir).unwrap().get("S1").unwrap().qty, held["S1"]), (4, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
