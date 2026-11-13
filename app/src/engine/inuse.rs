//! Jobs in use (CU5–CU12, built on the original's CU1–CU4): the job a
//! programmer has in front, and any job with changes they haven't saved, is
//! **in use**. The Library then holds one **hold** for it: a small file
//! `Holds\<record>.json` in the Library folder, naming who has it (user and
//! PC) and since when, so every PC sharing the Library sees it.
//!
//! - Taking, renewing, taking over and letting go of a hold are changes to
//!   the Library like any save: made under its write lock, the hold read
//!   again under it, written through a temporary file of this PC's own and
//!   renamed (LB1–LB3). So of two PCs opening one job at the same moment, one
//!   holds it and the other is told who does (CU5).
//! - The window renews its holds about every 30 s (CU6). A hold not renewed
//!   for about 2 minutes counts as gone (CU7), judged by the PC looking at
//!   it, by its own clock, as an abandoned write lock is (LB2): every renewal
//!   changes the hold, so a PC that has seen it unchanged for 2 minutes knows
//!   it wasn't renewed, whatever either PC's clock says. A hold whose last
//!   renewal is more than 10 minutes behind this PC's clock is gone at first
//!   sight (a PC that crashed long ago); clocks a few minutes apart never make
//!   a live hold look gone that way.
//! - Take over moves a hold to this PC whatever it says (CU9); the PC that
//!   lost it saves its changes as a copy.
//! - Deleting, renaming or moving a job someone else holds is refused
//!   (CU11). Saving over it isn't: a job opened anyway, or with warn off,
//!   opens and saves as usual (CU3), through the save check (LB7–LB10).
//! - Agents never hold a job, and are refused a change to any job that is
//!   held (CU12).
//!
//! The window and the agents give the engine who is working, its clock and
//! what it has seen of each hold (`Watch`, the Library's `watch`), so one test
//! can play several PCs and move their clocks.

use crate::library::{Kind, Library};
use crate::shared::{self, Clock, Who};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The holds' folder in the Library folder.
pub const FOLDER: &str = "Holds";
pub const FORMAT: &str = "nestpro-hold";
/// How often the window renews its holds (CU6).
pub const RENEW_EVERY: Duration = Duration::from_secs(30);
/// A hold a PC has seen unrenewed this long, by its own clock, counts as
/// gone (CU7).
pub const GONE_AFTER: Duration = Duration::from_secs(120);
/// A hold whose last renewal is this far behind this PC's clock counts as
/// gone at first sight (CU7): far more than two PCs' clocks differ.
pub const OLD_AT_SIGHT: Duration = Duration::from_secs(10 * 60);

/// A hold, as stored (CU5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    pub format: String,
    /// The job's record number, and its name when the hold was written.
    pub job: u64,
    pub name: String,
    /// Who has it: the user and the PC.
    pub by: Who,
    /// When it was taken, and last renewed, by the holder's clock (seconds
    /// since 1970).
    pub since: u64,
    pub renewed: u64,
    /// How often it has been written: every renewal changes it, so a PC
    /// watching the hold sees it renewed whatever the clocks say (CU7).
    pub beat: u64,
    /// The PCs it was taken over from (CU9), not yet told: kept when the
    /// hold is let go (`<record>.over.json`), so each is told at its next
    /// renewal even after the PC that took it over has let it go.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taken_from: Vec<Who>,
}

/// A job someone else has in use, as this PC sees it: its hold, or None for
/// a hold that can't be read (damaged, CU7). `unreadable`: the holds can't
/// be read now at all (the share), so whether the job is in use isn't known
/// and no change is made to it (CU11).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InUse {
    pub job: u64,
    pub name: String,
    pub hold: Option<Hold>,
    pub unreadable: Option<String>,
}

/// "since 09:14" today, else "since 9 Nov 09:14", in this PC's local time.
pub fn since(clock: &Clock, t: u64) -> String {
    let w = clock.when(t);
    format!("since {}", w.strip_prefix("at ").or_else(|| w.strip_prefix("on ")).unwrap_or(&w))
}

impl InUse {
    /// "sam on PROG-2 since 09:14".
    pub fn by_since(&self, clock: &Clock) -> String {
        match (&self.hold, &self.unreadable) {
            (Some(h), _) => format!("{} {}", h.by.label(), since(clock, h.since)),
            (None, Some(_)) => "a PC that can't be told now (the holds in the Library can't be read)".into(),
            (None, None) => "a PC its hold doesn't name (the hold in the Library can't be read)".into(),
        }
    }

    /// "In use by sam on PROG-2 since 09:14" (CU10).
    pub fn note(&self, clock: &Clock) -> String {
        match (&self.hold, &self.unreadable) {
            (Some(_), _) => format!("In use by {}", self.by_since(clock)),
            (None, Some(_)) => "Holds can't be read now".into(),
            (None, None) => "In use (its hold can't be read)".into(),
        }
    }

    /// "DEMO-7 is in use by sam on PROG-2 since 09:14." (CU11, CU12)
    pub fn said(&self, clock: &Clock) -> String {
        match (&self.hold, &self.unreadable) {
            (Some(_), _) => format!("{} is in use by {}.", self.name, self.by_since(clock)),
            (None, Some(e)) => format!("The holds in the Library can't be read now ({e}), so whether {} is in use isn't known: nothing was changed. Try again.", self.name),
            (None, None) => format!("{} is in use, but its hold in the Library can't be read to say by whom; it counts as gone 2 minutes after it last changed.", self.name),
        }
    }

    /// Who has it, when the hold says.
    pub fn by(&self) -> Option<&Who> {
        self.hold.as_ref().map(|h| &h.by)
    }
}

/// What this PC has seen of each job's hold, and since when, by its own
/// clock (CU7), and which jobs it took a hold on and hasn't let go of (held
/// now, or taken over from it, CU9). Shared by everything one window (or
/// one agent session) does, so every look counts.
#[derive(Clone, Debug, Default)]
pub struct Watch(Arc<Mutex<Memory>>);

#[derive(Debug, Default)]
struct Memory {
    /// Job → (the hold's fingerprint, when this PC first saw it so).
    looks: HashMap<u64, (u64, u64)>,
    /// Jobs this PC took a hold on and hasn't let go of.
    taken: std::collections::BTreeSet<u64>,
}

impl Watch {
    fn seen(&self) -> std::sync::MutexGuard<'_, Memory> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Note job `job`'s hold as it is now (`look`, its fingerprint) at `now`
    /// by this PC's clock; how long this PC has seen it just so.
    fn unchanged_for(&self, job: u64, look: u64, now: u64) -> u64 {
        let mut s = self.seen();
        let e = s.looks.entry(job).or_insert((look, now));
        // Changed, or this PC's clock was put back: seen from now.
        if e.0 != look || e.1 > now {
            *e = (look, now);
        }
        now - e.1
    }

    fn forget(&self, job: u64) {
        self.seen().looks.remove(&job);
    }

    fn took(&self, job: u64) {
        self.seen().taken.insert(job);
    }

    /// Did this PC take a hold on `job` and not let go of it (it holds it,
    /// or it was taken over from it)?
    pub fn took_it(&self, job: u64) -> bool {
        self.seen().taken.contains(&job)
    }

    /// This PC is done with `job`: let go, closed, or its changes saved as
    /// a copy after it was taken over.
    pub fn done_with(&self, job: u64) {
        self.seen().taken.remove(&job);
    }

    /// Done with every job (the Library folder changed).
    pub fn done_with_all(&self) {
        self.seen().taken.clear();
    }
}

fn fingerprint(b: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// A hold's file in the Library folder `dir`.
pub fn path(dir: &Path, job: u64) -> PathBuf {
    dir.join(FOLDER).join(format!("{job}.json"))
}

/// The file that keeps a let-go hold's takeover (CU9): who took job `job`
/// over, and the PCs it was taken from that haven't been told yet.
pub fn taken_path(dir: &Path, job: u64) -> PathBuf {
    dir.join(FOLDER).join(format!("{job}.over.json"))
}

/// The takeover kept for `job` after its hold was let go, if any.
fn taken_over(dir: &Path, job: u64) -> Option<Hold> {
    std::fs::read(taken_path(dir, job)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// What a hold's file holds now.
enum Found {
    Free,
    Hold(Hold, Vec<u8>),
    /// There, but can't be read: half-written or damaged.
    Damaged(Vec<u8>),
}

fn read(dir: &Path, job: u64) -> Result<Found, String> {
    match std::fs::read(path(dir, job)) {
        Ok(b) => Ok(match serde_json::from_slice::<Hold>(&b) {
            Ok(h) if h.format == FORMAT && h.job == job => Found::Hold(h, b),
            _ => Found::Damaged(b),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Found::Free),
        Err(e) => Err(format!("the hold on job {job} in the Library can't be read ({e})")),
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Is `h` this PC's own: the same user on the same PC (CU3: the same user on
/// another PC is someone else)? An agent never holds a job (CU12).
pub fn is_mine(h: &Hold, who: &Who) -> bool {
    same_pc(&h.by, who)
}

/// Are `a` and `b` the same user on the same PC (capitals aside), neither an
/// agent (CU3, CU12)?
fn same_pc(a: &Who, b: &Who) -> bool {
    !a.agent && !b.agent && same(&a.user, &b.user) && same(&a.pc, &b.pc)
}

/// What a hold is to this PC.
enum Seen {
    Free,
    Mine(Hold),
    /// Someone else's, and live.
    Other(InUse),
}

/// Judge what was found against who is looking, its clock and what it has
/// seen of the hold before (CU7). `name`: the job's name, for a hold that
/// can't be read.
fn judge(lib: &Library, job: u64, name: &str, f: Found) -> Seen {
    let now = lib.clock.now();
    match f {
        Found::Free => {
            lib.watch.forget(job);
            Seen::Free
        }
        Found::Hold(h, _) if is_mine(&h, &lib.who) => Seen::Mine(h),
        Found::Hold(h, b) => {
            let unchanged = lib.watch.unchanged_for(job, fingerprint(&b), now);
            let old = now.saturating_sub(h.renewed) >= OLD_AT_SIGHT.as_secs();
            if unchanged >= GONE_AFTER.as_secs() || old {
                Seen::Free
            } else {
                Seen::Other(InUse { job, name: h.name.clone(), hold: Some(h), ..Default::default() })
            }
        }
        Found::Damaged(b) => {
            if lib.watch.unchanged_for(job, fingerprint(&b), now) >= GONE_AFTER.as_secs() {
                Seen::Free
            } else {
                Seen::Other(InUse { job, name: name.to_string(), ..Default::default() })
            }
        }
    }
}

/// The job's name in the Library's list (for a hold that can't be read).
fn job_name(lib: &Library, job: u64) -> String {
    lib.entries().into_iter().find(|e| e.kind == Kind::Job && e.id == job).map_or_else(|| crate::library::auto_ref(Kind::Job, job), |e| e.name)
}

/// Why a hold wasn't taken.
#[derive(Clone, Debug, PartialEq)]
pub enum NotTaken {
    /// Someone else has the job in use (CU3).
    InUse(InUse),
    /// The Library can't be reached, or the hold couldn't be written.
    Failed(String),
}

/// Write a hold, under the write lock. A takeover kept from a let-go hold
/// (CU9) goes once someone holds the job again: the PCs it was taken from
/// then see that hold at their next renewal.
fn write(lib: &Library, h: &Hold, lock: &shared::WriteLock) -> Result<(), String> {
    let text = serde_json::to_string_pretty(h).map_err(|e| e.to_string())?;
    shared::write_file(&path(&lib.dir, h.job), text.as_bytes(), Some(lock))?;
    let _ = std::fs::remove_file(taken_path(&lib.dir, h.job));
    Ok(())
}

/// This PC's hold on `job`, going on from `before` (its earlier hold, kept
/// since then).
fn mine(lib: &Library, job: u64, name: &str, before: Option<&Hold>, beat: u64) -> Hold {
    let now = lib.clock.now();
    let taken_from = before.map_or_else(Vec::new, |h| h.taken_from.clone());
    Hold { format: FORMAT.into(), job, name: name.to_string(), by: lib.who.clone(), since: before.map_or(now, |h| h.since), renewed: now, beat, taken_from }
}

/// The beat a new write of a hold gets: one more than what's there.
fn next_beat(f: &Found) -> u64 {
    match f {
        Found::Hold(h, _) => h.beat + 1,
        _ => 1,
    }
}

/// Take the hold on library job `job` (named `name`) for this PC (CU5):
/// under the write lock, read again; when the job is free, its hold gone
/// (CU7) or already this PC's, the hold is written as this PC's. Someone
/// else's live hold refuses it, naming them, and nothing is written. Agents
/// never take a hold (CU12). Refused, naming the folder, while the Library
/// can't be reached; nothing is made in its place (LB11).
pub fn take(lib: &Library, job: u64, name: &str) -> Result<Hold, NotTaken> {
    if lib.who.agent {
        return Err(NotTaken::Failed("agents never hold a job".into()));
    }
    crate::library::usable(&lib.dir).map_err(NotTaken::Failed)?;
    let lock = lib.lock().map_err(NotTaken::Failed)?;
    let f = read(&lib.dir, job).map_err(NotTaken::Failed)?;
    let beat = next_beat(&f);
    let before = match judge(lib, job, name, f) {
        Seen::Other(u) => return Err(NotTaken::InUse(u)),
        Seen::Mine(h) => Some(h),
        Seen::Free => None,
    };
    shared::pause("hold checked");
    let h = mine(lib, job, name, before.as_ref(), beat);
    write(lib, &h, &lock).map_err(NotTaken::Failed)?;
    lib.watch.took(job);
    Ok(h)
}

/// A job the window wants to hold at a renewal, and whether it holds it now.
#[derive(Clone, Debug)]
pub struct Want {
    pub job: u64,
    pub name: String,
    /// It holds it (else it wants it once it's free: a job it opened while
    /// someone else had it, CU8).
    pub held: bool,
}

/// What a renewal found for a job.
#[derive(Clone, Debug, PartialEq)]
pub enum Renewed {
    /// This PC holds it, renewed (or taken, now that it's free).
    Held(Hold),
    /// It held it, and someone else holds it now: they took it over, or it
    /// was free while this PC couldn't renew it (CU9). Nothing was written.
    Lost(InUse),
    /// It didn't hold it, and someone else still does.
    Busy(InUse),
    /// The job isn't in the Library any more: nothing is held.
    JobGone,
}

/// Renew this PC's holds and take the ones it wants that are free (CU6),
/// under one hold of the write lock. A hold this PC held that names someone
/// else now is lost, and left as it is: it is never taken back without
/// asking (CU9). The whole renewal fails, changing nothing, while the
/// Library can't be reached (LB11); the jobs stay open as they are (LB13).
pub fn renew(lib: &Library, wants: &[Want]) -> Result<Vec<(u64, Renewed)>, String> {
    if lib.who.agent || wants.is_empty() {
        return Ok(vec![]);
    }
    crate::library::usable(&lib.dir)?;
    let lock = lib.lock()?;
    let listed = lib.entries();
    let mut out = vec![];
    for w in wants {
        if !lib.path(Kind::Job, w.job).exists() && !listed.iter().any(|e| e.kind == Kind::Job && e.id == w.job) {
            // Deleted: a hold of this PC's left on it goes too.
            if let Ok(Found::Hold(h, _)) = read(&lib.dir, w.job) {
                if is_mine(&h, &lib.who) && lock.still_held().is_ok() {
                    let _ = std::fs::remove_file(path(&lib.dir, w.job));
                }
            }
            lib.watch.done_with(w.job);
            out.push((w.job, Renewed::JobGone));
            continue;
        }
        let f = read(&lib.dir, w.job)?;
        // Taken over from this PC, and let go again since: told, not taken
        // back (CU9).
        if w.held && matches!(f, Found::Free) {
            if let Some(mut t) = taken_over(&lib.dir, w.job).filter(|t| t.taken_from.iter().any(|from| same_pc(from, &lib.who))) {
                t.taken_from.retain(|from| !same_pc(from, &lib.who));
                if lock.still_held().is_ok() {
                    let _ = if t.taken_from.is_empty() {
                        std::fs::remove_file(taken_path(&lib.dir, w.job)).map_err(|e| e.to_string())
                    } else {
                        serde_json::to_string_pretty(&t).map_err(|e| e.to_string()).and_then(|text| shared::write_file(&taken_path(&lib.dir, w.job), text.as_bytes(), Some(&lock)))
                    };
                }
                out.push((w.job, Renewed::Lost(InUse { job: w.job, name: t.name.clone(), hold: Some(t), ..Default::default() })));
                continue;
            }
        }
        let beat = next_beat(&f);
        let someone_else = matches!(&f, Found::Hold(h, _) if !is_mine(h, &lib.who));
        let r = match judge(lib, w.job, &w.name, f) {
            // Held here, and now someone else's (live or not): lost (CU9).
            Seen::Free if w.held && someone_else => Renewed::Lost(lost_to(lib, w)?),
            Seen::Other(u) if w.held => Renewed::Lost(u),
            Seen::Other(u) => Renewed::Busy(u),
            Seen::Mine(h) => renewed(lib, w, Some(&h), beat, &lock)?,
            Seen::Free => renewed(lib, w, None, beat, &lock)?,
        };
        out.push((w.job, r));
    }
    Ok(out)
}

/// The hold that now names someone else, for a PC that lost it.
fn lost_to(lib: &Library, w: &Want) -> Result<InUse, String> {
    match read(&lib.dir, w.job)? {
        Found::Hold(h, _) => Ok(InUse { job: w.job, name: h.name.clone(), hold: Some(h), ..Default::default() }),
        _ => Ok(InUse { job: w.job, name: w.name.clone(), ..Default::default() }),
    }
}

fn renewed(lib: &Library, w: &Want, before: Option<&Hold>, beat: u64, lock: &shared::WriteLock) -> Result<Renewed, String> {
    let h = mine(lib, w.job, &w.name, before, beat);
    write(lib, &h, lock)?;
    lib.watch.took(w.job);
    Ok(Renewed::Held(h))
}

/// Let go of this PC's holds on `jobs` (CU5): closed, switched away from
/// with nothing unsaved, the program quitting, or warn turned off (CU1).
/// Holds that name someone else are left alone.
pub fn release(lib: &Library, jobs: &[u64]) -> Result<(), String> {
    if lib.who.agent || jobs.is_empty() {
        return Ok(());
    }
    crate::library::usable(&lib.dir)?;
    let lock = lib.lock()?;
    for &job in jobs {
        if let Found::Hold(h, _) = read(&lib.dir, job)? {
            if is_mine(&h, &lib.who) {
                lock.still_held()?;
                // Taken over from PCs not told yet: kept, so they are (CU9).
                if !h.taken_from.is_empty() {
                    let text = serde_json::to_string_pretty(&h).map_err(|e| e.to_string())?;
                    shared::write_file(&taken_path(&lib.dir, job), text.as_bytes(), Some(&lock))?;
                }
                match std::fs::remove_file(path(&lib.dir, job)) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("couldn't let go of the hold on {}: {e}", h.name)),
                    _ => {}
                }
            }
        }
        lib.watch.forget(job);
        lib.watch.done_with(job);
    }
    Ok(())
}

/// Take over job `job` (CU9): the hold moves to this PC whatever it says
/// now, under the write lock. Returns whose it was (None: nobody's, or a
/// hold that couldn't be read). The PC that lost it is told at its next
/// renewal. Not for a job that's no longer in the Library, nor for agents.
pub fn take_over(lib: &Library, job: u64, name: &str) -> Result<Option<Hold>, String> {
    if lib.who.agent {
        return Err("agents never hold a job".into());
    }
    crate::library::usable(&lib.dir)?;
    let lock = lib.lock()?;
    if !lib.path(Kind::Job, job).exists() {
        return Err(format!("{name} is no longer in the Library: another PC may have deleted it. Refresh to see the jobs as they are now."));
    }
    let f = read(&lib.dir, job)?;
    let beat = next_beat(&f);
    let (before, keep) = match f {
        Found::Hold(h, _) if is_mine(&h, &lib.who) => (None, Some(h)),
        Found::Hold(h, _) => (Some(h), None),
        _ => (None, None),
    };
    shared::pause("taking over");
    let mut h = mine(lib, job, name, keep.as_ref(), beat);
    // Who it was taken from, to be told even once it's let go again (CU9).
    if let Some(b) = &before {
        h.taken_from = b.taken_from.iter().chain(std::iter::once(&b.by)).filter(|w| !same_pc(w, &lib.who)).cloned().collect();
    }
    write(lib, &h, &lock)?;
    lib.watch.forget(job);
    lib.watch.took(job);
    Ok(before)
}

/// Who else has job `job` in use now, as this PC judges it (CU7), read
/// without the write lock (to ask before anything else is done).
pub fn in_use(lib: &Library, job: u64) -> Result<Option<InUse>, String> {
    let f = read(&lib.dir, job)?;
    let name = match &f {
        Found::Damaged(_) => job_name(lib, job),
        _ => String::new(),
    };
    Ok(match judge(lib, job, &name, f) {
        Seen::Other(u) => Some(u),
        Seen::Mine(_) | Seen::Free => None,
    })
}

/// Every job someone else has in use now, by record (CU10): read without
/// the write lock, each hold judged as this PC sees it, so every look counts
/// towards a hold's expiry (CU7). A Library without holds has none.
pub fn look(lib: &Library) -> Result<BTreeMap<u64, InUse>, String> {
    let mut out = BTreeMap::new();
    let dir = lib.dir.join(FOLDER);
    let files = match std::fs::read_dir(&dir) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::library::usable(&lib.dir)?;
            return Ok(out);
        }
        Err(e) => return Err(format!("the holds in the Library can't be read ({e})")),
    };
    let mut names: Option<Vec<crate::library::Entry>> = None;
    for f in files.flatten() {
        let n = f.file_name().to_string_lossy().into_owned();
        let Some(job) = n.strip_suffix(".json").and_then(|s| s.parse::<u64>().ok()) else { continue };
        // A hold that can't be read now: whether its job is in use isn't
        // known, so no change may go past it (CU11).
        let found = read(&lib.dir, job).map_err(|e| format!("The holds in the Library can't be read now ({e}), so nothing was changed. Try again."))?;
        let name = match &found {
            Found::Damaged(_) => {
                let es = names.get_or_insert_with(|| lib.entries());
                es.iter().find(|e| e.kind == Kind::Job && e.id == job).map_or_else(|| crate::library::auto_ref(Kind::Job, job), |e| e.name.clone())
            }
            _ => String::new(),
        };
        if let Seen::Other(u) = judge(lib, job, &name, found) {
            out.insert(job, u);
        }
    }
    Ok(out)
}

/// May this PC change library job `job`: save over it, delete, rename or
/// move it, or a folder holding it (CU11, CU12)? Not while someone else has
/// it in use; an agent, not while anyone has. Called under the write lock
/// the change holds, so no hold is taken between the check and the change.
pub fn may_change(lib: &Library, job: u64) -> Result<(), InUse> {
    // A hold that can't be read now (the share): not known, so refused.
    let f = read(&lib.dir, job).map_err(|e| InUse { job, name: job_name(lib, job), unreadable: Some(e), ..Default::default() })?;
    let name = match &f {
        Found::Damaged(_) => job_name(lib, job),
        _ => String::new(),
    };
    match judge(lib, job, &name, f) {
        Seen::Other(u) => Err(u),
        Seen::Mine(h) if lib.who.agent => Err(InUse { job, name: h.name.clone(), hold: Some(h), ..Default::default() }),
        _ => Ok(()),
    }
}

/// May this PC save library job `job` over its record under the name `name`
/// (CU3, CU9, CU11, CU12)? An agent: not while anyone holds it. A window:
/// not when it took the job and someone else has it now (taken over: it
/// saves its changes as a copy), and not under a new name while someone
/// else has it (that renames it). A job it opened anyway, or opened with
/// warn off, saves as usual (CU3), through the save check (LB7–LB10).
/// Called before anything else is done, and again under the save's write
/// lock.
pub fn may_save_as(lib: &Library, job: u64, name: &str) -> Result<(), InUse> {
    if lib.who.agent || lib.watch.took_it(job) {
        may_change(lib, job)?;
    }
    if lib.entries().iter().any(|e| e.kind == Kind::Job && e.id == job && e.name != name) {
        may_change(lib, job)?;
    }
    Ok(())
}

/// A job deleted from the Library takes every hold on it with it (CU2,
/// CU11). Under the delete's write lock.
pub fn forget_job(lib: &Library, job: u64, lock: &shared::WriteLock) {
    if lock.still_held().is_ok() {
        let _ = std::fs::remove_file(path(&lib.dir, job));
        let _ = std::fs::remove_file(taken_path(&lib.dir, job));
    }
    lib.watch.forget(job);
}

/// For an agent: what to do instead of changing a job in use (CU12).
pub fn agent_refusal(u: &InUse, clock: &Clock, instead: &str) -> String {
    format!("{} Try again later. {instead}", u.said(clock))
}

#[cfg(test)]
mod inuse_tests {
    use super::*;
    use crate::library::{self, SaveTo};
    use crate::shared::{Clock, Who};
    use std::time::Duration;

    /// 2026-11-09 09:14 UTC: a test clock shows UTC.
    const NOV_9_0914: u64 = 1_794_215_640;

    fn new_library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_inuse_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        library::create(&dir).unwrap();
        dir
    }

    /// `user` on PC `pc`, on the Library in `dir`, going by `clock`, with
    /// what this PC has seen of the holds.
    fn pc(dir: &Path, user: &str, pc: &str, clock: &Clock) -> Library {
        Library::at(dir, &Who::new(user, pc), clock)
    }

    /// Library job `name`, saved by kim on PROG-3.
    fn job(dir: &Path, name: &str) -> u64 {
        let spec = crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default(), gas: None };
        let mut j = crate::jobfile::JobFile::new_job(name, spec);
        j.week = "Acme".into();
        Library::open(dir, &Who::new("kim", "PROG-3"), &Clock::at(NOV_9_0914)).unwrap().save_job(&j, SaveTo::New).unwrap().id
    }

    fn holder(r: Result<Hold, NotTaken>) -> InUse {
        match r {
            Err(NotTaken::InUse(u)) => u,
            other => panic!("expected in use, got {other:?}"),
        }
    }

    fn every_30s(clock: &Clock, minutes: u64, mut each: impl FnMut()) {
        for _ in 0..minutes * 2 {
            clock.advance(RENEW_EVERY);
            each();
        }
    }

    /// CU5, CU10: PC 1 opens DEMO-7 and holds it; PC 2 sees it in use by PC
    /// 1's user and PC since when, and can't take it; released, it's free.
    #[test]
    fn a_hold_names_who_and_since_when_and_keeps_others_off_until_released() {
        let dir = new_library("basic");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "Sam", "PROG-2", &clock), pc(&dir, "sam", "PROG-1", &clock));
        let h = take(&p1, id, "DEMO-7").unwrap();
        assert_eq!((h.by.label().as_str(), h.since), ("Sam on PROG-2", NOV_9_0914));
        clock.advance(Duration::from_secs(60));
        let u = holder(take(&p2, id, "DEMO-7"));
        assert_eq!(u.note(&clock), "In use by Sam on PROG-2 since 09:14");
        assert_eq!(u.said(&clock), "DEMO-7 is in use by Sam on PROG-2 since 09:14.");
        assert_eq!(look(&p2).unwrap().get(&id).map(|u| u.note(&clock)).as_deref(), Some("In use by Sam on PROG-2 since 09:14"));
        // Its own PC doesn't see it as someone else's.
        assert!(look(&p1).unwrap().is_empty());
        assert_eq!(may_change(&p2, id).map_err(|u| u.said(&clock)), Err("DEMO-7 is in use by Sam on PROG-2 since 09:14.".to_string()));
        assert_eq!(may_change(&p1, id), Ok(()));
        release(&p1, &[id]).unwrap();
        assert!(!path(&dir, id).exists());
        assert!(take(&p2, id, "DEMO-7").is_ok());
        // Released by someone it isn't theirs: left alone.
        release(&p1, &[id]).unwrap();
        assert!(path(&dir, id).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU3: the same user on another PC is someone else; the same user on
    /// the same PC (whatever the capitals) is this PC, which goes on holding
    /// it, since kept, as after a crash and a restart.
    #[test]
    fn the_same_user_on_another_pc_is_someone_else() {
        let dir = new_library("same_user");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        take(&pc(&dir, "sam", "PROG-1", &clock), id, "DEMO-7").unwrap();
        let u = holder(take(&pc(&dir, "sam", "PROG-2", &clock), id, "DEMO-7"));
        assert_eq!(u.by().map(Who::label).as_deref(), Some("sam on PROG-1"));
        clock.advance(Duration::from_secs(300));
        let again = take(&pc(&dir, "SAM", "prog-1", &clock), id, "DEMO-7").unwrap();
        assert_eq!(again.since, NOV_9_0914, "this PC's own hold goes on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU5: two PCs opening one job at the same moment end with one holder,
    /// and the other is told who it is.
    #[test]
    fn two_pcs_taking_one_job_at_the_same_moment_end_with_one_holder() {
        let dir = new_library("race");
        let id = job(&dir, "DEMO-7");
        for round in 0..6 {
            let clock = Clock::at(NOV_9_0914 + round * 1000);
            let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
            let runs: Vec<_> = ["PROG-1", "PROG-2"]
                .into_iter()
                .map(|p| {
                    let (dir, clock, gate) = (dir.clone(), clock.clone(), gate.clone());
                    std::thread::spawn(move || {
                        let lib = pc(&dir, p, p, &clock);
                        gate.wait();
                        (p, take(&lib, id, "DEMO-7"))
                    })
                })
                .collect();
            let got: Vec<(&str, Result<Hold, NotTaken>)> = runs.into_iter().map(|t| t.join().unwrap()).collect();
            let won: Vec<&str> = got.iter().filter(|(_, r)| r.is_ok()).map(|(p, _)| *p).collect();
            assert_eq!(won.len(), 1, "round {round}: {got:?}");
            for (_, r) in &got {
                if let Err(NotTaken::InUse(u)) = r {
                    assert_eq!(u.by().map(|w| w.pc.as_str()), Some(won[0]));
                }
            }
            let on_disk: Hold = serde_json::from_slice(&std::fs::read(path(&dir, id)).unwrap()).unwrap();
            assert_eq!(on_disk.by.pc, won[0]);
            release(&pc(&dir, won[0], won[0], &clock), &[id]).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU6, CU7: a holder renewing every 30 s stays in use however long PC 2
    /// watches. Once it stops (a crash), PC 2 sees it gone 2 minutes later
    /// and takes the job freely.
    #[test]
    fn a_hold_no_longer_renewed_is_gone_two_minutes_later() {
        let dir = new_library("expiry");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "Sam", "PROG-2", &clock));
        take(&p1, id, "DEMO-7").unwrap();
        let want = [Want { job: id, name: "DEMO-7".into(), held: true }];
        every_30s(&clock, 10, || {
            assert!(matches!(renew(&p1, &want).unwrap()[0].1, Renewed::Held(_)));
            assert!(in_use(&p2, id).unwrap().is_some(), "a live hold looked gone");
        });
        // PROG-1 crashes: nothing renews its hold.
        clock.advance(Duration::from_secs(110));
        assert!(holder(take(&p2, id, "DEMO-7")).by().is_some_and(|w| w.pc == "PROG-1"), "gone too soon");
        clock.advance(Duration::from_secs(10));
        let h = take(&p2, id, "DEMO-7").unwrap();
        assert_eq!(h.by.pc, "PROG-2");
        // PROG-1 comes back: it's told someone else has the job now.
        assert!(matches!(&renew(&p1, &want).unwrap()[0].1, Renewed::Lost(u) if u.by().is_some_and(|w| w.pc == "PROG-2")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU7: clocks a few minutes apart. A live hold whose PC's clock is 5
    /// minutes behind never looks gone; a dead one whose PC's clock is 5
    /// minutes ahead is gone 2 minutes after this PC first saw it unchanged,
    /// not 7. A hold renewed more than 10 minutes ago by its PC's clock is
    /// gone at first sight; one renewed 9 minutes ago is not.
    #[test]
    fn clocks_minutes_apart_never_make_a_live_hold_look_gone_or_a_dead_one_live() {
        let dir = new_library("clocks");
        let (a, b) = (job(&dir, "DEMO-7"), job(&dir, "DEMO-8"));
        let behind = Clock::at(NOV_9_0914 - 300);
        let ahead = Clock::at(NOV_9_0914 + 300);
        let mid = Clock::at(NOV_9_0914);
        let tick = |d: Duration| [&behind, &ahead, &mid].iter().for_each(|c| c.advance(d));
        let slow = pc(&dir, "sam", "PROG-1", &behind);
        let fast = pc(&dir, "Sam", "PROG-2", &ahead);
        let me = pc(&dir, "kim", "PROG-3", &mid);
        take(&slow, a, "DEMO-7").unwrap();
        take(&fast, b, "DEMO-8").unwrap();
        let want = [Want { job: a, name: "DEMO-7".into(), held: true }];
        // PROG-1 (5 min slow) keeps renewing; PROG-2 (5 min fast) has died.
        // PROG-3 looks every 30 s.
        for k in 1..=8 {
            tick(RENEW_EVERY);
            renew(&slow, &want).unwrap();
            assert!(in_use(&me, a).unwrap().is_some(), "a live hold looked gone to a PC 5 minutes ahead of it");
            let dead = in_use(&me, b).unwrap();
            if k < 5 {
                assert!(dead.is_some(), "a dead hold went before 2 minutes ({k})");
            } else {
                assert!(dead.is_none(), "a dead hold still looked live after 2 minutes ({k}) because its PC's clock is ahead");
            }
        }
        // A hold its PC's clock says was renewed more than 10 minutes ago:
        // gone at first sight; 9 minutes: not yet.
        let c = job(&dir, "DEMO-9");
        let old = Clock::at(mid.now() - 9 * 60);
        take(&pc(&dir, "lee", "PROG-5", &old), c, "DEMO-9").unwrap();
        assert!(in_use(&pc(&dir, "kim", "PROG-6", &mid), c).unwrap().is_some());
        let older = Clock::at(mid.now() - 11 * 60);
        take_over(&pc(&dir, "lee", "PROG-5", &older), c, "DEMO-9").unwrap();
        assert!(in_use(&pc(&dir, "kim", "PROG-7", &mid), c).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU7: a damaged or half-written hold counts as in use, by nobody it
    /// can name, until it has stayed unchanged for 2 minutes; then the job is
    /// free and the next hold is written over it.
    #[test]
    fn a_damaged_or_half_written_hold_is_in_use_until_unchanged_for_two_minutes() {
        let dir = new_library("damaged");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let p2 = pc(&dir, "Sam", "PROG-2", &clock);
        for bad in [&b"{\"format\":\"nestpro-hold\",\"job\":"[..], b""] {
            std::fs::create_dir_all(dir.join(FOLDER)).unwrap();
            std::fs::write(path(&dir, id), bad).unwrap();
            let u = holder(take(&p2, id, "DEMO-7"));
            assert_eq!((u.name.as_str(), u.hold.is_none()), ("DEMO-7", true));
            assert_eq!(u.note(&clock), "In use (its hold can't be read)");
            assert!(u.said(&clock).contains("can't be read"), "{}", u.said(&clock));
            assert!(look(&p2).unwrap().contains_key(&id));
            clock.advance(GONE_AFTER);
            assert_eq!(take(&p2, id, "DEMO-7").unwrap().by.pc, "PROG-2");
            release(&p2, &[id]).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11, CU6: the Library can't be reached while PROG-1 holds DEMO-7:
    /// its renewal fails, says so and makes nothing; PROG-2, which still
    /// reaches it, sees the hold gone 2 minutes later and takes the job.
    /// Back again, PROG-1's renewal is told it lost the job. Had nobody
    /// taken it, PROG-1 just holds it again.
    #[test]
    fn a_library_out_of_reach_while_holding_is_said_and_the_hold_lost_only_if_taken() {
        let dir = new_library("reach");
        let (a, b) = (job(&dir, "DEMO-7"), job(&dir, "DEMO-8"));
        let clock = Clock::at(NOV_9_0914);
        let p1 = pc(&dir, "sam", "PROG-1", &clock);
        take(&p1, a, "DEMO-7").unwrap();
        take(&p1, b, "DEMO-8").unwrap();
        let wants = [Want { job: a, name: "DEMO-7".into(), held: true }, Want { job: b, name: "DEMO-8".into(), held: true }];
        // PROG-1's share is down: its Library folder isn't there for it.
        let away = dir.with_file_name(format!("{}-away", dir.file_name().unwrap().to_string_lossy()));
        let p1_off = Library { dir: away.clone(), ..pc(&dir, "sam", "PROG-1", &clock) };
        let p2 = pc(&dir, "Sam", "PROG-2", &clock);
        assert!(in_use(&p2, a).unwrap().is_some());
        for _ in 0..4 {
            clock.advance(RENEW_EVERY);
            let e = renew(&p1_off, &wants).unwrap_err();
            assert!(e.contains("can't be reached") && e.contains(&away.display().to_string()), "{e}");
            assert!(!away.exists(), "a renewal made a folder in the Library's place");
        }
        take(&p2, a, "DEMO-7").unwrap();
        let r = renew(&p1, &wants).unwrap();
        assert!(matches!(&r[0].1, Renewed::Lost(u) if u.by().is_some_and(|w| w.pc == "PROG-2")), "{r:?}");
        assert!(matches!(&r[1].1, Renewed::Held(h) if h.by.pc == "PROG-1"), "{r:?}");
        // Lost: the hold stays PROG-2's.
        let on_disk: Hold = serde_json::from_slice(&std::fs::read(path(&dir, a)).unwrap()).unwrap();
        assert_eq!(on_disk.by.pc, "PROG-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU9: Take over moves the hold to this PC whatever it says, and says
    /// whose it was; the PC that lost it is told at its next renewal, and
    /// is refused a change to the job while the other PC has it.
    #[test]
    fn take_over_moves_the_hold_and_the_loser_is_told_at_its_next_renewal() {
        let dir = new_library("take_over");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "Sam", "PROG-2", &clock));
        take(&p1, id, "DEMO-7").unwrap();
        clock.advance(Duration::from_secs(6 * 60));
        let was = take_over(&p2, id, "DEMO-7").unwrap();
        assert_eq!(was.map(|h| h.by.label()).as_deref(), Some("sam on PROG-1"));
        let want = [Want { job: id, name: "DEMO-7".into(), held: true }];
        let r = renew(&p1, &want).unwrap();
        let Renewed::Lost(u) = &r[0].1 else { panic!("{r:?}") };
        assert_eq!(u.note(&clock), "In use by Sam on PROG-2 since 09:20");
        assert!(may_change(&p1, id).is_err());
        // Taken over from nobody, or from this PC itself: fine.
        let other = job(&dir, "DEMO-8");
        assert_eq!(take_over(&p1, other, "DEMO-8").unwrap(), None);
        // Not for a job that's gone, nor for agents.
        assert!(take_over(&p1, 999, "DEMO-404").unwrap_err().contains("no longer in the Library"));
        assert!(take_over(&Library::at(&dir, &Who::new("sam", "PROG-1").as_agent(), &clock), id, "DEMO-7").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU9, LB1: Take over while the holder is saving waits for the save to
    /// finish (both need the write lock), so the save is stored whole and the
    /// hold then moves. The holder saving right after it lost the hold is
    /// refused, naming who has it now, and nothing is written; saved as a
    /// new job (a copy) it goes through.
    #[test]
    fn take_over_while_the_holder_saves_waits_and_the_holder_is_then_refused() {
        let dir = new_library("take_over_saving");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let p1 = pc(&dir, "sam", "PROG-1", &clock);
        take(&p1, id, "DEMO-7").unwrap();
        let mut mine = p1.load_job(id).unwrap();
        mine.parts.push(crate::engine::shape_part(crate::nest::Shape::Disc, 1, 1));
        let taking: std::sync::Arc<Mutex<Option<std::thread::JoinHandle<Result<Option<Hold>, String>>>>> = Default::default();
        {
            let (dir, clock, taking) = (dir.clone(), clock.clone(), taking.clone());
            shared::hooks::set(move |s| {
                if s == "revision checked" {
                    // PROG-2 takes over while PROG-1's save holds the lock.
                    let (dir, clock) = (dir.clone(), clock.clone());
                    let (waiting, waiting_rx) = std::sync::mpsc::channel::<()>();
                    let t = std::thread::spawn(move || {
                        shared::hooks::set(move |s| {
                            if s == "lock busy" {
                                let _ = waiting.send(());
                            }
                        });
                        take_over(&pc(&dir, "Sam", "PROG-2", &clock), id, "DEMO-7")
                    });
                    waiting_rx.recv_timeout(Duration::from_secs(60)).expect("Take over waits for the write lock");
                    assert!(!t.is_finished(), "Take over didn't wait for the save");
                    *taking.lock().unwrap() = Some(t);
                }
            });
        }
        let saved = p1.save_job(&mine, SaveTo::Read(mine.library.unwrap()));
        shared::hooks::clear();
        let rec = saved.expect("the save the holder had begun is stored");
        let t = taking.lock().unwrap().take().expect("Take over started");
        assert_eq!(t.join().unwrap().unwrap().map(|h| h.by.pc).as_deref(), Some("PROG-1"));
        assert_eq!(p1.load_job(id).unwrap().parts.len(), 1, "the save landed whole before the takeover");
        // PROG-1 saves again, its hold lost: refused, nothing written.
        let record = std::fs::read(p1.path(Kind::Job, id)).unwrap();
        let mut again = p1.load_job(id).unwrap();
        again.library = Some(rec);
        again.parts.clear();
        let Err(crate::library::Refused::InUse(u)) = p1.save_job(&again, SaveTo::Read(rec)) else { panic!("saved over a job taken over") };
        assert_eq!(u.said(&clock), "DEMO-7 is in use by Sam on PROG-2 since 09:14.");
        assert_eq!(std::fs::read(p1.path(Kind::Job, id)).unwrap(), record);
        // As a copy it goes through; DEMO-7 stays Sam's.
        again.name = "DEMO-7 (1)".into();
        again.library = None;
        let copy = p1.save_job(&again, SaveTo::New).unwrap();
        assert_ne!(copy.id, id);
        assert_eq!(std::fs::read(p1.path(Kind::Job, id)).unwrap(), record);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU3, CU9, CU11: a PC that didn't take the job (opened anyway, or warn
    /// off) saves over it while someone else holds it, through the save
    /// check alone: the holder's next save is asked (LB7), nothing lost, and
    /// the hold stays the holder's. Saving it under a new name is renaming
    /// it: refused. The PC it was taken over from is refused; so are agents.
    #[test]
    fn a_pc_that_didnt_take_a_job_saves_over_it_and_the_one_it_was_taken_from_cant() {
        let dir = new_library("save_held");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "Sam", "PROG-2", &clock), pc(&dir, "sam", "PROG-1", &clock));
        take(&p1, id, "DEMO-7").unwrap();
        let read = p1.load_job(id).unwrap();
        clock.advance(Duration::from_secs(60));
        // PROG-1 (never took it) saves: through.
        let mut theirs = p2.load_job(id).unwrap();
        theirs.parts.push(crate::engine::shape_part(crate::nest::Shape::Disc, 1, 1));
        let rec = p2.save_job(&theirs, SaveTo::Read(theirs.library.unwrap())).expect("a job opened anyway saves as usual");
        assert_eq!(p2.load_job(id).unwrap().saved.map(|s| s.by.label()).as_deref(), Some("sam on PROG-1"));
        let on_disk: Hold = serde_json::from_slice(&std::fs::read(path(&dir, id)).unwrap()).unwrap();
        assert_eq!(on_disk.by.label(), "Sam on PROG-2", "the hold stays the holder's");
        // Under a new name: renaming a job in use, refused.
        let mut renamed = p2.load_job(id).unwrap();
        renamed.name = "DEMO-7 frames".into();
        let Err(crate::library::Refused::InUse(u)) = p2.save_job(&renamed, SaveTo::Read(rec)) else { panic!("renamed a job in use") };
        assert_eq!(u.said(&clock), "DEMO-7 is in use by Sam on PROG-2 since 09:14.");
        // The holder saves what it read: the save check asks (LB7).
        assert!(matches!(p1.save_job(&read, SaveTo::Read(read.library.unwrap())), Err(crate::library::Refused::Changed(c)) if c.by.as_ref().map(Who::label).as_deref() == Some("sam on PROG-1")));
        assert_eq!(p1.load_job(id).unwrap().parts.len(), 1, "PROG-1's save is still there");
        // Taken over from PROG-2: PROG-2 may not save over it now.
        take_over(&pc(&dir, "kim", "PROG-3", &clock), id, "DEMO-7").unwrap();
        let now = p1.load_job(id).unwrap();
        let Err(crate::library::Refused::InUse(u)) = p1.save_job(&now, SaveTo::Read(now.library.unwrap())) else { panic!("saved over a job taken over from it") };
        assert_eq!(u.by().map(Who::label).as_deref(), Some("kim on PROG-3"));
        // Agents: refused on any job in use.
        let agent = Library::at(&dir, &Who::new("sam", "PROG-1").as_agent(), &clock);
        assert!(matches!(agent.save_job(&now, SaveTo::Read(now.library.unwrap())), Err(crate::library::Refused::InUse(_))));
        // Done with it (saved as a copy, or closed): PROG-2 is like any PC again.
        p1.watch.done_with(id);
        assert!(p1.save_job(&now, SaveTo::Read(now.library.unwrap())).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// S2 (CU9): PROG-2 takes DEMO-7 over from PROG-1 and lets it go again
    /// before PROG-1's next renewal. PROG-1 is still told it was taken over
    /// at that renewal, and doesn't take it back; once told, nothing of the
    /// takeover is kept. Had a third PC taken the job meanwhile, PROG-1 sees
    /// that hold instead.
    #[test]
    fn a_takeover_let_go_before_the_next_renewal_is_still_told() {
        let dir = new_library("takeover_let_go");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "Sam", "PROG-2", &clock));
        let want = [Want { job: id, name: "DEMO-7".into(), held: true }];
        for third in [false, true] {
            take(&p1, id, "DEMO-7").unwrap();
            clock.advance(RENEW_EVERY);
            take_over(&p2, id, "DEMO-7").unwrap();
            clock.advance(Duration::from_secs(10));
            release(&p2, &[id]).unwrap();
            assert!(!path(&dir, id).exists());
            if third {
                take(&pc(&dir, "kim", "PROG-3", &clock), id, "DEMO-7").unwrap();
            }
            clock.advance(Duration::from_secs(20));
            let r = renew(&p1, &want).unwrap();
            let Renewed::Lost(u) = &r[0].1 else { panic!("PROG-1 wasn't told: {r:?}") };
            assert_eq!(u.by().map(|w| w.pc.as_str()), Some(if third { "PROG-3" } else { "PROG-2" }));
            let now: Option<Hold> = std::fs::read(path(&dir, id)).ok().and_then(|b| serde_json::from_slice(&b).ok());
            assert_ne!(now.map(|h| h.by.pc), Some("PROG-1".to_string()), "PROG-1 took it back");
            assert!(!taken_path(&dir, id).exists(), "the takeover was kept after PROG-1 was told");
            release(&pc(&dir, "kim", "PROG-3", &clock), &[id]).unwrap();
            p1.watch.done_with(id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// S4 (CU11, CU12): a hold that can't be read now (an I/O error, not a
    /// damaged file) refuses a change rather than letting it past: an
    /// agent's save, deleting the job, moving it into another folder.
    /// Nothing is written, and the refusal says so in plain words.
    #[test]
    fn a_hold_that_cant_be_read_now_refuses_the_change() {
        let dir = new_library("unreadable");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        // A hold file the share won't give back (here: a folder in its place).
        std::fs::create_dir_all(path(&dir, id)).unwrap();
        let record = std::fs::read(Library::at(&dir, &Who::new("kim", "PROG-3"), &clock).path(Kind::Job, id)).unwrap();
        let agent = Library::at(&dir, &Who::new("sam", "PROG-1").as_agent(), &clock);
        let j = agent.load_job(id).unwrap();
        let Err(crate::library::Refused::InUse(u)) = agent.save_job(&j, SaveTo::Read(j.library.unwrap())) else { panic!("saved past a hold that can't be read") };
        assert!(u.said(&clock).contains("can't be read now") && u.said(&clock).contains("nothing was changed"), "{}", u.said(&clock));
        let window = pc(&dir, "Sam", "PROG-2", &clock);
        let e = window.delete(Kind::Job, id).unwrap_err();
        assert!(e.contains("can't be read now") && e.contains("Nothing was deleted"), "{e}");
        let e = crate::folders::change(&window, Kind::Job, |t, _| t.put(id, crate::folders::TOP)).unwrap_err().say(Kind::Job, false);
        assert!(e.contains("can't be read now"), "{e}");
        assert_eq!(std::fs::read(window.path(Kind::Job, id)).unwrap(), record);
        assert!(window.load_job(id).is_ok(), "the job was deleted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU12: agents never take a hold, and are refused a change to a job
    /// anyone holds, their own user's included.
    #[test]
    fn agents_never_hold_and_are_refused_a_job_anyone_holds() {
        let dir = new_library("agents");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let agent = Library::at(&dir, &Who::new("sam", "PROG-1").as_agent(), &clock);
        assert!(matches!(take(&agent, id, "DEMO-7"), Err(NotTaken::Failed(_))));
        assert!(renew(&agent, &[Want { job: id, name: "DEMO-7".into(), held: false }]).unwrap().is_empty());
        assert!(!path(&dir, id).exists());
        assert_eq!(may_change(&agent, id), Ok(()));
        take(&pc(&dir, "sam", "PROG-1", &clock), id, "DEMO-7").unwrap();
        let u = may_change(&agent, id).unwrap_err();
        assert_eq!(agent_refusal(&u, &clock, "Nothing was changed."), "DEMO-7 is in use by sam on PROG-1 since 09:14. Try again later. Nothing was changed.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CU6: a job this PC wanted while someone else had it (opened anyway)
    /// is taken at the renewal after it's free, and stays busy until then.
    /// A job deleted from the Library is said gone, and nothing is held.
    #[test]
    fn a_renewal_takes_a_wanted_job_once_free_and_says_when_a_job_is_gone() {
        let dir = new_library("wanted");
        let id = job(&dir, "DEMO-7");
        let clock = Clock::at(NOV_9_0914);
        let (p1, p2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "Sam", "PROG-2", &clock));
        take(&p1, id, "DEMO-7").unwrap();
        let want = [Want { job: id, name: "DEMO-7".into(), held: false }];
        assert!(matches!(&renew(&p2, &want).unwrap()[0].1, Renewed::Busy(_)));
        release(&p1, &[id]).unwrap();
        assert!(matches!(&renew(&p2, &want).unwrap()[0].1, Renewed::Held(h) if h.by.pc == "PROG-2"));
        assert!(matches!(renew(&p2, &[Want { job: 999, name: "DEMO-404".into(), held: true }]).unwrap()[0].1, Renewed::JobGone));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
