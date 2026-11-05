//! The open job's file on disk. Agents and scripts change job files through
//! the command line / MCP server while the window may have the same job open;
//! the window looks at the file once a second. Changed outside and nothing
//! changed here: it reloads. Changed in both places: it asks which to keep.

use crate::engine::imports::job_sig;
use crate::jobfile::{self, JobFile};
use crate::theme::th;
use crate::{App, Pending};
use eframe::egui;
use egui::{Align2, Color32, RichText, Vec2};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How often the window looks at the open job's file, seconds.
pub const POLL_S: f64 = 1.0;

/// A job's file and what the window last read or wrote there.
#[derive(Clone, Debug, PartialEq)]
pub struct JobLink {
    pub path: PathBuf,
    /// The file's modification time then.
    pub mtime: Option<SystemTime>,
    /// Signature of the file's content then, and of the window's job then
    /// (they differ when, say, the window gave the job a unique name).
    pub disk_sig: u64,
    pub local_sig: u64,
}

pub fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl App {
    /// The job in the window, as it would be saved.
    pub fn job_now_sig(&mut self) -> u64 {
        job_sig(&self.current_jobfile())
    }

    /// Remember that the open job matches `disk` (just read or written) at `path`.
    pub fn link_job_file(&mut self, path: &Path, disk: &JobFile) {
        self.job_path = Some(path.to_path_buf());
        let local_sig = self.job_now_sig();
        self.job_link = Some(JobLink { path: path.to_path_buf(), mtime: mtime(path), disk_sig: job_sig(disk), local_sig });
    }

    /// Once a second: has the open job's file been changed by someone else?
    pub fn watch_job_file(&mut self, ctx: &egui::Context) {
        let Some(link) = self.job_link.clone() else { return };
        if self.shot.is_some() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(POLL_S));
        let now = ctx.input(|i| i.time);
        if now - self.disk_poll < POLL_S || self.pending.is_some() {
            return;
        }
        self.disk_poll = now;
        let m = mtime(&link.path);
        if m.is_none() || m == link.mtime {
            return;
        }
        let Ok(disk) = jobfile::load(&link.path) else {
            // Not readable as a job (being written, or replaced): look again
            // only when it changes once more.
            self.job_link = Some(JobLink { mtime: m, ..link });
            return;
        };
        let disk_sig = job_sig(&disk);
        if disk_sig == link.disk_sig {
            // Touched, not changed (or our own save).
            self.job_link = Some(JobLink { mtime: m, ..link });
            return;
        }
        if self.job_now_sig() == link.local_sig {
            self.reload_job_file(disk, &link.path);
        } else {
            self.pending = Some(Pending::JobFileChanged { path: link.path.clone() });
        }
    }

    /// Put the job file's content into the open job, in place.
    pub fn reload_job_file(&mut self, j: JobFile, path: &Path) {
        self.checkpoint();
        if let Some(job) = self.jobs.get_mut(self.cur_job) {
            if !j.week.is_empty() {
                job.week = j.week.clone();
            }
        }
        let copy = j.clone();
        self.job_name = j.name;
        self.parts = j.parts;
        self.spec = j.spec;
        self.result = { let mut r = crate::nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried: 0 }; r.normalise(&self.spec); r };
        self.held = j.held;
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.stale = j.stale;
        self.sel_placed = None;
        self.sel_cut = None;
        self.ssim = None;
        self.ghost = None;
        if self.sel_part.is_some_and(|i| i >= self.parts.len()) {
            self.sel_part = None;
        }
        self.refresh();
        self.store_job();
        self.link_job_file(path, &copy);
        self.mark_cam_changed();
        self.mark_job_saved(self.cur_job);
        self.status = format!("Reloaded {}: its file was changed outside the app (an agent or another program).", self.job_name);
    }

    /// The question when the file and the window both changed.
    pub fn job_file_dialog(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        let mut close = false;
        egui::Window::new("Job changed on disk").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(format!(
                "'{}' was changed in its file by another program (an agent or a script) while you have changes here that aren't saved.",
                self.job_name
            ));
            ui.label(RichText::new(path.display().to_string()).size(11.0).color(th().text_dim));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let primary = egui::Button::new(RichText::new("Reload from the file").color(Color32::WHITE).strong()).fill(th().accent);
                if ui.add(primary).on_hover_text("Take the file's version; your changes here are dropped").clicked() {
                    match jobfile::load(path) {
                        Ok(j) => self.reload_job_file(j, path),
                        Err(e) => self.status = format!("Couldn't reload {}: {e}", path.display()),
                    }
                    close = true;
                }
                if ui.button("Keep mine").on_hover_text("Keep the window's version; saving will write over the file's").clicked() {
                    // Don't ask again for this version of the file.
                    if let (Some(l), Ok(j)) = (self.job_link.as_mut(), jobfile::load(path)) {
                        l.mtime = mtime(path);
                        l.disk_sig = job_sig(&j);
                    }
                    self.status = "Kept the window's version — saving will write over the file.".into();
                    close = true;
                }
            });
        });
        close
    }
}

#[cfg(test)]
mod filewatch_tests {
    use super::*;

    fn wait_tick() {
        // File times can be coarse; make sure the next write gets a new one.
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }

    #[test]
    fn an_agent_change_reloads_the_window_unless_both_changed() {
        let dir = std::env::temp_dir().join(format!("nestpro_watch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("w.snjob");
        let mut a = App::new(None);
        a.save_job_to(&path);
        let link = a.job_link.clone().unwrap();
        assert_eq!(link.path, path);
        let n = a.parts.len();

        // An agent adds a part through the engine (the CLI's way).
        wait_tick();
        let mut s = crate::engine::session::Session::new(crate::engine::Ctx::builtin());
        s.call("add-shape", Some(path.to_str().unwrap()), &serde_json::json!({"shape": "disc", "w": 150, "qty": 2, "name": "From agent"})).unwrap();
        let ctx = egui::Context::default();
        a.disk_poll = -10.0;
        a.watch_job_file(&ctx);
        assert_eq!(a.parts.len(), n + 1, "reloaded: {}", a.status);
        assert!(a.parts.iter().any(|p| p.name == "From agent"));

        // Both change: the window asks.
        a.parts[0].qty += 1;
        wait_tick();
        s.call("add-shape", None, &serde_json::json!({"shape": "rectangle", "qty": 1, "name": "Second"})).unwrap();
        a.disk_poll = -10.0;
        a.watch_job_file(&ctx);
        assert!(matches!(a.pending, Some(Pending::JobFileChanged { .. })));
        assert!(!a.parts.iter().any(|p| p.name == "Second"), "nothing taken without asking");
        // Saving without answering doesn't write over the agent's change.
        a.pending = None;
        a.save_job_to(&path);
        assert!(matches!(a.pending, Some(Pending::JobFileChanged { .. })), "{}", a.status);
        assert!(jobfile::load(&path).unwrap().parts.iter().any(|p| p.name == "Second"));
        // Keep mine, then save: the agent's undo can't bring back a version
        // from before the window's save.
        if let (Some(l), Ok(j)) = (a.job_link.as_mut(), jobfile::load(&path)) {
            l.mtime = mtime(&path);
            l.disk_sig = job_sig(&j);
        }
        a.pending = None;
        wait_tick();
        a.save_job_to(&path);
        assert!(a.pending.is_none(), "{}", a.status);
        assert!(s.call("undo", None, &serde_json::json!({})).is_err());
        let disk = jobfile::load(&path).unwrap();
        assert!(!disk.parts.iter().any(|p| p.name == "Second") && disk.parts[0].qty == a.parts[0].qty);

        // Each job keeps its own file: switching jobs doesn't carry the path along.
        a.pending = None;
        let other = (0..a.jobs.len()).find(|&i| i != a.cur_job).unwrap();
        let mine = a.cur_job;
        a.open_job(other);
        assert!(a.job_path.is_none() && a.job_link.is_none());
        a.open_job(mine);
        assert_eq!(a.job_path.as_deref(), Some(path.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
