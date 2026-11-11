//! The two-PC Library check (`library-check`, LB4): before the shop shares
//! one Library folder between programming PCs (ADR 0003), run this from two
//! or more PCs at once against a test folder on the share. Each run saves
//! made-up jobs and library parts, saves over them again and changes the
//! sheet stock in a loop, tagged with its PC's name, then checks that
//! everything it and every other run wrote is there and whole, and prints a
//! plain pass/fail report. Each round also makes, renames, nests, moves and
//! deletes job folders and files its new job in one (JX10–JX15), and the
//! check finds every folder and every job where its run left it.
//!
//! It only works in an empty folder or one it made itself (it leaves
//! `library-check.json` there), and refuses a real Library.

use crate::folders::{self, TOP};
use crate::jobfile::{self, JobFile};
use crate::library::{Kind, Library};
use crate::nest::{SheetSpec, Shape};
use crate::shared::{self, WriteLock};
use crate::stock::{Item, Stock};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Every job, library part and stock sheet the check makes starts with this.
pub const TAG: &str = "DEMO-CHECK";
/// The file that marks a folder as the check's own.
pub const MARKER: &str = "library-check.json";
/// The folder of the runs' logs (what each run saved).
pub const LOGS: &str = "library-check";
/// A run's own stock sheet goes back down to 1 when it reaches this, so a
/// long run never meets the stock's ceiling.
const SHEET_TOP: i32 = 1000;

/// How long to run and how to tag the run.
pub struct Options {
    pub minutes: f64,
    /// Stop after this many rounds instead of after `minutes`.
    pub rounds: Option<u64>,
    /// The PC name the run is tagged with.
    pub pc: String,
}

/// A record a run saved: its number, name and how often it was saved over.
#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    id: u64,
    name: String,
    rev: u32,
    /// Jobs: the folder the run put it in (None: where its save filed it,
    /// the top-level DEMO-CHECK folder).
    #[serde(default)]
    folder: Option<u64>,
}

/// A job folder a run made, as its last change left it.
#[derive(Clone, Serialize, Deserialize)]
struct MadeFolder {
    id: u64,
    name: String,
    parent: u64,
}

/// What a run saved, written after every round (`library-check\<run>.json`).
#[derive(Clone, Serialize, Deserialize)]
struct Log {
    format: String,
    run: String,
    pc: String,
    user: String,
    finished: bool,
    rounds: u64,
    jobs: Vec<Saved>,
    parts: Vec<Saved>,
    /// The run's own stock sheet and the quantity its changes leave it at.
    sheet: String,
    sheet_qty: i32,
    sheet_changes: u64,
    /// Job folders the run made, and those it deleted again.
    #[serde(default)]
    folders: Vec<MadeFolder>,
    #[serde(default)]
    deleted: Vec<u64>,
    #[serde(default)]
    folder_changes: u64,
    errors: Vec<String>,
}

/// The report: passed or not, and the plain text.
pub struct Report {
    pub passed: bool,
    pub text: String,
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Refuse a folder that could be a real Library (LB4): this PC's own
/// Library, a folder with files the check didn't make, or one where real
/// jobs, library parts or stock sheets have been saved since. Anything it
/// can't read or doesn't recognise counts as possibly real.
pub fn refuse_real_library(folder: &Path, this_library: &Path) -> Result<(), String> {
    let shown = folder.display();
    let empty = "Give it an empty test folder (it is made if missing), or one it made before";
    if same_folder(folder, this_library) {
        return Err(format!("{shown} is this PC's Library (Settings → Folders → Library folder). The check saves made-up jobs, so it never runs on a real Library. {empty}."));
    }
    if !folder.exists() {
        return Ok(());
    }
    let names = |d: &Path| -> Result<Vec<String>, String> {
        let mut n = vec![];
        for f in std::fs::read_dir(d).map_err(|e| format!("can't read {}: {e}", d.display()))? {
            n.push(f.map_err(|e| format!("can't read {}: {e}", d.display()))?.file_name().to_string_lossy().into_owned());
        }
        Ok(n)
    };
    let top = names(folder)?;
    if top.is_empty() {
        return Ok(());
    }
    if !folder.join(MARKER).is_file() {
        return Err(format!("{shown} isn't empty and wasn't made by the Library check — it may be a real Library. {empty}."));
    }
    let real = |what: String| format!("{shown} holds {what}, which the Library check didn't make: it may be a real Library. {empty}.");
    let unsure = |what: String| format!("{shown}: {what}, so it may be a real Library. {empty}.");
    // Only what the check and the Library under it make: temporary files and
    // the write lock's files come and go while another run is saving.
    let passing = |n: &str| n.ends_with(".tmp") || n.starts_with(shared::LOCK_FILE);
    let tree = folders::file_name(Kind::Job);
    if let Some(n) = top.iter().find(|n| ![MARKER, LOGS, crate::library::INDEX, tree, "Jobs", "Products", "Sheets"].contains(&n.as_str()) && !passing(n)) {
        return Err(real(format!("'{n}'")));
    }
    if let Some(n) = folder.join("Sheets").is_dir().then(|| names(&folder.join("Sheets"))).transpose()?.unwrap_or_default().iter().find(|n| *n != "stock.json" && !passing(n)) {
        return Err(real(format!("'Sheets/{n}'")));
    }
    // The list: readable, and only the check's own records.
    let lib = Library::at(folder, &shared::Who::this_pc(), &shared::Clock::pc());
    // The job folders: readable, and only the check's own.
    if folder.join(tree).exists() {
        let t = folders::load(&lib, Kind::Job).map_err(|e| unsure(e.to_string()))?;
        if let Some(f) = t.folders.iter().find(|f| !f.name.starts_with(TAG)) {
            return Err(real(format!("the job folder '{}'", f.name)));
        }
    }
    if folder.join(crate::library::INDEX).exists() {
        let listed = lib.read_index().map_err(|e| unsure(format!("its list of records ({}) can't be read ({e})", crate::library::INDEX)))?;
        if let Some(e) = listed.iter().find(|e| !e.name.starts_with(TAG)) {
            return Err(real(format!("'{}'", e.name)));
        }
    }
    // Every record file, listed or not: one of the check's own; or one a run
    // of the check saved and logged (if it's damaged now, that is the check's
    // to report); or empty, a save cut off before it wrote anything.
    let logged = logged_records(folder);
    for (kind, sub) in [(Kind::Job, "Jobs"), (Kind::Product, "Products")] {
        let root = folder.join(sub);
        if !root.is_dir() {
            continue;
        }
        for d in names(&root)? {
            let dir = root.join(&d);
            if !dir.is_dir() || d.parse::<u64>().is_err() {
                return Err(real(format!("'{sub}/{d}'")));
            }
            for n in names(&dir)? {
                if n.ends_with(".tmp") {
                    continue;
                }
                let p = dir.join(&n);
                let id = match n.rsplit_once('.') {
                    Some((stem, _)) if lib.path(kind, stem.parse().unwrap_or(0)) == p => stem.parse::<u64>().unwrap_or(0),
                    _ => return Err(real(format!("'{sub}/{d}/{n}'"))),
                };
                let name = match kind {
                    Kind::Job => jobfile::load(&p).map(|j| j.name),
                    Kind::Product => lib.load_product(id).map(|pf| pf.part.name),
                };
                match name {
                    Ok(name) if name.starts_with(TAG) => {}
                    Ok(name) => return Err(real(format!("'{name}' (record {id})"))),
                    Err(_) if logged.contains(&(kind == Kind::Job, id)) || std::fs::metadata(&p).is_ok_and(|m| m.len() == 0) => {}
                    Err(e) => return Err(unsure(format!("record {id} ({sub}/{d}/{n}) can't be read ({e}) and no run of the check saved it"))),
                }
            }
        }
    }
    // The stock: readable, and only the check's own sheets.
    let stock = Stock::load(folder).map_err(|e| unsure(format!("its sheet stock can't be read ({e})")))?;
    if let Some(it) = stock.items.iter().find(|i| !i.reference.starts_with(TAG)) {
        return Err(real(format!("stock sheet '{}'", it.reference)));
    }
    Ok(())
}

/// The records the runs' logs say they saved.
fn logged_records(folder: &Path) -> std::collections::HashSet<(bool, u64)> {
    let mut out = std::collections::HashSet::new();
    for f in std::fs::read_dir(folder.join(LOGS)).into_iter().flatten().flatten() {
        let Some(l) = std::fs::read_to_string(f.path()).ok().and_then(|t| serde_json::from_str::<Log>(&t).ok()) else { continue };
        out.extend(l.jobs.iter().map(|s| (true, s.id)));
        out.extend(l.parts.iter().map(|s| (false, s.id)));
    }
    out
}

fn demo_spec() -> SheetSpec {
    SheetSpec { w: 3000.0, h: 1500.0, margin: 10.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() }
}

/// A made-up job; `rev` (how often it was saved over) goes in its method.
fn demo_job(name: &str, rev: u32) -> JobFile {
    let mut j = JobFile::new_job(name, demo_spec());
    j.week = TAG.into();
    j.method = format!("rev {rev}");
    for (n, shape) in [Shape::Rectangle, Shape::PlateHole, Shape::Disc].into_iter().enumerate() {
        j.parts.push(super::shape_part(shape, n + 1, n + 1));
    }
    j
}

/// A made-up library part; `rev` + 1 is its quantity.
fn demo_part(name: &str, rev: u32) -> crate::nest::Part {
    let mut p = super::shape_part(Shape::Flange, 1, 1);
    p.name = name.into();
    p.qty = rev + 1;
    p
}

fn job_rev(j: &JobFile) -> Option<u32> {
    j.method.strip_prefix("rev ").and_then(|r| r.parse().ok())
}

/// One round of job folder changes (JX10–JX15): the run's own top-level
/// folder (first round), a new folder in it, renamed, this round's new job
/// moved into it, every second round the folder before moved into it, and a
/// spare folder made and deleted again. Each change is logged as it leaves
/// the folder, so the check can find it.
fn folder_round(lib: &Library, log: &mut Log, n: u64, own: &str) {
    let change = |log: &mut Log, what: &str, f: &dyn Fn(&mut folders::Tree, &[crate::library::Entry]) -> Result<u64, folders::Refused>| -> Option<u64> {
        match folders::change(lib, Kind::Job, |t, es| f(t, es)) {
            Ok(id) => {
                log.folder_changes += 1;
                Some(id)
            }
            Err(e) => {
                log.errors.push(format!("{what} reported an error: {}", e.say(true)));
                None
            }
        }
    };
    let run = match log.folders.first() {
        Some(f) => f.id,
        None => match change(log, &format!("making job folder '{own}'"), &|t, _| t.make(TOP, own)) {
            Some(id) => {
                log.folders.push(MadeFolder { id, name: own.to_string(), parent: TOP });
                id
            }
            None => return,
        },
    };
    let name = format!("{TAG} round {n}");
    let Some(id) = change(log, &format!("making job folder '{name}'"), &|t, _| t.make(run, &name)) else { return };
    log.folders.push(MadeFolder { id, name: name.clone(), parent: run });
    let renamed = format!("{name} renamed");
    if change(log, &format!("renaming job folder '{name}'"), &|t, _| t.rename(id, &renamed).map(|_| id)).is_some() {
        if let Some(f) = log.folders.iter_mut().find(|f| f.id == id) {
            f.name = renamed;
        }
    }
    // This round's new job, into the new folder.
    if let Some(k) = log.jobs.iter().position(|j| j.name.ends_with(&format!(" job {n}"))) {
        let r = log.jobs[k].id;
        if change(log, &format!("moving job '{}' into a folder", log.jobs[k].name.clone()), &|t, _| t.put(r, id).map(|_| id)).is_some() {
            log.jobs[k].folder = Some(id);
        }
    }
    // Every second round, the folder before goes into this one.
    if n % 2 == 0 {
        let prev = format!("{TAG} round {} ", n - 1);
        if let Some(before) = log.folders.iter().find(|f| f.parent == run && format!("{} ", f.name).starts_with(&prev)).map(|f| f.id) {
            if change(log, "moving a job folder into another", &|t, _| t.move_folder(before, id).map(|_| before)).is_some() {
                if let Some(f) = log.folders.iter_mut().find(|f| f.id == before) {
                    f.parent = id;
                }
            }
        }
    }
    let spare = format!("{TAG} spare {n}");
    if let Some(s) = change(log, &format!("making job folder '{spare}'"), &|t, _| t.make(run, &spare)) {
        if change(log, &format!("deleting job folder '{spare}'"), &|t, es| t.delete(s, es).map(|_| s)).is_some() {
            log.deleted.push(s);
        } else {
            log.folders.push(MadeFolder { id: s, name: spare, parent: run });
        }
    }
}

fn write_log(folder: &Path, log: &Log) -> Result<(), String> {
    let text = serde_json::to_string_pretty(log).map_err(|e| e.to_string())?;
    shared::write_file(&folder.join(LOGS).join(format!("{}.json", log.run)), text.as_bytes(), None)
}

/// Run the check on `folder`: refuse a real Library, mark the folder,
/// save in a loop, then check everything every run saved.
pub fn run(folder: &Path, this_library: &Path, opt: &Options) -> Result<Report, String> {
    refuse_real_library(folder, this_library)?;
    std::fs::create_dir_all(folder).map_err(|e| format!("can't make {}: {e}", folder.display()))?;
    let who = shared::Who::this_pc();
    match std::fs::OpenOptions::new().write(true).create_new(true).open(folder.join(MARKER)) {
        Ok(mut f) => {
            use std::io::Write as _;
            let mark = json!({"format": "nestpro-library-check", "made_by": who.label(), "note": "A test folder for the Library check. Not a Library: never point the program's Library at it."});
            f.write_all(mark.to_string().as_bytes()).map_err(|e| e.to_string())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("can't write in {}: {e}", folder.display())),
    }
    let pc = if opt.pc.trim().is_empty() { who.pc.clone() } else { opt.pc.trim().to_string() };
    // Its saves record the run's user and the PC it's tagged with (LB5).
    let lib = Library::open(folder, &shared::Who::new(&who.user, &pc), &shared::Clock::pc())?;
    let started = Instant::now();
    // The run's own tag: when it started, its process and a count, so no two
    // runs (on any PC, or one after another) share records or a stock sheet.
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let n = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = format!("{}-{}-{}", crate::stock::base36(ms), crate::stock::base36(u64::from(std::process::id())), crate::stock::base36(n));
    let mut log = Log {
        format: "nestpro-library-check-log".into(),
        run: format!("{}-{stamp}", shared::file_safe(&pc)),
        pc: pc.clone(),
        user: who.user.clone(),
        finished: false,
        rounds: 0,
        jobs: vec![],
        parts: vec![],
        sheet: format!("{TAG}-{stamp}"),
        sheet_qty: 0,
        sheet_changes: 0,
        folders: vec![],
        deleted: vec![],
        folder_changes: 0,
        errors: vec![],
    };
    let until = started + Duration::from_secs_f64(opt.minutes.max(0.0) * 60.0);
    let (mut job_overs, mut part_overs) = (0u64, 0u64);
    while opt.rounds.map_or(Instant::now() < until, |n| log.rounds < n) {
        log.rounds += 1;
        let n = log.rounds;
        // A new job, then one of this run's jobs saved over again.
        let name = format!("{TAG} {pc} {stamp} job {n}");
        match lib.save_job(&demo_job(&name, 0), None) {
            Ok(id) => log.jobs.push(Saved { id, name, rev: 0, folder: None }),
            Err(e) => log.errors.push(format!("saving job '{name}' reported an error: {e}")),
        }
        if let Some(k) = (!log.jobs.is_empty()).then(|| (n as usize * 7) % log.jobs.len()) {
            let s = &mut log.jobs[k];
            match lib.save_job(&demo_job(&s.name, s.rev + 1), Some(s.id)) {
                Ok(_) => {
                    s.rev += 1;
                    job_overs += 1;
                }
                Err(e) => log.errors.push(format!("saving job '{}' over record {} reported an error: {e}", s.name, s.id)),
            }
        }
        // The same for a library part.
        let name = format!("{TAG} {pc} {stamp} part {n}");
        match lib.save_product(&demo_part(&name, 0), TAG, 0, 3.0, None) {
            Ok(id) => log.parts.push(Saved { id, name, rev: 0, folder: None }),
            Err(e) => log.errors.push(format!("saving library part '{name}' reported an error: {e}")),
        }
        if let Some(k) = (!log.parts.is_empty()).then(|| (n as usize * 5) % log.parts.len()) {
            let s = &mut log.parts[k];
            match lib.save_product(&demo_part(&s.name, s.rev + 1), TAG, 0, 3.0, Some(s.id)) {
                Ok(_) => {
                    s.rev += 1;
                    part_overs += 1;
                }
                Err(e) => log.errors.push(format!("saving library part '{}' over record {} reported an error: {e}", s.name, s.id)),
            }
        }
        folder_round(&lib, &mut log, n, &format!("{TAG} {pc} {stamp}"));
        // A stock change on this run's own sheet: one more on the rack.
        let sheet = log.sheet.clone();
        let changed = Stock::update(folder, |st| {
            let q = st.get(&sheet).map_or(0, |i| i.qty);
            let want = if q == 0 || q >= SHEET_TOP { 1 } else { q + 1 };
            if q == 0 {
                st.add_sheet(Item::sheet(&sheet, 0, 3.0, 1000.0, 500.0, 1), true)?;
            } else {
                st.change_qty(&sheet, want - q)?;
            }
            Ok((q, want))
        });
        match changed {
            Ok((q, want)) => {
                if q != log.sheet_qty {
                    log.errors.push(format!("Stock sheet {sheet} had {q} on the rack where this run's changes left {}: a stock change was lost.", log.sheet_qty));
                }
                log.sheet_qty = want;
                log.sheet_changes += 1;
            }
            Err(e) => log.errors.push(format!("changing stock sheet {sheet} reported an error: {e}")),
        }
        if let Err(e) = write_log(folder, &log) {
            log.errors.push(format!("writing this run's log: {e}"));
        }
    }
    log.finished = true;
    if let Err(e) = write_log(folder, &log) {
        log.errors.push(format!("writing this run's log: {e}"));
    }
    let checked = verify(folder, &lib, &log);
    let mut problems: Vec<String> = log.errors.clone();
    problems.extend(checked.problems);
    let took = started.elapsed().as_secs_f64();
    let mut text = format!(
        "Library check of {} by {pc} ({}): {} round(s) in {}.\nThis run: {} job(s) and {} library part(s) saved, {job_overs} job(s) and {part_overs} part(s) saved over again, {} stock change(s), {} job folder change(s).\nChecked: {} job(s), {} library part(s), {} stock sheet(s) and {} job folder(s) from {} run(s) ({}).\n",
        folder.display(),
        who.user,
        log.rounds,
        if took < 90.0 { format!("{took:.1} s") } else { format!("{:.1} min", took / 60.0) },
        log.jobs.len(),
        log.parts.len(),
        log.sheet_changes,
        log.folder_changes,
        checked.jobs,
        checked.parts,
        checked.sheets,
        checked.folders,
        checked.runs.len(),
        checked.runs.join(", "),
    );
    let passed = problems.is_empty();
    if passed {
        text.push_str("PASS — everything every run saved is there and whole.");
    } else {
        text.push_str(&format!("FAIL — {} problem(s). Don't share the Library on this folder until they are explained:\n", problems.len()));
        for p in &problems {
            text.push_str(&format!("  - {p}\n"));
        }
    }
    Ok(Report { passed, text: text.trim_end().to_string() })
}

struct Checked {
    jobs: usize,
    parts: usize,
    sheets: usize,
    folders: usize,
    runs: Vec<String>,
    problems: Vec<String>,
}

/// A record that can't be read, said plainly.
fn damaged(kind: Kind, id: u64, path: &Path, rel: &str, e: &str) -> String {
    let what = if kind == Kind::Job { "Job" } else { "Library part" };
    match std::fs::metadata(path) {
        Err(_) => format!("{what} record {id} ({rel}) is missing."),
        Ok(m) if m.len() == 0 => format!("{what} record {id} ({rel}) is empty: its number was taken but the save was cut off before anything was written (a PC stopped in the middle of saving)."),
        Ok(_) => format!("{what} record {id} ({rel}) is half-written or damaged — it can't be read ({e})."),
    }
}

/// Check that every record, list entry and stock change every run logged is
/// there and whole, and that every record in the folder can be read.
fn verify(folder: &Path, lib: &Library, own: &Log) -> Checked {
    let mut problems: BTreeMap<String, String> = BTreeMap::new();
    let mut add = |key: String, p: String| {
        problems.entry(key).or_insert(p);
    };
    // The logs first, then the list: a run logs a record only after its save
    // finished, so the list read after its log has it.
    let mut logs: Vec<Log> = vec![];
    for f in std::fs::read_dir(folder.join(LOGS)).into_iter().flatten().flatten() {
        let p = f.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Log>(&t).map_err(|e| e.to_string())) {
            Ok(l) if l.run == own.run => {}
            Ok(l) => logs.push(l),
            Err(e) => add(format!("log {}", p.display()), format!("The log {} is half-written or damaged ({e}).", p.display())),
        }
    }
    logs.push(own.clone());
    logs.sort_by(|a, b| a.run.cmp(&b.run));
    let list = match lib.read_index() {
        Ok(l) => l,
        Err(e) => {
            add("index".into(), format!("The Library's list ({}) can't be read ({e}).", crate::library::INDEX));
            vec![]
        }
    };
    let stock = match Stock::load(folder) {
        Ok(s) => Some(s),
        Err(e) => {
            add("stock".into(), format!("The sheet stock (Sheets/stock.json) is half-written or damaged ({e})."));
            None
        }
    };
    let tree = match folders::load(lib, Kind::Job) {
        Ok(t) => Some(t),
        Err(e) => {
            add("folders".into(), format!("The Library's job folders ({}) are half-written or damaged ({e}).", folders::file_name(Kind::Job)));
            None
        }
    };
    let filed = tree.as_ref().and_then(|t| t.find(TAG));
    let rel = |p: &Path| p.strip_prefix(folder).unwrap_or(p).to_string_lossy().replace('\\', "/");
    // Records that can't be read are looked at again under the write lock: a
    // save in progress on another PC has its number taken but nothing in it yet.
    let mut suspects: Vec<(Kind, u64, PathBuf)> = vec![];
    let (mut jobs, mut parts, mut folder_count) = (0, 0, 0);
    for l in &logs {
        // A run still going may save over its records again after its log
        // was read: its saves can only be newer.
        let exact = l.finished || l.run == own.run;
        for s in &l.jobs {
            jobs += 1;
            let p = lib.path(Kind::Job, s.id);
            match jobfile::load(&p) {
                Err(_) => suspects.push((Kind::Job, s.id, p)),
                Ok(j) if j.name != s.name => add(rel(&p), format!("Job record {} ({}) holds '{}' instead of '{}', which {} saved there.", s.id, rel(&p), j.name, s.name, l.pc)),
                Ok(j) => {
                    let rev = job_rev(&j).unwrap_or(0);
                    if rev < s.rev || (exact && rev != s.rev) {
                        add(rel(&p), format!("Job '{}' (record {}) has save {rev} where {} saved {}: a save was lost.", s.name, s.id, l.pc, s.rev));
                    }
                }
            }
            if !list.iter().any(|e| e.kind == Kind::Job && e.id == s.id && e.name == s.name) {
                add(format!("list job {}", s.id), format!("Job '{}' (record {}), saved by {}, is missing from the Library's list.", s.name, s.id, l.pc));
            }
            // Where its run put it, or where its save filed it.
            if let Some(t) = &tree {
                let (at, want) = (t.folder_of(s.id), s.folder.or(filed).unwrap_or(TOP));
                if at != want {
                    add(format!("place job {}", s.id), format!("Job '{}' (record {}) is in {} where {} left it in {}: its place in the job folders was lost.", s.name, s.id, t.place(at), l.pc, t.place(want)));
                }
            }
        }
        for s in &l.parts {
            parts += 1;
            let p = lib.path(Kind::Product, s.id);
            match lib.load_product(s.id) {
                Err(_) => suspects.push((Kind::Product, s.id, p)),
                Ok(pf) if pf.part.name != s.name => add(rel(&p), format!("Library part record {} ({}) holds '{}' instead of '{}', which {} saved there.", s.id, rel(&p), pf.part.name, s.name, l.pc)),
                Ok(pf) => {
                    let rev = pf.part.qty.saturating_sub(1);
                    if rev < s.rev || (exact && rev != s.rev) {
                        add(rel(&p), format!("Library part '{}' (record {}) has save {rev} where {} saved {}: a save was lost.", s.name, s.id, l.pc, s.rev));
                    }
                }
            }
            if !list.iter().any(|e| e.kind == Kind::Product && e.id == s.id && e.name == s.name) {
                add(format!("list part {}", s.id), format!("Library part '{}' (record {}), saved by {}, is missing from the Library's list.", s.name, s.id, l.pc));
            }
        }
        if let Some(t) = &tree {
            for f in &l.folders {
                folder_count += 1;
                match t.get(f.id) {
                    None => add(format!("folder {}", f.id), format!("Job folder '{}' (#{}), made by {}, is missing: a folder change was lost.", f.name, f.id, l.pc)),
                    Some(now) if exact && (now.name != f.name || now.parent != f.parent) => add(
                        format!("folder {}", f.id),
                        format!("Job folder #{} is '{}' in {} where {}'s last change left it '{}' in {}: a folder change was lost.", f.id, now.name, t.place(now.parent), l.pc, f.name, t.place(f.parent)),
                    ),
                    Some(_) => {}
                }
            }
            for id in &l.deleted {
                if t.get(*id).is_some() {
                    add(format!("folder {id}"), format!("Job folder #{id}, which {} deleted, is back: a folder change was lost.", l.pc));
                }
            }
        }
        if let (Some(st), true) = (&stock, exact && l.sheet_changes > 0) {
            match st.get(&l.sheet) {
                None => add(format!("sheet {}", l.sheet), format!("Stock sheet {} of {} is missing from the stock.", l.sheet, l.pc)),
                Some(it) if it.qty != l.sheet_qty => add(format!("sheet {}", l.sheet), format!("Stock sheet {} of {} has {} on the rack where its changes left {}: a stock change was lost.", l.sheet, l.pc, it.qty, l.sheet_qty)),
                Some(_) => {}
            }
        }
    }
    // Every record in the folder, logged or not, must be whole.
    for (kind, sub) in [(Kind::Job, "Jobs"), (Kind::Product, "Products")] {
        for d in std::fs::read_dir(folder.join(sub)).into_iter().flatten().flatten() {
            for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
                let p = f.path();
                let Some(id) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()) else { continue };
                let whole = match kind {
                    Kind::Job => jobfile::load(&p).is_ok(),
                    Kind::Product => lib.load_product(id).is_ok(),
                };
                if !whole && !suspects.iter().any(|s| s.2 == p) {
                    suspects.push((kind, id, p));
                }
            }
        }
    }
    if !suspects.is_empty() {
        let _lock = WriteLock::take(folder);
        for (kind, id, p) in &suspects {
            let r = match kind {
                Kind::Job => jobfile::load(p).map(|_| ()),
                Kind::Product => lib.load_product(*id).map(|_| ()),
            };
            if let Err(e) = r {
                add(rel(p), damaged(*kind, *id, p, &rel(p), &e));
            }
        }
    }
    let sheets = logs.iter().filter(|l| l.sheet_changes > 0).count();
    Checked { jobs, parts, sheets, folders: folder_count, runs: logs.iter().map(|l| l.pc.clone()).collect(), problems: problems.into_values().collect() }
}
