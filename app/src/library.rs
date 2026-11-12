//! The app's own library ("database"): products and jobs kept as one open,
//! readable file per record in a folder that can be local or shared on the
//! network, so a shop without Lantek has its own part and job database.
//! Laid out like Lantek's record folders (rule ST1:
//! `Products\<n/1000>\<n>`, `Nestings\<n/1000>\<n>` — 1000
//! records per folder), with an index for quick lists and search. Lantek
//! keeps the lists in SQL Server and the files encrypted; here both are
//! plain files.

use crate::jobfile::JobFile;
use crate::nest::Part;
use crate::shared::{self, Clock, Saved, Who, WriteLock};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "nestpro-library";
pub const VERSION: u32 = 1;
/// Records per sub-folder, as Lantek (n / 1000).
pub const PER_FOLDER: u64 = 1000;
pub const INDEX: &str = "index.json";

/// Automatic references, as Lantek makes them (rules RF2, RF3): a fixed text then
/// a 6-character counter from "000001" to "999999" — "JOB" for jobs,
/// "SHP" for sheet parts (also KIT, CMI, RWM, SHT, BEAM, BPRT…).
pub const JOB_PREFIX: &str = "JOB";
pub const PART_PREFIX: &str = "SHP";
pub const REF_DIGITS: usize = 6;
pub const REF_MAX: u64 = 999_999;

/// The automatic reference of a library record.
pub fn auto_ref(kind: Kind, id: u64) -> String {
    let p = match kind {
        Kind::Job => JOB_PREFIX,
        Kind::Product => PART_PREFIX,
    };
    format!("{p}{id:0w$}", w = REF_DIGITS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Product,
    Job,
}

impl Kind {
    fn folder(self) -> &'static str {
        match self {
            Kind::Product => "Products",
            Kind::Job => "Jobs",
        }
    }
    fn ext(self) -> &'static str {
        match self {
            Kind::Product => "json",
            Kind::Job => crate::jobfile::EXT,
        }
    }
}

/// One line of the index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    pub id: u64,
    pub name: String,
    /// Jobs: their folder (week); products: a group, e.g. the customer.
    pub folder: String,
    pub material: usize,
    pub thickness: f64,
    pub w: f64,
    pub h: f64,
    /// Jobs: products and sheets; products: default quantity and 0.
    pub count_a: usize,
    pub count_b: usize,
    /// Seconds since 1970 of the last save.
    pub updated: u64,
    /// Products: set up with their own technology (leads, joints, quality)
    /// or texts — "preprogrammed".
    #[serde(default)]
    pub tech: bool,
    /// Who last saved it (LB5); `updated` is when. None for a record saved
    /// before saves recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_by: Option<Who>,
}

impl Entry {
    /// "Saved by sam on PROG-2, 9 Nov 09:20" (LB6); "Saved 9 Nov 09:20" for
    /// a record saved before saves recorded who.
    pub fn saved_note(&self, clock: &Clock) -> String {
        match &self.saved_by {
            Some(w) => format!("Saved by {}, {}", w.label(), clock.shown(self.updated)),
            None if self.updated > 0 => format!("Saved {}", clock.shown(self.updated)),
            None => String::new(),
        }
    }
}

/// A Library record as a job or a part in it was read from it, or last
/// saved to it: its number, and its revision then (LB7). Every save of a
/// record gives it the next revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: u64,
    pub revision: u64,
}

/// Where a Library save goes (LB7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveTo {
    /// A new record.
    New,
    /// Over the record as it was read: refused when someone saved it since.
    Read(Record),
    /// Over record `id`, never read here (a part saved under a name a
    /// library part already has): refused while that record is there.
    Unread(u64),
    /// Over record `id`, whatever it holds now (the two-PC check's own
    /// records).
    Over(u64),
}

impl SaveTo {
    /// The record this save goes over, if any.
    pub fn id(self) -> Option<u64> {
        match self {
            SaveTo::New => None,
            SaveTo::Read(r) => Some(r.id),
            SaveTo::Unread(id) | SaveTo::Over(id) => Some(id),
        }
    }
}

/// Someone saved a record since it was read (LB7): which, who and when.
#[derive(Clone, Debug, PartialEq)]
pub struct Changed {
    pub kind: Kind,
    pub id: u64,
    /// Its name as saved now.
    pub name: String,
    /// Who saved it (LB5); None when the PC that saved it didn't record it.
    pub by: Option<Who>,
    /// When it was saved (seconds since 1970).
    pub at: u64,
    /// Its revision now: what a save that overwrites it was read at.
    pub revision: u64,
}

impl Changed {
    /// "sam on PROG-1, 9 Nov 09:30" (LB6).
    pub fn saved_by(&self, clock: &Clock) -> String {
        format!("{}, {}", self.by.as_ref().map_or_else(|| "another PC".to_string(), |w| w.label()), clock.shown(self.at))
    }

    /// The same for agents: "sam on PROG-1 at 2026-11-09 09:30" (LB6).
    pub fn saved_by_stamp(&self, clock: &Clock) -> String {
        format!("{} at {}", self.by.as_ref().map_or_else(|| "another PC".to_string(), |w| w.label()), clock.stamp(self.at))
    }

    /// The record as it is now, read at its revision now.
    pub fn record(&self) -> Record {
        Record { id: self.id, revision: self.revision }
    }
}

/// Why a Library save stored nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// Someone saved the record since it was read (LB7).
    Changed(Changed),
    /// It couldn't be saved (the share, the write lock, a name taken…).
    Failed(String),
}

impl From<String> for Refused {
    fn from(e: String) -> Refused {
        Refused::Failed(e)
    }
}

impl From<Refused> for String {
    fn from(r: Refused) -> String {
        r.to_string()
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.words(&Clock::pc()))
    }
}

impl Refused {
    /// In words, for a status line or an agent's error, with times as
    /// `clock` shows them.
    pub fn words(&self, clock: &Clock) -> String {
        match self {
            Refused::Changed(c) => format!("{} was saved by {} since it was read here. Nothing was saved", c.name, c.saved_by(clock)),
            Refused::Failed(e) => e.clone(),
        }
    }
}

/// What a save reads of the record it goes over, under the write lock
/// (LB7): its revision, and who saved it, when and under what name.
#[derive(Deserialize, Default)]
struct OnDisk {
    /// Jobs: the record and its revision.
    #[serde(default)]
    library: Option<Record>,
    /// Library parts: their revision.
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    saved: Option<Saved>,
    /// Jobs: their name.
    #[serde(default)]
    name: Option<String>,
    /// Library parts: their part, for its name.
    #[serde(default)]
    part: Option<NameOnly>,
}

#[derive(Deserialize, Default)]
struct NameOnly {
    #[serde(default)]
    name: String,
}

impl OnDisk {
    fn revision(&self) -> u64 {
        self.library.map_or(self.revision, |r| r.revision)
    }
}

#[derive(Serialize, Deserialize)]
struct Index {
    format: String,
    version: u32,
    entries: Vec<Entry>,
}

/// A product as stored: the part plus its material and thickness.
#[derive(Serialize, Deserialize)]
pub struct ProductFile {
    pub format: String,
    pub version: u32,
    pub part: Part,
    pub folder: String,
    pub material: usize,
    pub thickness: f64,
    /// Who saved it, and when (LB5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<Saved>,
    /// Its revision: every save gives it the next one (LB7). 0 for a part
    /// saved before revisions.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub revision: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// A Library folder, as one user on one PC works with it: what they save
/// records them and the time by their clock (LB5).
pub struct Library {
    pub dir: PathBuf,
    pub who: Who,
    pub clock: Clock,
}

/// Said when the Library folder isn't there (LB11): the share, the server or
/// the network is down, or the folder was moved. Names the folder. This PC's
/// own default folder that was never made says so instead.
pub fn unreachable(dir: &Path) -> String {
    if dir == default_dir() {
        format!("There's no Library on this PC yet ({}). Make one in Settings → Folders, or pick the shop's Library folder there.", dir.display())
    } else {
        format!("The Library on {} can't be reached. Check the network or the server.", dir.display())
    }
}

/// Said when the folder is there but holds no Library (LB12).
pub fn no_library(dir: &Path) -> String {
    format!("{} holds no Library. Pick the Library's folder in Settings → Folders, or make a new Library there.", dir.display())
}

/// The folder holds a Library: its list, or its record or stock folders
/// (LB12). A Library from before the list was kept still counts.
pub fn holds_library(dir: &Path) -> bool {
    dir.join(INDEX).is_file() || [Kind::Job.folder(), Kind::Product.folder(), crate::stock::FOLDER].iter().any(|f| dir.join(f).is_dir())
}

/// The Library in `dir` can be used: the folder is there (LB11) and holds a
/// Library (LB12). Nothing is made either way.
pub fn usable(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(unreachable(dir));
    }
    if !holds_library(dir) {
        return Err(no_library(dir));
    }
    Ok(())
}

/// Make a new, empty Library in `dir`, and the folder if it isn't there
/// (LB12). Only for a folder the user picked in Settings and confirmed; a
/// folder that already holds a Library is left as it is.
pub fn create(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("A new Library can't be made in {} ({e}). Check the network or the server, or pick another folder.", dir.display()))?;
    if holds_library(dir) {
        return Ok(());
    }
    // An empty list records no save, so who and the clock don't matter here.
    let lib = Library::at(dir, &Who::this_pc(), &Clock::pc());
    let lock = lib.lock()?;
    if !holds_library(dir) {
        lib.write_index(&[], &lock)?;
    }
    Ok(())
}

impl Library {
    /// The Library in `dir`, as `who` going by `clock`, without setting
    /// anything up (reading).
    pub fn at(dir: &Path, who: &Who, clock: &Clock) -> Library {
        Library { dir: dir.to_path_buf(), who: who.clone(), clock: clock.clone() }
    }

    /// Open the Library in a folder, as `who` going by `clock`, setting up
    /// its list if it was lost. A folder that isn't there, or holds no
    /// Library, is refused, naming it, and nothing is made (LB11, LB12).
    pub fn open(dir: &Path, who: &Who, clock: &Clock) -> Result<Self, String> {
        usable(dir)?;
        let lib = Library::at(dir, who, clock);
        if !dir.join(INDEX).exists() {
            let first = lib.scan(&Scanned::new());
            let lock = lib.lock()?;
            if !dir.join(INDEX).exists() {
                lib.write_index(&entries_of(lib.scan(&first)), &lock)?;
            }
        }
        // A Library from before folders gets its job folders now (JX11).
        crate::folders::ensure(&lib, Kind::Job)?;
        Ok(lib)
    }

    /// The Library's write lock: every change to its files is made under it
    /// (LB1).
    pub fn lock(&self) -> Result<WriteLock, String> {
        WriteLock::take(&self.dir)
    }

    /// Where a record's file is: <dir>\<Kind>\<id / 1000>\<id>.<ext>.
    pub fn path(&self, kind: Kind, id: u64) -> PathBuf {
        self.dir.join(kind.folder()).join((id / PER_FOLDER).to_string()).join(format!("{id}.{}", kind.ext()))
    }

    pub fn entries(&self) -> Vec<Entry> {
        std::fs::read_to_string(self.dir.join(INDEX))
            .ok()
            .and_then(|t| serde_json::from_str::<Index>(&t).ok())
            .map(|i| i.entries)
            .unwrap_or_else(|| self.rescan())
    }

    /// The list as stored, or why it can't be read (the Library check).
    pub fn read_index(&self) -> Result<Vec<Entry>, String> {
        let t = std::fs::read_to_string(self.dir.join(INDEX)).map_err(|e| e.to_string())?;
        serde_json::from_str::<Index>(&t).map(|i| i.entries).map_err(|e| e.to_string())
    }

    fn write_index(&self, entries: &[Entry], lock: &WriteLock) -> Result<(), String> {
        let i = Index { format: FORMAT.into(), version: VERSION, entries: entries.to_vec() };
        shared::write_file(&self.dir.join(INDEX), serde_json::to_string_pretty(&i).map_err(|e| e.to_string())?.as_bytes(), Some(lock))
    }

    /// Rebuild the index from the record files (after copying folders in,
    /// or if the index is lost).
    pub fn rescan(&self) -> Vec<Entry> {
        entries_of(self.scan(&Scanned::new()))
    }

    /// Every record file's line in the list, with the file's stamp (when it
    /// was last written, and its size). A file whose stamp is in `known` is
    /// not read again.
    fn scan(&self, known: &Scanned) -> Scanned {
        let mut out = Scanned::new();
        for kind in [Kind::Product, Kind::Job] {
            let root = self.dir.join(kind.folder());
            let Ok(subs) = std::fs::read_dir(&root) else { continue };
            for sub in subs.flatten() {
                let Ok(files) = std::fs::read_dir(sub.path()) else { continue };
                for f in files.flatten() {
                    let p = f.path();
                    let Some(id) = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()) else { continue };
                    let meta = f.metadata().ok();
                    let stamp = (meta.as_ref().and_then(|m| m.modified().ok()), meta.as_ref().map_or(0, |m| m.len()));
                    if let Some((st, e)) = known.get(&(kind as u8, id)).filter(|(st, _)| st.0.is_some() && *st == stamp) {
                        out.insert((kind as u8, id), (*st, e.clone()));
                        continue;
                    }
                    let written = stamp.0.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                    let e = match kind {
                        Kind::Job => crate::jobfile::load(&p).ok().map(|j| job_entry(id, &j, written)),
                        Kind::Product => std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<ProductFile>(&t).ok()).map(|pf| product_entry(id, &pf, written)),
                    };
                    if let Some(e) = e {
                        out.insert((kind as u8, id), (stamp, e));
                    }
                }
            }
        }
        out
    }

    /// Re-read the index from the record files and save it. The files are
    /// read first without the write lock (that can take a while on a big
    /// Library on the share), then only those written since are read again
    /// under it, so the lock stays short (LB1, LB2).
    pub fn rebuild_index(&self) -> Result<usize, String> {
        let first = self.scan(&Scanned::new());
        shared::pause("scanned");
        let lock = self.lock()?;
        let e = entries_of(self.scan(&first));
        self.write_index(&e, &lock)?;
        Ok(e.len())
    }

    /// A new record number: one more than the highest, taken by creating
    /// its file so two people saving at once on a shared folder can't get
    /// the same number (RF1: the original takes the next number under a
    /// database lock). Called under the write lock (LB1); the exclusive
    /// create still keeps a number from being taken twice.
    fn new_id(&self, kind: Kind) -> Result<u64, String> {
        // Highest number + 1, or 1 for the first (RF1).
        let mut id = self.entries().iter().filter(|e| e.kind == kind).map(|e| e.id).max().unwrap_or(0) + 1;
        loop {
            if id > REF_MAX {
                return Err(format!("the reference counter is full ({}{REF_MAX})", if kind == Kind::Job { JOB_PREFIX } else { PART_PREFIX }));
            }
            let p = self.path(kind, id);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(_) => return Ok(id),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => id += 1,
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    fn upsert(&self, e: Entry, lock: &WriteLock) -> Result<(), String> {
        let mut all = self.entries();
        all.retain(|x| !(x.kind == e.kind && x.id == e.id));
        all.push(e);
        all.sort_by_key(|e| (e.kind as u8, e.id));
        self.write_index(&all, lock)
    }

    /// Store a record and its line in the list, under the write lock, all or
    /// nothing (LB3): a new record takes the next number. When the record
    /// can't be written, nothing was replaced and a new record's number is
    /// given back. When the list can't be written, this save's own write is
    /// undone (the record it replaced put back, or the new one's number
    /// given back), only while the lock is still this PC's, so another PC's
    /// newer save is never undone (LB2). Either way the error says nothing
    /// was saved, so saving again never makes a duplicate. A save reports
    /// success only when its record was written.
    /// Once the lock is held, the record it goes over is read: when someone
    /// saved it since it was read (`to`), nothing else happens (LB7). Then
    /// `prepare` runs, still under the lock (the window books the stock
    /// there, so a refused save takes nothing from it, LB8), and `record`
    /// makes the record's text and list line, signed with when it is written
    /// (LB5) and the next revision (LB7).
    fn store<T, E: FnOnce(u64) -> Entry>(&self, kind: Kind, to: SaveTo, prepare: impl FnOnce() -> Result<T, String>, record: impl FnOnce(T, Saved, Record) -> Result<(String, E), String>) -> Result<Record, Refused> {
        let nothing = |e: String| if e.contains("Nothing was saved") { e } else { format!("{e}. Nothing was saved") };
        let lock = self.lock()?;
        // The record being saved over: to check it (LB7), and to put back if
        // this save fails.
        let before = match to.id() {
            None => None,
            Some(id) => match std::fs::read(self.path(kind, id)) {
                Ok(b) => Some(b),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(Refused::Failed(format!("can't read record {id} before saving over it ({e}). Nothing was saved"))),
            },
        };
        let now = before.as_ref().map(|b| serde_json::from_slice::<OnDisk>(b).unwrap_or_default());
        if let Some(c) = now.as_ref().and_then(|d| self.changed(kind, to, d)) {
            return Err(Refused::Changed(c));
        }
        shared::pause("revision checked");
        let read = if let SaveTo::Read(r) = to { r.revision } else { 0 };
        let revision = now.as_ref().map_or(0, |d| d.revision()).max(read) + 1;
        let prepared = prepare().map_err(nothing)?;
        let (id, new) = match to.id() {
            Some(i) => (i, false),
            None => (self.new_id(kind)?, true),
        };
        let path = self.path(kind, id);
        let give_back = |e: String| {
            // Nothing was replaced. A new record's number is given back: its
            // empty file is this save's own, as no other PC writes to a
            // number that isn't in the list (RF1).
            if new {
                let _ = std::fs::remove_file(&path);
            }
            Refused::Failed(nothing(e))
        };
        let (text, entry) = record(prepared, self.saved(), Record { id, revision }).map_err(give_back)?;
        if let Err(e) = shared::write_file(&path, text.as_bytes(), Some(&lock)) {
            return Err(give_back(e));
        }
        let stored = Record { id, revision };
        shared::pause("record written");
        let Err(e) = self.upsert(entry(id), &lock) else { return Ok(stored) };
        // The record is stored but its line isn't: undo this save's own
        // write, only while the lock is still this PC's (checked again just
        // before the record is put back).
        let undone = match before {
            Some(b) => shared::write_file(&path, &b, Some(&lock)),
            None => lock.still_held().and_then(|_| std::fs::remove_file(&path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())),
        };
        match undone {
            Ok(()) => Err(Refused::Failed(nothing(e))),
            // Couldn't be undone (the lock was taken over, or the share went
            // away): the record is stored, so the save counts (an error would
            // make the next save a duplicate); its line in the list comes
            // with its next save or Rescan.
            Err(_) => Ok(stored),
        }
    }

    /// Has someone saved the record `to` goes over since it was read (LB7)?
    /// Read without the write lock, to ask before anything else is done; the
    /// save checks again under it.
    pub fn check(&self, kind: Kind, to: SaveTo) -> Result<(), Changed> {
        let Some(id) = to.id() else { return Ok(()) };
        let Ok(b) = std::fs::read(self.path(kind, id)) else { return Ok(()) };
        self.changed(kind, to, &serde_json::from_slice::<OnDisk>(&b).unwrap_or_default()).map_or(Ok(()), Err)
    }

    /// The record `to` goes over as it is now (`d`), when that isn't as it
    /// was read (LB7). A record that isn't there any more is saved again.
    fn changed(&self, kind: Kind, to: SaveTo, d: &OnDisk) -> Option<Changed> {
        let differs = match to {
            SaveTo::Read(r) => d.revision() != r.revision,
            SaveTo::Unread(_) => true,
            SaveTo::New | SaveTo::Over(_) => false,
        };
        let id = to.id()?;
        differs.then(|| Changed {
            kind,
            id,
            name: d.name.clone().or_else(|| d.part.as_ref().map(|p| p.name.clone())).unwrap_or_default(),
            by: d.saved.as_ref().map(|s| s.by.clone()),
            at: d.saved.as_ref().map_or_else(|| self.written(kind, id), |s| s.at),
            revision: d.revision(),
        })
    }

    /// When a record's file was last written (seconds since 1970): when a
    /// record saved before saves recorded it was saved.
    fn written(&self, kind: Kind, id: u64) -> u64 {
        std::fs::metadata(self.path(kind, id)).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
    }

    /// Who is saving, and now (LB5). Taken once the write lock is held.
    pub(crate) fn saved(&self) -> Saved {
        Saved { by: self.who.clone(), at: self.clock.now() }
    }

    /// Save a job, recording who saved it and when (LB5) and its revision
    /// (LB7). Returns its record. A new record goes into the top-level
    /// folder named like the job's folder (JX15).
    pub fn save_job(&self, job: &JobFile, to: SaveTo) -> Result<Record, Refused> {
        self.save_job_in(job, to, None)
    }

    /// `save_job`, a new record going into the folder `folder` while that is
    /// still there (JX15).
    pub fn save_job_in(&self, job: &JobFile, to: SaveTo, folder: Option<u64>) -> Result<Record, Refused> {
        self.save_job_with(to, &job.name, folder, || Ok(job.clone()))
    }

    /// Save the job `job` makes, under the name `name`, while the write lock
    /// is held (LB1) and once nobody else has saved it since it was read
    /// (LB7) and no other job in the Library holds the name (JX16): the
    /// window books the stock in `job`, so a save refused for either reason
    /// books nothing (LB8). A new record then goes into the folder `folder`
    /// while that is still there, else the top-level folder named like the
    /// job's folder (JX15); saving over a record never moves it. All of it
    /// is one hold of the write lock, so of two PCs saving one new name at
    /// the same moment, one is stored and the other refused. A place in the
    /// folder tree that can't be written leaves the job at the top, saved.
    pub fn save_job_with(&self, to: SaveTo, name: &str, folder: Option<u64>, job: impl FnOnce() -> Result<JobFile, String>) -> Result<Record, Refused> {
        let _lock = self.lock()?;
        let job = || {
            self.check_job_name(name, to.id())?;
            job()
        };
        let mut week = String::new();
        let rec = self.store(Kind::Job, to, job, |job, saved, rec| {
            week = job.week.clone();
            let job = JobFile { saved: Some(saved), library: Some(rec), ..job };
            let text = serde_json::to_string_pretty(&job).map_err(|e| e.to_string())?;
            Ok((text, move |id| job_entry(id, &job, 0)))
        })?;
        if to.id().is_none() {
            let _ = crate::folders::change(self, Kind::Job, |t, _| Ok(t.file_new(rec.id, folder, &week)));
        }
        Ok(rec)
    }

    /// May job `id` (None: a new record, as Save as makes) be saved as
    /// `name`? Not when another job holds the name (JX16); the refusal names
    /// that job's folder. A job saved under its own name, unchanged, always
    /// may, even when an older job shares it (JX17: Libraries from before
    /// the rule).
    pub fn check_job_name(&self, name: &str, id: Option<u64>) -> Result<(), String> {
        let list = self.entries();
        if id.is_some_and(|id| list.iter().any(|e| e.kind == Kind::Job && e.id == id && e.name == name)) {
            return Ok(());
        }
        let key = name_key(name);
        match list.iter().find(|e| e.kind == Kind::Job && Some(e.id) != id && name_key(&e.name) == key) {
            // Its folder as the tree shows it (JX10), not the name written on it.
            Some(other) => Err(name_taken(other, &crate::folders::load(self, Kind::Job).map_or_else(|_| other.folder.clone(), |t| t.path(t.folder_of(other.id))))),
            None => {
                shared::pause("job name checked");
                Ok(())
            }
        }
    }

    /// A Library job, linked to its record and the revision read (LB7).
    pub fn load_job(&self, id: u64) -> Result<JobFile, String> {
        let mut j = crate::jobfile::load(&self.path(Kind::Job, id))?;
        j.library = Some(Record { id, revision: j.library.map_or(0, |r| r.revision) });
        Ok(j)
    }

    /// Save a product, recording who saved it and when (LB5) and its
    /// revision (LB7). Returns its record.
    pub fn save_product(&self, part: &Part, folder: &str, material: usize, thickness: f64, to: SaveTo) -> Result<Record, Refused> {
        // A library product is reusable stock, not one order's part: its ERP orders stay with the job.
        let mut part = part.clone();
        part.erp.clear();
        self.store(Kind::Product, to, || Ok(part), |part, saved, rec| {
            let pf = ProductFile { format: FORMAT.into(), version: VERSION, part, folder: folder.into(), material, thickness, saved: Some(saved), revision: rec.revision };
            let text = serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?;
            Ok((text, move |id| product_entry(id, &pf, 0)))
        })
    }

    pub fn load_product(&self, id: u64) -> Result<ProductFile, String> {
        let t = std::fs::read_to_string(self.path(Kind::Product, id)).map_err(|e| e.to_string())?;
        serde_json::from_str(&t).map_err(|e| format!("not a library product ({e})"))
    }

    /// The product stored under this reference. As in Lantek
    /// (rule PK1: only `PrdRef` is checked in PPRR_PPRR_00000100) the reference alone is the key;
    /// material and thickness are properties of that one product.
    pub fn find_product(&self, name: &str) -> Option<u64> {
        self.entries().into_iter().find(|e| e.kind == Kind::Product && e.name == name).map(|e| e.id)
    }

    /// The first free name for a copy of `base`: "base (1)", "base (2)"…,
    /// that no record of `kind` in the Library has, whatever its capitals,
    /// and `taken` doesn't refuse (LB9).
    pub fn free_name(&self, kind: Kind, base: &str, taken: impl Fn(&str) -> bool) -> String {
        let used: std::collections::HashSet<String> = self.entries().into_iter().filter(|e| e.kind == kind).map(|e| e.name.trim().to_lowercase()).collect();
        (1..).map(|k| format!("{base} ({k})")).find(|n| !used.contains(&n.trim().to_lowercase()) && !taken(n)).unwrap_or_else(|| base.to_string())
    }

    pub fn delete(&self, kind: Kind, id: u64) -> Result<(), String> {
        let lock = self.lock()?;
        lock.still_held()?;
        let _ = std::fs::remove_file(self.path(kind, id));
        let mut all = self.entries();
        all.retain(|x| !(x.kind == kind && x.id == id));
        self.write_index(&all, &lock)?;
        // Its place in the folder tree goes with it (JX10).
        if kind == Kind::Job {
            let _ = crate::folders::change(self, kind, |t, _| t.put(id, crate::folders::TOP));
        }
        Ok(())
    }
}

/// Record files read: (kind, number) → (stamp, line in the list).
type Scanned = std::collections::HashMap<(u8, u64), ((Option<std::time::SystemTime>, u64), Entry)>;

/// The lines of a scan, in the list's order.
fn entries_of(s: Scanned) -> Vec<Entry> {
    let mut out: Vec<Entry> = s.into_values().map(|(_, e)| e).collect();
    out.sort_by_key(|e| (e.kind as u8, e.id));
    out
}

/// What a job name is compared by: two names are one when they differ only
/// in capitals or in spaces around them, as folder names are (JX17,
/// estimated).
fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// "with no folder", or "in folder W12".
fn folder_words(folder: &str) -> String {
    if folder.trim().is_empty() { "with no folder".into() } else { format!("in folder {folder}") }
}

/// The refusal of a name `other` holds (JX16): which job, and its folder
/// (`folder`, its path in the folder tree).
fn name_taken(other: &Entry, folder: &str) -> String {
    format!(
        "the job '{}' ({}) {} already has that name, and job names are unique across the Library, whatever their capitals: pick another name",
        other.name,
        auto_ref(Kind::Job, other.id),
        folder_words(folder)
    )
}

/// The jobs whose name another job in the list also holds (JX17: Libraries
/// saved before names were unique), each with the others holding its name.
pub fn shared_job_names(entries: &[Entry]) -> std::collections::HashMap<u64, Vec<&Entry>> {
    let mut by_name: std::collections::HashMap<String, Vec<&Entry>> = Default::default();
    for e in entries.iter().filter(|e| e.kind == Kind::Job) {
        by_name.entry(name_key(&e.name)).or_default().push(e);
    }
    let mut out = std::collections::HashMap::new();
    for same in by_name.values().filter(|v| v.len() > 1) {
        for e in same {
            out.insert(e.id, same.iter().filter(|o| o.id != e.id).copied().collect());
        }
    }
    out
}

/// When and by whom a record was saved, for its line in the list: as the
/// record says (LB5), or for a record saved before, no one and `written`
/// (when its file was written).
fn saved_line(saved: Option<&Saved>, written: u64) -> (u64, Option<Who>) {
    saved.map_or((written, None), |s| (s.at, Some(s.by.clone())))
}

fn job_entry(id: u64, j: &JobFile, written: u64) -> Entry {
    let (updated, saved_by) = saved_line(j.saved.as_ref(), written);
    Entry { kind: Kind::Job, id, name: j.name.clone(), folder: j.week.clone(), material: j.spec.material, thickness: j.spec.thickness, w: j.spec.w, h: j.spec.h, count_a: j.parts.len(), count_b: j.sheets.len(), updated, tech: false, saved_by }
}

fn product_entry(id: u64, p: &ProductFile, written: u64) -> Entry {
    let (w, h) = p.part.bbox();
    let tech = p.part.tech.is_some() || !p.part.texts.is_empty();
    let (updated, saved_by) = saved_line(p.saved.as_ref(), written);
    Entry { kind: Kind::Product, id, name: p.part.name.clone(), folder: p.folder.clone(), material: p.material, thickness: p.thickness, w, h, count_a: p.part.qty as usize, count_b: 0, updated, tech, saved_by }
}

/// Thickness filters below this mean "any thickness" (rule PK3: Lantek adds
/// `AND (DIS_Thickness = '%f')` only when the value is >= 0.0).
pub const THICKNESS_FILTER_MIN: f64 = 0.0;
/// Decimals the thickness is written with in that filter ('%f').
pub const THICKNESS_FILTER_DECIMALS: i32 = 6;

/// Lantek's product filter (rule PK3): the same material when one is
/// given, the same thickness when it is not negative, and the name.
pub fn product_matches(e: &Entry, material: Option<usize>, thickness: f64, needle: &str) -> bool {
    let k = 10f64.powi(THICKNESS_FILTER_DECIMALS);
    material.is_none_or(|m| e.material == m)
        && (thickness < THICKNESS_FILTER_MIN || (e.thickness * k).round() == (thickness * k).round())
        && (needle.is_empty() || e.name.to_lowercase().contains(needle) || e.folder.to_lowercase().contains(needle))
}

/// The app's folder in the user's documents: Documents\Sheet Nesting
/// (%USERPROFILE% on Windows, $HOME elsewhere).
pub fn documents_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("Sheet Nesting")
}

/// The default library folder: Documents\Sheet Nesting\Library.
pub fn default_dir() -> PathBuf {
    documents_dir().join("Library")
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::nest::{Placed, Shape, SheetSpec};
    use crate::shared::{Clock, Saved, Who};

    #[test]
    fn stores_products_and_jobs_like_record_folders() {
        let dir = std::env::temp_dir().join(format!("nestpro_lib_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create(&dir).unwrap();
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let part = Part { name: "Bracket".into(), shape: Shape::LBracket, w: 120.0, h: 80.0, qty: 5, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        assert_eq!((auto_ref(Kind::Job, 1), auto_ref(Kind::Product, 1234)), ("JOB000001".to_string(), "SHP001234".to_string()));
        let p1 = lib.save_product(&part, "ACME", 0, 3.0, SaveTo::New).unwrap().id;
        let p2 = lib.save_product(&part, "ACME", 1, 2.0, SaveTo::New).unwrap().id;
        // The reference is the key: the first "Bracket" is found whatever its material.
        assert_eq!(lib.find_product("Bracket"), Some(p1));
        assert_eq!(lib.find_product("Plate"), None);
        let es = lib.entries();
        let e2 = es.iter().find(|e| e.id == p2 && e.kind == Kind::Product).unwrap();
        assert!(product_matches(e2, Some(1), 2.0, ""));
        assert!(!product_matches(e2, Some(1), 5.0, ""));
        assert!(product_matches(e2, Some(1), -1.0, "brack"));
        assert!(product_matches(e2, None, -1.0, "acme"));
        assert!(!product_matches(e2, Some(0), 2.0, ""));
        let mut tp = part.clone();
        tp.tech = Some(Default::default());
        let p3 = lib.save_product(&tp, "ACME", 2, 1.0, SaveTo::New).unwrap().id;
        assert!(lib.entries().iter().find(|e| e.kind == Kind::Product && e.id == p3).unwrap().tech);
        lib.delete(Kind::Product, p3).unwrap();
        assert_eq!((p1, p2), (1, 2));
        assert_eq!(lib.path(Kind::Product, 1234), dir.join("Products").join("1").join("1234.json"));
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        let job = JobFile { format: crate::jobfile::FORMAT.into(), version: crate::jobfile::VERSION, name: "Frames".into(), week: "W12".into(), parts: vec![part.clone()], spec, sheets: vec![vec![Placed { part: 0, x: 5.0, y: 5.0, w: 120.0, h: 80.0, turns: 0, mirror: false }]], formats: vec![], held: Default::default(), cut_lines: vec![], method: String::new(), manual_cuts: vec![], stale: false, booked: false, erp: None, saved: None, library: None, library_parts: Default::default() };
        let j = lib.save_job(&job, SaveTo::New).unwrap().id;
        // Saving again over the same number keeps one record.
        lib.save_job(&job, SaveTo::Over(j)).unwrap();
        let e = lib.entries();
        assert_eq!(e.iter().filter(|x| x.kind == Kind::Job).count(), 1);
        assert_eq!(e.iter().filter(|x| x.kind == Kind::Product).count(), 2);
        assert_eq!(lib.load_job(j).unwrap().sheets.len(), 1);
        assert_eq!(lib.load_product(p2).unwrap().thickness, 2.0);
        // The index can be rebuilt from the files alone.
        std::fs::remove_file(dir.join(INDEX)).unwrap();
        assert_eq!(lib.rebuild_index().unwrap(), 3);
        lib.delete(Kind::Product, p1).unwrap();
        assert_eq!(lib.entries().iter().filter(|x| x.kind == Kind::Product).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A new, empty Library of the test's own.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_shared_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create(&dir).unwrap();
        dir
    }

    fn job(name: &str) -> JobFile {
        let spec = SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
        JobFile::new_job(name, spec)
    }

    #[test]
    fn two_pcs_saving_at_once_keep_every_job_in_the_list() {
        let dir = scratch("two_pcs");
        let (pcs, saves) = (2, 40);
        let start = std::sync::Arc::new(std::sync::Barrier::new(pcs));
        let runs: Vec<_> = (0..pcs)
            .map(|pc| {
                let (dir, start) = (dir.clone(), start.clone());
                std::thread::spawn(move || {
                    let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
                    start.wait();
                    (0..saves).map(|n| (format!("PROG-{pc} job {n}"), lib.save_job(&job(&format!("PROG-{pc} job {n}")), SaveTo::New).map(|r| r.id))).collect::<Vec<_>>()
                })
            })
            .collect();
        let saved: Vec<(String, Result<u64, Refused>)> = runs.into_iter().flat_map(|r| r.join().unwrap()).collect();
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let list = lib.entries();
        for (name, r) in &saved {
            let id = *r.as_ref().unwrap_or_else(|e| panic!("saving {name} reported an error: {e}"));
            assert_eq!(lib.load_job(id).unwrap().name, *name, "record {id} holds another job");
            assert!(list.iter().any(|e| e.kind == Kind::Job && e.id == id && e.name == *name), "{name} (record {id}) is missing from the Library's list");
        }
        assert_eq!(list.iter().filter(|e| e.kind == Kind::Job).count(), pcs * saves);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11, LB12: a Library folder that isn't there (the share is down, or
    /// it was never made) is never made by opening or saving; only making a
    /// new Library (Settings, after asking) makes one.
    #[test]
    fn a_library_folder_that_isnt_there_is_never_made() {
        let dir = scratch("not_there").join("Nesting");
        let (who, clock) = (Who::new("sam", "PROG-1"), Clock::pc());
        let refused = Library::open(&dir, &who, &clock).err().expect("a folder that isn't there was opened");
        assert!(refused.contains(&dir.display().to_string()) && refused.contains("can't be reached"), "{refused}");
        assert!(!dir.exists(), "opening made the Library folder");
        // Made on purpose, it opens, empty.
        create(&dir).unwrap();
        let lib = Library::open(&dir, &who, &clock).unwrap();
        assert!(lib.entries().is_empty());
        lib.save_job(&job("Frames"), SaveTo::New).unwrap();
        // The share goes away: saving is refused, naming the folder, and
        // nothing is made in its place.
        std::fs::remove_dir_all(&dir).unwrap();
        let Some(Refused::Failed(refused)) = lib.save_job(&job("Brackets"), SaveTo::New).err() else { panic!("a save to a folder that isn't there was stored") };
        assert!(refused.contains(&dir.display().to_string()) && refused.contains("can't be reached"), "{refused}");
        assert!(!dir.exists(), "saving made the Library folder again");
        // A folder that's there but holds no Library is refused too, and left
        // as it was.
        std::fs::create_dir_all(&dir).unwrap();
        let refused = Library::open(&dir, &who, &clock).err().expect("an empty folder was opened as a Library");
        assert!(refused.contains(&dir.display().to_string()) && refused.contains("holds no Library"), "{refused}");
        assert!(files(&dir).is_empty(), "opening set up a Library in an empty folder");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// Every file under `dir`, as paths relative to it.
    fn files(dir: &Path) -> Vec<String> {
        let mut out = vec![];
        let mut todo = vec![dir.to_path_buf()];
        while let Some(d) = todo.pop() {
            for f in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                if f.path().is_dir() {
                    todo.push(f.path());
                } else {
                    out.push(f.path().strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_save_that_fails_leaves_nothing_behind_and_saving_again_makes_no_duplicate() {
        let dir = scratch("failed_save");
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let mut frames = job("Frames");
        frames.week = "W12".into();
        let first = lib.save_job(&frames, SaveTo::New).unwrap().id;
        let before = files(&dir);
        // The Library's list can't be written (here: something else sits
        // where it goes), as when the share drops in the middle of a save.
        std::fs::remove_file(dir.join(INDEX)).unwrap();
        std::fs::create_dir_all(dir.join(INDEX).join("in the way")).unwrap();
        let blocked = files(&dir);
        // A new job: refused, and neither its record nor its number is left.
        assert!(lib.save_job(&job("Brackets"), SaveTo::New).is_err());
        assert_eq!(files(&dir), blocked, "a failed new save left files behind");
        assert!(lib.rescan().iter().all(|e| e.name != "Brackets"), "a failed save left a record");
        // Saving over a stored job: refused, and the stored job is as it was.
        let mut changed = frames.clone();
        changed.week = "W13".into();
        assert!(lib.save_job(&changed, SaveTo::Over(first)).is_err());
        assert_eq!(lib.load_job(first).unwrap().week, "W12");
        assert_eq!(files(&dir), blocked, "a failed save over a record left files behind");
        // Once the list can be written again, saving again stores the job once.
        std::fs::remove_dir_all(dir.join(INDEX)).unwrap();
        lib.rebuild_index().unwrap();
        assert_eq!(files(&dir), before);
        let again = lib.save_job(&job("Brackets"), SaveTo::New).unwrap().id;
        assert_eq!(again, first + 1, "the failed save kept a record number");
        let list = lib.entries();
        assert_eq!(list.iter().filter(|e| e.name == "Brackets").count(), 1);
        assert_eq!(list.iter().filter(|e| e.kind == Kind::Job).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB3: when the record itself can't be written, the save says so, even
    /// if nothing could be tidied up either; it never reports a save that
    /// didn't happen.
    #[cfg(unix)]
    #[test]
    fn a_save_whose_record_cant_be_written_is_never_reported_as_saved() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = scratch("record_unwritable");
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let mut frames = job("Frames");
        frames.week = "W12".into();
        let first = lib.save_job(&frames, SaveTo::New).unwrap().id;
        // The record's folder takes no new files (a read-only share folder).
        let folder = lib.path(Kind::Job, first).parent().unwrap().to_path_buf();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(folder.join("probe"), b"x").is_ok() {
            // Running as root: permissions don't stop writes, nothing to test.
            let _ = std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755));
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let mut changed = frames.clone();
        changed.week = "W13".into();
        let r = lib.save_job(&changed, SaveTo::Over(first)).map(|r| r.id);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(r.is_err(), "a save that wrote nothing was reported as saved: {r:?}");
        assert_eq!(lib.load_job(first).unwrap().week, "W12");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PROG-2 takes the write lock over (as abandoned) and saves `name` into
    /// `record`, as happens while this PC is stalled in the middle of a save.
    fn prog2_takes_over_and_saves(dir: &Path, record: &Path, name: &str, week: &str) {
        std::fs::write(dir.join(shared::LOCK_FILE), r#"{"user":"kim","pc":"PROG-2","process":9,"since":0,"token":"PROG-2-9-1"}"#).unwrap();
        let mut j = job(name);
        j.week = week.into();
        std::fs::write(record, serde_json::to_string_pretty(&j).unwrap()).unwrap();
    }

    /// LB3: a save that fails after its record was written puts nothing back
    /// once another PC has taken the write lock over, so the other PC's newer
    /// save of that record is never undone, whether this PC was saving over a
    /// record or making a new one.
    #[test]
    fn a_failed_save_never_undoes_another_pcs_newer_save() {
        let dir = scratch("undo_after_takeover");
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let mut frames = job("Frames");
        frames.week = "W12".into();
        let first = lib.save_job(&frames, SaveTo::New).unwrap().id;
        // Saving over the job: just after this PC wrote the record, PROG-2
        // takes over and saves the job as W14.
        let rec = lib.path(Kind::Job, first);
        {
            let (dir, rec) = (dir.clone(), rec.clone());
            shared::hooks::set(move |s| {
                if s == "record written" {
                    prog2_takes_over_and_saves(&dir, &rec, "Frames", "W14");
                }
            });
        }
        let mut changed = frames.clone();
        changed.week = "W13".into();
        let r = lib.save_job(&changed, SaveTo::Over(first)).map(|r| r.id);
        shared::hooks::clear();
        assert_eq!(lib.load_job(first).unwrap().week, "W14", "this PC's failed save put its old copy back over PROG-2's newer save ({r:?})");
        // Its record was written before PROG-2 saved over it: the save counts.
        assert_eq!(r, Ok(first));
        // PROG-2 is done. A new job: PROG-2 takes over just after this PC
        // wrote the new record, and saves over that record.
        std::fs::remove_file(dir.join(shared::LOCK_FILE)).unwrap();
        let next = lib.path(Kind::Job, first + 1);
        {
            let (dir, next) = (dir.clone(), next.clone());
            shared::hooks::set(move |s| {
                if s == "record written" {
                    prog2_takes_over_and_saves(&dir, &next, "Brackets", "W15");
                }
            });
        }
        let r = lib.save_job(&job("Brackets"), SaveTo::New).map(|r| r.id);
        shared::hooks::clear();
        assert_eq!(lib.load_job(first + 1).map(|j| j.week), Ok("W15".to_string()), "this PC's failed save removed PROG-2's newer save ({r:?})");
        assert_eq!(r, Ok(first + 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 2026-11-09 09:20 UTC: a test clock shows UTC.
    const NOV_9_0920: u64 = 1_794_216_000;

    /// LB5, LB6: two PCs on one Library each save a job; the list carries
    /// who saved each one and when, and shows it.
    #[test]
    fn the_list_shows_who_saved_each_job_and_when() {
        let dir = scratch("saved_by_jobs");
        let clock = Clock::at(NOV_9_0920);
        let prog1 = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let prog2 = Library::open(&dir, &Who::new("kim", "PROG-2"), &clock).unwrap();
        let frames = prog1.save_job(&job("DEMO-001 frames"), SaveTo::New).unwrap().id;
        clock.advance(std::time::Duration::from_secs(25 * 60));
        let brackets = prog2.save_job(&job("DEMO-002 brackets"), SaveTo::New).unwrap().id;
        let note = |lib: &Library, id: u64| lib.entries().into_iter().find(|e| e.kind == Kind::Job && e.id == id).map(|e| e.saved_note(&clock));
        for lib in [&prog1, &prog2] {
            assert_eq!(note(lib, frames).as_deref(), Some("Saved by sam on PROG-1, 9 Nov 09:20"));
            assert_eq!(note(lib, brackets).as_deref(), Some("Saved by kim on PROG-2, 9 Nov 09:45"));
        }
        // PROG-2 saves over PROG-1's job: the job is now PROG-2's.
        clock.advance(std::time::Duration::from_secs(60));
        prog2.save_job(&job("DEMO-001 frames"), SaveTo::Over(frames)).unwrap();
        assert_eq!(note(&prog1, frames).as_deref(), Some("Saved by kim on PROG-2, 9 Nov 09:46"));
        // The record itself says so too: Rescan rebuilds the same list.
        assert_eq!(prog1.load_job(frames).unwrap().saved, Some(Saved { by: Who::new("kim", "PROG-2"), at: NOV_9_0920 + 26 * 60 }));
        let before = prog1.entries();
        prog1.rebuild_index().unwrap();
        assert_eq!(prog1.entries(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB5: a save that waits for another PC's write lock is signed with
    /// when it was written, not when it started waiting, so a save made
    /// after another one never reads as older.
    #[test]
    fn a_save_that_waits_for_the_lock_is_signed_when_it_is_written() {
        let dir = scratch("saved_after_wait");
        let clock = Clock::at(NOV_9_0920);
        let prog1 = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let part = Part { name: "DEMO-P1".into(), shape: Shape::LBracket, w: 120.0, h: 80.0, qty: 5, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        // PROG-1 holds the lock; PROG-2's saves wait while the clock moves on 10 minutes.
        let held = prog1.lock().unwrap();
        let (d, c) = (dir.clone(), clock.clone());
        let prog2 = std::thread::spawn(move || {
            let lib = Library::open(&d, &Who::new("kim", "PROG-2"), &c).unwrap();
            (lib.save_job(&job("DEMO-002 brackets"), SaveTo::New).map(|r| r.id), lib.save_product(&part, "Acme", 0, 3.0, SaveTo::New).map(|r| r.id))
        });
        std::thread::sleep(std::time::Duration::from_millis(400));
        clock.advance(std::time::Duration::from_secs(10 * 60));
        drop(held);
        let (j, p) = prog2.join().unwrap();
        let (j, p) = (j.unwrap(), p.unwrap());
        let at = NOV_9_0920 + 10 * 60;
        assert_eq!(prog1.load_job(j).unwrap().saved.map(|s| s.at), Some(at));
        assert_eq!(prog1.load_product(p).unwrap().saved.map(|s| s.at), Some(at));
        assert!(prog1.entries().iter().all(|e| e.updated == at), "{:?}", prog1.entries());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB5: a Library from before saves recorded who saved them. Its list
    /// and records load unchanged, and show when but no name, also after
    /// Rescan.
    #[test]
    fn records_saved_before_load_unchanged_and_show_no_name() {
        let dir = scratch("saved_by_old");
        let old_job = serde_json::to_string_pretty(&job("DEMO-001 frames")).unwrap();
        assert!(!old_job.contains("saved"), "{old_job}");
        let part = Part { name: "DEMO-P1".into(), shape: Shape::LBracket, w: 120.0, h: 80.0, qty: 5, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let old_part = serde_json::json!({"format": FORMAT, "version": 1, "part": part, "folder": "Acme", "material": 0, "thickness": 3.0}).to_string();
        let old_part = old_part.as_str();
        let old_index = r#"{"format":"nestpro-library","version":1,"entries":[
  {"kind":"Product","id":1,"name":"DEMO-P1","folder":"Acme","material":0,"thickness":3.0,"w":120.0,"h":80.0,"count_a":5,"count_b":0,"updated":1794216000,"tech":false},
  {"kind":"Job","id":1,"name":"DEMO-001 frames","folder":"","material":0,"thickness":3.0,"w":2000.0,"h":1000.0,"count_a":0,"count_b":0,"updated":1794216000}
]}"#;
        for (f, t) in [("Jobs/0/1.snjob", old_job.as_str()), ("Products/0/1.json", old_part), (INDEX, old_index)] {
            std::fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
            std::fs::write(dir.join(f), t).unwrap();
        }
        let clock = Clock::at(NOV_9_0920 + 3600);
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let list = lib.entries();
        assert_eq!(list.len(), 2);
        for e in &list {
            assert_eq!(e.saved_by, None);
            assert_eq!(e.saved_note(&clock), "Saved 9 Nov 09:20", "{}", e.name);
        }
        assert_eq!(lib.load_job(1).unwrap().saved, None);
        let pf = lib.load_product(1).unwrap();
        assert_eq!((pf.part.name.as_str(), pf.saved), ("DEMO-P1", None));
        // Opening and reading it changed nothing on disk.
        assert_eq!(std::fs::read_to_string(dir.join(INDEX)).unwrap(), old_index);
        assert_eq!(std::fs::read_to_string(dir.join("Jobs/0/1.snjob")).unwrap(), old_job);
        // Rescan: still no name (the time is then when the file was written).
        lib.rebuild_index().unwrap();
        assert!(lib.entries().iter().all(|e| e.saved_by.is_none() && !e.saved_note(&clock).contains(" by ")), "{:?}", lib.entries());
        // Saved again, it's signed.
        lib.save_job(&lib.load_job(1).unwrap(), SaveTo::Over(1)).unwrap();
        let e = lib.entries().into_iter().find(|e| e.kind == Kind::Job).unwrap();
        assert_eq!(e.saved_note(&clock), "Saved by sam on PROG-1, 9 Nov 10:20");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB6: dates read "9 Nov 09:20", with the year when it isn't this year.
    #[test]
    fn save_times_read_plainly() {
        let clock = Clock::at(NOV_9_0920);
        assert_eq!(clock.shown(NOV_9_0920), "9 Nov 09:20");
        assert_eq!(clock.shown(NOV_9_0920 - 365 * 86400), "9 Nov 2025 09:20");
        assert_eq!(clock.shown(1_798_761_540), "31 Dec 23:59");
        assert_eq!(clock.stamp(NOV_9_0920), "2026-11-09 09:20");
        // SV10: "at 09:20" today, the day too on another day.
        assert_eq!((clock.when(NOV_9_0920 + 60), clock.when(NOV_9_0920 - 86400)), ("at 09:21".to_string(), "on 8 Nov 09:20".to_string()));
        assert_eq!(Who::new("sam", "PROG-2").as_agent().label(), "sam on PROG-2 (agent)");
    }

    /// LB7: PROG-1 and PROG-2 both read DEMO-7; PROG-1 saves it; PROG-2's
    /// save of the copy it read is refused, naming who saved it and when,
    /// and stores nothing. Saved again from what is there now, it goes.
    #[test]
    fn a_save_over_a_job_someone_saved_since_it_was_read_is_refused() {
        let dir = scratch("changed_since");
        let clock = Clock::at(NOV_9_0920);
        let prog1 = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let prog2 = Library::open(&dir, &Who::new("kim", "PROG-2"), &clock).unwrap();
        let first = prog1.save_job(&job("DEMO-7"), SaveTo::New).unwrap();
        let (mut on1, mut on2) = (prog1.load_job(first.id).unwrap(), prog2.load_job(first.id).unwrap());
        clock.advance(std::time::Duration::from_secs(10 * 60));
        on1.week = "W12".into();
        let saved = prog1.save_job(&on1, SaveTo::Read(on1.library.unwrap())).unwrap();
        assert_eq!(saved.id, first.id);
        clock.advance(std::time::Duration::from_secs(5 * 60));
        on2.week = "W13".into();
        let stored = files(&dir);
        let refused = prog2.save_job(&on2, SaveTo::Read(on2.library.unwrap())).unwrap_err();
        let Refused::Changed(c) = &refused else { panic!("{refused:?}") };
        assert_eq!((c.name.as_str(), c.saved_by(&clock).as_str()), ("DEMO-7", "sam on PROG-1, 9 Nov 09:30"));
        assert_eq!(files(&dir), stored, "a refused save stored something");
        assert_eq!(prog2.load_job(first.id).unwrap().week, "W12");
        // LB8: what a save does before writing (the window books the stock
        // there) never runs for a refused save.
        let mut booked = false;
        let r = prog2.save_job_with(SaveTo::Read(on2.library.unwrap()), &on2.name, None, || {
            booked = true;
            Ok(on2.clone())
        });
        assert!(matches!(r, Err(Refused::Changed(_))) && !booked, "{r:?}");
        // PROG-2 reads it again: its save now goes over the same record.
        let mut again = prog2.load_job(first.id).unwrap();
        again.week = "W13".into();
        assert_eq!(prog2.save_job(&again, SaveTo::Read(again.library.unwrap())).unwrap().id, first.id);
        assert_eq!(prog1.entries().iter().filter(|e| e.kind == Kind::Job).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB1: Rescan reads the records without the write lock, then takes it
    /// and reads again only what changed meanwhile, so a save another PC
    /// made while it was reading is in the list it writes.
    #[test]
    fn a_rescan_takes_in_what_another_pc_saved_while_it_was_reading() {
        let dir = scratch("rescan");
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &Clock::pc()).unwrap();
        let frames = lib.save_job(&job("Frames"), SaveTo::New).unwrap().id;
        lib.save_job(&job("Brackets"), SaveTo::New).unwrap();
        {
            let dir = dir.clone();
            shared::hooks::set(move |s| {
                if s == "scanned" {
                    // PROG-2 renames one job and saves a new one.
                    let other = Library::at(&dir, &Who::new("kim", "PROG-2"), &Clock::pc());
                    other.save_job(&job("Frames, second batch"), SaveTo::Over(frames)).unwrap();
                    other.save_job(&job("Gussets"), SaveTo::New).unwrap();
                }
            });
        }
        let n = lib.rebuild_index();
        shared::hooks::clear();
        assert_eq!(n, Ok(3));
        let mut names: Vec<String> = lib.entries().into_iter().map(|e| e.name).collect();
        names.sort();
        assert_eq!(names, ["Brackets", "Frames, second batch", "Gussets"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX16, JX17: no two jobs in the Library share a name, in any folder,
    /// whatever its case. A new save, Save as (the job saved as a new
    /// record) and a rename (saved over its own record under a new name) to
    /// a name another job holds are refused, naming that job's folder, and
    /// nothing is stored.
    #[test]
    fn job_names_are_unique_across_the_library_ignoring_case() {
        let dir = scratch("unique_names");
        // Sam on PROG-1 saved the frames job; Kim on PROG-2 works on the
        // same Library.
        let clock = Clock::at(NOV_9_0920);
        let prog1 = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let lib = Library::open(&dir, &Who::new("kim", "PROG-2"), &clock).unwrap();
        let mut frames = job("DEMO-001 frames");
        frames.week = "Acme".into();
        let frames_id = prog1.save_job(&frames, SaveTo::New).unwrap().id;
        let mut brackets = job("DEMO-002 brackets");
        brackets.week = "W12".into();
        let brackets_id = lib.save_job(&brackets, SaveTo::New).unwrap().id;
        let stored = files(&dir);
        // A new job under a name another job holds, in another folder and
        // in other capitals.
        let mut new = job("demo-001 FRAMES");
        new.week = "W12".into();
        let e = lib.save_job(&new, SaveTo::New).unwrap_err().to_string();
        assert!(e.contains("DEMO-001 frames") && e.contains("folder Acme") && e.contains("Nothing was saved"), "{e}");
        // Save as: the brackets job saved as a new record under that name.
        let mut copy = brackets.clone();
        copy.name = "DEMO-001 Frames".into();
        let e = lib.save_job(&copy, SaveTo::New).unwrap_err().to_string();
        assert!(e.contains("folder Acme"), "{e}");
        // Rename: the brackets job saved over its own record under that name.
        let e = lib.save_job(&copy, SaveTo::Over(brackets_id)).unwrap_err().to_string();
        assert!(e.contains("folder Acme"), "{e}");
        assert_eq!(files(&dir), stored, "a refused save stored something");
        assert_eq!(lib.load_job(brackets_id).unwrap().name, "DEMO-002 brackets");
        // Its own name in other capitals, or a free name: saved.
        brackets.name = "DEMO-002 BRACKETS".into();
        lib.save_job(&brackets, SaveTo::Over(brackets_id)).unwrap();
        frames.name = "DEMO-003 frames".into();
        lib.save_job(&frames, SaveTo::Over(frames_id)).unwrap();
        // The old name is free once its job has another.
        assert!(lib.save_job(&new, SaveTo::New).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX17: a Library saved before names were unique, where two jobs in
    /// different folders share a name. Both keep saving under their own
    /// name, and both are marked, each naming the other, until one is
    /// renamed; neither may take the other's name.
    #[test]
    fn jobs_that_already_share_a_name_keep_saving_and_are_marked() {
        let dir = scratch("old_twins");
        let (who, clock) = (Who::new("sam", "PROG-1"), Clock::at(NOV_9_0920));
        // Written as an older version did, with no name check and before
        // folders: the first open files them by their folder names (JX11).
        let old = Library::at(&dir, &who, &clock);
        for (id, name, folder) in [(1, "DEMO-001 frames", "Acme"), (2, "demo-001 FRAMES", "W12"), (3, "DEMO-002 brackets", "W12")] {
            let mut j = job(name);
            j.week = folder.into();
            let p = old.path(Kind::Job, id);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, serde_json::to_string_pretty(&j).unwrap()).unwrap();
        }
        old.rebuild_index().unwrap();
        let lib = Library::open(&dir, &who, &clock).unwrap();
        let marked = |lib: &Library| {
            let list = lib.entries();
            let mut m: Vec<(u64, Vec<u64>)> = shared_job_names(&list).into_iter().map(|(id, o)| (id, o.iter().map(|e| e.id).collect())).collect();
            m.sort();
            m
        };
        assert_eq!(marked(&lib), vec![(1, vec![2]), (2, vec![1])]);
        // Each keeps saving under its own name.
        let mut first = lib.load_job(1).unwrap();
        first.parts.clear();
        lib.save_job(&first, SaveTo::Over(1)).unwrap();
        lib.save_job(&lib.load_job(2).unwrap(), SaveTo::Over(2)).unwrap();
        // Neither takes the other's spelling of the name.
        let mut second = lib.load_job(2).unwrap();
        second.name = "DEMO-001 frames".into();
        let e = lib.save_job(&second, SaveTo::Over(2)).unwrap_err().to_string();
        assert!(e.contains("folder Acme"), "{e}");
        // Renamed, it's no longer marked.
        second.name = "DEMO-004 frames, second batch".into();
        lib.save_job(&second, SaveTo::Over(2)).unwrap();
        assert_eq!(marked(&lib), vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
