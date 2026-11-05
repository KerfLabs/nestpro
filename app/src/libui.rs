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
}

impl App {
    pub fn library_dir(&self) -> std::path::PathBuf {
        if self.cfg.library_dir.is_empty() { crate::library::default_dir() } else { std::path::PathBuf::from(&self.cfg.library_dir) }
    }

    /// The name shown in brackets in the title bar, as the original shows its
    /// database there: the library folder's name.
    pub fn db_label(&self) -> String {
        self.library_dir().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Library".into())
    }

    pub fn library(&self) -> Result<Library, String> {
        Library::open(&self.library_dir())
    }

    pub fn lib_refresh(&mut self) {
        match self.library() {
            Ok(l) => {
                self.libui.entries = l.entries();
                self.libui.products = self.libui.entries.iter().filter(|e| e.kind == Kind::Product).filter_map(|e| l.load_product(e.id).ok().map(|pf| (e.clone(), pf.part))).collect();
                self.libui.products.sort_by(|a, b| b.0.updated.cmp(&a.0.updated));
                self.libui.loaded = true;
            }
            Err(e) => self.libui.status = e,
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
        }
    }

    /// Another library folder: the open jobs' record numbers belong to the old
    /// one, so saving would replace whatever has that number in the new one.
    /// They become new records there.
    pub fn library_folder_changed(&mut self) {
        self.libui.loaded = false;
        self.job_lib_id = None;
        for j in &mut self.jobs {
            j.lib_id = None;
        }
    }

    /// Save the open job into the library (over its record if it came from there).
    pub fn save_job_to_library(&mut self) {
        if !self.ready_to_save(crate::stockui::SaveTarget::Library) {
            return;
        }
        let stock_note = self.stock_on_save();
        let j = self.current_jobfile();
        let r = self.library().and_then(|l| l.save_job(&j, self.job_lib_id));
        self.status = match r {
            Ok(id) => {
                self.job_lib_id = Some(id);
                self.mark_job_saved(self.cur_job);
                self.lib_refresh();
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
        self.jobs.push(crate::Job { week: if j.week.is_empty() { "Library".into() } else { j.week }, name, parts: j.parts, spec: j.spec.clone(), result: { let mut r = crate::nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried: 0 }; r.normalise(&j.spec); r }, stale: j.stale, lib_id: Some(id), cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: j.held.clone(), erp: None, file: None, saved: None });
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
            if ribbon_button(ui, Icon::Numbers, "Library", self.libui.open, true).on_hover_text("This app's own product and job database — a folder you can share").clicked() {
                self.libui.open = !self.libui.open;
                if self.libui.open {
                    self.lib_refresh();
                }
            }
            let tip = match self.job_lib_id {
                Some(id) => format!("Save the open job over library job {id}"),
                None => "Save the open job into the library".into(),
            };
            if ribbon_button(ui, Icon::Export, "Save to\nlibrary", false, true).on_hover_text(tip).clicked() {
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
        let mut save_parts = false;
        let mut rescan = false;
        let mut settings = false;
        let dir = self.library_dir();
        egui::Window::new(RichText::new("Library").size(16.0).strong()).id(egui::Id::unique("library_window")).open(&mut open).default_size([900.0, 600.0]).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(dir.display().to_string()).monospace().size(11.5).color(th().text_dim));
                if ui.small_button("Folder…").on_hover_text("Settings → Folders: put the library on a shared drive for the whole shop").clicked() {
                    settings = true;
                }
                if ui.small_button("Rescan").on_hover_text("Rebuild the list from the record files").clicked() {
                    rescan = true;
                }
            });
            ui.add_space(4.0);
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
                if self.libui.tab == 0 {
                    if ui.button("Save open job").clicked() {
                        save_job = true;
                    }
                } else if ui.button("Add open job's products").clicked() {
                    save_parts = true;
                }
            });
            ui.separator();
            let kind = if self.libui.tab == 0 { Kind::Job } else { Kind::Product };
            let needle = self.libui.search.to_lowercase();
            let mut rows: Vec<&Entry> = self.libui.entries.iter().filter(|e| e.kind == kind && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || e.folder.to_lowercase().contains(&needle))).collect();
            rows.sort_by(|a, b| b.updated.cmp(&a.updated));
            if rows.is_empty() {
                ui.label(RichText::new(if self.libui.entries.is_empty() { "The library is empty — save a job or add products to start it." } else { "Nothing matches." }).color(th().text_dim));
            }
            let mats = crate::cnc::MATERIALS;
            egui::ScrollArea::vertical().id_salt("lib_rows").auto_shrink([false, false]).show_rows(ui, 30.0, rows.len(), |ui, range| {
                for e in &rows[range] {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        ui.label(RichText::new(crate::library::auto_ref(e.kind, e.id)).monospace().size(11.0).color(th().text_dim));
                        ui.label(RichText::new(&e.name).strong());
                        if e.tech {
                            chip(ui, "technology", th().accent_soft, th().accent_fg);
                        }
                        ui.label(RichText::new(&e.folder).size(11.5).color(th().accent_fg));
                        let mat = mats.get(e.material).copied().unwrap_or("");
                        let info = if e.kind == Kind::Job { format!("{mat} {} mm · {:.0}×{:.0} · {} products · {} sheets", e.thickness, e.w, e.h, e.count_a, e.count_b) } else { format!("{mat} {} mm · {:.0}×{:.0} · qty {}", e.thickness, e.w, e.h, e.count_a) };
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

    /// The library's jobs in the Job Explorer, grouped by folder.
    pub fn library_job_tree(&mut self, ui: &mut egui::Ui) {
        if !self.libui.loaded {
            self.lib_refresh();
        }
        let jobs: Vec<Entry> = self.libui.entries.iter().filter(|e| e.kind == Kind::Job).cloned().collect();
        let needle = self.job_search.to_lowercase();
        let mut open: Option<u64> = None;
        ui.add_space(6.0);
        if jobs.is_empty() {
            // One quiet line until the library has jobs.
            ui.label(RichText::new("📚 Saved jobs (Library) — none yet").color(th().text_dim))
                .on_hover_text(format!("Jobs saved on disk in {}.\nRight-click an open job → Save to library (or Jobs → Save to library) to keep it here; open it again any time.", self.library_dir().display()));
            return;
        }
        let lib_tip = format!("Saved jobs: kept on disk in {} — they stay after the app closes. Click one to open it (it joins Open jobs).", self.library_dir().display());
        egui::CollapsingHeader::new(RichText::new(format!("📚 Saved jobs (Library) ({})", jobs.len())).strong()).id_salt("lib_jobs_root").default_open(true).show(ui, |ui| {
            let mut folders: Vec<String> = jobs.iter().map(|e| e.folder.clone()).collect();
            folders.sort_by_key(|f| f.to_lowercase());
            folders.dedup();
            for f in folders {
                let items: Vec<&Entry> = jobs.iter().filter(|e| e.folder == f && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || f.to_lowercase().contains(&needle))).collect();
                if items.is_empty() {
                    continue;
                }
                let title = if f.is_empty() { "(no folder)".to_string() } else { f.clone() };
                egui::CollapsingHeader::new(format!("{title}  ({})", items.len())).id_salt(("libf", &f)).default_open(!needle.is_empty()).show(ui, |ui| {
                    for e in items {
                        let sel = self.job_lib_id == Some(e.id);
                        let open_now = self.jobs.iter().any(|j| j.lib_id == Some(e.id)) || self.job_lib_id == Some(e.id);
                        let label = format!("{}  ·  {} sh{}", e.name, e.count_b, if open_now { "  · open" } else { "" });
                        if ui.selectable_label(sel, label).on_hover_text(format!("{} — {lib_tip}", crate::library::auto_ref(e.kind, e.id))).clicked() {
                            open = Some(e.id);
                        }
                    }
                });
            }
        });
        if let Some(id) = open {
            self.open_library_job(id);
        }
    }
}
