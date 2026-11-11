//! The agents' `job-folders` and `part-folders` operations: the Library's
//! job folders (folders.rs, JX10–JX15), level with the Job Explorer's
//! Library section, and its part folders (JX18–JX20), level with the
//! Library window's Products tab — list the tree, make, rename and delete
//! folders, move saved jobs or library parts and folders. Every change goes
//! through the same rules and write lock as the window's.

use super::ops::{Args, Outcome};
use super::Ctx;
use crate::folders::{self, Refused, Tree, TOP};
use crate::library::{auto_ref, Entry, Kind, Library};
use serde_json::{json, Value};

fn done(message: String, data: Value) -> Outcome {
    Outcome { message, data, changed: false, files: vec![] }
}

/// "jobs" or "parts": what a tree holds, as list names it.
fn plural(kind: Kind) -> &'static str {
    match kind {
        Kind::Job => "jobs",
        Kind::Product => "parts",
    }
}

/// The argument naming the record to move: `saved_job` or `library_part`.
fn record_arg(kind: Kind) -> &'static str {
    match kind {
        Kind::Job => "saved_job",
        Kind::Product => "library_part",
    }
}

/// "saved job" or "library part".
fn record_words(kind: Kind) -> &'static str {
    match kind {
        Kind::Job => "saved job",
        Kind::Product => "library part",
    }
}

/// What list shows: every folder (path, number, what it holds), every saved
/// job or library part with its folder, and the tree opened out.
fn listing(kind: Kind, t: &Tree, records: &[Entry]) -> Value {
    let mut fs: Vec<Value> = t
        .folders
        .iter()
        .map(|f| {
            let mut v = json!({"folder": format!("#{}", f.id), "path": t.path(f.id), "name": f.name, "folders": t.children(f.id).len()});
            v[plural(kind)] = json!(t.records_in(f.id, records).len());
            v
        })
        .collect();
    fs.sort_by_key(|v| v["path"].as_str().unwrap_or("").to_lowercase());
    let rs: Vec<Value> = records.iter().map(|e| json!({"ref": auto_ref(kind, e.id), "record": e.id, "name": e.name, "folder": t.path(t.folder_of(e.id))})).collect();
    let mut out = json!({"folders": fs, "outline": t.outline(records)});
    out[plural(kind)] = json!(rs);
    out
}

/// A folder named by the agent: its path, or #number; "" or "/" is the top.
fn folder_at(kind: Kind, t: &Tree, path: &str) -> Result<u64, String> {
    t.find(path).ok_or_else(|| format!("There is no folder '{}' in the Library's {} folders. `{} mode=list` shows the folders (paths like \"Acme/2026-11\", or #<number>).", path.trim(), if kind == Kind::Job { "job" } else { "part" }, folders::op_name(kind)))
}

/// A saved job or library part named by the agent: its reference
/// (JOB000012, SHP000012), record number, or a name only one of them has
/// (without case).
fn saved_record(kind: Kind, records: &[Entry], t: &Tree, s: &str) -> Result<u64, String> {
    let s = s.trim();
    let prefix = auto_ref(kind, 0).trim_end_matches('0').to_string();
    let number = s.strip_prefix(prefix.as_str()).or_else(|| s.strip_prefix(prefix.to_lowercase().as_str())).unwrap_or(s);
    if let Ok(n) = number.parse::<u64>() {
        if let Some(e) = records.iter().find(|e| e.id == n) {
            return Ok(e.id);
        }
    }
    let named: Vec<&Entry> = records.iter().filter(|e| e.name.trim().to_lowercase() == s.to_lowercase()).collect();
    match named.as_slice() {
        [e] => Ok(e.id),
        [] => Err(format!("No {} in the Library is called or numbered '{s}'. `{} mode=list` lists them with their references ({}).", record_words(kind), folders::op_name(kind), auto_ref(kind, 12))),
        many => Err(format!(
            "{} {}s are called '{s}': {}. Give its reference instead.",
            many.len(),
            record_words(kind),
            many.iter().map(|e| format!("{} in {}", auto_ref(kind, e.id), t.place(t.folder_of(e.id)))).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// `job-folders` (kind Job) and `part-folders` (kind Product): list, new,
/// rename, delete, move.
pub fn run(ctx: &Ctx, a: &Args, kind: Kind) -> Result<Outcome, String> {
    let (op, noun) = (folders::op_name(kind), if kind == Kind::Job { "job" } else { "part" });
    // A Library that can't be reached is refused as agents' saves are (LB11).
    super::ops::reach_library(ctx, &format!("Nothing was read from the Library, and no {noun} folder was made, changed or moved"))?;
    let lib = Library::open(&ctx.library_dir, &ctx.who, &ctx.clock)?;
    let mode = a.str("mode").unwrap_or_default();
    let records = |l: &Library| -> Vec<Entry> { l.entries().into_iter().filter(|e| e.kind == kind).collect() };
    let now = folders::load(&lib, kind)?;
    let change = |f: &dyn Fn(&mut Tree, &[Entry]) -> Result<String, Refused>| -> Result<Outcome, String> {
        let message = folders::change(&lib, kind, |t, es| f(t, es)).map_err(|e| e.say(kind, true))?;
        // The tree as it is now (`list` gives the details).
        let t = folders::load(&lib, kind)?;
        Ok(Outcome { message, data: json!({"outline": t.outline(&records(&lib))}), changed: false, files: vec![] })
    };
    match mode.as_str() {
        "list" => {
            let rs = records(&lib);
            Ok(done(format!("{} folder(s) and {} {}(s) in the Library.", now.folders.len(), rs.len(), record_words(kind)), listing(kind, &now, &rs)))
        }
        "new" => {
            let parent = folder_at(kind, &now, &a.str("folder").unwrap_or_default())?;
            let name = a.str("name");
            change(&|t, _| {
                let n = match &name {
                    Some(n) => n.clone(),
                    None => t.new_name(parent)?,
                };
                let id = t.make(parent, &n)?;
                Ok(format!("Made folder '{}' (#{id}) in {}.", t.path(id), t.place(parent)))
            })
        }
        "rename" => {
            let f = a.str("folder").ok_or("rename needs the folder (its path or #number) and its new name")?;
            let id = folder_at(kind, &now, &f)?;
            if id == TOP {
                return Err("The top of the Library has no name to change — give a folder.".into());
            }
            let name = a.str("name").ok_or("rename needs the new name (name)")?;
            change(&|t, _| {
                t.rename(id, &name)?;
                Ok(format!("Renamed the folder to '{}'.", t.path(id)))
            })
        }
        "delete" => {
            let f = a.str("folder").ok_or("delete needs the folder (its path or #number)")?;
            let id = folder_at(kind, &now, &f)?;
            if id == TOP {
                return Err("The top of the Library can't be deleted — give a folder.".into());
            }
            change(&|t, es| {
                let p = t.path(id);
                t.delete(id, es)?;
                Ok(format!("Deleted the empty folder '{p}'."))
            })
        }
        "move" => {
            let to = folder_at(kind, &now, &a.str("to").ok_or("move needs where to (to): a folder's path or #number, or \"/\" for the top of the Library")?)?;
            let arg = record_arg(kind);
            match (a.str(arg), a.str("folder")) {
                (Some(_), Some(_)) => Err(format!("move takes one thing at a time: a {} ({arg}) or a folder (folder), not both.", record_words(kind))),
                (Some(s), None) => {
                    let r = saved_record(kind, &records(&lib), &now, &s)?;
                    let what = format!("{} {}", if kind == Kind::Job { "Job" } else { "Part" }, auto_ref(kind, r));
                    change(&|t, es| {
                        if !es.iter().any(|e| e.id == r) {
                            return Err(Refused::Gone(what.clone()));
                        }
                        if t.folder_of(r) == to {
                            return Ok(format!("{what} is already in {}: nothing moved.", t.place(to)));
                        }
                        t.put(r, to)?;
                        Ok(format!("Moved {what} to {}.", t.place(to)))
                    })
                }
                (None, Some(f)) => {
                    let id = folder_at(kind, &now, &f)?;
                    if id == TOP {
                        return Err("The top of the Library can't be moved — give a folder.".into());
                    }
                    change(&|t, _| {
                        t.move_folder(id, to)?;
                        Ok(format!("Moved the folder to '{}'.", t.path(id)))
                    })
                }
                (None, None) => Err(format!("move needs what to move: a {} ({arg}: its reference, e.g. {}) or a folder (folder: its path or #number).", record_words(kind), auto_ref(kind, 12))),
            }
        }
        m => Err(format!("{op} mode '{m}'? list, new, rename, delete or move")),
    }
}
