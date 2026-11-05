//! The Jobs screen: what the original program itself shows. A job explorer
//! grouped by week, the product list of the open job, and its nested sheets
//! as thumbnails (Sheet View) or as a table (Sheet list). Double-clicking a
//! sheet opens it on the nesting screen.

use crate::nest::Part;
use crate::theme::th;
use crate::ui_parts::*;
use crate::{cnc, App, SheetTab, Workspace};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

const MACHINE: &str = "Laser 01";
/// Placeholder for the active database name shown in the title and tree root.

fn material_code(m: usize) -> &'static str {
    ["ST", "SS", "AL"][m.min(2)]
}

/// Draw a small picture of a part inside `r`.
fn part_thumb(p: &egui::Painter, part: &Part, r: Rect) {
    let (w, h) = part.bbox();
    let s = (r.width() / w as f32).min(r.height() / h as f32);
    let o = r.center() + Vec2::new(-(w as f32) * s / 2.0, h as f32 * s / 2.0);
    paint_part(
        p,
        part,
        |q| Pos2::new(o.x + q[0] as f32 * s, o.y - q[1] as f32 * s),
        part_color(part.color),
        Stroke::new(1.0, darker(part_color(part.color), 0.5)),
        th().card,
    );
}

impl App {
    pub fn ribbon_jobs(&mut self, ui: &mut egui::Ui) {
        let has_sheets = !self.result.sheets.is_empty();
        ribbon_group(ui, "Jobs", |ui| {
            if ribbon_button(ui, Icon::New, "New\njob", false, true).clicked() {
                let week = self.jobs.get(self.cur_job).map_or("W01".to_string(), |j| j.week.clone());
                self.new_job_in_week(week);
            }
            if ribbon_button(ui, Icon::Copy, "Copy\njob", false, !self.jobs.is_empty())
                .on_hover_text("Copy the open job with its products and sheets")
                .clicked()
            {
                self.copy_job();
            }
            let saved = self.job_path.as_ref().map_or("not saved yet".to_string(), |p| p.display().to_string());
            if ribbon_button(ui, Icon::Save, "Save\njob", false, true).on_hover_text(format!("Save the job — products, sheets and cut lines (Ctrl + S; Ctrl + Shift + S to choose where)\n{saved}")).clicked() {
                self.save_job(false);
            }
            let r = crate::drawing::dropdown(ui, Icon::Folder, "Open\njob", true).on_hover_text("Recent jobs, the Library, or any job file (Ctrl + O)");
            egui::Popup::menu(&r).show(|ui| self.open_job_menu(ui));
        });
        ribbon_group(ui, "Sheets", |ui| {
            if ribbon_button(ui, Icon::Stock, "Sheet\nstock", self.stockui.open, true).on_hover_text("Sheets and remnants in stock: quantities, priorities, locking, new sheets").clicked() {
                self.stockui.open = !self.stockui.open;
                self.stockui.loaded = false;
            }
        });
        ribbon_group(ui, "Status line", |ui| {
            if ribbon_button(ui, Icon::Tag, "Show labor\nticket", self.show_labor_ticket, true)
                .on_hover_text("Add the labor ticket to the product line in the status bar")
                .clicked()
            {
                self.show_labor_ticket = !self.show_labor_ticket;
            }
        });
        ribbon_group(ui, "Import", |ui| {
            if ribbon_button(ui, Icon::Import, "Import\ndrawings", false, true)
                .on_hover_text("Add part drawings: DXF, DWG, Lantek MEC, IGES, PDF, ESSI, DSTV or GEO files (or drop them on the window)")
                .clicked()
            {
                self.import_dialog();
            }
        });
        ribbon_group(ui, "Products", |ui| {
            let r = ribbon_button(ui, Icon::AddPart, "Add\nproduct", false, true);
            add_part_menu(self, &r);
            if ribbon_button(ui, Icon::LibraryAdd, "Parts from\nlibrary", self.libui.open && self.libui.tab == 1, true)
                .on_hover_text("Preprogrammed parts kept in the library, with their technology")
                .clicked()
            {
                self.open_library_products();
            }
            let has = self.sel_part.is_some_and(|i| i < self.parts.len());
            if ribbon_button(ui, Icon::Remove, "Delete\nproduct", false, has).clicked() {
                if let Some(i) = self.sel_part {
                    self.remove_part(i);
                }
            }
        });
        ribbon_group(ui, "Nest", |ui| {
            if ribbon_button_ex(ui, Icon::NestAll, "Nest\nall", false, !self.parts.is_empty(), true).on_hover_text("F5").clicked() {
                self.run_nesting();
            }
        });
        ribbon_group(ui, "Nesting", |ui| {
            if ribbon_button(ui, Icon::Program, "Open CNC\nprogram", false, true)
                .on_hover_text("Open a nesting from its machine programs (LC5 .zip / .XML / .SPF — Lantek's or this app's) to edit it")
                .clicked()
            {
                if let Some(f) = rfd::FileDialog::new().add_filter("Machine programs or Lantek export", &["zip", "ZIP", "xml", "XML", "spf", "SPF", "nxp", "NXP"]).add_filter("Lantek nesting files", &["mec", "MEC", "chp", "CHP"]).pick_file() {
                    self.open_nesting_file(&f);
                }
            }
        });
        self.ribbon_library(ui);
        self.ribbon_erp(ui);
        self.ribbon_database(ui);
        ribbon_group(ui, "Output", |ui| {
            if ribbon_button(ui, Icon::Export, "Export\nCNC", false, has_sheets).on_hover_text(crate::export_tip(self)).clicked() {
                self.export_checked();
            }
            if ribbon_button(ui, Icon::Times, "Times and\ncosts", self.show_times, true).clicked() {
                self.show_times = !self.show_times;
            }
        });
    }

    pub fn jobs_workspace(&mut self, ui: &mut egui::Ui) {
        self.dock_panels(ui);
        egui::Panel::bottom("sheet_tabs")
            .frame(egui::Frame::new().fill(th().pane_header).inner_margin(egui::Margin::symmetric(6, 2)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.sheet_tab == SheetTab::View, "▣ Sheet View").clicked() {
                        self.sheet_tab = SheetTab::View;
                    }
                    if ui.selectable_label(self.sheet_tab == SheetTab::List, "≡ Sheet list").clicked() {
                        self.sheet_tab = SheetTab::List;
                    }
                });
            });
        // The product list is the main work; the sheet thumbnails get what
        // one row of cards needs (drag the edge for more).
        egui::Panel::bottom("sheet_area")
            .resizable(true)
            .default_size(240.0)
            .min_size(180.0)
            .frame(egui::Frame::new().fill(th().panel))
            .show(ui, |ui| self.sheet_area(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(th().panel))
            .show(ui, |ui| self.product_list(ui));
    }

    pub fn job_tree_body(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.add(egui::TextEdit::singleline(&mut self.job_search).hint_text("🔍 Find a job or folder…").desired_width(ui.available_width() - 8.0));
        });
        ui.add_space(4.0);
        let needle = self.job_search.to_lowercase();
        let weeks = self.job_folder_list();
        // What the menus asked for, done after drawing.
        #[derive(Clone)]
        enum Act {
            Open(usize),
            NewJob(String),
            NewFolder,
            EditFolder(String),
            DeleteFolder(String),
            EditJob(usize),
            AddNesting(usize),
            CopyJob(usize),
            SaveToLib(usize),
            CloseJob(usize),
            DeleteJob(usize),
            CommitFolder(String, String),
            CommitJob(usize, String),
            CancelEdit,
        }
        let mut act: Option<Act> = None;
        let edit = self.tree_edit.clone();
        let tips: Vec<String> = (0..self.jobs.len()).map(|i| self.job_tip(i)).collect();
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // The jobs open right now (examples, new, opened from files or the library).
            let root = egui::CollapsingHeader::new(RichText::new(format!("Open jobs ({}) — this window", self.jobs.len())).strong()).id_salt("jobs_root").default_open(true).show(ui, |ui| {
                for week in &weeks {
                    // Folder name being typed (New folder / Change name).
                    if let Some(crate::jobtree::TreeEdit::Folder { old, text }) = &edit {
                        if old == week {
                            let mut t = text.clone();
                            let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(160.0));
                            r.request_focus();
                            if r.changed() {
                                self.tree_edit = Some(crate::jobtree::TreeEdit::Folder { old: old.clone(), text: t.clone() });
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                act = Some(Act::CommitFolder(old.clone(), t));
                            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                act = Some(Act::CancelEdit);
                            }
                            continue;
                        }
                    }
                    let resp = egui::CollapsingHeader::new(week.to_string()).id_salt(("week", week)).default_open(true).show(ui, |ui| {
                        for (i, j) in self.jobs.iter().enumerate() {
                            if &j.week != week || (!needle.is_empty() && !j.name.to_lowercase().contains(&needle) && !week.to_lowercase().contains(&needle)) {
                                continue;
                            }
                            let name = if i == self.cur_job { &self.job_name } else { &j.name };
                            let sheets = if i == self.cur_job { self.result.sheets.len() } else { j.result.sheets.len() };
                            if let Some(crate::jobtree::TreeEdit::Job { index, text }) = &edit {
                                if *index == i {
                                    let mut t = text.clone();
                                    let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(160.0));
                                    r.request_focus();
                                    if r.changed() {
                                        self.tree_edit = Some(crate::jobtree::TreeEdit::Job { index: i, text: t.clone() });
                                    }
                                    if ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                                        act = Some(Act::CommitJob(i, t));
                                    } else if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
                                        act = Some(Act::CancelEdit);
                                    }
                                    continue;
                                }
                            }
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                                ui.painter().rect_stroke(r.shrink(2.0), CornerRadius::same(2), Stroke::new(1.3, th().accent_fg), egui::StrokeKind::Middle);
                                let txt = if i == self.cur_job { RichText::new(name).strong() } else { RichText::new(name) };
                                let row = ui.selectable_label(i == self.cur_job, txt).on_hover_text(&tips[i]);
                                if row.clicked() {
                                    act = Some(Act::Open(i));
                                }
                                if row.double_clicked() {
                                    act = Some(Act::EditJob(i));
                                }
                                row.context_menu(|ui| {
                                    if ui.button("Open").clicked() {
                                        act = Some(Act::Open(i));
                                        ui.close();
                                    }
                                    if ui.button("➕ Add nesting (new sheet)").on_hover_text("Open the job in Nesting on a new empty sheet").clicked() {
                                        act = Some(Act::AddNesting(i));
                                        ui.close();
                                    }
                                    if ui.button("💾 Save to library").on_hover_text("Keep this job on disk under Saved jobs (Library)").clicked() {
                                        act = Some(Act::SaveToLib(i));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button("Copy").clicked() {
                                        act = Some(Act::CopyJob(i));
                                        ui.close();
                                    }
                                    if ui.button("Change name").clicked() {
                                        act = Some(Act::EditJob(i));
                                        ui.close();
                                    }
                                    let saved = if i == self.cur_job { self.job_lib_id.is_some() } else { j.lib_id.is_some() };
                                    if ui.add_enabled(saved, egui::Button::new("Close")).on_hover_text("Take it out of Open jobs; it stays in Saved jobs (Library)").on_disabled_hover_text("Save it to the library first — or Delete it").clicked() {
                                        act = Some(Act::CloseJob(i));
                                        ui.close();
                                    }
                                    if ui.button("Delete…").clicked() {
                                        act = Some(Act::DeleteJob(i));
                                        ui.close();
                                    }
                                });
                                ui.label(RichText::new(format!("{sheets} sh")).size(11.0).color(th().text_dim));
                                let saved = if i == self.cur_job { self.job_lib_id.is_some() } else { j.lib_id.is_some() };
                                if saved {
                                    ui.label(RichText::new("✔").size(11.0).color(th().accent_fg)).on_hover_text("Saved in the library");
                                }
                            });
                        }
                    });
                    resp.header_response.on_hover_text(format!("{} job(s) — right-click for New job, New folder, Change name, Delete", self.jobs.iter().filter(|j| &j.week == week).count())).context_menu(|ui| {
                        ui.menu_button("New", |ui| {
                            if ui.button("📁 Folder").clicked() {
                                act = Some(Act::NewFolder);
                                ui.close();
                            }
                            if ui.button("Job").clicked() {
                                act = Some(Act::NewJob(week.clone()));
                                ui.close();
                            }
                        });
                        ui.separator();
                        if ui.button("Change name").clicked() {
                            act = Some(Act::EditFolder(week.clone()));
                            ui.close();
                        }
                        if ui.button("Delete").on_hover_text("Only an empty folder can be deleted").clicked() {
                            act = Some(Act::DeleteFolder(week.clone()));
                            ui.close();
                        }
                    });
                }
            });
            root.header_response.on_hover_text("Open jobs: the jobs loaded in this window now (examples, new jobs, opened files and programs, jobs opened from the library). ✔ = saved in the library; others are lost when the app closes unless you Save job or Save to library.\nRight-click: New ▸ Folder / Job").context_menu(|ui| {
                ui.menu_button("New", |ui| {
                    if ui.button("📁 Folder").clicked() {
                        act = Some(Act::NewFolder);
                        ui.close();
                    }
                    if ui.button("Job").clicked() {
                        act = Some(Act::NewJob(weeks.first().cloned().unwrap_or_else(|| "W01".into())));
                        ui.close();
                    }
                });
            });
        });
        match act {
            Some(Act::Open(i)) => self.open_job(i),
            Some(Act::NewJob(w)) => self.new_job_in_week(w),
            Some(Act::NewFolder) => self.new_folder(),
            Some(Act::EditFolder(f)) => self.tree_edit = Some(crate::jobtree::TreeEdit::Folder { old: f.clone(), text: f }),
            Some(Act::DeleteFolder(f)) => self.delete_folder(&f),
            Some(Act::EditJob(i)) => {
                let n = if i == self.cur_job { self.job_name.clone() } else { self.jobs[i].name.clone() };
                self.tree_edit = Some(crate::jobtree::TreeEdit::Job { index: i, text: n });
            }
            Some(Act::AddNesting(i)) => self.add_nesting_to_job(i),
            Some(Act::CopyJob(i)) => {
                self.open_job(i);
                self.copy_job();
            }
            Some(Act::DeleteJob(i)) => self.confirm_delete_job = Some(i),
            Some(Act::CloseJob(i)) => self.close_job(i),
            Some(Act::SaveToLib(i)) => {
                self.open_job(i);
                self.save_job_to_library();
            }
            Some(Act::CommitFolder(old, t)) => {
                if self.rename_folder(&old, &t) {
                    self.tree_edit = None;
                }
            }
            Some(Act::CommitJob(i, t)) => {
                if self.rename_job(i, &t) {
                    self.tree_edit = None;
                }
            }
            Some(Act::CancelEdit) => self.tree_edit = None,
            None => {}
        }
        // Delete a job: asked first, as the original does.
        if let Some(i) = self.confirm_delete_job {
            let name = if i == self.cur_job { self.job_name.clone() } else { self.jobs.get(i).map(|j| j.name.clone()).unwrap_or_default() };
            let mut keep = true;
            egui::Window::new("Delete job").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
                ui.label(format!("Delete the job '{name}' with its products and nestings?"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.delete_job(i);
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if !keep {
                self.confirm_delete_job = None;
            }
        }
        self.library_job_tree(ui);
        self.db_job_tree(ui);
    }

    /// The connection's jobs (Expert's Job Explorer folders) below the local
    /// ones: loaded in the background, or a Connect row when it can't sign in
    /// by itself yet.
    fn db_job_tree(&mut self, ui: &mut egui::Ui) {
        let Some(conn) = self.cfg.conn().cloned() else { return };
        if conn.server.is_empty() {
            return;
        }
        ui.add_space(6.0);
        let loading = self.dbb.pending.is_some();
        let failed = self.dbb.status.starts_with('✖');
        let mut connect = false;
        let mut settings = false;
        let mut show: Option<String> = None;
        let title = format!("🗄 {} (Lantek)", conn.database);
        let needle = self.job_search.to_lowercase();
        if self.dbb.jobs.rows.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&title).color(th().text_dim));
                if loading {
                    ui.spinner();
                } else {
                    if ui.small_button("Connect").on_hover_text(format!("Sign in to {} on {} and list its jobs here", conn.database, conn.server)).clicked() {
                        connect = true;
                    }
                    if failed {
                        ui.label(RichText::new("⚠").color(th().warn_fg)).on_hover_text(format!("{}\n{}", self.dbb.status.trim_start_matches("✖ "), dbquery_advice(&self.dbb.status)));
                    }
                }
            });
            if connect {
                if self.db_ready() {
                    self.db_open_now();
                } else {
                    self.open_db_browser();
                }
            }
            let _ = &mut settings;
            return;
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt("db_jobs_root").default_open(true).show(ui, |ui| {
                let t = &self.dbb.jobs;
                if t.rows.is_empty() {
                    ui.horizontal(|ui| {
                        if loading {
                            ui.spinner();
                            ui.label(RichText::new("Loading…").size(11.5).color(th().text_dim));
                        } else {
                            if ui.small_button("Connect").on_hover_text(format!("Sign in to {} on {}", conn.database, conn.server)).clicked() {
                                connect = true;
                            }
                            if ui.small_button("Settings…").clicked() {
                                settings = true;
                            }
                        }
                    });
                    if failed && !loading {
                        ui.label(RichText::new(self.dbb.status.trim_start_matches("✖ ")).size(10.5).color(th().warn_fg)).on_hover_text(dbquery_advice(&self.dbb.status));
                    }
                    return;
                }
                let row = |ui: &mut egui::Ui, k: usize, show: &mut Option<String>| {
                    let r = &t.rows[k];
                    let job = r.first().cloned().unwrap_or_default();
                    let name = r.get(1).cloned().unwrap_or_default();
                    ui.horizontal(|ui| {
                        let (b, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                        ui.painter().rect_stroke(b.shrink(2.0), CornerRadius::same(2), Stroke::new(1.3, th().text_dim), egui::StrokeKind::Middle);
                        let label = if name.is_empty() || name == job { job.clone() } else { format!("{job} {name}") };
                        if ui.selectable_label(false, crate::dbquery::cell(&label)).on_hover_text("Open in the Database window").clicked() {
                            *show = Some(job.clone());
                        }
                        ui.label(RichText::new(format!("{} nst", r.get(4).map_or("0", |s| s))).size(11.0).color(th().text_dim));
                    });
                };
                if !needle.is_empty() {
                    let (fs, js) = crate::dbquery::search(&self.dbb.tree, t, &needle);
                    for &k in &fs {
                        folder(ui, &self.dbb.tree, k, &row, &mut show, true);
                    }
                    for &k in js.iter().take(crate::dbquery::SEARCH_MAX) {
                        row(ui, k, &mut show);
                    }
                    if js.len() > crate::dbquery::SEARCH_MAX {
                        ui.label(RichText::new(format!("{} more — type more to narrow it down", js.len() - crate::dbquery::SEARCH_MAX)).size(11.0).color(th().text_dim));
                    }
                    if fs.is_empty() && js.is_empty() {
                        ui.label(RichText::new("No folder or job matches.").size(11.0).color(th().text_dim));
                    }
                    return;
                }
                fn folder(ui: &mut egui::Ui, tree: &crate::dbquery::JobTree, k: usize, row: &dyn Fn(&mut egui::Ui, usize, &mut Option<String>), show: &mut Option<String>, found: bool) {
                    let f = &tree.folders[k];
                    egui::CollapsingHeader::new(f.name.as_str()).id_salt(("dbjf", &f.id, found)).default_open(found).show(ui, |ui| {
                        for &c in &f.children {
                            folder(ui, tree, c, row, show, false);
                        }
                        for &j in f.jobs.iter().take(crate::dbquery::FOLDER_MAX) {
                            row(ui, j, show);
                        }
                        if f.jobs.len() > crate::dbquery::FOLDER_MAX {
                            ui.label(RichText::new(format!("{} more — search to find them", f.jobs.len() - crate::dbquery::FOLDER_MAX)).size(11.0).color(th().text_dim));
                        }
                    });
                }
                let tree = &self.dbb.tree;
                for &k in &tree.roots {
                    folder(ui, tree, k, &row, &mut show, false);
                }
                for &j in &tree.loose {
                    row(ui, j, &mut show);
                }
            });
        });
        if connect {
            if self.db_ready() {
                self.db_open_now();
            } else {
                self.open_db_browser();
            }
        }
        if settings {
            self.open_settings(crate::settings_ui::Section::Connections);
        }
        if let Some(job) = show {
            self.db_show_job(job);
        }
    }

    pub fn job_summary_body(&mut self, ui: &mut egui::Ui) {
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        let sheets = self.result.sheets.len();
        let parts_area: f64 = self.result.sheets.iter().flatten().map(|p| self.parts[p.part].net_area()).sum();
        let util = if sheets > 0 { parts_area / self.total_sheet_area().max(1.0) * 100.0 } else { 0.0 };
        let secs: f64 = self.plans.iter().map(|p| p.seconds).sum();
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 2)).show(ui, |ui| {
            // Values that don't fit wrap onto a second line rather than being
            // cut off: a job or part number cut short could pass for another one.
            let label_w = ["Job", "Material", "Sheet", "Products", "Sheets", "Usage", "Cut time", "Nestings changed"]
                .iter()
                .map(|k| ui.painter().layout_no_wrap(k.to_string(), egui::TextStyle::Body.resolve(ui.style()), th().text_dim).size().x)
                .fold(0.0, f32::max);
            let value_w = (ui.available_width() - label_w - 12.0).max(60.0);
            egui::Grid::new("job_sum").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.label(RichText::new(k).color(th().text_dim));
                    ui.scope(|ui| {
                        ui.set_max_width(value_w);
                        ui.add(egui::Label::new(RichText::new(&v).strong()).wrap());
                    });
                    ui.end_row();
                };
                row(ui, "Job", self.job_name.clone());
                row(ui, "Material", format!("{} · {} mm", cnc::MATERIALS[self.spec.material], self.spec.thickness));
                row(ui, "Sheet", format!("{:.0} × {:.0}", self.spec.w, self.spec.h));
                row(ui, "Products", format!("{} / {} pcs", self.parts.len(), need));
                row(ui, "Sheets", format!("{sheets}"));
                row(ui, "Usage", format!("{util:.1} %"));
                row(ui, "Cut time", {
                    let s = secs.round() as u64;
                    format!("{} min {:02} s", s / 60, s % 60)
                });
                row(ui, "Nestings changed", crate::jobtree::cam_date_label(self.cam_changed));
            });
        });
    }

    fn product_list(&mut self, ui: &mut egui::Ui) {
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        pane_header(ui, &format!("Product List: {} / Total Qty: {}", self.parts.len(), need));
        if self.parts.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("This job has no products yet.").color(th().text_dim));
                let r = ui.button("+ Add product");
                add_part_menu(self, &r);
            });
            return;
        }
        let mut pick = None;
        let mut open_nest = false;
        let nested: Vec<u32> = (0..self.parts.len()).map(|i| self.placed_of(i) as u32).collect();
        egui::ScrollArea::both().id_salt("products").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                egui::Grid::new("product_grid").striped(true).num_columns(11).spacing([22.0, 6.0]).min_row_height(34.0).show(ui, |ui| {
                    for h in ["", "Reference", "Machine", "Material", "Thickness", "Quantity", "Labor ticket", "Priority", "Pending quantity", "Nested", "Technology"] {
                        ui.label(RichText::new(h).strong().color(th().text_dim).size(12.0));
                    }
                    ui.end_row();
                    for (i, p) in self.parts.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(64.0, 30.0), Sense::click());
                        part_thumb(ui.painter(), p, r.shrink(2.0));
                        let sel = self.sel_part == Some(i);
                        let name = ui.selectable_label(sel, RichText::new(&p.name).strong());
                        if name.clicked() || resp.clicked() {
                            pick = Some(i);
                        }
                        if name.double_clicked() || resp.double_clicked() {
                            pick = Some(i);
                            open_nest = true;
                        }
                        ui.label(MACHINE);
                        ui.label(cnc::MATERIALS[self.spec.material]);
                        ui.label(format!("{}", self.spec.thickness));
                        ui.label(format!("{}", p.qty));
                        ui.label(format!("{}", 101 + i));
                        ui.label("0");
                        let pend = p.qty.saturating_sub(nested[i]);
                        // Only parts still waiting stand out; 0 left is plain text.
                        if pend == 0 {
                            ui.label(RichText::new("0").color(th().text_dim));
                        } else {
                            chip(ui, &format!("{pend}"), th().warn_bg, th().warn_fg);
                        }
                        ui.label(format!("{}", nested[i]));
                        match p.tech {
                            Some(t) => {
                                chip(ui, &format!("Own · {} joints", t.micro_joints), th().accent_soft, th().accent_fg);
                            }
                            None => {
                                ui.label(RichText::new("Automatic").color(th().text_dim));
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });
        if let Some(i) = pick {
            self.sel_part = Some(i);
            self.jobs_pick_sheet = false;
        }
        if open_nest {
            // Double-clicking a product opens it on its own to apply technology,
            // like the original's drawing module (O1).
            if let Some(i) = self.sel_part {
                self.open_part(i);
            }
        }
    }

    fn sheet_area(&mut self, ui: &mut egui::Ui) {
        let n = self.result.sheets.len();
        let rows = self.sheet_groups().len();
        pane_header(ui, &format!("Sheet View: {rows} / Total Qty: {n}"));
        if n == 0 {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Not nested yet.").color(th().text_dim));
                if ui.add(egui::Button::new(RichText::new("Nest all").color(Color32::WHITE).strong()).fill(th().accent)).clicked() {
                    self.run_nesting();
                }
            });
            return;
        }
        match self.sheet_tab {
            SheetTab::View => self.sheet_thumbs(ui),
            SheetTab::List => self.sheet_table(ui),
        }
    }

    fn sheet_thumbs(&mut self, ui: &mut egui::Ui) {
        let mut pick = None;
        let mut open = false;
        egui::ScrollArea::vertical().id_salt("sheet_thumbs").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                for (i, copies) in self.sheet_groups() {
                    let sheet = &self.result.sheets[i];
                    let (slot, resp) = ui.allocate_exact_size(Vec2::new(200.0, 136.0), Sense::click());
                    let hover = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
                    let card = slot.translate(Vec2::new(0.0, -2.0 * hover));
                    let p = ui.painter();
                    p.rect_filled(card.translate(Vec2::new(0.0, 2.0 + 2.0 * hover)), CornerRadius::same(10), Color32::from_black_alpha((th().shadow as f32 * (0.6 + hover)) as u8));
                    p.rect_filled(card, CornerRadius::same(10), th().card);
                    let sel = i == self.cur_sheet;
                    p.rect_stroke(card, CornerRadius::same(10), if sel { Stroke::new(2.0, th().accent) } else { Stroke::new(1.0, th().border) }, egui::StrokeKind::Inside);
                    // Mini sheet
                    let area = Rect::from_min_max(card.min + Vec2::new(10.0, 10.0), Pos2::new(card.right() - 10.0, card.bottom() - 34.0));
                    let fs = self.sheet_spec(i);
                    let s = (area.width() / fs.w as f32).min(area.height() / fs.h as f32);
                    let sw = fs.w as f32 * s;
                    let sh = fs.h as f32 * s;
                    let srect = Rect::from_center_size(area.center(), Vec2::new(sw, sh));
                    p.rect_filled(srect, CornerRadius::same(2), th().sheet);
                    let o = srect.left_bottom();
                    for pl in sheet {
                        let part = &self.parts[pl.part];
                        paint_part(
                            p,
                            part,
                            |q| {
                                let m = pl.map(q);
                                Pos2::new(o.x + m[0] as f32 * s, o.y - m[1] as f32 * s)
                            },
                            part_color(part.color),
                            Stroke::NONE,
                            th().sheet,
                        );
                    }
                    // Original marks the picture of a modified nesting (MECHANICS.md "Mechanics read from the original" #5).
                    if self.stale {
                        p.rect_filled(srect, CornerRadius::same(2), with_alpha(th().card, 150));
                        let badge = Rect::from_min_size(srect.left_top() + Vec2::new(4.0, 4.0), Vec2::new(62.0, 18.0));
                        p.rect_filled(badge, CornerRadius::same(9), th().warn_bg);
                        p.text(badge.center(), Align2::CENTER_CENTER, "Modified", FontId::proportional(11.0), th().warn_fg);
                    }
                    if copies > 1 {
                        let badge = Rect::from_min_size(Pos2::new(srect.right() - 34.0, srect.top() + 4.0), Vec2::new(30.0, 18.0));
                        p.rect_filled(badge, CornerRadius::same(9), th().accent);
                        p.text(badge.center(), Align2::CENTER_CENTER, format!("×{copies}"), FontId::proportional(11.0), Color32::WHITE);
                    }
                    let (u, _, _) = self.sheet_usage(i);
                    p.text(Pos2::new(card.left() + 12.0, card.bottom() - 16.0), Align2::LEFT_CENTER, format!("Sheet {}", i + 1), FontId::proportional(13.0), th().text_strong);
                    p.text(Pos2::new(card.right() - 12.0, card.bottom() - 16.0), Align2::RIGHT_CENTER, format!("{} parts · {u:.0}%", sheet.len()), FontId::proportional(11.5), th().text_dim);
                    let resp = resp.on_hover_text("Double-click to open on the nesting screen");
                    if resp.clicked() {
                        pick = Some(i);
                    }
                    if resp.double_clicked() {
                        pick = Some(i);
                        open = true;
                    }
                }
            });
        });
        self.apply_sheet_pick(pick, open);
    }

    fn apply_sheet_pick(&mut self, pick: Option<usize>, open: bool) {
        if let Some(i) = pick {
            self.jobs_pick_sheet = true;
            if i != self.cur_sheet {
                self.cur_sheet = i;
                self.sel_placed = None;
                self.refresh();
            }
        }
        if open {
            self.workspace = Workspace::Nesting;
        }
    }

    fn sheet_table(&mut self, ui: &mut egui::Ui) {
        let mut pick = None;
        let mut open = false;
        egui::ScrollArea::both().id_salt("sheet_list").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                egui::Grid::new("sheet_grid").striped(true).num_columns(14).spacing([20.0, 5.0]).show(ui, |ui| {
                    for h in ["Nesting", "Sheet", "Material", "Thickness", "Length", "Width", "Parts", "Quantity", "Usage %", "Remnant %", "Max X", "Max Y", "Weight kg", "Machine"] {
                        ui.label(RichText::new(h).strong().color(th().text_dim).size(12.0));
                    }
                    ui.end_row();
                    for (i, copies) in self.sheet_groups() {
                        let (u, rem, _) = self.sheet_usage(i);
                        let (mx, my) = self.sheet_extent(i);
                        let nest_ref = format!("{}-{:02}", self.job_name, i + 1);
                        let r = ui.selectable_label(i == self.cur_sheet, RichText::new(nest_ref).strong());
                        if r.clicked() {
                            pick = Some(i);
                        }
                        if r.double_clicked() {
                            pick = Some(i);
                            open = true;
                        }
                        let f = self.result.format(i, &self.spec);
                        let sheet_ref = if f.stock_ref.is_empty() { format!("{}{}-{:.0}x{:.0}", material_code(self.spec.material), self.spec.thickness, f.w, f.h) } else { f.stock_ref.clone() };
                        ui.label(sheet_ref);
                        ui.label(cnc::MATERIALS[self.spec.material]);
                        ui.label(format!("{}", self.spec.thickness));
                        ui.label(format!("{:.0}", f.w));
                        ui.label(format!("{:.0}", f.h));
                        ui.label(format!("{}", self.result.sheets[i].len()));
                        ui.label(format!("{copies}"));
                        ui.label(format!("{u:.1}"));
                        ui.label(format!("{rem:.1}"));
                        ui.label(format!("{mx:.1}"));
                        ui.label(format!("{my:.1}"));
                        ui.label(format!("{:.1}", cnc::weight_kg(self.spec.material, f.area_mm2(), self.spec.thickness)));
                        ui.label(MACHINE);
                        ui.end_row();
                    }
                });
            });
        });
        self.apply_sheet_pick(pick, open);
    }

    /// Status-bar line for the selection, in the original's formats:
    /// product  "%s / %d / %s / %s / %1.4f" [+ " / %ld" labor ticket]  (#7)
    /// nesting  "%s / %d / %s (%s) / %s / %1.4f"                       (#6)
    /// (MECHANICS.md "Mechanics read from the original").
    pub fn selection_line(&self) -> String {
        let t = self.spec.thickness; // unit factor 1.0 (mm)
        let mat = cnc::MATERIALS[self.spec.material];
        if self.jobs_pick_sheet {
            if self.cur_sheet >= self.result.sheets.len() {
                return String::new();
            }
            let copies = self.sheet_groups().iter().find(|g| g.0 == self.cur_sheet).map_or(1, |g| g.1);
            let nest = format!("{}-{:02}", self.job_name, self.cur_sheet + 1);
            let f = self.result.format(self.cur_sheet, &self.spec);
            let sheet = if f.stock_ref.is_empty() { format!("{}{}-{:.0}x{:.0}", material_code(self.spec.material), self.spec.thickness, f.w, f.h) } else { f.stock_ref };
            format!("{nest} / {copies} / {MACHINE} ({mat}) / {sheet} / {t:.4}")
        } else if let Some(p) = self.sel_part.and_then(|i| self.parts.get(i).map(|p| (i, p))) {
            let (i, part) = p;
            let base = format!("{} / {} / {MACHINE} / {mat} / {t:.4}", part.name, part.qty);
            if self.show_labor_ticket { format!("{base} / {}", 101 + i) } else { base }
        } else {
            String::new()
        }
    }

    pub fn jobs_status_bar(&mut self, ui: &mut egui::Ui) {
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        ui.horizontal(|ui| {
            ui.label(format!("Product List: {} / Total Qty: {}", self.parts.len(), need));
            ui.separator();
            ui.label(format!("Sheets: {}", self.result.sheets.len()));
            ui.separator();
            let line = self.selection_line();
            if !line.is_empty() {
                ui.label(RichText::new(line).monospace().color(th().text_strong));
                ui.separator();
            }
            ui.add_space(8.0);
            ui.label(RichText::new(&self.status).color(th().text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("mm");
            });
        });
    }
}

fn dbquery_advice(status: &str) -> String {
    crate::dbquery::advice(status).unwrap_or("").to_string()
}
