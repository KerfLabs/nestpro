//! The Nesting module's "Verify Machining" command for the current sheet or
//! all sheets, with the "Check safe distance" test (MECHANICS V1–V3, VM1–VM6).

use crate::nest::{self, Part, Placed, P};
use crate::theme::th;
use crate::ui_parts::*;
use crate::App;
use eframe::egui;
use egui::{Pos2, RichText, Stroke};

/// How far below the required distance still counts as respected (mm).
pub const SAFE_DIST_TOL: f64 = 0.01;

/// Verify Machining result codes, as the original numbers them; the panel
/// turns each code into its message (rule VM3; anything unknown reads as
/// "no problems found"). A sheet is correct when its only code is 0 (rule VM4).
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
/// A part whose drawing would be cut wrong (JOB-41, VP1; this rebuild's).
pub const OWN_PART: u8 = 254;

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
        OWN_PART => "A part's drawing would be cut wrong (this rebuild)",
        // VM3: the no-problem result, in words that can't be read as "the
        // sheet has been cut".
        _ => "No problems found — all machining is in place",
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
        ribbon_group(ui, "Check", |ui| {
            let r = crate::drawing::dropdown(ui, Icon::Verify, "Verify\nmachining", has).on_hover_text("Check the machining of this sheet or of every sheet: pierces in the scrap, holes first, safe distance between parts");
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
            // VM4/VM5: totals whenever all sheets were checked (even if there is
            // only one), "Current sheet" when only the shown sheet was.
            if v.sheets.len() > 1 || v.all_sheets {
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
            // VM3: say what was checked, so "no problems" isn't read as more.
            ui.label(RichText::new("Checked: parts touching, overlapping or closer than the minimum distance; parts reaching outside the sheet; cut contours without lead-ins.").size(11.0).color(th().text_dim));
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
                        crate::units::drag_len_in(ui, &mut v.value, self.cfg.nesting.units, 0.0, MIN_DIST_MAX);
                    } else {
                        let mut sep = self.spec.gap;
                        ui.add_enabled_ui(false, |ui| crate::units::drag_len_in(ui, &mut sep, self.cfg.nesting.units, 0.0, f64::MAX));
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
        assert_eq!(verify_message(VERIFY_OK), "No problems found — all machining is in place");
        // A part hanging off a 150 mm sheet is out of the work zone (code 4).
        assert_eq!(out_of_zone_issues(&parts, &[pl(0.0), pl(103.0)], 150.0, 100.0).len(), 1);
        let (a, b) = bad[0].at.unwrap();
        assert!((dist(a, b) - 3.0).abs() < 1e-6);
        // Far apart: never checked in detail.
        assert!(safe_distance_issues(&parts, &[pl(0.0), pl(500.0)], 5.0).is_empty());
    }
}
