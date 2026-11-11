#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appicon;
mod arrange_ui;
mod bands;
mod blm;
mod startup;
mod iconfile;
mod cli;
mod cnc;
mod engine;
mod filewatch;
mod mcp;
mod ct1;
mod cutpanel;
mod joints;
mod kerf;
mod leads;
mod xmlimport;
mod jobtree;
mod filename;
mod dock;
mod cutoff;
mod cuttime;
mod techtable;
mod dbbrowse;
mod dbconn;
mod dbquery;
mod dbui;
mod drawing;
mod erpui;
mod integrations;
mod font;
mod ghost;
mod manual;
mod radial;
mod settings;
mod settings_ui;
mod sheetsim;
mod textedit;
mod dxf;
mod formats;
mod importui;
mod jobfile;
mod jobs;
mod libui;
mod library;
mod region;
mod remline;
mod remmark;
mod remnant;
mod stock;
mod stockui;
mod machine;
mod nest;
mod post;
mod progimport;
mod qualities;
mod partview;
mod techui;
mod theme;
mod ui_parts;
mod units;
mod v44;

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, Vec2};
use nest::{NestResult, Part, Placed, Shape, SheetSpec, P};
use std::path::PathBuf;
use theme::th;
use ui_parts::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Home,
    Sheet,
    Technology,
    View,
    /// Drawing module (a part opened on its own) only:
    Machining,
    Simulation,
    Notes,
    Utilities,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BottomTab {
    PartsList,
    PartsView,
}

/// The two main screens: the jobs manager (what the original program is)
/// and the nesting screen it hands sheets to.
#[derive(Clone, Copy, PartialEq)]
pub enum Workspace {
    Jobs,
    Nesting,
    /// One part opened on its own to apply technology (the original opens it in
    /// its Drawing module, rule O6).
    Part,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SheetTab {
    View,
    List,
}

/// Which parts open together with the chosen one (original PIEZADIB.SYS modes,
/// rule O3): the selection only, every part of the job, parts on
/// the same machine, or parts with the same machine, material and thickness.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpenScope {
    Selected,
    All,
    SameMachine,
    SameTechnology,
}

impl OpenScope {
    pub const ALL: [OpenScope; 4] = [OpenScope::Selected, OpenScope::All, OpenScope::SameMachine, OpenScope::SameTechnology];
    pub fn label(self) -> &'static str {
        match self {
            OpenScope::Selected => "This part only",
            OpenScope::All => "All parts of the job",
            OpenScope::SameMachine => "Same machine",
            OpenScope::SameTechnology => "Same machine, material & thickness",
        }
    }
}

/// A question waiting for the person's answer.
#[derive(Clone)]
pub enum Pending {
    /// Geometry edit on a part that has its own technology.
    TechPartEdit { idx: usize, draft: Part },
    /// Material or thickness change while parts are nested.
    NestedSpecChange { new_spec: SheetSpec },
    /// The open job's file was changed outside the app while it also changed here.
    JobFileChanged { path: PathBuf },
    /// Export CNC: what will be written, where and for which machine, shown
    /// before writing; with the sheets that have overlapping parts or parts off
    /// the sheet: (sheet index, overlapping pairs, parts off the sheet).
    /// `checks`: Verify machining's findings per sheet (sheet, problems).
    ExportReview { problems: Vec<(usize, usize, usize)>, checks: Vec<(usize, usize)> },
}

/// Answers to "this part has technology" (the original's choices, rule T2).
#[derive(Clone, Copy, PartialEq)]
pub enum TechChoice {
    /// Keep the original with its technology and put the change on a copy.
    EditCopy,
    /// Change the part itself; its technology is removed.
    EditPart,
}

/// One work order: its products and the sheets they were nested onto.
#[derive(Clone)]
pub struct Job {
    pub week: String,
    pub name: String,
    pub parts: Vec<Part>,
    pub spec: SheetSpec,
    pub result: NestResult,
    pub stale: bool,
    /// The job's record in the library when it has been saved there (or opened from it).
    pub lib_id: Option<u64>,
    /// Free cut lines / remnant lines and contours cut by hand, per sheet.
    pub cut_lines: Vec<Vec<cutoff::CutLine>>,
    pub manual_cuts: Vec<Vec<cnc::ManualCut>>,
    /// When the job's nestings last changed (the original's CAM last date of the
    /// job, rule ND1), seconds since 1970.
    pub cam_changed: Option<u64>,
    /// The job's file on disk (opened from or saved to), if any.
    pub file: Option<filewatch::JobLink>,
    /// Sheets this job has taken from stock, per stock reference, as of its
    /// last save (stock.rs; reconciled on every save).
    pub held: std::collections::BTreeMap<String, i32>,
    /// The ERP/MES work this job was brought in from and sent back to.
    pub erp: Option<integrations::ErpJob>,
    /// The job as last saved or opened (`job_sig`); None = never saved. It
    /// has unsaved changes when it no longer matches (jobtree.rs).
    pub saved: Option<u64>,
}

pub struct App {
    pub job_name: String,
    pub parts: Vec<Part>,
    pub sel_part: Option<usize>,
    pub spec: SheetSpec,
    pub result: NestResult,
    pub plans: Vec<cnc::SheetPlan>,
    pub program: String,
    pub stale: bool,
    pub cur_sheet: usize,
    pub sel_placed: Option<usize>,
    /// More selected parts on the sheet besides `sel_placed` (arrange_ui.rs).
    pub sel_more: Vec<usize>,
    pub drag: Option<arrange_ui::DragState>,
    /// A selection window being drawn on the sheet: corners (mm) and whether it adds.
    pub window_sel: Option<(P, P, bool)>,
    /// The movement value window (Ctrl+R) is open.
    pub step_popup: bool,
    panning: bool,
    pub zoom: f32,
    pub pan: Vec2,
    pub tab: Tab,
    pub bottom: BottomTab,
    pub collision: bool,
    /// Pairs of parts on the shown sheet that touch or overlap (QR6), worked
    /// out on every change.
    pub overlaps: Vec<(usize, usize)>,
    /// The last export (folder, file names, warning), shown on the sheet
    /// until closed so the files can be found.
    pub last_export: Option<(PathBuf, Vec<String>, Option<String>)>,
    /// A placed part to bring to the middle of the view (Show next problem).
    pub focus_placed: Option<usize>,
    /// Which problem part "Show next" shows next.
    pub problem_cursor: usize,
    /// Copies on the pointer from the quick ring's Copy (QR4).
    pub carry: Option<arrange_ui::Carry>,
    pub show_path: bool,
    pub show_numbers: bool,
    pub show_grid: bool,
    pub show_program: bool,
    pub dock: dock::Dock,
    pub show_times: bool,
    pub status: String,
    pub cursor_mm: Option<P>,
    hover_name: Option<String>,
    shot: Option<PathBuf>,
    frames: u32,
    pub drag_test_on: bool,
    pub drag_test: dock::DragTest,
    color_seq: usize,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    last_edit: f64,
    pub search: String,
    toasts: Vec<(String, f64)>,
    last_status: String,
    anim_pending: bool,
    anim_start: f64,
    pub workspace: Workspace,
    pub sheet_tab: SheetTab,
    pub jobs: Vec<Job>,
    pub cur_job: usize,
    pub job_search: String,
    /// Original option: append the labor ticket to the product status line.
    pub show_labor_ticket: bool,
    /// Which list the Jobs status line describes (last one clicked).
    pub jobs_pick_sheet: bool,
    pub pending: Option<Pending>,
    /// Closing the window with unsaved jobs: their names while asking, and
    /// whether the person chose to close anyway (jobtree.rs `quit_guard`).
    pub quit_ask: Option<Vec<String>>,
    pub quit_confirmed: bool,
    /// The Library folder as it was when typing in its box began (settings).
    pub lib_dir_typed_from: Option<String>,
    /// Original option: warn before changing a part that has technology.
    pub warn_tech: bool,
    /// "Use this answer for the rest" given in the technology question.
    pub tech_answer_all: Option<TechChoice>,
    tech_all_box: bool,
    /// Technology tab: lead type and keypad position for Automatic, and "All parts".
    pub lead_choice: nest::LeadType,
    pub lead_pos: u8,
    pub tech_all: bool,
    /// The scope picked by hand on the sheet (Nesting): None until then,
    /// when the technology commands work on all part types.
    pub tech_nest_all: Option<bool>,
    /// Screenshots only (--open-job-menu): show the Open job list as if opened.
    pub demo_open_menu: bool,
    /// Parts View scrolls this part's tile into view once (e.g. after import).
    pub scroll_to_part: Option<usize>,
    /// What the last import or automatic technology command did, shown as a
    /// card on the drawing with the next steps, until closed.
    pub result_card: Option<ResultCard>,
    pub auto_joints: nest::AutoJoints,
    pub show_joint_sizes: bool,
    /// Cut Technology → By size window.
    pub show_cut_ranges: bool,
    /// Nest parts grouped by their Group attribute (CT1 A785 sets the start value).
    pub group_nest: bool,
    /// Job Explorer: empty folders, a name being typed, a job waiting for delete confirmation.
    pub job_folders: Vec<String>,
    pub tree_edit: Option<jobtree::TreeEdit>,
    pub confirm_delete_job: Option<usize>,
    /// An empty sheet added with Add nesting, kept while it is the current sheet.
    pub empty_sheet_kept: Option<usize>,
    /// The open job's last nesting change (see Job::cam_changed).
    pub cam_changed: Option<u64>,
    /// What the open job holds from stock (see Job::held).
    pub held: std::collections::BTreeMap<String, i32>,
    /// The semi-automatic remnant tool's line type.
    pub semi_mode: remline::SemiMode,
    /// A save waiting for the new remnants' references, and the names given.
    pub naming: Option<stockui::RemnantNaming>,
    pub remnant_names: Option<std::collections::BTreeMap<(usize, usize), String>>,
    /// Parts opened together in the Part view (the original's PIEZADIB.SYS list) and which one is shown.
    pub open_set: Vec<usize>,
    pub open_idx: usize,
    pub open_scope: OpenScope,
    pub part_zoom: f32,
    /// Part view: how far the drawing has been moved (screen px).
    pub part_pan: Vec2,
    /// Earlier Part views (zoom, pan) for "Previous", and when the wheel last moved.
    pub part_view_hist: Vec<(f32, Vec2)>,
    pub part_view_last: f64,
    /// Part view: the extent the view is framed on while a lead end is dragged
    /// (part index, extent), so the part doesn't move under the pointer.
    pub part_ext_lock: Option<(usize, [f64; 4])>,
    /// Follow Windows' light/dark setting (the original's default style takes
    /// the system colours).
    pub follow_system: bool,
    /// The Options menu is open.
    pub options_open: bool,
    /// Where the open job was last saved or opened from.
    pub job_path: Option<PathBuf>,
    /// What the window last read from or wrote to that file (to notice when
    /// an agent or script changes it), and when it last looked.
    pub job_link: Option<filewatch::JobLink>,
    pub disk_poll: f64,
    /// The Library window, and the open job's record number in the library.
    pub libui: libui::LibUi,
    pub importui: importui::ImportUi,
    pub stockui: stockui::StockUi,
    pub job_lib_id: Option<u64>,
    /// The Drawing module's view state (axes, colours, simulation, measuring…).
    pub dv: drawing::DrawView,
    /// The cut technology panel on the part's drawing screen (CS6).
    pub cut_panel: cutpanel::CutPanel,
    /// Simulation of the current sheet's program (Nesting → Simulation tab).
    pub ssim: Option<sheetsim::SheetSim>,
    /// Saved settings (connections, machines, materials, folders), what was
    /// last saved, the Settings window, and the machine last applied.
    pub cfg: settings::AppSettings,
    pub cfg_saved: settings::AppSettings,
    pub setui: settings_ui::SettingsUi,
    pub applied_machine: Option<settings::Machine>,
    /// The machine's post-processor settings (block numbers, units, extension…).
    pub post: post::PostConfig,
    /// Read-only browser for Expert's database.
    pub dbb: dbbrowse::DbBrowser,
    /// BLM LC5 output (Siemens 840D SPF set + XML + zip, like Lantek's BLM post).
    pub blm: blm::BlmSettings,
    /// The nesting Verify Machining panel.
    pub nverify: v44::NestVerify,
    /// Part view: selected elements (by index), the part they belong to, the
    /// start of a box selection, and details of the selection for the ribbon.
    pub sel_elems: Vec<usize>,
    pub elem_part: Option<usize>,
    pub box_start: Option<egui::Pos2>,
    pub sel_elem_info: Vec<(usize, nest::ElemKind, f64, P)>,
    /// Where the element was clicked (part coordinates).
    pub sel_pick: Option<(usize, P)>,
    /// Texts selected in the Part view (window selection).
    pub sel_texts: Vec<usize>,
    // Text being added to a part (Technology tab, Text group).
    pub text_draft: String,
    pub text_height: f64,
    pub text_kind: nest::TextKind,
    pub placing_text: bool,
    /// Arc radius for new texts (0 = straight; see `PartText::curve`).
    pub text_curve: f64,
    /// Manual machining: contours picked by hand, per sheet, in cutting order.
    pub manual_cuts: Vec<Vec<cnc::ManualCut>>,
    pub manual_mode: bool,
    pub manual_reverse: bool,
    /// The radial (gesture) menu, when open.
    pub radial: Option<crate::radial::Radial>,
    /// The quick text editor, when open.
    pub text_editor: Option<crate::textedit::TextEditor>,
    /// A part picked in the Parts View, following the pointer until dropped.
    pub ghost: Option<crate::ghost::Ghost>,
    /// Cut lines on each sheet that belong to no part (remnant line, free paths).
    pub cut_lines: Vec<Vec<cutoff::CutLine>>,
    pub cut_tool: Option<cutoff::CutTool>,
    pub cut_draft: Vec<P>,
    pub sel_cut: Option<usize>,
    /// Moving a straight cut line: (index, grab point, its points before).
    pub cut_drag: Option<(usize, P, Vec<P>)>,
    pub round_edge: bool,
    /// The open job's ERP/MES link (see Job::erp).
    pub erp_job: Option<integrations::ErpJob>,
    /// ERP/MES window and background requests (erpui.rs).
    pub erp: erpui::ErpUi,
}

/// Everything an undo step needs to restore.
#[derive(Clone)]
pub struct Snapshot {
    parts: Vec<Part>,
    spec: SheetSpec,
    result: NestResult,
    cur_sheet: usize,
    stale: bool,
    /// Each sheet's cut lines and hand-picked cut order, so undoing a nest
    /// brings back the lines that went with the old layout.
    cut_lines: Vec<Vec<cutoff::CutLine>>,
    manual_cuts: Vec<Vec<cnc::ManualCut>>,
}

fn demo_parts() -> Vec<Part> {
    let p = |name: &str, shape, w, h, qty, color| Part { name: name.into(), shape, w, h, qty, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    vec![
        p("810", Shape::Strip, 980.0, 60.0, 4, 0),
        p("1210", Shape::Strip, 1440.0, 60.0, 4, 1),
        p("Square", Shape::Rectangle, 190.0, 190.0, 2, 2),
        p("Square hole", Shape::PlateHole, 190.0, 190.0, 2, 3),
        p("Ring", Shape::Disc, 200.0, 200.0, 8, 4),
        p("Bracket", Shape::LBracket, 150.0, 120.0, 12, 5),
        p("Gusset", Shape::Triangle, 120.0, 120.0, 14, 6),
        p("Clip", Shape::Flange, 110.0, 70.0, 16, 7),
        p("Angle", Shape::LBracket, 60.0, 45.0, 40, 8),
    ]
    .into_iter()
    .map(|mut part| {
        // Two sample parts carry their own technology.
        part.tech = match part.name.as_str() {
            "Square hole" => Some(nest::PartTech { lead_in: 4.0, lead_out: 2.0, micro_joints: 4, joint_width: 0.6, joint_type: nest::JointType::Leads, quality: nest::CutQuality::new(1), corner_start: true, lead_type: nest::LeadType::LineArc, ..Default::default() }),
            "1210" => Some(nest::PartTech { lead_in: 6.0, lead_out: 0.0, micro_joints: 6, joint_width: 0.8, quality: None, ..Default::default() }),
            _ => None,
        };
        // The holed plate carries its reference as a special marking text.
        if part.name == "Square hole" {
            // Curved over the hole, placed relative to it like "Text on element".
            let g = part.geometry();
            let elems = nest::elements_of(&g);
            if let Some(t) = elems.iter().find(|e| e.contour == 0).and_then(|e| partview::text_for_element(&g, e, "#REF", 11.0, nest::TextKind::Special)) {
                part.texts.push(t);
            }
            part.texts.push(nest::PartText { text: "QTY #QTY".into(), at: [52.0, 14.0], height: 9.0, angle: 0.0, curve: 0.0, kind: nest::TextKind::Special });
        }
        part
    })
    .chain(dxf::read(dxf::SAMPLE_DXF).ok().and_then(|d| dxf::build(&d, &dxf::sample_roles()).ok()).map(|(g, _)| Part {
        name: "Plate-R60 (DXF)".into(),
        shape: Shape::Custom,
        w: g.w,
        h: g.h,
        qty: 3,
        rotate: true,
        color: 9,
        tech: None,
        texts: g.texts.clone(),
        group: String::new(),
        erp: vec![],
        custom: Some(std::sync::Arc::new(g)),
        rots: Default::default(),
    }))
    .collect()
}

/// A few sample work orders grouped by week, each already nested.
fn demo_jobs(first: &App) -> Vec<Job> {
    let spec = first.spec.clone();
    let p = |name: &str, shape, w, h, qty, color| Part { name: name.into(), shape, w, h, qty, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
    let mk = |week: &str, name: &str, parts: Vec<Part>, spec: SheetSpec| {
        let result = nest::nest(&parts, &spec);
        Job { week: week.into(), name: name.into(), parts, spec, result, stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None }
    };
    let mut steel5 = spec.clone();
    steel5.thickness = 5.0;
    let mut alu = spec.clone();
    alu.material = 2;
    alu.thickness = 2.0;
    alu.w = 2500.0;
    alu.h = 1250.0;
    vec![
        Job {
            week: "W09_10".into(),
            name: first.job_name.clone(),
            parts: first.parts.clone(),
            spec: first.spec.clone(),
            result: first.result.clone(),
            stale: false,
            lib_id: None,
            cut_lines: vec![],
            manual_cuts: vec![],
            cam_changed: None,
            file: None,
            saved: None,
            held: Default::default(),
            erp: None,
        },
        mk(
            "W09_10",
            "Brackets-0311",
            vec![p("BR-100", Shape::LBracket, 220.0, 180.0, 24, 5), p("BR-101", Shape::LBracket, 160.0, 120.0, 30, 1), p("Gusset-S", Shape::Triangle, 140.0, 140.0, 40, 6)],
            steel5,
        ),
        mk(
            "W11",
            "Covers-0312",
            vec![p("Cover-L", Shape::SlottedPlate, 900.0, 300.0, 6, 4), p("Cover-S", Shape::SlottedPlate, 450.0, 200.0, 12, 9), p("Plate-H", Shape::PlateHole, 250.0, 250.0, 10, 3)],
            alu,
        ),
        mk(
            "W11",
            "Frames-A",
            vec![p("Rail-1400", Shape::Strip, 1400.0, 80.0, 12, 1), p("Rail-900", Shape::Strip, 900.0, 80.0, 16, 0), p("Flange-200", Shape::Flange, 200.0, 200.0, 20, 7)],
            spec.clone(),
        ),
        mk(
            "W12",
            "Discs-Batch",
            vec![p("Disc-300", Shape::Disc, 300.0, 300.0, 18, 4), p("Disc-180", Shape::Disc, 180.0, 180.0, 30, 2), p("Washer-90", Shape::Disc, 90.0, 90.0, 60, 9)],
            spec,
        ),
    ]
}

impl App {
    fn new(shot: Option<PathBuf>) -> Self {
        let mut a = App {
            job_name: "Sample job 1-3.0000".into(),
            parts: demo_parts(),
            sel_part: Some(1),
            spec: SheetSpec { w: 2000.0, h: 1000.0, margin: 5.0, gap: 5.0, material: 0, thickness: 3.0, level: 3, machine: Default::default(), kerf: Default::default() },
            result: NestResult::default(),
            plans: vec![],
            program: String::new(),
            stale: false,
            cur_sheet: 0,
            sel_placed: None,
            sel_more: vec![],
            drag: None,
            window_sel: None,
            step_popup: false,
            panning: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tab: Tab::Home,
            bottom: BottomTab::PartsView,
            collision: true,
            overlaps: vec![],
            last_export: None,
            focus_placed: None,
            problem_cursor: 0,
            carry: None,
            show_path: false,
            show_numbers: false,
            show_grid: false,
            show_program: false,
            dock: dock::Dock::default(),
            show_times: false,
            status: String::new(),
            cursor_mm: None,
            hover_name: None,
            shot,
            frames: 0,
            drag_test_on: false,
            drag_test: dock::DragTest::default(),
            color_seq: 9,
            undo: vec![],
            redo: vec![],
            last_edit: -10.0,
            search: String::new(),
            toasts: vec![],
            last_status: String::new(),
            anim_pending: false,
            anim_start: 0.0,
            workspace: Workspace::Nesting,
            sheet_tab: SheetTab::View,
            jobs: vec![],
            cur_job: 0,
            job_search: String::new(),
            show_labor_ticket: false,
            jobs_pick_sheet: false,
            pending: None,
            quit_ask: None,
            quit_confirmed: false,
            lib_dir_typed_from: None,
            warn_tech: true,
            tech_answer_all: None,
            tech_all_box: false,
            lead_choice: nest::LeadType::Machine,
            lead_pos: 1,
            tech_all: false,
            tech_nest_all: None,
            demo_open_menu: false,
            scroll_to_part: None,
            result_card: None,
            auto_joints: nest::AutoJoints::default(),
            show_joint_sizes: false,
            show_cut_ranges: false,
            group_nest: crate::ct1::active().group_parts,
            job_folders: vec![],
            tree_edit: None,
            confirm_delete_job: None,
            empty_sheet_kept: None,
            open_set: vec![],
            open_idx: 0,
            open_scope: OpenScope::All,
            part_zoom: 1.0,
            part_pan: Vec2::ZERO,
            part_view_hist: vec![],
            part_ext_lock: None,
            part_view_last: 0.0,
            follow_system: false,
            options_open: false,
            job_path: None,
            job_link: None,
            disk_poll: -10.0,
            libui: Default::default(),
            importui: Default::default(),
            stockui: Default::default(),
            job_lib_id: None,
            dv: drawing::DrawView::default(),
            cut_panel: Default::default(),
            ssim: None,
            cfg: settings::AppSettings::load(),
            cfg_saved: settings::AppSettings::default(),
            setui: Default::default(),
            applied_machine: None,
            post: post::PostConfig::default(),
            dbb: Default::default(),
            blm: blm::BlmSettings::for_machine(""),
            nverify: v44::NestVerify { check_safe: true, value: 5.0, ..Default::default() },
            sel_elems: vec![],
            elem_part: None,
            box_start: None,
            sel_elem_info: vec![],
            sel_pick: None,
            sel_texts: vec![],
            text_draft: "#REF".into(),
            text_height: 8.0,
            text_kind: nest::TextKind::Special,
            placing_text: false,
            text_curve: 0.0,
            manual_cuts: vec![],
            cam_changed: None,
            held: Default::default(),
            semi_mode: Default::default(),
            naming: None,
            remnant_names: None,
            manual_mode: false,
            manual_reverse: false,
            radial: None,
            text_editor: None,
            ghost: None,
            cut_lines: vec![],
            cut_tool: None,
            cut_draft: vec![],
            sel_cut: None,
            cut_drag: None,
            round_edge: true,
            erp_job: None,
            erp: Default::default(),
        };
        a.run_nesting();
        a.undo.clear();
        a.jobs = demo_jobs(&a);
        // The examples count as saved: nothing to lose when closing them.
        for i in 0..a.jobs.len() {
            a.mark_job_saved(i);
        }
        a
    }

    /// Keep the open job's copy in the job list up to date.
    pub fn store_job(&mut self) {
        if let Some(j) = self.jobs.get_mut(self.cur_job) {
            j.name = self.job_name.clone();
            j.parts = self.parts.clone();
            j.spec = self.spec.clone();
            j.result = self.result.clone();
            j.stale = self.stale;
            j.lib_id = self.job_lib_id;
            j.cut_lines = self.cut_lines.clone();
            j.manual_cuts = self.manual_cuts.clone();
            j.cam_changed = self.cam_changed;
            j.held = self.held.clone();
            j.file = self.job_link.clone();
            j.erp = self.erp_job.clone();
        }
    }

    /// Open another job: its products, sheet settings and nested sheets.
    pub fn open_job(&mut self, i: usize) {
        if i >= self.jobs.len() || i == self.cur_job {
            return;
        }
        self.store_job();
        let j = self.jobs[i].clone();
        self.cur_job = i;
        self.job_name = j.name;
        self.parts = j.parts;
        self.spec = j.spec;
        self.result = j.result;
        self.stale = j.stale;
        self.job_lib_id = j.lib_id;
        // The job's own cut lines and hand-cut contours (not the previous job's).
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.cam_changed = j.cam_changed;
        self.held = j.held;
        // The job's own file (not the previous job's).
        self.job_path = j.file.as_ref().map(|l| l.path.clone());
        self.job_link = j.file;
        self.erp_job = j.erp;
        self.ghost = None;
        self.sel_cut = None;
        self.cur_sheet = 0;
        self.sel_placed = None;
        self.sel_part = if self.parts.is_empty() { None } else { Some(0) };
        self.undo.clear();
        self.redo.clear();
        self.jobs_reindexed();
        self.refresh();
        self.status = format!("Opened job {}.", self.job_name);
    }

    /// Open a nesting from machine programs (.zip / .XML / .SPF, see
    /// progimport.rs) as a new job, ready to edit in the Nesting module.
    pub fn open_nesting_file(&mut self, path: &std::path::Path) {
        let pj = match engine::imports::program_job(&self.cfg, &self.spec, path, self.color_seq) {
            Ok(pj) => pj,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.color_seq += pj.parts.len();
        let name = self.unique_job_name(&pj.name).unwrap_or_else(|| pj.name.clone());
        let spec = pj.spec.clone();
        let n_parts = pj.sheet.len();
        let notes = pj.notes.clone();
        let cuts = pj.cuts.clone();
        self.store_job();
        self.jobs.push(Job {
            week: "Opened programs".into(),
            name: name.clone(),
            parts: pj.parts,
            spec: spec.clone(),
            result: nest::NestResult { sheets: vec![pj.sheet], formats: vec![nest::SheetFormat::rect(spec.w, spec.h)], method: "opened from machine program".into(), tried: 0 },
            stale: false,
            lib_id: None,
            cut_lines: vec![],
            manual_cuts: vec![],
            cam_changed: None,
            held: Default::default(),
            file: None,
            saved: None,
            erp: None,
        });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        // Cut-offs come back as remnant lines (edge to edge) or free cuts.
        self.cut_lines = vec![cuts];
        self.workspace = Workspace::Nesting;
        self.refresh();
        let extra = if notes.is_empty() { String::new() } else { format!(" ({})", notes.join("; ")) };
        self.status = format!("Opened {name}: {n_parts} parts of {} kinds on a {:.0} × {:.0} sheet — move, rotate, add or remove parts, then export.{extra}", self.parts.len(), spec.w, spec.h);
    }

    /// Save the open job (Save as… asks where; Save reuses the last place).
    pub fn save_job(&mut self, ask: bool) {
        let safe: String = self.job_name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' || c == ' ' { c } else { '_' }).collect();
        let path = match (&self.job_path, ask) {
            (Some(p), false) => p.clone(),
            _ => {
                let dir = if !self.cfg.export_dir.is_empty() { PathBuf::from(&self.cfg.export_dir) } else { library::documents_dir() };
                let _ = std::fs::create_dir_all(&dir);
                match rfd::FileDialog::new().add_filter("Sheet nesting job", &[jobfile::EXT]).set_directory(&dir).set_file_name(format!("{safe}.{}", jobfile::EXT)).save_file() {
                    Some(p) => p,
                    None => return,
                }
            }
        };
        self.save_job_to(&path);
    }

    pub fn save_job_to(&mut self, path: &std::path::Path) {
        // Someone else (an agent) changed the file since the window last read
        // or wrote it: don't write over their work without asking which to keep.
        if let Some(link) = self.job_link.clone().filter(|l| l.path == path) {
            let m = filewatch::mtime(path);
            if m.is_some() && m != link.mtime && jobfile::load(path).is_ok_and(|d| engine::imports::job_sig(&d) != link.disk_sig) {
                self.pending = Some(Pending::JobFileChanged { path: path.to_path_buf() });
                self.status = "Not saved: the job's file was changed by another program — choose which version to keep first.".into();
                return;
            }
        }
        if !self.ready_to_save(stockui::SaveTarget::File(path.to_path_buf())) {
            return;
        }
        let stock_note = self.stock_on_save();
        self.store_job();
        let week = self.jobs.get(self.cur_job).map_or(String::new(), |j| j.week.clone());
        let j = jobfile::JobFile {
            format: jobfile::FORMAT.into(),
            version: jobfile::VERSION,
            name: self.job_name.clone(),
            week,
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
        };
        match jobfile::save(path, &j) {
            Ok(()) => {
                self.link_job_file(path, &j);
                self.mark_job_saved(self.cur_job);
                // An agent's one-step undo copy is from before this save: undoing
                // to it would drop what was just saved.
                let _ = std::fs::remove_file(engine::session::backup_path(path));
                self.note_recent_job(path);
                self.status = format!("Saved job {} to {}{stock_note}", self.job_name, path.display());
            }
            Err(e) => self.status = format!("Couldn't save the job: {e}"),
        }
    }

    /// Open a saved job as a job of its own (a second copy gets " (1)").
    pub fn open_job_file(&mut self, path: &std::path::Path) {
        let j = match jobfile::load(path) {
            Ok(j) => j,
            Err(e) => {
                self.status = format!("Couldn't open {}: {e}", path.display());
                return;
            }
        };
        let disk = j.clone();
        let name = self.unique_job_name(&j.name).unwrap_or_else(|| j.name.clone());
        self.store_job();
        let tried = 0;
        self.jobs.push(Job { week: if j.week.is_empty() { "Opened jobs".into() } else { j.week }, name, parts: j.parts, spec: j.spec.clone(), result: { let mut r = nest::NestResult { sheets: j.sheets, formats: j.formats, method: j.method, tried }; r.normalise(&j.spec); r }, stale: j.stale, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: j.held.clone(), erp: None, file: None, saved: None });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.cut_lines = j.cut_lines;
        self.manual_cuts = j.manual_cuts;
        self.stale = j.stale;
        self.erp_job = j.erp;
        self.job_lib_id = None;
        self.refresh();
        self.link_job_file(path, &disk);
        self.store_job();
        self.mark_job_saved(self.cur_job);
        self.note_recent_job(path);
        self.status = format!("Opened job {} from {} — {} product(s), {} sheet(s).", self.job_name, path.display(), self.parts.len(), self.result.sheets.len());
    }

    pub fn open_job_dialog(&mut self) {
        // Starts where jobs are saved by default.
        let dir = self.cfg.recent_jobs.first().and_then(|p| PathBuf::from(p).parent().map(|d| d.to_path_buf())).unwrap_or_else(|| self.export_dir());
        if let Some(f) = rfd::FileDialog::new().add_filter("Sheet nesting job", &[jobfile::EXT]).set_directory(&dir).pick_file() {
            self.open_job_file(&f);
        }
    }

    /// A job file just opened or saved goes to the top of the recent list.
    pub fn note_recent_job(&mut self, path: &std::path::Path) {
        let p = path.display().to_string();
        self.cfg.recent_jobs.retain(|r| r != &p);
        self.cfg.recent_jobs.insert(0, p);
        self.cfg.recent_jobs.truncate(8);
        let _ = self.cfg.save();
    }

    /// The Open job menu: recent job files, the library, or any file.
    pub fn open_job_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(380.0);
        if !self.libui.loaded {
            self.lib_refresh();
        }
        // One list of the jobs saved lately, wherever they were saved — job
        // files and the Library's jobs — newest first.
        enum Src {
            File(PathBuf),
            Lib(u64),
        }
        let mut rows: Vec<(u64, String, String, Src, bool)> = vec![];
        for r in &self.cfg.recent_jobs {
            let p = PathBuf::from(r);
            let name = p.file_stem().map_or(r.clone(), |n| n.to_string_lossy().to_string());
            let folder = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            let exists = p.exists();
            rows.push((t, name, folder, Src::File(p), exists));
        }
        for e in self.libui.entries.iter().filter(|e| e.kind == library::Kind::Job) {
            let what = format!("{} {} mm · {} sheet(s){}", cnc::MATERIALS[e.material.min(2)], e.thickness, e.count_b, if e.folder.is_empty() { String::new() } else { format!(" · {}", e.folder) });
            rows.push((e.updated, e.name.clone(), what, Src::Lib(e.id), true));
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0));
        ui.label(RichText::new("Recent jobs").strong().color(th().text_strong));
        ui.label(RichText::new("Job files and Library jobs together, newest first").size(11.5).color(th().text_dim));
        if rows.is_empty() {
            ui.label(RichText::new("None yet — jobs you save or open show up here.").size(12.0).color(th().text_dim));
        }
        let id = egui::Id::unique("open_job_search");
        let mut needle = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
        if rows.len() > 6 {
            ui.add(egui::TextEdit::singleline(&mut needle).hint_text("🔍 Find a job…").desired_width(f32::INFINITY));
            ui.data_mut(|d| d.insert_temp(id, needle.clone()));
        }
        let n = needle.to_lowercase();
        let mut open_file = None;
        let mut open_lib = None;
        let when = |t: u64| {
            if t == 0 {
                return String::new();
            }
            let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()).saturating_sub(t)) / 86_400;
            match days {
                0 => "today".to_string(),
                1 => "yesterday".to_string(),
                d => format!("{d} days ago"),
            }
        };
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (t, name, detail, src, exists) in rows.iter().filter(|r| n.is_empty() || r.1.to_lowercase().contains(&n) || r.2.to_lowercase().contains(&n)).take(14) {
                let (badge, tip) = match src {
                    Src::File(p) => ("File", if *exists { p.display().to_string() } else { format!("{}\n(moved or deleted)", p.display()) }),
                    Src::Lib(_) => ("Library", format!("Saved to the Library ({})", self.library_dir().display())),
                };
                ui.horizontal(|ui| {
                    let resp = ui.add_enabled(*exists, egui::Button::new(RichText::new(name).strong()).frame(false)).on_hover_text(tip);
                    ui.label(RichText::new(badge).size(10.5).color(th().text_dim).background_color(th().panel));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(when(*t)).size(11.0).color(th().text_dim));
                    });
                    if resp.clicked() {
                        match src {
                            Src::File(p) => open_file = Some(p.clone()),
                            Src::Lib(id) => open_lib = Some(*id),
                        }
                    }
                });
                ui.label(RichText::new(detail).size(11.5).color(th().text_dim));
                ui.add_space(2.0);
            }
        });
        ui.separator();
        if ui_parts::menu_row(ui, "📂", "Browse for a job file…", "Ctrl + O") {
            self.open_job_dialog();
        }
        if ui_parts::menu_row(ui, "📚", "Library…", "all saved jobs and products") {
            self.libui.open = true;
            self.libui.tab = 0;
        }
        if let Some(p) = open_file {
            self.open_job_file(&p);
        }
        if let Some(id) = open_lib {
            self.open_library_job(id);
        }
    }

    /// The next part colour.
    pub fn color_seq_next(&mut self) -> usize {
        self.color_seq += 1;
        self.color_seq - 1
    }

    /// Name rule from the original (MECHANICS.md first table, #9): if the
    /// name is taken, try "Name (1)" … "Name (9)"; give up after 9 tries or when the
    /// name is longer than 250 characters.
    pub fn unique_job_name(&self, base: &str) -> Option<String> {
        let taken = |n: &str| self.jobs.iter().enumerate().any(|(i, j)| if i == self.cur_job { self.job_name == n } else { j.name == n });
        if base.chars().count() > 250 {
            return None;
        }
        if !taken(base) {
            return Some(base.to_string());
        }
        (1..10).map(|k| format!("{base} ({k})")).find(|n| !taken(n))
    }

    pub fn new_job_in_week(&mut self, week: String) {
        let Some(name) = self.unique_job_name("New job") else {
            self.status = "Couldn't create the job: 'New job (1)' to '(9)' are all taken.".into();
            return;
        };
        self.store_job();
        self.jobs.push(Job { week, name, parts: vec![], spec: engine::with_prefs(&self.spec, &self.cfg), result: NestResult::default(), stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None });
        let i = self.jobs.len() - 1;
        self.open_job(i);
        self.status = format!("Created job {} — add products, then nest.", self.job_name);
    }

    /// Copy the open job, products and sheets included, under the same name rule.
    pub fn copy_job(&mut self) {
        let base = self.job_name.clone();
        let Some(name) = self.unique_job_name(&base) else {
            self.status = format!("Couldn't copy: '{base} (1)' to '(9)' are all taken.");
            return;
        };
        self.store_job();
        let mut j = self.jobs[self.cur_job].clone();
        j.name = name;
        // Not the original's library record or file, or saving the copy would
        // overwrite it; and never saved.
        j.lib_id = None;
        j.file = None;
        j.saved = None;
        // Nor its stock: the copy takes its own sheets when it's saved, and the
        // original's remnants stay the original's.
        j.held.clear();
        for f in j.result.formats.iter_mut() {
            f.made.clear();
            f.made_from = 0;
        }
        // Nor its work order: the copy's nest would replace the original's.
        if let Some(e) = j.erp.as_mut() {
            e.work_order = None;
            e.sent = None;
        }
        self.jobs.insert(self.cur_job + 1, j);
        self.jobs_reindexed();
        let i = self.cur_job + 1;
        self.open_job(i);
        self.status = format!("Copied job to {}.", self.job_name);
    }

    /// Identical sheets folded together, in nesting order (original: rows with
    /// CopyIndx > 1 are hidden and Quantity counts the copies — MECHANICS.md #4).
    /// Returns (first sheet index, number of identical sheets).
    pub fn sheet_groups(&self) -> Vec<(usize, usize)> {
        let sig = |s: &Vec<Placed>| {
            let mut v: Vec<(usize, i64, i64, u8, bool)> = s.iter().map(|p| (p.part, (p.x * 10.0).round() as i64, (p.y * 10.0).round() as i64, p.turns, p.mirror)).collect();
            v.sort();
            v
        };
        // Same parts in the same places on the same kind of sheet (a remnant
        // or another stock size is a different sheet).
        let fmt = |i: usize| {
            let f = self.result.format(i, &self.spec);
            ((f.w * 10.0).round() as i64, (f.h * 10.0).round() as i64, f.stock_ref)
        };
        let mut groups: Vec<(usize, usize, Vec<(usize, i64, i64, u8, bool)>)> = vec![];
        for (i, s) in self.result.sheets.iter().enumerate() {
            let g = sig(s);
            if let Some(e) = groups.iter_mut().find(|e| e.2 == g && fmt(e.0) == fmt(i)) {
                e.1 += 1;
            } else {
                groups.push((i, 1, g));
            }
        }
        groups.into_iter().map(|(i, n, _)| (i, n)).collect()
    }

    /// Add part drawings as parts (dropped on the window, picked in the
    /// system's file picker). A drawing with several layer rows, or rows that
    /// start as Leave out (DX3), opens the Import drawings window instead,
    /// picked and with its cut technology panel open, so its layers are set
    /// before it comes in (IM12).
    pub fn import_drawings(&mut self, paths: Vec<PathBuf>) {
        // Lantek import files (xsd\Importers: <Import><Product …>) go to the XML importer.
        let (xml, paths): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
        for x in &xml {
            self.import_xml(x);
        }
        if paths.is_empty() {
            return;
        }
        let mut added = 0;
        let mut notes: Vec<String> = vec![];
        let mut first = true;
        let mut layered: Vec<PathBuf> = vec![];
        for path in paths {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
            let reads = match formats::read_file(&path, None, &self.cfg) {
                Ok(p) => formats::incoming(&stem, p),
                Err(e) => {
                    notes.push(e);
                    continue;
                }
            };
            if reads.iter().any(|r| r.part.drawing.layers().len() > 1 || !r.part.drawing.hidden.is_empty()) {
                layered.push(path);
                continue;
            }
            added += self.add_drawing(&reads, &Default::default(), &mut notes, &mut first);
        }
        if !layered.is_empty() {
            self.review_drawings(layered);
        }
        self.finish_import(added, notes);
    }

    /// Open the Import drawings window on drawings whose layers need a look
    /// (IM12): picked, the first shown, with the cut technology panel open.
    fn review_drawings(&mut self, files: Vec<PathBuf>) {
        let Some(dir) = files.first().and_then(|f| f.parent()).map(std::path::Path::to_path_buf) else { return };
        let mut dirs = vec![dir];
        dirs.extend(self.cfg.import_dirs.iter().map(PathBuf::from));
        self.importui.show(&dirs);
        self.importui.picked = files.iter().cloned().collect();
        self.importui.focus = files.first().cloned();
        self.importui.panel.open = true;
        let names: Vec<String> = files.iter().filter_map(|f| f.file_name()).map(|n| n.to_string_lossy().to_string()).collect();
        self.status = format!("{} {} layers: set them in Cut technology, then Import.", names.join(", "), if names.len() == 1 { "has several" } else { "have several" });
    }

    /// Import drawings from the Import drawings window (IM11): each with the
    /// drawing as its preview read it and what was set for it there, or read
    /// now with the layer roles it starts with. A drawing set in the preview
    /// whose file changed since comes in as it starts, and the notes say so.
    pub fn import_chosen(&mut self, files: Vec<(PathBuf, importui::Chosen)>) {
        let (xml, files): (Vec<_>, Vec<_>) = files.into_iter().partition(|(p, _)| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")));
        for (x, _) in &xml {
            self.import_xml(x);
        }
        if files.is_empty() {
            return;
        }
        let mut added = 0;
        let mut notes: Vec<String> = vec![];
        let mut first = true;
        for (path, chosen) in files {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Drawing".into());
            let (reads, choices) = match chosen {
                importui::Chosen::Read(d, c) => (d.reads.clone(), c),
                read_now => {
                    if matches!(read_now, importui::Chosen::Changed) {
                        notes.push(format!("{stem}: the file or the import settings changed after its layers and contours were set in the preview, so it came in as it starts; set them again in Cut technology"));
                    }
                    match formats::read_file(&path, None, &self.cfg) {
                        Ok(p) => (formats::incoming(&stem, p), Default::default()),
                        Err(e) => {
                            notes.push(e);
                            continue;
                        }
                    }
                }
            };
            added += self.add_drawing(&reads, &choices, &mut notes, &mut first);
        }
        self.finish_import(added, notes);
    }

    /// Make each part of one drawing file with the layers and contours chosen
    /// for it (IM11; `engine::imports::chosen_part`, as `import-drawing`) and
    /// add it to the job; how many were added.
    fn add_drawing(&mut self, reads: &[formats::Incoming], choices: &engine::imports::DrawingChoices, notes: &mut Vec<String>, first: &mut bool) -> usize {
        let roles = choices.roles(reads);
        let mut added = 0;
        for (ix, inc) in reads.iter().enumerate() {
            match engine::imports::chosen_part(&self.cfg, &self.spec, inc, &roles, choices.of(ix), self.color_seq) {
                Ok((part, n)) => {
                    if *first {
                        self.checkpoint();
                        *first = false;
                    }
                    notes.extend(n);
                    self.parts.push(part);
                    self.color_seq += 1;
                    added += 1;
                }
                Err(e) => notes.push(e),
            }
        }
        added
    }

    fn finish_import(&mut self, added: usize, notes: Vec<String>) {
        if added > 0 {
            self.sel_part = Some(self.parts.len() - 1);
            self.stale = true;
            self.refresh();
        }
        if added == 0 && notes.is_empty() {
            return; // the Import drawings window shows the rest (IM12)
        }
        self.status = if notes.is_empty() {
            format!("Imported {added} drawing(s). Set quantities, then Nest all.")
        } else {
            format!("Imported {added} drawing(s). {}", notes.join(" · "))
        };
        // The same, as a card on the drawing with the next steps.
        let first = self.parts.len() - added;
        let new = &self.parts[first..];
        let pcs: u32 = new.iter().map(|p| p.qty).sum();
        let mut names: Vec<String> = new.iter().take(4).map(|p| format!("{} × {}", p.name, p.qty)).collect();
        if added > 4 {
            names.push(format!("and {} more", added - 4));
        }
        let mut lines = vec![
            names.join(" · "),
            format!("Material {} {} mm — the job's; change it before nesting if it's wrong.", cnc::MATERIALS[self.spec.material.min(2)], self.spec.thickness),
        ];
        lines.extend(notes.iter().take(4).cloned());
        if added > 0 {
            self.scroll_to_part = Some(first);
        }
        self.result_card = Some(ResultCard {
            title: if added == 0 { "Nothing imported".to_string() } else { format!("Imported {added} drawing(s) — {added} new part type(s), {pcs} pcs") },
            lines: if added == 0 { notes.clone() } else { lines },
            warn: added == 0 || !notes.is_empty(),
            actions: if added == 0 { vec![] } else { vec![("Nest all".to_string(), CardAct::NestAll), ("Show new parts".to_string(), CardAct::ShowPart(first)), ("Material and sheet".to_string(), CardAct::SheetTab)] },
        });
    }

    /// Finish drawings that waited for the layer question.
    /// Save the selected part as DXF (outline and holes on CUT, marking on MARK),
    /// with a one-line text file of the same name beside it — the way the original
    /// hands a part to its bending program (rule D1).
    pub fn export_part_dxf(&mut self, idx: usize) {
        let Some(p) = self.parts.get(idx) else { return };
        let g = p.geometry();
        let marks = p.custom.as_ref().map_or(vec![], |c| c.marks.clone());
        let text = dxf::write(&g.outline, &g.holes, &marks);
        let safe = |s: &str| s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect::<String>();
        let dir = library::documents_dir()
            .join(safe(&self.job_name))
            .join("parts");
        let base = safe(&p.name);
        let info = format!("{} / {} / {} mm", p.name, cnc::MATERIALS[self.spec.material], self.spec.thickness);
        let res = (|| -> std::io::Result<PathBuf> {
            std::fs::create_dir_all(&dir)?;
            let f = dir.join(format!("{base}.dxf"));
            std::fs::write(&f, text)?;
            std::fs::write(dir.join(format!("{base}.txt")), info + "\n")?;
            Ok(f)
        })();
        self.status = match res {
            Ok(f) => format!("Saved {} as DXF: {}", p.name, f.display()),
            Err(e) => format!("Couldn't save the DXF: {e}"),
        };
    }

    /// Carry out the answer to "this part has technology".
    pub fn apply_tech_choice(&mut self, idx: usize, draft: Part, choice: TechChoice) {
        if idx >= self.parts.len() {
            return;
        }
        self.checkpoint();
        match choice {
            TechChoice::EditCopy => {
                // Original inserts a copy of the part (rule T2).
                let mut copy = draft;
                copy.tech = None;
                let base = self.parts[idx].name.clone();
                copy.name = engine::copy_name(&self.parts, &base);
                copy.color = self.color_seq;
                self.color_seq += 1;
                // Placed indices keep pointing at the same parts.
                engine::insert_part(&mut self.parts, &mut self.result.sheets, idx + 1, copy);
                self.sel_part = Some(idx + 1);
                self.status = format!("Kept '{}' with its technology; the change went to '{}'.", base, self.parts[idx + 1].name);
            }
            TechChoice::EditPart => {
                let mut p = draft;
                p.tech = None;
                self.status = format!("Changed '{}'; its technology was removed.", p.name);
                self.parts[idx] = p;
            }
        }
        self.stale = true;
        self.refresh();
    }

    /// Material/thickness change while parts are nested (original asks once:
    /// keep the nestings, remove them and free their sheets, or cancel — rule T4).
    pub fn apply_spec_change(&mut self, new_spec: SheetSpec, remove_nestings: bool) {
        self.checkpoint();
        self.spec = new_spec;
        if remove_nestings {
            let n = self.result.sheets.len();
            self.result = NestResult::default();
            self.sel_placed = None;
            self.status = format!("Removed {n} nested sheet(s) and freed them. Nest again for the new material.");
        } else {
            self.stale = true;
            self.status = "Kept the nested sheets; they're marked Modified.".into();
        }
        self.refresh();
    }

    /// The question dialog, when one is waiting.
    fn pending_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.clone() else { return };
        if let Pending::JobFileChanged { path } = &p {
            if self.job_file_dialog(ctx, path) {
                self.pending = None;
            }
            return;
        }
        let mut close = false;
        let title = match &p {
            Pending::TechPartEdit { .. } => "Part with technology",
            Pending::NestedSpecChange { .. } => "Nested parts will change",
            Pending::JobFileChanged { .. } => "Job changed on disk",
            Pending::ExportReview { .. } => "Export CNC programs",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                match p {
                    Pending::ExportReview { problems, checks } => {
                        // What will be written, for which machine and where.
                        let n = self.result.sheets.len();
                        let dir = self.export_dir().join(engine::safe_name(&self.job_name));
                        let names: Vec<String> = (0..n).map(|k| self.cnc_name(k + 1)).collect();
                        let shown = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                        let more = if n > 3 { format!(" and {} more", n - 3) } else { String::new() };
                        egui::Grid::new("export_review").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                            ui.label(RichText::new("Sheets").color(th().text_dim));
                            ui.label(RichText::new(format!("all {n} sheet(s) of {}", self.job_name)).strong());
                            ui.end_row();
                            // Both can be changed here, without leaving the review.
                            ui.label(RichText::new("Machine").color(th().text_dim));
                            if self.machine_combo(ui, 200.0) {
                                self.refresh();
                            }
                            ui.end_row();
                            ui.label(RichText::new("Post-processor").color(th().text_dim));
                            self.output_combo(ui, 200.0);
                            ui.end_row();
                            ui.label(RichText::new("Files").color(th().text_dim));
                            ui.label(format!("{shown}{more}, and a parts report (.csv)"));
                            ui.end_row();
                            ui.label(RichText::new("Folder").color(th().text_dim));
                            ui.label(RichText::new(dir.display().to_string()).monospace().size(12.0));
                            ui.end_row();
                        });
                        ui.add_space(4.0);
                        ui.label(RichText::new("The programs follow each sheet's machining as Cutting path shows it — lead-ins, cut order, and your hand-picked order where you set one.").size(12.0).color(th().text_dim));
                        // Verify machining, run for this review on every sheet.
                        if checks.is_empty() {
                            ui.label(RichText::new(format!("✔ Verify machining: all {n} sheet(s) pass (overlaps, part gap, off the sheet, lead-ins)")).size(12.5).color(th().ok_fg));
                        } else {
                            let total: usize = checks.iter().map(|c| c.1).sum();
                            let which = checks.iter().map(|c| (c.0 + 1).to_string()).collect::<Vec<_>>().join(", ");
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(format!("⚠ Verify machining: {total} problem(s) on sheet {which}")).size(12.5).color(th().warn_fg));
                                if ui.link("Show them").clicked() {
                                    self.nverify.all_sheets = true;
                                    self.verify_nest();
                                    close = true;
                                }
                            });
                        }
                        // Files of an earlier export of this job that will be replaced, said plainly.
                        let old = std::fs::read_dir(&dir).map_or(0, |rd| rd.flatten().filter(|e| e.path().is_file()).count());
                        if old > 0 {
                            ui.label(RichText::new(format!("⚠ This folder already has {old} file(s) from an earlier export of this job — they will be replaced.")).size(12.5).color(th().warn_fg));
                        }
                        if !problems.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("These sheets would cut parts into each other or past the sheet:").color(Color32::from_rgb(185, 28, 28)));
                            for &(k, pairs, off) in &problems {
                                let mut w = vec![];
                                if pairs > 0 {
                                    w.push(format!("{pairs} overlap(s)"));
                                }
                                if off > 0 {
                                    w.push(format!("{off} part(s) off the sheet"));
                                }
                                ui.label(RichText::new(format!("Sheet {}: {}", k + 1, w.join(", "))).strong().color(Color32::from_rgb(185, 28, 28)));
                            }
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if problems.is_empty() {
                                let old = std::fs::read_dir(&dir).map_or(0, |rd| rd.flatten().filter(|e| e.path().is_file()).count());
                                let label = if old > 0 { format!("Replace and export {n} program(s)") } else { format!("Export {n} program(s)") };
                                let primary = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(th().accent);
                                if ui.add(primary).clicked() {
                                    self.export();
                                    close = true;
                                }
                            } else {
                                let first = problems.first().map_or(0, |s| s.0);
                                let primary = egui::Button::new(RichText::new(format!("Show sheet {}", first + 1)).color(Color32::WHITE).strong()).fill(th().accent);
                                if ui.add(primary).on_hover_text("Go to the sheet; the parts in the way are drawn in red").clicked() {
                                    self.workspace = Workspace::Nesting;
                                    self.select_sheet(first);
                                    close = true;
                                }
                                if ui.button("Export anyway").clicked() {
                                    self.export();
                                    close = true;
                                }
                            }
                            if ui.button("Change folder…").on_hover_text("Settings → Folders").clicked() {
                                self.open_settings(settings_ui::Section::Folders);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::TechPartEdit { idx, draft } => {
                        let (name, items) = self.parts.get(idx).map_or((String::new(), 0), |q| (q.name.clone(), q.tech.map_or(0, |t| t.items())));
                        ui.label(format!(
                            "'{name}' has its own technology ({items} item(s): lead-ins, micro-joints, cut quality). Changing its shape or size can't keep it."
                        ));
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.tech_all_box, "Use this answer for the rest of this session");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let primary = egui::Button::new(RichText::new("Edit a copy").color(Color32::WHITE).strong()).fill(th().accent);
                            if ui.add(primary).on_hover_text("Keep this part and its technology; make a copy with the change").clicked() {
                                if self.tech_all_box {
                                    self.tech_answer_all = Some(TechChoice::EditCopy);
                                }
                                self.apply_tech_choice(idx, draft.clone(), TechChoice::EditCopy);
                                close = true;
                            }
                            if ui.button("Edit this part (remove technology)").clicked() {
                                if self.tech_all_box {
                                    self.tech_answer_all = Some(TechChoice::EditPart);
                                }
                                self.apply_tech_choice(idx, draft.clone(), TechChoice::EditPart);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::NestedSpecChange { new_spec } => {
                        ui.label(format!(
                            "{} sheet(s) are nested in {} {} mm. Changing to {} {} mm affects them.",
                            self.result.sheets.len(),
                            cnc::MATERIALS[self.spec.material],
                            self.spec.thickness,
                            cnc::MATERIALS[new_spec.material],
                            new_spec.thickness
                        ));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Keep nested sheets").clicked() {
                                self.apply_spec_change(new_spec.clone(), false);
                                close = true;
                            }
                            let primary = egui::Button::new(RichText::new("Remove nestings").color(Color32::WHITE).strong()).fill(th().accent);
                            if ui.add(primary).on_hover_text("Take the parts off the sheets and free the sheets").clicked() {
                                self.apply_spec_change(new_spec.clone(), true);
                                close = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    }
                    Pending::JobFileChanged { .. } => {}
                }
            });
        if close {
            self.pending = None;
            self.tech_all_box = false;
        }
    }

    /// Furthest X and Y any part reaches on a sheet.
    pub fn sheet_extent(&self, i: usize) -> (f64, f64) {
        self.result.sheets.get(i).map_or((0.0, 0.0), |s| {
            (s.iter().map(|p| p.x + p.w).fold(0.0, f64::max), s.iter().map(|p| p.y + p.h).fold(0.0, f64::max))
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            parts: self.parts.clone(),
            spec: self.spec.clone(),
            result: self.result.clone(),
            cur_sheet: self.cur_sheet,
            stale: self.stale,
            cut_lines: self.cut_lines.clone(),
            manual_cuts: self.manual_cuts.clone(),
        }
    }

    /// Remember the current state so the next change can be undone.
    pub fn checkpoint(&mut self) {
        // Any other change ends a copy carried on the pointer (QR4).
        self.carry = None;
        let s = self.snapshot();
        self.push_undo(s);
    }

    fn push_undo(&mut self, s: Snapshot) {
        self.undo.push(s);
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn push_undo_coalesced(&mut self, before: Snapshot, now: f64) {
        if now - self.last_edit > 1.0 {
            self.push_undo(before);
        }
        self.last_edit = now;
    }

    fn restore(&mut self, s: Snapshot) {
        let mut result = s.result;
        // Remnants stored since are not rolled back: follow the stock (RN6).
        if result.formats.iter().any(|f| !f.made.is_empty()) {
            if let Ok(stock) = crate::stock::Stock::load(&self.library_dir()) {
                engine::stockflow::follow_stock(&stock, &mut result.formats);
            }
        }
        self.parts = s.parts;
        self.spec = s.spec;
        self.result = result;
        self.cur_sheet = s.cur_sheet;
        self.stale = s.stale;
        if self.cut_lines != s.cut_lines || self.manual_cuts != s.manual_cuts {
            self.mark_cam_changed();
        }
        self.cut_lines = s.cut_lines;
        self.manual_cuts = s.manual_cuts;
        self.sel_cut = None;
        self.sel_placed = None;
        self.drag = None;
        // A part on the pointer may be one the restored job doesn't have.
        self.ghost = None;
        if self.sel_part.is_some_and(|i| i >= self.parts.len()) {
            self.sel_part = None;
        }
        self.refresh();
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(s);
            self.result_card = None;
            self.status = "Undone.".into();
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(s);
            self.result_card = None;
            self.status = "Redone.".into();
        }
    }

    /// Slide every part on the current sheet left, then down, as far as it
    /// goes without touching another part — leaving one large remnant.
    pub fn compact(&mut self) {
        let ci = self.cur_sheet;
        if ci >= self.result.sheets.len() {
            return;
        }
        if self.result.format(ci, &self.spec).outline.is_some() {
            self.status = engine::COMPACT_REMNANT.into();
            return;
        }
        self.checkpoint();
        let before = self.sheet_usage(ci).2;
        // Follow the real contours, not the boxes (see MECHANICS.md "True-shape").
        let cs = self.sheet_spec(ci);
        nest::compact_contours(&self.parts, &mut self.result.sheets[ci], &cs);
        self.refresh();
        let after = self.sheet_usage(ci).2;
        self.status = format!(
            "Compacted — remnant now starts at X {:.0} mm ({:.0} mm gained).",
            after,
            (before - after).max(0.0)
        );
    }

    /// Place a copy of the selected parts in the first free spot (left first),
    /// keeping their layout (HM10).
    pub fn duplicate_selected(&mut self) {
        let sel = self.selection();
        let ci = self.cur_sheet;
        if sel.is_empty() || ci >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(ci, &self.spec);
        match engine::arrange::copy_spot(&self.result.sheets[ci], &sel, &self.spec, &f) {
            Some(d) => {
                self.checkpoint();
                let n = self.result.sheets[ci].len();
                for c in engine::arrange::copies(&self.result.sheets[ci], &sel, d) {
                    self.parts[c.part].qty += 1;
                    self.result.sheets[ci].push(c);
                }
                self.set_selection((n..self.result.sheets[ci].len()).collect());
                self.refresh();
                let c = self.result.sheets[ci][n];
                self.status = if sel.len() == 1 {
                    format!("Copied {} to X {}  Y {}.", self.parts[c.part].name, self.len_txt(c.x), self.len_txt(c.y))
                } else {
                    format!("Copied {} parts; the copies are selected.", sel.len())
                };
            }
            None => self.status = "No free spot on this sheet for a copy.".into(),
        }
    }

    /// Write every sheet's machine program plus a CSV parts report.
    /// BLM settings with this job's name, material and thickness filled in.
    pub fn blm_now(&self) -> blm::BlmSettings {
        engine::blm_for(&self.spec, &self.blm, &self.job_name, &self.cfg)
    }

    /// The program file name shown and written for a sheet.
    pub fn cnc_name(&self, sheet: usize) -> String {
        if self.blm.on {
            format!("{}.zip", blm::nest_name(&self.job_name, sheet))
        } else {
            self.post.file_name(&self.job_name, sheet)
        }
    }

    /// Where Export CNC writes, for its tooltip: all sheets, the folder, the format.
    pub fn export_dir(&self) -> PathBuf {
        if !self.cfg.export_dir.is_empty() { PathBuf::from(&self.cfg.export_dir) } else { library::documents_dir() }
    }

    /// Export CNC from the ribbon: a review first — the sheets, the machine and
    /// output, the folder and file names — and every sheet checked, so a
    /// program that cuts parts into each other is never written unnoticed.
    pub fn export_checked(&mut self) {
        let mut bad = vec![];
        for (k, sheet) in self.result.sheets.iter().enumerate() {
            let spec = self.sheet_spec(k);
            let pairs = engine::overlapping(&self.parts, sheet, &spec).len();
            let off = sheet.iter().filter(|p| !spec.fits(p)).count();
            if pairs > 0 || off > 0 {
                bad.push((k, pairs, off));
            }
        }
        if self.result.sheets.is_empty() {
            self.status = "Nothing to export — nest first.".into();
            return;
        }
        // Verify machining on every sheet, so the review can say it passed.
        self.refresh();
        let need = self.safe_need();
        let checks: Vec<(usize, usize)> = (0..self.result.sheets.len())
            .filter_map(|k| {
                let (sheet, plan) = (self.result.sheets.get(k)?, self.plans.get(k)?);
                let n = engine::verify_sheet(&self.parts, sheet, &self.sheet_spec(k), plan, self.nverify.check_safe.then_some(need)).len();
                (n > 0).then_some((k, n))
            })
            .collect();
        self.pending = Some(Pending::ExportReview { problems: bad, checks });
    }

    pub fn export(&mut self) {
        // Nothing nested: engine::export says so (and removes the job's old programs).
        // Settings → Folders, or Documents\Sheet Nesting.
        let base = if !self.cfg.export_dir.is_empty() {
            PathBuf::from(&self.cfg.export_dir)
        } else {
            library::documents_dir()
        };
        // Every sheet's program (or BLM LC5 set) and the CSV parts report.
        let blm = self.blm_now();
        let marks: Vec<Vec<Vec<nest::P>>> = (0..self.result.sheets.len()).map(|k| self.remnant_marks(k)).collect();
        let res = engine::export(&base, &self.job_name, &self.parts, &self.result.sheets, &self.result.formats, &self.plans, &self.spec, &self.cut_lines, &marks, &self.post, Some(&blm), self.cfg.stock.line.edge_offset);
        if let Ok(ex) = &res {
            let names = ex.files.iter().filter_map(|f| f.file_name().map(|n| n.to_string_lossy().to_string())).collect();
            self.last_export = Some((ex.dir.clone(), names, ex.warning.clone()));
        } else {
            // A refused export takes the job's programs with it (BL39).
            self.last_export = None;
        }
        self.status = match res {
            Ok(ex) if ex.warning.is_some() => format!("Exported, but {}.", ex.warning.unwrap_or_default()),
            Ok(ex) if self.blm.on => format!("Exported {} BLM LC5 program set(s) (.zip + .XML) and a parts report to {}", self.result.sheets.len(), ex.dir.display()),
            Ok(ex) => format!("Exported {} program(s) and a parts report to {}", self.result.sheets.len(), ex.dir.display()),
            Err(e) => e,
        };
    }

    pub fn run_nesting(&mut self) {
        self.checkpoint();
        self.anim_pending = true;
        // Groups in their own zones; else from the sheet stock when it has
        // sheets for this job, else the job's sheet size; then contour
        // compaction on rectangular sheets.
        // Hand work that a new nesting replaces: said in the status line, so
        // nobody loses it without knowing (Ctrl+Z brings it back).
        let lost_order = self.manual_cuts.iter().filter(|s| !s.is_empty()).count();
        let lost_lines: usize = self.cut_lines.iter().map(|s| s.len()).sum();
        self.result = engine::nest_all(&self.parts, &self.spec, self.group_nest, Some(&self.stock_use()));
        self.manual_cuts.clear();
        self.ssim = None;
        self.cut_lines.clear();
        if self.cfg.stock.lines_after_nesting {
            self.auto_lines_all_sheets();
        }
        self.mark_cam_changed();
        self.sel_cut = None;
        self.cur_sheet = 0;
        self.sel_placed = None;
        self.drag = None;
        self.stale = false;
        self.refresh();
        let need: u32 = self.parts.iter().map(|p| p.qty).sum();
        let placed = self.total_placed();
        self.status = if placed as u32 == need {
            format!("Nesting done: {} parts on {} sheet(s) — level {}, best of {} methods.", placed, self.result.sheets.len(), self.spec.nest_level().name, self.result.tried)
        } else {
            format!("{} of {} parts nested — some are larger than the sheet.", placed, need)
        };
        let mut lost = vec![];
        if lost_order > 0 {
            lost.push(format!("the hand-picked cut order on {lost_order} sheet(s)"));
        }
        if lost_lines > 0 {
            lost.push(format!("{lost_lines} cut / remnant line(s)"));
        }
        if !lost.is_empty() {
            self.status += &format!(" Removed {} — Ctrl+Z brings them back.", lost.join(" and "));
        }
    }

    /// Nest only the sheet on screen again (RN1): the parts already on it, on
    /// the same sheet; the other sheets keep their layout. Refused, sheet
    /// unchanged, when they don't all fit back (RN2).
    pub fn renest_sheet(&mut self) {
        let ci = self.cur_sheet;
        if ci >= self.result.sheets.len() {
            return;
        }
        let f = self.result.format(ci, &self.spec);
        match engine::renest_sheet(&self.parts, &self.spec, &self.result.sheets[ci], &f, self.group_nest) {
            Ok(placed) => {
                self.checkpoint();
                self.anim_pending = true;
                let before = self.sheet_usage(ci).2;
                self.result.sheets[ci] = placed;
                self.result.normalise(&self.spec);
                engine::stockflow::remnants_outdated(&mut self.result.formats[ci]);
                if let Some(l) = self.cut_lines.get_mut(ci) {
                    l.clear();
                }
                if let Some(m) = self.manual_cuts.get_mut(ci) {
                    m.clear();
                }
                self.ssim = None;
                self.sel_cut = None;
                self.sel_placed = None;
                self.drag = None;
                if self.cfg.stock.lines_after_nesting {
                    self.auto_lines_sheet(ci);
                }
                self.mark_cam_changed();
                self.refresh();
                let (usage, _, after) = self.sheet_usage(ci);
                self.status = format!(
                    "Sheet {} nested again: its {} parts, usage {:.1} %, remnant from X {:.0} mm (was {:.0}). Other sheets unchanged.",
                    ci + 1,
                    self.result.sheets[ci].len(),
                    usage,
                    after,
                    before
                );
            }
            Err(e) => self.status = e,
        }
    }

    pub fn total_placed(&self) -> usize {
        self.result.sheets.iter().map(|s| s.len()).sum()
    }

    pub fn placed_of(&self, part: usize) -> usize {
        self.result.sheets.iter().flatten().filter(|p| p.part == part).count()
    }

    pub fn refresh(&mut self) {
        // Empty sheets go, except one just added with Add nesting while it is shown.
        let keep = self.empty_sheet_kept.filter(|&k| k == self.cur_sheet);
        // Per-sheet cut lines and hand cuts go with their sheet, so later
        // sheets don't pick up their neighbour's lines.
        if engine::drop_empty_sheets(&mut self.result.sheets, &mut self.result.formats, &self.spec, &mut self.cut_lines, &mut self.manual_cuts, keep) {
            self.mark_cam_changed();
        }
        if keep.is_none() {
            self.empty_sheet_kept = None;
        }
        if self.cur_sheet >= self.result.sheets.len() {
            self.cur_sheet = self.result.sheets.len().saturating_sub(1);
        }
        self.plans = engine::plans(&self.parts, &self.result.sheets, &self.result.formats, &self.spec, &self.manual_cuts);
        self.overlaps = self.result.sheets.get(self.cur_sheet).map_or(vec![], |s| engine::overlapping(&self.parts, s, &self.cur_spec()));
        self.program = match (self.result.sheets.get(self.cur_sheet), self.plans.get(self.cur_sheet)) {
            (Some(s), Some(pl)) => engine::program(self.cur_sheet + 1, &self.parts, s, &self.cur_spec(), pl, self.cut_list(), &self.remnant_marks(self.cur_sheet), &self.post, self.cfg.stock.line.edge_offset),
            _ => "( NO SHEET NESTED )".into(),
        };
    }

    pub fn select_sheet(&mut self, i: usize) {
        if i < self.result.sheets.len() && i != self.cur_sheet {
            // A copy on the pointer belongs to the sheet it was made on.
            self.carry = None;
            self.cur_sheet = i;
            self.ssim = None;
            self.sel_placed = None;
            self.refresh();
        }
    }

    pub fn clear(&mut self) {
        self.checkpoint();
        self.clear_sheets();
        self.status = "Sheets cleared.".into();
    }

    fn clear_sheets(&mut self) {
        self.result = NestResult::default();
        self.cut_lines.clear();
        self.sel_cut = None;
        self.sel_placed = None;
        self.drag = None;
        self.refresh();
    }

    pub fn new_job(&mut self) {
        self.checkpoint();
        self.parts.clear();
        self.sel_part = None;
        self.job_name = "New job".into();
        self.spec = engine::with_prefs(&self.spec, &self.cfg);
        self.clear_sheets();
        self.status = "New job — add parts, then press Nest all.".into();
    }

    pub fn add_part(&mut self, shape: Shape) {
        self.checkpoint();
        self.parts.push(engine::shape_part(shape, self.parts.len() + 1, self.color_seq));
        self.color_seq += 1;
        self.sel_part = Some(self.parts.len() - 1);
        self.stale = true;
    }

    pub fn remove_part(&mut self, idx: usize) {
        self.checkpoint();
        engine::remove_part(&mut self.parts, &mut self.result.sheets, idx);
        // Placements were renumbered: hand picks would land on other parts
        // (as the agents' remove-part does); the part on the pointer may be gone.
        self.manual_cuts.iter_mut().for_each(Vec::clear);
        self.ghost = None;
        self.sel_part = if self.parts.is_empty() { None } else { Some(idx.min(self.parts.len() - 1)) };
        self.sel_placed = None;
        self.refresh();
        self.stale = true;
    }

    /// R: one part turns to its next allowed quarter turn, several turn 90°
    /// together — about their centre, moved to the nearest free spot when
    /// they'd land on another part (HM5).
    pub fn rotate_selected(&mut self) {
        self.step_turn_selected(false);
    }

    /// The quick ring's Turn back: one part to its previous allowed turn,
    /// several 90° the other way (QR3).
    pub fn turn_back_selected(&mut self) {
        self.step_turn_selected(true);
    }

    /// Whether Turn (or Turn back) can turn the selection: one part needs
    /// another allowed turn that way, several must all be allowed 90° (QR2).
    pub fn can_step_turn(&self, back: bool) -> bool {
        let sel = self.selection();
        let Some(sheet) = self.result.sheets.get(self.cur_sheet) else { return false };
        match sel[..] {
            [] => false,
            [i] => engine::step_turn(&self.parts[sheet[i].part], sheet[i], back).is_ok(),
            _ => engine::arrange::can_turn(&self.parts, sheet, &sel, if back { 3 } else { 1 }),
        }
    }

    fn step_turn_selected(&mut self, back: bool) {
        let sel = self.selection();
        let s = self.cur_sheet;
        if sel.is_empty() || s >= self.result.sheets.len() {
            return;
        }
        let q = if let [i] = sel[..] {
            let old = self.result.sheets[s][i];
            match engine::step_turn(&self.parts[old.part], old, back) {
                Ok(c) => (c.turns + 4 - old.turns) % 4,
                Err(e) => {
                    self.status = e.into();
                    return;
                }
            }
        } else if back {
            3
        } else {
            1
        };
        self.turn_selected(q);
    }

    pub fn remove_selected(&mut self) {
        let sel = self.selection();
        if sel.is_empty() || self.cur_sheet >= self.result.sheets.len() {
            return;
        }
        self.checkpoint();
        for &i in sel.iter().rev() {
            self.result.sheets[self.cur_sheet].remove(i);
        }
        // The sheet's placements were renumbered: its hand picks would land on
        // other parts (as the agents' operations do, sheet_changed).
        if let Some(m) = self.manual_cuts.get_mut(self.cur_sheet) {
            m.clear();
        }
        self.set_selection(vec![]);
        self.refresh();
        self.status = if sel.len() == 1 { "Part taken off the sheet. Press Nest all to place it again.".into() } else { format!("{} parts taken off the sheet.", sel.len()) };
    }

    /// (usage of whole sheet %, usage up to the remnant cut line %, X of that cut line)
    pub fn sheet_usage(&self, i: usize) -> (f64, f64, f64) {
        let Some(s) = self.result.sheets.get(i) else { return (0.0, 0.0, 0.0) };
        engine::sheet_usage(&self.parts, s, &self.spec, &self.result.format(i, &self.spec))
    }

    /// The job's settings with sheet `i`'s own size.
    pub fn sheet_spec(&self, i: usize) -> SheetSpec {
        self.spec.for_format(&self.result.format(i, &self.spec))
    }

    /// The real area of all sheets in mm² (remnants count by their shape).
    pub fn total_sheet_area(&self) -> f64 {
        (0..self.result.sheets.len()).map(|i| self.result.format(i, &self.spec).area_mm2()).sum()
    }

    /// A sheet's size as shown in lists, with its stock reference if it has one.
    pub fn sheet_label(&self, i: usize, decimals: usize) -> String {
        let f = self.result.format(i, &self.spec);
        let size = format!("{:.d$}x{:.d$}", f.w, f.h, d = decimals);
        match (f.remnant, f.stock_ref.is_empty()) {
            (true, _) => format!("{size} remnant {}", f.stock_ref),
            (false, false) => format!("{size} [{}]", f.stock_ref),
            _ => size,
        }
    }

    /// Is a part inside sheet `i`: within its margin and, on a remnant, inside
    /// its real shape?
    pub fn fits_sheet(&self, c: &Placed, i: usize) -> bool {
        engine::part_fits_format(&self.parts, &self.spec, &self.result.format(i, &self.spec), c)
    }

    /// The settings for the sheet on screen.
    pub fn cur_spec(&self) -> SheetSpec {
        self.sheet_spec(self.cur_sheet)
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.stale {
            egui::Frame::new()
                .fill(th().warn_bg)
                .stroke(Stroke::new(1.0, with_alpha(th().warn_fg, 90)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚠ Parts or sheet changed since the last nest").color(th().warn_fg));
                        let b = egui::Button::new(RichText::new("Nest all again").color(Color32::WHITE).strong()).fill(th().accent).corner_radius(CornerRadius::same(6));
                        if ui.add(b).clicked() {
                            self.run_nesting();
                        }
                    });
                });
            ui.add_space(3.0);
        }
        let now = ui.input(|i| i.time);
        if self.anim_pending {
            self.anim_pending = false;
            // No animation for the automatic screenshot, so it shows the full sheet.
            self.anim_start = if self.shot.is_some() { now - 100.0 } else { now };
        }
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // Horizontal navy-to-blue gradient background.
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), th().canvas_a);
        mesh.colored_vertex(rect.right_top(), th().canvas_b);
        mesh.colored_vertex(rect.right_bottom(), th().canvas_b);
        mesh.colored_vertex(rect.left_bottom(), th().canvas_a);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(EShape::mesh(mesh));

        let view = rect;
        let cspec = self.cur_spec();
        let cformat = self.result.format(self.cur_sheet, &self.spec);
        let fit = ((view.width() - 40.0) / cspec.w as f32).min((view.height() - 24.0) / cspec.h as f32).max(0.0001);
        // Show next problem: zoom in a little and put that part in the middle.
        if let Some(i) = self.focus_placed.take() {
            if let Some(p) = self.result.sheets.get(self.cur_sheet).and_then(|sh| sh.get(i)) {
                self.zoom = self.zoom.max(2.5);
                let s = fit * self.zoom;
                let (cx, cy) = ((p.x + p.w / 2.0) as f32, (p.y + p.h / 2.0) as f32);
                self.pan = Vec2::new(cspec.w as f32 * s / 2.0 - cx * s, cy * s - cspec.h as f32 * s / 2.0);
                self.sel_placed = Some(i);
                self.sel_more.clear();
            }
        }
        let mut s = fit * self.zoom;
        let origin_for = |s: f32, pan: Vec2, spec: &SheetSpec| view.center() + pan + Vec2::new(-(spec.w as f32) * s / 2.0, spec.h as f32 * s / 2.0);
        let mut o = origin_for(s, self.pan, &cspec);

        if let Some(hp) = resp.hover_pos() {
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let wx = (hp.x - o.x) / s;
                let wy = (o.y - hp.y) / s;
                self.zoom = (self.zoom * partview::wheel_factor(scroll)).clamp(0.3, 40.0);
                s = fit * self.zoom;
                let o2 = origin_for(s, self.pan, &cspec);
                self.pan += hp - Pos2::new(o2.x + wx * s, o2.y - wy * s);
                o = origin_for(s, self.pan, &cspec);
            }
        }
        let to_s = |p: P| Pos2::new(o.x + p[0] as f32 * s, o.y - p[1] as f32 * s);
        let to_w = |q: Pos2| -> P { [((q.x - o.x) / s) as f64, ((o.y - q.y) / s) as f64] };

        self.cursor_mm = resp.hover_pos().map(to_w);
        let sheet_idx = self.cur_sheet;
        let has_sheet = sheet_idx < self.result.sheets.len();
        // Right-click (or S): the radial tool menu, for the part under the pointer.
        let ghosting = self.ghost.is_some() || self.cut_tool.is_some();
        if let Some(hp) = resp.hover_pos().filter(|_| !ghosting) {
            if self.radial_trigger(ui) {
                // On a part: the part ring for the selection; on empty sheet: the sheet ring (QR1).
                let w = to_w(hp);
                let under = self.result.sheets.get(sheet_idx).and_then(|sh| sh.iter().rposition(|p| nest::placed_contains(&self.parts, p, w)));
                if let Some(i) = under {
                    if !self.is_selected(i) {
                        self.set_selection(vec![i]);
                    }
                }
                self.open_radial(ui.ctx(), hp, if under.is_some() { radial::Ring::OnPart } else { radial::Ring::Sheet });
                if let Some(r) = self.radial.as_mut() {
                    r.at = Some(w);
                }
            }
        }

        if ghosting {
            // Dropping parts: clicks drop the ghost (see ghost.rs); dragging pans.
            if resp.dragged_by(egui::PointerButton::Primary) && self.ghost.is_none_or(|g| !g.dragging) {
                self.pan += resp.drag_delta();
            }
        } else if has_sheet {
            // Select, drag (snap and slide), Ctrl+drag copies, window, pan (arrange_ui.rs).
            self.sheet_pointer(ui, &resp, &to_w, s);
        } else if resp.dragged() {
            self.pan += resp.drag_delta();
        }

        let srect = Rect::from_two_pos(to_s([0.0, 0.0]), to_s([cspec.w, cspec.h]));
        match cformat.outline.as_ref().filter(|o| o.outer.len() >= 3) {
            // A remnant: its box dimmed, its real shape as the sheet.
            Some(ol) => {
                painter.rect_filled(srect, CornerRadius::ZERO, th().sheet.gamma_multiply(0.25));
                for r in region::free_rects(ol) {
                    painter.rect_filled(Rect::from_two_pos(to_s([r.x, r.y]), to_s([r.x + r.w, r.y + r.h])), CornerRadius::ZERO, th().sheet);
                }
                let edge = Stroke::new(1.5, th().text_dim);
                for ring in std::iter::once(&ol.outer).chain(ol.holes.iter()) {
                    let mut pts: Vec<Pos2> = ring.iter().map(|&p| to_s(p)).collect();
                    pts.extend(pts.first().copied());
                    painter.add(EShape::line(pts, edge));
                }
            }
            None => {
                painter.rect_filled(srect, CornerRadius::ZERO, th().sheet);
            }
        }
        if self.show_grid {
            let grid = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
            let mut x = 100.0;
            while x < cspec.w {
                painter.line_segment([to_s([x, 0.0]), to_s([x, cspec.h])], Stroke::new(1.0, grid));
                x += 100.0;
            }
            let mut y = 100.0;
            while y < cspec.h {
                painter.line_segment([to_s([0.0, y]), to_s([cspec.w, y])], Stroke::new(1.0, grid));
                y += 100.0;
            }
        }
        painter.rect_stroke(srect, CornerRadius::ZERO, Stroke::new(1.0, th().sheet_edge), egui::StrokeKind::Middle);

        self.hover_name = None;
        if has_sheet {
            let hover_w = resp.hover_pos().map(to_w);
            let sheet = &self.result.sheets[sheet_idx];
            let mt = crate::ct1::for_spec(&self.spec);
            let n = sheet.len().max(1) as f64;
            let stagger = (0.7 / n).min(0.03);
            let mut animating = false;
            for (i, pl) in sheet.iter().enumerate() {
                let part = &self.parts[pl.part];
                let base = part_color(part.color);
                let selected = self.is_selected(i);
                let hovered = hover_w.is_some_and(|w| nest::placed_contains(&self.parts, pl, w));
                if hovered {
                    self.hover_name = Some(format!(
                        "{}  ·  #{}\n{:.0} × {:.0} mm{}\nX {:.0}   Y {:.0}",
                        part.name,
                        i + 1,
                        pl.w,
                        pl.h,
                        &match (pl.angle(), pl.mirror) { (0, false) => String::new(), (a, m) => format!("  ·  turned {a}°{}", if m { ", mirrored" } else { "" }) },
                        pl.x,
                        pl.y
                    ));
                }
                // Parts drop onto the sheet one after another after a nest.
                let t = (((now - self.anim_start) - i as f64 * stagger) / 0.25).clamp(0.0, 1.0) as f32;
                if t < 1.0 {
                    animating = true;
                }
                if t <= 0.0 {
                    continue;
                }
                let ease = 1.0 - (1.0 - t) * (1.0 - t);
                let drop = (1.0 - ease) * 14.0;
                let fill = if hovered && !selected { base.gamma_multiply(1.2) } else { base };
                let fill = if t < 1.0 { fill.gamma_multiply(ease) } else { fill };
                let invalid = self.drag.as_ref().is_some_and(|d| !d.valid && d.sel.contains(&i));
                let edge = if invalid {
                    Stroke::new(2.5, Color32::from_rgb(248, 80, 90))
                } else if selected {
                    Stroke::new(2.5, Color32::WHITE)
                } else {
                    Stroke::new(1.0, th().part_edge.gamma_multiply(ease))
                };
                if selected {
                    // Soft glow behind the selected part.
                    let r = Rect::from_two_pos(to_s([pl.x, pl.y]), to_s([pl.x + pl.w, pl.y + pl.h])).expand(4.0);
                    painter.rect_filled(r, CornerRadius::same(4), Color32::from_rgba_unmultiplied(255, 255, 255, 40));
                }
                paint_part_placed(&painter, part, pl, |q| to_s(q) - Vec2::new(0.0, drop), fill, edge, th().sheet);
                // Every contour in its cut quality's or state's colour (CS7).
                crate::cutpanel::paint_contour_colours(&painter, part, &mt, &|q| to_s(pl.map(q)) - Vec2::new(0.0, drop), 1.2, false);
                // Micro-joints as the cut plan places them (automatic ones too, holes
                // included): small uncut bridges on the contours.
                let planned: Vec<(&[P], (f64, f64))> = if self.drag.is_none() {
                    self.plans.get(sheet_idx).map_or(vec![], |plan| {
                        plan.contours.iter().filter(|c| c.part_no == i + 1).flat_map(|c| c.joints.iter().map(move |&j| (c.points.as_slice(), j))).collect()
                    })
                } else {
                    vec![]
                };
                for (pts, (a, b)) in planned {
                    let half = ((b - a) / 2.0).max(3.5 / s as f64);
                    let mid = (a + b) / 2.0;
                    {
                        let pa = to_s(cnc::point_at(pts, mid - half)) - Vec2::new(0.0, drop);
                        let pb = to_s(cnc::point_at(pts, mid + half)) - Vec2::new(0.0, drop);
                        painter.line_segment([pa, pb], Stroke::new(3.0, th().sheet));
                        let m = pa + (pb - pa) * 0.5;
                        painter.circle_filled(m, 2.2, Color32::from_rgb(250, 204, 21));
                    }
                }
                if self.show_numbers {
                    let size = (pl.w.min(pl.h) as f32 * s * 0.35).clamp(0.0, 15.0);
                    if size >= 7.0 {
                        let c = if matches!(part.shape, Shape::Triangle | Shape::LBracket) {
                            to_s([pl.x + pl.w * 0.3, pl.y + pl.h * 0.3])
                        } else {
                            to_s([pl.x + pl.w / 2.0, pl.y + pl.h / 2.0])
                        };
                        painter.text(c, Align2::CENTER_CENTER, format!("{}", i + 1), FontId::proportional(size), Color32::BLACK);
                    }
                }
            }
            if animating {
                ui.ctx().request_repaint();
            }
            if self.show_path && self.drag.is_none() {
                if let Some(plan) = self.plans.get(sheet_idx) {
                    let mut pos = to_s([0.0, 0.0]);
                    let rapid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 120, 120, 200));
                    for c in &plan.contours {
                        let li = to_s(c.lead_in);
                        painter.extend(EShape::dashed_line(&[pos, li], rapid, 4.0, 4.0));
                        if let Some(pu) = &c.punch {
                            // Punched: a cross per hit (one for a standard shape, a row when nibbling).
                            let pc = Color32::from_rgb(255, 140, 60);
                            for &h in &pu.hits {
                                let q = to_s(h);
                                painter.line_segment([q - Vec2::new(3.0, 3.0), q + Vec2::new(3.0, 3.0)], Stroke::new(1.3, pc));
                                painter.line_segment([q - Vec2::new(3.0, -3.0), q + Vec2::new(3.0, -3.0)], Stroke::new(1.3, pc));
                            }
                            pos = to_s(*pu.hits.last().unwrap_or(&c.points[0]));
                            continue;
                        }
                        let st = to_s(c.points[0]);
                        // System compensation: the beam's centre path beside the edge (KC8).
                        if c.comp.system {
                            let mut pts: Vec<Pos2> = c.points.iter().map(|&q| to_s(q)).collect();
                            pts.push(pts[0]);
                            painter.add(EShape::line(pts, Stroke::new(1.0, crate::partview::KERF_PATH)));
                        }
                        // Lead-in as cut: line, line + arc, arc, or from a hole's centre.
                        let lp: Vec<Pos2> = c.lead_path.iter().map(|&q| to_s(q)).collect();
                        painter.add(EShape::line(lp, Stroke::new(1.6, Color32::WHITE)));
                        painter.circle_filled(li, 2.0, Color32::WHITE);
                        // Loops at sharp corners.
                        for (vi, l) in &c.loops {
                            let mut pts = vec![to_s(c.points[*vi])];
                            pts.extend(l.iter().map(|&q| to_s(q)));
                            painter.add(EShape::line(pts, Stroke::new(1.4, Color32::from_rgb(250, 204, 21))));
                        }
                        if let Some(lo) = c.lead_out {
                            painter.line_segment([st, to_s(lo)], Stroke::new(1.4, Color32::from_rgb(129, 140, 248)));
                        }
                        pos = st;
                    }
                }
            }
        }
        if has_sheet {
            self.arrange_canvas(&painter, &to_s);
            self.carry_canvas(&painter, &to_s);
        }
        // Hover card next to the pointer.
        if let (Some(info), Some(hp), None, false, None) = (&self.hover_name, resp.hover_pos(), &self.drag, ghosting, &self.carry) {
            let galley = painter.layout(info.clone(), FontId::proportional(12.5), th().text, 260.0);
            let size = galley.size() + Vec2::new(20.0, 14.0);
            let mut pos = hp + Vec2::new(18.0, 18.0);
            if pos.x + size.x > rect.right() - 6.0 {
                pos.x = hp.x - size.x - 12.0;
            }
            if pos.y + size.y > rect.bottom() - 6.0 {
                pos.y = hp.y - size.y - 12.0;
            }
            let card = Rect::from_min_size(pos, size);
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(8), Color32::from_black_alpha(90));
            painter.rect_filled(card, CornerRadius::same(8), th().card);
            painter.rect_filled(Rect::from_min_size(card.min + Vec2::new(0.0, 8.0), Vec2::new(3.0, size.y - 16.0)), CornerRadius::same(2), th().accent);
            painter.galley(card.min + Vec2::new(11.0, 7.0), galley, th().text);
        }

        // Manual cutting: badges on picked contours, and picking while the mode is on.
        if has_sheet {
            if self.manual_mode {
                self.manual_canvas(ui, &painter, &resp, &to_s, &to_w, s);
            }
            self.manual_badges(&painter, &to_s);
        }
        if has_sheet && self.ssim.is_some() {
            self.sheet_sim_canvas(ui, &painter, rect, &to_s);
        }
        if has_sheet {
            self.verify_canvas(&painter, &to_s);
            self.overlap_canvas(&painter, rect, &to_s);
            // The warning leads to each part in the way, one click at a time.
            let bad = self.problem_parts();
            if !bad.is_empty() {
                let b = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 50.0), Vec2::new(170.0, 26.0));
                let k = self.problem_cursor % bad.len();
                let label = format!("Show problem {} of {} ▸", k + 1, bad.len());
                if ui.put(b, egui::Button::new(RichText::new(label).strong())).on_hover_text("Zoom to the next part that overlaps another or sticks out past the sheet, and select it").clicked() {
                    self.focus_placed = Some(bad[k]);
                    self.problem_cursor = k + 1;
                }
            }
        }
        if has_sheet || self.cut_tool.is_some() {
            self.cut_canvas(ui, &painter, &resp, &to_s, &to_w, s);
        }
        if self.ghost.is_some() {
            self.ghost_canvas(ui, &painter, rect, &resp, &to_s, &to_w, s);
        }

        self.canvas_overlays(ui, rect, now);
    }

    /// Floating zoom / sheet bar, usage chip and pop-up notices on the canvas.
    fn canvas_overlays(&mut self, ui: &mut egui::Ui, rect: Rect, now: f64) {
        let painter = ui.painter_at(rect);
        let n = self.result.sheets.len();
        // --- floating upright toolbar in the right-hand margin ---
        // (zoom in, zoom %, zoom out, fit, previous sheet, sheet n/m, next sheet)
        let items: [(&str, f32); 7] = [("+", 32.0), ("", 22.0), ("−", 32.0), ("Fit", 32.0), ("‹", 30.0), ("", 22.0), ("›", 30.0)];
        let total: f32 = items.iter().map(|i| i.1).sum::<f32>() + 12.0;
        let bw = 48.0;
        let bar = Rect::from_min_size(Pos2::new(rect.right() - bw - 12.0, rect.center().y - total / 2.0), Vec2::new(bw, total));
        painter.rect_filled(bar.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(14), Color32::from_black_alpha(60));
        painter.rect_filled(bar, CornerRadius::same(14), th().overlay);
        painter.rect_stroke(bar, CornerRadius::same(14), Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)), egui::StrokeKind::Inside);
        let mut y = bar.top() + 6.0;
        for (k, (label, h)) in items.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(bar.left() + 4.0, y), Vec2::new(bw - 8.0, *h));
            y += h;
            let text = match k {
                1 => format!("{:.0}%", self.zoom * 100.0),
                5 => format!("{} / {}", if n > 0 { self.cur_sheet + 1 } else { 0 }, n),
                _ => label.to_string(),
            };
            let clickable = !matches!(k, 1 | 5);
            let enabled = match k {
                4 => self.cur_sheet > 0,
                6 => self.cur_sheet + 1 < n,
                _ => true,
            };
            if k == 4 {
                painter.line_segment(
                    [Pos2::new(bar.left() + 10.0, r.top() - 1.0), Pos2::new(bar.right() - 10.0, r.top() - 1.0)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
                );
            }
            let mut col = if enabled { Color32::WHITE } else { Color32::from_rgba_unmultiplied(255, 255, 255, 70) };
            if clickable && enabled {
                let resp = ui.interact(r, ui.scope_id().with(("ov", k)), Sense::click());
                let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
                if h > 0.0 {
                    painter.rect_filled(r.shrink(1.0), CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, (h * 45.0) as u8));
                }
                if resp.hovered() {
                    col = Color32::from_rgb(199, 210, 254);
                }
                if resp.clicked() {
                    match k {
                        0 => self.zoom = (self.zoom * 1.25).min(40.0),
                        2 => self.zoom = (self.zoom / 1.25).max(0.3),
                        3 => {
                            self.zoom = 1.0;
                            self.pan = Vec2::ZERO;
                        }
                        4 => self.select_sheet(self.cur_sheet - 1),
                        6 => self.select_sheet(self.cur_sheet + 1),
                        _ => {}
                    }
                }
            }
            let size = if matches!(k, 0 | 2 | 4 | 6) { 18.0 } else { 11.5 };
            painter.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(size), col);
        }

        // --- compact usage card in the left-hand margin ---
        if n > 0 {
            let (u, _, cut_x) = self.sheet_usage(self.cur_sheet);
            let card = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.bottom() - 70.0), Vec2::new(104.0, 58.0));
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(10), Color32::from_black_alpha(60));
            painter.rect_filled(card, CornerRadius::same(10), th().overlay);
            painter.text(card.left_top() + Vec2::new(10.0, 7.0), Align2::LEFT_TOP, "Usage", FontId::proportional(11.0), Color32::from_rgb(161, 161, 170));
            painter.text(card.right_top() + Vec2::new(-10.0, 5.0), Align2::RIGHT_TOP, format!("{u:.1}%"), FontId::proportional(14.0), Color32::WHITE);
            let track = Rect::from_min_size(card.left_top() + Vec2::new(10.0, 27.0), Vec2::new(card.width() - 20.0, 5.0));
            painter.rect_filled(track, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 50));
            let mut f = track;
            f.set_width(track.width() * (u as f32 / 100.0).clamp(0.0, 1.0));
            painter.rect_filled(f, CornerRadius::same(3), Color32::from_rgb(129, 140, 248));
            painter.text(
                card.left_bottom() + Vec2::new(10.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("Free from X {cut_x:.0}"),
                FontId::proportional(10.5),
                Color32::from_rgb(161, 161, 170),
            );
        }

        // --- what the active tool wants next, in view (not only in the status bar) ---
        let hint = if self.ghost.is_some() {
            Some("Click to drop the part  ·  R or Space turns it  ·  M mirrors  ·  Enter finds a free spot  ·  Esc or right-click cancels")
        } else if self.manual_mode {
            Some("Click the contours in the order to cut them  ·  Esc ends")
        } else {
            match self.cut_tool {
                Some(cutoff::CutTool::Straight) => Some("Click where the cut line starts, then where it ends  ·  Esc stops"),
                Some(cutoff::CutTool::Path) => Some("Click the corners of the cut path  ·  double-click or Enter finishes  ·  Esc stops"),
                Some(cutoff::CutTool::Semi(_)) => Some("Click an empty area to make it a remnant  ·  Esc stops"),
                None => None,
            }
        };
        if let Some(h) = hint {
            let galley = painter.layout_no_wrap(h.to_string(), FontId::proportional(13.0), Color32::WHITE);
            let size = galley.size() + Vec2::new(28.0, 14.0);
            // At the top of the drawing, out of the way of the work; when the
            // pointer comes near it, it moves to the bottom.
            let top = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 12.0 + size.y / 2.0), size);
            let near = ui.ctx().pointer_hover_pos().is_some_and(|p| top.expand(40.0).contains(p));
            let pill = if near { Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 24.0 - size.y / 2.0), size) } else { top };
            painter.rect_filled(pill, CornerRadius::same(10), th().overlay);
            painter.galley(pill.min + Vec2::new(14.0, 7.0), galley, Color32::WHITE);
        }

        // --- what the last import / technology command did, with next steps ---
        let mut tech_card_h = 0.0;
        if let Some(card) = self.result_card.clone() {
            let mut close = false;
            let mut act = None;
            let r = egui::Area::new(egui::Id::unique("result_card")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(12.0, 12.0)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.horizontal(|ui| {
                        let (mark, col) = if card.warn { ("⚠", th().warn_fg) } else { ("✔", th().ok_fg) };
                        ui.label(RichText::new(format!("{mark} {}", card.title)).strong().color(col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    for l in &card.lines {
                        ui.label(RichText::new(l).size(12.0).color(th().text_dim));
                    }
                    if !card.actions.is_empty() {
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            for (k, (label, a)) in card.actions.iter().enumerate() {
                                let b = if k == 0 { egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(th().accent) } else { egui::Button::new(label) };
                                if ui.add(b).on_hover_text(a.tip()).clicked() {
                                    act = Some(*a);
                                }
                            }
                        });
                    }
                });
            });
            tech_card_h = r.response.rect.height() + 8.0;
            if close {
                self.result_card = None;
            }
            if let Some(a) = act {
                self.result_card = None;
                match a {
                    CardAct::TechAll(k) => {
                        self.tech_all = true;
                        if self.workspace == Workspace::Nesting {
                            self.tech_nest_all = Some(true);
                        }
                        self.run_tech(k);
                    }
                    CardAct::NestAll => self.run_nesting(),
                    CardAct::ShowPart(i) => {
                        self.sel_part = Some(i);
                        self.scroll_to_part = Some(i);
                        // Keep the card: it still says what came in.
                        self.result_card = Some(card.clone());
                    }
                    CardAct::SheetTab => self.tab = Tab::Sheet,
                }
            }
        }

        // --- the last export: where the files went, until closed ---
        if let Some((dir, files, warning)) = self.last_export.clone() {
            let mut close = false;
            egui::Area::new(egui::Id::unique("last_export")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(12.0, 12.0 + tech_card_h)).show(ui.ctx(), |ui| {
                egui::Frame::new().fill(th().card).stroke(Stroke::new(1.0, th().border)).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_max_width(330.0);
                    ui.horizontal(|ui| {
                        let (ok_col, title) = if warning.is_some() { (th().warn_fg, "Programs written — with a warning") } else { (th().ok_fg, "Programs written") };
                        ui.label(RichText::new(format!("✔ {title}")).strong().color(ok_col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    let shown: Vec<&String> = files.iter().take(4).collect();
                    for f in &shown {
                        ui.label(RichText::new(f.as_str()).monospace().size(12.0));
                    }
                    if files.len() > shown.len() {
                        ui.label(RichText::new(format!("and {} more", files.len() - shown.len())).size(12.0).color(th().text_dim));
                    }
                    ui.label(RichText::new(dir.display().to_string()).size(11.5).color(th().text_dim));
                    if let Some(w) = &warning {
                        ui.label(RichText::new(w).size(12.0).color(th().warn_fg));
                    }
                    ui.add_space(4.0);
                    if ui.button("📂 Open folder").clicked() {
                        open_folder(&dir);
                    }
                });
            });
            if close {
                self.last_export = None;
            }
        }

        // --- pop-up notices (bottom-right), fade out after a few seconds ---
        // Long messages (an export's folder, what a nesting removed) stay longer.
        let life = |m: &str| (4.0 + m.len() as f64 / 40.0).min(9.0);
        self.toasts.retain(|t| now - t.1 < life(&t.0));
        let mut y = rect.bottom() - 14.0;
        for (msg, t0) in self.toasts.iter().rev().take(3) {
            let age = (now - t0) as f32;
            let end = life(msg) as f32;
            let alpha = if age < 0.2 { age / 0.2 } else if age > end - 0.6 { ((end - age) / 0.6).max(0.0) } else { 1.0 };
            let galley = painter.layout(msg.clone(), FontId::proportional(12.5), Color32::WHITE, 420.0);
            let size = galley.size() + Vec2::new(28.0, 14.0);
            let slide = (1.0 - alpha) * 12.0;
            let card = Rect::from_min_size(Pos2::new(rect.right() - size.x - 14.0 + slide, y - size.y), size);
            painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(10), Color32::from_black_alpha((70.0 * alpha) as u8));
            painter.rect_filled(card, CornerRadius::same(10), Color32::from_rgba_unmultiplied(24, 24, 30, (240.0 * alpha) as u8));
            painter.circle_filled(card.left_center() + Vec2::new(11.0, 0.0), 3.5, Color32::from_rgba_unmultiplied(129, 140, 248, (255.0 * alpha) as u8));
            painter.galley(card.min + Vec2::new(20.0, 7.0), galley, Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * alpha) as u8));
            y -= size.y + 8.0;
        }
        if !self.toasts.is_empty() {
            ui.ctx().request_repaint();
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.drag_test_on {
            self.drag_test_input(ctx, raw, self.frames + 1);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames += 1;
        self.quit_guard(&ctx);
        // The active job (ORDENACT.SYS in the original): the open job's library
        // record, so it opens again next time. Not while taking screenshots.
        if self.shot.is_none() {
            let want = self.job_lib_id.map(|i| i.to_string()).unwrap_or_default();
            if self.cfg.active_job != want {
                self.cfg.active_job = want;
            }
        }
        let speed = 40.0 * self.cfg.scroll_speed;
        if (ctx.options(|o| o.input_options.line_scroll_speed) - speed).abs() > 0.01 {
            ctx.options_mut(|o| o.input_options.line_scroll_speed = speed);
        }
        if self.shot.is_none() {
            let z = settings::ui_zoom(self.cfg.ui_size);
            if (ctx.zoom_factor() - z).abs() > 1e-3 {
                ctx.set_zoom_factor(z);
            }
        }
        // An agent or script may change the open job's file (CLI / MCP).
        self.watch_job_file(&ctx);
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_job(false);
        }
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S)) {
            self.save_job(true);
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.open_job_dialog();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.open_settings(settings_ui::Section::General);
        }
        self.settings_window(&ctx);
        self.library_window(&ctx);
        self.import_window(&ctx);
        self.stock_window(&ctx);
        if self.demo_open_menu {
            egui::Area::new(egui::Id::unique("demo_open_menu")).order(egui::Order::Foreground).fixed_pos(Pos2::new(8.0, 138.0)).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| self.open_job_menu(ui));
            });
        }
        self.erp_window(&ctx);
        self.naming_window(&ctx);
        self.db_browser(&ctx);
        self.nest_verify_window(&ctx);
        if self.follow_system {
            if let Some(t) = ctx.system_theme() {
                let dark = t == egui::Theme::Dark;
                if dark != theme::is_dark() {
                    theme::set_dark(&ctx, dark);
                }
            }
        }
        if self.shot.is_some() && self.frames == 60 {
            self.shot_arrange();
        }
        if let Some(path) = &self.shot {
            ctx.request_repaint();
            if self.frames == 120 {
                let path = path.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut bytes = Vec::with_capacity(w * h * 4);
                    for c in &img.pixels {
                        bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let code = match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                        Some(im) => match im.save(&path) {
                            Ok(_) => 0,
                            Err(e) => {
                                eprintln!("screenshot save failed: {e}");
                                1
                            }
                        },
                        None => 1,
                    };
                    std::process::exit(code);
                });
            }
            if self.frames > 900 {
                eprintln!("screenshot timed out");
                std::process::exit(2);
            }
        }

        // Drawings dropped onto the window become products.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| formats::is_drawing(p) || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xml")))
                .collect()
        });
        if !dropped.is_empty() {
            self.import_drawings(dropped);
        }
        // A copy on the pointer ends when its sheet changes, the screen
        // changes or another tool starts (QR4).
        self.check_carry();
        if self.workspace == Workspace::Nesting {
            // Alt/Ctrl + arrows, Ctrl+Q/W, M, Ctrl+E… on the selected parts.
            self.sheet_keys(&ctx);
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing {
            let (undo, redo, dup) = ctx.input_mut(|i| {
                let redo = i.consume_key(egui::Modifiers::COMMAND, Key::Y)
                    || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z);
                (i.consume_key(egui::Modifiers::COMMAND, Key::Z), redo, i.consume_key(egui::Modifiers::COMMAND, Key::D))
            });
            if undo {
                self.undo();
            }
            if redo {
                self.redo();
            }
            if dup {
                self.duplicate_selected();
            }
            let (f5, r, del) = ctx.input(|i| (i.key_pressed(Key::F5), i.key_pressed(Key::R) && i.modifiers.is_none(), i.key_pressed(Key::Delete)));
            if f5 {
                self.run_nesting();
            }
            if r {
                self.rotate_selected();
            }
            if del {
                self.remove_selected();
            }
        }
        if self.status != self.last_status {
            self.last_status = self.status.clone();
            if !self.status.is_empty() {
                let now = ctx.input(|i| i.time);
                self.toasts.push((self.status.clone(), now));
            }
        }

        egui::Panel::top("title")
            .frame(egui::Frame::new().fill(th().chrome).inner_margin(egui::Margin::symmetric(6, 4)))
            .show_separator_line(false)
            .show(ui, |ui| self.title_row(ui));
        egui::Panel::top("tabs")
            .frame(egui::Frame::new().fill(th().chrome).inner_margin(egui::Margin { left: 0, right: 0, top: 3, bottom: 0 }))
            .show_separator_line(false)
            .show(ui, |ui| self.ribbon_tabs(ui));
        // One fixed height for every tab, so the drawing never jumps when
        // switching tabs.
        egui::Panel::top("ribbon")
            .exact_size(crate::ui_parts::RIBBON_H)
            .frame(
                egui::Frame::new()
                    .fill(th().chrome)
                    .inner_margin(egui::Margin::symmetric(4, 4))
                    .stroke(Stroke::new(1.0, th().border)),
            )
            .show(ui, |ui| self.ribbon(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(th().chrome).stroke(Stroke::new(1.0, th().border)).inner_margin(egui::Margin::symmetric(8, 3)))
            .show(ui, |ui| {
                match self.workspace {
                    Workspace::Jobs => self.jobs_status_bar(ui),
                    Workspace::Part => self.part_status_bar(ui),
                    Workspace::Nesting => self.status_bar(ui),
                }
            });
        if self.workspace == Workspace::Part {
            self.part_workspace(ui);
            self.radial_ui(&ctx);
            self.text_editor_ui(&ctx);
            self.dock_overlay(&ctx);
            self.pending_dialog(&ctx);
            self.joint_sizes_window(&ctx);
            return;
        }
        if self.workspace == Workspace::Jobs {
            self.jobs_workspace(ui);
            self.dock_overlay(&ctx);
            self.times_window(&ctx);
            self.pending_dialog(&ctx);
            self.joint_sizes_window(&ctx);
            return;
        }
        egui::Panel::bottom("bottom_tabs")
            .frame(egui::Frame::new().fill(th().pane_header).inner_margin(egui::Margin::symmetric(6, 2)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.bottom == BottomTab::PartsList, "≡ Parts List").clicked() {
                        self.bottom = BottomTab::PartsList;
                    }
                    if ui.selectable_label(self.bottom == BottomTab::PartsView, "▣ Parts View").clicked() {
                        self.bottom = BottomTab::PartsView;
                    }
                    ui.add_space(16.0);
                    self.parts_search_bar(ui);
                });
            });
        // Sized to one row of part cards; drag the edge for more.
        egui::Panel::bottom("parts_view")
            .resizable(true)
            .default_size(142.0)
            .min_size(120.0)
            .frame(egui::Frame::new().fill(th().panel))
            .show(ui, |ui| self.parts_view(ui));
        if self.show_program {
            egui::Panel::right("program")
                .resizable(true)
                .default_size(320.0)
                .frame(egui::Frame::new().fill(th().panel))
                .show(ui, |ui| self.program_panel(ui));
        }
        self.dock_panels(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(th().bg).inner_margin(egui::Margin::same(3)))
            .show(ui, |ui| self.canvas(ui));
        self.radial_ui(&ctx);
        self.dock_overlay(&ctx);
        self.times_window(&ctx);
        self.step_window(&ctx);
        self.pending_dialog(&ctx);
        self.joint_sizes_window(&ctx);
    }
}

impl App {
    fn times_window(&mut self, ctx: &egui::Context) {
        if self.show_times {
            let mut open = true;
            egui::Window::new("Times and costs")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::RIGHT_TOP, Vec2::new(-24.0, 176.0))
                .show(ctx, |ui| self.results(ui));
            if !open {
                self.show_times = false;
            }
        }
    }
}

/// Show a folder in the computer's file manager (Explorer on Windows).
pub fn open_folder(dir: &std::path::Path) {
    let prog = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(prog).arg(dir).spawn();
}

/// The Export CNC tooltip: what is written (every sheet), in which format and
/// where, so nobody has to guess before or after exporting.
pub fn export_tip(app: &App) -> String {
    let n = app.result.sheets.len();
    let what = if app.blm.on { "BLM LC5 program set" } else { "CNC program" };
    format!("Writes a {what} for each of the {n} sheet(s), plus a parts report, to\n{}\n(Settings → Folders to change it).", app.export_dir().display())
}

fn main() -> eframe::Result {
    std::panic::set_hook(Box::new(|info| eprintln!("panic: {info}")));
    // `nestpro <operation> …` and `nestpro mcp`: the command line and the MCP
    // server for agents and scripts, without opening the window.
    let argv: Vec<String> = std::env::args().collect();
    if cli::is_cli(&argv) {
        std::process::exit(cli::main(argv));
    }
    // Letting the graphics layer pick among every backend at once closes the
    // window silently on some Windows PCs; one explicit backend is reliable.
    #[cfg(windows)]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: called at start-up before any other thread exists.
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let want_dark = args.iter().any(|a| a == "--dark");
    let want_light = args.iter().any(|a| a == "--light");
    let want_jobs = args.iter().any(|a| a == "--jobs");
    let want_tech = args.iter().any(|a| a == "--tech");
    let want_part = args.iter().any(|a| a == "--part");
    let want_float = args.iter().any(|a| a == "--float-panel");
    let want_radial = args.iter().any(|a| a == "--radial");
    let want_manual = args.iter().any(|a| a == "--manual");
    let want_ghost = args.iter().any(|a| a == "--ghost");
    let want_ghost_ring = args.iter().any(|a| a == "--ghost-ring");
    let want_cutoff = args.iter().any(|a| a == "--cutoff");
    let want_part_pan = args.iter().any(|a| a == "--part-pan");
    // --machine=laser|plasma|oxyfuel|waterjet|laserpunch|punch (screenshots).
    let want_sim = args.iter().any(|a| a == "--sim");
    let want_times = args.iter().any(|a| a == "--times");
    let want_db = args.iter().any(|a| a == "--db");
    let want_post = args.iter().any(|a| a == "--post");
    let want_dbb = args.iter().any(|a| a == "--dbbrowse");
    let want_lib = args.iter().any(|a| a == "--library");
    let job_save = args.iter().find_map(|a| a.strip_prefix("--save-job=").map(PathBuf::from));
    let job_open = args.iter().find_map(|a| a.strip_prefix("--open-job=").map(PathBuf::from));
    let open_nest = args.iter().find_map(|a| a.strip_prefix("--open-nest=").map(PathBuf::from));
    // Another program hands parts over the way it would to Expert: a Lantek
    // import XML (DXF Import / Parts Import schema).
    let import_xml = args.iter().find_map(|a| a.strip_prefix("--import-xml=").map(PathBuf::from));
    let want_verify = args.iter().any(|a| a == "--verify");
    let machine_arg = args.iter().find_map(|a| a.strip_prefix("--machine=").map(|s| s.to_lowercase()));
    // --part-tab=machining|view|simulation|notes|utilities|technology (screenshots).
    let part_tab = args.iter().find_map(|a| a.strip_prefix("--part-tab=").map(|s| s.to_string()));
    let want_edit_text = args.iter().any(|a| a == "--edit-text");
    let want_drag_test = args.iter().any(|a| a == "--drag-test");
    let (icon, _icon_file) = iconfile::window_icon();
    // Like the original's start-up (rules SU3, SU4): the first
    // plain word may name a file to open, and the window opens maximized
    // (SW_SHOWMAXIMIZED) — except for screenshots, which need a fixed size.
    // "/hide" isn't followed: a hidden window could never be reached here.
    let first = startup::first_word(&args);
    let start_file = startup::start_file(first, jobfile::EXT);
    let maximized = shot.is_none() && first != Some(startup::HIDE_SWITCH) && startup::show_state(first) == startup::SW_SHOWMAXIMIZED;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([900.0, 600.0])
            .with_maximized(maximized)
            .with_title("Nesting and Machining - Sheet Nesting")
            // The black icon (assets/icon.png) for the title bar and taskbar,
            // or nestpro.ico / nestpro.png when one is there.
            .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon.rgba, width: icon.width, height: icon.height })),
        ..Default::default()
    };
    let res = eframe::run_native(
        "Sheet Nesting",
        options,
        Box::new(move |cc| {
            // Start in the mode asked for on the command line, otherwise follow
            // the computer's own light/dark setting (light if unknown).
            let dark = if want_dark {
                true
            } else if want_light || shot.is_some() {
                false
            } else {
                cc.egui_ctx.system_theme() == Some(egui::Theme::Dark)
            };
            theme::set_dark(&cc.egui_ctx, dark);
            let load_layout = shot.is_none();
            let mut app = App::new(shot);
            // No mode asked for: keep following Windows, like the original's
            // default style (rule V5).
            app.follow_system = !want_dark && !want_light && load_layout;
            // Without the example jobs: one empty job (not for screenshots).
            if load_layout && !app.cfg.examples {
                let spec = app.spec.clone();
                app.jobs = vec![Job { week: String::new(), name: "New job".into(), parts: vec![], spec, result: nest::NestResult::default(), stale: false, lib_id: None, cut_lines: vec![], manual_cuts: vec![], cam_changed: None, held: Default::default(), erp: None, file: None, saved: None }];
                app.cur_job = 0;
                app.job_name = "New job".into();
                app.parts.clear();
                app.result = nest::NestResult::default();
                app.sel_part = None;
                app.cur_sheet = 0;
                app.cut_lines.clear();
                app.refresh();
            }
            // Reopen the active job (rule AJ2: no record = no active job).
            if load_layout {
                if let Some(id) = crate::jobtree::active_job_id(&app.cfg.active_job) {
                    app.open_library_job(id);
                    if app.job_lib_id != Some(id) {
                        app.cfg.active_job.clear();
                    } else {
                        app.status = format!("Reopened the active job {}.", app.job_name);
                    }
                }
            }
            // Saved preferences (Settings → General / Appearance).
            app.warn_tech = app.cfg.warn_tech;
            app.dv.show_unbuilt = app.cfg.show_unbuilt;
            if !want_dark && !want_light && load_layout {
                match app.cfg.theme.as_str() {
                    "light" | "dark" => {
                        app.follow_system = false;
                        theme::set_dark(&cc.egui_ctx, app.cfg.theme == "dark");
                    }
                    _ => app.follow_system = true,
                }
            }
            if std::env::args().any(|a| a == "--options") {
                app.options_open = true;
            }
            if let Some(s) = std::env::args().find_map(|a| a.strip_prefix("--settings=").map(str::to_string)) {
                use settings_ui::Section::*;
                let sec = match s.as_str() {
                    "appearance" => Appearance,
                    "shortcuts" => Shortcuts,
                    "about" => About,
                    "connections" => Connections,
                    "machines" => Machines,
                    "materials" => Materials,
                    "erp" | "integrations" => Integrations,
                    "folders" => Folders,
                    "nesting" => Nesting,
                    _ => General,
                };
                app.open_settings(sec);
            }
            // Load the saved "ExpertLayout" (the original does this at start, D6).
            if load_layout {
                if let Some(d) = dock::Dock::load() {
                    app.dock = d;
                }
            }
            if want_part {
                if let Some(i) = app.parts.iter().position(|p| p.name == "Square hole") {
                    app.open_part(i);
                    // Show a picked element: the first line of the outer contour.
                    let el = nest::elements_of(&app.parts[i].geometry());
                    if let Some(k) = el.iter().position(|e| e.contour == app.parts[i].geometry().holes.len()) {
                        app.elem_part = Some(i);
                        app.sel_elems = vec![k];
                    }
                }
            }
            if let Some(t) = &part_tab {
                app.tab = match t.as_str() {
                    "technology" => Tab::Technology,
                    "machining" => Tab::Machining,
                    "view" => Tab::View,
                    "simulation" => {
                        app.dv.sim = Some(0.0);
                        Tab::Simulation
                    }
                    "notes" => Tab::Notes,
                    "utilities" => {
                        app.dv.show_unbuilt = true;
                        Tab::Utilities
                    }
                    "missing" => {
                        app.dv.show_missing = true;
                        Tab::Home
                    }
                    _ => Tab::Home,
                };
            }
            app.apply_settings(true);
            // --part-sim-at=<share>: the part's simulation that far through its program.
            if let Some(f) = args.iter().find_map(|a| a.strip_prefix("--part-sim-at=").and_then(|v| v.parse::<f64>().ok())) {
                let total: f64 = app.part_sim_ops().0.iter().map(|o| o.secs).sum();
                app.dv.sim = Some(total * f.clamp(0.0, 1.0));
                app.dv.sim_follow = true;
            }
            // --part-zoom=<z>: the part view zoomed in (with --part-sim-at, on the head).
            if let Some(z) = args.iter().find_map(|a| a.strip_prefix("--part-zoom=").and_then(|v| v.parse::<f32>().ok())) {
                app.part_zoom = z.clamp(crate::partview::PART_ZOOM_MIN, crate::partview::PART_ZOOM_MAX);
            }
            // The database's jobs at start-up (Settings → General).
            if load_layout && app.cfg.auto_db && app.db_ready() {
                app.db_open_now();
            }
            // --export: write the programs at start (checks the BLM set).
            if std::env::args().any(|a| a == "--export") {
                app.export();
            }
            if want_verify {
                app.tab = Tab::Machining;
                app.nverify.all_sheets = true;
                app.verify_nest();
            }
            if let Some(p) = &job_save {
                app.save_job_to(p);
            }
            if let Some(p) = &job_open {
                app.open_job_file(p);
                // With --part, the opened job's first part in the Drawing module.
                if want_part {
                    app.open_part(0);
                }
            }
            if let Some(p) = &import_xml {
                app.import_xml(p);
                app.workspace = Workspace::Jobs;
            }
            if let Some(p) = &open_nest {
                app.open_nesting_file(p);
            }
            // A file named on the command line (double-click), opened once the
            // window is up, as the original does with a ".NXP" (SU3).
            match &start_file {
                Some(startup::StartFile::Nesting(p)) => app.open_nesting_file(p),
                Some(startup::StartFile::Job(p)) => app.open_job_file(p),
                None => {}
            }
            // --library-dir=<folder> / --lib-save: screenshots of a filled library.
            if let Some(d) = std::env::args().find_map(|a| a.strip_prefix("--library-dir=").map(str::to_string)) {
                app.cfg.library_dir = d;
            }
            if std::env::args().any(|a| a == "--lib-save") {
                app.save_job_to_library();
                app.add_products_to_library();
            }
            if want_lib {
                app.workspace = Workspace::Jobs;
                app.libui.open = true;
            }
            // --import=<folder> [--import-pick=<file name>]: the Import drawings
            // window there, with that file picked and previewed (screenshots).
            if let Some(d) = std::env::args().find_map(|a| a.strip_prefix("--import=").map(PathBuf::from)) {
                app.importui.sync = true;
                app.importui.show(std::slice::from_ref(&d));
                if let Some(f) = std::env::args().find_map(|a| a.strip_prefix("--import-pick=").map(|n| d.join(n))) {
                    app.importui.focus = Some(f.clone());
                    app.importui.picked.insert(f);
                }
                // --import-panel (screenshots): the preview's cut technology panel
                // (IM11). --import-layer=<layer>=<cut|mark|ignore> sets a layer row
                // and --import-paint="7=mark; 1=leave-out" paints contours, both
                // through the panel's own functions; --import-row=<state> chooses a row.
                if std::env::args().any(|a| a == "--import-panel") {
                    app.importui.panel.open = true;
                    let (cfg, spec) = (app.cfg.clone(), app.spec.clone());
                    app.importui.want_preview(&cfg, &spec, &egui::Context::default());
                    let state = |w: &str| dxf::ContourState::ALL.into_iter().find(|s| s.key() == w.trim());
                    let row = |st: dxf::ContourState| match st {
                        dxf::ContourState::Cut => cutpanel::Row::Cut(None),
                        dxf::ContourState::Mark => cutpanel::Row::Mark,
                        dxf::ContourState::NoCut => cutpanel::Row::NoCut,
                        dxf::ContourState::BendLine => cutpanel::Row::BendLine,
                        dxf::ContourState::LeaveOut => cutpanel::Row::LeaveOut,
                    };
                    for a in std::env::args().filter_map(|a| a.strip_prefix("--import-layer=").map(str::to_string)) {
                        if let Some((l, r)) = a.rsplit_once('=') {
                            let role = dxf::LayerRole::ALL.into_iter().find(|x| x.label().eq_ignore_ascii_case(r) || (r == "ignore" && *x == dxf::LayerRole::Ignore));
                            if let Some(role) = role {
                                app.preview_set_layer(l, role);
                            }
                        }
                    }
                    if let Some(p) = std::env::args().find_map(|a| a.strip_prefix("--import-paint=").map(str::to_string)) {
                        for (k, st) in p.split(';').filter_map(|c| c.split_once('=')).filter_map(|(k, w)| Some((k.trim().parse::<usize>().ok()?.checked_sub(1)?, state(w)?))) {
                            app.preview_paint(&[k], row(st), false);
                        }
                    }
                    app.importui.panel.row = std::env::args().find_map(|a| a.strip_prefix("--import-row=").and_then(state)).map(row);
                }
            }
            // --nest-stock: nest again from the library's sheet stock;
            // --stock: show the Sheet stock window (screenshots).
            if std::env::args().any(|a| a == "--nest-stock") {
                app.run_nesting();
                app.anim_pending = false;
            }
            // --units=inch: lengths in inches (screenshots; --select is applied in `ui`).
            if std::env::args().any(|a| a == "--units=inch") {
                app.cfg.nesting.units = units::LenUnit::Inch;
            }
            // --remnant-mark: the automatic mark on remnants on (screenshots).
            if std::env::args().any(|a| a == "--remnant-mark") {
                app.cfg.stock.mark = remmark::MarkSettings { on: true, text: "#300 #301x#302".into(), height: 18.0, ..Default::default() };
            }
            // --remline=<code>: the automatic remnant line in that shape (screenshots).
            if let Some(c) = std::env::args().find_map(|a| a.strip_prefix("--remline=").and_then(|v| v.parse::<u8>().ok())) {
                if let Some(s) = remline::LineShape::ALL.into_iter().find(|s| s.code() == c) {
                    app.tab = Tab::Sheet;
                    app.cur_sheet = app.result.sheets.len().saturating_sub(1);
                    app.cfg.stock.line.shape = s;
                    app.cut_lines.clear();
                    app.auto_remnant_edge();
                }
            }
            // --stock-demo (screenshots): two made-up sheets in the stock, the
            // smaller one picked, so the window shows a real choice.
            if std::env::args().any(|a| a == "--stock-demo") {
                let (m, t) = (app.spec.material, app.spec.thickness);
                let _ = stock::Stock::update(&app.library_dir(), |s| {
                    s.add_sheet(stock::Item::sheet("DEMO-S1", m, t, 2000.0, 1000.0, 5), true)?;
                    let mut r = stock::Item::sheet("DEMO-R1", m, t, 1200.0, 1000.0, 1);
                    r.kind = stock::Kind::UserRemnant;
                    s.add_sheet(r, true)
                });
                app.stockui.sel = Some("DEMO-R1".into());
            }
            if std::env::args().any(|a| a == "--stock") {
                app.stockui.open = true;
                app.stockui.show_options = std::env::args().any(|a| a == "--stock-options");
            }
            // --erp: the ERP window filled with made-up work (screenshots, no network).
            if std::env::args().any(|a| a == "--erp") {
                app.workspace = Workspace::Jobs;
                app.erp_demo();
                // With --settings=erp the page is what is shown.
                app.erp.open = !std::env::args().any(|a| a == "--settings=erp");
            }
            if want_dbb {
                app.open_db_browser();
            }
            if want_post {
                app.tab = Tab::Machining;
                app.open_settings(settings_ui::Section::Machines);
                app.show_program = true;
            }
            if want_times {
                app.show_times = true;
            }
            if want_db {
                app.workspace = Workspace::Jobs;
                app.open_db_window();
            }
            if want_sim {
                // Halfway through the first sheet's program, Simulation tab open.
                app.tab = Tab::Simulation;
                let mut s = sheetsim::SheetSim::default();
                if let Some(plan) = app.plans.first() {
                    let names: Vec<String> = app.result.sheets[0].iter().map(|p| app.parts[p.part].name.clone()).collect();
                    let total: f64 = sheetsim::ops(plan, &names).iter().map(|o| o.secs).sum();
                    s.t = total * 0.45;
                }
                app.ssim = Some(s);
            }
            if let Some(mk) = &machine_arg {
                if let Some(k) = machine::MachineKind::ALL.into_iter().find(|k| k.label().to_lowercase().replace(['-', ' '], "").starts_with(mk.as_str())) {
                    app.spec.machine = k;
                    app.dv.std_shapes = true;
                    app.refresh();
                }
            }
            if want_part_pan {
                // Zoomed in and moved, as after scrolling and middle-dragging.
                app.part_zoom = 2.4;
                app.part_pan = Vec2::new(-260.0, 140.0);
            }
            app.drag_test_on = want_drag_test;
            if want_edit_text {
                if let Some(i) = app.shown_part() {
                    if app.parts[i].texts.len() > 1 {
                        app.edit_text_at(egui::Pos2::new(1150.0, 860.0), i, 1);
                    }
                }
            }
            if want_manual && !app.result.sheets.is_empty() {
                // Pick contours is on the Machining tab, with Undo pick and Clear.
                app.tab = Tab::Machining;
                app.manual_mode = true;
                let picks: Vec<nest::P> = app.result.sheets[0].iter().take(3).map(|p| [p.x + p.w * 0.5, p.y]).collect();
                for (k, p) in picks.into_iter().enumerate() {
                    app.manual_reverse = k == 1;
                    app.manual_pick(p, 1.0);
                }
                app.manual_reverse = false;
            }
            if want_ghost {
                // A part picked in the Parts View, hovering near a free spot.
                if let Some(i) = app.parts.iter().position(|p| p.name == "Square") {
                    app.start_ghost(i, false);
                    if let (Some(c), Some(g)) = (app.ghost_free_spot(), app.ghost.as_mut()) {
                        g.at = Some([c.x + c.w / 2.0 + 6.0, c.y + c.h / 2.0 + 30.0]);
                    }
                }
            }
            if want_ghost_ring {
                // A ring hovering by the pocket between two nested rings.
                if let Some(i) = app.parts.iter().position(|p| p.shape == Shape::Disc) {
                    // Lift the last ring off the sheet so there's a gap to drop it back into.
                    let lifted = app.result.sheets.first_mut().and_then(|s| s.iter().rposition(|p| p.part == i).map(|k| s.remove(k)));
                    app.refresh();
                    app.start_ghost(i, false);
                    let rings: Vec<(P, f64)> = app.result.sheets.first().map_or(vec![], |s| s.iter().filter_map(|o| nest::placed_circle(&app.parts, o)).collect());
                    let (lc, r) = nest::part_circle(&app.parts[i]).unwrap_or(([0.0, 0.0], 0.0));
                    let (fw, fh) = ghost::footprint(&app.parts[i], 0);
                    let mut spot = None;
                    'find: for a in 0..rings.len() {
                        for b in a + 1..rings.len() {
                            for p in nest::pocket_points(rings[a].0, rings[a].1, rings[b].0, rings[b].1, r, app.spec.gap) {
                                let c = Placed { part: i, x: p[0] - lc[0], y: p[1] - lc[1], w: fw, h: fh, turns: 0, mirror: false };
                                if app.ghost_ok(&c) {
                                    spot = Some(p);
                                    break 'find;
                                }
                            }
                        }
                    }
                    let spot = spot.or(lifted.map(|l| [l.x + lc[0], l.y + lc[1]]));
                    if let (Some(p), Some(g)) = (spot, app.ghost.as_mut()) {
                        g.at = Some([p[0] + 9.0, p[1] + 7.0]);
                    }
                }
            }
            if want_cutoff {
                // A stepped remnant line hugging the parts, a free cut path, and
                // the straight-line tool half-drawn.
                app.tab = Tab::Sheet;
                app.exact_remnant_edge();
                let (w, h) = (app.cur_spec().w, app.cur_spec().h);
                let x = app.cut_list().first().map_or(w * 0.8, |l| l.pts.iter().map(|p| p[0]).fold(0.0, f64::max));
                let path = vec![[x + 60.0, h * 0.62], [w - 60.0, h * 0.62], [w - 60.0, h * 0.9]];
                app.add_cut_line(cutoff::CutLine { pts: path, kind: cutoff::CutKind::Free });
                app.sel_cut = Some(0);
                if std::env::args().any(|a| a == "--export") {
                    app.export();
                }
            }
            if args.iter().any(|a| a == "--overlap") {
                // Collision checking off and the second part pushed onto the
                // first: the red outlines and the badge (QR5, QR6).
                app.collision = false;
                if let Some(s) = app.result.sheets.first_mut().filter(|s| s.len() > 1) {
                    s[1].x = s[0].x + s[0].w * 0.5;
                    s[1].y = s[0].y + s[0].h * 0.3;
                }
                app.refresh();
            }
            if want_radial {
                // The Part view's ring, or on the sheet the part ring with a
                // selection (--select=…) and the sheet ring without.
                let ring = if app.workspace == Workspace::Part {
                    radial::Ring::Part
                } else if args.iter().any(|a| a.starts_with("--select=")) {
                    radial::Ring::OnPart
                } else {
                    radial::Ring::Sheet
                };
                app.radial = Some(radial::Radial::new(egui::Pos2::new(1060.0, 640.0), ring));
            }
            if want_float {
                // Show a floating, a collapsed and a docked-right pane.
                let ws = app.workspace;
                let first = dock::PaneId::for_workspace(ws)[1];
                app.dock.apply_drop(ws, first, dock::Drop::Float(Pos2::new(1200.0, 300.0)));
                app.dock.toggle_auto_hide(dock::PaneId::for_workspace(ws)[0]);
            }
            if want_tech {
                app.tab = Tab::Technology;
                app.show_path = true;
                app.zoom = 3.0;
            }
            // --cut-ranges (screenshots): Quality by size on the plate with holes,
            // its rows and each contour's quality (CQ1) by the machine's names
            // and colours (CQ4, CQ5): rows at qualities 1, 2 and 3; on a job
            // opened with --open-job, its first part.
            if std::env::args().any(|a| a == "--cut-ranges") {
                app.tab = Tab::Technology;
                app.sel_part = app.parts.iter().position(|p| p.name == "Square hole").or((!app.parts.is_empty()).then_some(0));
                app.tech_nest_all = Some(false);
                app.show_cut_ranges = true;
                if let Some(t) = app.sel_part.and_then(|i| app.parts[i].tech.as_mut()) {
                    for (k, (from, to)) in [(0.0, 5.0), (5.0, 50.0), (50.0, 150.0)].into_iter().enumerate() {
                        if let Some(q) = crate::nest::CutQuality::new(k + 1) {
                            t.cut_ranges[k] = crate::nest::CutRange { on: true, min_r: from, max_r: to, quality: q };
                        }
                    }
                }
            }
            // --paint-panel / --paint-contours (screenshots): the cut technology
            // panel on the first imported part with Slow (1) chosen and All the
            // same contours on; --paint-contours also paints, through the same
            // engine function as contour-tech, its bend lines Mark, its first
            // hole Leave out and every hole equal to its second Slow (1) (CS6,
            // CS7). Without --part the sheet shows the result. --paint-card=<row>
            // (cut, slow, mark, no-cut, bend-line, leave-out) shows a row's hover card.
            let paint_panel = std::env::args().any(|a| a == "--paint-panel" || a == "--paint-contours");
            if let Some(i) = (0..app.parts.len()).find(|&i| app.parts[i].shape == Shape::Custom).filter(|_| paint_panel) {
                use crate::cutpanel::Row;
                let slow = Row::Cut(nest::CutQuality::new(1));
                if std::env::args().any(|a| a == "--paint-contours") {
                    let list = engine::contours::list(&app.parts[i]);
                    let bends: Vec<usize> = (0..list.len()).filter(|&k| list[k].state == dxf::ContourState::BendLine).collect();
                    app.paint_contours(i, &bends, Row::Mark, false);
                    app.paint_contours(i, &[0], Row::LeaveOut, false);
                    app.paint_contours(i, &[1], slow, true);
                }
                app.cut_panel.open = true;
                app.cut_panel.same = true;
                app.cut_panel.row = Some(slow);
                app.cut_panel.card = std::env::args().find_map(|a| a.strip_prefix("--paint-card=").map(str::to_string)).and_then(|r| match r.as_str() {
                    "cut" => Some(Row::Cut(None)),
                    "slow" => Some(slow),
                    "mark" => Some(Row::Mark),
                    "no-cut" => Some(Row::NoCut),
                    "bend-line" => Some(Row::BendLine),
                    "leave-out" => Some(Row::LeaveOut),
                    _ => None,
                });
                if app.workspace == Workspace::Part {
                    app.tab = Tab::Technology;
                }
            }
            // --sheet-tab (screenshots): the Sheet tab, e.g. its machine-table line.
            if std::env::args().any(|a| a == "--sheet-tab") {
                app.tab = Tab::Sheet;
            }
            if want_jobs {
                app.workspace = Workspace::Jobs;
            }
            // --export-review / --part-check (screenshots): the Export CNC
            // review, or Verify machining's result on the opened part.
            if std::env::args().any(|a| a == "--export-review") {
                app.export_checked();
            }
            if std::env::args().any(|a| a == "--part-check") {
                app.verify_machining_now();
            }
            // --import-card (screenshots): the card after importing the last
            // two parts' drawings. --open-job-menu: the Open job list, with a
            // job file and a Library job saved under the (temporary) home.
            // --import=<drawing>: import it as if dropped on the window (the
            // Import drawings window opens on a drawing with several rows, IM12).
            if let Some(p) = std::env::args().find_map(|a| a.strip_prefix("--import=").map(PathBuf::from)).filter(|p| p.is_file()) {
                app.import_drawings(vec![p]);
            }
            if std::env::args().any(|a| a == "--import-card") {
                // Two new drawings (made-up references), not yet nested.
                for (k, name) in ["DEMO-101", "DEMO-102"].into_iter().enumerate() {
                    if let Some(mut p) = app.parts.get(k * 3).cloned() {
                        p.name = name.into();
                        p.qty = [6, 4][k];
                        p.color = app.color_seq_next();
                        app.parts.push(p);
                    }
                }
                app.finish_import(2.min(app.parts.len()), vec![]);
            }
            if std::env::args().any(|a| a == "--open-job-menu") {
                let dir = library::documents_dir();
                let _ = std::fs::create_dir_all(&dir);
                app.save_job_to(&dir.join("DEMO-002 base plates.snjob"));
                app.save_job_to_library();
                app.demo_open_menu = true;
            }
            // --tech-result (screenshots): Auto lead-ins run on one part type,
            // with its result card and "Apply to all".
            if std::env::args().any(|a| a == "--tech-result") {
                app.tab = Tab::Technology;
                app.sel_part = Some(1.min(app.parts.len().saturating_sub(1)));
                app.tech_all = false;
                app.tech_nest_all = Some(false);
                app.run_tech(techui::TechRerun::AutoLeads);
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = &res {
        eprintln!("window error: {e}");
    }
    res
}

/// A short "what just happened" card on the drawing, with the next steps.
#[derive(Clone, Debug)]
pub struct ResultCard {
    pub title: String,
    pub lines: Vec<String>,
    /// Something went wrong or needs a look (orange, not green).
    pub warn: bool,
    /// Buttons: the first is the main one.
    pub actions: Vec<(String, CardAct)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CardAct {
    /// Run the technology command again on every part type.
    TechAll(techui::TechRerun),
    NestAll,
    /// Pick this part and bring its tile into view in Parts View.
    ShowPart(usize),
    /// The Sheet tab: material, thickness, sheet.
    SheetTab,
}

impl CardAct {
    fn tip(self) -> &'static str {
        match self {
            CardAct::TechAll(_) => "Run the same command on every part type in the job (Ctrl + Z undoes it)",
            CardAct::NestAll => "Nest all the job's parts (F5)",
            CardAct::ShowPart(_) => "Pick the first new part and show its tile in Parts View",
            CardAct::SheetTab => "Open the Sheet tab: material, thickness and sheet size",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_clashes(a: &App) {
        for s in &a.result.sheets {
            for (i, p) in s.iter().enumerate() {
                assert!(a.spec.fits(p), "part leaves sheet");
                for q in &s[i + 1..] {
                    assert!(!nest::clash_exact(&a.parts, p, q, a.spec.gap - 1e-6), "parts overlap");
                }
            }
        }
    }

    #[test]
    fn copy_compact_undo_keep_layout_valid() {
        let mut a = App::new(None);
        assert!(a.undo.is_empty());
        // A half-empty sheet, so the copy surely has room.
        a.parts = vec![
            Part { name: "A".into(), shape: Shape::Rectangle, w: 300.0, h: 200.0, qty: 3, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
            Part { name: "B".into(), shape: Shape::Disc, w: 150.0, h: 150.0, qty: 4, rotate: true, color: 1, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] },
        ];
        a.run_nesting();
        a.compact();
        no_clashes(&a);
        let placed = a.total_placed();
        a.cur_sheet = 0;
        a.sel_placed = Some(0);
        a.duplicate_selected();
        no_clashes(&a);
        assert_eq!(a.total_placed(), placed + 1, "the copy found a free spot");
        a.undo();
        assert_eq!(a.total_placed(), placed);
        a.redo();
        assert_eq!(a.total_placed(), placed + 1);
        no_clashes(&a);
    }

    #[test]
    fn compact_undo_and_delete_leave_nothing_pointing_at_the_wrong_part() {
        let mut a = App::new(None);
        a.run_nesting();
        a.cur_sheet = 0;
        // Compact on a remnant would slide parts off its outline: refused.
        let before = a.result.sheets[0].clone();
        let (w, h) = (a.spec.w, a.spec.h);
        a.result.formats[0].outline = Some(crate::stock::Outline { outer: vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], holes: vec![] });
        a.compact();
        assert!(a.status.contains("rectangular") && a.result.sheets[0] == before, "{}", a.status);
        a.result.formats[0].outline = None;
        // Undo with a just-added part still on the pointer: the pointer lets go.
        a.checkpoint();
        a.parts.push(a.parts[0].clone());
        a.start_ghost(a.parts.len() - 1, false);
        a.undo();
        assert!(a.ghost.is_none());
        // Taking a part off the sheet renumbers the rest: its hand picks go.
        a.manual_cuts = vec![vec![cnc::ManualCut { placed: 1, contour: 0, start: [0.0, 0.0], reverse: false }]];
        a.set_selection(vec![0]);
        a.remove_selected();
        assert!(a.manual_cuts.iter().all(|m| m.is_empty()));
    }

    #[test]
    fn a_selection_moves_turns_copies_and_goes_by_hand() {
        // HM1–HM10 as the keys and buttons run them.
        let mut a = App::new(None);
        let r = |name: &str, color| Part { name: name.into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 3, rotate: true, color, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![r("A", 0), r("B", 1)];
        a.run_nesting();
        assert_eq!(a.total_placed(), 6);
        a.cur_sheet = 0;
        a.set_selection(vec![0, 1]);
        assert_eq!(a.selection(), vec![0, 1]);
        let undo0 = a.undo.len();
        // Ctrl + Up: they sit under a third part, so they stay; Ctrl + Right:
        // to the right margin, together.
        let before: Vec<Placed> = a.result.sheets[0].clone();
        a.nudge(engine::arrange::Dir::Up, true, 10.0);
        assert_eq!(a.result.sheets[0], before, "{}", a.status);
        assert!(a.status.contains("they are at the part gap"), "{}", a.status);
        a.nudge(engine::arrange::Dir::Right, true, 10.0);
        let after = &a.result.sheets[0];
        let dx = after[0].x - before[0].x;
        assert!((after[0].x + after[0].w - (a.spec.w - a.spec.margin)).abs() < 1e-6, "{:?}", after[0]);
        assert!(dx > 0.0 && (after[1].x - before[1].x - dx).abs() < 1e-9, "moved together");
        no_clashes(&a);
        // Alt + Left twice in a row: one undo step, 2 × the step.
        let x = a.result.sheets[0][0].x;
        a.nudge(engine::arrange::Dir::Left, false, 20.0);
        a.nudge(engine::arrange::Dir::Left, false, 20.5);
        assert!((x - a.result.sheets[0][0].x - 2.0 * a.cfg.nesting.step).abs() < 1e-6);
        assert_eq!(a.undo.len(), undo0 + 2, "Ctrl+Right, then the Alt+Left run");
        // Turn, copy and remove the pair.
        a.turn_selected(1);
        assert!(a.selection().iter().all(|&i| a.result.sheets[0][i].turns == 1));
        no_clashes(&a);
        a.duplicate_selected();
        assert_eq!(a.total_placed(), 8);
        assert_eq!(a.selection(), vec![6, 7], "the copies are selected");
        no_clashes(&a);
        a.remove_selected();
        assert_eq!(a.total_placed(), 6);
        assert!(a.selection().is_empty());
        a.select_all_on_sheet();
        assert_eq!(a.selection().len(), 6);
    }

    #[test]
    fn alt_arrows_force_and_ctrl_arrows_always_keep_the_gap() {
        // HM2, HM3: the step move doesn't test for parts; the slide always does,
        // even with Collision off.
        let mut a = App::new(None);
        let r = Part { name: "A".into(), shape: Shape::Rectangle, w: 100.0, h: 100.0, qty: 2, rotate: true, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        a.parts = vec![r];
        a.spec.gap = 6.35;
        a.result = NestResult { sheets: vec![vec![Placed { part: 0, x: 100.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false }, Placed { part: 0, x: 210.0, y: 100.0, w: 100.0, h: 100.0, turns: 0, mirror: false }]], formats: vec![nest::SheetFormat::rect(2000.0, 1000.0)], method: String::new(), tried: 0 };
        a.cur_sheet = 0;
        a.collision = false;
        a.set_selection(vec![1]);
        a.nudge(engine::arrange::Dir::Left, true, 1.0);
        assert!((a.result.sheets[0][1].x - 206.35).abs() < 1e-6, "Ctrl+Left stops at the gap: {:?}", a.result.sheets[0][1]);
        a.collision = true;
        a.cfg.nesting.step = 5.0;
        a.nudge(engine::arrange::Dir::Left, false, 2.0);
        assert!((a.result.sheets[0][1].x - 201.35).abs() < 1e-6, "Alt+Left moves the whole step");
        assert!(a.status.starts_with('⚠') && a.status.contains("#1"), "{}", a.status);
        a.nudge(engine::arrange::Dir::Right, true, 3.0);
        assert!(a.result.sheets[0][1].x > 1500.0, "Ctrl+Right to the margin");
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;

    #[test]
    fn switching_jobs_keeps_each_jobs_products() {
        let mut a = App::new(None);
        assert!(a.jobs.len() >= 3);
        let first = a.parts.len();
        a.parts[0].qty = 7;
        a.open_job(1);
        assert_eq!(a.parts.len(), a.jobs[1].parts.len());
        assert!(!a.result.sheets.is_empty());
        a.open_job(0);
        assert_eq!(a.parts.len(), first);
        assert_eq!(a.parts[0].qty, 7);
        a.new_job_in_week("W12".into());
        assert!(a.parts.is_empty());
        assert_eq!(a.cur_job, a.jobs.len() - 1);
    }
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    #[test]
    fn duplicate_job_names_follow_the_original_rule() {
        let mut a = App::new(None);
        let base = a.job_name.clone();
        a.copy_job();
        assert_eq!(a.job_name, format!("{base} (1)"));
        a.open_job(0);
        a.copy_job();
        assert_eq!(a.job_name, format!("{base} (2)"));
        // Fill (3)..(9); the tenth copy must be refused.
        for _ in 3..=9 {
            a.open_job(0);
            a.copy_job();
        }
        let before = a.jobs.len();
        a.open_job(0);
        a.copy_job();
        assert_eq!(a.jobs.len(), before);
        assert!(a.status.starts_with("Couldn't copy"));
    }

    #[test]
    fn identical_sheets_are_folded() {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "P".into(), shape: Shape::Rectangle, w: 900.0, h: 900.0, qty: 6, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        let groups = a.sheet_groups();
        let total: usize = groups.iter().map(|g| g.1).sum();
        assert_eq!(total, a.result.sheets.len());
        assert!(groups.len() < a.result.sheets.len());
    }
}

#[cfg(test)]
mod tech_tests {
    use super::*;

    fn tech_part_index(a: &App) -> usize {
        a.parts.iter().position(|p| p.tech.is_some()).expect("a sample part has technology")
    }

    #[test]
    fn micro_joints_reach_the_program_and_cut_length() {
        let a = App::new(None);
        assert!(a.program.contains("MICRO-JOINT"));
        let mut b = App::new(None);
        for p in &mut b.parts {
            p.tech = None;
        }
        b.refresh();
        assert!(!b.program.contains("MICRO-JOINT"));
    }

    #[test]
    fn editing_a_tech_part_copy_keeps_the_original() {
        let mut a = App::new(None);
        let i = tech_part_index(&a);
        let n = a.parts.len();
        let original = a.parts[i].clone();
        let mut draft = original.clone();
        draft.w += 20.0;
        a.apply_tech_choice(i, draft, TechChoice::EditCopy);
        assert_eq!(a.parts.len(), n + 1);
        assert_eq!(a.parts[i].w, original.w);
        assert!(a.parts[i].tech.is_some());
        assert!(a.parts[i + 1].tech.is_none());
        assert_eq!(a.parts[i + 1].w, original.w + 20.0);
        // Placed parts still point at the same products.
        for s in &a.result.sheets {
            for p in s {
                assert!(p.part < a.parts.len());
            }
        }
    }

    #[test]
    fn editing_a_tech_part_itself_removes_its_technology() {
        let mut a = App::new(None);
        let i = tech_part_index(&a);
        let mut draft = a.parts[i].clone();
        draft.h += 10.0;
        a.apply_tech_choice(i, draft, TechChoice::EditPart);
        assert!(a.parts[i].tech.is_none());
    }

    #[test]
    fn material_change_can_remove_or_keep_nestings() {
        let mut a = App::new(None);
        let mut s = a.spec.clone();
        s.thickness = 5.0;
        a.apply_spec_change(s.clone(), false);
        assert!(!a.result.sheets.is_empty() && a.stale);
        a.apply_spec_change(s, true);
        assert!(a.result.sheets.is_empty());
    }
}

#[cfg(test)]
mod part_tech_tests {
    use super::*;

    fn square_app(tech: nest::PartTech) -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "S".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: Some(tech), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        a
    }

    #[test]
    fn automatic_lead_starts_at_the_corner_nearest_the_position() {
        // Position 9 = top right of the contour box; the machine's table puts a
        // corner lead (type 10) on rectangular parts.
        let a = square_app(nest::PartTech { corner_start: true, position: 9, lead_type: nest::LeadType::Machine, ..Default::default() });
        let c = &a.plans[0].contours[0];
        let pl = a.result.sheets[0][0];
        let top_right = [pl.x + pl.w, pl.y + pl.h];
        assert!((c.points[0][0] - top_right[0]).abs() < 1e-6 && (c.points[0][1] - top_right[1]).abs() < 1e-6);
        // Corrected (AL7): the lead extends the first side
        // (the top, cut right to left) backwards past the corner, not the bisector.
        let d = [c.lead_in[0] - c.points[0][0], c.lead_in[1] - c.points[0][1]];
        assert!(d[0] > 0.0 && d[1].abs() < 1e-6, "{d:?}");
        // Other lead types go in the middle of the side nearest the position.
        let a = square_app(nest::PartTech { corner_start: true, position: 9, ..Default::default() });
        let c = &a.plans[0].contours[0];
        assert!((c.points[0][0] - top_right[0]).abs() < 1e-6 && (c.points[0][1] - (pl.y + pl.h / 2.0)).abs() < 1e-6, "{:?}", c.points[0]);
    }

    #[test]
    fn arc_lead_ends_on_the_contour_and_round_holes_lead_from_the_zone_angle() {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "F".into(), shape: Shape::Flange, w: 200.0, h: 200.0, qty: 1, rotate: false, color: 0, tech: Some(nest::PartTech { lead_type: nest::LeadType::Arc, corner_start: true, ..Default::default() }), custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        let plan = &a.plans[0];
        // Corrected: a round hole's lead starts on the circle at the position's
        // angle (225° for position 1), kept inside the hole and short of its
        // centre (AL4, AL10), not always pierced at the centre.
        let hole = plan.contours.iter().find(|c| c.inner).unwrap();
        let cc = nest::centroid(&hole.points);
        let r = hole.points.iter().map(|p| ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt()).fold(0.0, f64::max);
        let s = hole.points[0];
        let ang = (s[1] - cc[1]).atan2(s[0] - cc[0]).to_degrees().rem_euclid(360.0);
        assert!((ang - 225.0).abs() < 1.0, "{ang}");
        assert!(hole.lead_path.len() > 2, "an arc lead");
        assert!(hole.lead_path.iter().all(|p| ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt() <= r + 1e-6));
        let outer = plan.contours.iter().find(|c| !c.inner).unwrap();
        let end = *outer.lead_path.last().unwrap();
        assert!((end[0] - outer.points[0][0]).abs() < 1e-9 && (end[1] - outer.points[0][1]).abs() < 1e-9);
    }

    #[test]
    fn loops_go_on_sharp_corners_and_reach_the_program() {
        let a = square_app(nest::PartTech { loops: true, ..Default::default() });
        let outer = &a.plans[0].contours[0];
        assert_eq!(outer.loops.len(), 3); // four 90° corners; the start corner has the lead
        assert!(a.program.contains("( LOOP )"));
    }

    #[test]
    fn joints_by_size_class_follow_the_rule() {
        // Corrected (P7, AM2): each size class gives the fewest
        // and most joints (not counts along X and Y); −1 = the general F121/F122.
        let cfg = nest::AutoJoints { small: [50.0, 50.0], large: [300.0, 300.0], parts: [[0, 0], [-1, -1], [4, 6]], holes: [[1, 2], [-1, -1], [-1, -1]], min: 0, max: 99999 };
        assert_eq!(cfg.limits(40.0, 40.0, false), (0, 0));
        assert_eq!(cfg.limits(40.0, 40.0, true), (1, 2));
        assert_eq!(cfg.limits(200.0, 100.0, false), (0, 99999));
        // 160 mm small part: none allowed. 600 mm medium: one per 100 → 6.
        assert_eq!(cfg.count_for(160.0, 100.0, 40.0, 40.0, false), 0);
        assert_eq!(cfg.count_for(600.0, 100.0, 200.0, 100.0, false), 6);
        // 1500 mm large part: 15 → clamped to 6. Small hole 120 mm: 2 → at most 2.
        assert_eq!(cfg.count_for(1500.0, 100.0, 400.0, 350.0, false), 6);
        assert_eq!(cfg.count_for(120.0, 100.0, 40.0, 40.0, true), 2);
        assert_eq!(nest::AutoJoints::default().count_for(400.0, 100.0, 150.0, 50.0, false), 4);
    }
}

#[cfg(test)]
mod part_view_tests {
    use super::*;

    #[test]
    fn opening_a_part_builds_the_set_and_steps_through_it() {
        let mut a = App::new(None);
        let n = a.parts.len();
        a.open_scope = OpenScope::All;
        a.open_part(2);
        assert!(a.workspace == Workspace::Part);
        assert!(a.tab == Tab::Home, "the Drawing module opens on its Home tab");
        assert_eq!(a.open_set.len(), n);
        assert_eq!(a.shown_part(), Some(2));
        a.open_part_step(1);
        assert_eq!(a.sel_part, Some(3));
        a.open_part_step(-2);
        assert_eq!(a.sel_part, Some(1));
        a.open_scope = OpenScope::Selected;
        a.open_part(4);
        assert_eq!(a.open_set, vec![4]);
    }
}

#[cfg(test)]
mod element_tests {
    use super::*;

    fn rect_app() -> App {
        let mut a = App::new(None);
        a.parts = vec![Part { name: "R".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] }];
        a.run_nesting();
        a.open_scope = OpenScope::Selected;
        a.open_part(0);
        a
    }

    fn pick(a: &mut App, which: &[usize]) {
        let el = nest::elements_of(&a.parts[0].geometry());
        a.sel_elem_info = which.iter().map(|&i| (el[i].contour, el[i].kind, el[i].length, el[i].midpoint())).collect();
        a.sel_elems = which.to_vec();
    }

    #[test]
    fn a_rectangle_has_four_lines_and_a_disc_one_circle() {
        let a = rect_app();
        let el = nest::elements_of(&a.parts[0].geometry());
        assert_eq!(el.len(), 4);
        assert!(el.iter().all(|e| e.kind == nest::ElemKind::Line));
        assert!((el[0].length - 200.0).abs() < 1e-9 && (el[1].length - 100.0).abs() < 1e-9);
        assert_eq!(el[0].midpoint(), [100.0, 0.0]);
        let disc = Part { name: "D".into(), shape: Shape::Disc, w: 100.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![], group: String::new(), erp: vec![] };
        let de = nest::elements_of(&disc.geometry());
        assert!(de.iter().any(|e| e.kind == nest::ElemKind::Circle));
    }

    #[test]
    fn capture_picks_the_nearest_and_breaks_ties_by_the_nearer_start() {
        let a = rect_app();
        let el = nest::elements_of(&a.parts[0].geometry());
        assert_eq!(partview::capture(&el, [60.0, 1.0], 2.0), Some(0));
        assert_eq!(partview::capture(&el, [60.0, 50.0], 2.0), None);
        // On the corner (200, 0) both the bottom and right lines are at distance 0:
        // the right line starts there, so it wins.
        assert_eq!(partview::capture(&el, [200.0, 0.0], 2.0), Some(1));
    }

    #[test]
    fn lead_on_element_starts_the_cut_in_the_middle_of_the_line() {
        let mut a = rect_app();
        pick(&mut a, &[0]);
        a.lead_on_element();
        let t = a.parts[0].tech.unwrap();
        assert_eq!(t.starts[0], Some((0, [100.0, 0.0])));
        let pl = a.result.sheets[0][0];
        let c = a.plans[0].contours.iter().find(|c| !c.inner).unwrap();
        assert!((c.points[0][0] - (pl.x + 100.0)).abs() < 1e-6 && (c.points[0][1] - pl.y).abs() < 1e-6, "{:?} vs {:?}", c.points[0], (pl.x, pl.y));
        // Picking again on the same contour replaces the start.
        pick(&mut a, &[1]);
        a.lead_on_element();
        let t = a.parts[0].tech.unwrap();
        assert_eq!(t.starts.iter().flatten().count(), 1);
        assert_eq!(t.starts[0], Some((0, [200.0, 50.0])));
    }

    #[test]
    fn a_lead_goes_by_the_third_of_the_line_that_was_clicked() {
        // LP3: first third → 2·tol + overlap from the start,
        // last third → length − 2·tol − overlap, middle → (length − overlap) / 2.
        let mut a = rect_app();
        a.parts[0].tech = Some(nest::PartTech { overlap: 0.5, ..Default::default() });
        pick(&mut a, &[0]);
        a.sel_pick = Some((0, [20.0, 3.0]));
        a.lead_on_element();
        let s = a.parts[0].tech.unwrap().starts[0].unwrap().1;
        assert!((s[0] - 0.52).abs() < 1e-9 && s[1].abs() < 1e-9, "{s:?}");
        a.sel_pick = Some((0, [190.0, 0.0]));
        a.lead_on_element();
        let s = a.parts[0].tech.unwrap().starts[0].unwrap().1;
        assert!((s[0] - (200.0 - 0.02 - 0.5)).abs() < 1e-9, "{s:?}");
        a.sel_pick = Some((0, [100.0, 0.0]));
        a.lead_on_element();
        assert!((a.parts[0].tech.unwrap().starts[0].unwrap().1[0] - 99.75).abs() < 1e-9);
        // A click on another element (stale after a box selection) is ignored:
        // the lead goes in the element's middle (E4).
        a.sel_pick = Some((2, [20.0, 3.0]));
        a.lead_on_element();
        assert!((a.parts[0].tech.unwrap().starts[0].unwrap().1[0] - 100.0).abs() < 1e-9);
        // Too short: not longer than 4 × 0.01 + overlap.
        let e = nest::Element { contour: 0, kind: nest::ElemKind::Line, pts: vec![[0.0, 0.0], [0.5, 0.0]], length: 0.5 };
        assert_eq!(e.lead_point(Some([0.1, 0.0]), 0.46, nest::LEAD_LINEAR_TOL), None);
        assert!(e.lead_point(Some([0.1, 0.0]), 0.4, nest::LEAD_LINEAR_TOL).is_some());
    }

    #[test]
    fn short_elements_are_refused_and_joints_go_on_each_selected_element() {
        let mut a = rect_app();
        a.parts[0].tech = Some(nest::PartTech { lead_in: 150.0, ..Default::default() });
        pick(&mut a, &[1]); // 100 mm line, 150 mm lead
        a.lead_on_element();
        assert!(a.parts[0].tech.unwrap().starts.iter().all(|s| s.is_none()));
        pick(&mut a, &[0, 2]);
        a.joint_on_elements();
        assert_eq!(a.parts[0].tech.unwrap().joint_picks.iter().flatten().count(), 2);
        let c = a.plans[0].contours.iter().find(|c| !c.inner).unwrap();
        assert_eq!(c.joints.len(), 2);
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    #[test]
    fn marking_texts_reach_the_plan_and_program_but_notes_do_not() {
        let mut a = App::new(None);
        let mk = |kind| nest::PartText { text: "AB1".into(), at: [10.0, 10.0], height: 10.0, angle: 0.0, curve: 0.0, kind };
        a.parts = vec![Part { name: "P".into(), shape: Shape::Rectangle, w: 200.0, h: 100.0, qty: 1, rotate: false, color: 0, tech: None, custom: None, rots: Default::default(), texts: vec![mk(nest::TextKind::Note)], group: String::new(), erp: vec![] }];
        a.run_nesting();
        assert!(a.plans[0].marks.is_empty(), "notes are not machined");
        a.parts[0].texts = vec![mk(nest::TextKind::Mark)];
        a.refresh();
        assert!(!a.plans[0].marks.is_empty());
        assert!(a.plans[0].mark_len > 0.0);
        assert!(a.program.contains("M10"));
        // Text added from the ribbon lands inside the part.
        a.sel_part = Some(0);
        a.add_text_to_targets("#REF", 8.0, nest::TextKind::Special);
        let t = a.parts[0].texts.last().unwrap();
        assert!(t.at[0] >= 0.0 && t.at[0] + font::width(&t.text, t.height) <= 200.0);
    }

    #[test]
    fn dxf_text_on_a_mark_layer_becomes_a_marking_text() {
        let src = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\nCUT\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n50\n10\n0\n20\n50\n0\nTEXT\n8\nMARK\n10\n10\n20\n10\n40\n6\n1\nPN-7\n0\nTEXT\n8\nCUT\n10\n10\n20\n30\n40\n4\n1\nnote\n0\nENDSEC\n0\nEOF\n";
        let d = dxf::read(src).unwrap();
        let mut roles = std::collections::BTreeMap::new();
        roles.insert("CUT".to_string(), dxf::LayerRole::Cut);
        roles.insert("MARK".to_string(), dxf::LayerRole::Mark);
        let (g, _) = dxf::build(&d, &roles).unwrap();
        assert_eq!(g.texts.len(), 2);
        let mark = g.texts.iter().find(|t| t.text == "PN-7").unwrap();
        assert_eq!(mark.kind, nest::TextKind::Mark);
        assert_eq!(mark.height, 6.0);
        assert_eq!(g.texts.iter().find(|t| t.text == "note").unwrap().kind, nest::TextKind::Note);
    }
}
