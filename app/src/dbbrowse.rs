//! The Database window: sign in to the chosen connection's SQL Server
//! database (read-only). For a Lantek Expert database it shows the Job
//! Explorer tree and each job's parts, nestings and CNC programs; for any
//! database it lists the tables with row counts and shows their first rows.

use crate::dbquery::{self, Table};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::RichText;
use std::sync::mpsc::Receiver;

/// What a running query will fill in.
#[derive(Clone, Debug, PartialEq)]
pub enum Want {
    Tables,
    Rows(String),
    Jobs,
    Folders,
    /// (job, view in dbquery::JOB_VIEWS)
    JobDetail(String, usize),
}

#[derive(Default)]
pub struct DbBrowser {
    pub open: bool,
    /// Typed for a SQL login without a remembered password; memory only.
    pub password: String,
    pub tables: Vec<(String, String)>,
    pub filter: String,
    pub current: Option<String>,
    pub data: Table,
    pub status: String,
    pub pending: Option<(Want, Receiver<Result<Table, String>>)>,
    /// Shown page: 0 jobs, 1 all tables.
    pub page: usize,
    pub jobs: Table,
    pub tree: dbquery::JobTree,
    pub job: Option<String>,
    pub job_view: usize,
    pub job_data: Table,
}

impl App {
    pub(crate) fn db_query(&mut self, want: Want, sql: String) {
        let Some(db) = self.cfg.conn().cloned() else {
            self.dbb.status = "No connection yet — add one in Settings → Connections.".into();
            return;
        };
        let pw = self.dbb.password.clone();
        self.dbb.status = match &want {
            Want::Tables => format!("Signing in to {} on {}…", db.database, db.server),
            Want::Rows(t) => format!("Reading {t}…"),
            Want::Jobs => "Reading jobs…".into(),
            Want::Folders => "Reading the Job Explorer folders…".into(),
            Want::JobDetail(j, _) => format!("Reading job {j}…"),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(dbquery::run_conn(&db, &pw, &sql));
        });
        self.dbb.pending = Some((want, rx));
    }

    /// Can the active connection sign in without asking (Windows account or
    /// a remembered / typed password)?
    pub(crate) fn db_ready(&self) -> bool {
        self.cfg.conn().is_some_and(|c| !c.server.is_empty() && (c.windows || !c.password_enc.is_empty() || !self.dbb.password.is_empty()))
    }

    /// Show the database window; it loads the jobs right away when the
    /// connection can sign in by itself (otherwise it waits for a password —
    /// failed sign-ins can lock a SQL account).
    pub fn open_db_browser(&mut self) {
        self.dbb.open = true;
        if self.shot_mode() {
            return;
        }
        if self.db_ready() && self.dbb.jobs.rows.is_empty() && self.dbb.pending.is_none() {
            self.db_open_now();
        } else if self.cfg.conn().is_none() {
            self.dbb.status = "No connection yet — add one in Settings → Connections.".into();
        } else if !self.db_ready() && self.dbb.status.is_empty() {
            self.dbb.status = "Type the password (or set one up in Settings → Connections), then Open.".into();
        }
    }

    /// Sign in and read the jobs (then the folders).
    pub fn db_open_now(&mut self) {
        self.dbb.page = 0;
        self.db_query(Want::Jobs, dbquery::jobs_sql());
    }

    fn shot_mode(&self) -> bool {
        std::env::args().any(|a| a == "--screenshot")
    }

    /// The Jobs page: recent jobs on the left; for the chosen job, its parts,
    /// its nestings with their CNC programs, or the parts on them.
    fn db_jobs_page(&mut self, ui: &mut egui::Ui, pick: &mut Option<(String, usize)>) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(430.0);
                ui.add(egui::TextEdit::singleline(&mut self.dbb.filter).hint_text("🔍 Find a job or folder").desired_width(420.0));
                let needle = self.dbb.filter.to_lowercase();
                let t = &self.dbb.jobs;
                if t.rows.is_empty() {
                    ui.label(RichText::new("Open database to see the Job Explorer: Expert's job folders and the jobs in them, with how many parts and nestings each has.").size(11.5).color(th().text_dim));
                }
                let db_name = self.cfg.conn().map(|d| d.database.clone()).unwrap_or_default();
                let cur = self.dbb.job.clone();
                let view = self.dbb.job_view;
                egui::ScrollArea::vertical().id_salt("db_jobs").max_height(520.0).show(ui, |ui| {
                    let job_row = |ui: &mut egui::Ui, j: usize, pick: &mut Option<(String, usize)>| {
                        let r = &t.rows[j];
                        let job = r.first().cloned().unwrap_or_default();
                        let name = r.get(1).cloned().unwrap_or_default();
                        let counts = format!("{} parts · {} nestings", r.get(3).map_or("0", |s| s), r.get(4).map_or("0", |s| s));
                        ui.horizontal(|ui| {
                            let label = if name.is_empty() || name == job { job.clone() } else { format!("{job}  {name}") };
                            if ui.selectable_label(cur.as_deref() == Some(job.as_str()), RichText::new(dbquery::cell(&label)).size(12.0)).on_hover_text(format!("{label}\n{counts}\n{}", r.get(2).cloned().unwrap_or_default())).clicked() {
                                *pick = Some((job.clone(), view));
                            }
                            ui.label(RichText::new(counts).size(10.5).color(th().text_dim));
                        });
                    };
                    if !needle.is_empty() {
                        let (fs, js) = dbquery::search(&self.dbb.tree, t, &needle);
                        for &k in &fs {
                            folder(ui, &self.dbb.tree, k, &job_row, pick, true);
                        }
                        for &j in js.iter().take(dbquery::SEARCH_MAX) {
                            job_row(ui, j, pick);
                        }
                        if js.len() > dbquery::SEARCH_MAX {
                            ui.label(RichText::new(format!("{} more jobs — type more to narrow it down", js.len() - dbquery::SEARCH_MAX)).size(11.0).color(th().text_dim));
                        }
                        if fs.is_empty() && js.is_empty() {
                            ui.label(RichText::new("No folder or job matches.").size(11.5).color(th().text_dim));
                        }
                        return;
                    }
                    if t.rows.is_empty() {
                        return;
                    }
                    fn folder(ui: &mut egui::Ui, tree: &dbquery::JobTree, k: usize, row: &dyn Fn(&mut egui::Ui, usize, &mut Option<(String, usize)>), pick: &mut Option<(String, usize)>, found: bool) {
                        let f = &tree.folders[k];
                        egui::CollapsingHeader::new(RichText::new(format!("{}  ({})", f.name, f.jobs.len())).size(12.0)).id_salt(("dbfolder", &f.id, found)).default_open(found).show(ui, |ui| {
                            for &c in &f.children {
                                folder(ui, tree, c, row, pick, false);
                            }
                            for &j in f.jobs.iter().take(dbquery::FOLDER_MAX) {
                                row(ui, j, pick);
                            }
                            if f.jobs.len() > dbquery::FOLDER_MAX {
                                ui.label(RichText::new(format!("{} more — search to find them", f.jobs.len() - dbquery::FOLDER_MAX)).size(11.0).color(th().text_dim));
                            }
                        });
                    }
                    let tree = &self.dbb.tree;
                    egui::CollapsingHeader::new(RichText::new(format!("🗄 Jobs '{db_name}'")).strong()).id_salt("dbjobsroot").default_open(true).show(ui, |ui| {
                        for &k in &tree.roots {
                            folder(ui, tree, k, &job_row, pick, false);
                        }
                        for &j in &tree.loose {
                            job_row(ui, j, pick);
                        }
                    });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some(job) = self.dbb.job.clone() else {
                    ui.label(RichText::new("Pick a job to see its parts, its nestings and CNC programs, and the parts on each nesting.").color(th().text_dim));
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Job {job}")).strong());
                    for (k, name) in dbquery::JOB_VIEWS.iter().enumerate() {
                        if ui.selectable_label(self.dbb.job_view == k, *name).clicked() {
                            *pick = Some((job.clone(), k));
                        }
                    }
                });
                let t = &self.dbb.job_data;
                egui::ScrollArea::both().id_salt("db_job_rows").max_height(520.0).show(ui, |ui| {
                    egui::Grid::new("db_job_detail").striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                        for c in &t.cols {
                            ui.label(RichText::new(c).strong().size(11.5));
                        }
                        ui.end_row();
                        for r in &t.rows {
                            for v in r {
                                let shown = dbquery::cell(v);
                                let l = ui.label(RichText::new(&shown).size(11.5).monospace());
                                if shown.len() != v.len() {
                                    l.on_hover_text(v);
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }

    /// Collect finished database queries — every frame, so the Job Explorer
    /// fills in even when the Database window is closed.
    pub fn db_poll(&mut self, ctx: &egui::Context) {
        let mut then_folders = false;
        if let Some((want, rx)) = &self.dbb.pending {
            match rx.try_recv() {
                Ok(res) => {
                    let want = want.clone();
                    self.dbb.pending = None;
                    match (want, res) {
                        (Want::Tables, Ok(t)) => {
                            let mut v: Vec<(String, String)> = t.rows.iter().filter(|r| !r.is_empty()).map(|r| (r[0].clone(), r.get(1).cloned().unwrap_or_default())).collect();
                            // Expert's main tables first.
                            v.sort_by_key(|(n, _)| (dbquery::describe(n).is_none(), n.clone()));
                            self.dbb.status = format!("Signed in — {} tables.", v.len());
                            self.dbb.tables = v;
                        }
                        (Want::Rows(name), Ok(t)) => {
                            self.dbb.status = format!("{name}: first {} rows.", t.rows.len());
                            self.dbb.current = Some(name);
                            self.dbb.data = t;
                        }
                        (Want::Jobs, Ok(t)) => {
                            self.dbb.status = format!("Signed in — {} jobs.", t.rows.len());
                            self.dbb.jobs = t;
                            then_folders = true;
                        }
                        (Want::Folders, Ok(t)) => {
                            self.dbb.tree = dbquery::job_tree(&t, &self.dbb.jobs);
                            self.dbb.status = format!("Signed in — {} jobs in {} folders.", self.dbb.jobs.rows.len(), self.dbb.tree.folders.len());
                        }
                        (Want::JobDetail(j, v), Ok(t)) => {
                            self.dbb.status = format!("Job {j}: {} — {} rows.", dbquery::JOB_VIEWS[v.min(2)].to_lowercase(), t.rows.len());
                            self.dbb.job = Some(j);
                            self.dbb.job_view = v;
                            self.dbb.job_data = t;
                        }
                        (_, Err(e)) => self.dbb.status = format!("✖ {e}"),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => self.dbb.pending = None,
            }
        }
        if then_folders {
            self.db_query(Want::Folders, dbquery::folders_sql());
        }
    }

    /// Open a database job in the Database window (its parts first).
    pub fn db_show_job(&mut self, job: String) {
        self.dbb.open = true;
        self.dbb.page = 0;
        let sql = dbquery::job_sql(0, &job);
        self.db_query(Want::JobDetail(job, 0), sql);
    }

    pub fn db_browser(&mut self, ctx: &egui::Context) {
        self.db_poll(ctx);
        if !self.dbb.open {
            return;
        }
        let mut open = true;
        let mut pick: Option<String> = None;
        let mut reload = false;
        let mut job_pick: Option<(String, usize)> = None;
        let mut settings = false;
        let mut switch: Option<usize> = None;
        let conns: Vec<String> = self.cfg.connections.iter().map(|c| c.name.clone()).collect();
        let cur = self.cfg.conn().cloned().unwrap_or_default();
        egui::Window::new(RichText::new("Database").size(16.0).strong()).id(egui::Id::unique("db_browser")).open(&mut open).default_size([1100.0, 640.0]).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("db_conn_pick").width(220.0).selected_text(if conns.is_empty() { "no connection".to_string() } else { cur.name.clone() }).show_ui(ui, |ui| {
                    for (k, n) in conns.iter().enumerate() {
                        if ui.selectable_label(k == self.cfg.active_conn, n).clicked() {
                            switch = Some(k);
                        }
                    }
                });
                ui.label(RichText::new(format!("{} · {}", cur.database, cur.server)).color(th().text_dim));
                ui.label(RichText::new(if cur.windows { "Windows account".to_string() } else { format!("login {}", cur.user) }).color(th().text_dim));
                if !cur.windows && cur.password_enc.is_empty() {
                    ui.add(egui::TextEdit::singleline(&mut self.dbb.password).password(true).hint_text("password").desired_width(120.0)).on_hover_text("Kept in memory only — use Settings → Connections → Remember to save it encrypted");
                }
                if ui.button("⚙ Connection…").clicked() {
                    settings = true;
                }
                let busy = self.dbb.pending.is_some();
                if ui.add_enabled(!busy, egui::Button::new(if self.dbb.jobs.rows.is_empty() { "Open database" } else { "Reload" })).clicked() {
                    if self.dbb.page == 0 {
                        job_pick = Some((String::new(), usize::MAX));
                    } else {
                        reload = true;
                    }
                }
                chip(ui, "read-only", th().ok_bg, th().ok_fg);
            });
            let col = if self.dbb.status.starts_with('✖') { th().warn_fg } else { th().text_dim };
            ui.label(RichText::new(&self.dbb.status).size(11.5).color(col));
            if let Some(a) = dbquery::advice(&self.dbb.status) {
                ui.label(RichText::new(a).size(11.5).color(th().text));
            }
            ui.horizontal(|ui| {
                for (k, name) in ["Jobs", "All tables"].iter().enumerate() {
                    if ui.selectable_label(self.dbb.page == k, RichText::new(*name).strong()).clicked() {
                        self.dbb.page = k;
                        if k == 1 && self.dbb.tables.is_empty() {
                            reload = true;
                        }
                    }
                }
            });
            ui.separator();
            if self.dbb.page == 0 {
                self.db_jobs_page(ui, &mut job_pick);
                return;
            }
            ui.horizontal_top(|ui| {
                // Table list.
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    ui.add(egui::TextEdit::singleline(&mut self.dbb.filter).hint_text("🔍 Filter tables").desired_width(290.0));
                    egui::ScrollArea::vertical().id_salt("db_tables").max_height(520.0).show(ui, |ui| {
                        let needle = self.dbb.filter.to_lowercase();
                        if self.dbb.tables.is_empty() {
                            ui.label(RichText::new("Expert's main tables:").size(11.5).color(th().text_dim));
                            for (n, d, _) in dbquery::LANTEK_TABLES {
                                if ui.selectable_label(self.dbb.current.as_deref() == Some(n), RichText::new(*n).monospace().size(11.0)).on_hover_text(*d).clicked() {
                                    pick = Some(n.to_string());
                                }
                                ui.label(RichText::new(*d).size(10.5).color(th().text_dim));
                            }
                        }
                        for (n, rows) in &self.dbb.tables {
                            let d = dbquery::describe(n);
                            if !needle.is_empty() && !n.to_lowercase().contains(&needle) && !d.is_some_and(|d| d.to_lowercase().contains(&needle)) {
                                continue;
                            }
                            let label = RichText::new(format!("{n}  ({rows})")).monospace().size(11.0);
                            let r = ui.selectable_label(self.dbb.current.as_deref() == Some(n), if d.is_some() { label.strong() } else { label });
                            let r = match d {
                                Some(d) => {
                                    ui.label(RichText::new(d).size(10.5).color(th().accent_fg));
                                    r.on_hover_text(d)
                                }
                                None => r,
                            };
                            if r.clicked() {
                                pick = Some(n.clone());
                            }
                        }
                    });
                });
                ui.separator();
                // Rows of the chosen table.
                ui.vertical(|ui| {
                    if let Some(cur) = &self.dbb.current {
                        ui.label(RichText::new(cur).strong().monospace());
                        if let Some(d) = dbquery::describe(cur) {
                            ui.label(RichText::new(d).size(11.0).color(th().text_dim));
                        }
                    } else {
                        ui.label(RichText::new("Pick a table to see its first rows.").color(th().text_dim));
                    }
                    let t = &self.dbb.data;
                    egui::ScrollArea::both().id_salt("db_rows").max_height(520.0).show(ui, |ui| {
                        egui::Grid::new("db_grid_rows").striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                            for c in &t.cols {
                                ui.label(RichText::new(c).strong().size(11.5));
                            }
                            ui.end_row();
                            for r in &t.rows {
                                for v in r {
                                    let shown = dbquery::cell(v);
                                    let l = ui.label(RichText::new(&shown).size(11.5).monospace());
                                    if shown.len() != v.len() {
                                        l.on_hover_text(v);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
            });
        });
        if settings {
            self.open_settings(crate::settings_ui::Section::Connections);
        }
        if let Some(k) = switch {
            if k != self.cfg.active_conn {
                self.cfg.active_conn = k;
                let open = self.dbb.open;
                self.dbb = Default::default();
                self.dbb.open = open;
                self.open_db_browser();
            }
        }
        if reload {
            self.db_query(Want::Tables, dbquery::tables_sql());
        }
        match job_pick {
            Some((_, usize::MAX)) => self.db_query(Want::Jobs, dbquery::jobs_sql()),
            Some((j, v)) => {
                let sql = dbquery::job_sql(v, &j);
                self.db_query(Want::JobDetail(j, v), sql);
            }
            None => {}
        }
        if let Some(n) = pick {
            if let Some(sql) = dbquery::browse_sql(&n) {
                self.db_query(Want::Rows(n), sql);
            }
        }
        if !open {
            self.dbb.open = false;
        }
    }
}
