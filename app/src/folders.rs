//! The Library's folder trees: one for its jobs (JX10–JX15) and one for its
//! library parts (JX18–JX20), each kept in the Library folder, the same on
//! every PC sharing the Library. As in the original's tables (DT8, DT9: job
//! and product folders in one table, told apart by which tree), each folder
//! has a number of its own, a name and a parent, and each saved job's or
//! part's place is a line naming its record and its folder's number:
//! renaming or moving a folder changes only its own line, and moving a job
//! or part only its line, never its record.
//!
//! Every change is made under the Library's write lock (LB1–LB3): the tree
//! is read again under it, changed and written back whole, so two PCs'
//! changes never undo each other.

use crate::library::{Entry, Kind, Library};
use crate::shared::{self, WriteLock};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The Library's top: the parent of the top-level folders, and the place of
/// a job in no folder.
pub const TOP: u64 = 0;
pub const FORMAT: &str = "nestpro-folders";
pub const VERSION: u32 = 1;

/// The tree's file in the Library folder.
pub fn file_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Job => "job-folders.json",
        Kind::Product => "part-folders.json",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: u64,
    pub name: String,
    /// The folder it is in (TOP for a top-level folder).
    pub parent: u64,
}

/// The folder tree as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tree {
    pub format: String,
    pub version: u32,
    /// The highest folder number given so far: a number is never given
    /// twice, so a PC still showing a deleted folder can't reach a new one.
    pub last: u64,
    pub folders: Vec<Folder>,
    /// Each filed record's folder: record number → folder number. A record
    /// with no line is at the top.
    pub placed: BTreeMap<u64, u64>,
}

impl Default for Tree {
    fn default() -> Self {
        Tree { format: FORMAT.into(), version: VERSION, last: 0, folders: vec![], placed: BTreeMap::new() }
    }
}

/// What a tree holds, in words: "job" or "part" (`n` of them: "2 jobs").
pub fn noun(kind: Kind, n: usize) -> String {
    let one = match kind {
        Kind::Job => "job",
        Kind::Product => "part",
    };
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// The agents' operation for a tree: `job-folders` or `part-folders`.
pub fn op_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Job => "job-folders",
        Kind::Product => "part-folders",
    }
}

/// Why a change to the tree was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// The folder, job or part (named) is no longer in the Library.
    Gone(String),
    /// A folder under the same parent has the name, without case (JX4–JX6:
    /// the original's error 9).
    Taken { name: String, place: String },
    /// No name.
    NoName,
    /// Delete: the folder (`id`) still holds jobs (parts) or folders (JX13).
    NotEmpty { id: u64, name: String, records: usize, folders: usize },
    /// Move: into the folder itself or one of its own folders (JX12).
    IntoItself { name: String },
    /// "New folder (1)" … "(9)" are all taken (JX7).
    NoDefaultName { place: String },
    /// A job someone else has in use would move, or a folder holding one be
    /// renamed, moved or deleted (CU11; for an agent, a job anyone holds,
    /// CU12): what and who, in words.
    InUse(String),
    /// The tree couldn't be read or written.
    Failed(String),
}

/// The original's error for a folder name that's taken (JX4).
pub const NAME_TAKEN: i32 = crate::jobtree::FOLDER_NAME_TAKEN;

impl Refused {
    /// The refusal of a change to the `kind` tree in plain words, ending
    /// with what to do instead: in the window (`agent` false) or for an
    /// agent.
    pub fn say(&self, kind: Kind, agent: bool) -> String {
        let again = if agent { format!("`{} mode=list` shows the folders as they are now", op_name(kind)) } else { "Refresh to see the folders as they are now".into() };
        match self {
            Refused::Gone(what) => format!("{what} is no longer in the Library: another PC may have deleted it. {again}."),
            Refused::Taken { name, place } => format!("A folder called '{name}' is already in {place} (names are compared without case, error {NAME_TAKEN}). Pick another name."),
            Refused::NoName => "A folder needs a name.".into(),
            Refused::NotEmpty { name, records, folders, .. } => {
                let mut what = vec![];
                if *records > 0 {
                    what.push(noun(kind, *records));
                }
                if *folders > 0 {
                    what.push(format!("{folders} folder{}", if *folders == 1 { "" } else { "s" }));
                }
                format!("'{name}' still holds {}: only an empty folder can be deleted. Move or delete what's in it first.", what.join(" and "))
            }
            Refused::IntoItself { name } => format!("'{name}' can't go into itself or one of its own folders. Paste it somewhere outside '{name}'."),
            Refused::NoDefaultName { place } => format!("'{} (1)' to '(9)' are all taken in {place}. Rename one of them first.", crate::jobtree::NEW_FOLDER_NAME),
            Refused::InUse(said) => format!("{said} {}", if agent { "Try again later. Nothing was changed." } else { "Try again when they've closed it." }),
            Refused::Failed(e) => e.clone(),
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Tree {
    pub fn get(&self, id: u64) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// The folders in `parent`, by name ignoring case (as the database
    /// tree is shown).
    pub fn children(&self, parent: u64) -> Vec<&Folder> {
        let mut c: Vec<&Folder> = self.folders.iter().filter(|f| f.parent == parent).collect();
        c.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        c
    }

    /// The folder a record is in: TOP when it has no line, or its folder is
    /// gone.
    pub fn folder_of(&self, record: u64) -> u64 {
        self.placed.get(&record).copied().filter(|f| self.get(*f).is_some()).unwrap_or(TOP)
    }

    /// The records of `entries` in `folder`, in the list's order.
    pub fn records_in<'a>(&self, folder: u64, entries: &'a [Entry]) -> Vec<&'a Entry> {
        entries.iter().filter(|e| self.folder_of(e.id) == folder).collect()
    }

    /// How many of `entries` are in `folder` or any folder inside it.
    pub fn records_within(&self, folder: u64, entries: &[Entry]) -> usize {
        entries.iter().filter(|e| self.is_within(self.folder_of(e.id), folder)).count()
    }

    /// "Acme/2026-11"; "" for the top.
    pub fn path(&self, id: u64) -> String {
        let mut names = vec![];
        let mut at = id;
        while let Some(f) = self.get(at) {
            names.push(f.name.clone());
            at = f.parent;
            if names.len() > self.folders.len() {
                break;
            }
        }
        names.reverse();
        names.join("/")
    }

    /// Where a folder is, in words: "the top of the Library" or "'Acme'".
    pub fn place(&self, id: u64) -> String {
        if id == TOP { "the top of the Library".into() } else { format!("'{}'", self.path(id)) }
    }

    /// The folder at a path ("Acme/2026-11", names separated by / or \,
    /// compared without case), or `#12` for folder number 12. "" or "/" is
    /// the top.
    pub fn find(&self, path: &str) -> Option<u64> {
        let p = path.trim();
        if let Some(n) = p.strip_prefix('#').and_then(|n| n.trim().parse::<u64>().ok()) {
            return (n == TOP || self.get(n).is_some()).then_some(n);
        }
        let mut at = TOP;
        for name in p.split(['/', '\\']).map(str::trim).filter(|n| !n.is_empty()) {
            at = self.folders.iter().find(|f| f.parent == at && same(&f.name, name))?.id;
        }
        Some(at)
    }

    /// Is `id` the folder `of` or inside it?
    pub fn is_within(&self, id: u64, of: u64) -> bool {
        let mut at = id;
        for _ in 0..=self.folders.len() {
            if at == of {
                return true;
            }
            match self.get(at) {
                Some(f) => at = f.parent,
                None => return false,
            }
        }
        false
    }

    fn need(&self, id: u64) -> Result<&Folder, Refused> {
        self.get(id).ok_or_else(|| Refused::Gone("That folder".into()))
    }

    fn need_parent(&self, id: u64) -> Result<(), Refused> {
        if id == TOP { Ok(()) } else { self.need(id).map(|_| ()) }
    }

    /// The sibling check (JX4–JX6): the name trimmed, not empty, and not
    /// used by another folder in `parent` (without case).
    fn check_name(&self, parent: u64, name: &str, except: Option<u64>) -> Result<String, Refused> {
        let n = name.trim();
        if n.is_empty() {
            return Err(Refused::NoName);
        }
        if self.folders.iter().any(|f| f.parent == parent && Some(f.id) != except && same(&f.name, n)) {
            return Err(Refused::Taken { name: n.to_string(), place: self.place(parent) });
        }
        Ok(n.to_string())
    }

    /// New folder in `parent` (JX4): the next number, after the sibling check.
    pub fn make(&mut self, parent: u64, name: &str) -> Result<u64, Refused> {
        self.need_parent(parent)?;
        let n = self.check_name(parent, name, None)?;
        self.last = self.last.max(self.folders.iter().map(|f| f.id).max().unwrap_or(0)) + 1;
        self.folders.push(Folder { id: self.last, name: n, parent });
        Ok(self.last)
    }

    /// The default name for a new folder in `parent` (JX7): "New folder",
    /// then "New folder (1)" … "(9)".
    pub fn new_name(&self, parent: u64) -> Result<String, Refused> {
        let siblings: Vec<String> = self.folders.iter().filter(|f| f.parent == parent).map(|f| f.name.clone()).collect();
        crate::jobtree::new_folder_name(&siblings).ok_or_else(|| Refused::NoDefaultName { place: self.place(parent) })
    }

    /// Change name (JX5): the same sibling check, then only the name changes.
    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), Refused> {
        let parent = self.need(id)?.parent;
        let n = self.check_name(parent, name, Some(id))?;
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.name = n;
        }
        Ok(())
    }

    /// Move a folder into another (JX5): never into itself or one of its own
    /// folders, and the sibling check in its new parent.
    pub fn move_folder(&mut self, id: u64, to: u64) -> Result<(), Refused> {
        let f = self.need(id)?.clone();
        self.need_parent(to)?;
        if self.is_within(to, id) {
            return Err(Refused::IntoItself { name: f.name });
        }
        self.check_name(to, &f.name, Some(id))?;
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.parent = to;
        }
        Ok(())
    }

    /// Delete a folder (JX13): only when it holds no records of `entries`
    /// and no folders.
    pub fn delete(&mut self, id: u64, entries: &[Entry]) -> Result<(), Refused> {
        let name = self.need(id)?.name.clone();
        let (records, folders) = (self.records_in(id, entries).len(), self.children(id).len());
        if records > 0 || folders > 0 {
            return Err(Refused::NotEmpty { id, name, records, folders });
        }
        self.folders.retain(|f| f.id != id);
        self.placed.retain(|_, f| *f != id);
        Ok(())
    }

    /// Put a record in a folder (TOP: in none).
    pub fn put(&mut self, record: u64, to: u64) -> Result<(), Refused> {
        self.need_parent(to)?;
        if to == TOP {
            self.placed.remove(&record);
        } else {
            self.placed.insert(record, to);
        }
        Ok(())
    }

    /// The top-level folder called `name` (without case), made when there is
    /// none; TOP for no name.
    pub fn top_folder_named(&mut self, name: &str) -> u64 {
        self.folder_made(&[name.to_string()]).unwrap_or(TOP)
    }

    /// The names of the folders from the top down to `id` ("Acme",
    /// "2026-11"); none for the top.
    pub fn names(&self, id: u64) -> Vec<String> {
        let mut names = vec![];
        let mut at = id;
        while let Some(f) = self.get(at) {
            names.push(f.name.clone());
            at = f.parent;
            if names.len() > self.folders.len() {
                break;
            }
        }
        names.reverse();
        names
    }

    /// The folder reached from the top by `names`, each level found without
    /// case (JX6) or made when missing (JX19); the top for none.
    pub fn folder_made(&mut self, names: &[String]) -> Result<u64, Refused> {
        let mut at = TOP;
        for n in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
            at = match self.folders.iter().find(|f| f.parent == at && same(&f.name, n)) {
                Some(f) => f.id,
                None => self.make(at, n)?,
            };
        }
        Ok(at)
    }

    /// Where a record saved into the Library for the first time goes (JX15):
    /// into `prefer` while that folder is still there, else into the
    /// top-level folder named `name`, made if missing. Returns the folder.
    pub fn file_new(&mut self, record: u64, prefer: Option<u64>, name: &str) -> u64 {
        let to = match prefer.filter(|&f| f == TOP || self.get(f).is_some()) {
            Some(f) => f,
            None => self.top_folder_named(name),
        };
        let _ = self.put(record, to);
        to
    }

    /// The tree of a Library saved before there were folders (JX11): each
    /// folder name written on its records becomes a top-level folder holding
    /// them (names that differ only in case are one folder, spelt as on the
    /// lowest-numbered record); records with none stay at the top.
    pub fn from_names(entries: &[Entry]) -> Tree {
        let mut t = Tree::default();
        let mut es: Vec<&Entry> = entries.iter().collect();
        es.sort_by_key(|e| e.id);
        for e in es {
            let f = t.top_folder_named(&e.folder);
            if f != TOP {
                t.placed.insert(e.id, f);
            }
        }
        t
    }

    /// Every folder and record as paths, folders first in each folder, by
    /// name: "Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001" — what the
    /// Library section shows, opened out.
    pub fn outline(&self, entries: &[Entry]) -> Vec<String> {
        fn walk(t: &Tree, at: u64, prefix: &str, entries: &[Entry], out: &mut Vec<String>) {
            for f in t.children(at) {
                let p = format!("{prefix}{}/", f.name);
                out.push(p.clone());
                walk(t, f.id, &p, entries, out);
            }
            for e in t.records_in(at, entries) {
                out.push(format!("{prefix}{}", e.name));
            }
        }
        let mut out = vec![];
        walk(self, TOP, "", entries, &mut out);
        out
    }
}

/// The tree as stored now, or why it can't be read. A Library without one
/// (made before folders) gets it when it is opened (`ensure`).
pub fn load(lib: &Library, kind: Kind) -> Result<Tree, String> {
    let p = lib.dir.join(file_name(kind));
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str::<Tree>(&t).map_err(|e| format!("the Library's folder tree ({}) can't be read ({e})", file_name(kind))),
        // No tree yet: a Library from before folders (JX11), never one that
        // can't be reached or a folder that holds no Library (LB11, LB12).
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::library::usable(&lib.dir)?;
            Ok(Tree::from_names(&of_kind(lib, kind)))
        }
        Err(e) => Err(format!("the Library's folder tree ({}) can't be read ({e})", file_name(kind))),
    }
}

fn of_kind(lib: &Library, kind: Kind) -> Vec<Entry> {
    lib.entries().into_iter().filter(|e| e.kind == kind).collect()
}

fn write(lib: &Library, kind: Kind, t: &Tree, lock: &WriteLock) -> Result<(), String> {
    let text = serde_json::to_string_pretty(t).map_err(|e| e.to_string())?;
    shared::write_file(&lib.dir.join(file_name(kind)), text.as_bytes(), Some(lock))
}

/// A Library opened for the first time since folders came gets its tree
/// from the folder names written on its records (JX11), once: made under the
/// write lock, so two PCs opening it at once make one tree. A tree that is
/// there but can't be read is left alone.
pub fn ensure(lib: &Library, kind: Kind) -> Result<(), String> {
    let p = lib.dir.join(file_name(kind));
    if p.exists() {
        return Ok(());
    }
    let lock = lib.lock()?;
    if p.exists() {
        return Ok(());
    }
    write(lib, kind, &Tree::from_names(&of_kind(lib, kind)), &lock)
}

/// Change the tree under the Library's write lock (JX10): read it again
/// under the lock with the records listed now, change it, write it whole.
/// Nothing is written when `f` refuses.
pub fn change<T>(lib: &Library, kind: Kind, f: impl FnOnce(&mut Tree, &[Entry]) -> Result<T, Refused>) -> Result<T, Refused> {
    // Refused, naming the folder, when the Library can't be reached or the
    // folder holds no Library: nothing is made there (LB11, LB12).
    crate::library::usable(&lib.dir).map_err(Refused::Failed)?;
    let lock = lib.lock().map_err(Refused::Failed)?;
    let mut t = load(lib, kind).map_err(|e| Refused::Failed(format!("{e}. Nothing was changed")))?;
    let entries = of_kind(lib, kind);
    let before = t.clone();
    let out = match f(&mut t, &entries) {
        // A folder that isn't empty, holding a job in use: said with who
        // has it (CU11).
        Err(Refused::NotEmpty { id, name, records, folders }) if kind == Kind::Job => {
            let not_empty = Refused::NotEmpty { id, name, records, folders };
            return Err(match in_use_within(lib, &crate::engine::inuse::look(lib).unwrap_or_default(), &before, id, &entries) {
                Some(said) => Refused::InUse(format!("{said} And only an empty folder can be deleted: {}", not_empty.say(kind, false))),
                None => not_empty,
            });
        }
        r => r?,
    };
    if kind == Kind::Job {
        in_use_refusal(lib, &before, &t, &entries)?;
    }
    write(lib, kind, &t, &lock).map_err(|e| Refused::Failed(format!("Couldn't change the Library's folders: {e}")))?;
    Ok(out)
}

/// "'Acme' holds DEMO-7, which is in use by Sam on PROG-2 since 09:14.":
/// the first job in folder `id` of `t` (at any depth) that this PC may not
/// change because someone has it in use (CU11, CU12).
fn in_use_within(lib: &Library, held: &std::collections::BTreeMap<u64, crate::engine::inuse::InUse>, t: &Tree, id: u64, entries: &[Entry]) -> Option<String> {
    entries.iter().filter(|e| t.is_within(t.folder_of(e.id), id)).find_map(|e| held.get(&e.id).map(|u| format!("'{}' holds {}, which is in use by {}.", t.path(id), e.name, u.by_since(&lib.clock))))
}

/// A change to the jobs' tree (`before` → `after`) may not move a job
/// someone else has in use, nor rename or move a folder holding one at any
/// depth (CU11); for an agent, a job anyone holds (CU12). Read under the
/// change's write lock, so no hold is taken between the check and the
/// change. New folders and new jobs filed are no one's yet.
fn in_use_refusal(lib: &Library, before: &Tree, after: &Tree, entries: &[Entry]) -> Result<(), Refused> {
    let held = crate::engine::inuse::look(lib).unwrap_or_default();
    if held.is_empty() {
        return Ok(());
    }
    for e in entries.iter().filter(|e| before.folder_of(e.id) != after.folder_of(e.id)) {
        if let Some(u) = held.get(&e.id) {
            return Err(Refused::InUse(format!("{} It can't be moved while it's in use.", u.said(&lib.clock))));
        }
    }
    for f in before.folders.iter().filter(|f| after.get(f.id).is_none_or(|g| g.name != f.name || g.parent != f.parent)) {
        if let Some(said) = in_use_within(lib, &held, before, f.id, entries) {
            return Err(Refused::InUse(said));
        }
    }
    Ok(())
}

/// The part folder a library part saved into the Library for the first
/// time goes into (JX19).
#[derive(Clone, Debug, PartialEq)]
pub enum PartsFolder {
    /// Named like the folder of the job it is saved from: the job's own
    /// place in the jobs' tree when the job is in the Library (`record`),
    /// else the jobs' folder `folder` while that is still there (the library
    /// job it was opened or copied from, JX15; the top: no folder), else its
    /// Open jobs folder `week`, a top-level name ("" for none).
    LikeJob { record: Option<u64>, folder: Option<u64>, week: String },
    /// A path in the parts tree ("Acme/2026-11", names separated by / or \).
    Path(String),
}

impl PartsFolder {
    /// The names, from the top, of the part folder it means, with the
    /// Library's job folders `jobs` and saved jobs `saved` as they are now.
    pub fn names(&self, jobs: &Tree, saved: &[Entry]) -> Vec<String> {
        let names = |p: &str| p.split(['/', '\\']).map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect::<Vec<_>>();
        match self {
            PartsFolder::Path(p) => names(p),
            PartsFolder::LikeJob { record, folder, week } => {
                let at = record.filter(|r| saved.iter().any(|e| e.kind == Kind::Job && e.id == *r)).map(|r| jobs.folder_of(r)).or_else(|| folder.filter(|&f| f == TOP || jobs.get(f).is_some()));
                match at {
                    Some(f) => jobs.names(f),
                    None => Some(week.trim()).filter(|w| !w.is_empty()).map(String::from).into_iter().collect(),
                }
            }
        }
    }
}

/// File the library part `record`, saved into the Library for the first
/// time, into the part folder `to` names, made if missing (JX19), under the
/// write lock with the job folders and saved jobs as they are now. Returns
/// the folder.
pub fn file_part(lib: &Library, record: u64, to: &PartsFolder) -> Result<u64, Refused> {
    change(lib, Kind::Product, |t, _| {
        let jobs = load(lib, Kind::Job).unwrap_or_default();
        let f = t.folder_made(&to.names(&jobs, &of_kind(lib, Kind::Job)))?;
        t.put(record, f)?;
        Ok(f)
    })
}

#[cfg(test)]
mod folders_tests {
    use super::*;

    fn entry(id: u64, name: &str, folder: &str) -> Entry {
        Entry { kind: Kind::Job, id, name: name.into(), folder: folder.into(), material: 0, thickness: 3.0, w: 2000.0, h: 1000.0, count_a: 1, count_b: 1, updated: 0, tech: false, saved_by: None }
    }

    #[test]
    fn paths_find_folders_without_case_and_by_number() {
        let mut t = Tree::default();
        let acme = t.make(TOP, "Acme").unwrap();
        let nov = t.make(acme, "2026-11").unwrap();
        assert_eq!(t.find("acme/2026-11"), Some(nov));
        assert_eq!(t.find("Acme\\2026-11"), Some(nov));
        assert_eq!(t.find(&format!("#{acme}")), Some(acme));
        assert_eq!(t.find("/"), Some(TOP));
        assert_eq!(t.find("Beta"), None);
        assert_eq!(t.path(nov), "Acme/2026-11");
        // A deleted folder's number is never given again.
        t.delete(nov, &[]).unwrap();
        assert!(t.make(acme, "2026-12").unwrap() > nov);
        assert_eq!(t.outline(&[entry(1, "DEMO-001", "")]), ["Acme/", "Acme/2026-12/", "DEMO-001"]);
        // A folder counts the jobs in its own folders too.
        let dec = t.find("Acme/2026-12").unwrap();
        let jobs = [entry(1, "DEMO-001", ""), entry(2, "DEMO-002", ""), entry(3, "DEMO-003", "")];
        t.put(1, dec).unwrap();
        t.put(2, acme).unwrap();
        assert_eq!((t.records_within(acme, &jobs), t.records_within(dec, &jobs), t.records_in(acme, &jobs).len()), (2, 1, 1));
    }

    /// LB11, LB12: the folder tree of a Library that can't be reached, or
    /// of a folder that holds no Library, is never read as empty and never
    /// made: it is refused in the words a save uses, naming the folder, and
    /// nothing is written there.
    #[test]
    fn the_folders_of_a_library_that_cant_be_reached_are_refused_never_made() {
        let d = std::env::temp_dir().join(format!("nestpro_folders_unreachable_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (who, clock) = (crate::shared::Who::new("sam", "PROG-1"), crate::shared::Clock::pc());
        let gone = d.join("Nesting");
        let lib = Library::at(&gone, &who, &clock);
        assert_eq!(load(&lib, Kind::Job).err(), Some(crate::library::unreachable(&gone)));
        assert_eq!(change(&lib, Kind::Job, |t, _| t.make(TOP, "Acme")).err().map(|e| e.say(Kind::Job, false)), Some(crate::library::unreachable(&gone)));
        assert!(!gone.exists(), "a folder change made the Library folder");
        // A folder that's there but holds no Library (LB12).
        let empty = d.join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        let lib = Library::at(&empty, &who, &clock);
        assert_eq!(load(&lib, Kind::Job).err(), Some(crate::library::no_library(&empty)));
        assert_eq!(change(&lib, Kind::Job, |t, _| t.make(TOP, "Acme")).err().map(|e| e.say(Kind::Job, false)), Some(crate::library::no_library(&empty)));
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0, "a folder change wrote into a folder that holds no Library");
        let _ = std::fs::remove_dir_all(&d);
    }
}
