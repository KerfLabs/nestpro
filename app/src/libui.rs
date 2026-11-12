//! The Library window and actions: save the open job or its products to the
//! app's own library (library.rs), browse and search it, open jobs and add
//! stored products to the open job.

use crate::library::{Changed, Entry, Kind, Library, Record, Refused, SaveTo};
use crate::libtree::{TreeAct, TreeLook};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::RichText;

#[derive(Default)]
pub struct LibUi {
    pub open: bool,
    /// 0 jobs, 1 products.
    pub tab: usize,
    pub search: String,
    pub entries: Vec<Entry>,
    pub loaded: bool,
    pub status: String,
    /// Library products with their parts, for the Add product menu.
    pub products: Vec<(Entry, crate::nest::Part)>,
    /// Search text in the Add product menu.
    pub part_search: String,
    /// Show library parts of every material and thickness, not only the job's.
    pub part_all: bool,
    /// A save the Library refused because someone else saved it since it was
    /// read, waiting for this user's choice (LB9).
    pub question: Option<SaveQuestion>,
    /// Overwrite was chosen: the open job's next save replaces this save of
    /// its record, their stock booking included (LB9). Nothing in the job
    /// changes until that save is under the write lock.
    pub overwrite: Option<Changed>,
    /// The Library's job folders (JX10), as last read.
    pub tree: crate::folders::Tree,
    /// A job or folder cut in the Library section, waiting for Paste (JX14).
    pub cut: Option<LibItem>,
    /// A Library folder whose name is being typed (New folder, Change name).
    pub edit: Option<(u64, String)>,
    /// Library folders to open out the next time the tree is drawn (where a
    /// folder was made or something pasted; every folder for screenshots).
    pub reveal: Vec<u64>,
    pub reveal_all: bool,
    /// The Library's part folders (JX18), shown in the Library window's
    /// Products tab, with what is cut or being named there.
    pub parts: FolderView,
    /// Why the Library can't be listed (LB11, LB12): its folder can't be
    /// reached or holds no Library. The lists say this instead of looking
    /// empty.
    pub error: Option<String>,
}

/// What an open job read from the Library (LB7): the revision of its
/// record, and the record and revision of each library part in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibRead {
    pub revision: u64,
    pub parts: std::collections::BTreeMap<String, Record>,
}

/// A save someone else's newer save stopped (LB7), and what it asks (LB9).
#[derive(Clone, Debug, PartialEq)]
pub enum SaveQuestion {
    /// The open job: Reload theirs, Save mine as a copy, or Overwrite.
    Job(Changed),
    /// The job's part `part` (its index): Keep theirs, Overwrite, or Save
    /// mine under a new name. `added`: the job's part was added from that
    /// library part (else it only has its name).
    Part { part: usize, changed: Changed, added: bool },
    /// An open job with unsaved changes here that someone else saved since
    /// it was opened, found by a refresh (LB15): Reload theirs, Keep mine,
    /// or Save mine as a copy.
    Theirs(Changed),
}

/// A job, part or folder in one of the Library's trees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibItem {
    /// A saved job, by its record number.
    Job(u64),
    /// A library part, by its record number.
    Part(u64),
    Folder(u64),
}

impl LibItem {
    /// The record `id` of a `kind` tree.
    pub fn record(kind: Kind, id: u64) -> LibItem {
        match kind {
            Kind::Job => LibItem::Job(id),
            Kind::Product => LibItem::Part(id),
        }
    }
}

/// One of the Library's folder trees as this window shows it: the tree as
/// last read, what is cut waiting for Paste (JX14), the folder whose name is
/// being typed, and the folders to open out next time it's drawn.
#[derive(Clone, Default)]
pub struct FolderView {
    pub tree: crate::folders::Tree,
    pub cut: Option<LibItem>,
    pub edit: Option<(u64, String)>,
    pub reveal: Vec<u64>,
}

/// A `kind` tree's view, to change (`LibUi::view`).
pub struct ViewMut<'a> {
    pub tree: &'a mut crate::folders::Tree,
    pub cut: &'a mut Option<LibItem>,
    pub edit: &'a mut Option<(u64, String)>,
    pub reveal: &'a mut Vec<u64>,
}

impl LibUi {
    /// The jobs' (JX10) or the parts' (JX18) folder tree as last read.
    pub fn tree_of(&self, kind: Kind) -> &crate::folders::Tree {
        match kind {
            Kind::Job => &self.tree,
            Kind::Product => &self.parts.tree,
        }
    }

    /// The jobs' or the parts' tree view, to change.
    pub fn view(&mut self, kind: Kind) -> ViewMut<'_> {
        match kind {
            Kind::Job => ViewMut { tree: &mut self.tree, cut: &mut self.cut, edit: &mut self.edit, reveal: &mut self.reveal },
            Kind::Product => ViewMut { tree: &mut self.parts.tree, cut: &mut self.parts.cut, edit: &mut self.parts.edit, reveal: &mut self.parts.reveal },
        }
    }

    /// A library part's folder, as the parts tree shows it (JX20): its path,
    /// "" at the top.
    pub fn part_folder(&self, id: u64) -> String {
        self.parts.tree.path(self.parts.tree.folder_of(id))
    }
}

/// The Library can't be listed (LB11, LB12): the message naming the folder,
/// then Try again and the Settings page. Returns (Try again, Settings) clicks.
pub fn unreachable_note(ui: &mut egui::Ui, msg: &str) -> (bool, bool) {
    let mut out = (false, false);
    egui::Frame::new().fill(th().warn_bg).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.label(RichText::new(format!("⚠ {msg}")).color(th().warn_fg));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            out.0 = ui.button("Try again").on_hover_text("Look for the Library again").clicked();
            out.1 = ui.button("Library folder…").on_hover_text("Settings → Folders: where the Library is").clicked();
        });
    });
    out
}

/// The folder a Library folder setting names (empty: the default one on
/// this PC).
pub fn library_dir_of(setting: &str) -> std::path::PathBuf {
    if setting.trim().is_empty() { crate::library::default_dir() } else { std::path::PathBuf::from(setting.trim()) }
}

impl App {
    pub fn library_dir(&self) -> std::path::PathBuf {
        library_dir_of(&self.cfg.library_dir)
    }

    /// Point the Library at a folder picked in Settings (LB12). A folder that
    /// holds a Library is used at once. Any other (new, empty, not there yet,
    /// or on a share that's down) is asked about first: the Library stays
    /// where it was until the answer is yes.
    pub fn pick_library_folder(&mut self, setting: String) {
        let setting = setting.trim().to_string();
        if setting == self.cfg.library_dir.trim() {
            return;
        }
        let dir = library_dir_of(&setting);
        if crate::library::holds_library(&dir) {
            self.cfg.library_dir = setting;
            self.library_folder_changed();
            self.status = format!("The Library is now the one on {}.", dir.display());
        } else {
            self.lib_new_ask = Some(setting);
        }
    }

    /// Yes: make a new, empty Library in the folder asked about, and use it
    /// (LB12). If it can't be made, the Library stays where it was.
    pub fn confirm_new_library(&mut self) {
        let Some(setting) = self.lib_new_ask.take() else { return };
        let dir = library_dir_of(&setting);
        match crate::library::create(&dir) {
            Ok(()) => {
                self.cfg.library_dir = setting;
                self.library_folder_changed();
                self.status = format!("A new, empty Library was made on {}. Every PC that shares it picks this same folder.", dir.display());
            }
            Err(e) => self.status = format!("{e} The Library stays on {}.", self.library_dir().display()),
        }
    }

    /// No: nothing is made, and the Library stays where it was (LB12).
    pub fn cancel_new_library(&mut self) {
        if self.lib_new_ask.take().is_some() {
            self.status = format!("No new Library made. The Library stays on {}.", self.library_dir().display());
        }
    }

    /// The question before a new Library is made in a folder picked in
    /// Settings (LB12).
    pub fn new_library_window(&mut self, ctx: &egui::Context) {
        let Some(setting) = self.lib_new_ask.clone() else { return };
        let dir = library_dir_of(&setting);
        let what = if !dir.is_dir() {
            format!("There's no folder {} yet.", dir.display())
        } else if std::fs::read_dir(&dir).map_or(false, |mut d| d.next().is_none()) {
            format!("{} is empty: there's no Library in it.", dir.display())
        } else {
            format!("{} holds other files, but no Library.", dir.display())
        };
        let (mut yes, mut no) = (false, false);
        let mut open = true;
        egui::Window::new(RichText::new("Make a new Library?").size(16.0).strong())
            .id(egui::Id::unique("new_library_question"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(RichText::new(what).color(th().text_strong));
                ui.add_space(6.0);
                ui.label(RichText::new("Make a new, empty Library there? Every programming PC that should share it then picks this same folder.").color(th().text));
                ui.add_space(4.0);
                ui.label(RichText::new("If the shop's Library is somewhere else, or on a share that can't be reached just now, cancel and pick that folder instead.").size(11.5).color(th().text_dim));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    yes = ui.add(egui::Button::new(RichText::new("Make a new Library").color(egui::Color32::WHITE)).fill(th().accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                    no = ui.add(egui::Button::new(format!("Cancel — keep the Library on {}", self.library_dir().display())).min_size(egui::Vec2::new(0.0, 30.0))).clicked();
                });
            });
        if yes {
            self.confirm_new_library();
        } else if no || !open {
            self.cancel_new_library();
        }
    }

    /// The name shown in brackets in the title bar, as the original shows its
    /// database there: the library folder's name.
    pub fn db_label(&self) -> String {
        self.library_dir().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Library".into())
    }

    /// The Library, as whoever works at this window (LB5).
    /// Holds are judged by what this window has seen of them (CU7).
    pub fn library(&self) -> Result<Library, String> {
        Library::open(&self.library_dir(), &self.who, &self.clock).map(|l| l.watching(&self.inuse.watch))
    }

    pub fn lib_refresh(&mut self) {
        match self.library() {
            Ok(l) => {
                self.libui.entries = l.entries();
                self.lib_reload_tree(&l);
                self.libui.products = self.libui.entries.iter().filter(|e| e.kind == Kind::Product).filter_map(|e| l.load_product(e.id).ok().map(|pf| (e.clone(), pf.part))).collect();
                self.libui.products.sort_by(|a, b| b.0.updated.cmp(&a.0.updated));
                self.libui.error = None;
                // Which jobs other PCs have in use, for the lists (CU10).
                self.look_at_holds();
            }
            // The lists show the Library as unreachable, not empty (LB11).
            // Not tried again every frame (a share that's down can take
            // seconds to answer): Try again, or another folder, looks again.
            Err(e) => {
                self.libui.entries.clear();
                self.libui.products.clear();
                self.libui.status = e.clone();
                self.libui.error = Some(e);
            }
        }
        self.libui.loaded = true;
    }

    /// Look for the Library again (Try again).
    pub fn lib_retry(&mut self) {
        self.lib_refresh();
        if let Some(e) = &self.libui.error {
            self.status = e.clone();
        }
    }

    pub(crate) fn current_jobfile(&mut self) -> crate::jobfile::JobFile {
        self.store_job();
        crate::jobfile::JobFile {
            format: crate::jobfile::FORMAT.into(),
            version: crate::jobfile::VERSION,
            name: self.job_name.clone(),
            week: self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone()),
            parts: self.parts.clone(),
            spec: self.spec.clone(),
            sheets: self.result.sheets.clone(),
            formats: self.result.formats.clone(),
            held: self.held.clone(),
            cut_lines: self.cut_lines.clone(),
            method: self.result.method.clone(),
            manual_cuts: self.manual_cuts.clone(),
            stale: self.stale,
            // The window books the stock on every save (stock_on_save).
            booked: true,
            erp: self.erp_job.clone(),
            saved: None,
            library: self.job_lib_id.map(|id| Record { id, revision: self.lib_read.revision }),
            library_parts: self.lib_read.parts.clone(),
        }
    }

    /// Another library folder: the open jobs' record numbers belong to the old
    /// one, so saving would replace whatever has that number in the new one.
    /// They become new records there.
    pub fn library_folder_changed(&mut self) {
        // The holds belong to the old Library: let go of them there (CU5).
        self.release_holds();
        self.inuse.watch.done_with_all();
        self.inuse = crate::inuseui::InUseUi { watch: self.inuse.watch.clone(), ..Default::default() };
        self.libui.loaded = false;
        // The Sheet stock window lists the new Library's stock (LB12).
        self.stockui.loaded = false;
        self.stockui.sel = None;
        self.job_lib_id = None;
        self.lib_read = Default::default();
        self.libui.question = None;
        self.libui.overwrite = None;
        // What auto-refresh saw was the old Library's (LB14).
        self.autorefresh.forget();
        for j in &mut self.jobs {
            j.lib_id = None;
            j.lib_read = Default::default();
            // Its folder's number is the old Library's too (JX15).
            j.lib_folder = None;
        }
        // A cut, or a name being typed, belongs to the old Library's tree.
        self.libui.cut = None;
        self.libui.edit = None;
        self.libui.reveal.clear();
        self.libui.parts = Default::default();
    }

    /// Where the open job saves to in the Library: over its record as it was
    /// read, or a new record.
    fn job_save_to(&self) -> SaveTo {
        self.job_lib_id.map_or(SaveTo::New, |id| {
            // Overwrite (LB9) goes over their save as the question saw it.
            let revision = self.libui.overwrite.as_ref().filter(|o| o.id == id).map_or(self.lib_read.revision, |o| o.revision);
            SaveTo::Read(Record { id, revision })
        })
    }

    /// A save that stopped before Overwrite changed any stock: the job is as
    /// it was, and the question is back (LB9, LB13).
    fn overwrite_not_done(&mut self) {
        if let Some(c) = self.libui.overwrite.take().filter(|c| Some(c.id) == self.job_lib_id) {
            self.libui.question = Some(SaveQuestion::Job(c));
        }
    }

    /// Save to library refused because the Library can't be reached (LB11):
    /// the lists say so, and the open job stays as it is, unsaved, and open
    /// (LB13).
    fn library_save_unreachable(&mut self, e: String) {
        self.libui.error = Some(e.clone());
        self.status = format!("Couldn't save to the library: {e} Nothing was saved to it; Save job keeps a copy in a file on this PC.");
    }

    /// Save the open job into the library (over its record if it came from
    /// there). When someone else saved that record since it was read, nothing
    /// is saved and the window asks what to do (LB7, LB9): before remnants
    /// are named, and under the write lock before any stock is booked (LB8).
    pub fn save_job_to_library(&mut self) {
        // A Library that can't be reached refuses the save before anything
        // else happens (LB11, LB13).
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.overwrite_not_done();
                return self.library_save_unreachable(e);
            }
        };
        let to = self.job_save_to();
        // Not over a job taken over from this PC, nor under a new name over
        // one someone else has in use (CU9, CU11): asked first, before
        // anything else, and checked again under the write lock. A job
        // opened anyway, or with warn off, saves as usual (CU3).
        if let Some(id) = to.id() {
            if let Err(u) = crate::engine::inuse::may_save_as(&lib, id, &self.job_name) {
                self.ask_in_use(u);
                return;
            }
        }
        if let Err(c) = lib.check(Kind::Job, to) {
            self.ask_about_job(c);
            return;
        }
        if !self.ready_to_save(crate::stockui::SaveTarget::Library) {
            return;
        }
        crate::shared::pause("library reached");
        // Under the Library's write lock (LB1), held until the job is stored
        // and filed: the revision (LB7), then the name, which no other job may
        // hold (JX16), then the stock (LB8), then the record, a new one going
        // into the folder of the library job it was opened or copied from
        // (JX15). A save refused for either reason takes nothing from stock,
        // and no other PC saves in between. The lock is let go before the
        // lists are read again (LB2).
        let mut stock_note = String::new();
        let name = self.job_name.clone();
        let folder = self.jobs.get(self.cur_job).and_then(|j| j.lib_folder);
        let r = lib.save_job_with(to, &name, folder, || {
            // Overwrite (LB9): their sheets go back to stock and their unused
            // remnants come out, then this job is booked as a new one. Done
            // once: saving again after a failure books only what changed.
            if let Some(o) = self.libui.overwrite.take().filter(|o| Some(o.id) == self.job_lib_id) {
                let mut theirs = lib.load_job(o.id)?;
                crate::engine::stockflow::release(&lib, &mut theirs.formats, &mut theirs.held)?;
                crate::engine::stockflow::forget_booking(&mut self.result.formats, &mut self.held);
                // From here the job goes over their save as Overwrite saw it:
                // if storing it fails, saving again finishes it without
                // asking, and gives their sheets back only this once.
                self.lib_read.revision = o.revision;
            }
            stock_note = self.stock_on_save();
            Ok(self.current_jobfile())
        });
        match r {
            Ok(rec) => {
                self.job_lib_id = Some(rec.id);
                self.lib_read.revision = rec.revision;
                self.mark_job_saved(self.cur_job);
                self.lib_refresh();
                let f = self.libui.tree.folder_of(rec.id);
                if let Some(j) = self.jobs.get_mut(self.cur_job) {
                    j.lib_folder = Some(f);
                }
                self.status = format!("Saved job {} to the library as {} ({}).{stock_note}", self.job_name, crate::library::auto_ref(Kind::Job, rec.id), self.library_dir().display());
                // Saved over a job taken over from here once it was free again:
                // it's this PC's to hold again. A new record (a copy) is in
                // use now, and held (CU5).
                self.inuse.lost.remove(&rec.id);
                self.sync_holds();
            }
            Err(Refused::Changed(c)) => self.ask_about_job(c),
            Err(Refused::InUse(u)) => self.ask_in_use(u),
            // Gone since it was found: said the same way, with nothing taken
            // or made (LB11, LB13).
            Err(Refused::Failed(e)) => {
                self.overwrite_not_done();
                match crate::library::usable(&lib.dir) {
                    Err(gone) => self.library_save_unreachable(gone),
                    Ok(()) => self.status = format!("Couldn't save to the library: {e}"),
                }
            }
        }
    }

    /// Reload theirs (LB9): the open job becomes its record as saved now;
    /// this window's changes to it are dropped. The stock isn't touched: the
    /// job holds what their save holds.
    pub fn reload_theirs(&mut self) {
        let Some(c) = self.job_question() else { return };
        let j = match self.library().and_then(|l| l.load_job(c.id)) {
            Ok(j) => j,
            Err(e) => {
                self.status = format!("Couldn't reload {}: {e}", self.job_name);
                return;
            }
        };
        let cur = self.cur_job;
        let name = if self.jobs.iter().enumerate().any(|(k, o)| k != cur && o.name == j.name) { self.job_name.clone() } else { j.name.clone() };
        let place = self.lib_place(c.id);
        self.jobs[cur] = job_from_library(c.id, name, j, place);
        self.cur_job = usize::MAX;
        self.open_job_forced(cur);
        self.mark_job_saved(cur);
        self.lib_refresh();
        self.status = format!("Reloaded {} as {} saved it; your changes were dropped.", self.job_name, c.saved_by(&self.clock));
    }

    /// Save mine as a copy (LB9): the open job is saved as a new job under
    /// the first free name. Like Copy job, the copy takes its own sheets from
    /// stock and stores its own remnants; their job keeps its own.
    pub fn save_mine_as_copy(&mut self) {
        let Some(c) = self.job_question() else { return };
        let theirs = self.job_name.clone();
        if self.save_copy() {
            self.status = format!("{} {theirs} stays as {} saved it.", self.status, c.saved_by(&self.clock));
        }
    }

    /// Save the open job as a new job under the first free name (LB9; also
    /// for a job someone else has in use, CU9). Like Copy job, the copy takes
    /// its own sheets from stock and stores its own remnants. Saved or not.
    pub fn save_copy(&mut self) -> bool {
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("Couldn't save to the library: {e}");
                return false;
            }
        };
        let cur = self.cur_job;
        let open: Vec<String> = self.jobs.iter().enumerate().filter(|(k, _)| *k != cur).map(|(_, j)| j.name.to_lowercase()).collect();
        let theirs = self.job_name.clone();
        self.job_name = lib.free_name(Kind::Job, &theirs, |n| open.contains(&n.to_lowercase()));
        self.job_lib_id = None;
        self.lib_read.revision = 0;
        crate::engine::stockflow::forget_booking(&mut self.result.formats, &mut self.held);
        if let Some(e) = self.erp_job.as_mut() {
            e.work_order = None;
            e.sent = None;
        }
        self.save_job_to_library();
        self.job_lib_id.is_some()
    }

    /// Overwrite (LB9): the open job is saved over their save as it is now
    /// (asked again if someone saves in between). Their sheets go back to
    /// stock and their unused remnants come out; this job's are booked as
    /// new.
    pub fn overwrite_theirs(&mut self) {
        let Some(c) = self.job_question() else { return };
        self.libui.overwrite = Some(c);
        self.save_job_to_library();
    }

    /// The job question being answered, while the open job is still the job
    /// it's about (the window isn't modal: another job may be open by now).
    fn job_question(&mut self) -> Option<Changed> {
        match self.libui.question.take() {
            Some(SaveQuestion::Job(c)) if self.job_lib_id == Some(c.id) => {
                self.libui.overwrite = None;
                Some(c)
            }
            Some(q @ SaveQuestion::Part { .. }) => {
                self.libui.question = Some(q);
                None
            }
            _ => None,
        }
    }

    /// Someone else saved the open job since it was read: nothing was saved;
    /// ask (LB9).
    fn ask_about_job(&mut self, c: Changed) {
        self.libui.overwrite = None;
        self.status = format!("{} was saved by {} since you opened it — nothing was saved. Reload theirs, save yours as a copy, or overwrite theirs.", self.job_name, c.saved_by(&self.clock));
        self.libui.question = Some(SaveQuestion::Job(c));
    }

    /// Where a library part saved from the open job for the first time goes
    /// (JX19): the part folder named like the job's folder.
    pub fn parts_folder_of_job(&self) -> crate::folders::PartsFolder {
        let j = self.jobs.get(self.cur_job);
        crate::folders::PartsFolder::LikeJob { record: self.job_lib_id, folder: j.and_then(|j| j.lib_folder), week: j.map_or_else(String::new, |j| j.week.clone()) }
    }

    /// "in part folder Acme/2026-11", or "at the top of the part folders".
    fn part_place_words(&self, id: u64) -> String {
        let f = self.libui.part_folder(id);
        if f.is_empty() { "at the top of the part folders".into() } else { format!("in part folder {f}") }
    }

    /// Store the open job's products in the library (group = the job's
    /// folder), each new one in the part folder named like the job's folder
    /// (JX19).
    pub fn add_products_to_library(&mut self) {
        let folder = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        let to = self.parts_folder_of_job();
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let known: Vec<String> = lib.entries().iter().filter(|e| e.kind == Kind::Product).map(|e| e.name.clone()).collect();
        let (mut added, mut skipped, mut last) = (0, 0, None);
        for p in &self.parts {
            // A reference already stored is the same product (as in Lantek,
            // PK1): keep the stored one, where it is.
            if known.contains(&p.name) {
                skipped += 1;
                continue;
            }
            if let Ok(rec) = lib.save_product_in(p, &folder, self.spec.material, self.spec.thickness, SaveTo::New, &to) {
                // The job's part is that library part now, as saved (LB7).
                self.lib_read.parts.insert(p.name.clone(), rec);
                added += 1;
                last = Some(rec.id);
            }
        }
        self.lib_refresh();
        let place = last.map_or_else(String::new, |id| format!(" {}", self.part_place_words(id)));
        self.status = format!("Library: {added} product(s) added{place}{}.", if skipped > 0 { format!(", {skipped} already there") } else { String::new() });
    }

    /// Save one product (with its technology and texts) to the library,
    /// over the stored one with the same reference (its material and
    /// thickness become this job's, as one Lantek product has one of each).
    /// When that library part was saved by someone else since it was added
    /// to this job, or this job's part never came from it, nothing is saved
    /// and the window asks what to do (LB7, LB9).
    pub fn save_part_to_library(&mut self, i: usize) {
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("Couldn't save to the library: {e}");
                return;
            }
        };
        let Some(name) = self.parts.get(i).map(|p| p.name.clone()) else { return };
        let to = match lib.find_product(&name) {
            None => SaveTo::New,
            Some(id) => self.lib_read.parts.get(&name).filter(|r| r.id == id).map_or(SaveTo::Unread(id), |r| SaveTo::Read(*r)),
        };
        self.save_part_over(&lib, i, to);
    }

    /// Save the job's part `i` to the library (`to`), and take it as that
    /// library part as saved; or ask, when someone else saved it (LB7).
    fn save_part_over(&mut self, lib: &Library, i: usize, to: SaveTo) {
        let Some(p) = self.parts.get(i).cloned() else { return };
        let folder = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        // A new part goes into the part folder named like the job's folder,
        // in the same hold of the write lock as the revision check; saving
        // over one never moves it (JX19).
        let into = self.parts_folder_of_job();
        match lib.save_product_in(&p, &folder, self.spec.material, self.spec.thickness, to, &into) {
            Ok(rec) => {
                self.lib_read.parts.insert(p.name.clone(), rec);
                self.lib_refresh();
                let new = to == SaveTo::New;
                self.status = format!(
                    "{} {} as {}{}, {}{}.",
                    if new { "Saved" } else { "Updated" },
                    p.name,
                    crate::library::auto_ref(Kind::Product, rec.id),
                    if p.tech.is_some() { " with its technology" } else { "" },
                    if new { "" } else { "where it was: " },
                    self.part_place_words(rec.id)
                );
            }
            Err(Refused::Changed(c)) => {
                let added = !matches!(to, SaveTo::Unread(_));
                self.status = format!("{} — nothing was saved. Keep theirs, overwrite it, or save yours under a new name.", part_changed(&c, added, &self.clock));
                self.libui.question = Some(SaveQuestion::Part { part: i, changed: c, added });
            }
            Err(Refused::Failed(e)) => self.status = format!("Couldn't save to the library: {e}"),
            // Library parts have no in-use state (spec #44): never said.
            Err(r @ Refused::InUse(_)) => self.status = r.words(&self.clock),
        }
    }

    /// The part a part question is about, while the job still has it under
    /// that name.
    fn part_question(&mut self) -> Option<(usize, Changed)> {
        match self.libui.question.take() {
            Some(SaveQuestion::Part { part, changed, .. }) if self.parts.get(part).is_some_and(|p| p.name == changed.name) => Some((part, changed)),
            other => {
                self.libui.question = other.filter(|q| matches!(q, SaveQuestion::Job(_) | SaveQuestion::Theirs(_)));
                None
            }
        }
    }

    /// Keep theirs (LB9): nothing is saved, and the job's part stays as it
    /// is; saving it again asks again.
    pub fn keep_theirs(&mut self) {
        if let Some((_, c)) = self.part_question() {
            self.status = format!("Kept {} as {} saved it; yours wasn't saved.", c.name, c.saved_by(&self.clock));
        }
    }

    /// Overwrite (LB9): the job's part replaces theirs, as saved now (asked
    /// again if someone saves it in between).
    pub fn overwrite_part(&mut self) {
        let Some((i, c)) = self.part_question() else { return };
        match self.library() {
            Ok(lib) => self.save_part_over(&lib, i, SaveTo::Read(c.record())),
            Err(e) => self.status = format!("Couldn't save to the library: {e}"),
        }
    }

    /// Save mine under a new name (LB9): the job's part takes the first free
    /// name ("DEMO-P1 (1)") and is saved as a new library part; theirs stays.
    pub fn save_part_under_new_name(&mut self) {
        let Some((i, c)) = self.part_question() else { return };
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = format!("Couldn't save to the library: {e}");
                return;
            }
        };
        let names: Vec<String> = self.parts.iter().map(|p| p.name.to_lowercase()).collect();
        let name = lib.free_name(Kind::Product, &c.name, |n| names.contains(&n.to_lowercase()));
        self.checkpoint();
        self.parts[i].name = name;
        self.save_part_over(&lib, i, SaveTo::New);
    }

    /// Open the Library window on its products.
    pub fn open_library_products(&mut self) {
        self.libui.open = true;
        self.libui.tab = 1;
        self.lib_refresh();
    }

    /// Where library job `id` sits: its folder in the Library's job folders
    /// and the name of that folder, which its Open jobs folder takes; a copy
    /// of it saved into the Library goes into that folder too (JX15).
    fn lib_place(&mut self, id: u64) -> (u64, String) {
        if !self.libui.loaded {
            self.lib_refresh();
        }
        place_in(&self.libui.tree, id)
    }

    /// Open library job `id` now, whoever has it in use (asked before,
    /// `open_library_job`, CU8).
    pub fn open_library_job_now(&mut self, id: u64) {
        let j = match self.library().and_then(|l| l.load_job(id)) {
            Ok(j) => j,
            Err(e) => {
                self.status = format!("Couldn't open library job {id}: {e}");
                return;
            }
        };
        // Already open (linked to this record): just switch to it.
        self.store_job();
        if let Some(i) = self.jobs.iter().position(|x| x.lib_id == Some(id)) {
            self.open_job(i);
            self.status = format!("{} is already open — switched to it.", self.job_name);
            return;
        }
        let name = self.unique_job_name(&j.name).unwrap_or_else(|| j.name.clone());
        self.store_job();
        let place = self.lib_place(id);
        self.jobs.push(job_from_library(id, name, j, place));
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.refresh();
        self.mark_job_saved(self.cur_job);
        self.status = format!("Opened library job {id}: {}.", self.job_name);
    }

    pub fn add_library_product(&mut self, id: u64) {
        match self.library().and_then(|l| l.load_product(id)) {
            Ok(pf) => {
                let mut p = pf.part;
                p.color = self.color_seq_next();
                let name = p.name.clone();
                // Saved back later, it goes over that part as read here (LB7).
                // A part of that name already in the job keeps what it read:
                // taken as read now, saving it would go over a newer save
                // unasked.
                if !self.parts.iter().any(|q| q.name == name) {
                    self.lib_read.parts.insert(name.clone(), Record { id, revision: pf.revision });
                }
                self.parts.push(p);
                self.sel_part = Some(self.parts.len() - 1);
                self.stale = true;
                self.status = format!("Added {name} from the library — nest to place it.");
            }
            Err(e) => self.status = format!("Couldn't add library product {id}: {e}"),
        }
    }

    /// Ribbon group (Jobs screen): Library.
    pub fn ribbon_library(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Library", |ui| {
            if ribbon_button(ui, Icon::Library, "Library", self.libui.open, true).on_hover_text("This app's own product and job database — a folder you can share").clicked() {
                self.libui.open = !self.libui.open;
                if self.libui.open {
                    self.lib_refresh();
                }
            }
            let tip = match self.job_lib_id {
                Some(id) => format!("Save the open job over library job {id}"),
                None => "Save the open job into the library".into(),
            };
            if ribbon_button(ui, Icon::LibrarySave, "Save to\nlibrary", false, true).on_hover_text(tip).clicked() {
                self.save_job_to_library();
            }
        });
    }

    pub fn library_window(&mut self, ctx: &egui::Context) {
        if !self.libui.open {
            return;
        }
        if !self.libui.loaded {
            self.lib_refresh();
        }
        let mut open = true;
        // What a row or a part folder's menu asked for, and in which tab.
        let mut acted: Option<(Kind, TreeAct)> = None;
        let mut save_job = false;
        let mut open_file = false;
        let mut save_parts = false;
        let mut rescan = false;
        let mut settings = false;
        let mut retry = false;
        let dir = self.library_dir();
        let error = self.libui.error.clone();
        egui::Window::new(RichText::new("Library").size(16.0).strong()).id(egui::Id::unique("library_window")).pivot(egui::Align2::CENTER_TOP).default_pos(dialog_pos(ctx)).open(&mut open).default_size([980.0, 460.0]).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // The folder's own name (the full path on hover): where saved jobs go.
                let short = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().to_string());
                ui.label(RichText::new(format!("Library folder: {short}")).size(11.5).color(th().text_dim)).on_hover_text(dir.display().to_string());
                if error.is_some() {
                    return;
                }
                if ui.small_button("Folder…").on_hover_text("Settings → Folders: put the library on a shared drive for the whole shop").clicked() {
                    settings = true;
                }
                if ui.small_button("Rescan").on_hover_text("Rebuild the list from the record files").clicked() {
                    rescan = true;
                }
            });
            ui.add_space(4.0);
            // Unreachable, not empty (LB11): the message instead of the lists.
            if let Some(e) = &error {
                ui.add_space(12.0);
                ui.set_max_width(640.0);
                let (again, folder) = unreachable_note(ui, e);
                (retry, settings) = (again, settings || folder);
                ui.add_space(8.0);
                ui.label(RichText::new("Open jobs stay open. Save job keeps a copy of a job in a file on this PC until the Library can be reached again.").size(11.5).color(th().text_dim));
                return;
            }
            ui.horizontal(|ui| {
                let nj = self.libui.entries.iter().filter(|e| e.kind == Kind::Job).count();
                let np = self.libui.entries.len() - nj;
                for (k, n) in [format!("Jobs ({nj})"), format!("Products ({np})")].iter().enumerate() {
                    if ui.selectable_label(self.libui.tab == k, RichText::new(n).strong()).clicked() {
                        self.libui.tab = k;
                    }
                }
                ui.add_space(12.0);
                ui.add(egui::TextEdit::singleline(&mut self.libui.search).hint_text("🔍 Search name or folder").desired_width(240.0));
                ui.add_space(12.0);
                // An empty list offers this itself, in the middle of the window.
                let empty = !self.libui.entries.iter().any(|e| e.kind == if self.libui.tab == 0 { Kind::Job } else { Kind::Product });
                if !empty {
                    if self.libui.tab == 0 {
                        if ui.button("Save open job").clicked() {
                            save_job = true;
                        }
                    } else if ui.button("Add open job's products").clicked() {
                        save_parts = true;
                    }
                }
            });
            ui.separator();
            let kind = if self.libui.tab == 0 { Kind::Job } else { Kind::Product };
            let needle = self.libui.search.to_lowercase();
            let un = self.unit();
            if !self.libui.entries.iter().any(|e| e.kind == kind) {
                // Empty: say what the library is for and offer the next step.
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    let (msg, btn) = if kind == Kind::Job {
                        ("No jobs in the Library yet — it holds jobs saved with \"Save to library\", for every PC that shares this folder. Jobs saved with Save job are files: find them under Open job ▾ (recent jobs) or browse for one.", "Save open job to the Library")
                    } else {
                        ("No products yet. Products can be added to any job from here.", "Add open job's products")
                    };
                    ui.set_max_width(560.0);
                    ui.label(RichText::new(msg).color(th().text));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(btn).color(egui::Color32::WHITE)).fill(th().accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked() {
                            if kind == Kind::Job {
                                save_job = true;
                            } else {
                                save_parts = true;
                            }
                        }
                        if kind == Kind::Job && ui.add(egui::Button::new("Open a job file…").min_size(egui::Vec2::new(0.0, 30.0))).clicked() {
                            open_file = true;
                        }
                    });
                    ui.add_space(12.0);
                });
            }
            if kind == Kind::Product {
                // The Library's part folders (JX18, JX20): in each folder its
                // folders, then its parts; a search lists the matching parts
                // with their folders.
                let parts = self.lib_records(Kind::Product);
                let (cut, clock) = (self.libui.parts.cut, self.clock.clone());
                let row = move |ui: &mut egui::Ui, e: &Entry, path: Option<&str>, act: &mut Option<TreeAct>| {
                    lib_row(ui, e, path, None, None, &record_info(e, un), &e.saved_note(&clock), cut == Some(LibItem::Part(e.id)), act);
                };
                let look = TreeLook {
                    title: format!("📦 Library parts ({})", parts.len()),
                    tip: "Library parts, filed in the Library's part folders: the same on every PC that shares it. Right-click: New ▸ Folder, Paste, Refresh".into(),
                    empty: "📦 Part folders — none yet".into(),
                    empty_tip: "Library parts go into the part folder named like their job's folder. Right-click here for New ▸ Folder.".into(),
                    salt: "lib_parts_root",
                };
                egui::ScrollArea::vertical().id_salt("lib_parts").auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(a) = self.lib_tree(ui, Kind::Product, &parts, &needle, &look, &row) {
                        acted = Some((Kind::Product, a));
                    }
                });
                return;
            }
            // Jobs, newest first. A job's folder is its place in the
            // Library's job folders (JX10).
            let tree = &self.libui.tree;
            let folder = |e: &Entry| tree.path(tree.folder_of(e.id));
            let mut rows: Vec<&Entry> = self.libui.entries.iter().filter(|e| e.kind == Kind::Job && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || folder(e).to_lowercase().contains(&needle))).collect();
            rows.sort_by(|a, b| b.updated.cmp(&a.updated));
            if rows.is_empty() && self.libui.entries.iter().any(|e| e.kind == Kind::Job) {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new("Nothing matches the search.").color(th().text));
                });
            }
            // Jobs that share their name with another job: Libraries saved
            // before names were unique (JX17). Marked so one can be renamed.
            let twins = crate::library::shared_job_names(&self.libui.entries);
            let mut act = None;
            egui::ScrollArea::vertical().id_salt("lib_rows").auto_shrink([false, false]).show_rows(ui, 30.0, rows.len(), |ui, range| {
                for e in &rows[range] {
                    let same = twins.get(&e.id).map(|others| {
                        let which: Vec<String> = others
                            .iter()
                            .map(|o| {
                                // Its folder as the tree shows it (JX10).
                                let f = folder(o);
                                format!("{} '{}' ({})", crate::library::auto_ref(o.kind, o.id), o.name, if f.trim().is_empty() { "no folder".to_string() } else { format!("folder {f}") })
                            })
                            .collect();
                        format!(
                            "Another job in the Library has this name: {}.\nJob names are unique across the Library, whatever their capitals. These jobs were saved before that, so each still saves under its own name: open one, change its name and save it to the library.",
                            which.join(", ")
                        )
                    });
                    // Another PC has it in use (CU10).
                    let in_use = self.in_use_note(e.id);
                    lib_row(ui, e, Some(&folder(e)), same.as_deref(), in_use.as_deref(), &record_info(e, un), &e.saved_note(&self.clock), false, &mut act);
                }
            });
            if let Some(a) = act {
                acted = Some((Kind::Job, a));
            }
        });
        if !open {
            self.libui.open = false;
        }
        if settings {
            self.open_settings(crate::settings_ui::Section::Folders);
        }
        if retry {
            self.lib_retry();
        }
        if rescan {
            match self.library().and_then(|l| l.rebuild_index()) {
                Ok(n) => self.status = format!("Library rescanned: {n} record(s)."),
                Err(e) => self.status = e,
            }
            self.lib_refresh();
        }
        if save_job {
            self.save_job_to_library();
        }
        if open_file {
            self.open_job_dialog();
        }
        if save_parts {
            self.add_products_to_library();
        }
        match acted {
            Some((Kind::Job, TreeAct::Open(id))) => self.open_library_job(id),
            Some((Kind::Product, TreeAct::Open(id))) => self.add_library_product(id),
            // A job someone else has in use is refused, naming them (CU11).
            Some((k, TreeAct::Remove(id))) => {
                if self.delete_from_library(k, id) {
                    let v = self.libui.view(k);
                    if *v.cut == Some(LibItem::record(k, id)) {
                        *v.cut = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Delete a record from the Library (the Library window's 🗑). A job
    /// someone else has in use is refused, naming who and since when
    /// (CU11). Deleted or not.
    pub fn delete_from_library(&mut self, k: Kind, id: u64) -> bool {
        let done = match self.library().and_then(|l| l.delete(k, id)) {
            Err(e) => {
                self.status = e;
                false
            }
            Ok(()) => {
                if k == Kind::Job && self.job_lib_id == Some(id) {
                    self.job_lib_id = None;
                }
                true
            }
        };
        self.lib_refresh();
        done
    }

    /// The question a save asks when someone else saved the job or library
    /// part since it was read (LB9): who and when, and the choices.
    pub fn save_question_window(&mut self, ctx: &egui::Context) {
        let Some(q) = self.libui.question.clone() else { return };
        let (title, text, choices): (&str, String, Vec<(&str, &str)>) = match &q {
            SaveQuestion::Job(c) => (
                "Someone else saved this job",
                format!("{} was saved by {} since you opened it. Nothing was saved.", self.job_name, c.saved_by(&self.clock)),
                vec![
                    ("Reload theirs", "Open the job as they saved it. Your changes are dropped."),
                    ("Save mine as a copy", "Save yours as a new job under a free name. Theirs stays as they saved it."),
                    ("Overwrite", "Yours replaces theirs. Their sheets go back to stock; yours are taken."),
                ],
            ),
            SaveQuestion::Theirs(c) => (
                "Someone else saved a job you've changed",
                format!(
                    "{} was saved by {} since you opened it. Your changes to it here aren't saved.",
                    self.jobs.iter().find(|j| j.lib_id == Some(c.id)).map_or(c.name.as_str(), |j| j.name.as_str()),
                    c.saved_by(&self.clock)
                ),
                vec![
                    ("Reload theirs", "Open the job as they saved it. Your changes are dropped."),
                    ("Keep mine", "Keep working on yours. Saving it asks first, as theirs is newer."),
                    ("Save mine as a copy", "Save yours as a new job under a free name. Theirs stays as they saved it."),
                ],
            ),
            SaveQuestion::Part { changed: c, added, .. } => (
                "Someone else saved this library part",
                format!("{}. Nothing was saved.", part_changed(c, *added, &self.clock)),
                vec![
                    ("Keep theirs", "Save nothing. The part in this job stays as it is."),
                    ("Overwrite", "Yours replaces theirs in the library."),
                    ("Save mine under a new name", "This job's part takes a free name and is saved as a new library part."),
                ],
            ),
        };
        let mut pick: Option<usize> = None;
        let mut cancel = false;
        egui::Window::new(RichText::new(title).size(15.0).strong()).id(egui::Id::unique("save_question")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_width(520.0);
            egui::Frame::new().fill(th().warn_bg).corner_radius(4.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(th().warn_fg));
            });
            ui.add_space(8.0);
            for (k, (label, what)) in choices.iter().enumerate() {
                ui.horizontal(|ui| {
                    let b = egui::Button::new(RichText::new(*label).strong()).min_size(egui::vec2(200.0, 28.0));
                    if ui.add(b).clicked() {
                        pick = Some(k);
                    }
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(*what).size(12.0).color(th().text_dim)).wrap());
                });
                ui.add_space(2.0);
            }
            if matches!(q, SaveQuestion::Job(_)) {
                ui.add_space(4.0);
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 28.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").on_hover_text("Close this and keep working; nothing is saved").clicked() {
                        cancel = true;
                    }
                });
            }
        });
        match (&q, pick) {
            (SaveQuestion::Job(_), Some(0)) => self.reload_theirs(),
            (SaveQuestion::Job(_), Some(1)) => self.save_mine_as_copy(),
            (SaveQuestion::Job(_), Some(2)) => self.overwrite_theirs(),
            (SaveQuestion::Theirs(_), Some(0)) => self.refresh_reload_theirs(),
            (SaveQuestion::Theirs(_), Some(1)) => self.keep_mine(),
            (SaveQuestion::Theirs(_), Some(2)) => self.refresh_copy_mine(),
            (SaveQuestion::Part { .. }, Some(0)) => self.keep_theirs(),
            (SaveQuestion::Part { .. }, Some(1)) => self.overwrite_part(),
            (SaveQuestion::Part { .. }, Some(2)) => self.save_part_under_new_name(),
            _ if cancel => {
                self.libui.question = None;
                self.status = format!("Not saved: {} has someone else's newer save.", self.job_name);
            }
            _ => {}
        }
    }

    /// The library's jobs in the Job Explorer, in the Library's folder tree
    /// (libtree.rs, JX10–JX15).
    pub fn library_job_tree(&mut self, ui: &mut egui::Ui) {
        if !self.libui.loaded {
            self.lib_refresh();
        }
        let jobs: Vec<Entry> = self.libui.entries.iter().filter(|e| e.kind == Kind::Job).cloned().collect();
        let needle = self.job_search.to_lowercase();
        let mut open: Option<u64> = None;
        ui.add_space(6.0);
        // Job files opened or saved lately, next to the library's jobs, so a
        // job saved with Save job is found here too.
        let recent: Vec<String> = self.cfg.recent_jobs.clone();
        let mut open_file: Option<std::path::PathBuf> = None;
        if recent.is_empty() {
            ui.label(RichText::new("📂 Job files — none yet").color(th().text_dim)).on_hover_text("Jobs you save with Save job, or open from a file, are listed here");
        } else {
            egui::CollapsingHeader::new(RichText::new(format!("📂 Recent job files ({})", recent.len())).strong()).id_salt("recent_files_root").default_open(true).show(ui, |ui| {
                for r in &recent {
                    let p = std::path::PathBuf::from(r);
                    let name = p.file_stem().map_or(r.clone(), |n| n.to_string_lossy().to_string());
                    let ok = p.exists();
                    if ui.add_enabled(ok, egui::Button::new(name).frame(false)).on_hover_text(if ok { r.clone() } else { format!("{r}\n(moved or deleted)") }).clicked() {
                        open_file = Some(p);
                    }
                }
            });
        }
        if let Some(p) = open_file {
            self.open_job_file(&p);
        }
        ui.add_space(4.0);
        // Unreachable, not empty (LB11).
        if let Some(e) = self.libui.error.clone() {
            ui.label(RichText::new("📚 Saved jobs (Library) — can't be reached").color(th().text_dim));
            ui.add_space(2.0);
            let (again, folder) = unreachable_note(ui, &e);
            if again {
                self.lib_retry();
            }
            if folder {
                self.open_settings(crate::settings_ui::Section::Folders);
            }
            return;
        }
        if let Some(id) = self.library_folder_tree(ui, &jobs, &needle) {
            open = Some(id);
        }
        if let Some(id) = open {
            self.open_library_job(id);
        }
    }
}

/// What a library record is, for its row: its material, thickness and size,
/// and its products and sheets (a job) or its quantity (a part).
fn record_info(e: &Entry, un: crate::units::LenUnit) -> String {
    let mat = crate::cnc::MATERIALS.get(e.material).copied().unwrap_or("");
    if e.kind == Kind::Job {
        format!("{mat} {} · {} · {} products · {} sheets", un.thickness(e.thickness), un.size(e.w, e.h, 0), e.count_a, e.count_b)
    } else {
        format!("{mat} {} · {} · qty {}", un.thickness(e.thickness), un.size(e.w, e.h, 0), e.count_a)
    }
}

/// One record's row in the Library window: its reference, name and chips
/// ("same name", JX17, with `same_name` on hover; a job another PC has in
/// use, `in_use`, CU10), its folder's path when given (JX10, JX20), what it
/// is (`info`), then its buttons and who last
/// saved it (`saved`, LB6). A part's name has a menu: Add to job, Cut
/// (`cut`: shown cut), Delete from the library (JX20).
#[allow(clippy::too_many_arguments)]
fn lib_row(ui: &mut egui::Ui, e: &Entry, folder: Option<&str>, same_name: Option<&str>, in_use: Option<&str>, info: &str, saved: &str, cut: bool, act: &mut Option<TreeAct>) {
    let part = e.kind == Kind::Product;
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.label(RichText::new(crate::library::auto_ref(e.kind, e.id)).monospace().size(11.0).color(th().text_dim));
        let name = if cut { RichText::new(format!("✂ {}", e.name)).strong().color(th().text_dim) } else { RichText::new(&e.name).strong() };
        let r = ui.add(egui::Label::new(name).sense(egui::Sense::click()));
        if part {
            r.on_hover_text("Right-click: Add to job, Cut, Delete from the library").context_menu(|ui| {
                if ui.button("Add to job").clicked() {
                    *act = Some(TreeAct::Open(e.id));
                    ui.close();
                }
                if ui.button("Cut").on_hover_text("Then right-click a folder (or Library parts) and Paste to move the part there").clicked() {
                    *act = Some(TreeAct::Cut(LibItem::Part(e.id)));
                    ui.close();
                }
                if ui.button("Delete from the library").clicked() {
                    *act = Some(TreeAct::Remove(e.id));
                    ui.close();
                }
            });
        }
        if e.tech {
            chip(ui, "technology", th().accent_soft, th().accent_fg);
        }
        if let Some(words) = same_name {
            chip(ui, "same name", th().warn_bg, th().warn_fg).on_hover_text(words);
        }
        if let Some(note) = in_use {
            crate::inuseui::in_use_chip(ui, note);
        }
        if let Some(f) = folder.filter(|f| !f.is_empty()) {
            ui.label(RichText::new(f).size(11.5).color(th().accent_fg));
        }
        ui.label(RichText::new(info).size(11.5).color(th().text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("🗑").on_hover_text("Delete from the library").clicked() {
                *act = Some(TreeAct::Remove(e.id));
            }
            if !part {
                if ui.button("Open").clicked() {
                    *act = Some(TreeAct::Open(e.id));
                }
            } else if ui.button("Add to job").clicked() {
                *act = Some(TreeAct::Open(e.id));
            }
            // Who last saved it, and when (LB6).
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(saved).size(11.5).color(th().text_dim)).truncate());
        });
    });
}

impl App {
    /// Screenshots (`--save-question=job|part` with `--library-dir=` a new,
    /// empty folder): sam on PROG-1 saved library job DEMO-7, or library part
    /// DEMO-P1, after this window (kim on PROG-2) read it, and this window's
    /// save asks (LB9). Made-up users and PCs on a clock set to 9 Nov 09:20.
    pub fn demo_save_question(&mut self, what: &str) {
        let dir = self.library_dir();
        if self.cfg.library_dir.is_empty() || dir == crate::library::default_dir() || crate::library::holds_library(&dir) {
            self.status = "--save-question needs --library-dir= set to a new, empty folder".into();
            return;
        }
        let clock = crate::shared::Clock::at(1_794_216_000);
        self.clock = clock.clone();
        self.who = crate::shared::Who::new("kim", "PROG-2");
        let r = (|| -> Result<(), String> {
            // A new Library there, as Settings makes one (LB12).
            crate::library::create(&dir)?;
            let sam = Library::open(&dir, &crate::shared::Who::new("sam", "PROG-1"), &clock)?;
            let later = |m: u64| clock.advance(std::time::Duration::from_secs(m * 60));
            if what == "part" {
                let mut p = self.parts.first().cloned().ok_or("no part")?;
                p.name = "DEMO-P1".into();
                let rec = sam.save_product(&p, "Acme", self.spec.material, self.spec.thickness, SaveTo::New)?;
                self.add_library_product(rec.id);
                later(10);
                let pf = sam.load_product(rec.id)?;
                let mut q = pf.part.clone();
                q.qty += 1;
                sam.save_product(&q, "Acme", pf.material, pf.thickness, SaveTo::Read(Record { id: rec.id, revision: pf.revision }))?;
                later(5);
                let i = self.parts.len() - 1;
                self.parts[i].qty += 3;
                self.save_part_to_library(i);
            } else {
                let mut j = self.current_jobfile();
                j.name = "DEMO-7".into();
                j.week = "Acme".into();
                j.library = None;
                j.held.clear();
                let rec = sam.save_job(&j, SaveTo::New)?;
                self.open_library_job(rec.id);
                later(10);
                let mut theirs = sam.load_job(rec.id)?;
                if let Some(p) = theirs.parts.first_mut() {
                    p.qty += 5;
                }
                sam.save_job(&theirs, SaveTo::Read(theirs.library.ok_or("no record")?))?;
                later(5);
                if let Some(p) = self.parts.first_mut() {
                    p.qty += 2;
                }
                self.save_job_to_library();
            }
            Ok(())
        })();
        if let Err(e) = r {
            self.status = format!("--save-question: {e}");
        }
    }
}

/// "DEMO-P1 in the library was saved by sam on PROG-1, 9 Nov 09:30 since it
/// was added to this job" (LB7); for a part that wasn't added from it, that
/// this job's part isn't that one.
fn part_changed(c: &Changed, added: bool, clock: &crate::shared::Clock) -> String {
    let why = if added { "since it was added to this job" } else { "and this job's part of that name didn't come from it" };
    format!("{} in the library was saved by {} {why}", c.name, c.saved_by(clock))
}

/// Where library job `id` sits in the job folders `tree`: its folder, and
/// that folder's name, the Open jobs folder it sits in (JX15).
pub(crate) fn place_in(tree: &crate::folders::Tree, id: u64) -> (u64, String) {
    let folder = tree.folder_of(id);
    (folder, tree.get(folder).map_or_else(|| "Library".to_string(), |f| f.name.clone()))
}

/// An open job made from library job `id` as read (`j`), named `name`:
/// linked to its record and the revisions read (LB7).
/// `place`: its folder in the Library's job folders and that folder's name,
/// the Open jobs folder it sits in (JX15).
pub(crate) fn job_from_library(id: u64, name: String, j: crate::jobfile::JobFile, place: (u64, String)) -> crate::Job {
    let lib_read = LibRead { revision: j.library.map_or(0, |r| r.revision), parts: j.library_parts };
    let mut result = crate::nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried: 0 };
    result.normalise(&j.spec);
    let (folder, week) = place;
    crate::Job { week, name, parts: j.parts, spec: j.spec, result, stale: j.stale, lib_id: Some(id), cut_lines: j.cut_lines, manual_cuts: j.manual_cuts, cam_changed: None, held: j.held, erp: j.erp, file: None, saved: None, lib_read, lib_folder: Some(folder) }
}

#[cfg(test)]
pub(crate) mod libui_tests {
    use super::SaveQuestion;
    use crate::jobfile::JobFile;
    use crate::library::{self, Kind, Library};
    use crate::nest::{Part, Shape};
    use crate::shared::{Clock, Who};
    use crate::stock::{Item, Stock};
    use crate::ui_parts::shown_text;
    use crate::App;
    use eframe::egui;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// 2026-11-09 09:20 UTC: a test clock shows UTC.
    pub(crate) const NOV_9_0920: u64 = 1_794_216_000;

    /// A window on PC `pc`, working on the Library in `dir`. Its warn switch
    /// is off (CU1): these PCs hold nothing, so two of them can open one job
    /// and what's tested is the save check alone, which stays on whatever
    /// the holds say (LB7–LB10). With holds: `inuseui.rs`.
    pub(crate) fn window(dir: &Path, user: &str, pc: &str, clock: &Clock) -> App {
        let mut a = App::new(None);
        a.cfg.in_use_warn = false;
        a.cfg.library_dir = dir.display().to_string();
        a.who = Who::new(user, pc);
        a.clock = clock.clone();
        a
    }

    /// What the Library window shows of each record of `kind`: its name and
    /// who saved it, and when.
    pub(crate) fn listed(a: &App, kind: Kind) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = a.libui.entries.iter().filter(|e| e.kind == kind).map(|e| (e.name.clone(), e.saved_note(&a.clock))).collect();
        v.sort();
        v
    }

    /// A new, empty Library of the test's own (made as Settings would).
    pub(crate) fn new_library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_libui_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        library::create(&dir).unwrap();
        dir
    }

    /// A window that keeps sheet stock in the Library (remnants stored on
    /// save), with job DEMO-7 open: DEMO-P1 ×`qty` nested on sheets from
    /// stock.
    pub(crate) fn stock_window(dir: &std::path::Path, user: &str, pc: &str, clock: &Clock) -> App {
        let mut a = window(dir, user, pc, clock);
        a.cfg.stock = crate::stock::StockSettings { remnants_on_save: true, ..Default::default() };
        a.group_nest = false;
        a
    }

    /// PROG-1 makes DEMO-7 (DEMO-P1 ×20 on 10 mm steel) from a stock of twelve
    /// 3000 × 1500 sheets, and saves it to the Library.
    pub(crate) fn demo7(dir: &std::path::Path, clock: &Clock) -> App {
        let mut s = crate::stock::Stock::default();
        s.add_sheet(crate::stock::Item::sheet("S3000", 0, 10.0, 3000.0, 1500.0, 12), true).unwrap();
        s.save(dir).unwrap();
        let mut a = stock_window(dir, "sam", "PROG-1", clock);
        a.job_name = "DEMO-7".into();
        a.spec.material = 0;
        a.spec.thickness = 10.0;
        a.spec.w = 3000.0;
        a.spec.h = 1500.0;
        a.parts = vec![crate::nest::Part { name: "DEMO-P1".into(), shape: crate::nest::Shape::Rectangle, w: 600.0, h: 400.0, qty: 20, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.held.clear();
        a.cut_lines.clear();
        a.run_nesting();
        a.save_job_to_library();
        assert!(a.job_lib_id.is_some(), "{}", a.status);
        a
    }

    /// The stock file as it is now.
    pub(crate) fn stock_file(dir: &std::path::Path) -> Vec<u8> {
        std::fs::read(crate::stock::Stock::file(dir)).unwrap()
    }

    /// A remnant line across the last sheet, right of its parts.
    pub(crate) fn remnant_line(a: &mut App) {
        let last = a.result.sheets.len() - 1;
        let x = a.result.sheets[last].iter().map(|p| p.x + p.w).fold(0.0, f64::max) + 20.0;
        a.cut_lines.resize(last + 1, vec![]);
        a.cut_lines[last] = vec![crate::cutoff::CutLine { pts: vec![[x, 0.0], [x, 1500.0]], kind: crate::cutoff::CutKind::Remnant }];
    }

    /// Two PCs after a refused save (LB7): PROG-1 and PROG-2 opened library
    /// job DEMO-7; PROG-1 saved it at 09:30 with 25 parts and a remnant
    /// line; PROG-2's save at 09:35, with 40 parts and a remnant line, was
    /// refused.
    struct Refusal {
        dir: std::path::PathBuf,
        clock: Clock,
        prog1: App,
        prog2: App,
        /// DEMO-7's record.
        id: u64,
        /// The stock file and DEMO-7's record just before PROG-2 saved.
        stock: Vec<u8>,
        record: Vec<u8>,
    }

    fn refusal(tag: &str) -> Refusal {
        let dir = new_library(tag);
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let id = prog1.job_lib_id.unwrap();
        let mut prog2 = stock_window(&dir, "kim", "PROG-2", &clock);
        prog2.open_library_job(id);
        assert_eq!(prog2.job_name, "DEMO-7");
        clock.advance(Duration::from_secs(10 * 60));
        prog1.parts[0].qty = 25;
        prog1.run_nesting();
        remnant_line(&mut prog1);
        prog1.save_job_to_library();
        assert!(prog1.result.formats.iter().any(|f| !f.made.is_empty()), "PROG-1 stored a remnant: {}", prog1.status);
        let stock = stock_file(&dir);
        let record = std::fs::read(prog1.library().unwrap().path(Kind::Job, id)).unwrap();
        clock.advance(Duration::from_secs(5 * 60));
        prog2.parts[0].qty = 40;
        prog2.run_nesting();
        remnant_line(&mut prog2);
        prog2.save_job_to_library();
        Refusal { dir, clock, prog1, prog2, id, stock, record }
    }

    /// How many of DEMO-7's 3000 × 1500 sheets are left in stock, and the
    /// references of the remnants in stock.
    pub(crate) fn in_stock(dir: &std::path::Path) -> (i32, Vec<String>) {
        let s = crate::stock::Stock::load(dir).unwrap();
        (s.get("S3000").map_or(0, |i| i.qty), s.items.iter().filter(|i| i.kind == crate::stock::Kind::NestRemnant).map(|i| i.reference.clone()).collect())
    }

    /// The remnants a window's job stored.
    pub(crate) fn made(a: &App) -> Vec<String> {
        a.result.formats.iter().flat_map(|f| f.made.iter().cloned()).collect()
    }

    /// How many of DEMO-7's 3000 × 1500 sheets a window's job holds (its
    /// remnant sheets aside).
    pub(crate) fn holds(a: &App) -> i32 {
        a.held.get("S3000").copied().unwrap_or(0)
    }

    /// LB7, LB8: PROG-1 and PROG-2 open library job DEMO-7; PROG-1 saves;
    /// PROG-2's save is refused, naming PROG-1 and the time, and asks what
    /// to do. It took nothing from stock and stored no remnants.
    #[test]
    fn a_save_over_a_job_another_pc_saved_since_takes_nothing_from_stock() {
        let r = refusal("refused_save");
        assert!(r.prog2.status.contains("sam on PROG-1, 9 Nov 09:30"), "{}", r.prog2.status);
        let q = r.prog2.libui.question.as_ref().expect("PROG-2 is asked what to do");
        assert!(matches!(q, SaveQuestion::Job(c) if c.saved_by(&r.clock) == "sam on PROG-1, 9 Nov 09:30"), "{q:?}");
        assert_eq!(stock_file(&r.dir), r.stock, "PROG-2's refused save changed the stock");
        assert!(made(&r.prog2).is_empty(), "PROG-2's refused save stored remnants");
        assert_eq!(std::fs::read(r.prog1.library().unwrap().path(Kind::Job, r.id)).unwrap(), r.record, "PROG-2's refused save wrote the job");
        let _ = std::fs::remove_dir_all(&r.dir);
    }

    /// LB9 Reload theirs: PROG-2's job becomes the job as PROG-1 saved it,
    /// with nothing unsaved; the stock stays as it is; PROG-2's next save
    /// goes over the record.
    #[test]
    fn reload_theirs_takes_their_job_and_drops_mine() {
        let Refusal { dir, clock, prog1, mut prog2, id, stock, .. } = refusal("reload");
        prog2.reload_theirs();
        assert!(prog2.libui.question.is_none());
        assert_eq!((prog2.job_name.as_str(), prog2.parts[0].qty, prog2.job_lib_id), ("DEMO-7", 25, Some(id)));
        assert_eq!((&prog2.held, made(&prog2)), (&prog1.held, made(&prog1)), "it holds what PROG-1's save holds");
        assert!(!prog2.job_unsaved(prog2.cur_job));
        assert!(prog2.status.contains("sam on PROG-1"), "{}", prog2.status);
        assert_eq!(stock_file(&dir), stock);
        clock.advance(Duration::from_secs(60));
        prog2.parts[0].qty = 30;
        prog2.run_nesting();
        prog2.save_job_to_library();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(prog2.job_lib_id, Some(id));
        prog2.lib_refresh();
        assert_eq!(listed(&prog2, Kind::Job), vec![("DEMO-7".to_string(), "Saved by kim on PROG-2, 9 Nov 09:36".to_string())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9 Save mine as a copy: PROG-2's job is saved as a new job under the
    /// first free name, which takes its own sheets from stock and stores its
    /// own remnant; PROG-1's DEMO-7 stays as PROG-1 saved it, with its
    /// sheets and remnant.
    #[test]
    fn save_mine_as_a_copy_makes_a_new_job_with_its_own_sheets() {
        let Refusal { dir, prog1, mut prog2, id, record, .. } = refusal("copy");
        let (sheets, remnants) = in_stock(&dir);
        prog2.save_mine_as_copy();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!((prog2.job_name.as_str(), prog2.parts[0].qty), ("DEMO-7 (1)", 40));
        assert!(prog2.job_lib_id.is_some_and(|c| c != id), "{}", prog2.status);
        assert_eq!(std::fs::read(prog1.library().unwrap().path(Kind::Job, id)).unwrap(), record, "PROG-1's DEMO-7 changed");
        prog2.lib_refresh();
        let mut names: Vec<String> = listed(&prog2, Kind::Job).into_iter().map(|(n, by)| format!("{n}: {by}")).collect();
        names.sort();
        assert_eq!(names, ["DEMO-7 (1): Saved by kim on PROG-2, 9 Nov 09:35", "DEMO-7: Saved by sam on PROG-1, 9 Nov 09:30"]);
        // The copy took its own sheets and stored its own remnant.
        let (left, now) = in_stock(&dir);
        assert!(holds(&prog2) > 0);
        assert_eq!(left, sheets - holds(&prog2));
        assert_eq!(made(&prog2).len(), 1, "{}", prog2.status);
        assert!(remnants.iter().all(|r| now.contains(r)) && now.contains(&made(&prog2)[0]), "{remnants:?} → {now:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9 with JX16: a copy takes a name no other job in the Library holds,
    /// whatever its capitals. With "demo-7 (1)" saved in another folder,
    /// Save mine as a copy saves "DEMO-7 (2)".
    #[test]
    fn save_mine_as_a_copy_skips_a_name_another_job_holds() {
        let Refusal { dir, mut prog2, id, .. } = refusal("copy_taken");
        let taken = saved_elsewhere(&dir, "demo-7 (1)", "Acme");
        prog2.save_mine_as_copy();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(prog2.job_name, "DEMO-7 (2)", "{}", prog2.status);
        assert!(prog2.job_lib_id.is_some_and(|c| c != id && c != taken), "{}", prog2.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9 Overwrite: PROG-2's job replaces PROG-1's. PROG-1's sheets go back
    /// to stock and its unused remnant comes out; PROG-2's are booked. If
    /// someone saves again before it is saved, it asks again.
    #[test]
    fn overwrite_replaces_their_job_and_their_stock() {
        let Refusal { dir, clock, mut prog1, mut prog2, id, .. } = refusal("overwrite");
        // PROG-1 saves once more while PROG-2 is choosing: asked again.
        clock.advance(Duration::from_secs(60));
        prog1.parts[0].qty = 26;
        prog1.run_nesting();
        remnant_line(&mut prog1);
        prog1.save_job_to_library();
        prog2.overwrite_theirs();
        let q = prog2.libui.question.as_ref().expect("asked again");
        assert!(matches!(q, SaveQuestion::Job(c) if c.saved_by(&clock) == "sam on PROG-1, 9 Nov 09:36"), "{q:?}");
        let theirs = made(&prog1);
        assert!(!theirs.is_empty());
        let (sheets, _) = in_stock(&dir);
        let before = sheets + holds(&prog1);
        prog2.overwrite_theirs();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(prog2.job_lib_id, Some(id));
        let lib = prog2.library().unwrap();
        let rec = lib.load_job(id).unwrap();
        assert_eq!((rec.parts[0].qty, rec.saved.map(|s| s.by.label())), (40, Some("kim on PROG-2".to_string())));
        assert_eq!(lib.entries().iter().filter(|e| e.kind == Kind::Job).count(), 1);
        let (left, now) = in_stock(&dir);
        assert_eq!(left, before - holds(&prog2), "PROG-1's sheets went back, PROG-2's were taken");
        assert!(theirs.iter().all(|r| !now.contains(r)), "PROG-1's remnants are still in stock: {theirs:?} in {now:?}");
        assert_eq!(made(&prog2).len(), 1);
        assert!(now.contains(&made(&prog2)[0]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9, JX15: DEMO-7 sits in folder Acme/2026-11. Each answer to PROG-2's
    /// refused save leaves it there: Save mine as a copy files the copy next
    /// to it, Overwrite saves over it where it is, and a job reloaded from
    /// there saves over it where it is too.
    #[test]
    fn the_answers_keep_the_job_in_its_folder_and_file_the_copy_next_to_it() {
        use crate::folders::TOP;
        let dir = new_library("answer_folders");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let id = prog1.job_lib_id.unwrap();
        let acme = prog1.lib_new_folder(Kind::Job, TOP).unwrap();
        assert!(prog1.lib_rename_folder(Kind::Job, acme, "Acme"), "{}", prog1.status);
        let nov = prog1.lib_new_folder(Kind::Job, acme).unwrap();
        assert!(prog1.lib_rename_folder(Kind::Job, nov, "2026-11"), "{}", prog1.status);
        prog1.lib_cut(Kind::Job, super::LibItem::Job(id));
        assert!(prog1.lib_paste(Kind::Job, nov), "{}", prog1.status);
        let outline = |a: &mut App| {
            a.lib_refresh();
            // The jobs in Acme/2026-11 (the folder's own line left out).
            a.library_outline().into_iter().filter(|l| l.starts_with("Acme/2026-11/") && !l.ends_with('/')).collect::<Vec<_>>()
        };
        // PROG-2 opens it from there; PROG-1 saves it; PROG-2's save is refused.
        let mut prog2 = stock_window(&dir, "kim", "PROG-2", &clock);
        let refused = |prog1: &mut App, prog2: &mut App| {
            clock.advance(Duration::from_secs(60));
            prog1.parts[0].qty += 1;
            prog1.save_job_to_library();
            assert!(prog1.libui.question.is_none(), "{}", prog1.status);
            prog2.parts[0].qty += 2;
            prog2.save_job_to_library();
            assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "{}", prog2.status);
        };
        prog2.open_library_job(id);
        refused(&mut prog1, &mut prog2);
        // Save mine as a copy: next to theirs.
        prog2.save_mine_as_copy();
        assert_eq!(prog2.job_name, "DEMO-7 (1)", "{}", prog2.status);
        assert_eq!(outline(&mut prog2), ["Acme/2026-11/DEMO-7", "Acme/2026-11/DEMO-7 (1)"]);
        // Overwrite: over theirs, where it is.
        prog2.open_library_job(id);
        refused(&mut prog1, &mut prog2);
        prog2.overwrite_theirs();
        assert!(prog2.libui.question.is_none() && prog2.job_lib_id == Some(id), "{}", prog2.status);
        assert_eq!(outline(&mut prog2), ["Acme/2026-11/DEMO-7", "Acme/2026-11/DEMO-7 (1)"]);
        // Reload theirs (PROG-1, which still has DEMO-7 open): it sits in an
        // Open jobs folder named like its Library folder, and its next save
        // goes over it, where it is.
        refused(&mut prog2, &mut prog1);
        prog1.reload_theirs();
        assert_eq!((prog1.job_lib_id, prog1.jobs[prog1.cur_job].week.as_str()), (Some(id), "2026-11"));
        prog1.parts[0].qty += 1;
        prog1.save_job_to_library();
        assert!(prog1.libui.question.is_none() && prog1.job_lib_id == Some(id), "{}", prog1.status);
        assert_eq!(outline(&mut prog1), ["Acme/2026-11/DEMO-7", "Acme/2026-11/DEMO-7 (1)"]);
        assert_eq!(prog1.library().unwrap().entries().iter().filter(|e| e.kind == Kind::Job).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB5, LB6: two windows on two PCs, one Library: each saves a job, and
    /// both Library lists show each job with who saved it and when.
    #[test]
    fn two_pcs_saving_jobs_see_who_saved_each_and_when() {
        let dir = new_library("jobs");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.save_job_to_library();
        let first = prog1.job_name.clone();
        clock.advance(Duration::from_secs(25 * 60));
        prog2.open_job(1);
        prog2.save_job_to_library();
        let second = prog2.job_name.clone();
        assert_ne!(first, second);
        for a in [&mut prog1, &mut prog2] {
            a.lib_refresh();
            assert_eq!(
                listed(a, Kind::Job),
                {
                    let mut v = vec![(first.clone(), "Saved by sam on PROG-1, 9 Nov 09:20".to_string()), (second.clone(), "Saved by kim on PROG-2, 9 Nov 09:45".to_string())];
                    v.sort();
                    v
                }
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB5, LB6: the same for library parts, in the Library window and in
    /// Add product's library list.
    #[test]
    fn two_pcs_saving_library_parts_see_who_saved_each_and_when() {
        let dir = new_library("parts");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.save_part_to_library(0);
        let first = prog1.parts[0].name.clone();
        clock.advance(Duration::from_secs(10 * 60));
        prog2.open_job(0);
        prog2.parts[1].name = "DEMO-P2".into();
        prog2.save_part_to_library(1);
        let mut want = vec![(first.clone(), "Saved by sam on PROG-1, 9 Nov 09:20".to_string()), ("DEMO-P2".to_string(), "Saved by kim on PROG-2, 9 Nov 09:30".to_string())];
        want.sort();
        for a in [&mut prog1, &mut prog2] {
            a.lib_refresh();
            assert_eq!(listed(a, Kind::Product), want);
            let mut menu: Vec<(String, String)> = a.libui.products.iter().map(|(e, p)| (p.name.clone(), e.saved_note(&a.clock))).collect();
            menu.sort();
            assert_eq!(menu, want, "Add product's library list");
        }
        // PROG-2 saves its own part of that name over PROG-1's (asked first,
        // LB7): it's PROG-2's now.
        clock.advance(Duration::from_secs(60));
        prog2.save_part_to_library(0);
        prog2.overwrite_part();
        prog1.lib_refresh();
        assert!(listed(&prog1, Kind::Product).contains(&(first, "Saved by kim on PROG-2, 9 Nov 09:31".to_string())), "{:?}", listed(&prog1, Kind::Product));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The library part named `name`, as PROG-1 sees it: its width, and who
    /// saved it when.
    fn library_part(a: &App, name: &str) -> Option<(f64, String)> {
        let lib = a.library().unwrap();
        let e = lib.entries().into_iter().find(|e| e.kind == Kind::Product && e.name == name)?;
        Some((lib.load_product(e.id).unwrap().part.w, e.saved_note(&a.clock)))
    }

    /// LB7: a library part added to a job a second time, after someone saved
    /// it again, leaves the first one as it read it: saving that one asks,
    /// and their save stays.
    #[test]
    fn a_library_part_added_twice_keeps_what_the_first_one_read() {
        let dir = new_library("part_twice");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.parts[0].name = "DEMO-P1".into();
        prog1.save_part_to_library(0);
        let id = prog1.library().unwrap().find_product("DEMO-P1").unwrap();
        prog2.open_job(0);
        prog2.add_library_product(id);
        let first = prog2.parts.len() - 1;
        clock.advance(Duration::from_secs(10 * 60));
        prog1.parts[0].w += 10.0;
        prog1.save_part_to_library(0);
        let theirs = library_part(&prog1, "DEMO-P1");
        prog2.add_library_product(id);
        prog2.save_part_to_library(first);
        assert!(matches!(&prog2.libui.question, Some(SaveQuestion::Part { part, .. }) if *part == first), "{}", prog2.status);
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9: a part question is about the job it was asked in. Answered once
    /// another job is in front, it saves nothing: that job's part of the
    /// same name never replaces the library part, or takes a new name.
    #[test]
    fn a_part_question_answered_in_another_job_saves_nothing() {
        let dir = new_library("part_question_other_job");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.parts[0].name = "DEMO-P1".into();
        prog1.save_part_to_library(0);
        let theirs = library_part(&prog1, "DEMO-P1");
        prog2.open_job(1);
        prog2.parts[0].name = "DEMO-P1".into();
        prog2.parts[0].w += 70.0;
        prog2.open_job(0);
        prog2.parts[0].name = "DEMO-P1".into();
        prog2.save_part_to_library(0);
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Part { part: 0, .. })), "{}", prog2.status);
        prog2.open_job(1);
        prog2.overwrite_part();
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs, "the other job's part replaced the library part");
        prog2.save_part_under_new_name();
        assert_eq!((prog2.parts[0].name.as_str(), library_part(&prog1, "DEMO-P1 (1)")), ("DEMO-P1", None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB8, LB13: a Library job saved to a file on this PC leaves the stock
    /// alone: its sheets are booked when it's saved to the Library, after
    /// the revision check. The file is a copy holding none of its sheets, so
    /// opening and saving it can never give back a sheet the record holds.
    #[test]
    fn a_library_job_saved_to_a_file_leaves_the_stock_alone() {
        let dir = new_library("lib_job_to_file");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let (held, stock) = (holds(&prog1), in_stock(&dir));
        assert!(held > 1, "DEMO-7 holds {held} sheets");
        prog1.parts[0].qty = 5;
        prog1.run_nesting();
        let file = std::env::temp_dir().join(format!("nestpro_libui_lib_job_to_file_{}.snjob", std::process::id()));
        prog1.save_job_to(&file);
        assert_eq!(in_stock(&dir), stock, "saving to a file changed the stock: {}", prog1.status);
        let copy = crate::jobfile::load(&file).unwrap();
        assert!(copy.held.is_empty() && !copy.booked && copy.library.is_none(), "the file holds stock: {:?}", copy.held);
        // The open job still holds what its record holds; Save to library books the change.
        assert_eq!(holds(&prog1), held);
        prog1.save_job_to_library();
        assert_eq!(holds(&prog1), prog1.result.sheets.len() as i32, "{}", prog1.status);
        assert_eq!(in_stock(&dir).0, stock.0 + held - holds(&prog1));
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9: a part question goes with the job it was asked in, also when
    /// that job is deleted or closed: the next job's part of the same name
    /// never replaces the library part.
    #[test]
    fn a_part_question_dropped_with_its_deleted_job_saves_nothing() {
        let dir = new_library("part_question_deleted_job");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.parts[0].name = "DEMO-P1".into();
        prog1.save_part_to_library(0);
        let theirs = library_part(&prog1, "DEMO-P1");
        prog2.open_job(1);
        prog2.parts[0].name = "DEMO-P1".into();
        prog2.parts[0].w += 70.0;
        prog2.open_job(0);
        prog2.parts[0].name = "DEMO-P1".into();
        prog2.save_part_to_library(0);
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Part { part: 0, .. })), "{}", prog2.status);
        prog2.delete_job(0);
        assert_eq!(prog2.parts[0].name, "DEMO-P1", "the next job's part has the same name");
        prog2.overwrite_part();
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs, "the next job's part replaced the library part");
        prog2.save_part_under_new_name();
        assert_eq!((prog2.parts[0].name.as_str(), library_part(&prog1, "DEMO-P1 (1)")), ("DEMO-P1", None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB13: a Library job's file is a copy; the window doesn't follow it.
    /// An agent's change to the copy never reaches the Library job, so its
    /// next Save to library takes from stock only what the change needs, not
    /// its sheets again.
    #[test]
    fn an_agents_change_to_a_library_jobs_file_copy_takes_no_sheets_twice() {
        let dir = new_library("lib_job_file_agent");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let held = holds(&prog1);
        assert_eq!(12 - in_stock(&dir).0, held, "DEMO-7 took its sheets");
        let file = std::env::temp_dir().join(format!("nestpro_libui_lib_job_file_agent_{}.snjob", std::process::id()));
        prog1.save_job_to(&file);
        // An agent adds a part to the file (the CLI's way); the window looks.
        std::thread::sleep(Duration::from_millis(1100));
        let mut s = crate::engine::session::Session::new(crate::engine::Ctx::builtin());
        s.call("add-shape", Some(file.to_str().unwrap()), &serde_json::json!({"shape": "disc", "w": 150, "qty": 2, "name": "From agent"})).unwrap();
        prog1.disk_poll = -10.0;
        prog1.watch_job_file(&egui::Context::default());
        assert!(!prog1.parts.iter().any(|p| p.name == "From agent"), "the window followed the copy: {}", prog1.status);
        assert_eq!(holds(&prog1), held, "the job's booking was dropped");
        prog1.save_job_to_library();
        assert_eq!(12 - in_stock(&dir).0, holds(&prog1), "the sheets were taken twice: {}", prog1.status);
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB7: a job saved to a file and opened again still knows the library
    /// parts it read: saving one back goes over it without asking.
    #[test]
    fn a_job_file_remembers_the_library_parts_it_read() {
        let dir = new_library("part_in_file");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.parts[0].name = "DEMO-P1".into();
        prog1.save_part_to_library(0);
        let id = prog1.library().unwrap().find_product("DEMO-P1").unwrap();
        prog2.open_job(0);
        prog2.add_library_product(id);
        let path = dir.join("DEMO-8.snjob");
        prog2.save_job_to(&path);
        prog2.open_job_file(&path);
        let mine = prog2.parts.iter().position(|p| p.name == "DEMO-P1").unwrap();
        prog2.parts[mine].w += 50.0;
        prog2.save_part_to_library(mine);
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(library_part(&prog1, "DEMO-P1").map(|t| t.0), Some(prog2.parts[mine].w));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB13: Overwrite answered once the Library can't be reached changes
    /// nothing in the job, and the question stays.
    #[test]
    fn overwrite_once_the_library_cant_be_reached_changes_nothing() {
        let dir = new_library("overwrite_gone");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let id = prog1.job_lib_id.unwrap();
        let mut prog2 = stock_window(&dir, "kim", "PROG-2", &clock);
        prog2.open_library_job(id);
        clock.advance(Duration::from_secs(60));
        prog1.parts[0].qty += 1;
        prog1.save_job_to_library();
        prog2.parts[0].qty += 2;
        prog2.save_job_to_library();
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "{}", prog2.status);
        let job = serde_json::to_string(&prog2.current_jobfile()).unwrap();
        let away = dir.with_extension("away");
        std::fs::rename(&dir, &away).unwrap();
        prog2.overwrite_theirs();
        assert!(prog2.status.contains("can't be reached"), "{}", prog2.status);
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "the question went: {}", prog2.status);
        assert_eq!(serde_json::to_string(&prog2.current_jobfile()).unwrap(), job, "the refused Overwrite changed the job");
        let _ = std::fs::remove_dir_all(&away);
    }

    /// LB13: the share drops while Overwrite is saving, just after it found
    /// the Library: the job, its stock and the question stay as they were.
    #[test]
    fn overwrite_cut_off_while_saving_changes_nothing() {
        let dir = new_library("overwrite_cut_off");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let id = prog1.job_lib_id.unwrap();
        let mut prog2 = stock_window(&dir, "kim", "PROG-2", &clock);
        prog2.open_library_job(id);
        clock.advance(Duration::from_secs(60));
        prog1.parts[0].qty += 1;
        prog1.save_job_to_library();
        prog2.parts[0].qty += 2;
        prog2.save_job_to_library();
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "{}", prog2.status);
        let job = serde_json::to_string(&prog2.current_jobfile()).unwrap();
        let stock = std::fs::read_to_string(Stock::file(&dir)).unwrap();
        let away = dir.with_extension("away");
        {
            let (dir, away) = (dir.clone(), away.clone());
            crate::shared::hooks::set(move |s| {
                if s == "library reached" {
                    std::fs::rename(&dir, &away).unwrap();
                }
            });
        }
        prog2.overwrite_theirs();
        crate::shared::hooks::clear();
        assert!(prog2.status.contains("can't be reached"), "{}", prog2.status);
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "the question went: {}", prog2.status);
        assert!(prog2.libui.overwrite.is_none());
        assert_eq!(serde_json::to_string(&prog2.current_jobfile()).unwrap(), job, "the refused Overwrite changed the job");
        assert_eq!(std::fs::read_to_string(Stock::file(&away)).unwrap(), stock, "the refused Overwrite changed the stock");
        // Back again: a plain Save still asks, it doesn't overwrite unasked.
        std::fs::rename(&away, &dir).unwrap();
        prog2.libui.question = None;
        prog2.save_job_to_library();
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "{}", prog2.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9: Overwrite gave their sheets back and booked this job, then its
    /// record couldn't be stored (the Library's list was in the way). Saving
    /// again finishes it without asking: their sheets went back once and
    /// this job's were taken once.
    #[test]
    fn overwrite_whose_record_couldnt_be_stored_finishes_on_the_next_save() {
        let Refusal { dir, prog1, mut prog2, id, .. } = refusal("overwrite_store_fails");
        let (sheets, _) = in_stock(&dir);
        let before = sheets + holds(&prog1);
        {
            let index = dir.join(crate::library::INDEX);
            crate::shared::hooks::set(move |s| {
                if s == "record written" && index.is_file() {
                    std::fs::remove_file(&index).unwrap();
                    std::fs::create_dir_all(index.join("in the way")).unwrap();
                }
            });
        }
        prog2.overwrite_theirs();
        crate::shared::hooks::clear();
        assert!(prog2.status.contains("Couldn't save"), "{}", prog2.status);
        assert_eq!(prog2.library().unwrap().load_job(id).unwrap().parts[0].qty, 25, "their save was replaced");
        std::fs::remove_dir_all(dir.join(crate::library::INDEX)).unwrap();
        prog2.library().unwrap().rebuild_index().unwrap();
        prog2.save_job_to_library();
        assert!(prog2.libui.question.is_none(), "asked again: {}", prog2.status);
        assert_eq!(prog2.library().unwrap().load_job(id).unwrap().parts[0].qty, 40);
        let (left, _) = in_stock(&dir);
        assert_eq!(left, before - holds(&prog2), "PROG-1's sheets went back once, PROG-2's were taken once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB9 with Prompt for references: Overwrite books the job as a new one,
    /// so the remnants it had stored are named again before anything is
    /// saved.
    #[test]
    fn overwrite_asks_for_the_names_of_every_remnant_it_stores() {
        let dir = new_library("overwrite_names");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = demo7(&dir, &clock);
        let id = prog1.job_lib_id.unwrap();
        remnant_line(&mut prog1);
        prog1.save_job_to_library();
        assert!(!made(&prog1).is_empty(), "{}", prog1.status);
        let mut prog2 = stock_window(&dir, "kim", "PROG-2", &clock);
        prog2.open_library_job(id);
        assert_eq!(made(&prog2), made(&prog1));
        clock.advance(Duration::from_secs(60));
        prog1.save_job_to_library();
        prog2.save_job_to_library();
        assert!(matches!(prog2.libui.question, Some(SaveQuestion::Job(_))), "{}", prog2.status);
        prog2.cfg.stock.prompt_remnant_ref = true;
        let stock = stock_file(&dir);
        prog2.overwrite_theirs();
        let nm = prog2.naming.as_ref().expect("asked for the remnants' names");
        assert_eq!(nm.rows.len(), made(&prog2).len());
        assert_eq!(stock_file(&dir), stock, "something was saved before they were named");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB7, LB9: PROG-1 and PROG-2 added library part DEMO-P1 to their jobs;
    /// PROG-1 saved it again. PROG-2's Save to library of its DEMO-P1 asks,
    /// naming PROG-1 and the time, and saves nothing until answered: Keep
    /// theirs, Save mine under a new name, or Overwrite.
    #[test]
    fn saving_a_library_part_someone_saved_since_it_was_added_asks_first() {
        let dir = new_library("part_question");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.parts[0].name = "DEMO-P1".into();
        prog1.save_part_to_library(0);
        let id = prog1.library().unwrap().find_product("DEMO-P1").unwrap();
        prog2.open_job(0);
        prog2.add_library_product(id);
        let mine = prog2.parts.len() - 1;
        // PROG-1 saves it again (its own, as it last saved it: no question).
        clock.advance(Duration::from_secs(10 * 60));
        prog1.parts[0].w += 10.0;
        prog1.save_part_to_library(0);
        assert!(prog1.libui.question.is_none(), "{}", prog1.status);
        let theirs = library_part(&prog1, "DEMO-P1");
        assert_eq!(theirs.as_ref().map(|t| t.1.as_str()), Some("Saved by sam on PROG-1, 9 Nov 09:30"));
        // PROG-2 changes its copy and saves it: asked, nothing saved.
        clock.advance(Duration::from_secs(5 * 60));
        prog2.parts[mine].w += 50.0;
        prog2.save_part_to_library(mine);
        assert!(prog2.status.contains("sam on PROG-1, 9 Nov 09:30"), "{}", prog2.status);
        assert!(matches!(&prog2.libui.question, Some(SaveQuestion::Part { part, changed, added: true }) if *part == mine && changed.name == "DEMO-P1"), "{:?}", prog2.libui.question);
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs);
        // Keep theirs: nothing saved, the job's part as it was; asked again
        // on the next save.
        let kept = prog2.parts[mine].clone();
        prog2.keep_theirs();
        assert!(prog2.libui.question.is_none());
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs);
        assert_eq!((prog2.parts[mine].name.as_str(), prog2.parts[mine].w), (kept.name.as_str(), kept.w));
        prog2.save_part_to_library(mine);
        assert!(prog2.libui.question.is_some(), "{}", prog2.status);
        // Save mine under a new name: the job's part takes the first free
        // name and is saved as a new library part; theirs stays.
        prog2.save_part_under_new_name();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(prog2.parts[mine].name, "DEMO-P1 (1)");
        assert_eq!(library_part(&prog1, "DEMO-P1"), theirs);
        assert_eq!(library_part(&prog1, "DEMO-P1 (1)"), Some((kept.w, "Saved by kim on PROG-2, 9 Nov 09:35".to_string())));
        // Saved again, it goes over its own new record without asking.
        prog2.parts[mine].w += 5.0;
        prog2.save_part_to_library(mine);
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(library_part(&prog1, "DEMO-P1 (1)").map(|p| p.0), Some(kept.w + 5.0));
        // Overwrite: PROG-2 adds DEMO-P1 again, PROG-1 saves it again, PROG-2
        // saves its changed copy and overwrites theirs.
        prog2.add_library_product(id);
        let again = prog2.parts.len() - 1;
        clock.advance(Duration::from_secs(60));
        prog1.parts[0].w += 10.0;
        prog1.save_part_to_library(0);
        clock.advance(Duration::from_secs(60));
        prog2.parts[again].w = 777.0;
        prog2.save_part_to_library(again);
        assert!(prog2.status.contains("sam on PROG-1, 9 Nov 09:36"), "{}", prog2.status);
        prog2.overwrite_part();
        assert!(prog2.libui.question.is_none(), "{}", prog2.status);
        assert_eq!(library_part(&prog1, "DEMO-P1"), Some((777.0, "Saved by kim on PROG-2, 9 Nov 09:37".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB7: a part that wasn't added from the Library, saved under a name a
    /// library part already has, asks too: it would replace that part.
    #[test]
    fn saving_a_part_under_a_library_parts_name_asks_first() {
        let dir = new_library("part_unread");
        let clock = Clock::at(NOV_9_0920);
        let mut prog1 = window(&dir, "sam", "PROG-1", &clock);
        let mut prog2 = window(&dir, "kim", "PROG-2", &clock);
        prog1.open_job(0);
        prog1.save_part_to_library(0);
        let name = prog1.parts[0].name.clone();
        let theirs = library_part(&prog1, &name);
        prog2.open_job(0);
        assert_eq!(prog2.parts[0].name, name);
        prog2.parts[0].w += 50.0;
        prog2.save_part_to_library(0);
        assert!(matches!(&prog2.libui.question, Some(SaveQuestion::Part { part: 0, added: false, .. })), "{}", prog2.status);
        assert!(prog2.status.contains("sam on PROG-1, 9 Nov 09:20"), "{}", prog2.status);
        assert_eq!(library_part(&prog1, &name), theirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder of the test's own; the Library folder in it isn't made.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nestpro_unreachable_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The text a few frames of `f` leave on screen (the last frame's).
    fn shown(ctx: &egui::Context, mut f: impl FnMut(&egui::Context)) -> String {
        let mut text = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_pass(egui::RawInput::default(), |c| f(c));
            out.textures_delta.clear();
            text = shown_text(&out);
        }
        text
    }

    /// The text a few frames of `f` in a panel leave on screen.
    fn shown_in_panel(ctx: &egui::Context, mut f: impl FnMut(&mut egui::Ui)) -> String {
        let mut text = String::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| f(ui));
            out.textures_delta.clear();
            text = shown_text(&out);
        }
        text
    }

    /// LB11: with the Library folder missing, the Library window, the Job
    /// Explorer, Add product and the Sheet stock say so, naming the folder,
    /// instead of an empty list; no folder is made. Once it's back, Try
    /// again lists it.
    #[test]
    fn a_library_that_cant_be_reached_is_said_so_in_every_list_and_never_made() {
        let d = scratch("lists");
        let dir = d.join("Nesting");
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        let said = format!("The Library on {} can't be reached", dir.display());
        let ctx = egui::Context::default();
        a.libui.open = true;
        let window = shown(&ctx, |c| a.library_window(c));
        assert!(window.contains(&said), "Library window:\n{window}");
        assert!(!window.contains("No jobs in the Library yet"), "shown as empty:\n{window}");
        let explorer = shown_in_panel(&ctx, |ui| a.library_job_tree(ui));
        assert!(explorer.contains(&said), "Job Explorer:\n{explorer}");
        assert!(!explorer.contains("none yet"), "shown as empty:\n{explorer}");
        let add = shown_in_panel(&ctx, |ui| {
            crate::ui_parts::library_parts_section(&mut a, ui);
        });
        assert!(add.contains(&said), "Add product:\n{add}");
        assert!(!add.contains("No parts in the library yet"), "shown as empty:\n{add}");
        a.stockui.open = true;
        let stock = shown(&ctx, |c| a.stock_window(c));
        assert!(stock.contains(&said), "Sheet stock:\n{stock}");
        assert!(!stock.contains("Reference"), "the stock shown as an empty list:\n{stock}");
        // Settings → Folders and the Open job menu say so too, each looking
        // for the Library itself when nothing else has yet.
        for which in ["Settings", "Open job"] {
            let mut b = App::new(None);
            b.cfg.library_dir = dir.display().to_string();
            let text = shown_in_panel(&ctx, |ui| if which == "Settings" { b.settings_folders(ui) } else { b.open_job_menu(ui) });
            assert!(text.contains(&said) && text.contains("Try again"), "{which}:\n{text}");
        }
        assert!(!dir.exists(), "looking at the lists made the Library folder");
        // The share is back: Try again lists what's there.
        library::create(&dir).unwrap();
        Library::open(&dir, &a.who, &a.clock).unwrap().save_job(&crate::jobfile::JobFile::new_job("DEMO-001", a.spec.clone()), crate::library::SaveTo::New).unwrap();
        a.lib_retry();
        assert!(a.libui.error.is_none());
        assert!(a.libui.entries.iter().any(|e| e.kind == Kind::Job && e.name == "DEMO-001"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// LB12: picking a folder in Settings that holds no Library (new, empty,
    /// or not there yet) asks before a new Library is made there, and the
    /// Library stays where it was until the answer is yes. A folder that
    /// holds a Library is used without asking.
    #[test]
    fn picking_a_new_empty_folder_asks_before_making_a_library_there() {
        let d = scratch("pick");
        let shop = d.join("Shop");
        library::create(&shop).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = shop.display().to_string();
        let empty = d.join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        a.pick_library_folder(empty.display().to_string());
        assert_eq!(a.lib_new_ask.as_deref(), Some(empty.display().to_string().as_str()), "no question");
        assert_eq!(a.cfg.library_dir, shop.display().to_string(), "switched before the answer");
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0, "made a Library before the answer");
        let ctx = egui::Context::default();
        let q = shown(&ctx, |c| a.new_library_window(c));
        assert!(q.contains(&empty.display().to_string()) && q.contains("Make a new Library"), "the question:\n{q}");
        // No: nothing is made and the Library stays where it was.
        a.cancel_new_library();
        assert!(a.lib_new_ask.is_none());
        assert_eq!(a.cfg.library_dir, shop.display().to_string());
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
        // Yes: a new, empty Library there, and it's the Library from now on.
        a.pick_library_folder(empty.display().to_string());
        a.confirm_new_library();
        assert_eq!(a.cfg.library_dir, empty.display().to_string(), "{}", a.status);
        assert!(Library::open(&empty, &a.who, &a.clock).unwrap().entries().is_empty());
        // A folder that holds a Library is used without asking, and the
        // Sheet stock window shows its stock, not the last folder's.
        a.stockui.open = true;
        a.stockui.sel = Some("S1".into());
        let _ = shown(&ctx, |c| a.stock_window(c));
        crate::stock::Stock::update(&Library::at(&shop, &a.who, &a.clock), |s| s.add_sheet(crate::stock::Item::sheet("SHOP-SHEET", 0, 3.0, 3000.0, 1500.0, 1), true)).unwrap();
        a.pick_library_folder(shop.display().to_string());
        assert!(a.lib_new_ask.is_none());
        assert_eq!(a.cfg.library_dir, shop.display().to_string());
        assert!(a.stockui.sel.is_none(), "a sheet picked in the last Library is still selected");
        let _ = shown(&ctx, |c| a.stock_window(c));
        assert!(a.stockui.stock.get("SHOP-SHEET").is_some(), "the stock window still shows the last Library's stock");
        // A folder that isn't there is asked about too, and not made before.
        let new = d.join("New");
        a.pick_library_folder(new.display().to_string());
        assert_eq!(a.lib_new_ask.as_deref(), Some(new.display().to_string().as_str()));
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// LB11, LB13: Save to library with the folder missing is refused with
    /// the message; the open job is as it was, stays open, keeps its link to
    /// its Library record, and Save job still keeps a copy on this PC. The
    /// program never switches to another Library.
    #[test]
    fn save_to_library_is_refused_while_it_cant_be_reached_and_save_job_still_works() {
        let d = scratch("save");
        let dir = d.join("Nesting");
        library::create(&dir).unwrap();
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        a.open_job(0);
        a.save_job_to_library();
        let id = a.job_lib_id.expect("saved while the Library was there");
        a.parts[0].qty += 2;
        // The share goes away.
        std::fs::remove_dir_all(&dir).unwrap();
        let before = serde_json::to_string(&a.current_jobfile()).unwrap();
        let (jobs, cur) = (a.jobs.len(), a.cur_job);
        a.save_job_to_library();
        assert!(a.status.contains(&format!("The Library on {} can't be reached", dir.display())), "{}", a.status);
        assert_eq!(serde_json::to_string(&a.current_jobfile()).unwrap(), before, "the refused save changed the job");
        assert_eq!((a.jobs.len(), a.cur_job, a.job_lib_id), (jobs, cur, Some(id)), "the open job was closed or unlinked");
        assert!(a.job_unsaved(a.cur_job), "a refused save marked the job saved");
        assert_eq!(a.cfg.library_dir, dir.display().to_string(), "switched to another Library");
        assert!(!dir.exists(), "saving made the Library folder");
        // Save job keeps a copy on this PC.
        let file = d.join("DEMO-001.snjob");
        a.save_job_to(&file);
        assert_eq!(crate::jobfile::load(&file).unwrap().parts[0].qty, a.parts[0].qty, "{}", a.status);
        assert!(!dir.exists(), "saving the file made the Library folder");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `user`'s window on PC `pc`, working on the Library in `dir`, with the
    /// open job named `name`: one kind of part, 10 mm, nested from a stock
    /// of two 3000 × 1500 sheets ("S-1").
    fn app_on(dir: &Path, name: &str, user: &str, pc: &str, clock: &Clock) -> App {
        let mut a = window(dir, user, pc, clock);
        a.cfg.stock = crate::stock::StockSettings::default();
        a.job_name = name.into();
        a.spec.material = 0;
        a.spec.thickness = 10.0;
        a.spec.w = 3000.0;
        a.spec.h = 1500.0;
        a.parts = vec![Part { name: "DEMO-P1".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 4, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.group_nest = false;
        a.held.clear();
        a
    }

    fn stock_of_two(dir: &Path) {
        let mut s = Stock::default();
        s.add_sheet(Item::sheet("S-1", 0, 10.0, 3000.0, 1500.0, 2), true).unwrap();
        s.save(dir).unwrap();
    }

    /// A job kim saved on PROG-3 into the Library, in `folder`.
    fn saved_elsewhere(dir: &Path, name: &str, folder: &str) -> u64 {
        let mut j = JobFile::new_job(name, crate::nest::SheetSpec { w: 3000.0, h: 1500.0, margin: 5.0, gap: 5.0, material: 0, thickness: 10.0, level: 3, machine: Default::default(), kerf: Default::default() });
        j.week = folder.into();
        Library::open(dir, &Who::new("kim", "PROG-3"), &Clock::at(NOV_9_0920)).unwrap().save_job(&j, crate::library::SaveTo::New).unwrap().id
    }

    /// JX16: saving the open job into the Library under a name another job
    /// holds is refused before the job's sheets are taken from stock, so the
    /// refused save takes nothing.
    #[test]
    fn a_save_refused_for_its_name_takes_nothing_from_stock() {
        let dir = new_library("refused_stock");
        stock_of_two(&dir);
        saved_elsewhere(&dir, "DEMO-001 frames", "Acme");
        let mut a = app_on(&dir, "DEMO-001 FRAMES", "sam", "PROG-1", &Clock::at(NOV_9_0920));
        a.run_nesting();
        assert_eq!(a.result.formats.first().map(|f| f.stock_ref.as_str()), Some("S-1"), "{}", a.status);
        let before = std::fs::read_to_string(Stock::file(&dir)).unwrap();
        a.save_job_to_library();
        assert!(a.status.contains("folder Acme") && a.status.contains("Nothing was saved"), "{}", a.status);
        assert_eq!(a.job_lib_id, None);
        assert_eq!(std::fs::read_to_string(Stock::file(&dir)).unwrap(), before, "the refused save took sheets from stock");
        assert!(a.held.is_empty(), "the job counts sheets as taken: {:?}", a.held);
        // Under a name of its own it's saved, and takes its sheet.
        assert!(a.rename_job(a.cur_job, "DEMO-002 frames"), "{}", a.status);
        a.save_job_to_library();
        assert!(a.job_lib_id.is_some(), "{}", a.status);
        assert_eq!(Stock::load(&dir).unwrap().get("S-1").map(|s| s.qty), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// LB11, LB13 with JX16: the Library goes away after Save to library
    /// found it, before the write lock is taken for the name check. The save
    /// is still refused with the message naming the folder, the lists say
    /// so, nothing is taken from stock or stored, no folder is made, and the
    /// open job is as it was, unsaved.
    #[test]
    fn a_library_that_goes_away_during_save_to_library_is_said_so_and_takes_nothing() {
        let dir = new_library("gone_mid_save");
        stock_of_two(&dir);
        let mut a = app_on(&dir, "DEMO-001 frames", "sam", "PROG-1", &Clock::at(NOV_9_0920));
        a.run_nesting();
        assert_eq!(a.result.formats.first().map(|f| f.stock_ref.as_str()), Some("S-1"), "{}", a.status);
        let stock = std::fs::read_to_string(Stock::file(&dir)).unwrap();
        let job = serde_json::to_string(&a.current_jobfile()).unwrap();
        assert!(a.job_unsaved(a.cur_job));
        // The share drops (the folder is moved away) just after the save
        // found the Library.
        let away = dir.with_extension("away");
        {
            let (dir, away) = (dir.clone(), away.clone());
            crate::shared::hooks::set(move |s| {
                if s == "library reached" {
                    std::fs::rename(&dir, &away).unwrap();
                }
            });
        }
        a.save_job_to_library();
        crate::shared::hooks::clear();
        let said = format!("The Library on {} can't be reached", dir.display());
        assert!(a.status.contains(&said) && a.status.contains("Save job keeps a copy"), "{}", a.status);
        assert!(a.libui.error.as_ref().is_some_and(|e| e.contains(&said)), "the lists don't say so: {:?}", a.libui.error);
        assert_eq!(serde_json::to_string(&a.current_jobfile()).unwrap(), job, "the refused save changed the job");
        assert!(a.held.is_empty(), "the job counts sheets as taken: {:?}", a.held);
        assert_eq!(a.job_lib_id, None);
        assert!(a.job_unsaved(a.cur_job), "a refused save marked the job saved");
        assert!(!dir.exists(), "saving made the Library folder again");
        assert_eq!(std::fs::read_to_string(Stock::file(&away)).unwrap(), stock, "the refused save took sheets from stock");
        assert!(Library::open(&away, &a.who, &a.clock).unwrap().entries().is_empty(), "a job was stored");
        let _ = std::fs::remove_dir_all(&away);
    }

    /// JX16 with two PCs: both save a new job under one name (in other
    /// capitals) at the same moment. PROG-1 checks the name first and is
    /// held up just after; PROG-2 saves meanwhile. One job is stored, the
    /// other save is refused naming the first one's folder, and only the
    /// stored one took a sheet from stock.
    #[test]
    fn two_pcs_saving_one_new_name_at_the_same_moment_store_one_and_refuse_the_other() {
        use std::sync::mpsc::channel;
        let dir = new_library("same_moment");
        stock_of_two(&dir);
        let clock = Clock::at(NOV_9_0920);
        let (checked, checked_rx) = channel::<()>();
        let prog1 = {
            let (dir, clock) = (dir.clone(), clock.clone());
            std::thread::spawn(move || {
                let mut a = app_on(&dir, "DEMO-001 frames", "sam", "PROG-1", &clock);
                a.jobs[a.cur_job].week = "Acme".into();
                a.run_nesting();
                let mut first = true;
                crate::shared::hooks::set(move |s| {
                    if s == "job name checked" && std::mem::take(&mut first) {
                        let _ = checked.send(());
                        std::thread::sleep(Duration::from_millis(500));
                    }
                });
                a.save_job_to_library();
                crate::shared::hooks::clear();
                (a.job_lib_id, a.status)
            })
        };
        let prog2 = {
            let (dir, clock) = (dir.clone(), clock.clone());
            std::thread::spawn(move || {
                let mut a = app_on(&dir, "demo-001 FRAMES", "kim", "PROG-2", &clock);
                a.jobs[a.cur_job].week = "W12".into();
                a.run_nesting();
                checked_rx.recv_timeout(Duration::from_secs(60)).expect("PROG-1 checked the name");
                a.save_job_to_library();
                (a.job_lib_id, a.status)
            })
        };
        let (one, two) = (prog1.join().unwrap(), prog2.join().unwrap());
        assert!(one.0.is_some(), "PROG-1: {}", one.1);
        assert!(two.0.is_none() && two.1.contains("DEMO-001 frames") && two.1.contains("folder Acme"), "PROG-2: {}", two.1);
        // The one stored is PROG-1's, signed by it (LB5).
        let lib = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap();
        let jobs: Vec<(String, Option<Who>)> = lib.entries().into_iter().filter(|e| e.kind == Kind::Job).map(|e| (e.name, e.saved_by)).collect();
        assert_eq!(jobs, [("DEMO-001 frames".to_string(), Some(Who::new("sam", "PROG-1")))]);
        assert_eq!(Stock::load(&dir).unwrap().get("S-1").map(|s| s.qty), Some(1), "the refused save took a sheet too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// JX16: renaming a Library job in the Job Explorer to a name another
    /// job in the Library holds is refused at once, naming that job's
    /// folder. A free name is taken, and the record takes it on the next
    /// save.
    #[test]
    fn renaming_a_library_job_to_a_name_another_job_holds_is_refused() {
        let dir = new_library("rename");
        saved_elsewhere(&dir, "DEMO-001 frames", "Acme");
        let clock = Clock::at(NOV_9_0920);
        let mut a = app_on(&dir, "DEMO-002 brackets", "sam", "PROG-1", &clock);
        a.save_job_to_library();
        let id = a.job_lib_id.expect("saved");
        let i = a.cur_job;
        assert!(!a.rename_job(i, "Demo-001 Frames"));
        assert!(a.status.contains("DEMO-001 frames") && a.status.contains("folder Acme") && !a.status.contains("Nothing was saved"), "{}", a.status);
        assert_eq!(a.job_name, "DEMO-002 brackets");
        assert!(a.rename_job(i, "DEMO-003 brackets"), "{}", a.status);
        a.save_job_to_library();
        assert_eq!(a.job_lib_id, Some(id), "{}", a.status);
        let mut names: Vec<String> = Library::open(&dir, &Who::new("sam", "PROG-1"), &clock).unwrap().entries().into_iter().map(|e| e.name).collect();
        names.sort();
        assert_eq!(names, ["DEMO-001 frames", "DEMO-003 brackets"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
