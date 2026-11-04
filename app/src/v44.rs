//! What Lantek Expert v44 added (Help\Docs\NewFeatures.pdf), sized by what it
//! would take to rebuild here, and the one that's built: the Nesting module's
//! "Verify Machining" command for the current sheet or all sheets, with the new
//! "Check safe distance" test (NewFeatures.pdf 3.1.4, 3.1.4.1).

use crate::nest::{self, Part, Placed, P};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Pos2, RichText, Stroke};

/// Rebuild size of a v44 feature.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Huge,
    Big,
    Medium,
    Small,
}

impl Size {
    pub fn label(self) -> &'static str {
        match self {
            Size::Huge => "HUGE",
            Size::Big => "Big",
            Size::Medium => "Medium",
            Size::Small => "Small",
        }
    }
}

/// Where the rebuild stands on a v44 feature.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Have {
    Built,
    Partly,
    No,
}

/// (PDF section, feature, size, have, what it means for the rebuild).
pub const FEATURES: &[(&str, &str, Size, Have, &str)] = &[
    ("4.4", "New Import Assistant", Size::Huge, Have::No, "A full editor before import: many files at once, layers with Cut / Mark / Do not import and colours, select by element, contour or window (right = touching, left = inside), invert, delete, undo/redo, dimensions and texts shown, Zoom All (A). Our DXF import only reads the file."),
    ("3.4.2", "Component extraction (stops, traps, lifts, magnetic cups)", Size::Huge, Have::No, "Automatic, manual and semi-automatic Stop / Trap / Lift instructions in the program, magnetic-cup activation with a 10 s timeout (LANTEKPR.CFG V145), movable lift strips with live collision display. Mostly for punch and combined machines; a laser only needs Manual Stop."),
    ("3.1.1", "Nesting parts grouped by attributes", Size::Huge, Have::Partly, "Groups parts by customer, order, delivery date, priority, reference, user data… into separate zones of the sheet with a gap between zones, a start corner and direction, and optional straight or irregular remnant lines between zones. Changes the nesting engine itself. Built: a Group attribute per part (Part Properties) and Home → Group by attribute: each group nested in its own zone after the last one, with the machine's zone gap. Not built: several attributes at once, start corner/direction, remnant lines between zones."),
    ("2.2.1", "Nano-joints", Size::Big, Have::Partly, "A micro-joint where the laser lowers its power and cuts part of the thickness; point or length, several qualities (cut tables), own colours (green), automatic at contour ends per material/thickness. We have micro-joints; nano-joints also need the machine's post to support them."),
    ("3.1.4", "Verify Machining command (current / all sheets)", Size::Big, Have::Built, "Side panel with progress and result, all sheets of the subjob with correct / incorrect counts, sheet status icons in the Nesting explorer, and the new safe-distance test between parts. Built: Machining -> Verify Machining."),
    ("4.1 / 4.3", "Part properties from the file name", Size::Medium, Have::Partly, "A file-name format such as [Reference]_[Quantity]_[Material] with separators and ignored text (*), stored in D018, re-read automatically when the format changes (DXF, DWG, PDF, DSTV, ESSI). Built: Settings → General → Part data from the file name; DXF imports take the reference and quantity from the name and warn when its material or thickness differs from the job's. Not built: re-reading when the format changes, other file types."),
    ("3.1.6", "Text / QR / template command with preview", Size::Medium, Have::Partly, "Marking and written texts, QR codes and templates in a side panel with an exact or quick preview on the sheet. We have marking texts (straight and curved); QR codes and templates are missing."),
    ("3.1.5", "Special pre-cutting cycles on lead-ins and bevels", Size::Medium, Have::No, "Pick a figure, pick the lead-ins, preview the cycle at each point, then apply."),
    ("3.2.1", "Multi-head: ascending order by number of heads", Size::Medium, Have::No, "Needs multi-torch groups first; then groups with fewer parts are cut first."),
    ("3.3.1 / 3.3.2 / 2.3.1", "Punch: remnant line direction, replacement punches, cycle offset", Size::Medium, Have::No, "Punch machines only: horizontal or vertical punching direction for the remnant line, swap a square/rectangle punch for any width and length, offset a cycle at a segment end."),
    ("2.1.1 / 2.1.2", "Select equal contours, inside / outside contours", Size::Small, Have::No, "Selection buttons: every contour equal to the picked one; all outside or all inside contours."),
    ("3.1.2 / 3.1.3", "Part Viewer: reference and size on tiles, Ctrl + scroll", Size::Small, Have::No, "Show each tile's reference and dimensions (on by default) and resize tiles with Ctrl + mouse wheel."),
    ("2.1.3", "Active / inactive machines", Size::Small, Have::No, "Inactive machines are grey italics and can't receive new orders or nestings."),
    ("1.1 / 1.2", "Edit several items, shortcut-key list", Size::Small, Have::No, "\"<Different values>\" when editing several parts or sheets at once; a list of shortcut keys with Copy."),
];

/// How far below the required distance still counts as respected (mm).
pub const SAFE_DIST_TOL: f64 = 0.01;

/// Verify Machining result codes, as the original numbers them; the panel
/// turns each code into its message (rule VM3; anything unknown reads as
/// "all executed"). A sheet is correct when its only code is 0 (rule VM4).
pub const VERIFY_OK: u8 = 0;
pub const ERR_OVERLAPS: u8 = 3;
pub const ERR_OUT_OF_ZONE: u8 = 4;
pub const ERR_NO_LEADS: u8 = 8;
pub const ERR_PARTS_FALL: u8 = 10;
/// Elements not (completely) cut because of the kerf compensation (KC18; the
/// test itself is ours).
pub const ERR_KERF: u8 = 13;
pub const ERR_SEPARATION: u8 = 20;
/// A check this rebuild adds that has no code in the original.
pub const OWN_CHECK: u8 = 255;

/// The original's message for a result code (rule VM3).
pub fn verify_message(code: u8) -> &'static str {
    match code {
        1 => "Not every machining of the sheet has been executed",
        2 => "One of the extractions has no lift data",
        3 => "The machining has overlaps",
        4 => "There is machining out of the work zone or in clamps",
        5 => "The overlap ratio of some punches are too large",
        6 => "There is shearing less than the minimum length",
        7 => "It is impossible to shear this material with this thickness",
        8 => "There are cut contours without lead-in/lead-outs.",
        9 => "There are punches or shearings on form tool hits",
        10 => "There are remnants or parts that fall",
        11 => "Invalid reposition",
        13 => "There are elements that have not been completely cut due to contour compensation",
        14 => "Some contours do not have enough micro-joints",
        15 => "Maximum head height exceeded",
        16 => "The contour has been machined with collisions",
        17 => "The number of pieces in a hole destruction has been exceeded",
        18 => "There are contours whose cuts can cause collisions",
        19 => "Collision avoidance has not been executed because there is machining of type:",
        20 => "Not enough separation between parts",
        OWN_CHECK => "Pierce check (this rebuild)",
        _ => "All machining on the sheet has been executed",
    }
}

/// Largest "Minimum distance" the panel accepts (mm) — the field's range is
/// 0 to this (rule VM1).
pub const MIN_DIST_MAX: f64 = 99999.999;

/// One problem on one sheet.
#[derive(Clone, Debug)]
pub struct Issue {
    /// The original's result code (see `verify_message`).
    pub code: u8,
    pub text: String,
    /// Where it is on the sheet (two points to join), when it has a place.
    pub at: Option<(P, P)>,
}

/// The result of Verify Machining.
#[derive(Clone, Debug, Default)]
pub struct NestVerify {
    pub open: bool,
    pub all_sheets: bool,
    pub check_safe: bool,
    pub by_value: bool,
    pub value: f64,
    /// One entry per sheet checked: (sheet index, issues).
    pub sheets: Vec<(usize, Vec<Issue>)>,
}

fn closest_on_seg(a: P, b: P, q: P) -> P {
    let v = [b[0] - a[0], b[1] - a[1]];
    let l2 = v[0] * v[0] + v[1] * v[1];
    if l2 < 1e-18 {
        return a;
    }
    let t = (((q[0] - a[0]) * v[0] + (q[1] - a[1]) * v[1]) / l2).clamp(0.0, 1.0);
    [a[0] + t * v[0], a[1] + t * v[1]]
}

fn dist(a: P, b: P) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Closest points between two closed contours.
pub fn contour_gap(a: &[P], b: &[P]) -> (f64, P, P) {
    let mut best = (f64::MAX, [0.0, 0.0], [0.0, 0.0]);
    for i in 0..a.len() {
        let (a0, a1) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            let (b0, b1) = (b[j], b[(j + 1) % b.len()]);
            if nest::seg_seg_dist(a0, a1, b0, b1) < 1e-9 {
                return (0.0, a0, b0);
            }
            for (p, q) in [(a0, closest_on_seg(b0, b1, a0)), (a1, closest_on_seg(b0, b1, a1)), (closest_on_seg(a0, a1, b0), b0), (closest_on_seg(a0, a1, b1), b1)] {
                let d = dist(p, q);
                if d < best.0 {
                    best = (d, p, q);
                }
            }
        }
    }
    best
}

/// Pairs of parts on a sheet closer than `need` mm (outline to outline; a
/// part sitting in another's hole is measured to that hole).
pub fn safe_distance_issues(parts: &[Part], sheet: &[Placed], need: f64) -> Vec<Issue> {
    let shapes: Vec<(Vec<P>, Vec<Vec<P>>)> = sheet.iter().map(|pl| nest::placed_shape(parts, pl)).collect();
    let mut out = vec![];
    for i in 0..sheet.len() {
        for j in i + 1..sheet.len() {
            let (a, b) = (&sheet[i], &sheet[j]);
            // Boxes further apart than the distance can't be too close.
            if a.x - (b.x + b.w) > need || b.x - (a.x + a.w) > need || a.y - (b.y + b.h) > need || b.y - (a.y + a.h) > need {
                continue;
            }
            let mut g = contour_gap(&shapes[i].0, &shapes[j].0);
            for h in &shapes[i].1 {
                let gh = contour_gap(h, &shapes[j].0);
                if gh.0 < g.0 {
                    g = gh;
                }
            }
            for h in &shapes[j].1 {
                let gh = contour_gap(&shapes[i].0, h);
                if gh.0 < g.0 {
                    g = gh;
                }
            }
            let nm = |pl: &Placed| parts[pl.part].name.clone();
            if g.0 <= 1e-9 {
                out.push(Issue { code: ERR_OVERLAPS, text: format!("{} and {} touch or overlap", nm(a), nm(b)), at: Some((g.1, g.2)) });
            } else if g.0 < need - SAFE_DIST_TOL {
                out.push(Issue { code: ERR_SEPARATION, text: format!("{} and {} are {:.2} mm apart (need {:.2})", nm(a), nm(b), g.0, need), at: Some((g.1, g.2)) });
            }
        }
    }
    out
}

/// Parts reaching outside the sheet (code 4).
pub fn out_of_zone_issues(parts: &[Part], sheet: &[Placed], w: f64, h: f64) -> Vec<Issue> {
    let mut out = vec![];
    for pl in sheet {
        let (o, _) = nest::placed_shape(parts, pl);
        if let Some(&q) = o.iter().find(|q| q[0] < -SAFE_DIST_TOL || q[1] < -SAFE_DIST_TOL || q[0] > w + SAFE_DIST_TOL || q[1] > h + SAFE_DIST_TOL) {
            out.push(Issue { code: ERR_OUT_OF_ZONE, text: format!("{} reaches outside the sheet", parts[pl.part].name), at: Some((q, q)) });
        }
    }
    out
}

/// Cut contours without a lead-in (code 8): one issue for the sheet.
pub fn no_lead_issues(plan: &crate::cnc::SheetPlan) -> Vec<Issue> {
    let n = plan.contours.iter().filter(|c| c.punch.is_none() && c.lead_path.len() < 2 && c.lead_in == c.points[0]).count();
    if n == 0 {
        return vec![];
    }
    vec![Issue { code: ERR_NO_LEADS, text: format!("{n} contour(s)"), at: None }]
}

impl App {
    /// The safe distance in force.
    pub fn safe_need(&self) -> f64 {
        let v = &self.nverify;
        if v.by_value {
            v.value
        } else {
            self.spec.gap
        }
    }

    /// Run Verify Machining on the current sheet or on every sheet.
    pub fn verify_nest(&mut self) {
        self.refresh();
        let all = self.nverify.all_sheets;
        let idx: Vec<usize> = if all { (0..self.result.sheets.len()).collect() } else { vec![self.cur_sheet.min(self.result.sheets.len().saturating_sub(1))] };
        let need = self.safe_need();
        let mut res = vec![];
        for k in idx {
            let (Some(sheet), Some(plan)) = (self.result.sheets.get(k), self.plans.get(k)) else { continue };
            let issues = crate::engine::verify_sheet(&self.parts, sheet, &self.sheet_spec(k), plan, self.nverify.check_safe.then_some(need));
            res.push((k, issues));
        }
        let bad = res.iter().filter(|r| !r.1.is_empty()).count();
        self.status = if res.len() == 1 {
            if bad == 0 { format!("Verify Machining: sheet {} is correct.", res[0].0 + 1) } else { format!("Verify Machining: sheet {} has {} problem(s).", res[0].0 + 1, res[0].1.len()) }
        } else {
            format!("Verify Machining: {} sheet(s) correct, {} incorrect.", res.len() - bad, bad)
        };
        self.nverify.sheets = res;
        self.nverify.open = true;
    }

    /// Ribbon group in the nesting Machining tab.
    pub fn ribbon_verify(&mut self, ui: &mut egui::Ui) {
        let has = !self.result.sheets.is_empty();
        ribbon_group(ui, "Verify", |ui| {
            let r = crate::drawing::dropdown(ui, Icon::Collision, "Verify\nMachining", has).on_hover_text("Check the machining of this sheet or of every sheet: pierces in the scrap, holes first, safe distance between parts");
            egui::Popup::menu(&r).show(|ui| {
                if ui.button("Verify machining in current sheet").clicked() {
                    self.nverify.all_sheets = false;
                    self.verify_nest();
                }
                if ui.button("Verify machining in all sheets").clicked() {
                    self.nverify.all_sheets = true;
                    self.verify_nest();
                }
            });
        });
    }

    /// Verify Machining side panel: result at the top, then the settings.
    pub fn nest_verify_window(&mut self, ctx: &egui::Context) {
        if !self.nverify.open {
            return;
        }
        let mut open = true;
        let mut rerun = false;
        let mut goto = None;
        egui::Window::new("Verify Machining").open(&mut open).anchor(egui::Align2::RIGHT_TOP, [-16.0, 140.0]).default_width(330.0).collapsible(false).show(ctx, |ui| {
            let v = &self.nverify;
            let bad = v.sheets.iter().filter(|s| !s.1.is_empty()).count();
            if v.sheets.len() > 1 {
                // The original's totals (rule VM4).
                ui.horizontal(|ui| {
                    chip(ui, &format!("Correct sheets: {}", v.sheets.len() - bad), th().ok_bg, th().ok_fg);
                    chip(ui, &format!("Incorrect sheets: {bad}"), if bad > 0 { th().warn_bg } else { th().hover }, if bad > 0 { th().warn_fg } else { th().text_dim });
                });
            } else {
                ui.label(RichText::new("Current sheet").strong());
            }
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (k, issues) in &v.sheets {
                    ui.horizontal(|ui| {
                        let (mark, col) = if issues.is_empty() { ("✔", th().ok_fg) } else { ("⚠", th().warn_fg) };
                        ui.label(RichText::new(mark).color(col));
                        if ui.link(format!("Sheet {}", k + 1)).clicked() {
                            goto = Some(*k);
                        }
                        ui.label(RichText::new(if issues.is_empty() { verify_message(VERIFY_OK).to_string() } else { format!("{} problem(s)", issues.len()) }).color(th().text_dim));
                    });
                    // Grouped by the original's result code, its message first.
                    let mut codes: Vec<u8> = issues.iter().map(|i| i.code).collect();
                    codes.sort_unstable();
                    codes.dedup();
                    for c in codes {
                        ui.label(RichText::new(format!("  ⚠ {}", verify_message(c))).size(11.5).color(th().warn_fg));
                        for i in issues.iter().filter(|i| i.code == c) {
                            ui.label(RichText::new(format!("      • {}", i.text)).size(11.0));
                        }
                    }
                }
            });
            ui.separator();
            let v = &mut self.nverify;
            ui.label(RichText::new("Sheet").strong());
            ui.checkbox(&mut v.check_safe, "Check safe distance");
            ui.add_enabled_ui(v.check_safe, |ui| {
                ui.horizontal(|ui| {
                    // The original's list: "Value", then "The separation of parts".
                    egui::ComboBox::from_id_salt("safe_kind").width(170.0).selected_text(if v.by_value { "Value" } else { "The separation of parts" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut v.by_value, true, "Value");
                        ui.selectable_value(&mut v.by_value, false, "The separation of parts");
                    });
                });
                // "Minimum distance": typed for Value; for the separation of
                // parts it shows that separation and can't be edited
                // (rule VM2).
                ui.horizontal(|ui| {
                    ui.label("Minimum distance");
                    if v.by_value {
                        ui.add(egui::DragValue::new(&mut v.value).range(0.0..=MIN_DIST_MAX).speed(0.1).suffix(" mm"));
                    } else {
                        let mut sep = self.spec.gap;
                        ui.add_enabled(false, egui::DragValue::new(&mut sep).suffix(" mm"));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.nverify.all_sheets, false, "Current sheet");
                ui.radio_value(&mut self.nverify.all_sheets, true, "All sheets");
                if ui.button("Verify").clicked() {
                    rerun = true;
                }
            });
        });
        if let Some(k) = goto {
            self.cur_sheet = k;
            self.refresh();
        }
        if rerun {
            self.verify_nest();
        }
        if !open {
            self.nverify.open = false;
        }
    }

    /// Red marks between parts that are too close, on the shown sheet.
    pub fn verify_canvas(&self, painter: &egui::Painter, to_s: &dyn Fn(P) -> Pos2) {
        if !self.nverify.open {
            return;
        }
        let Some((_, issues)) = self.nverify.sheets.iter().find(|s| s.0 == self.cur_sheet) else { return };
        let red = egui::Color32::from_rgb(230, 50, 50);
        for (a, b) in issues.iter().filter_map(|i| i.at) {
            let (pa, pb) = (to_s(a), to_s(b));
            painter.line_segment([pa, pb], Stroke::new(2.5, red));
            let c = pa + (pb - pa) * 0.5;
            painter.circle_stroke(c, 9.0, Stroke::new(2.0, red));
        }
    }

    /// The v44 New Features window: what each feature would take.
    pub fn v44_window(&mut self, ctx: &egui::Context) {
        if !self.show_v44 {
            return;
        }
        let mut open = true;
        egui::Window::new("Lantek Expert v44 — new features and what they'd take").open(&mut open).default_size([640.0, 600.0]).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new("From the original's Help\\Docs\\NewFeatures.pdf, largest first.").size(11.5).color(th().text_dim));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (sec, name, size, have, note) in FEATURES {
                    ui.horizontal(|ui| {
                        let (bg, fg) = match size {
                            Size::Huge => (th().warn_bg, th().warn_fg),
                            Size::Big => (th().accent_soft, th().accent_fg),
                            _ => (th().hover, th().text_dim),
                        };
                        chip(ui, size.label(), bg, fg);
                        ui.label(RichText::new(*name).strong().color(th().text_strong));
                        ui.label(RichText::new(*sec).size(11.0).color(th().text_dim));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match have {
                            Have::Built => chip(ui, "built", th().ok_bg, th().ok_fg),
                            Have::Partly => chip(ui, "partly", th().accent_soft, th().accent_fg),
                            Have::No => chip(ui, "not yet", th().hover, th().text_dim),
                        });
                    });
                    ui.label(RichText::new(*note).size(11.5));
                    ui.add_space(6.0);
                }
            });
        });
        if !open {
            self.show_v44 = false;
        }
    }
}

#[cfg(test)]
mod v44_tests {
    use super::*;
    use crate::nest::Shape;

    fn part(name: &str) -> Part {
        Part { name: name.into(), shape: Shape::Rectangle, w: 100.0, h: 50.0, qty: 2, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }
    }

    #[test]
    fn finds_parts_closer_than_the_safe_distance() {
        let parts = vec![part("A")];
        let pl = |x: f64| Placed { part: 0, x, y: 0.0, w: 100.0, h: 50.0, turns: 0, mirror: false };
        // 3 mm apart: fine for 3, too close for 5.
        let sheet = vec![pl(0.0), pl(103.0)];
        assert!(safe_distance_issues(&parts, &sheet, 3.0).is_empty());
        let bad = safe_distance_issues(&parts, &sheet, 5.0);
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].code, ERR_SEPARATION);
        assert_eq!(verify_message(bad[0].code), "Not enough separation between parts");
        // Touching parts are an overlap (code 3).
        assert_eq!(safe_distance_issues(&parts, &[pl(0.0), pl(100.0)], 5.0)[0].code, ERR_OVERLAPS);
        assert_eq!(verify_message(VERIFY_OK), "All machining on the sheet has been executed");
        // A part hanging off a 150 mm sheet is out of the work zone (code 4).
        assert_eq!(out_of_zone_issues(&parts, &[pl(0.0), pl(103.0)], 150.0, 100.0).len(), 1);
        let (a, b) = bad[0].at.unwrap();
        assert!((dist(a, b) - 3.0).abs() < 1e-6);
        // Far apart: never checked in detail.
        assert!(safe_distance_issues(&parts, &[pl(0.0), pl(500.0)], 5.0).is_empty());
    }

    #[test]
    fn the_list_puts_huge_features_first_and_marks_verify_built() {
        assert_eq!(FEATURES[0].2, Size::Huge);
        assert!(FEATURES.iter().any(|f| f.1.starts_with("Verify Machining") && f.3 == Have::Built));
    }
}
