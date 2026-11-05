//! The Settings window: database connections, machines and how their
//! programs are written, material codes and folders. Every value is
//! editable and saved (settings.rs); Lantek Expert is only an import source.

use crate::settings::{AppSettings, Connection, Machine, Output};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{CornerRadius, RichText, Stroke};
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    General,
    Appearance,
    Shortcuts,
    Connections,
    Machines,
    Nesting,
    Materials,
    Folders,
    Integrations,
    About,
}

impl Section {
    /// The navigation, in groups.
    const GROUPS: [(&'static str, &'static [Section]); 4] = [
        ("APP", &[Section::General, Section::Appearance, Section::Shortcuts]),
        ("PRODUCTION", &[Section::Connections, Section::Machines, Section::Nesting, Section::Materials, Section::Folders]),
        ("INTEGRATIONS", &[Section::Integrations]),
        ("INFO", &[Section::About]),
    ];
    fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Shortcuts => "Shortcuts",
            Section::Connections => "Connections",
            Section::Machines => "Machines & output",
            Section::Nesting => "Nesting",
            Section::Materials => "Materials",
            Section::Folders => "Folders",
            Section::Integrations => "ERP / MES",
            Section::About => "About",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Section::General => "Warnings and commands",
            Section::Appearance => "Light, dark or Windows",
            Section::Shortcuts => "Keys and mouse",
            Section::Connections => "Databases to open",
            Section::Machines => "How programs are written",
            Section::Nesting => "Units, part gap, moving parts",
            Section::Materials => "Names and codes in programs",
            Section::Folders => "Imports and exports",
            Section::Integrations => "Fulcrum and other systems",
            Section::About => "This rebuild",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Section::General => "🔧",
            Section::Appearance => "◐",
            Section::Shortcuts => "⌨",
            Section::Connections => "🗄",
            Section::Machines => "⚙",
            Section::Nesting => "▣",
            Section::Materials => "▦",
            Section::Folders => "📁",
            Section::Integrations => "🔗",
            Section::About => "ℹ",
        }
    }
}

/// The keyboard and mouse shortcuts (shown in Settings → Shortcuts).
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("F5", "Nest all"),
    ("Ctrl + Z  /  Ctrl + Y", "Undo / redo"),
    ("Click / Shift or Ctrl + click", "Select a part / add it to the selection or take it out"),
    ("Drag on empty sheet", "Select the parts wholly inside the window (Shift or Ctrl adds)"),
    ("Ctrl + E  /  Ctrl + A", "Select every part on the sheet"),
    ("Alt + arrow keys", "Move the selected parts by the step (Settings → Nesting, or Ctrl + R); like the original, this move doesn't stop at other parts"),
    ("Ctrl + arrow keys", "Slide the selected parts to the next part (at the part gap) or the margin"),
    ("Arrow keys  /  Page Down, Page Up", "Look around the sheet / zoom in, out"),
    ("Ctrl + R", "Set the step (movement value)"),
    ("Drag a part", "Move the selection; it snaps one part gap from a neighbour and slides along parts it meets (hold Alt for no snap)"),
    ("Ctrl + drag a part", "Drag a copy of the selection"),
    ("Ctrl + D", "Copy the selected parts to the first free spot"),
    ("R  /  Ctrl + Q  /  Ctrl + W  /  Ctrl + 9", "Turn the selection: next allowed turn / 90° left / 90° right / 90° (direction in Settings → Nesting)"),
    ("M  /  Shift + M", "Mirror the selection left↔right / top↔bottom"),
    ("Del", "Remove the selected parts"),
    ("Esc", "Clear the selection"),
    ("Ctrl + S", "Save the job"),
    ("Ctrl + Shift + S", "Save the job as…"),
    ("Ctrl + O", "Open a saved job"),
    ("Ctrl + ,", "Settings"),
    ("Middle button / Space + drag", "Pan"),
    ("Mouse wheel", "Zoom"),
    ("Right-click (or S) on a part", "Quick ring: turn, turn back, copy on the pointer, mirror, delete, undo, fit (flick toward a tool, or click it)"),
    ("Right-click (or S) on empty sheet", "Quick ring: select all, next / previous sheet, collisions on/off, compact, undo, redo, fit"),
    ("Right-click (or S) in the Part view", "Quick ring: lead-ins, micro-joints, texts, loops, fit"),
];

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    pub section: Section,
    /// Connection / machine being edited.
    pub sel_conn: usize,
    pub sel_machine: usize,
    /// Password typed for the selected connection (never saved as text).
    pub pw_input: String,
    pub note: String,
    pub test: String,
    pub pending: Option<Receiver<String>>,
    /// Materials page: a name typed to see which material it means.
    pub mat_probe: String,
}

/// A rounded card with a title.
pub(crate) fn card<R>(ui: &mut egui::Ui, title: &str, sub: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = th();
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(14.5).strong().color(t.text_strong));
            if !sub.is_empty() {
                ui.label(RichText::new(sub).size(11.5).color(t.text_dim));
            }
            ui.add_space(8.0);
            body(ui)
        })
        .inner
}

/// A labelled row inside a card.
pub(crate) fn row(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(150.0, 24.0), egui::Sense::hover());
        ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), th().text_dim);
        body(ui);
    });
    ui.add_space(2.0);
}

/// A file path box with Browse… and a "built-in" reset.
fn file_row(ui: &mut egui::Ui, label: &str, path: &mut String, filter: (&str, &[&str]), start: &str) -> bool {
    let mut changed = false;
    row(ui, label, |ui| {
        changed |= ui.add(egui::TextEdit::singleline(path).desired_width(360.0).hint_text("built-in values")).changed();
        if ui.button("Browse…").clicked() {
            let mut d = rfd::FileDialog::new().add_filter(filter.0, filter.1);
            if !start.is_empty() {
                d = d.set_directory(start);
            }
            if let Some(f) = d.pick_file() {
                *path = f.display().to_string();
                changed = true;
            }
        }
        if !path.is_empty() && ui.small_button("✕").on_hover_text("Use the built-in values").clicked() {
            path.clear();
            changed = true;
        }
    });
    changed
}

/// A clickable tile in a list of connections / machines.
pub(crate) fn tile(ui: &mut egui::Ui, selected: bool, active: bool, title: &str, sub: &str) -> bool {
    let t = th();
    let w = 210.0;
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 54.0), egui::Sense::click());
    let hover = resp.hovered();
    let fill = if selected { t.accent_soft } else if hover { t.hover } else { t.card };
    ui.painter().rect(r, CornerRadius::same(9), fill, Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { t.accent } else { t.border }), egui::StrokeKind::Inside);
    let clip = |s: &str, n: usize| if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() };
    ui.painter().text(r.left_top() + egui::vec2(12.0, 10.0), egui::Align2::LEFT_TOP, clip(title, 24), egui::FontId::proportional(13.5), if selected { t.accent_fg } else { t.text_strong });
    ui.painter().text(r.left_top() + egui::vec2(12.0, 30.0), egui::Align2::LEFT_TOP, clip(sub, 30), egui::FontId::proportional(11.0), t.text_dim);
    if active {
        let c = r.right_top() + egui::vec2(-14.0, 14.0);
        ui.painter().circle_filled(c, 4.5, t.ok_fg);
    }
    resp.on_hover_text(format!("{title}\n{sub}{}", if active { "\n● in use" } else { "" })).clicked()
}

/// The connection string, as Expert builds it (DB2, CN1: no user → OLE DB with Windows
/// sign-in, otherwise the ODBC "SQL Server" driver), password masked.
pub fn conn_string(c: &Connection) -> String {
    let db = crate::dbconn::LantekDb {
        server: c.server.clone(),
        database: c.database.clone(),
        user: if c.windows { String::new() } else { c.user.clone() },
        has_password: !c.password_enc.is_empty(),
        ..Default::default()
    };
    db.connection_string()
}

impl App {
    pub fn open_settings(&mut self, s: Section) {
        self.setui.open = true;
        self.setui.section = s;
        self.setui.sel_conn = self.cfg.active_conn;
        self.setui.sel_machine = self.cfg.active_machine;
    }

    /// Apply the settings: the active machine's post library and BLM post
    /// files, its output kind and gas; saves when anything changed.
    pub fn apply_settings(&mut self, force: bool) {
        let m = self.cfg.machine();
        // Which machine-file section each material reads (MTL2).
        crate::engine::install_material_keys(&self.cfg);
        if force || self.applied_machine.as_ref() != Some(&m) {
            let reload_post = force || self.applied_machine.as_ref().map(|a| &a.post_lib) != Some(&m.post_lib);
            if reload_post {
                self.post = crate::engine::load_post(&m);
            }
            // BLM post values and the machine's technology file: cutting zones
            // and their order (B110–B123), leads, joints, grouping.
            crate::engine::install_machine(&m, &self.cfg.lantek_dir);
            self.blm.on = m.output == Output::BlmSiemens;
            self.blm.machine = m.name.clone();
            self.blm.gas_id = m.gas_id;
            self.blm.gas_name = m.gas_name.clone();
            self.applied_machine = Some(m);
            self.refresh();
        }
        self.cfg.warn_tech = self.warn_tech;
        self.cfg.show_unbuilt = self.dv.show_unbuilt;
        self.cfg.theme = if self.follow_system { "system" } else if crate::theme::is_dark() { "dark" } else { "light" }.into();
        if self.cfg != self.cfg_saved {
            if !self.shot_mode_now() {
                if let Err(e) = self.cfg.save() {
                    self.setui.note = format!("Couldn't save settings: {e}");
                }
            }
            self.cfg_saved = self.cfg.clone();
        }
    }

    fn shot_mode_now(&self) -> bool {
        std::env::args().any(|a| a == "--screenshot")
    }

    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.setui.pending {
            if let Ok(r) = rx.try_recv() {
                self.setui.test = r;
                self.setui.pending = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if !self.setui.open {
            self.apply_settings(false);
            return;
        }
        let mut open = true;
        let post_before = self.post.clone();
        egui::Window::new(RichText::new("Settings").size(16.0).strong())
            .id(egui::Id::unique("settings_window"))
            .open(&mut open)
            .default_size([1040.0, 680.0])
            .min_size([820.0, 480.0])
            .max_width(1200.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // Navigation.
                    ui.vertical(|ui| {
                        ui.set_width(200.0);
                        for (g, items) in Section::GROUPS {
                        ui.add_space(6.0);
                        ui.label(RichText::new(g).size(10.5).strong().color(th().text_dim));
                        ui.add_space(2.0);
                        for &s in items {
                            let sel = self.setui.section == s;
                            let (r, resp) = ui.allocate_exact_size(egui::vec2(196.0, 46.0), egui::Sense::click());
                            let t = th();
                            if sel || resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(9), if sel { t.accent_soft } else { t.hover });
                            }
                            if sel {
                                ui.painter().rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(0.0, 9.0), egui::vec2(3.0, 28.0)), CornerRadius::same(2), t.accent);
                            }
                            ui.painter().text(r.left_center() + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, s.icon(), egui::FontId::proportional(18.0), if sel { t.accent_fg } else { t.text_dim });
                            ui.painter().text(r.left_top() + egui::vec2(44.0, 7.0), egui::Align2::LEFT_TOP, s.title(), egui::FontId::proportional(13.5), if sel { t.accent_fg } else { t.text_strong });
                            ui.painter().text(r.left_top() + egui::vec2(44.0, 26.0), egui::Align2::LEFT_TOP, s.hint(), egui::FontId::proportional(11.0), t.text_dim);
                            if resp.clicked() {
                                self.setui.section = s;
                            }
                            ui.add_space(2.0);
                        }
                        }
                        ui.add_space(12.0);
                        ui.label(RichText::new("Saved automatically to").size(10.5).color(th().text_dim));
                        ui.label(RichText::new(AppSettings::file().display().to_string()).size(10.0).color(th().text_dim));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("settings_body").auto_shrink([false, false]).show(ui, |ui| ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - 8.0);
                        match self.setui.section {
                            Section::General => self.settings_general(ui),
                            Section::Appearance => self.settings_appearance(ui),
                            Section::Shortcuts => self.settings_shortcuts(ui),
                            Section::About => self.settings_about(ui),
                            Section::Connections => self.settings_connections(ui),
                            Section::Machines => self.settings_machines(ui),
                            Section::Nesting => self.settings_nesting(ui),
                            Section::Materials => self.settings_materials(ui),
                            Section::Folders => self.settings_folders(ui),
                            Section::Integrations => self.settings_integrations(ui),
                        }
                        if !self.setui.note.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new(&self.setui.note).size(11.5).color(th().accent_fg));
                        }
                    }));
                });
            });
        if !open {
            self.setui.open = false;
        }
        if self.post != post_before {
            self.refresh();
        }
        self.apply_settings(false);
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("General").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Part technology", "When you change a part that has its own technology (leads, micro-joints, texts).", |ui| {
            ui.checkbox(&mut self.warn_tech, "Ask before changing a part that has technology");
            if self.tech_answer_all.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("You chose an answer for the rest of the parts.").size(11.5).color(th().text_dim));
                    if ui.button("Ask again").clicked() {
                        self.tech_answer_all = None;
                    }
                });
            }
        });
        ui.add_space(10.0);
        card(ui, "Part data from the file name", "When DXF names follow a pattern, the reference and quantity are read from the name. Fields: [Reference] [Quantity] [Material] [Thickness], * skips text; the text between fields is the separator.", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.name_format).hint_text("[Reference]_[Quantity]_[Material]").desired_width(320.0));
                if ui.button("Example").clicked() {
                    self.cfg.name_format = "[Reference]_[Quantity]_[Material]".into();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Reference");
                ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_prefix).hint_text("prefix").desired_width(90.0));
                ui.label(RichText::new("+ name +").color(th().text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.cfg.ref_suffix).hint_text("suffix").desired_width(90.0));
                ui.label(RichText::new(format!("(at most {} characters, the name is shortened)", crate::filename::REF_MAX_CHARS)).size(11.5).color(th().text_dim));
            });
            if !self.cfg.name_format.trim().is_empty() {
                match crate::filename::parse_format(&self.cfg.name_format) {
                    Ok(f) => {
                        let sample: Vec<String> = f.items.iter().map(|(fl, sep)| format!("{}{}", match fl {
                            crate::filename::Field::Reference => "PLATE-7",
                            crate::filename::Field::Quantity => "12",
                            crate::filename::Field::Material => "S235",
                            crate::filename::Field::Thickness => "3",
                            crate::filename::Field::Ignore => "JOB88",
                        }, sep)).collect();
                        ui.label(RichText::new(format!("✓ e.g. {}.dxf", sample.concat())).size(11.5).color(th().text_dim));
                    }
                    Err(e) => {
                        ui.label(RichText::new(format!("⚠ {e}")).size(11.5).color(th().warn_fg));
                    }
                }
            }
        });
        ui.add_space(10.0);
        card(ui, "Example jobs", "Five example jobs to try the app with. Turn off to start with one empty job.", |ui| {
            ui.checkbox(&mut self.cfg.examples, "Start with the example jobs (takes effect next start)");
        });
        ui.add_space(10.0);
        card(ui, "Database jobs", "The Job Explorer can list the jobs of the connection in use, next to this PC's jobs.", |ui| {
            ui.checkbox(&mut self.cfg.auto_db, "Load them at start-up when the connection can sign in by itself (Windows account or remembered password)");
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Connection in use: {}", self.cfg.conn().map_or("none".to_string(), |c| c.name.clone()))).size(11.5).color(th().text_dim));
                if ui.small_button("Connections…").clicked() {
                    self.setui.section = Section::Connections;
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Commands", "The original has commands this rebuild doesn't have yet.", |ui| {
            ui.checkbox(&mut self.dv.show_unbuilt, "Show them greyed out in the ribbons");
            if ui.button("What's still missing in the Drawing module…").clicked() {
                self.dv.show_missing = true;
            }
        });
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Appearance").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Theme", "Also on the sun / moon switch at the top right.", |ui| {
            ui.horizontal(|ui| {
                let cur = if self.follow_system { 2 } else if crate::theme::is_dark() { 1 } else { 0 };
                for (k, (icon, name, sub)) in [("☀", "Light", "Bright workshop screens"), ("🌙", "Dark", "Easy on the eyes"), ("◐", "Follow Windows", "Matches the system")].into_iter().enumerate() {
                    let t = th();
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(190.0, 84.0), egui::Sense::click());
                    let sel = cur == k;
                    ui.painter().rect(r, CornerRadius::same(10), if sel { t.accent_soft } else if resp.hovered() { t.hover } else { t.card }, Stroke::new(if sel { 1.5 } else { 1.0 }, if sel { t.accent } else { t.border }), egui::StrokeKind::Inside);
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 12.0), egui::Align2::LEFT_TOP, icon, egui::FontId::proportional(22.0), if sel { t.accent_fg } else { t.text_dim });
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 42.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(14.0), if sel { t.accent_fg } else { t.text_strong });
                    ui.painter().text(r.left_top() + egui::vec2(14.0, 61.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(11.0), t.text_dim);
                    if resp.clicked() {
                        match k {
                            0 | 1 => {
                                self.follow_system = false;
                                crate::theme::set_dark(ui.ctx(), k == 1);
                            }
                            _ => self.follow_system = true,
                        }
                    }
                    ui.add_space(6.0);
                }
            });
        });
        ui.add_space(10.0);
        self.settings_ui_size(ui);
    }

    fn settings_ui_size(&mut self, ui: &mut egui::Ui) {
        card(ui, "Scrolling", "How far one turn of the mouse wheel scrolls lists and panels.", |ui| {
            ui.add(egui::Slider::new(&mut self.cfg.scroll_speed, 1.0..=5.0).step_by(0.5).suffix("×"));
        });
        ui.add_space(10.0);
        card(ui, "Interface size", "Bigger text and buttons for shop-floor screens (Expert's ribbon font sizes).", |ui| {
            ui.horizontal(|ui| {
                for (k, name) in crate::settings::UI_SIZES.iter().enumerate() {
                    if ui.add(egui::Button::new(*name).selected(self.cfg.ui_size as usize == k).min_size(egui::vec2(120.0, 30.0))).clicked() {
                        self.cfg.ui_size = k as u8;
                    }
                }
            });
        });
    }

    fn settings_nesting(&mut self, ui: &mut egui::Ui) {
        use crate::units::{drag_len, LenUnit};
        ui.label(RichText::new("Nesting").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Units", "Part gap, edge margin and moves on the Nesting screen are shown and typed in this unit. Inches can be typed as fractions (1/4, 1 1/2) or decimals; either unit can be typed with its suffix (6.35mm, 1/4in). DXF and DWG drawings that don't say their unit are read in it too.", |ui| {
            ui.horizontal(|ui| {
                for u in [LenUnit::Mm, LenUnit::Inch] {
                    ui.radio_value(&mut self.cfg.nesting.units, u, u.label());
                }
            });
        });
        ui.add_space(10.0);
        let u = self.cfg.nesting.units;
        card(ui, "New jobs start with", "The smallest distance between two parts, and the distance every part keeps from the sheet edge. A job keeps its own values (Nesting screen → Spacing).", |ui| {
            row(ui, "Part gap", |ui| {
                drag_len(ui, &mut self.cfg.nesting.part_gap, u, 50.0);
                ui.label(RichText::new(u.other(self.cfg.nesting.part_gap)).size(11.5).color(th().text_dim));
            });
            row(ui, "Edge margin", |ui| {
                drag_len(ui, &mut self.cfg.nesting.edge_margin, u, 100.0);
                ui.label(RichText::new(u.other(self.cfg.nesting.edge_margin)).size(11.5).color(th().text_dim));
            });
            ui.horizontal(|ui| {
                let same = self.spec.gap == self.cfg.nesting.part_gap && self.spec.margin == self.cfg.nesting.edge_margin;
                if ui.add_enabled(!same, egui::Button::new("Use for the open job too")).on_hover_text("Set the open job's part gap and edge margin to these (nest again afterwards)").clicked() {
                    self.checkpoint();
                    self.spec.gap = self.cfg.nesting.part_gap;
                    self.spec.margin = self.cfg.nesting.edge_margin;
                    self.stale = true;
                    self.refresh();
                    self.setui.note = "The open job now uses these — nest again to apply them to its sheets.".into();
                }
                ui.label(RichText::new(format!("Open job: gap {}, margin {}", u.show(self.spec.gap), u.show(self.spec.margin))).size(11.5).color(th().text_dim));
            });
        });
        ui.add_space(10.0);
        card(ui, "Moving parts by hand", "Select parts on the sheet (click, Shift/Ctrl + click, or drag a window on empty sheet), then: Ctrl + arrow keys slide them until they meet the next part at the part gap (or the margin); Alt + arrow keys move them exactly by the step — like the original, that move doesn't stop at other parts (the status line warns when they end up closer than the part gap). Lantek's own values are taken from its install the first time the app starts.", |ui| {
            row(ui, "Step (Alt + arrows)", |ui| {
                if drag_len(ui, &mut self.cfg.nesting.step, u, 1000.0).changed() {
                    self.cfg.nesting.step = self.cfg.nesting.step.max(0.001);
                }
                ui.label(RichText::new(u.other(self.cfg.nesting.step)).size(11.5).color(th().text_dim));
            });
            ui.checkbox(&mut self.cfg.nesting.snap, "Snap dragged parts to one part gap from a neighbour or to the margin (hold Alt while dragging to place freely)");
            ui.add_enabled_ui(self.cfg.nesting.snap, |ui| {
                row(ui, "Snap reach", |ui| {
                    ui.add(egui::DragValue::new(&mut self.cfg.nesting.snap_px).range(2.0..=60.0).speed(0.5).suffix(" px on screen"));
                });
            });
            ui.checkbox(&mut self.cfg.nesting.halo, "Show the part gap as a dashed outline around parts being dragged");
            ui.checkbox(&mut self.cfg.nesting.keys_clockwise, "Ctrl + 9 turns clockwise (off: counter-clockwise)");
        });
    }

    fn settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Shortcuts").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            // Expert v44's shortcut list has Copy, keeping the list's layout.
            if ui.button("Copy list").clicked() {
                let w = SHORTCUTS.iter().map(|s| s.0.len()).max().unwrap_or(0);
                let text: String = SHORTCUTS.iter().map(|(k, d)| format!("{k:<w$}  {d}\n")).collect();
                ui.ctx().copy_text(text);
                self.setui.note = "Shortcut list copied.".into();
            }
        });
        ui.add_space(8.0);
        card(ui, "Keyboard and mouse", "", |ui| {
            egui::Grid::new("shortcut_grid").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
                for (k, what) in SHORTCUTS {
                    egui::Frame::new().fill(th().hover).corner_radius(CornerRadius::same(5)).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                        ui.label(RichText::new(*k).monospace().size(12.0).color(th().text_strong));
                    });
                    ui.label(*what);
                    ui.end_row();
                }
            });
        });
    }

    fn settings_about(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("About").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Sheet nesting", "A rebuild of a sheet-metal CAD/CAM nesting system.", |ui| {
            row(ui, "Settings file", |ui| {
                ui.label(RichText::new(AppSettings::file().display().to_string()).monospace().size(11.5));
            });
            row(ui, "Machine in use", |ui| {
                ui.label(self.cfg.machine().name);
            });
            row(ui, "Database in use", |ui| {
                ui.label(self.cfg.conn().map_or("none".to_string(), |c| c.name.clone()));
            });
        });
        ui.add_space(10.0);
        card(ui, "Compared with the original", "", |ui| {
            ui.horizontal(|ui| {
                if ui.button("What's still missing in the Drawing module…").clicked() {
                    self.dv.show_missing = true;
                }
                if ui.button("Lantek v44 new features — what they'd take…").clicked() {
                    self.show_v44 = true;
                }
            });
        });
    }

    fn import_lantek_ui(&mut self) {
        let start = if self.cfg.lantek_dir.is_empty() { crate::settings::lantek_default_dir() } else { self.cfg.lantek_dir.clone() };
        if let Some(d) = rfd::FileDialog::new().set_title("Lantek Expert folder (with LANTEKIN.CNI)").set_directory(&start).pick_folder() {
            self.setui.note = self.cfg.import_lantek(&d.display().to_string());
            self.setui.sel_conn = self.cfg.active_conn;
            self.setui.sel_machine = self.cfg.active_machine;
        }
    }

    fn settings_connections(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Database connections").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            if ui.button("+ New connection").clicked() {
                self.cfg.connections.push(Connection::default());
                self.setui.sel_conn = self.cfg.connections.len() - 1;
                self.setui.pw_input.clear();
            }
            if ui.button("Import from Lantek…").on_hover_text("Add the database and machines of a Lantek Expert install, and its settings for moving parts by hand (step, turn direction, gap outline)").clicked() {
                self.import_lantek_ui();
            }
        });
        ui.label(RichText::new("Any SQL Server database — Lantek Expert's or another. The one marked ● opens in Browse database.").size(11.5).color(th().text_dim));
        ui.add_space(8.0);
        if self.cfg.connections.is_empty() {
            card(ui, "No connections yet", "Add one, or import the database a Lantek Expert install uses.", |_| {});
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for k in 0..self.cfg.connections.len() {
                let c = &self.cfg.connections[k];
                let sub = if c.server.is_empty() { "not set up".to_string() } else { format!("{} · {}", c.database, c.server) };
                if tile(ui, self.setui.sel_conn == k, self.cfg.active_conn == k, &c.name, &sub) {
                    self.setui.sel_conn = k;
                    self.setui.pw_input.clear();
                    self.setui.test.clear();
                }
            }
        });
        ui.add_space(10.0);
        let k = self.setui.sel_conn.min(self.cfg.connections.len() - 1);
        self.setui.sel_conn = k;
        let mut delete = false;
        let mut use_it = false;
        let mut browse = false;
        let mut test = false;
        card(ui, "Connection", "Where the database is and how to sign in.", |ui| {
            let c = &mut self.cfg.connections[k];
            row(ui, "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(280.0));
            });
            row(ui, "Server", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.server).desired_width(280.0).hint_text("host\\instance, host or host,port"));
            });
            row(ui, "Database", |ui| {
                ui.add(egui::TextEdit::singleline(&mut c.database).desired_width(280.0).hint_text("database name"));
            });
            row(ui, "Sign in with", |ui| {
                ui.radio_value(&mut c.windows, true, "My Windows account");
                ui.radio_value(&mut c.windows, false, "SQL Server login");
            });
            if !c.windows {
                row(ui, "User name", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut c.user).desired_width(200.0).hint_text("e.g. sa or lantek"));
                });
                row(ui, "Password", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.setui.pw_input).password(true).desired_width(200.0).hint_text(if c.password_enc.is_empty() { "password" } else { "remembered — type to change" }));
                    if ui.add_enabled(!self.setui.pw_input.is_empty(), egui::Button::new("Remember")).on_hover_text("Saved encrypted for your Windows account on this PC").clicked() {
                        match crate::settings::protect(&self.setui.pw_input) {
                            Ok(enc) => {
                                c.password_enc = enc;
                                self.setui.pw_input.clear();
                                self.setui.note = "Password remembered (encrypted for your Windows account).".into();
                            }
                            Err(e) => self.setui.note = e,
                        }
                    }
                    if !c.password_enc.is_empty() {
                        chip(ui, "remembered", th().ok_bg, th().ok_fg);
                        if ui.small_button("Forget").clicked() {
                            c.password_enc.clear();
                        }
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(conn_string(c)).monospace().size(11.0).color(th().text_dim));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let active = self.cfg.active_conn == k;
                if ui.add_enabled(!active, egui::Button::new(if active { "● In use" } else { "Use this connection" })).clicked() {
                    use_it = true;
                }
                if ui.add_enabled(self.setui.pending.is_none(), egui::Button::new("Test server")).on_hover_text("Checks the server answers (no sign-in)").clicked() {
                    test = true;
                }
                if ui.button("Open database…").on_hover_text("Sign in and browse jobs and tables").clicked() {
                    browse = true;
                }
                ui.add_space(24.0);
                if ui.button(RichText::new("Delete").color(th().warn_fg)).clicked() {
                    delete = true;
                }
            });
            if !self.setui.test.is_empty() {
                let col = if self.setui.test.starts_with('✓') { th().ok_fg } else { th().warn_fg };
                ui.label(RichText::new(&self.setui.test).color(col));
            }
        });
        if test {
            let c = self.cfg.connections[k].clone();
            let db = crate::dbconn::LantekDb { server: c.server, database: c.database, ..Default::default() };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::dbconn::test_reach(&db));
            });
            self.setui.pending = Some(rx);
            self.setui.test = "Testing…".into();
        }
        if use_it || browse {
            self.cfg.active_conn = k;
        }
        if browse {
            if !self.setui.pw_input.is_empty() {
                self.dbb.password = self.setui.pw_input.clone();
            }
            self.open_db_browser();
            self.dbb.open = true;
            self.db_open_now();
        }
        if delete {
            self.cfg.connections.remove(k);
            self.setui.sel_conn = k.saturating_sub(1);
            if self.cfg.active_conn >= self.cfg.connections.len() {
                self.cfg.active_conn = self.cfg.connections.len().saturating_sub(1);
            }
        }
    }

    fn settings_machines(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Machines & output").size(18.0).strong().color(th().text_strong));
            ui.add_space(16.0);
            if ui.button("+ New machine").clicked() {
                self.cfg.machines.push(Machine { name: format!("Machine {}", self.cfg.machines.len() + 1), ..Default::default() });
                self.setui.sel_machine = self.cfg.machines.len() - 1;
            }
            if ui.button("Import from Lantek…").clicked() {
                self.import_lantek_ui();
            }
        });
        ui.label(RichText::new("Each machine has its own program format. The one marked ● is used for Export CNC.").size(11.5).color(th().text_dim));
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for k in 0..self.cfg.machines.len() {
                let m = &self.cfg.machines[k];
                let sub = match m.output {
                    Output::Iso => "ISO G-code",
                    Output::BlmSiemens => "BLM · Siemens 840D",
                };
                if tile(ui, self.setui.sel_machine == k, self.cfg.active_machine == k, &m.name, sub) {
                    self.setui.sel_machine = k;
                }
            }
        });
        ui.add_space(10.0);
        let k = self.setui.sel_machine.min(self.cfg.machines.len() - 1);
        self.setui.sel_machine = k;
        let start = self.cfg.lantek_dir.clone();
        let mut delete = false;
        card(ui, "Machine", "", |ui| {
            let can_delete = self.cfg.machines.len() > 1;
            let active = self.cfg.active_machine == k;
            let m = &mut self.cfg.machines[k];
            row(ui, "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(280.0));
            });
            row(ui, "Program format", |ui| {
                for (o, title) in [(Output::Iso, "ISO G-code (.CNC) — most controls"), (Output::BlmSiemens, "BLM / Siemens 840D (.SPF set + XML, zipped)")] {
                    ui.radio_value(&mut m.output, o, title);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(154.0);
                if ui.add_enabled(!active, egui::Button::new(if active { "● In use" } else { "Use this machine" })).clicked() {
                    self.cfg.active_machine = k;
                }
                if can_delete && ui.button(RichText::new("Delete").color(th().warn_fg)).clicked() {
                    delete = true;
                }
            });
        });
        if delete {
            self.cfg.machines.remove(k);
            self.setui.sel_machine = k.saturating_sub(1);
            self.cfg.active_machine = self.cfg.active_machine.min(self.cfg.machines.len() - 1);
            return;
        }
        ui.add_space(10.0);
        let output = self.cfg.machines[k].output;
        if output == Output::Iso {
            card(ui, "Post library", "Block numbers, units, axis signs and file name. Load a .CFF / .CF1 library or edit the values.", |ui| {
                let m = &mut self.cfg.machines[k];
                file_row(ui, "Library file", &mut m.post_lib, ("Post library", &["CFF", "cff", "CF1", "cf1", "CF2", "cf2"]), &start);
                let editing_active = self.cfg.active_machine == k;
                if !editing_active {
                    ui.label(RichText::new("Use this machine to edit its values below.").size(11.5).color(th().text_dim));
                    return;
                }
                let p = &mut self.post;
                ui.add_space(4.0);
                row(ui, "Block prefix", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.prefix).desired_width(60.0));
                });
                row(ui, "First block / step", |ui| {
                    ui.add(egui::DragValue::new(&mut p.first).range(0..=99999));
                    ui.add(egui::DragValue::new(&mut p.step).range(1..=1000));
                });
                row(ui, "Digits", |ui| {
                    ui.add(egui::DragValue::new(&mut p.block_digits).range(1..=9));
                    let mut keep = p.zero_suppress == 0;
                    if ui.checkbox(&mut keep, "keep leading zeros (N00010)").changed() {
                        p.zero_suppress = if keep { 0 } else { 1 };
                    }
                });
                row(ui, "Units", |ui| {
                    ui.radio_value(&mut p.units, 1.0, "mm");
                    ui.radio_value(&mut p.units, 25.4, "inch");
                });
                row(ui, "Axes", |ui| {
                    let mut fx = p.sign_x < 0.0;
                    let mut fy = p.sign_y < 0.0;
                    if ui.checkbox(&mut fx, "flip X").changed() {
                        p.sign_x = if fx { -1.0 } else { 1.0 };
                    }
                    if ui.checkbox(&mut fy, "flip Y").changed() {
                        p.sign_y = if fy { -1.0 } else { 1.0 };
                    }
                });
                row(ui, "Circles", |ui| {
                    ui.checkbox(&mut p.circles_one_move, "full circle as one G2/G3");
                });
                row(ui, "File extension", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.extension).desired_width(80.0));
                });
            });
        } else {
            card(ui, "BLM / Siemens post", "The post's own settings files give the cut-off overruns, CORNER angle and arc tolerance.", |ui| {
                let m = &mut self.cfg.machines[k];
                file_row(ui, "Post settings (.PS1)", &mut m.blm_ps1, ("BLM post settings", &["PS1", "ps1"]), &start);
                file_row(ui, "Post library (.CF1)", &mut m.blm_cf1, ("BLM post library", &["CF1", "cf1"]), &start);
                let a = crate::blm::AdiParams::from_files(&m.blm_ps1, &m.blm_cf1);
                row(ui, "Values in use", |ui| {
                    ui.label(RichText::new(format!("overrun −{} / +{} mm · CORNER ≤ {}° · arcs within {} mm", a.trim_neg, a.trim_pos, a.corner_deg, a.arc_tol)).size(12.0));
                });
                row(ui, "Gas id / name", |ui| {
                    ui.add(egui::DragValue::new(&mut m.gas_id).range(0..=99));
                    ui.add(egui::TextEdit::singleline(&mut m.gas_name).desired_width(140.0));
                });
            });
        }
        ui.add_space(10.0);
        if self.cfg.active_machine == k {
            let sheet_no = self.cur_sheet + 1;
            let text = if self.blm.on {
                match (self.result.sheets.get(self.cur_sheet), self.plans.get(self.cur_sheet)) {
                    (Some(s), Some(pl)) => {
                        let set = crate::blm::sheet_set(sheet_no, &self.parts, s, &self.sheet_spec(sheet_no - 1), pl, self.cut_list(), &self.blm_now());
                        format!("{}  +  {}.XML\n\n{}", set.spf.iter().map(|f| f.0.clone()).collect::<Vec<_>>().join(", "), set.nest, set.spf[0].1.replace("\r\n", "\n"))
                    }
                    _ => "Nest a job to see its program.".into(),
                }
            } else {
                self.program.clone()
            };
            let name = self.cnc_name(sheet_no);
            card(ui, &format!("Preview — {name}"), "", |ui| {
                let preview: String = text.lines().take(18).collect::<Vec<_>>().join("\n");
                ui.add(egui::Label::new(RichText::new(preview).monospace().size(11.0)));
            });
        }
    }

    fn settings_materials(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Materials").size(18.0).strong().color(th().text_strong));
        ui.label(RichText::new("The name and code each material gets in machine programs and job sheets.").size(11.5).color(th().text_dim));
        ui.add_space(8.0);
        card(ui, "Material names and codes", "Also called: other names a material goes by — grades, norms, shop names. When an import, a program or a file name uses one of them, it means this material (as Lantek's aliases).", |ui| {
            egui::Grid::new("mat_codes").num_columns(5).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label(RichText::new("Material").strong());
                ui.label(RichText::new("Name in programs").strong());
                ui.label(RichText::new("Code").strong());
                ui.label(RichText::new("Machine table").strong()).on_hover_text(
                    "The material's key in the machine file's material and thickness sections: a catalogue number (0010 mild steel, 0100 stainless, 0200 pure aluminium, 0210 aluminium-magnesium, 0300 galvanised, 0510 copper, 0590 brass) or the name the file uses. Empty = the name in programs.",
                );
                ui.label(RichText::new("Also called (comma-separated)").strong());
                ui.end_row();
                for (i, m) in crate::cnc::MATERIALS.iter().enumerate() {
                    if self.cfg.materials.len() <= i {
                        self.cfg.materials.push(crate::settings::default_materials()[i].clone());
                    }
                    if self.cfg.material_aliases.len() <= i {
                        self.cfg.material_aliases.push(crate::settings::default_material_aliases()[i].clone());
                    }
                    if self.cfg.material_refs.len() <= i {
                        self.cfg.material_refs.push(crate::settings::default_material_refs()[i].clone());
                    }
                    ui.label(*m);
                    let (n, id) = &mut self.cfg.materials[i];
                    ui.add(egui::TextEdit::singleline(n).desired_width(160.0));
                    ui.add(egui::TextEdit::singleline(id).desired_width(80.0));
                    ui.add(egui::TextEdit::singleline(&mut self.cfg.material_refs[i]).desired_width(80.0).hint_text("name"));
                    ui.add(egui::TextEdit::singleline(&mut self.cfg.material_aliases[i]).desired_width(320.0));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Try a name:").size(11.5).color(th().text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.setui.mat_probe).desired_width(140.0).hint_text("e.g. S355"));
                if !self.setui.mat_probe.trim().is_empty() {
                    let t = match self.cfg.material_for(&self.setui.mat_probe) {
                        Some(k) => format!("→ {}", crate::cnc::MATERIALS[k]),
                        None => "→ not a known material".into(),
                    };
                    ui.label(RichText::new(t).size(11.5));
                }
            });
            ui.add_space(6.0);
            if ui.button("Reset to defaults").clicked() {
                self.cfg.materials = crate::settings::default_materials();
                self.cfg.material_aliases = crate::settings::default_material_aliases();
                self.cfg.material_refs = crate::settings::default_material_refs();
            }
        });
    }

    fn settings_folders(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Folders").size(18.0).strong().color(th().text_strong));
        ui.add_space(8.0);
        card(ui, "Export folder", "Where Export CNC writes programs (a sub-folder per job). Empty = Documents\\Sheet Nesting.", |ui| {
            row(ui, "Folder", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.export_dir).desired_width(380.0).hint_text("Documents\\Sheet Nesting"));
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.export_dir = d.display().to_string();
                    }
                }
                if !self.cfg.export_dir.is_empty() && ui.small_button("✕").clicked() {
                    self.cfg.export_dir.clear();
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Library folder", "This app's own product and job database. Put it on a shared drive and everyone in the shop sees the same products and jobs. Empty = Documents\\Sheet Nesting\\Library.", |ui| {
            row(ui, "Folder", |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.cfg.library_dir).desired_width(380.0).hint_text("Documents\\Sheet Nesting\\Library"));
                // Typed: it counts as another folder once the typing is done.
                if r.gained_focus() {
                    self.lib_dir_typed_from = Some(self.cfg.library_dir.clone());
                }
                if r.lost_focus() && self.lib_dir_typed_from.take().is_some_and(|d| d != self.cfg.library_dir) {
                    self.library_folder_changed();
                }
                if r.changed() {
                    self.libui.loaded = false;
                }
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.library_dir = d.display().to_string();
                        self.library_folder_changed();
                    }
                }
                if !self.cfg.library_dir.is_empty() && ui.small_button("✕").clicked() {
                    self.cfg.library_dir.clear();
                    self.library_folder_changed();
                }
            });
        });
        ui.add_space(10.0);
        card(ui, "Lantek Expert install (optional)", "Only used to import its database connection, machines and post files.", |ui| {
            row(ui, "Folder", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.cfg.lantek_dir).desired_width(380.0).hint_text("not used"));
                if ui.button("Import…").clicked() {
                    self.import_lantek_ui();
                }
            });
        });
    }
}

#[cfg(test)]
mod settings_ui_tests {
    use super::*;

    #[test]
    fn connection_string_masks_the_password() {
        let mut c = Connection { server: "srv".into(), database: "DB".into(), windows: false, user: "u".into(), password_enc: "0100abc".into(), ..Default::default() };
        assert_eq!(conn_string(&c), "DRIVER=SQL SERVER;SERVER=srv;DATABASE=DB;User ID=u;Password=••••••");
        c.windows = true;
        assert_eq!(conn_string(&c), "Provider=sqloledb;Data Source=srv;Initial Catalog=DB;Persist Security Info=False;Integrated Security=SSPI");
    }
}
