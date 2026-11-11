//! The agents' `job-folders` operation: the Library's job folders
//! (folders.rs, JX10–JX15), level with the Job Explorer's Library section —
//! list the tree, make, rename and delete folders, move saved jobs and
//! folders. Every change goes through the same rules and write lock as the
//! window's.

use super::ops::{Args, Outcome};
use super::Ctx;
use crate::folders::{self, Refused, Tree, TOP};
use crate::library::{auto_ref, Entry, Kind, Library};
use serde_json::{json, Value};

fn done(message: String, data: Value) -> Outcome {
    Outcome { message, data, changed: false, files: vec![] }
}

/// What list shows: every folder (path, number, what it holds), every saved
/// job with its folder, and the tree opened out.
fn listing(t: &Tree, jobs: &[Entry]) -> Value {
    let mut fs: Vec<Value> = t
        .folders
        .iter()
        .map(|f| json!({"folder": format!("#{}", f.id), "path": t.path(f.id), "name": f.name, "folders": t.children(f.id).len(), "jobs": t.records_in(f.id, jobs).len()}))
        .collect();
    fs.sort_by_key(|v| v["path"].as_str().unwrap_or("").to_lowercase());
    let js: Vec<Value> = jobs.iter().map(|e| json!({"ref": auto_ref(Kind::Job, e.id), "record": e.id, "name": e.name, "folder": t.path(t.folder_of(e.id))})).collect();
    json!({"folders": fs, "jobs": js, "outline": t.outline(jobs)})
}

/// A folder named by the agent: its path, or #number; "" or "/" is the top.
fn folder_at(t: &Tree, path: &str) -> Result<u64, String> {
    t.find(path).ok_or_else(|| format!("There is no folder '{}' in the Library. `job-folders mode=list` shows the folders (paths like \"Acme/2026-11\", or #<number>).", path.trim()))
}

/// A saved job named by the agent: its reference (JOB000012), record number,
/// or a name only one saved job has (without case).
fn saved_job(jobs: &[Entry], t: &Tree, s: &str) -> Result<u64, String> {
    let s = s.trim();
    let number = s.strip_prefix("JOB").or_else(|| s.strip_prefix("job")).unwrap_or(s);
    if let Ok(n) = number.parse::<u64>() {
        if let Some(e) = jobs.iter().find(|e| e.id == n) {
            return Ok(e.id);
        }
    }
    let named: Vec<&Entry> = jobs.iter().filter(|e| e.name.trim().to_lowercase() == s.to_lowercase()).collect();
    match named.as_slice() {
        [e] => Ok(e.id),
        [] => Err(format!("No saved job in the Library is called or numbered '{s}'. `job-folders mode=list` lists them with their references (JOB000012).")),
        many => Err(format!(
            "{} saved jobs are called '{s}': {}. Give its reference instead.",
            many.len(),
            many.iter().map(|e| format!("{} in {}", auto_ref(Kind::Job, e.id), t.place(t.folder_of(e.id)))).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// `job-folders`: list, new, rename, delete, move.
pub fn run(ctx: &Ctx, a: &Args) -> Result<Outcome, String> {
    let lib = Library::open(&ctx.library_dir, &ctx.who, &ctx.clock)?;
    let mode = a.str("mode").unwrap_or_default();
    let jobs = |l: &Library| -> Vec<Entry> { l.entries().into_iter().filter(|e| e.kind == Kind::Job).collect() };
    let now = folders::load(&lib, Kind::Job)?;
    let change = |f: &dyn Fn(&mut Tree, &[Entry]) -> Result<String, Refused>| -> Result<Outcome, String> {
        let message = folders::change(&lib, Kind::Job, |t, es| f(t, es)).map_err(|e| e.say(true))?;
        // The tree as it is now (`list` gives the details).
        let t = folders::load(&lib, Kind::Job)?;
        Ok(Outcome { message, data: json!({"outline": t.outline(&jobs(&lib))}), changed: false, files: vec![] })
    };
    match mode.as_str() {
        "list" => {
            let js = jobs(&lib);
            Ok(done(format!("{} folder(s) and {} saved job(s) in the Library.", now.folders.len(), js.len()), listing(&now, &js)))
        }
        "new" => {
            let parent = folder_at(&now, &a.str("folder").unwrap_or_default())?;
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
            let id = folder_at(&now, &f)?;
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
            let id = folder_at(&now, &f)?;
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
            let to = folder_at(&now, &a.str("to").ok_or("move needs where to (to): a folder's path or #number, or \"/\" for the top of the Library")?)?;
            match (a.str("saved_job"), a.str("folder")) {
                (Some(_), Some(_)) => Err("move takes one thing at a time: a saved job (saved_job) or a folder (folder), not both.".into()),
                (Some(s), None) => {
                    let r = saved_job(&jobs(&lib), &now, &s)?;
                    change(&|t, es| {
                        if !es.iter().any(|e| e.id == r) {
                            return Err(Refused::Gone(format!("Job {}", auto_ref(Kind::Job, r))));
                        }
                        t.put(r, to)?;
                        Ok(format!("Moved {} to {}.", auto_ref(Kind::Job, r), t.place(to)))
                    })
                }
                (None, Some(f)) => {
                    let id = folder_at(&now, &f)?;
                    if id == TOP {
                        return Err("The top of the Library can't be moved — give a folder.".into());
                    }
                    change(&|t, _| {
                        t.move_folder(id, to)?;
                        Ok(format!("Moved the folder to '{}'.", t.path(id)))
                    })
                }
                (None, None) => Err("move needs what to move: a saved job (saved_job: its reference, e.g. JOB000012) or a folder (folder: its path or #number).".into()),
            }
        }
        m => Err(format!("job-folders mode '{m}'? list, new, rename, delete or move")),
    }
}
