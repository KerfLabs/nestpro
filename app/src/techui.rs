//! The part Technology tab, laid out like the original's drawing module:
//! Lead-in/Lead-outs, Micro-Joints, Loops and Cut Technology, each with
//! Automatic and Delete. The rules it follows are in MECHANICS.md.

use crate::engine::TechEdit;
use crate::nest::{LeadType, PartTech};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

impl App {
    /// Parts the Technology buttons act on: the selected one, or all.
    pub(crate) fn tech_targets(&self) -> Vec<usize> {
        if self.tech_all {
            (0..self.parts.len()).collect()
        } else {
            self.sel_part.filter(|&i| i < self.parts.len()).into_iter().collect()
        }
    }

    /// Automatic lead-ins: placed by contour class (leads.rs), with the chosen
    /// type (by default the machine's tables) and position.
    pub fn auto_leads(&mut self) {
        let e = TechEdit::AutoLeads { lead_type: self.lead_choice, position: self.lead_pos };
        self.edit_tech("Automatic lead-ins", |t, b| e.apply(t, b));
    }

    /// The active machine's technology file (CT1), or the BLM file's values.
    pub fn machine_tech(&self) -> crate::ct1::MachineTech {
        crate::ct1::MachineTech::for_machine(&self.cfg.machine(), &self.cfg.lantek_dir)
    }

    pub fn machine_tech_tip(&self) -> String {
        let mt = self.machine_tech();
        let q = mt.leads.quality(1);
        let classes: Vec<String> = crate::leads::CLASS_NAMES.iter().zip(q.recs.iter()).map(|(n, r)| format!("{n}: type {} ({} / {} mm)", r.kind, r.d_in, r.d_out)).collect();
        format!(
            "The machine's lead-ins: {} mm in, {} mm out, {} mm overlap past the start, position {} ({}).\nTables by contour class (quality 1): {}",
            mt.lead_in,
            mt.lead_out,
            mt.overlap,
            mt.zone,
            mt.source.as_ref().map_or("built-in BLM values".to_string(), |p| p.display().to_string()),
            classes.join(", ")
        )
    }

    /// Lead-in, lead-out, overlap and position from the machine's CT1
    /// (E044, E045, B156, E000).
    pub fn apply_machine_leads(&mut self) {
        let mt = self.machine_tech();
        self.lead_pos = mt.zone;
        let e = TechEdit::MachineLeads(mt);
        self.edit_tech("Machine lead-ins", |t, b| e.apply(t, b));
    }

    /// Automatic micro-joints: one per the machine's maximum distance (F150, by
    /// the part's quality) on every contour the joint type reaches, holes too,
    /// within the size class's fewest/most (Settings → count by size).
    pub fn auto_joints_now(&mut self) {
        let mt = self.machine_tech();
        let e = TechEdit::AutoJoints { dist: mt.joint_dist, gap: mt.joint_gap, every: None };
        self.edit_tech("Automatic micro-joints", |t, b| e.apply(t, b));
    }

    /// Loops at sharp outer corners (toggles them).
    pub fn auto_loops(&mut self) {
        let on = self.tech_targets().first().and_then(|&i| self.parts[i].tech).is_some_and(|t| t.loops);
        let e = TechEdit::Loops(!on);
        self.edit_tech(if on { "Loops removed" } else { "Loops added at sharp corners" }, |t, b| e.apply(t, b));
    }

    pub(crate) fn edit_tech(&mut self, what: &str, f: impl Fn(&mut Option<PartTech>, (f64, f64))) {
        let targets = self.tech_targets();
        if targets.is_empty() {
            self.status = "Select a part first (or tick 'All parts').".into();
            return;
        }
        self.checkpoint();
        for &i in &targets {
            let bbox = self.parts[i].bbox();
            f(&mut self.parts[i].tech, bbox);
        }
        self.refresh();
        let who = if targets.len() == 1 { format!("'{}'", self.parts[targets[0]].name) } else { format!("{} parts", targets.len()) };
        self.status = format!("{what} for {who}.");
    }

    pub fn ribbon_part_tech(&mut self, ui: &mut egui::Ui) {
        let has = !self.tech_targets().is_empty();
        let in_part = self.workspace == crate::Workspace::Part;
        let picked = in_part && !self.sel_elem_info.is_empty();
        ribbon_group(ui, "Lead-in/Lead-outs", |ui| {
            if ribbon_button_ex(ui, Icon::LeadIn, "Automatic", false, has, true)
                .on_hover_text("Leads by contour class, from the machine's tables by default: round holes and parts at the position's angle, rectangles at its corner or nearest side, other shapes at the nearest valid corner, else the middle of the nearest long-enough line")
                .clicked()
            {
                self.auto_leads();
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, has).clicked() {
                self.edit_tech("Lead-ins removed", |t, b| TechEdit::DeleteLeads.apply(t, b));
            }
            if ribbon_button(ui, Icon::Program, "Machine", false, has)
                .on_hover_text(self.machine_tech_tip())
                .clicked()
            {
                self.apply_machine_leads();
            }
            if in_part
                && ribbon_button(ui, Icon::CutPath, "On element", false, picked)
                    .on_hover_text("Click a line or arc on the part, then put the lead-in in its middle")
                    .clicked()
            {
                self.lead_on_element();
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Type").size(11.0).color(th().text_dim));
                egui::ComboBox::from_id_salt("lead_type").width(96.0).selected_text(self.lead_choice.label()).show_ui(ui, |ui| {
                    for lt in LeadType::ALL {
                        ui.selectable_value(&mut self.lead_choice, lt, lt.label());
                    }
                });
            });
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Position").size(11.0).color(th().text_dim));
                self.position_pad(ui);
            });
        });

        ribbon_group(ui, "Micro-Joints", |ui| {
            if ribbon_button(ui, Icon::Joint, "Automatic", false, has)
                .on_hover_text("One joint per the machine's maximum distance (100 mm by default) on every contour, holes too, rounded up to an even number, each centred on its line or arc; within the size class's fewest/most")
                .clicked()
            {
                self.auto_joints_now();
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, has).clicked() {
                self.edit_tech("Micro-joints removed", |t, b| TechEdit::DeleteJoints.apply(t, b));
            }
            if in_part
                && ribbon_button(ui, Icon::CutPath, "On element", false, picked)
                    .on_hover_text("A micro-joint in the middle of each selected line or arc")
                    .clicked()
            {
                self.joint_on_elements();
            }
            if ribbon_button(ui, Icon::Grid, "Settings", self.show_joint_sizes, true).on_hover_text("Joint type (plain gap, with leads, at corners), gap width, lead distances and angle, and the automatic count by size").clicked() {
                if !self.show_joint_sizes {
                    self.auto_joints = crate::ct1::active().joint_sizes;
                }
                self.show_joint_sizes = !self.show_joint_sizes;
            }
        });
        ribbon_group(ui, "Loops", |ui| {
            if ribbon_button(ui, Icon::Loop, "Automatic", false, has)
                .on_hover_text("Small loops outside sharp outer corners (90° or less)")
                .clicked()
            {
                self.auto_loops();
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, has).clicked() {
                self.edit_tech("Loops removed", |t, b| TechEdit::Loops(false).apply(t, b));
            }
        });
        ribbon_group(ui, "Cut Technology", |ui| {
            if ribbon_button(ui, Icon::CutTech, "Automatic", false, has)
                .on_hover_text("Give the part its own technology with the defaults")
                .clicked()
            {
                self.edit_tech("Own cut technology", |t, b| TechEdit::Own.apply(t, b));
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, has)
                .on_hover_text("Back to the sheet's automatic technology")
                .clicked()
            {
                self.edit_tech("Technology removed (automatic again)", |t, b| TechEdit::Delete.apply(t, b));
            }
            if ribbon_button(ui, Icon::Grid, "By size", self.show_cut_ranges, true)
                .on_hover_text("Cut quality by contour size: e.g. small holes with fine quality (the original's automatic cut technology list)")
                .clicked()
            {
                self.show_cut_ranges = !self.show_cut_ranges;
            }
        });
        // Texts: in the original a text is an element with a type (marking,
        // special or plain note) and gets the marking technology; see MECHANICS.md.
        ribbon_group(ui, "Text", |ui| {
            ui.vertical(|ui| {
                // Three compact rows fill exactly a big button's height, so
                // this group's title lines up with the others.
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.spacing_mut().interact_size.y = 21.0;
                ui.spacing_mut().button_padding.y = 1.0;
                ui.add(egui::TextEdit::singleline(&mut self.text_draft).desired_width(118.0).hint_text("Text or #REF"))
                    .on_hover_text(format!("Special texts fill in: {}", crate::nest::TEXT_PARAMS.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(", ")));
                egui::ComboBox::from_id_salt("text_kind").width(118.0).selected_text(short_kind(self.text_kind)).show_ui(ui, |ui| {
                    for k in crate::nest::TextKind::ALL {
                        ui.selectable_value(&mut self.text_kind, k, k.label());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Height").size(11.0).color(th().text_dim));
                    ui.add(egui::DragValue::new(&mut self.text_height).range(2.0..=100.0).speed(0.2).suffix(" mm"));
                    ui.label(RichText::new("Arc").size(11.0).color(th().text_dim));
                    ui.add(egui::DragValue::new(&mut self.text_curve).range(-2000.0..=2000.0).speed(0.5).suffix(" r"))
                        .on_hover_text("Bend the text along an arc of this radius (mm). 0 = straight, positive = over the top of a circle, negative = round the bottom. Or select a line, arc or circle and press On element.");
                });
            });
            let in_part = self.workspace == crate::Workspace::Part;
            let on_elem = in_part && !self.sel_elems.is_empty();
            let tip = if on_elem {
                "Put the text next to the selected line (along it) or arc/circle (curved round it), inside the material"
            } else if in_part {
                "Click on the part to place the text (tip: select a line or arc first to place it relative to it)"
            } else {
                "Add the text along the bottom of the part"
            };
            let label = if on_elem { "On element" } else if in_part { "Place" } else { "Add" };
            if ribbon_button(ui, Icon::Numbers, label, self.placing_text, has && !self.text_draft.trim().is_empty())
                .on_hover_text(tip)
                .clicked()
            {
                if on_elem {
                    self.text_on_element();
                } else if in_part {
                    self.placing_text = !self.placing_text;
                    self.status = if self.placing_text { "Click on the part where the text should start.".into() } else { String::new() };
                } else {
                    let (txt, h, kind) = (self.text_draft.clone(), self.text_height, self.text_kind);
                    self.add_text_to_targets(&txt, h, kind);
                }
            }
            if ribbon_button(ui, Icon::Remove, "Delete", false, has).on_hover_text("Remove all texts from the part").clicked() {
                let targets = self.tech_targets();
                self.checkpoint();
                for i in targets {
                    self.parts[i].texts.clear();
                }
                self.refresh();
                self.status = "Texts removed.".into();
            }
        });
        ribbon_group(ui, "Apply to", |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.spacing_mut().interact_size.y = 21.0;
                ui.add_space(2.0);
                if ui.radio(!self.tech_all, "Selected part").clicked() {
                    self.tech_all = false;
                }
                if ui.radio(self.tech_all, "All parts").clicked() {
                    self.tech_all = true;
                }
                let sel = self.sel_part.and_then(|i| self.parts.get(i)).map_or("—".to_string(), |p| p.name.clone());
                ui.label(RichText::new(sel).size(11.0).color(th().text_dim));
            });
        });
    }

    /// 3 × 3 keypad for the lead position (7 8 9 / 4 5 6 / 1 2 3).
    fn position_pad(&mut self, ui: &mut egui::Ui) {
        let cell = 15.0;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(cell * 3.0 + 4.0), Sense::hover());
        let p = ui.painter();
        p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, th().border), egui::StrokeKind::Inside);
        for row in 0..3 {
            for col in 0..3 {
                let pos = (2 - row) as u8 * 3 + col as u8 + 1;
                let cr = Rect::from_min_size(r.min + Vec2::new(2.0 + col as f32 * cell, 2.0 + row as f32 * cell), Vec2::splat(cell));
                let resp = ui.interact(cr, ui.scope_id().with(("pos", pos)), Sense::click());
                let on = self.lead_pos == pos;
                let fill = if on { th().accent } else if resp.hovered() { th().hover } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(cr.shrink(1.5), CornerRadius::same(3), fill);
                ui.painter().circle_filled(cr.center(), 2.0, if on { Color32::WHITE } else { th().text_dim });
                if resp.on_hover_text(format!("Position {pos}")).clicked() {
                    self.lead_pos = pos;
                }
            }
        }
    }

    /// Settings for automatic micro-joints by contour size.
    pub fn joint_sizes_window(&mut self, ctx: &egui::Context) {
        self.cut_ranges_window(ctx);
        if !self.show_joint_sizes {
            return;
        }
        let mut open = true;
        let targets = self.tech_targets();
        let cur = targets.first().and_then(|&i| self.parts[i].tech).unwrap_or_default();
        let mut edit = cur;
        let mt = self.machine_tech();
        let sizes_before = self.auto_joints;
        egui::Window::new("Micro-joints").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            // Joint type and its leads (the original's micro-joint types and joint leads).
            ui.label(RichText::new("Type and leads").strong());
            ui.add_enabled_ui(!targets.is_empty(), |ui| {
                egui::Grid::new("jt").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Type");
                    egui::ComboBox::from_id_salt("joint_type").width(190.0).selected_text(edit.joint_type.label()).show_ui(ui, |ui| {
                        for jt in crate::nest::JointType::ALL {
                            ui.selectable_value(&mut edit.joint_type, jt, jt.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Gap width");
                    ui.add(egui::DragValue::new(&mut edit.joint_width).range(0.05..=10.0).speed(0.05).suffix(" mm"));
                    ui.end_row();
                    let leads = edit.joint_type.has_leads();
                    ui.label("Lead-in distance").on_hover_text("How far off the contour the lead-in starts (perpendicular distance)");
                    ui.add_enabled(leads, egui::DragValue::new(&mut edit.joint_lead).range(0.0..=50.0).speed(0.1).suffix(" mm"));
                    ui.end_row();
                    let mut out = edit.joint_lead_out.unwrap_or(edit.joint_lead);
                    ui.label("Lead-out distance").on_hover_text("0 = no lead-out: the beam just stops at the gap");
                    if ui.add_enabled(leads, egui::DragValue::new(&mut out).range(0.0..=50.0).speed(0.1).suffix(" mm")).changed() {
                        edit.joint_lead_out = Some(out);
                    }
                    ui.end_row();
                    ui.label("Lead angle").on_hover_text("Inclined leads only; perpendicular leads ignore it");
                    ui.add_enabled(edit.joint_type == crate::nest::JointType::LeadsInclined || edit.joint_type == crate::nest::JointType::Corner, egui::DragValue::new(&mut edit.joint_angle).range(1.0..=90.0).speed(0.5).suffix("°"));
                    ui.end_row();
                    ui.label("One every").on_hover_text("Automatic joints: one per this many mm of each contour, holes too (the machine's maximum distance, CT1 F150). 0 = the fixed number of joints on the outside");
                    ui.add(egui::DragValue::new(&mut edit.joint_every).range(0.0..=5000.0).speed(1.0).suffix(" mm"));
                    ui.end_row();
                });
                if ui
                    .button("Machine values")
                    .on_hover_text(format!(
                        "From the machine's technology file: type {}, gap {} mm, lead-in {} mm at {}°, lead-out {} mm, one joint per {} mm ({})",
                        mt.joint_type,
                        mt.joint_gap,
                        mt.joint_lead,
                        mt.joint_angle,
                        mt.joint_lead_out,
                        mt.joint_every,
                        mt.source.as_ref().map_or("built-in BLM values".to_string(), |p| p.display().to_string())
                    ))
                    .clicked()
                {
                    edit.joint_width = mt.joint_gap;
                    edit.joint_lead = mt.joint_lead;
                    edit.joint_lead_out = Some(mt.joint_lead_out);
                    edit.joint_angle = mt.joint_angle;
                    edit.joint_every = mt.joint_every;
                    if let Some(jt) = crate::nest::JointType::from_code(mt.joint_type) {
                        edit.joint_type = jt;
                    }
                }
                ui.label(
                    RichText::new("With leads, each joint cuts out into the scrap, stops, re-pierces and cuts back in; the leads lean away from the gap — drag the red handle on a joint in the Part view to change them. Each gap sits on one line or arc longer than the gap; a joint next to a corner goes on the corner.")
                        .size(11.0)
                        .color(th().text_dim),
                );
            });
            ui.separator();
            ui.label(RichText::new("Automatic count by size").strong());
            ui.label(RichText::new("A contour is small if its box is within the small size, large if it reaches the large size in both directions, otherwise medium. Each class keeps the count between its fewest and most (blank = the general limits).").size(12.0));
            ui.add_space(6.0);
            let c = &mut self.auto_joints;
            egui::Grid::new("aj").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new("X").strong());
                ui.label(RichText::new("Y").strong());
                ui.end_row();
                ui.label("Small up to");
                ui.add(egui::DragValue::new(&mut c.small[0]).range(0.0..=99999.999).suffix(" mm"));
                ui.add(egui::DragValue::new(&mut c.small[1]).range(0.0..=99999.999).suffix(" mm"));
                ui.end_row();
                ui.label("Large from");
                ui.add(egui::DragValue::new(&mut c.large[0]).range(0.0..=99999.999).suffix(" mm"));
                ui.add(egui::DragValue::new(&mut c.large[1]).range(0.0..=99999.999).suffix(" mm"));
                ui.end_row();
                ui.label("");
                ui.label(RichText::new("Fewest").strong());
                ui.label(RichText::new("Most").strong());
                ui.end_row();
                for (k, name) in ["Parts · small", "Parts · medium", "Parts · large", "Holes · small", "Holes · medium", "Holes · large"].iter().enumerate() {
                    ui.label(*name);
                    let pair = if k < 3 { &mut c.parts[k] } else { &mut c.holes[k - 3] };
                    ui.add(egui::DragValue::new(&mut pair[0]).range(-1..=999).custom_formatter(|v, _| if v < 0.0 { "—".into() } else { format!("{v:.0}") }));
                    ui.add(egui::DragValue::new(&mut pair[1]).range(-1..=99999).custom_formatter(|v, _| if v < 0.0 { "—".into() } else { format!("{v:.0}") }));
                    ui.end_row();
                }
                ui.label("General");
                ui.add(egui::DragValue::new(&mut c.min).range(0..=999));
                ui.add(egui::DragValue::new(&mut c.max).range(0..=99999));
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Original defaults").on_hover_text("Every contour medium, 0 to 99999 joints").clicked() {
                    self.auto_joints = crate::nest::AutoJoints::default();
                }
                if ui.button("Machine values").on_hover_text("The size classes of the machine's technology file (F121, F122, F136–F149)").clicked() {
                    self.auto_joints = mt.joint_sizes;
                }
            });
        });
        // The size rules edited here act on the active machine's tables.
        if self.auto_joints != sizes_before {
            let mut a = crate::ct1::active();
            a.joint_sizes = self.auto_joints;
            crate::ct1::set_active(a);
        }
        if !open {
            self.show_joint_sizes = false;
        }
        if edit != cur && !targets.is_empty() {
            let (jt, w, l, a, every) = (edit.joint_type, edit.joint_width, edit.joint_lead, edit.joint_angle, edit.joint_every);
            self.edit_tech(&format!("Micro-joints: {}", jt.label().to_lowercase()), |t, _| {
                let mut x = t.unwrap_or_default();
                x.joint_type = jt;
                x.joint_width = w;
                x.joint_lead = l;
                x.joint_angle = a;
                x.joint_every = every;
                *t = Some(x);
            });
        }
    }
}

impl App {
    /// Cut quality by contour size (CT1): each contour's equivalent radius √(area / π) picks the first
    /// matching row; contours outside every row keep the part's quality.
    pub fn cut_ranges_window(&mut self, ctx: &egui::Context) {
        if !self.show_cut_ranges {
            return;
        }
        let mut open = true;
        let targets = self.tech_targets();
        let cur = targets.first().and_then(|&i| self.parts[i].tech).unwrap_or_default();
        let mut edit = cur;
        // Contour sizes of the first target, to show which row each one takes.
        let sizes: Vec<(bool, f64)> = targets
            .first()
            .map(|&i| {
                let g = self.parts[i].geometry();
                let area = |p: &[crate::nest::P]| (0..p.len()).map(|k| p[k][0] * p[(k + 1) % p.len()][1] - p[(k + 1) % p.len()][0] * p[k][1]).sum::<f64>() / 2.0;
                g.holes.iter().map(|h| (true, crate::nest::equivalent_radius(area(h)))).chain(std::iter::once((false, crate::nest::equivalent_radius(area(&g.outline))))).collect()
            })
            .unwrap_or_default();
        egui::Window::new("Cut technology by size").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new("Each contour's size is the radius of a circle with the same area. The first ticked row it falls in sets its quality (from ≤ r < up to).").size(12.0));
            ui.add_space(6.0);
            ui.add_enabled_ui(!targets.is_empty(), |ui| {
                egui::Grid::new("cut_ranges").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("");
                    ui.label(RichText::new("From r").strong());
                    ui.label(RichText::new("Up to r").strong());
                    ui.label(RichText::new("Quality").strong());
                    ui.end_row();
                    for (k, row) in edit.cut_ranges.iter_mut().enumerate() {
                        ui.checkbox(&mut row.on, format!("Row {}", k + 1));
                        ui.add(egui::DragValue::new(&mut row.min_r).range(0.0..=99999.0).speed(0.1).suffix(" mm"));
                        ui.add(egui::DragValue::new(&mut row.max_r).range(0.0..=99999.0).speed(0.1).suffix(" mm"));
                        egui::ComboBox::from_id_salt(("cut_q", k)).width(80.0).selected_text(row.quality.label()).show_ui(ui, |ui| {
                            for q in crate::nest::CutQuality::ALL {
                                ui.selectable_value(&mut row.quality, q, q.label());
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            if !sizes.is_empty() {
                ui.separator();
                let part_q = edit.quality;
                for (hole, r) in &sizes {
                    let q = crate::nest::quality_for_radius(&edit.cut_ranges, *r, 0.0);
                    ui.label(
                        RichText::new(format!("{} r {:.1} mm → {}{}", if *hole { "Hole" } else { "Outside" }, r, q.unwrap_or(part_q).label(), if q.is_some() { " (by size)" } else { "" }))
                            .size(11.5)
                            .color(if q.is_some() { th().text_strong } else { th().text_dim }),
                    );
                }
            }
            ui.add_space(4.0);
            if ui.button("Example: small holes fine").on_hover_text(format!("Row 1: holes up to r {} mm cut with Fine quality", crate::nest::SMALL_HOLE_RADIUS_EXAMPLE)).clicked() {
                edit.cut_ranges = Default::default();
                edit.cut_ranges[0].on = true;
            }
        });
        if !open {
            self.show_cut_ranges = false;
        }
        if edit.cut_ranges != cur.cut_ranges && !targets.is_empty() {
            let ranges = edit.cut_ranges;
            self.edit_tech("Cut technology by size", |t, _| {
                let mut x = t.unwrap_or_default();
                x.cut_ranges = ranges;
                *t = Some(x);
            });
        }
    }
}

/// Small preview text helper for the position pad tooltip (kept for clarity).
#[allow(dead_code)]
fn keypad_label(pos: u8, r: Rect, p: &egui::Painter) {
    p.text(r.center(), Align2::CENTER_CENTER, pos.to_string(), FontId::proportional(9.0), th().text_dim);
    let _ = Pos2::ZERO;
}

fn short_kind(k: crate::nest::TextKind) -> &'static str {
    match k {
        crate::nest::TextKind::Mark => "Marking text",
        crate::nest::TextKind::Special => "Special text",
        crate::nest::TextKind::Note => "Note only",
    }
}

impl App {
    /// Add a text along the bottom of each target part, centred (shrunk to fit).
    pub fn add_text_to_targets(&mut self, txt: &str, height: f64, kind: crate::nest::TextKind) {
        let targets = self.tech_targets();
        if targets.is_empty() {
            self.status = "Select a part first (or tick 'All parts').".into();
            return;
        }
        self.checkpoint();
        for &i in &targets {
            crate::engine::add_text(&mut self.parts[i], txt, height, kind);
        }
        self.refresh();
        self.status = format!("Text '{txt}' added to {} part(s) as {}.", targets.len(), kind.label().to_lowercase());
    }
}
