//! The Job Explorer's Library section: the Library's job folder tree
//! (folders.rs), the same on every PC that shares the Library, with the
//! original's folder menu (JX1): New ▸ Folder, Cut, Paste, Delete, Change
//! name, Refresh (JX10–JX15).

use crate::folders::{self, Refused, Tree, TOP};
use crate::library::{Entry, Kind, Library};
use crate::libui::LibItem;
use crate::library::auto_ref;
use crate::theme::th;
use crate::App;
use eframe::egui;

/// What the tree's menus asked for, done after drawing.
#[derive(Clone)]
enum TreeAct {
    Open(u64),
    NewFolder(u64),
    Cut(LibItem),
    Paste(u64),
    Delete(u64),
    Rename(u64),
    Typing(u64, String),
    CommitName(u64, String),
    CancelName,
    Refresh,
}

/// What drawing one folder needs from the tree around it.
struct FolderCtx {
    edit: Option<(u64, String)>,
    cut: Option<LibItem>,
    reveal: Vec<u64>,
    reveal_all: bool,
    act: Option<TreeAct>,
}

impl App {
    /// Read the folder tree again (with the list already read).
    pub(crate) fn lib_reload_tree(&mut self, lib: &Library) {
        match folders::load(lib, Kind::Job) {
            Ok(t) => self.libui.tree = t,
            Err(e) => {
                self.libui.tree = Tree::default();
                self.libui.status = e;
            }
        }
    }

    /// The Library's saved jobs.
    pub fn lib_jobs(&self) -> Vec<Entry> {
        self.libui.entries.iter().filter(|e| e.kind == Kind::Job).cloned().collect()
    }

    /// Change the Library's job folders under its write lock (JX10), then
    /// show the tree as it is now, other PCs' changes included. A refusal
    /// goes to the status line.
    fn lib_change<T>(&mut self, f: impl FnOnce(&mut Tree, &[Entry]) -> Result<T, Refused>) -> Option<T> {
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = e;
                return None;
            }
        };
        let r = folders::change(&lib, Kind::Job, f);
        self.libui.entries = lib.entries();
        self.lib_reload_tree(&lib);
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.status = e.say(false);
                None
            }
        }
    }

    /// New ▸ Folder in `parent` (TOP: at the top of the Library): made in the
    /// Library at once as "New folder" (JX7), its name ready to type.
    pub fn lib_new_folder(&mut self, parent: u64) -> Option<u64> {
        let (id, name) = self.lib_change(|t, _| {
            let n = t.new_name(parent)?;
            t.make(parent, &n).map(|id| (id, n))
        })?;
        self.libui.edit = Some((id, name));
        self.status = "New folder — type its name and press Enter.".into();
        Some(id)
    }

    /// Change name (JX5, JX6).
    pub fn lib_rename_folder(&mut self, id: u64, name: &str) -> bool {
        let done = self.lib_change(|t, _| t.rename(id, name)).is_some();
        if done {
            self.libui.edit = None;
            self.status = format!("Folder renamed to {}.", name.trim());
        }
        done
    }

    /// Delete a folder: only an empty one (JX13).
    pub fn lib_delete_folder(&mut self, id: u64) -> bool {
        let name = self.libui.tree.get(id).map_or_else(String::new, |f| f.name.clone());
        let done = self.lib_change(|t, es| t.delete(id, es)).is_some();
        if done {
            if self.libui.cut == Some(LibItem::Folder(id)) {
                self.libui.cut = None;
            }
            self.status = format!("Folder {name} deleted.");
        }
        done
    }

    /// The name of a job or folder in the tree, for messages.
    fn lib_item_name(&self, item: LibItem) -> String {
        match item {
            LibItem::Job(r) => self.libui.entries.iter().find(|e| e.kind == Kind::Job && e.id == r).map_or_else(|| format!("Job {}", crate::library::auto_ref(Kind::Job, r)), |e| format!("Job '{}'", e.name)),
            LibItem::Folder(f) => self.libui.tree.get(f).map_or_else(|| "That folder".into(), |f| format!("Folder '{}'", f.name)),
        }
    }

    /// Cut: the job or folder waits for Paste (JX14).
    pub fn lib_cut(&mut self, item: LibItem) {
        self.libui.cut = Some(item);
        self.status = format!("{} cut — right-click a folder (or the Library) and Paste to move it there.", self.lib_item_name(item));
    }

    /// Paste what was cut into `to` (TOP: the top of the Library), JX12,
    /// JX14. A refused paste keeps it cut.
    pub fn lib_paste(&mut self, to: u64) -> bool {
        let Some(item) = self.libui.cut else {
            self.status = "Nothing is cut — right-click a job or folder and Cut it first.".into();
            return false;
        };
        let what = self.lib_item_name(item);
        let done = self
            .lib_change(|t, es| match item {
                LibItem::Folder(f) => t.move_folder(f, to),
                LibItem::Job(r) if es.iter().any(|e| e.id == r) => t.put(r, to),
                LibItem::Job(_) => Err(Refused::Gone(what.clone())),
            })
            .is_some();
        if done {
            self.libui.cut = None;
            if let LibItem::Job(r) = item {
                // An open copy of it now belongs in that folder (JX15).
                for j in self.jobs.iter_mut().filter(|j| j.lib_id == Some(r)) {
                    j.lib_folder = Some(to);
                }
            }
            self.status = format!("{what} moved to {}.", self.libui.tree.place(to));
        }
        done
    }

    /// The Library section of the Job Explorer: the job folder tree with the
    /// original's folder menu (JX1: New ▸ Folder, Cut, Paste, Delete, Change
    /// name, Refresh), each folder's sub-folders first, then its jobs. With a
    /// search, the matching jobs with their folders. Returns a job to open.
    pub(crate) fn library_folder_tree(&mut self, ui: &mut egui::Ui, jobs: &[Entry], needle: &str) -> Option<u64> {
        let tree = self.libui.tree.clone();
        let cut = self.libui.cut;
        let edit = self.libui.edit.clone();
        let reveal: Vec<u64> = std::mem::take(&mut self.libui.reveal);
        let reveal_all = self.libui.reveal_all;
        let lib_tip = format!("Saved jobs: kept in the Library ({}), the same on every PC that shares it. Click one to open it (it joins Open jobs).", self.library_dir().display());
        let open_ids: Vec<u64> = self.jobs.iter().filter_map(|j| j.lib_id).chain(self.job_lib_id).collect();
        let mut act: Option<TreeAct> = None;
        // A folder's menu, and the Library's own (`id` TOP): JX1.
        let menu = |ui: &mut egui::Ui, id: u64, act: &mut Option<TreeAct>| {
            ui.menu_button("New", |ui| {
                if ui.button("📁 Folder").clicked() {
                    *act = Some(TreeAct::NewFolder(id));
                    ui.close();
                }
            });
            if id != TOP && ui.button("Cut").on_hover_text("Then right-click another folder (or the Library) and Paste to move it there").clicked() {
                *act = Some(TreeAct::Cut(LibItem::Folder(id)));
                ui.close();
            }
            if ui.add_enabled(cut.is_some(), egui::Button::new("Paste")).on_disabled_hover_text("Cut a job or folder first").clicked() {
                *act = Some(TreeAct::Paste(id));
                ui.close();
            }
            if id != TOP {
                if ui.button("Delete").on_hover_text("Only an empty folder can be deleted").clicked() {
                    *act = Some(TreeAct::Delete(id));
                    ui.close();
                }
                if ui.button("Change name").clicked() {
                    *act = Some(TreeAct::Rename(id));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Refresh").on_hover_text("Read the Library again: what other PCs saved, made or moved").clicked() {
                *act = Some(TreeAct::Refresh);
                ui.close();
            }
        };
        let job_row = |ui: &mut egui::Ui, e: &Entry, path: Option<&str>, act: &mut Option<TreeAct>| {
            let open_now = open_ids.contains(&e.id);
            let mut label = format!("{}  ·  {} sh{}", e.name, e.count_b, if open_now { "  · open" } else { "" });
            if let Some(p) = path.filter(|p| !p.is_empty()) {
                label = format!("{label}  ·  {p}");
            }
            let mut text = egui::RichText::new(label);
            if cut == Some(LibItem::Job(e.id)) {
                text = egui::RichText::new(format!("✂ {}", text.text())).color(th().text_dim);
            }
            // Who last saved it, and when (LB6).
            let saved = e.saved_note(&self.clock);
            let saved = if saved.is_empty() { String::new() } else { format!("\n{saved}") };
            let r = ui.selectable_label(self.job_lib_id == Some(e.id), text).on_hover_text(format!("{} — {lib_tip}{saved}\nRight-click: Open, Cut", auto_ref(Kind::Job, e.id)));
            // Another PC has it in use (CU10): said under it.
            if let Some(n) = self.in_use_note(e.id) {
                ui.label(egui::RichText::new(format!("🔒 {n}")).size(11.0).color(th().warn_fg));
            }
            if r.clicked() {
                *act = Some(TreeAct::Open(e.id));
            }
            r.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    *act = Some(TreeAct::Open(e.id));
                    ui.close();
                }
                if ui.button("Cut").on_hover_text("Then right-click a folder (or the Library) and Paste to move the job there").clicked() {
                    *act = Some(TreeAct::Cut(LibItem::Job(e.id)));
                    ui.close();
                }
            });
        };
        if jobs.is_empty() && tree.folders.is_empty() {
            // One quiet line until the Library has jobs or folders.
            ui.label(egui::RichText::new("📚 Saved jobs (Library) — none yet").color(th().text_dim))
                .on_hover_text(format!("Jobs saved in {}.\nRight-click an open job → Save to library (or Jobs → Save to library) to keep it here; right-click here for New ▸ Folder.", self.library_dir().display()))
                .context_menu(|ui| menu(ui, TOP, &mut act));
        } else {
            let root = egui::CollapsingHeader::new(egui::RichText::new(format!("📚 Saved jobs (Library) ({})", jobs.len())).strong()).id_salt("lib_jobs_root").default_open(true).show(ui, |ui| {
                if !needle.is_empty() {
                    // Searching: the matching jobs, each with its folder.
                    let found: Vec<&Entry> = jobs.iter().filter(|e| e.name.to_lowercase().contains(needle) || tree.path(tree.folder_of(e.id)).to_lowercase().contains(needle)).collect();
                    if found.is_empty() {
                        ui.label(egui::RichText::new("No saved job matches.").size(11.0).color(th().text_dim));
                    }
                    for e in found {
                        job_row(ui, e, Some(&tree.path(tree.folder_of(e.id))), &mut act);
                    }
                    return;
                }
                fn folder(ui: &mut egui::Ui, t: &Tree, id: u64, jobs: &[Entry], ctx: &mut FolderCtx, job_row: &dyn Fn(&mut egui::Ui, &Entry, Option<&str>, &mut Option<TreeAct>), menu: &dyn Fn(&mut egui::Ui, u64, &mut Option<TreeAct>)) {
                    for f in t.children(id) {
                        // Its name being typed (New folder / Change name).
                        if let Some((eid, text)) = ctx.edit.as_ref().filter(|(e, _)| *e == f.id) {
                            let mut txt = text.clone();
                            let r = ui.add(egui::TextEdit::singleline(&mut txt).desired_width(160.0));
                            r.request_focus();
                            if r.changed() {
                                ctx.act = Some(TreeAct::Typing(*eid, txt.clone()));
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                ctx.act = Some(TreeAct::CommitName(*eid, txt));
                            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                ctx.act = Some(TreeAct::CancelName);
                            }
                            continue;
                        }
                        let inside = t.records_within(f.id, jobs);
                        let mut title = egui::RichText::new(format!("📁 {}  ({inside})", f.name));
                        if ctx.cut == Some(LibItem::Folder(f.id)) {
                            title = egui::RichText::new(format!("✂ 📁 {}  ({inside})", f.name)).color(th().text_dim);
                        }
                        let mut h = egui::CollapsingHeader::new(title).id_salt(("lib_folder", f.id)).default_open(false);
                        if ctx.reveal_all || ctx.reveal.contains(&f.id) {
                            h = h.open(Some(true));
                        }
                        let mut a = None;
                        let resp = h.show(ui, |ui| {
                            folder(ui, t, f.id, jobs, ctx, job_row, menu);
                            for e in t.records_in(f.id, jobs) {
                                job_row(ui, e, None, &mut a);
                            }
                        });
                        if a.is_some() {
                            ctx.act = a;
                        }
                        let mut a = None;
                        resp.header_response.on_hover_text(format!("{inside} job(s) in it and its folders; {} folder(s) in it — right-click for New folder, Cut, Paste, Delete, Change name, Refresh", t.children(f.id).len())).context_menu(|ui| menu(ui, f.id, &mut a));
                        if a.is_some() {
                            ctx.act = a;
                        }
                    }
                }
                let mut ctx = FolderCtx { edit: edit.clone(), cut, reveal: reveal.clone(), reveal_all, act: None };
                folder(ui, &tree, TOP, jobs, &mut ctx, &job_row, &menu);
                for e in tree.records_in(TOP, jobs) {
                    job_row(ui, e, None, &mut ctx.act);
                }
                if ctx.act.is_some() {
                    act = ctx.act;
                }
            });
            root.header_response.on_hover_text(format!("{lib_tip}\nRight-click: New ▸ Folder, Paste, Refresh")).context_menu(|ui| menu(ui, TOP, &mut act));
        }
        let mut open = None;
        match act {
            Some(TreeAct::Open(id)) => open = Some(id),
            Some(TreeAct::NewFolder(parent)) => {
                if self.lib_new_folder(parent).is_some() {
                    self.libui.reveal.push(parent);
                }
            }
            Some(TreeAct::Cut(item)) => self.lib_cut(item),
            Some(TreeAct::Paste(to)) => {
                if self.lib_paste(to) {
                    self.libui.reveal.push(to);
                }
            }
            Some(TreeAct::Delete(id)) => {
                self.lib_delete_folder(id);
            }
            Some(TreeAct::Rename(id)) => self.libui.edit = tree.get(id).map(|f| (id, f.name.clone())),
            Some(TreeAct::Typing(id, t)) => self.libui.edit = Some((id, t)),
            Some(TreeAct::CommitName(id, t)) => {
                // Unchanged: nothing to write.
                if tree.get(id).is_some_and(|f| f.name == t.trim()) {
                    self.libui.edit = None;
                } else {
                    self.lib_rename_folder(id, &t);
                }
            }
            Some(TreeAct::CancelName) => self.libui.edit = None,
            Some(TreeAct::Refresh) => {
                self.lib_refresh();
                self.status = "Library read again.".into();
            }
            None => {}
        }
        open
    }

    /// The Library section opened out, as paths: "Acme/", "Acme/2026-11/",
    /// "Acme/2026-11/DEMO-001" (folders first in each folder, by name).
    pub fn library_outline(&self) -> Vec<String> {
        self.libui.tree.outline(&self.lib_jobs())
    }
}

#[cfg(test)]
mod libtree_tests {
    use crate::folders::TOP;
    use crate::libui::LibItem;
    use crate::shared::{Clock, Who};
    use crate::App;
    use std::path::{Path, PathBuf};

    /// 2026-11-09 09:20 UTC: a test clock shows UTC.
    const NOV_9_0920: u64 = 1_794_216_000;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_libtree_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A new, empty Library in a scratch folder: made once, as in Settings
    /// (LB12).
    fn library(tag: &str) -> PathBuf {
        let dir = scratch(tag);
        crate::library::create(&dir).unwrap();
        dir
    }

    /// One programming PC: `user` at a window on PC `name`, working on the
    /// Library in `dir`, going by `clock` (LB5). Its warn switch is off
    /// (CU1): it holds no job, so another PC may move the jobs it has open;
    /// folders with jobs in use are tested in `inuseui.rs`.
    fn pc(dir: &Path, user: &str, name: &str, clock: &Clock) -> App {
        let mut a = App::new(None);
        a.cfg.in_use_warn = false;
        a.cfg.library_dir = dir.display().to_string();
        a.who = Who::new(user, name);
        a.clock = clock.clone();
        a.lib_refresh();
        a
    }

    /// A new folder in `parent`, named `name`.
    fn folder(a: &mut App, parent: u64, name: &str) -> u64 {
        let id = a.lib_new_folder(parent).unwrap_or_else(|| panic!("{}", a.status));
        assert!(a.lib_rename_folder(id, name), "{}", a.status);
        id
    }

    #[test]
    fn a_folder_made_renamed_nested_moved_or_deleted_on_one_pc_shows_on_another_after_a_refresh() {
        let dir = library("two_pcs");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/"]);
        // Renamed on PC 1.
        assert!(pc1.lib_rename_folder(acme, "Acme Ltd"), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme Ltd/", "Acme Ltd/2026-11/"]);
        // Moved into another folder with Cut and Paste.
        let beta = folder(&mut pc1, TOP, "Beta");
        pc1.lib_cut(LibItem::Folder(nov));
        assert!(pc1.lib_paste(beta), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Acme Ltd/", "Beta/", "Beta/2026-11/"]);
        // Deleted.
        assert!(pc1.lib_delete_folder(acme), "{}", pc1.status);
        pc2.lib_refresh();
        assert_eq!(pc2.library_outline(), ["Beta/", "Beta/2026-11/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_pcs_changing_folders_without_refreshing_never_lose_each_others_changes() {
        let dir = library("stale");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        // Both make a folder at the top at once, neither having seen the other's.
        let a = pc1.lib_new_folder(TOP).unwrap();
        let b = pc2.lib_new_folder(TOP).unwrap();
        assert_ne!(a, b);
        assert_eq!(pc2.library_outline(), ["New folder/", "New folder (1)/"]);
        assert_eq!(pc2.libui.edit.as_ref().map(|e| e.1.as_str()), Some("New folder (1)"), "the name being typed is the one it got");
        // PC 1 names its folder; PC 2, still showing the old name, makes one in it.
        assert!(pc1.lib_rename_folder(a, "Acme"));
        let inside = folder(&mut pc2, a, "2026-11");
        pc1.lib_refresh();
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "New folder (1)/"]);
        // PC 1 deletes PC 2's empty folder; PC 2 hasn't refreshed and tries to
        // use it: refused, saying why and what to do.
        assert!(pc1.lib_delete_folder(inside));
        assert!(!pc2.lib_rename_folder(inside, "2026-12"));
        assert!(pc2.status.contains("no longer in the Library") && pc2.status.contains("Refresh"), "{}", pc2.status);
        // A refused change still shows PC 2 the folders as they are now.
        assert_eq!(pc2.library_outline(), ["Acme/", "New folder (1)/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Save the open job `i` into the Library under `name` (in the Open jobs
    /// folder `week`); its record number.
    fn save(a: &mut App, i: usize, name: &str, week: &str) -> u64 {
        a.open_job(i);
        a.job_name = name.into();
        a.jobs[i].week = week.into();
        a.save_job_to_library();
        a.job_lib_id.unwrap_or_else(|| panic!("{}", a.status))
    }

    #[test]
    fn deleting_a_folder_that_holds_a_job_is_refused_and_an_empty_folder_is_kept() {
        let dir = library("delete");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        folder(&mut pc1, TOP, "Empty");
        let job = save(&mut pc1, 0, "DEMO-001", "W12");
        pc1.lib_cut(LibItem::Job(job));
        assert!(pc1.lib_paste(acme), "{}", pc1.status);
        assert!(!pc1.lib_delete_folder(acme), "a folder holding a job was deleted");
        assert!(pc1.status.contains("still holds 1 job"), "{}", pc1.status);
        // Another PC, a stale view of it: refused there too.
        let mut pc2 = pc(&dir, "kim", "PROG-2", &clock);
        assert!(!pc2.lib_delete_folder(acme));
        // A restart: both folders are still there, the empty one too.
        let mut after = pc(&dir, "sam", "PROG-1", &clock);
        assert_eq!(after.library_outline(), ["Acme/", "Acme/DEMO-001", "Empty/", "W12/"]);
        // Once the job is deleted from the Library, the folder can go.
        after.library().unwrap().delete(crate::library::Kind::Job, job).unwrap();
        after.lib_refresh();
        assert!(after.lib_delete_folder(acme), "{}", after.status);
        assert_eq!(after.library_outline(), ["Empty/", "W12/"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cut_and_paste_move_a_job_and_a_folder_never_goes_into_its_own_sub_folder() {
        let dir = library("cut_paste");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        let week = folder(&mut pc1, nov, "Week 46");
        let job = save(&mut pc1, 0, "DEMO-001", "");
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/Week 46/", "DEMO-001"]);
        // The job into a folder three levels down.
        pc1.lib_cut(LibItem::Job(job));
        assert!(pc1.lib_paste(week), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/Week 46/", "Acme/2026-11/Week 46/DEMO-001"]);
        // A folder into itself, or into one of its own folders: refused, still cut.
        pc1.lib_cut(LibItem::Folder(acme));
        assert!(!pc1.lib_paste(acme));
        assert!(!pc1.lib_paste(week));
        assert!(pc1.status.contains("can't go into itself or one of its own folders"), "{}", pc1.status);
        assert_eq!(pc1.libui.cut, Some(LibItem::Folder(acme)));
        // Out to the top instead: it goes with everything in it.
        pc1.lib_cut(LibItem::Folder(week));
        assert!(pc1.lib_paste(TOP), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Week 46/", "Week 46/DEMO-001"]);
        // A name taken where it goes (without case, JX5, JX6): refused.
        folder(&mut pc1, nov, "week 46");
        pc1.lib_cut(LibItem::Folder(week));
        assert!(!pc1.lib_paste(nov));
        assert!(pc1.status.contains("error 9"), "{}", pc1.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_saved_for_the_first_time_goes_into_its_folder_and_saving_again_never_moves_it() {
        let dir = library("first_save");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let nov = folder(&mut pc1, acme, "2026-11");
        // A new job: into the top-level folder named like its Open jobs folder, made if missing.
        let job = save(&mut pc1, 0, "DEMO-001", "W12");
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "W12/", "W12/DEMO-001"]);
        // Filed elsewhere on PC 2; saving it again on PC 1 doesn't move it back.
        let mut pc2 = pc(&dir, "kim", "PROG-2", &clock);
        pc2.lib_cut(LibItem::Job(job));
        assert!(pc2.lib_paste(nov), "{}", pc2.status);
        pc1.parts[0].qty += 1;
        pc1.save_job_to_library();
        assert_eq!(pc1.job_lib_id, Some(job), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "W12/"]);
        // Opened from there on PC 2, it sits in an Open jobs folder of that
        // name, and a copy of it is saved next to it.
        pc2.open_library_job(job);
        assert_eq!(pc2.jobs[pc2.cur_job].week, "2026-11");
        pc2.copy_job();
        pc2.save_job_to_library();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "Acme/2026-11/DEMO-001 (1)", "W12/"]);
        // The folder it came from deleted meanwhile: a copy goes by its Open
        // jobs folder's name instead.
        pc2.copy_job();
        let copy = pc2.cur_job;
        pc2.jobs[copy].week = "W13".into();
        let w12 = pc2.libui.tree.find("W12").unwrap();
        pc2.jobs[copy].lib_folder = Some(w12);
        assert!(pc2.lib_delete_folder(w12), "{}", pc2.status);
        pc2.save_job_to_library();
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/2026-11/", "Acme/2026-11/DEMO-001", "Acme/2026-11/DEMO-001 (1)", "W13/", "W13/DEMO-001 (1) (1)"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_saved_before_folders_opens_with_its_folder_names_as_top_level_folders() {
        // Record files and a list as the program wrote them before folders:
        // each job carries its folder's name, and there is no folder tree.
        let dir = scratch("old_format");
        let clock = Clock::at(NOV_9_0920);
        let job = |name: &str, week: &str| {
            let spec = crate::nest::SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() };
            let mut j = crate::jobfile::JobFile::new_job(name, spec);
            j.week = week.into();
            j
        };
        let jobs = [job("DEMO-001", "W12"), job("DEMO-002", "Acme"), job("DEMO-003", "w12"), job("DEMO-004", "")];
        let mut entries = vec![];
        for (k, j) in jobs.iter().enumerate() {
            let id = k as u64 + 1;
            let p = dir.join("Jobs").join("0").join(format!("{id}.snjob"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, serde_json::to_string_pretty(j).unwrap()).unwrap();
            entries.push(serde_json::json!({"kind": "Job", "id": id, "name": j.name, "folder": j.week, "material": 0, "thickness": 3.0, "w": 2000.0, "h": 1000.0, "count_a": 0, "count_b": 0, "updated": 1_790_000_000u64}));
        }
        std::fs::write(dir.join(crate::library::INDEX), serde_json::json!({"format": "nestpro-library", "version": 1, "entries": entries}).to_string()).unwrap();
        let before: Vec<Vec<u8>> = (1..=4).map(|id| std::fs::read(dir.join("Jobs").join("0").join(format!("{id}.snjob"))).unwrap()).collect();
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        // W12 and w12 are one folder (JX6), spelt as on the first job.
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/DEMO-002", "W12/", "W12/DEMO-001", "W12/DEMO-003", "DEMO-004"]);
        // Nothing moved: the job records are as they were.
        let after: Vec<Vec<u8>> = (1..=4).map(|id| std::fs::read(dir.join("Jobs").join("0").join(format!("{id}.snjob"))).unwrap()).collect();
        assert_eq!(after, before);
        // The folders are real ones, the same on another PC: renaming one
        // keeps its jobs.
        let w12 = pc1.libui.tree.find("W12").unwrap();
        assert!(pc1.lib_rename_folder(w12, "Week 12"));
        let pc2 = pc(&dir, "kim", "PROG-2", &clock);
        assert_eq!(pc2.library_outline(), ["Acme/", "Acme/DEMO-002", "Week 12/", "Week 12/DEMO-001", "Week 12/DEMO-003", "DEMO-004"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX16 with the tree (JX10): a name another job holds is refused
    /// naming that job's folder as the tree shows it, after it was moved
    /// there. Cut and Paste keep a job's name, so moving never trips the
    /// check, even for jobs from before names were unique (JX17).
    #[test]
    fn a_taken_name_is_refused_naming_its_folder_in_the_tree_and_moving_never_trips_it() {
        let dir = library("taken_name");
        let clock = Clock::at(NOV_9_0920);
        let (mut pc1, mut pc2) = (pc(&dir, "sam", "PROG-1", &clock), pc(&dir, "kim", "PROG-2", &clock));
        let job = save(&mut pc1, 0, "DEMO-001 frames", "Acme");
        let beta = folder(&mut pc1, TOP, "Beta");
        let nov = folder(&mut pc1, beta, "2026-11");
        pc1.lib_cut(LibItem::Job(job));
        assert!(pc1.lib_paste(nov), "{}", pc1.status);
        // PC 2 saves another job under that name, in other capitals: refused,
        // naming where the job is now, not the folder name written on it.
        pc2.lib_refresh();
        pc2.open_job(1);
        pc2.job_name = "demo-001 FRAMES".into();
        pc2.save_job_to_library();
        assert!(pc2.job_lib_id.is_none(), "saved under a taken name: {}", pc2.status);
        assert!(pc2.status.contains("'DEMO-001 frames'") && pc2.status.contains("folder Beta/2026-11") && !pc2.status.contains("Acme"), "{}", pc2.status);
        // A Library from before names were unique: another job shares the
        // name. Both move freely.
        let lib = pc1.library().unwrap();
        let mut twin = lib.load_job(job).unwrap();
        twin.name = "demo-001 FRAMES".into();
        std::fs::write(lib.path(crate::library::Kind::Job, 9), serde_json::to_string(&twin).unwrap()).unwrap();
        lib.rebuild_index().unwrap();
        pc1.lib_refresh();
        let acme = pc1.libui.tree.find("Acme").unwrap();
        pc1.lib_cut(LibItem::Job(9));
        assert!(pc1.lib_paste(acme), "{}", pc1.status);
        pc1.lib_cut(LibItem::Job(job));
        assert!(pc1.lib_paste(beta), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/demo-001 FRAMES", "Beta/", "Beta/2026-11/", "Beta/DEMO-001 frames"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11 for folders: with the Library's folder gone (the share down),
    /// New folder, Change name, Delete and Paste are refused naming the
    /// Library's folder, as a save is, and nothing is made. Once it's back,
    /// they work.
    #[test]
    fn folder_changes_refuse_a_library_that_cant_be_reached_and_make_nothing() {
        let dir = library("unreachable");
        let clock = Clock::at(NOV_9_0920);
        let mut pc1 = pc(&dir, "sam", "PROG-1", &clock);
        let acme = folder(&mut pc1, TOP, "Acme");
        let job = save(&mut pc1, 0, "DEMO-001", "");
        let away = dir.with_extension("away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&dir, &away).unwrap();
        let said = format!("The Library on {} can't be reached", dir.display());
        assert!(pc1.lib_new_folder(TOP).is_none());
        assert!(pc1.status.contains(&said), "New folder: {}", pc1.status);
        assert!(!pc1.lib_rename_folder(acme, "Acme Ltd"));
        assert!(pc1.status.contains(&said), "Change name: {}", pc1.status);
        assert!(!pc1.lib_delete_folder(acme));
        assert!(pc1.status.contains(&said), "Delete: {}", pc1.status);
        pc1.lib_cut(LibItem::Job(job));
        assert!(!pc1.lib_paste(acme));
        assert!(pc1.status.contains(&said), "Paste: {}", pc1.status);
        assert!(!dir.exists(), "a folder change made the Library folder");
        // The share is back: the job is still cut, and Paste moves it.
        std::fs::rename(&away, &dir).unwrap();
        pc1.lib_retry();
        assert!(pc1.lib_paste(acme), "{}", pc1.status);
        assert_eq!(pc1.library_outline(), ["Acme/", "Acme/DEMO-001"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
