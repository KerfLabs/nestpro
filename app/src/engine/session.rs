//! A working session on job files, shared by the command line and the MCP
//! server. The job file is the truth: a job is read from disk when it is named
//! (or changed on disk since), and every change is saved straight back, the
//! previous version kept beside it as `<job>.snjob~` for one-step undo across
//! runs. Inside one session every step can be undone.

use super::ops::{self, Kind, Outcome};
use super::*;
use serde_json::{json, Value};
use std::time::SystemTime;

pub struct Session {
    pub ctx: Ctx,
    pub job: Option<JobFile>,
    pub path: Option<PathBuf>,
    /// When the open job's file was last read or written by us.
    stamp: Option<SystemTime>,
    undo: Vec<JobFile>,
}

/// What a call gave back: the JSON answer and, for render, the PNG.
pub struct Reply {
    pub json: Value,
    pub image: Option<Vec<u8>>,
}

/// Undo copy of a job file.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push("~");
    PathBuf::from(s)
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// A job path as given, with the job extension added when it has none.
pub fn job_path(s: &str) -> PathBuf {
    let p = PathBuf::from(s);
    if p.extension().is_none() { p.with_extension(jobfile::EXT) } else { p }
}

impl Session {
    pub fn new(ctx: Ctx) -> Session {
        Session { ctx, job: None, path: None, stamp: None, undo: vec![] }
    }

    /// Open a job file (again, when it changed on disk).
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let same = self.path.as_deref() == Some(path);
        if same && self.job.is_some() && mtime(path) == self.stamp {
            return Ok(());
        }
        let j = jobfile::load(path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
        if same && self.job.is_some() {
            // Changed on disk by someone else (the window, another agent):
            // undoing to a copy from before that would drop their change.
            let _ = std::fs::remove_file(backup_path(path));
        }
        self.undo.clear();
        self.job = Some(j);
        self.path = Some(path.to_path_buf());
        self.stamp = mtime(path);
        Ok(())
    }

    fn save(&mut self, before: Option<JobFile>) -> Result<(), String> {
        let (Some(job), Some(path)) = (&self.job, &self.path) else { return Err("no job open".into()) };
        if path.exists() && before.is_some() {
            let _ = std::fs::copy(path, backup_path(path));
        }
        jobfile::save(path, job)?;
        self.stamp = mtime(path);
        if let Some(b) = before {
            self.undo.push(b);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
        Ok(())
    }

    /// Run an operation. `job` names the job file (needed for `new`, and when
    /// no job is open yet).
    pub fn call(&mut self, name: &str, job: Option<&str>, args: &Value) -> Result<Reply, String> {
        let op = ops::find(name).ok_or_else(|| format!("unknown operation '{name}' — see `nestpro ops`"))?;
        let args = if args.is_null() { json!({}) } else { args.clone() };
        ops::check_args(op, &args)?;
        let done = |o: Outcome| Reply { json: o.to_json(), image: None };
        match op.kind {
            Kind::Global => return ops::run_global(&self.ctx, name, &args).map(done),
            Kind::Create => {
                let p = job_path(job.ok_or_else(|| format!("{name} needs the job file to make (job)"))?);
                let force = ops::Args(&args).bool("force")?.unwrap_or(false);
                if p.exists() && !force {
                    return Err(format!("{} already exists — pick another name, or force=true to overwrite it.", p.display()));
                }
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "New job".into());
                let mut a = args.clone();
                if let Some(m) = a.as_object_mut() {
                    m.remove("force");
                }
                let (j, message) = ops::create(&self.ctx, op.name, &stem, &a)?;
                if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                self.job = Some(j);
                self.path = Some(p.clone());
                self.undo.clear();
                self.save(None)?;
                return Ok(done(Outcome {
                    message,
                    data: Value::Null,
                    changed: true,
                    files: vec![p],
                }));
            }
            _ => {}
        }
        match job {
            Some(p) => self.open(&job_path(p))?,
            None => match self.path.clone() {
                Some(p) => self.open(&p)?,
                None => return Err("No job open — give the job file (job).".into()),
            },
        }
        match op.name {
            "undo" => return self.undo_step().map(done),
            "render" => return self.render(&args),
            _ => {}
        }
        let job = self.job.as_mut().unwrap();
        let before = job.clone();
        let mut out = match ops::run(job, &self.ctx, name, &args) {
            Ok(o) => o,
            Err(e) => {
                // Refused: nothing it changed before refusing stays, or the
                // next change would save it.
                *job = before;
                return Err(e);
            }
        };
        // A booked job keeps the stock in step with every change (as the
        // window does on every save).
        if out.changed && op.kind == Kind::Write && job.booked && op.name != "stock-book" {
            let note = job.book_stock(&self.ctx, true);
            if !note.is_empty() {
                out.message = format!("{} Stock: {note}.", out.message);
            }
        }
        if out.changed && op.kind == Kind::Write {
            self.save(Some(before))?;
        }
        let mut reply = done(out);
        if let Some(p) = &self.path {
            reply.json["job"] = json!(p.display().to_string());
        }
        Ok(reply)
    }

    fn undo_step(&mut self) -> Result<Outcome, String> {
        let path = self.path.clone().ok_or("no job open")?;
        let prev = match self.undo.pop() {
            Some(j) => j,
            // Across runs: the copy kept beside the file.
            None => jobfile::load(&backup_path(&path)).map_err(|_| "Nothing to undo.".to_string())?,
        };
        let note = self.restock_for_undo(prev)?;
        jobfile::save(&path, self.job.as_ref().unwrap())?;
        // The copy beside the file moves one step back too, so a later run
        // can undo once more.
        match self.undo.last() {
            Some(older) => jobfile::save(&backup_path(&path), older)?,
            None => {
                let _ = std::fs::remove_file(backup_path(&path));
            }
        }
        self.stamp = mtime(&path);
        Ok(Outcome { message: format!("Undone — the job is back to before the last change.{note}"), data: Value::Null, changed: true, files: vec![] })
    }

    /// Undo doesn't roll the shared stock back: the stock follows the job
    /// again instead. What the job holds stays what is really taken, then a
    /// booked job is booked against its restored sheets (undoing a booking
    /// gives everything back).
    fn restock_for_undo(&mut self, mut prev: JobFile) -> Result<String, String> {
        let (was_booked, mut held, mut formats) = match self.job.as_ref() {
            Some(cur) => (cur.booked, cur.held.clone(), cur.formats.clone()),
            None => (false, Default::default(), vec![]),
        };
        let note = if prev.booked {
            // Remnants the undone change replaced: the restored sheets take
            // over their replacements, to be rebuilt (RN6).
            if let Ok(stock) = crate::stock::Stock::load(&self.ctx.library_dir) {
                prev.normalise_formats();
                stockflow::follow_stock(&stock, &mut prev.formats);
            }
            // Remnants the undone change stored go out of stock again.
            let mut gone: Vec<SheetFormat> = formats.into_iter().filter(|f| f.made.iter().any(|r| !prev.formats.iter().any(|p| p.made.contains(r)))).collect();
            for f in gone.iter_mut() {
                f.made.retain(|r| !prev.formats.iter().any(|p| p.made.contains(r)));
            }
            if !gone.is_empty() {
                stockflow::release(&self.ctx.library_dir, &mut gone, &mut Default::default())?;
            }
            prev.held = held;
            prev.book_stock(&self.ctx, true)
        } else if was_booked {
            let (back, removed, _) = stockflow::release(&self.ctx.library_dir, &mut formats, &mut held)?;
            prev.held.clear();
            format!("{back} sheet(s) given back to stock, {removed} remnant(s) taken out")
        } else {
            String::new()
        };
        self.job = Some(prev);
        Ok(if note.is_empty() { String::new() } else { format!(" Stock: {note}.") })
    }

    fn render(&mut self, args: &Value) -> Result<Reply, String> {
        let a = ops::Args(args);
        let job = self.job.as_ref().unwrap();
        let width = a.int("size")?.unwrap_or(1400).clamp(200, 6000) as u32;
        let path = a.bool("path")?.unwrap_or(true);
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "job".into());
        let dir = self.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_default();
        let (scene, what, default_out) = if args.get("part").is_some_and(|v| !v.is_null()) {
            let v = ops::parts_sel(&job.parts, args.get("part"), false)?;
            let i = *v.first().ok_or("name one part")?;
            (render::part_scene(job, i, width, path), format!("part {} '{}'", i + 1, job.parts[i].name), dir.join(format!("{stem}-part{}.png", i + 1)))
        } else {
            let k = a.int("sheet")?.unwrap_or(1);
            if k < 1 {
                return Err("sheets are numbered from 1".into());
            }
            let k = k as usize - 1;
            (render::sheet_scene(job, k, width, path)?, format!("sheet {}", k + 1), dir.join(format!("{stem}-sheet{}.png", k + 1)))
        };
        let out = a.str("out").map(PathBuf::from).unwrap_or(default_out);
        let png = render::write(&scene, &out)?;
        let image = match png {
            Some(p) => Some(p),
            None => Some(scene.png()?),
        };
        let o = Outcome {
            message: format!("Drew {what} to {}. Colours: parts filled with #numbers; red = cut contour, green = lead-in and pierce (with cut order), yellow = micro-joint, blue dashed = rapid move, magenta = cut/remnant line, light green = remnant; on a part, blue numbers are its elements.", out.display()),
            data: Value::Null,
            changed: false,
            files: vec![out],
        };
        Ok(Reply { json: o.to_json(), image })
    }
}
