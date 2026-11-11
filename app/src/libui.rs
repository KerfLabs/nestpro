//! The Library window and actions: save the open job or its products to the
//! app's own library (library.rs), browse and search it, open jobs and add
//! stored products to the open job.

use crate::library::{Entry, Kind, Library};
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
    /// Why the Library can't be listed (LB11, LB12): its folder can't be
    /// reached or holds no Library. The lists say this instead of looking
    /// empty.
    pub error: Option<String>,
}

/// A job or folder in the Library's tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibItem {
    /// A saved job, by its record number.
    Job(u64),
    Folder(u64),
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
    pub fn library(&self) -> Result<Library, String> {
        Library::open(&self.library_dir(), &self.who, &self.clock)
    }

    pub fn lib_refresh(&mut self) {
        match self.library() {
            Ok(l) => {
                self.libui.entries = l.entries();
                self.lib_reload_tree(&l);
                self.libui.products = self.libui.entries.iter().filter(|e| e.kind == Kind::Product).filter_map(|e| l.load_product(e.id).ok().map(|pf| (e.clone(), pf.part))).collect();
                self.libui.products.sort_by(|a, b| b.0.updated.cmp(&a.0.updated));
                self.libui.error = None;
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
        }
    }

    /// Another library folder: the open jobs' record numbers belong to the old
    /// one, so saving would replace whatever has that number in the new one.
    /// They become new records there.
    pub fn library_folder_changed(&mut self) {
        self.libui.loaded = false;
        // The Sheet stock window lists the new Library's stock (LB12).
        self.stockui.loaded = false;
        self.stockui.sel = None;
        self.job_lib_id = None;
        for j in &mut self.jobs {
            j.lib_id = None;
            // Its folder's number is the old Library's too (JX15).
            j.lib_folder = None;
        }
        // A cut, or a name being typed, belongs to the old Library's tree.
        self.libui.cut = None;
        self.libui.edit = None;
        self.libui.reveal.clear();
    }

    /// Save to library refused because the Library can't be reached (LB11):
    /// the lists say so, and the open job stays as it is, unsaved, and open
    /// (LB13).
    fn library_save_unreachable(&mut self, e: String) {
        self.libui.error = Some(e.clone());
        self.status = format!("Couldn't save to the library: {e} Nothing was saved to it; Save job keeps a copy in a file on this PC.");
    }

    /// Save the open job into the library (over its record if it came from there).
    pub fn save_job_to_library(&mut self) {
        // A Library that can't be reached refuses the save before anything
        // else happens (LB11, LB13).
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => return self.library_save_unreachable(e),
        };
        if !self.ready_to_save(crate::stockui::SaveTarget::Library) {
            return;
        }
        crate::shared::pause("library reached");
        // A name another job holds is refused before any sheet is taken from
        // stock (JX16), and the check, the stock and the save are one change
        // under the Library's write lock (LB1), so no other PC can save the
        // name in between.
        let lock = match lib.lock() {
            Ok(l) => l,
            // Gone since it was found: said the same way, with nothing taken
            // or made (LB11, LB13).
            Err(e) => match crate::library::usable(&lib.dir) {
                Err(gone) => return self.library_save_unreachable(gone),
                Ok(()) => {
                    self.status = format!("Couldn't save to the library: {e}.");
                    return;
                }
            },
        };
        if let Err(e) = lib.check_job_name(&self.job_name, self.job_lib_id) {
            self.status = format!("Couldn't save to the library: {e}. Nothing was saved.");
            return;
        }
        let stock_note = self.stock_on_save();
        let j = self.current_jobfile();
        // A new record goes where the library job it came from is (JX15).
        let folder = self.jobs.get(self.cur_job).and_then(|j| j.lib_folder);
        let r = lib.save_job_in(&j, self.job_lib_id, folder);
        // Let other PCs write while the lists are read again (LB2).
        drop(lock);
        self.status = match r {
            Ok(id) => {
                self.job_lib_id = Some(id);
                self.mark_job_saved(self.cur_job);
                self.lib_refresh();
                let f = self.libui.tree.folder_of(id);
                if let Some(j) = self.jobs.get_mut(self.cur_job) {
                    j.lib_folder = Some(f);
                }
                format!("Saved job {} to the library as {} ({}).{stock_note}", self.job_name, crate::library::auto_ref(Kind::Job, id), self.library_dir().display())
            }
            Err(e) => format!("Couldn't save to the library: {e}"),
        };
    }

    /// Store the open job's products in the library (group = the job's folder).
    pub fn add_products_to_library(&mut self) {
        let folder = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        let lib = match self.library() {
            Ok(l) => l,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let known: Vec<String> = lib.entries().iter().filter(|e| e.kind == Kind::Product).map(|e| e.name.clone()).collect();
        let (mut added, mut skipped) = (0, 0);
        for p in &self.parts {
            // A reference already stored is the same product (as in Lantek,
            // PK1): keep the stored one.
            if known.contains(&p.name) {
                skipped += 1;
                continue;
            }
            if lib.save_product(p, &folder, self.spec.material, self.spec.thickness, None).is_ok() {
                added += 1;
            }
        }
        self.lib_refresh();
        self.status = format!("Library: {added} product(s) added{}.", if skipped > 0 { format!(", {skipped} already there") } else { String::new() });
    }

    /// Save one product (with its technology and texts) to the library,
    /// over the stored one with the same reference (its material and
    /// thickness become this job's, as one Lantek product has one of each).
    pub fn save_part_to_library(&mut self, i: usize) {
        let Some(p) = self.parts.get(i).cloned() else { return };
        let folder = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        let r = self.library().and_then(|l| {
            let id = l.find_product(&p.name);
            l.save_product(&p, &folder, self.spec.material, self.spec.thickness, id).map(|n| (n, id.is_some()))
        });
        self.status = match r {
            Ok((n, again)) => {
                self.lib_refresh();
                format!(
                    "{} {} as {}{}.",
                    if again { "Updated" } else { "Saved" },
                    p.name,
                    crate::library::auto_ref(Kind::Product, n),
                    if p.tech.is_some() { " with its technology" } else { "" }
                )
            }
            Err(e) => format!("Couldn't save to the library: {e}"),
        };
    }

    /// Open the Library window on its products.
    pub fn open_library_products(&mut self) {
        self.libui.open = true;
        self.libui.tab = 1;
        self.lib_refresh();
    }

    pub fn open_library_job(&mut self, id: u64) {
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
        // In Open jobs it sits in a folder named like its Library folder, and
        // a copy of it is saved into that Library folder (JX15).
        if !self.libui.loaded {
            self.lib_refresh();
        }
        let folder = self.libui.tree.folder_of(id);
        let week = self.libui.tree.get(folder).map_or_else(|| "Library".to_string(), |f| f.name.clone());
        self.jobs.push(crate::Job { week, name, parts: j.parts, spec: j.spec.clone(), result: { let mut r = crate::nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried: 0 }; r.normalise(&j.spec); r }, stale: j.stale, lib_id: Some(id), cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: j.held.clone(), erp: None, file: None, saved: None, lib_folder: Some(folder) });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.stale = j.stale;
        self.erp_job = j.erp;
        self.job_lib_id = Some(id);
        self.job_path = None;
        self.job_link = None;
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
        let mut open_job: Option<u64> = None;
        let mut add_part: Option<u64> = None;
        let mut delete: Option<(Kind, u64)> = None;
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
            // A job's folder is its place in the Library's job folders (JX10).
            let tree = &self.libui.tree;
            let folder = |e: &Entry| if e.kind == Kind::Job { tree.path(tree.folder_of(e.id)) } else { e.folder.clone() };
            let mut rows: Vec<&Entry> = self.libui.entries.iter().filter(|e| e.kind == kind && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || folder(e).to_lowercase().contains(&needle))).collect();
            rows.sort_by(|a, b| b.updated.cmp(&a.updated));
            // Jobs that share their name with another job: Libraries saved
            // before names were unique (JX17). Marked so one can be renamed.
            let twins = crate::library::shared_job_names(&self.libui.entries);
            if rows.is_empty() {
                // Empty: say what the library is for and offer the next step.
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    let (msg, btn) = match (self.libui.entries.iter().any(|e| e.kind == kind), self.libui.tab) {
                        (true, _) => ("Nothing matches the search.", None),
                        (false, 0) => ("No jobs in the Library yet — it holds jobs saved with \"Save to library\", for every PC that shares this folder. Jobs saved with Save job are files: find them under Open job ▾ (recent jobs) or browse for one.", Some("Save open job to the Library")),
                        _ => ("No products yet. Products can be added to any job from here.", Some("Add open job's products")),
                    };
                    ui.set_max_width(560.0);
                    ui.label(RichText::new(msg).color(th().text));
                    if let Some(b) = btn {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new(b).color(egui::Color32::WHITE)).fill(th().accent).min_size(egui::Vec2::new(0.0, 30.0))).clicked() {
                                if self.libui.tab == 0 {
                                    save_job = true;
                                } else {
                                    save_parts = true;
                                }
                            }
                            if self.libui.tab == 0 && ui.add(egui::Button::new("Open a job file…").min_size(egui::Vec2::new(0.0, 30.0))).clicked() {
                                open_file = true;
                            }
                        });
                    }
                });
            }
            let mats = crate::cnc::MATERIALS;
            let un = self.unit();
            egui::ScrollArea::vertical().id_salt("lib_rows").auto_shrink([false, false]).show_rows(ui, 30.0, rows.len(), |ui, range| {
                for e in &rows[range] {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        ui.label(RichText::new(crate::library::auto_ref(e.kind, e.id)).monospace().size(11.0).color(th().text_dim));
                        ui.label(RichText::new(&e.name).strong());
                        if e.tech {
                            chip(ui, "technology", th().accent_soft, th().accent_fg);
                        }
                        if let Some(others) = twins.get(&e.id).filter(|_| e.kind == Kind::Job) {
                            let which: Vec<String> = others.iter().map(|o| format!("{} '{}' ({})", crate::library::auto_ref(o.kind, o.id), o.name, {
                                // Its folder as the tree shows it (JX10).
                                let f = folder(o);
                                if f.trim().is_empty() { "no folder".to_string() } else { format!("folder {f}") }
                            })).collect();
                            chip(ui, "same name", th().warn_bg, th().warn_fg).on_hover_text(format!(
                                "Another job in the Library has this name: {}.\nJob names are unique across the Library, whatever their capitals. These jobs were saved before that, so each still saves under its own name: open one, change its name and save it to the library.",
                                which.join(", ")
                            ));
                        }
                        ui.label(RichText::new(folder(e)).size(11.5).color(th().accent_fg));
                        let mat = mats.get(e.material).copied().unwrap_or("");
                        let info = if e.kind == Kind::Job { format!("{mat} {} · {} · {} products · {} sheets", un.thickness(e.thickness), un.size(e.w, e.h, 0), e.count_a, e.count_b) } else { format!("{mat} {} · {} · qty {}", un.thickness(e.thickness), un.size(e.w, e.h, 0), e.count_a) };
                        ui.label(RichText::new(info).size(11.5).color(th().text_dim));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Delete from the library").clicked() {
                                delete = Some((e.kind, e.id));
                            }
                            if e.kind == Kind::Job {
                                if ui.button("Open").clicked() {
                                    open_job = Some(e.id);
                                }
                            } else if ui.button("Add to job").clicked() {
                                add_part = Some(e.id);
                            }
                            // Who last saved it, and when (LB6).
                            ui.add_space(6.0);
                            ui.add(egui::Label::new(RichText::new(e.saved_note(&self.clock)).size(11.5).color(th().text_dim)).truncate());
                        });
                    });
                }
            });
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
        if let Some(id) = open_job {
            self.open_library_job(id);
        }
        if let Some(id) = add_part {
            self.add_library_product(id);
        }
        if let Some((k, id)) = delete {
            if let Ok(l) = self.library() {
                let _ = l.delete(k, id);
            }
            if k == Kind::Job && self.job_lib_id == Some(id) {
                self.job_lib_id = None;
            }
            self.lib_refresh();
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

#[cfg(test)]
mod libui_tests {
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
    const NOV_9_0920: u64 = 1_794_216_000;

    /// A window on PC `pc`, working on the Library in `dir`.
    fn window(dir: &Path, user: &str, pc: &str, clock: &Clock) -> App {
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        a.who = Who::new(user, pc);
        a.clock = clock.clone();
        a
    }

    /// What the Library window shows of each record of `kind`: its name and
    /// who saved it, and when.
    fn listed(a: &App, kind: Kind) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = a.libui.entries.iter().filter(|e| e.kind == kind).map(|e| (e.name.clone(), e.saved_note(&a.clock))).collect();
        v.sort();
        v
    }

    /// A new, empty Library of the test's own (made as Settings would).
    fn new_library(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestpro_libui_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        library::create(&dir).unwrap();
        dir
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
        // PROG-2 saves PROG-1's part again: it's PROG-2's now.
        clock.advance(Duration::from_secs(60));
        prog2.save_part_to_library(0);
        prog1.lib_refresh();
        assert!(listed(&prog1, Kind::Product).contains(&(first, "Saved by kim on PROG-2, 9 Nov 09:31".to_string())), "{:?}", listed(&prog1, Kind::Product));
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
        Library::open(&dir, &a.who, &a.clock).unwrap().save_job(&crate::jobfile::JobFile::new_job("DEMO-001", a.spec.clone()), None).unwrap();
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
        crate::stock::Stock::update(&shop, |s| s.add_sheet(crate::stock::Item::sheet("SHOP-SHEET", 0, 3.0, 3000.0, 1500.0, 1), true)).unwrap();
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
        Library::open(dir, &Who::new("kim", "PROG-3"), &Clock::at(NOV_9_0920)).unwrap().save_job(&j, None).unwrap()
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
