//! Job Explorer right-click menus, laid out like the original's (JX1, JX2): on a folder "New ▸ Folder / Job, Expand, Collapse, Change
//! name, Delete"; on a job "Open, Add nesting, Copy, Change name, Delete,
//! Properties"; "New sheet…" adds a nesting as in the Nesting Explorer.
//! Folder names follow the original's new-folder and rename rules (JX4–JX6): a
//! name already used by a folder under the same parent — compared without
//! case — is refused with error 9.

use crate::App;

/// Error the original returns for a folder name that is taken (JX4).
pub const FOLDER_NAME_TAKEN: i32 = 9;
/// Default name of a new folder (JX7).
pub const NEW_FOLDER_NAME: &str = "New folder";

/// Longest line the original reads back from the active-job record
/// (rule AJ2: 0x28 characters for the name and type).
pub const ACTIVE_JOB_LINE_MAX: usize = 40;

/// The library record named by the saved active job, if any (an empty or
/// unreadable record means no active job, as when ORDENACT.SYS is missing).
pub fn active_job_id(s: &str) -> Option<u64> {
    let t: String = s.trim().chars().take(ACTIVE_JOB_LINE_MAX).collect();
    t.parse::<u64>().ok()
}

/// A name being typed in the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeEdit {
    Folder { old: String, text: String },
    Job { index: usize, text: String },
}

/// Is `name` used by another folder (case-insensitive, JX6)?
pub fn folder_name_taken(folders: &[String], name: &str, except: Option<&str>) -> bool {
    let n = name.trim().to_lowercase();
    folders.iter().any(|f| Some(f.as_str()) != except && f.to_lowercase() == n)
}

/// Check a folder name: empty, or taken → the original's error 9.
pub fn check_folder_name(folders: &[String], name: &str, except: Option<&str>) -> Result<String, i32> {
    let n = name.trim();
    if n.is_empty() || folder_name_taken(folders, n, except) {
        return Err(FOLDER_NAME_TAKEN);
    }
    Ok(n.to_string())
}

/// "New folder", or "New folder (1)" … "(9)" when taken (the job-name rule,
/// MECHANICS.md #9, applied to folders).
pub fn new_folder_name(folders: &[String]) -> Option<String> {
    if !folder_name_taken(folders, NEW_FOLDER_NAME, None) {
        return Some(NEW_FOLDER_NAME.to_string());
    }
    (1..10).map(|k| format!("{NEW_FOLDER_NAME} ({k})")).find(|n| !folder_name_taken(folders, n, None))
}

impl App {
    /// The open job's nestings changed: a sheet nested, added or removed
    /// (rule ND1: the job's CAM last date is updated).
    pub fn mark_cam_changed(&mut self) {
        self.cam_changed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
    }

    /// The job list changed: a name being typed for a job, or a delete
    /// waiting for confirmation, would now point at another job — drop them.
    pub fn jobs_reindexed(&mut self) {
        if matches!(self.tree_edit, Some(TreeEdit::Job { .. })) {
            self.tree_edit = None;
        }
        self.confirm_delete_job = None;
    }

    /// Folders of the open jobs, in order: those holding jobs, then empty ones.
    pub fn job_folder_list(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for j in &self.jobs {
            if !out.contains(&j.week) {
                out.push(j.week.clone());
            }
        }
        for f in &self.job_folders {
            if !out.contains(f) {
                out.push(f.clone());
            }
        }
        out
    }

    /// New ▸ Folder: an empty folder, its name ready to type.
    pub fn new_folder(&mut self) {
        let folders = self.job_folder_list();
        let Some(name) = new_folder_name(&folders) else {
            self.status = format!("Couldn't create the folder: '{NEW_FOLDER_NAME} (1)' to '(9)' are all taken.");
            return;
        };
        self.job_folders.push(name.clone());
        self.tree_edit = Some(TreeEdit::Folder { old: name.clone(), text: name });
        self.status = "New folder — type its name and press Enter.".into();
    }

    pub fn rename_folder(&mut self, old: &str, new: &str) -> bool {
        let folders = self.job_folder_list();
        match check_folder_name(&folders, new, Some(old)) {
            Ok(n) => {
                for j in self.jobs.iter_mut().filter(|j| j.week == old) {
                    j.week = n.clone();
                }
                for f in self.job_folders.iter_mut().filter(|f| f.as_str() == old) {
                    *f = n.clone();
                }
                self.status = format!("Folder renamed to {n}.");
                true
            }
            Err(code) => {
                self.status = format!("A folder called '{}' already exists here (or the name is empty) — error {code}.", new.trim());
                false
            }
        }
    }

    /// Delete a folder: only when it holds no jobs (the jobs would be lost).
    pub fn delete_folder(&mut self, name: &str) {
        let n = self.jobs.iter().filter(|j| j.week == name).count();
        if n > 0 {
            self.status = format!("'{name}' still holds {n} job(s) — move or delete them first.");
            return;
        }
        self.job_folders.retain(|f| f != name);
        self.status = format!("Folder {name} deleted.");
    }

    pub fn rename_job(&mut self, i: usize, new: &str) -> bool {
        let n = new.trim();
        let current = if i == self.cur_job { self.job_name.clone() } else { self.jobs.get(i).map(|j| j.name.clone()).unwrap_or_default() };
        if n == current {
            return true;
        }
        let taken = self.jobs.iter().enumerate().any(|(k, j)| k != i && (if k == self.cur_job { &self.job_name } else { &j.name }) == n);
        if n.is_empty() || taken || n.chars().count() > 250 {
            self.status = format!("Can't rename to '{n}': empty, too long or already a job.");
            return false;
        }
        if i == self.cur_job {
            self.job_name = n.to_string();
        }
        if let Some(j) = self.jobs.get_mut(i) {
            j.name = n.to_string();
        }
        self.status = format!("Job renamed to {n}.");
        true
    }

    /// Delete a job (at least one job stays open).
    pub fn delete_job(&mut self, i: usize) {
        if i >= self.jobs.len() {
            return;
        }
        if self.jobs.len() == 1 {
            self.status = "This is the only open job — create another one first.".into();
            return;
        }
        self.store_job();
        let name = self.jobs[i].name.clone();
        let folder = self.jobs[i].week.clone();
        self.jobs.remove(i);
        // Keep the (now maybe empty) folder in the tree.
        if !self.job_folders.contains(&folder) {
            self.job_folders.push(folder);
        }
        let next = if self.cur_job == i { i.min(self.jobs.len() - 1) } else if self.cur_job > i { self.cur_job - 1 } else { self.cur_job };
        // Load the job that is now current (open_job skips the current index).
        self.cur_job = usize::MAX;
        self.open_job_forced(next);
        self.status = format!("Job {name} deleted.");
    }

    fn open_job_forced(&mut self, i: usize) {
        let j = self.jobs[i].clone();
        self.cur_job = i;
        self.job_name = j.name;
        self.parts = j.parts;
        self.spec = j.spec;
        self.result = j.result;
        self.stale = j.stale;
        self.job_lib_id = j.lib_id;
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.cam_changed = j.cam_changed;
        self.held = j.held;
        self.sel_cut = None;
        self.jobs_reindexed();
        self.cur_sheet = 0;
        self.sel_placed = None;
        self.sel_part = if self.parts.is_empty() { None } else { Some(0) };
        self.undo.clear();
        self.redo.clear();
        self.refresh();
    }

    /// Close a job: it leaves the open jobs; its saved copy stays in the library
    /// (the original's "Close the current job" deletes the active-job record,
    /// rule AJ3). A job never saved isn't closed
    /// silently — save it first, or delete it.
    pub fn close_job(&mut self, i: usize) {
        self.store_job();
        let Some(j) = self.jobs.get(i) else { return };
        if j.lib_id.is_none() {
            self.status = format!("{} isn't saved yet — Save to library first (or Delete it).", j.name);
            return;
        }
        if self.jobs.len() == 1 {
            self.status = "This is the only open job — open or create another one first.".into();
            return;
        }
        let name = j.name.clone();
        self.jobs.remove(i);
        let next = if self.cur_job == i { i.min(self.jobs.len() - 1) } else if self.cur_job > i { self.cur_job - 1 } else { self.cur_job };
        self.cur_job = usize::MAX;
        self.open_job_forced(next);
        self.status = format!("Closed {name} — it stays in Saved jobs (Library).");
    }

    /// Add nesting: the job opens in the Nesting module on a new, empty sheet
    /// (the original's "New sheet…" in the Nesting Explorer) — drag products
    /// onto it or nest automatically.
    pub fn add_nesting_to_job(&mut self, i: usize) {
        self.open_job(i);
        if i != self.cur_job {
            return;
        }
        self.checkpoint();
        self.result.push_sheet(crate::nest::SheetFormat::rect(self.spec.w, self.spec.h));
        self.mark_cam_changed();
        self.cur_sheet = self.result.sheets.len() - 1;
        self.empty_sheet_kept = Some(self.cur_sheet);
        self.workspace = crate::Workspace::Nesting;
        self.refresh();
        self.store_job();
        self.status = format!("Nesting {} added to {} — drag products from Parts View onto it, or Nest all.", self.cur_sheet + 1, self.job_name);
    }

    /// Tooltip for a job row.
    pub fn job_tip(&self, i: usize) -> String {
        let Some(j) = self.jobs.get(i) else { return String::new() };
        let (name, parts, spec, sheets) = if i == self.cur_job { (&self.job_name, &self.parts, &self.spec, self.result.sheets.len()) } else { (&j.name, &j.parts, &j.spec, j.result.sheets.len()) };
        let qty: u32 = parts.iter().map(|p| p.qty).sum();
        let cam = if i == self.cur_job { self.cam_changed } else { j.cam_changed };
        format!(
            "{name}\nFolder: {}\n{} product(s), {qty} piece(s)\n{sheets} nesting(s) · {} {} mm · {:.0} × {:.0}\nLast nesting change: {}\nRight-click: open, add nesting, rename, delete",
            j.week,
            parts.len(),
            crate::cnc::MATERIALS.get(spec.material).copied().unwrap_or(""),
            spec.thickness,
            spec.w,
            spec.h,
            cam_date_label(cam)
        )
    }
}

/// "today 14:05", "yesterday 09:30" or "2026-11-01" for a CAM-change time
/// (UTC; "—" when never changed in this app).
pub fn cam_date_label(t: Option<u64>) -> String {
    let Some(t) = t else { return "—".into() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(t, |d| d.as_secs());
    let (day, today) = (t / 86400, now / 86400);
    let hm = format!("{:02}:{:02}", (t % 86400) / 3600, (t % 3600) / 60);
    if day == today {
        return format!("today {hm} UTC");
    }
    if day + 1 == today {
        return format!("yesterday {hm} UTC");
    }
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = day as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y}-{m:02}-{d:02}")
}

#[cfg(test)]
mod jobtree_tests {
    use super::*;

    #[test]
    fn folder_names_are_unique_without_case() {
        let f = vec!["W09_10".to_string(), "New folder".to_string()];
        assert!(folder_name_taken(&f, "w09_10", None));
        assert!(!folder_name_taken(&f, "W09_10", Some("W09_10")));
        assert_eq!(check_folder_name(&f, "NEW FOLDER", None), Err(FOLDER_NAME_TAKEN));
        assert_eq!(check_folder_name(&f, "  ", None), Err(9));
        assert_eq!(check_folder_name(&f, " W13 ", None), Ok("W13".to_string()));
        assert_eq!(new_folder_name(&f).as_deref(), Some("New folder (1)"));
        assert_eq!(new_folder_name(&f[..1]).as_deref(), Some("New folder"));
    }

    #[test]
    fn open_jobs_remember_their_saved_copy() {
        let dir = std::env::temp_dir().join(format!("nestpro_lib_link_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        a.open_job(0);
        a.save_job_to_library();
        let id = a.job_lib_id.expect("saved");
        // Switch away and back: the link stays with the job.
        a.open_job(1);
        assert_eq!(a.job_lib_id, None);
        a.open_job(0);
        assert_eq!(a.job_lib_id, Some(id));
        // Opening the saved job again switches to the open one instead of adding a copy.
        a.open_job(1);
        let n = a.jobs.len();
        a.open_library_job(id);
        assert_eq!((a.jobs.len(), a.cur_job, a.job_lib_id), (n, 0, Some(id)));
        // Close: it leaves the open jobs but stays in the library; a job never
        // saved isn't closed.
        let n = a.jobs.len();
        a.close_job(1);
        assert_eq!(a.jobs.len(), n, "unsaved job stays open");
        a.close_job(0);
        assert_eq!(a.jobs.len(), n - 1);
        assert!(a.jobs.iter().all(|j| j.lib_id != Some(id)));
        a.open_library_job(id);
        assert_eq!(a.job_lib_id, Some(id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_jobs_keeps_each_jobs_cut_lines_and_copies_are_new_records() {
        let mut a = App::new(None);
        a.open_job(0);
        a.cut_lines = vec![vec![crate::cutoff::CutLine { pts: vec![[500.0, 0.0], [500.0, 1000.0]], kind: crate::cutoff::CutKind::Remnant }]];
        a.open_job(1);
        assert!(a.cut_lines.iter().all(|s| s.is_empty()), "job 1 doesn't show job 0's remnant line");
        a.open_job(0);
        assert_eq!(a.cut_lines.first().map_or(0, |s| s.len()), 1, "job 0 gets its line back");
        // A copy is not linked to the original's library record.
        a.job_lib_id = Some(42);
        a.copy_job();
        assert_eq!(a.job_lib_id, None);
        assert_eq!(a.jobs[0].lib_id, Some(42));
        // Typing a job name, then the job list changes: the edit is dropped.
        a.tree_edit = Some(TreeEdit::Job { index: 2, text: "x".into() });
        a.confirm_delete_job = Some(2);
        a.delete_job(0);
        assert_eq!((a.tree_edit.clone(), a.confirm_delete_job), (None, None));
    }

    #[test]
    fn an_emptied_sheet_takes_its_cut_lines_with_it() {
        let mut a = App::new(None);
        let p = a.result.sheets[0][0];
        a.result.sheets = vec![vec![p], vec![], vec![p]];
        let line = |x: f64| vec![crate::cutoff::CutLine { pts: vec![[x, 0.0], [x, 1000.0]], kind: crate::cutoff::CutKind::Remnant }];
        a.cut_lines = vec![line(100.0), line(200.0), line(300.0)];
        a.cur_sheet = 0;
        a.refresh();
        assert_eq!(a.result.sheets.len(), 2);
        assert_eq!(a.cut_lines.len(), 2);
        assert_eq!(a.cut_lines[1][0].pts[0][0], 300.0, "sheet 3's line stays with sheet 3 (now 2)");
        // Removing a nesting updates the job's last nesting change (ND1).
        assert!(a.cam_changed.is_some());
        a.cam_changed = None;
        a.add_nesting_to_job(a.cur_job);
        assert!(a.cam_changed.is_some(), "adding a nesting does too");
    }

    #[test]
    fn nesting_change_dates_read_plainly() {
        assert_eq!(cam_date_label(None), "—");
        // 2026-11-01 08:00 UTC = 1793520000 s.
        assert_eq!(cam_date_label(Some(1_793_520_000)), "2026-11-01");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        assert!(cam_date_label(Some(now)).starts_with("today "));
        assert!(cam_date_label(Some(now - 86400)).starts_with("yesterday "));
    }

    #[test]
    fn the_active_job_record_names_a_library_job() {
        // AJ2: no record (or an unreadable one) = no active job.
        assert_eq!(active_job_id("12"), Some(12));
        assert_eq!(active_job_id(" 7 "), Some(7));
        assert_eq!(active_job_id(""), None);
        assert_eq!(active_job_id("abc"), None);
        // Lines are read up to 40 characters.
        assert_eq!(active_job_id(&"1".repeat(ACTIVE_JOB_LINE_MAX + 5)), None);
    }

    #[test]
    fn the_tree_menus_create_rename_and_delete() {
        let mut a = App::new(None);
        let n0 = a.jobs.len();
        a.new_folder();
        assert!(a.job_folder_list().contains(&"New folder".to_string()));
        assert!(a.rename_folder("New folder", "Customer A"));
        let first = a.jobs[0].week.clone();
        assert!(!a.rename_folder("Customer A", &first.to_lowercase()), "taken without case");
        a.new_job_in_week("Customer A".into());
        assert_eq!(a.jobs.len(), n0 + 1);
        let i = a.cur_job;
        assert_eq!(a.jobs[i].week, "Customer A");
        a.delete_folder("Customer A");
        assert!(a.job_folder_list().contains(&"Customer A".to_string()), "a folder with jobs stays");
        assert!(a.rename_job(i, "Order 42"));
        assert_eq!(a.job_name, "Order 42");
        let before = a.result.sheets.len();
        a.add_nesting_to_job(i);
        assert_eq!(a.result.sheets.len(), before + 1);
        assert_eq!(a.cur_sheet, before);
        a.delete_job(i);
        assert_eq!(a.jobs.len(), n0);
        a.delete_folder("Customer A");
        assert!(!a.job_folder_list().contains(&"Customer A".to_string()));
    }
}
