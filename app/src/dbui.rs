//! The Database ribbon group: connection settings and the database browser.

use crate::settings_ui::Section;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;

impl App {
    /// Ribbon group (Jobs screen): Database.
    pub fn ribbon_database(&mut self, ui: &mut egui::Ui) {
        ribbon_group(ui, "Database", |ui| {
            let on = self.setui.open && self.setui.section == Section::Connections;
            if ribbon_button(ui, Icon::Plug, "Connections", on, true).on_hover_text("Databases this app can open — any SQL Server, any sign-in").clicked() {
                self.open_db_window();
            }
            if ribbon_button(ui, Icon::Database, "Browse\ndatabase", self.dbb.open, true).on_hover_text("Open the chosen database read-only: jobs, nestings, parts and every table").clicked() {
                self.open_db_browser();
            }
        });
    }

    /// Settings at Connections.
    pub fn open_db_window(&mut self) {
        self.open_settings(Section::Connections);
    }
}
