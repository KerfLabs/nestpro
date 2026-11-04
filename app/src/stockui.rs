//! The sheet stock in the app: nesting from stock, keeping stock right when a
//! job is saved, turning remnant lines into stock remnants, and the Sheet stock
//! window (the original's Sheet selection dialog and Sheets manager, manual
//! pp. 535–539, with the Do All and Sheet Remnants options, pp. 209–213, 556).

use crate::engine::stockflow::{self, StockUse};
use crate::nest::{self, FormatSlot, SheetFormat};
use crate::stock::{self, Criteria, EffMode, Item, Order, Stock};
use crate::theme::th;
use std::collections::BTreeMap;
use crate::App;
use eframe::egui;
use egui::{Color32, RichText, Stroke};

/// The Sheet stock window's state.
pub struct StockUi {
    pub open: bool,
    pub stock: Stock,
    pub loaded: bool,
    /// Show only the open job's material and thickness.
    pub job_only: bool,
    pub crit: Criteria,
    pub sel: Option<String>,
    pub qty_step: i32,
    pub new_item: Item,
    pub creating: bool,
    pub msg: String,
    pub show_options: bool,
}

impl Default for StockUi {
    fn default() -> Self {
        StockUi {
            open: false,
            stock: Stock::default(),
            loaded: false,
            job_only: true,
            crit: Criteria::everything(),
            sel: None,
            qty_step: 1,
            new_item: Item::sheet("", 0, 3.0, 3000.0, 1500.0, 1),
            creating: false,
            msg: String::new(),
            show_options: false,
        }
    }
}

impl App {
    /// What nesting from stock needs from the open job (engine::stockflow).
    /// The remnants this job cut itself are never nested on again.
    pub fn stock_use(&self) -> StockUse<'_> {
        StockUse { dir: self.library_dir(), st: &self.cfg.stock, held: &self.held, exclude: self.result.formats.iter().flat_map(|f| f.made.iter().cloned()).collect() }
    }

    /// The sheets nesting may use, in the order it takes them (SS4–SS6, SS12);
    /// None when stock isn't used or has nothing for this job.
    pub fn stock_plan(&self, exclude: &[String]) -> Option<Vec<FormatSlot>> {
        stockflow::stock_plan(&self.spec, &self.stock_use(), exclude)
    }

    /// Nest from stock, dropping stock sheets below the minimum efficiency and
    /// nesting again without them (SS7). None when stock isn't used.
    pub fn nest_from_stock(&self) -> Option<nest::NestResult> {
        stockflow::nest_from_stock(&self.parts, &self.spec, &self.stock_use())
    }

    fn job_sheets(&self) -> stockflow::JobSheets<'_> {
        stockflow::JobSheets { name: &self.job_name, parts: &self.parts, spec: &self.spec, sheets: &self.result.sheets, cut_lines: &self.cut_lines }
    }

    /// Before a job is written: put new remnants in stock and settle what the
    /// job holds. Returns a note for the status line ("" when nothing changed).
    pub fn stock_on_save(&mut self) -> String {
        let names = self.remnant_names.take();
        self.result.normalise(&self.spec);
        let dir = self.library_dir();
        let (mut formats, mut held) = (std::mem::take(&mut self.result.formats), std::mem::take(&mut self.held));
        let note = stockflow::on_save(&dir, &self.job_sheets(), &mut formats, &mut held, &self.cfg.stock, false, names.as_ref());
        (self.result.formats, self.held) = (formats, held);
        self.stockui.loaded = false;
        if note.is_empty() { String::new() } else { format!(" — {note}.") }
    }

    /// Take the sheets the job now uses from stock and give back the ones it no
    /// longer uses (SS10, SS11). Done at save time: the original takes them
    /// when a sheet is given to a nesting, but here nestings live in memory
    /// until saved.
    pub fn reconcile_stock(&mut self) -> Result<String, String> {
        let dir = self.library_dir();
        stockflow::reconcile(&dir, &self.result.formats, self.result.sheets.len(), &mut self.held, self.cfg.stock.allow_no_stock)
    }

    /// Store the remnants of every sheet that has a remnant line (or, with
    /// "completed sheets", every nested sheet) and none stored yet (RE1). When a
    /// sheet's remnant lines changed since its remnants were stored, they're
    /// made again — unless one of them has been used since; then they stay and
    /// the status line says so (UI12).
    pub fn make_remnants(&mut self) -> Result<usize, String> {
        self.make_remnants_named(None)
    }

    /// The new remnants as rows for the naming window, with suggested references.
    pub fn remnant_rows(&mut self) -> Vec<NameRow> {
        self.result.normalise(&self.spec);
        stockflow::remnant_rows(&self.library_dir(), &self.job_sheets(), &self.result.formats, &self.cfg.stock, false)
    }

    /// Store the remnants, under the references given for (sheet, piece) where
    /// there are some (Prompt for references, UI14), else numbered. Remnants
    /// of sheets whose remnant lines changed are made again unless already
    /// used (ST12; the status line says so).
    pub fn make_remnants_named(&mut self, names: Option<&BTreeMap<(usize, usize), String>>) -> Result<usize, String> {
        self.result.normalise(&self.spec);
        let dir = self.library_dir();
        let mut formats = std::mem::take(&mut self.result.formats);
        let r = stockflow::make_remnants(&dir, &self.job_sheets(), &mut formats, &self.cfg.stock, false, names);
        self.result.formats = formats;
        let m = r?;
        let k = m.kept_note();
        if !k.is_empty() {
            self.status = format!("{}{}.", k[..1].to_uppercase(), &k[1..]);
        }
        Ok(m.count)
    }
    /// Put a stock item on the current sheet (Nesting → Sheets → New). The
    /// parts on it stay if they fit.
    pub fn use_stock_item_on_sheet(&mut self, reference: &str) {
        let Some(it) = Stock::load(&self.library_dir()).ok().and_then(|s| s.get(reference).cloned()) else {
            self.status = format!("Sheet {reference} isn't in stock.");
            return;
        };
        if it.material != self.spec.material || (it.thickness - self.spec.thickness).abs() > stock::TOL {
            self.status = format!("Sheet {reference} is {} {} mm; this job is {} {} mm.", crate::cnc::MATERIALS[it.material.min(2)], it.thickness, crate::cnc::MATERIALS[self.spec.material.min(2)], self.spec.thickness);
            return;
        }
        self.checkpoint();
        if self.result.sheets.is_empty() {
            self.result.push_sheet(SheetFormat::from_stock(&it));
            self.cur_sheet = 0;
        } else {
            self.result.normalise(&self.spec);
            self.result.formats[self.cur_sheet] = SheetFormat::from_stock(&it);
        }
        let i = self.cur_sheet;
        let misfits = self.result.sheets[i].iter().filter(|p| !self.fits_sheet(p, i)).count();
        self.mark_cam_changed();
        self.refresh();
        self.status = if misfits > 0 { format!("Sheet {i1} is now {reference}; {misfits} part(s) no longer fit — move them or nest again.", i1 = i + 1) } else { format!("Sheet {} is now {reference}.", i + 1) };
    }

    fn stock_reload(&mut self) {
        match Stock::load(&self.library_dir()) {
            Ok(s) => {
                self.stockui.stock = s;
                self.stockui.msg.clear();
            }
            Err(e) => self.stockui.msg = e,
        }
        self.stockui.loaded = true;
    }

    /// Change the stock file and refresh the window; errors go to its message line.
    fn stock_change(&mut self, f: impl FnOnce(&mut Stock) -> Result<(), String>) {
        match Stock::update(&self.library_dir(), |s| f(s)) {
            Ok(()) => self.stock_reload(),
            Err(e) => self.stockui.msg = e,
        }
    }

    pub fn stock_window(&mut self, ctx: &egui::Context) {
        if !self.stockui.open {
            return;
        }
        if !self.stockui.loaded {
            self.stock_reload();
        }
        let mut open = true;
        let mut action: Option<Box<dyn FnOnce(&mut App)>> = None;
        let (mat, thk) = (self.spec.material, self.spec.thickness);
        // What the open job uses now, and held at its last save.
        let mut using: BTreeMap<String, i32> = BTreeMap::new();
        for f in self.result.formats.iter().take(self.result.sheets.len()) {
            if !f.stock_ref.is_empty() {
                *using.entry(f.stock_ref.clone()).or_default() += 1;
            }
        }
        let held = self.held.clone();
        egui::Window::new(RichText::new("Sheet stock").size(16.0).strong()).id(egui::Id::unique("sheet_stock")).open(&mut open).default_size([980.0, 560.0]).collapsible(false).show(ctx, |ui| {
            let su = &mut self.stockui;
            ui.horizontal(|ui| {
                ui.checkbox(&mut su.job_only, format!("Only {} {} mm (this job)", crate::cnc::MATERIALS[mat.min(2)], thk));
                ui.separator();
                ui.checkbox(&mut su.crit.sheets, "Sheets");
                ui.checkbox(&mut su.crit.nest_remnants, "Remnants from nesting");
                ui.checkbox(&mut su.crit.user_remnants, "Remnants by user");
                ui.checkbox(&mut su.crit.show_zero_qty, "Quantity 0");
                ui.checkbox(&mut su.crit.include_locked, "Locked");
            });
            ui.add_space(4.0);
            let rows: Vec<Item> = if su.job_only {
                su.stock.list(mat, thk, &su.crit).into_iter().cloned().collect()
            } else {
                let mut v: Vec<Item> = su.stock.items.iter().filter(|i| su.crit.admits(i)).cloned().collect();
                v.sort_by(|a, b| (a.material, (a.thickness * 1000.0) as i64, a.reference.len(), a.reference.clone()).cmp(&(b.material, (b.thickness * 1000.0) as i64, b.reference.len(), b.reference.clone())));
                v
            };
            let ctrl = ui.input(|i| i.modifiers.command);
            let list_h = if su.show_options { 180.0 } else { 330.0 };
            egui::ScrollArea::vertical().id_salt("stock_list").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("stock_grid").striped(true).min_col_width(40.0).show(ui, |ui| {
                    for h in ["Reference", "Kind", "Material", "Thk", "Length", "Width", "Area m²", "Qty", "This job", "Priority", "Locked", "From", "User data", "Added"] {
                        ui.label(RichText::new(h).strong().size(12.0));
                    }
                    ui.end_row();
                    for it in &rows {
                        let sel = su.sel.as_deref() == Some(it.reference.as_str());
                        if ui.selectable_label(sel, RichText::new(&it.reference).strong()).clicked() {
                            su.sel = Some(it.reference.clone());
                        }
                        let kind_col = if it.kind.is_remnant() { th().accent } else { th().text };
                        ui.label(RichText::new(it.kind.label()).color(kind_col));
                        ui.label(crate::cnc::MATERIALS[it.material.min(2)]);
                        ui.label(format!("{}", it.thickness));
                        ui.label(format!("{:.0}", it.length));
                        ui.label(format!("{:.0}", it.width));
                        ui.label(format!("{:.3}", it.area_m2()));
                        ui.label(RichText::new(format!("{}", it.qty)).color(if it.qty > 0 { th().text } else { th().text_dim }));
                        let (u, hd) = (using.get(&it.reference).copied().unwrap_or(0), held.get(&it.reference).copied().unwrap_or(0));
                        let job_txt = match (u, hd) {
                            (0, 0) => String::new(),
                            (u, h) if u == h => format!("{u}"),
                            (u, h) => format!("{u} (saved {h})"),
                        };
                        ui.label(RichText::new(job_txt).color(th().accent)).on_hover_text("Sheets of this item the open job uses. Stock changes when the job is saved.");
                        ui.label(format!("{}", it.priority));
                        let mut locked = it.locked;
                        if ui.checkbox(&mut locked, "").on_hover_text("Locked sheets are never used by automatic nesting. Ctrl + click: lock or unlock all listed").changed() {
                            let r = it.reference.clone();
                            let all: Vec<String> = rows.iter().map(|i| i.reference.clone()).collect();
                            action = Some(Box::new(move |app: &mut App| {
                                app.stock_change(|s| {
                                    if ctrl {
                                        for x in &all {
                                            s.set_locked(x, locked);
                                        }
                                    } else {
                                        s.set_locked(&r, locked);
                                    }
                                    Ok(())
                                })
                            }));
                        }
                        ui.label(if it.origin.is_empty() { it.from_job.clone() } else { format!("{} {}", it.origin, it.from_job) });
                        ui.label(it.user_data.iter().filter(|u| !u.is_empty()).cloned().collect::<Vec<_>>().join(" / "));
                        ui.label(RichText::new(&it.created).size(11.0).color(th().text_dim));
                        ui.end_row();
                    }
                });
                if rows.is_empty() {
                    ui.label(RichText::new("Nothing in stock matches — Create a sheet below.").color(th().text_dim));
                }
            });
            ui.separator();
            let selected = su.sel.clone().and_then(|r| su.stock.get(&r).cloned());
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut su.qty_step).range(1..=stock::MAX_QTY).prefix("× "));
                let has = selected.is_some();
                if ui.add_enabled(has, egui::Button::new("Add")).on_hover_text("Add this many of the selected sheet").clicked() {
                    let (r, n) = (su.sel.clone().unwrap_or_default(), su.qty_step);
                    action = Some(Box::new(move |app: &mut App| app.stock_change(|s| s.change_qty(&r, n))));
                }
                if ui.add_enabled(has, egui::Button::new("Remove")).on_hover_text("Take this many off; at 0 the sheet is deleted").clicked() {
                    let (r, n) = (su.sel.clone().unwrap_or_default(), su.qty_step);
                    action = Some(Box::new(move |app: &mut App| app.stock_change(|s| s.change_qty(&r, -n))));
                }
                if ui.add_enabled(has, egui::Button::new("Delete")).clicked() {
                    let r = su.sel.clone().unwrap_or_default();
                    action = Some(Box::new(move |app: &mut App| {
                        app.stock_change(|s| {
                            s.delete(&r);
                            Ok(())
                        });
                        app.stockui.sel = None;
                    }));
                }
                if let Some(it) = &selected {
                    let mut p = it.priority;
                    ui.label("Priority");
                    if ui.add(egui::DragValue::new(&mut p).range(1..=999)).on_hover_text("1 is used first when nesting by priority").changed() {
                        let r = it.reference.clone();
                        action = Some(Box::new(move |app: &mut App| {
                            app.stock_change(|s| {
                                if let Some(x) = s.items.iter_mut().find(|i| i.reference == r) {
                                    x.priority = p;
                                }
                                Ok(())
                            })
                        }));
                    }
                }
                ui.separator();
                let allow_empty = self.cfg.stock.allow_no_stock;
                let fits_job = selected.as_ref().is_some_and(|i| i.material == mat && (i.thickness - thk).abs() <= stock::TOL && (i.qty > 0 || allow_empty));
                if ui.add_enabled(fits_job, egui::Button::new(RichText::new("Use on current sheet").strong())).on_hover_text("Put the selected sheet or remnant on the sheet you're looking at").clicked() {
                    let r = su.sel.clone().unwrap_or_default();
                    action = Some(Box::new(move |app: &mut App| app.use_stock_item_on_sheet(&r)));
                }
                if ui.button("Create sheet…").clicked() {
                    su.creating = !su.creating;
                    su.new_item.material = mat;
                    su.new_item.thickness = thk;
                }
                if ui.selectable_label(su.show_options, "Options…").clicked() {
                    su.show_options = !su.show_options;
                }
            });
            if let Some(it) = &selected {
                if let Some(o) = &it.outline {
                    ui.add_space(4.0);
                    outline_preview(ui, o, it.length, it.width);
                }
            }
            if su.creating {
                ui.separator();
                ui.label(RichText::new("New sheet").strong());
                egui::Grid::new("stock_new").num_columns(4).show(ui, |ui| {
                    let n = &mut su.new_item;
                    ui.label("Reference");
                    ui.add(egui::TextEdit::singleline(&mut n.reference).hint_text("automatic").desired_width(120.0));
                    ui.label("Quantity");
                    ui.add(egui::DragValue::new(&mut n.qty).range(1..=stock::MAX_QTY));
                    ui.end_row();
                    ui.label("Length (X)");
                    ui.add(egui::DragValue::new(&mut n.length).range(10.0..=20000.0).suffix(" mm"));
                    ui.label("Width (Y)");
                    ui.add(egui::DragValue::new(&mut n.width).range(10.0..=6000.0).suffix(" mm"));
                    ui.end_row();
                    ui.label("Material");
                    egui::ComboBox::from_id_salt("stock_mat").selected_text(crate::cnc::MATERIALS[n.material.min(2)]).show_ui(ui, |ui| {
                        for (k, m) in crate::cnc::MATERIALS.iter().enumerate() {
                            ui.selectable_value(&mut n.material, k, *m);
                        }
                    });
                    ui.label("Thickness");
                    ui.add(egui::DragValue::new(&mut n.thickness).range(0.1..=200.0).speed(0.1).suffix(" mm"));
                    ui.end_row();
                    ui.label("Priority");
                    ui.add(egui::DragValue::new(&mut n.priority).range(1..=999));
                    ui.checkbox(&mut n.locked, "Locked");
                    ui.end_row();
                    for k in 0..3 {
                        ui.label(format!("User data {}", k + 1));
                        ui.add(egui::TextEdit::singleline(&mut n.user_data[k]).desired_width(120.0));
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                    ui.end_row();
                });
                if ui.button(RichText::new("Add to stock").strong()).clicked() {
                    let mut item = su.new_item.clone();
                    item.created = stock::today();
                    let refs = self.cfg.stock.with_references;
                    action = Some(Box::new(move |app: &mut App| {
                        let mut made = String::new();
                        app.stock_change(|s| {
                            made = s.add_sheet(item, refs)?;
                            Ok(())
                        });
                        if !made.is_empty() {
                            app.stockui.sel = Some(made.clone());
                            app.stockui.creating = false;
                            app.status = format!("Sheet {made} added to stock.");
                        }
                    }));
                }
            }
            if self.stockui.show_options {
                ui.separator();
                egui::ScrollArea::vertical().id_salt("stock_opts").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                    stock_options(ui, &mut self.cfg.stock);
                });
            }
            if !self.stockui.msg.is_empty() {
                ui.colored_label(Color32::from_rgb(220, 90, 70), &self.stockui.msg);
            }
            ui.label(RichText::new(Stock::file(&self.library_dir()).display().to_string()).monospace().size(10.5).color(th().text_dim));
        });
        if let Some(a) = action {
            a(self);
        }
        if !open {
            self.stockui.open = false;
            let _ = self.cfg.save();
        }
    }
}

pub use crate::engine::stockflow::NameRow;

/// Where a save paused for naming remnants was going.
#[derive(Clone, Debug)]
pub enum SaveTarget {
    File(std::path::PathBuf),
    Library,
}

/// The "name the new remnants" step of a save (Prompt for references when
/// generating remnants, UI14).
pub struct RemnantNaming {
    pub target: SaveTarget,
    pub rows: Vec<NameRow>,
}

impl App {
    /// Before a save: with "Prompt for references" on and new remnants to
    /// store, show the naming window instead and finish the save from there.
    /// True when the save may go ahead now.
    pub fn ready_to_save(&mut self, target: SaveTarget) -> bool {
        let st = &self.cfg.stock;
        if !st.prompt_remnant_ref || !(st.remnants_on_save || st.keep_completed) || self.remnant_names.is_some() {
            return true;
        }
        let rows = self.remnant_rows();
        if rows.is_empty() {
            return true;
        }
        self.status = format!("Name the {} new remnant(s) to finish saving.", rows.len());
        self.naming = Some(RemnantNaming { target, rows });
        false
    }

    pub fn naming_window(&mut self, ctx: &egui::Context) {
        if self.naming.is_none() {
            return;
        }
        let taken = Stock::load(&self.library_dir()).unwrap_or_default();
        let Some(nm) = self.naming.as_mut() else { return };
        let mut done: Option<bool> = None;
        egui::Window::new(RichText::new("New remnants").size(15.0).strong()).id(egui::Id::unique("remnant_naming")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Give each new remnant its reference (at most 40 characters).");
            ui.add_space(4.0);
            let mut errors = vec![];
            egui::Grid::new("naming_grid").striped(true).show(ui, |ui| {
                for h in ["Sheet", "Size", "Area", "Reference"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                let refs: Vec<String> = nm.rows.iter().map(|r| r.reference.trim().to_string()).collect();
                for (k, r) in nm.rows.iter_mut().enumerate() {
                    ui.label(format!("{}", r.sheet + 1));
                    ui.label(format!("{:.0} × {:.0}", r.w, r.h));
                    ui.label(format!("{:.3} m²", r.area_m2));
                    let resp = ui.add(egui::TextEdit::singleline(&mut r.reference).char_limit(stock::MAX_REF).desired_width(180.0));
                    let t = r.reference.trim();
                    let bad = if t.is_empty() {
                        Some("empty")
                    } else if taken.get(t).is_some() {
                        Some("already in stock")
                    } else if refs.iter().enumerate().any(|(j, o)| j != k && o == t) {
                        Some("used twice")
                    } else {
                        None
                    };
                    if let Some(b) = bad {
                        resp.on_hover_text(b);
                        errors.push(format!("{}: {b}", if t.is_empty() { "(empty)" } else { t }));
                    }
                    ui.end_row();
                }
            });
            if !errors.is_empty() {
                ui.colored_label(Color32::from_rgb(220, 90, 70), errors.join(" · "));
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(errors.is_empty(), egui::Button::new(RichText::new("OK").strong())).clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").on_hover_text("Save without storing these remnants").clicked() {
                    done = Some(false);
                }
            });
        });
        if let Some(ok) = done {
            let nm = self.naming.take().expect("naming");
            let names: BTreeMap<(usize, usize), String> = if ok { nm.rows.iter().map(|r| ((r.sheet, r.piece), r.reference.trim().to_string())).collect() } else { BTreeMap::new() };
            let keep = self.cfg.stock.clone();
            if !ok {
                // Cancel: this save stores no remnants.
                self.cfg.stock.remnants_on_save = false;
                self.cfg.stock.keep_completed = false;
            }
            self.remnant_names = Some(names);
            match nm.target {
                SaveTarget::File(p) => self.save_job_to(&p),
                SaveTarget::Library => self.save_job_to_library(),
            }
            self.cfg.stock = keep;
            self.remnant_names = None;
        }
    }
}

/// The Do All and Sheet Remnants options.
pub fn stock_options(ui: &mut egui::Ui, st: &mut stock::StockSettings) {
    ui.label(RichText::new("Automatic nesting (Do All)").strong());
    ui.checkbox(&mut st.use_stock, "Nest on sheets from stock");
    ui.add_enabled_ui(st.use_stock, |ui| {
        ui.horizontal(|ui| {
            ui.label("Order of sheets");
            egui::ComboBox::from_id_salt("stock_order").selected_text(st.order.label()).show_ui(ui, |ui| {
                for o in Order::ALL {
                    ui.selectable_value(&mut st.order, o, o.label());
                }
            });
            ui.checkbox(&mut st.remnants_first, "Remnants before sheets");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.criteria.nest_remnants, "Use remnants from nesting");
            ui.checkbox(&mut st.criteria.user_remnants, "Use remnants by user");
            ui.checkbox(&mut st.allow_no_stock, "Use sheets with no stock left");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.fallback_job_size, "When stock runs out, use the job's sheet size");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.best_format, "Look for the best sheet format");
            ui.checkbox(&mut st.best_combination, "Search for the best sheet combination");
            ui.add_enabled_ui(st.best_combination, |ui| {
                let mut pct = st.combination_min * 100.0;
                if ui.add(egui::DragValue::new(&mut pct).range(0.0..=100.0).suffix(" %")).on_hover_text("Minimum efficiency: a sheet used less than this is tried in the other sizes").changed() {
                    st.combination_min = pct / 100.0;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("Maximum time to look for the best sheets");
            ui.add(egui::DragValue::new(&mut st.max_seconds).range(1.0..=3600.0).suffix(" s"));
        });
        ui.horizontal(|ui| {
            ui.label("Minimum efficiency to keep a sheet");
            let mut pct = st.min_efficiency * 100.0;
            if ui.add(egui::DragValue::new(&mut pct).range(0.0..=100.0).suffix(" %")).on_hover_text("0 = off. A stock sheet used less than this is dropped and the parts nested on another").changed() {
                st.min_efficiency = pct / 100.0;
            }
            egui::ComboBox::from_id_salt("eff_mode").selected_text(st.eff_mode.label()).show_ui(ui, |ui| {
                for m in EffMode::ALL {
                    ui.selectable_value(&mut st.eff_mode, m, m.label());
                }
            });
        });
    });
    ui.checkbox(&mut st.with_references, "Work with sheet references").on_hover_text("Off: a new sheet of the same material, thickness and size just adds to the existing one");
    ui.add_space(6.0);
    ui.label(RichText::new("Sheet remnants").strong());
    ui.checkbox(&mut st.remnants_on_save, "Generate remnants when you save").on_hover_text("Each sheet's remnant line turns what's beyond it into a remnant in stock");
    ui.checkbox(&mut st.keep_completed, "Generate remnants with the completed sheets").on_hover_text("Keep the whole used sheet (less the parts) in stock to nest more on later");
    ui.checkbox(&mut st.prompt_remnant_ref, "Prompt for references when generating remnants");
    ui.horizontal(|ui| {
        ui.label("Minimum area of remnants");
        ui.add(egui::DragValue::new(&mut st.min_remnant_m2).range(0.0..=100.0).speed(0.01).max_decimals(3).suffix(" m²"));
    });
    ui.checkbox(&mut st.lines_after_nesting, "Create the remnant line automatically after nesting");
    ui.horizontal(|ui| {
        ui.label("Remnants can be rotated");
        egui::ComboBox::from_id_salt("rem_rot").selected_text(st.remnant_rotation.label()).show_ui(ui, |ui| {
            for r in stock::RemnantRotation::ALL {
                ui.selectable_value(&mut st.remnant_rotation, r, r.label());
            }
        });
        if st.remnant_rotation != stock::RemnantRotation::NotAllowed {
            ui.checkbox(&mut st.turn_over, "Allow turning over the remnant");
        }
    });
    let l = &mut st.line;
    egui::Grid::new("remline_opts").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
        ui.label("Cutting line shape");
        egui::ComboBox::from_id_salt("remline_shape_opts").width(170.0).selected_text(l.shape.label()).show_ui(ui, |ui| {
            for s in crate::remline::LineShape::ALL {
                ui.selectable_value(&mut l.shape, s, s.label());
            }
        });
        // The exact line follows the cut: no rounding (UI2).
        if l.shape == crate::remline::LineShape::Exact {
            l.round = false;
        }
        ui.label("Offset between the parts and the line");
        ui.horizontal(|ui| {
            ui.add_enabled(!l.cutter_radius, egui::DragValue::new(&mut l.offset).range(0.0..=500.0).speed(0.5).suffix(" mm"));
            ui.checkbox(&mut l.cutter_radius, "Cutter radius");
        });
        ui.end_row();
        ui.label("Minimum width");
        ui.add(egui::DragValue::new(&mut l.min_width).range(0.0..=5000.0).speed(1.0).suffix(" mm")).on_hover_text("A remnant strip narrower than this isn't made");
        ui.label("Offset in the sheet edge");
        ui.add(egui::DragValue::new(&mut l.edge_offset).range(0.0..=100.0).speed(0.1).suffix(" mm")).on_hover_text("Leave this much uncut where the line meets the sheet edge, so the remnant stays held");
        ui.end_row();
        let xy = l.shape.uses_min_xy();
        ui.label("Minimum X / Y");
        ui.add_enabled_ui(xy, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut l.min_x).range(0.0..=20000.0).suffix(" mm"));
                ui.add(egui::DragValue::new(&mut l.min_y).range(0.0..=6000.0).suffix(" mm"));
            });
        })
        .response
        .on_hover_text("The parts side is at least this long from where nesting starts (first six shapes)");
        ui.checkbox(&mut l.round, "Round the remnant line");
        ui.add_enabled(l.round, egui::DragValue::new(&mut l.radius).range(0.5..=500.0).speed(0.5).prefix("R ").suffix(" mm"));
        ui.end_row();
    });
}

/// A small drawing of a remnant's shape.
fn outline_preview(ui: &mut egui::Ui, o: &crate::stock::Outline, w: f64, h: f64) {
    let size = egui::vec2(220.0, 120.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let s = ((size.x as f64 - 10.0) / w.max(1.0)).min((size.y as f64 - 10.0) / h.max(1.0)) as f32;
    let o0 = rect.left_bottom() + egui::vec2(5.0, -5.0);
    let to = |q: nest::P| egui::pos2(o0.x + q[0] as f32 * s, o0.y - q[1] as f32 * s);
    p.rect_stroke(egui::Rect::from_two_pos(to([0.0, 0.0]), to([w, h])), 0.0, Stroke::new(1.0, th().border), egui::StrokeKind::Inside);
    for r in crate::region::free_rects(o) {
        p.rect_filled(egui::Rect::from_two_pos(to([r.x, r.y]), to([r.x + r.w, r.y + r.h])), 0.0, th().sheet);
    }
    for ring in std::iter::once(&o.outer).chain(o.holes.iter()) {
        let mut pts: Vec<egui::Pos2> = ring.iter().map(|&q| to(q)).collect();
        pts.extend(pts.first().copied());
        p.add(egui::Shape::line(pts, Stroke::new(1.5, th().accent)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nest::{Part, Shape};

    fn app_with_stock(tag: &str) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nestpro-stockui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = App::new(None);
        a.cfg.library_dir = dir.display().to_string();
        a.cfg.stock = stock::StockSettings { remnants_on_save: true, ..Default::default() };
        a.spec.material = 0;
        a.spec.thickness = 10.0;
        a.spec.w = 3000.0;
        a.spec.h = 1500.0;
        a.parts = vec![Part { name: "A".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 20, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.group_nest = false;
        a.held.clear();
        let l = crate::stock::Outline { outer: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [600.0, 1000.0], [600.0, 400.0], [0.0, 400.0]], holes: vec![] };
        let mut s = Stock::default();
        s.add_sheet(Item::sheet("1", 0, 10.0, 3000.0, 1500.0, 2), true).unwrap();
        let parent = s.get("1").unwrap().clone();
        s.add_remnant(l, &parent, 1, Some("1-1".into()), "old job");
        s.save(&dir).unwrap();
        (a, dir)
    }

    #[test]
    fn nesting_takes_the_remnant_first_and_saving_keeps_stock_right() {
        let (mut a, dir) = app_with_stock("flow");
        a.run_nesting();
        assert_eq!(a.total_placed(), 20);
        assert_eq!(a.result.formats[0].stock_ref, "1-1", "remnants before sheets");
        let used_full = a.result.formats.iter().filter(|f| f.stock_ref == "1").count() as i32;
        assert!(used_full >= 1);
        // Saving takes them from stock.
        a.stock_on_save();
        let s = Stock::load(&dir).unwrap();
        assert_eq!(s.get("1-1").unwrap().qty, 0);
        assert_eq!(s.get("1").unwrap().qty, 2 - used_full);
        // A remnant line on the last sheet: the far side goes into stock, once.
        let last = a.result.sheets.len() - 1;
        let mx = a.result.sheets[last].iter().map(|p| p.x + p.w).fold(0.0, f64::max) + 20.0;
        a.cut_lines.resize(last + 1, vec![]);
        a.cut_lines[last] = vec![crate::cutoff::CutLine { pts: vec![[mx, 0.0], [mx, 1500.0]], kind: crate::cutoff::CutKind::Remnant }];
        let note = a.stock_on_save();
        assert!(note.contains("1 remnant(s)"), "{note}");
        let made = a.result.formats[last].made.clone();
        assert_eq!(made.len(), 1);
        let s = Stock::load(&dir).unwrap();
        let r = s.get(&made[0]).unwrap();
        assert_eq!((r.kind, r.qty, r.origin.as_str()), (crate::stock::Kind::NestRemnant, 1, a.result.formats[last].stock_ref.as_str()));
        assert!((r.length - (3000.0 - mx - crate::cutoff::KERF_MM / 2.0)).abs() < 0.5, "{}", r.length);
        assert_eq!(a.stock_on_save(), "", "saving again changes nothing");
        assert_eq!(Stock::load(&dir).unwrap().items.len(), s.items.len());
        // Moving the line remakes the remnant (the old one wasn't used).
        a.cut_lines[last] = vec![crate::cutoff::CutLine { pts: vec![[mx + 100.0, 0.0], [mx + 100.0, 1500.0]], kind: crate::cutoff::CutKind::Remnant }];
        a.stock_on_save();
        let again = a.result.formats[last].made.clone();
        let s2 = Stock::load(&dir).unwrap();
        assert_eq!(s2.items.len(), s.items.len(), "replaced, not added");
        assert!(s2.get(&made[0]).is_none() && s2.get(&again[0]).is_some());
        // Once it's used, moving the line keeps it.
        Stock::update(&dir, |s| s.take(&again[0], 1, false)).unwrap();
        a.cut_lines[last] = vec![crate::cutoff::CutLine { pts: vec![[mx + 200.0, 0.0], [mx + 200.0, 1500.0]], kind: crate::cutoff::CutKind::Remnant }];
        let note = a.stock_on_save();
        assert_eq!(a.result.formats[last].made, again);
        assert!(note.contains("already in use"), "{note}");
        Stock::update(&dir, |s| {
            s.give_back(&again[0], 1);
            Ok(())
        })
        .unwrap();
        let s = Stock::load(&dir).unwrap();
        // Clearing the sheets gives the stock back.
        a.result = nest::NestResult::default();
        a.stock_on_save();
        let s = Stock::load(&dir).unwrap();
        assert_eq!((s.get("1-1").unwrap().qty, s.get("1").unwrap().qty), (1, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_remnant_sheet_turns_only_while_empty() {
        let (mut a, dir) = app_with_stock("turn");
        a.run_nesting();
        assert!(a.result.formats[0].remnant);
        a.cur_sheet = 0;
        a.transform_remnant(crate::cutoff::RemnantTurn::Rotate90);
        assert_eq!(a.result.formats[0].w, 1000.0, "refused: parts on it");
        a.result.sheets[0].clear();
        a.empty_sheet_kept = Some(0);
        let area = a.result.formats[0].area_mm2();
        a.transform_remnant(crate::cutoff::RemnantTurn::Rotate90);
        let f = &a.result.formats[0];
        assert!((f.area_mm2() - area).abs() < 1e-6);
        let o = f.outline.as_ref().unwrap();
        assert!(o.outer.iter().all(|p| p[0] >= -1e-9 && p[0] <= f.w + 1e-9 && p[1] >= -1e-9 && p[1] <= f.h + 1e-9), "{o:?}");
        a.transform_remnant(crate::cutoff::RemnantTurn::MirrorX);
        assert!((a.result.formats[0].area_mm2() - area).abs() < 1e-6, "mirroring keeps the area (ring direction fixed)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn semi_automatic_click_adds_a_remnant() {
        let (mut a, dir) = app_with_stock("semi");
        a.cfg.stock.use_stock = false;
        a.parts[0].qty = 4;
        a.run_nesting();
        a.cur_sheet = 0;
        a.semi_auto_at(crate::remline::SemiMode::ExteriorOrthogonal, [2800.0, 1400.0]);
        assert_eq!(a.remnant_pieces().len(), 1, "{}", a.status);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_for_references_pauses_the_save_until_named() {
        let (mut a, dir) = app_with_stock("prompt");
        a.cfg.stock.prompt_remnant_ref = true;
        a.cfg.stock.use_stock = false;
        a.parts[0].qty = 4;
        a.run_nesting();
        a.cut_lines = vec![vec![crate::cutoff::CutLine { pts: vec![[1500.0, 0.0], [1500.0, 1500.0]], kind: crate::cutoff::CutKind::Remnant }]];
        let file = dir.join("job.snjob");
        a.save_job_to(&file);
        assert!(!file.exists(), "paused for names");
        let nm = a.naming.as_ref().expect("naming step");
        assert_eq!(nm.rows.len(), 1);
        assert!(!nm.rows[0].reference.is_empty(), "a suggestion is filled in");
        // OK with a name of our own.
        let mut names = BTreeMap::new();
        names.insert((nm.rows[0].sheet, nm.rows[0].piece), "RACK-C-7".to_string());
        a.naming = None;
        a.remnant_names = Some(names);
        a.save_job_to(&file);
        assert!(file.exists());
        assert!(Stock::load(&dir).unwrap().get("RACK-C-7").is_some());
        assert_eq!(a.result.formats[0].made, vec!["RACK-C-7".to_string()]);
        // Nothing new to name: saving goes straight through.
        a.save_job_to(&file);
        assert!(a.naming.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn two_sizes(tag: &str) -> (App, std::path::PathBuf) {
        let (mut a, dir) = app_with_stock(tag);
        let mut s = Stock::default();
        s.add_sheet(Item::sheet("BIG", 0, 10.0, 3000.0, 1500.0, 5), true).unwrap();
        s.add_sheet(Item::sheet("SMALL", 0, 10.0, 1000.0, 1000.0, 5), true).unwrap();
        s.save(&dir).unwrap();
        a.parts[0].qty = 4;
        a.cfg.stock.fallback_job_size = false;
        (a, dir)
    }

    #[test]
    fn best_sheet_combination_and_best_format_pick_the_small_sheet() {
        let (mut a, dir) = two_sizes("combo");
        a.run_nesting();
        assert_eq!(a.result.formats[0].stock_ref, "BIG", "largest first, no search");
        a.cfg.stock.best_combination = true;
        a.cfg.stock.combination_min = 0.2;
        a.run_nesting();
        assert_eq!(a.result.formats.iter().map(|f| f.stock_ref.as_str()).collect::<Vec<_>>(), ["SMALL"]);
        a.cfg.stock.best_combination = false;
        a.cfg.stock.best_format = true;
        a.run_nesting();
        assert_eq!(a.result.formats.iter().map(|f| f.stock_ref.as_str()).collect::<Vec<_>>(), ["SMALL"]);
        assert_eq!(a.total_placed(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_stock_for_the_job_nesting_uses_the_job_size() {
        let (mut a, dir) = app_with_stock("none");
        a.spec.thickness = 3.0; // nothing in stock at 3 mm
        a.run_nesting();
        assert!(a.result.formats.iter().all(|f| f.stock_ref.is_empty() && f.w == 3000.0));
        assert_eq!(a.stock_on_save(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn low_efficiency_stock_sheets_are_dropped() {
        let (mut a, dir) = app_with_stock("eff");
        a.parts[0].qty = 1; // one small part: the remnant would be ~10 % used
        a.cfg.stock.min_efficiency = 0.5;
        a.run_nesting();
        assert!(a.result.formats.iter().all(|f| f.stock_ref != "1-1"), "{:?}", a.result.formats);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
