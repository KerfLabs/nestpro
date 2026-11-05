//! Dockable side panes, following the original's docking-pane manager
//! (rule D1; see MECHANICS.md "Side panes"):
//! - every title bar has the pane **menu button** ▾ (default pane option 16),
//!   a **pin** for Auto Hide and **×** to hide the pane;
//! - the ▾ menu holds Floating / Docking / Auto Hide / Hide (D2, D3);
//! - dragging a title moves the whole pane with its contents (show contents while
//!   dragging = 1); docking **stickers** appear (stickers = 1) and a translucent
//!   preview shows where it will dock (alpha docking context = 1);
//! - double-clicking a title floats or docks the pane;
//! - the last open pane can't be hidden (D5);
//! - Ctrl+Tab / Alt+F6 step through the panes (keyboard navigate = 15);
//! - the layout is saved as "ExpertLayout" and loaded at start (D6).

use crate::theme::th;
use crate::ui_parts::with_alpha;
use crate::{App, Workspace};
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Key, LayerId, Order, Pos2, Rect, Sense, Shape as EShape, Stroke, UiBuilder, Vec2};

/// Pane options of the original (the docking toolkit's option bits it tests).
pub const OPT_NO_CLOSEABLE: u32 = 1; // D5: the last open pane
pub const OPT_NO_FLOATABLE: u32 = 4; // D4: main work panes
pub const OPT_HAS_MENU_BUTTON: u32 = 16; // D1: default pane options 16
pub const OPT_NO_DOCKABLE: u32 = 32; // 0x20, D3 (menu "Floating")
/// Docking-manager switches the original sets up (D1).
pub const ALPHA_DOCKING_CONTEXT: bool = true;
pub const SHOW_DOCKING_STICKERS: bool = true;
pub const SHOW_CONTENTS_WHILE_DRAGGING: bool = true;
pub const KEYBOARD_NAVIGATE: u32 = 15;
/// Size of a pane the original creates on demand: 250 × 140 (D4).
pub const NEW_PANE_W: f32 = 250.0;
pub const NEW_PANE_H: f32 = 140.0;
/// Name the layout is saved under (D6).
pub const LAYOUT_NAME: &str = "ExpertLayout";
/// Pane actions after which the original posts its layout command 0x385
/// (D11: action <= 0x12 and bit set in 0x61501).
pub const LAYOUT_ACTIONS_MASK: u32 = 0x61501;
/// Action numbers of the rebuild's own menu actions (inferred from the toolkit's order:
/// floating 0, pinning 8, collapsing 10, expanding 12).
pub const ACTION_FLOATING: u32 = 0;
pub const ACTION_PINNING: u32 = 8;
pub const ACTION_COLLAPSING: u32 = 10;
pub const ACTION_EXPANDING: u32 = 12;

pub fn action_posts_layout(action: u32) -> bool {
    action <= 0x12 && (LAYOUT_ACTIONS_MASK >> action) & 1 == 1
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PaneId {
    NestTree,
    NestProps,
    PartList,
    PartProps,
    JobTree,
    JobSummary,
}

const ALL: [PaneId; 6] = [PaneId::NestTree, PaneId::NestProps, PaneId::PartList, PaneId::PartProps, PaneId::JobTree, PaneId::JobSummary];

impl PaneId {
    pub fn title(self) -> &'static str {
        match self {
            PaneId::NestTree => "Nesting Explorer",
            PaneId::NestProps => "Part Properties",
            PaneId::PartList => "Parts in this drawing",
            PaneId::PartProps => "Part Properties",
            PaneId::JobTree => "Job Explorer",
            PaneId::JobSummary => "Job Summary",
        }
    }

    pub fn for_workspace(ws: Workspace) -> &'static [PaneId] {
        match ws {
            Workspace::Nesting => &[PaneId::NestTree, PaneId::NestProps],
            Workspace::Part => &[PaneId::PartList, PaneId::PartProps],
            Workspace::Jobs => &[PaneId::JobTree, PaneId::JobSummary],
        }
    }

    fn default_height(self) -> f32 {
        match self {
            // Leaves Part Properties room for its main fields under it.
            PaneId::NestTree => 240.0,
            PaneId::PartList => 240.0,
            PaneId::JobTree => 330.0,
            _ => 260.0,
        }
    }

    fn key(self) -> &'static str {
        match self {
            PaneId::NestTree => "NestTree",
            PaneId::NestProps => "NestProps",
            PaneId::PartList => "PartList",
            PaneId::PartProps => "PartProps",
            PaneId::JobTree => "JobTree",
            PaneId::JobSummary => "JobSummary",
        }
    }

    fn from_key(s: &str) -> Option<PaneId> {
        ALL.into_iter().find(|p| p.key() == s)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneState {
    pub side: Side,
    pub open: bool,
    pub collapsed: bool,
    pub floating: bool,
    /// Auto Hide: the pane folds into a tab on its side and slides out on hover.
    pub auto_hide: bool,
    /// Original option 0x20: set by the menu's "Floating" — the pane won't dock.
    pub no_dock: bool,
    pub float_pos: Pos2,
    /// Body height when docked (the last expanded pane of a side fills the rest).
    pub height: f32,
}

/// Where a dragged pane would land.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drop {
    /// Dock on a side, before the `n`-th visible docked pane there (or at the end).
    Dock(Side, usize),
    Float(Pos2),
}

#[derive(Clone, Copy, Debug)]
pub struct Dragging {
    pub id: PaneId,
    /// Pointer offset from the pane's top-left corner when the drag began.
    pub grab: Vec2,
}

pub struct Dock {
    /// Global order; each side shows its panes in this order.
    pub order: Vec<PaneId>,
    states: Vec<(PaneId, PaneState)>,
    pub dragging: Option<Dragging>,
    /// The active pane (drawn with the active caption colour).
    pub active: Option<PaneId>,
    /// Auto-hidden pane slid out, and when the pointer was last over it.
    pub slide: Option<PaneId>,
    slide_seen: f64,
    /// Keep a pane that was slid out by a command open for a moment.
    slide_hold: bool,
    /// Layout changed since it was last saved.
    pub dirty: bool,
    /// Rects laid out this frame (for drop targets).
    pane_rects: Vec<(PaneId, Side, Rect)>,
    /// Panes already drawn this frame: a pane that changes place mid-frame is
    /// drawn in its new place from the next frame (never twice in one frame).
    drawn: Vec<PaneId>,
    side_rects: [Option<Rect>; 2],
    tab_rects: Vec<(PaneId, Rect)>,
    slide_rect: Option<Rect>,
    area: Rect,
}

pub const HEADER_H: f32 = 26.0;
const SPLIT_H: f32 = 6.0;
const MIN_BODY: f32 = 70.0;
const STICKER: f32 = 34.0;
const STRIP_W: f32 = 24.0;

impl Default for Dock {
    fn default() -> Self {
        Dock {
            order: ALL.to_vec(),
            states: ALL
                .iter()
                .map(|&id| {
                    (
                        id,
                        PaneState {
                            side: Side::Left,
                            open: true,
                            collapsed: false,
                            floating: false,
                            auto_hide: false,
                            no_dock: false,
                            float_pos: Pos2::new(320.0, 180.0),
                            height: id.default_height(),
                        },
                    )
                })
                .collect(),
            dragging: None,
            active: None,
            slide: None,
            slide_seen: 0.0,
            slide_hold: false,
            dirty: false,
            pane_rects: vec![],
            drawn: vec![],
            side_rects: [None, None],
            tab_rects: vec![],
            slide_rect: None,
            area: Rect::NOTHING,
        }
    }
}

impl Dock {
    pub fn get(&self, id: PaneId) -> PaneState {
        self.states.iter().find(|s| s.0 == id).map(|s| s.1).expect("every pane has a state")
    }

    pub fn get_mut(&mut self, id: PaneId) -> &mut PaneState {
        self.dirty = true;
        &mut self.states.iter_mut().find(|s| s.0 == id).expect("every pane has a state").1
    }

    fn in_ws(&self, ws: Workspace) -> impl Iterator<Item = PaneId> + '_ {
        let set = PaneId::for_workspace(ws);
        self.order.iter().copied().filter(move |id| set.contains(id))
    }

    /// Open docked (not auto-hidden) panes of a workspace on one side, in order.
    pub fn docked(&self, ws: Workspace, side: Side) -> Vec<PaneId> {
        self.in_ws(ws)
            .filter(|&id| {
                let s = self.get(id);
                s.open && !s.floating && !s.auto_hide && s.side == side
            })
            .collect()
    }

    pub fn auto_hidden(&self, ws: Workspace, side: Side) -> Vec<PaneId> {
        self.in_ws(ws)
            .filter(|&id| {
                let s = self.get(id);
                s.open && !s.floating && s.auto_hide && s.side == side
            })
            .collect()
    }

    pub fn floating(&self, ws: Workspace) -> Vec<PaneId> {
        self.in_ws(ws).filter(|&id| self.get(id).open && self.get(id).floating).collect()
    }

    pub fn open_panes(&self, ws: Workspace) -> Vec<PaneId> {
        self.in_ws(ws).filter(|&id| self.get(id).open).collect()
    }

    /// Option bits of a pane right now, as the original keeps them.
    pub fn options(&self, ws: Workspace, id: PaneId) -> u32 {
        let s = self.get(id);
        let mut o = OPT_HAS_MENU_BUTTON;
        if s.no_dock {
            o |= OPT_NO_DOCKABLE;
        }
        // The last open pane can't be closed (D5); with two open again
        // the flag is cleared.
        if s.open && self.open_panes(ws).len() <= 1 {
            o |= OPT_NO_CLOSEABLE;
        }
        o
    }

    /// Hide (close) a pane unless it's the last one open. Returns whether it closed.
    pub fn close(&mut self, ws: Workspace, id: PaneId) -> bool {
        if self.options(ws, id) & OPT_NO_CLOSEABLE != 0 {
            return false;
        }
        let s = self.get_mut(id);
        s.open = false;
        if self.slide == Some(id) {
            self.slide = None;
        }
        // The next open pane is shown and activated (D9): an auto-hidden one slides
        // out, a collapsed one opens.
        if let Some(next) = self.open_panes(ws).into_iter().find(|&p| p != id) {
            self.active = Some(next);
            let st = self.get(next);
            if st.auto_hide {
                self.slide = Some(next);
                self.slide_hold = true;
            } else if st.collapsed {
                self.get_mut(next).collapsed = false;
            }
        }
        true
    }

    /// Show a hidden pane again, at the size the original gives a pane it creates
    /// on demand (250 × 140, D4).
    pub fn reopen(&mut self, id: PaneId) {
        let s = self.get_mut(id);
        if !s.open {
            s.open = true;
            s.height = NEW_PANE_H;
        }
    }

    /// Menu "Floating" (D3): float it if it's docked and make it non-dockable.
    pub fn menu_floating(&mut self, id: PaneId) {
        let s = self.get(id);
        if s.no_dock || s.auto_hide {
            return;
        }
        let s = self.get_mut(id);
        s.floating = true;
        s.no_dock = true;
    }

    /// Menu "Docking": the pane may dock again (it stays where it is).
    pub fn menu_docking(&mut self, id: PaneId) {
        if self.get(id).no_dock {
            self.get_mut(id).no_dock = false;
        }
    }

    /// Menu "Auto Hide" and the pin button.
    pub fn toggle_auto_hide(&mut self, id: PaneId) {
        let s = self.get_mut(id);
        s.auto_hide = !s.auto_hide;
        if s.auto_hide {
            s.floating = false;
            s.no_dock = false;
        }
        if self.slide == Some(id) {
            self.slide = None;
        }
    }

    /// Double-click on the title: float a docked pane, dock a floating one.
    pub fn toggle_docking(&mut self, id: PaneId) {
        let s = self.get(id);
        if s.floating && s.no_dock {
            return;
        }
        let s = self.get_mut(id);
        s.floating = !s.floating;
        s.auto_hide = false;
    }

    /// Put the panes of a workspace back where they started.
    pub fn reset(&mut self, ws: Workspace) {
        let d = Dock::default();
        for &id in PaneId::for_workspace(ws) {
            *self.get_mut(id) = d.get(id);
        }
        let set = PaneId::for_workspace(ws);
        self.order.retain(|id| !set.contains(id));
        self.order.extend_from_slice(set);
        self.slide = None;
    }

    /// The docking stickers shown while a pane is dragged, with what each does.
    /// One at the middle of the left and right edges; and over the pane under the
    /// pointer an up and a down sticker (dock above / below that pane).
    pub fn stickers(&self, ws: Workspace, id: PaneId, p: Pos2) -> Vec<(Rect, Drop, &'static str)> {
        let a = self.area;
        let mut out = vec![];
        if !a.is_positive() {
            return out;
        }
        let left_n = self.docked(ws, Side::Left).into_iter().filter(|&x| x != id).count();
        let right_n = self.docked(ws, Side::Right).into_iter().filter(|&x| x != id).count();
        out.push((Rect::from_center_size(Pos2::new(a.left() + 30.0, a.center().y), Vec2::splat(STICKER)), Drop::Dock(Side::Left, left_n), "left"));
        out.push((Rect::from_center_size(Pos2::new(a.right() - 30.0, a.center().y), Vec2::splat(STICKER)), Drop::Dock(Side::Right, right_n), "right"));
        for side in [Side::Left, Side::Right] {
            let panes: Vec<&(PaneId, Side, Rect)> = self.pane_rects.iter().filter(|(pid, s, _)| *s == side && *pid != id).collect();
            for (k, (_, _, r)) in panes.iter().enumerate() {
                if r.contains(p) {
                    let c = r.center();
                    out.push((Rect::from_center_size(c - Vec2::new(0.0, STICKER * 0.6), Vec2::splat(STICKER)), Drop::Dock(side, k), "up"));
                    out.push((Rect::from_center_size(c + Vec2::new(0.0, STICKER * 0.6), Vec2::splat(STICKER)), Drop::Dock(side, k + 1), "down"));
                }
            }
        }
        out
    }

    /// Where a pane dropped at `p` lands: on a sticker it docks, elsewhere it floats.
    pub fn drop_target(&self, ws: Workspace, id: PaneId, p: Pos2, grab: Vec2) -> Drop {
        if !self.get(id).no_dock {
            // Pane stickers sit on top of the edge stickers.
            if let Some(s) = self.stickers(ws, id, p).into_iter().rev().find(|s| s.0.contains(p)) {
                return s.1;
            }
        }
        Drop::Float(p - grab)
    }

    /// Move a pane to a drop target.
    pub fn apply_drop(&mut self, ws: Workspace, id: PaneId, d: Drop) {
        match d {
            Drop::Float(p) => {
                let s = self.get_mut(id);
                s.floating = true;
                s.auto_hide = false;
                s.float_pos = p;
            }
            Drop::Dock(side, n) => {
                let list: Vec<PaneId> = self.docked(ws, side).into_iter().filter(|&x| x != id).collect();
                self.order.retain(|&x| x != id);
                let at = match list.get(n) {
                    Some(next) => self.order.iter().position(|x| x == next).unwrap_or(self.order.len()),
                    None => list.last().and_then(|last| self.order.iter().position(|x| x == last)).map_or(self.order.len(), |i| i + 1),
                };
                self.order.insert(at, id);
                let s = self.get_mut(id);
                s.side = side;
                s.floating = false;
                s.auto_hide = false;
                s.open = true;
            }
        }
    }

    /// The next open pane for Ctrl+Tab / Alt+F6 (backwards with Shift).
    pub fn next_pane(&self, ws: Workspace, back: bool) -> Option<PaneId> {
        let open = self.open_panes(ws);
        if open.is_empty() {
            return None;
        }
        let cur = self.active.and_then(|a| open.iter().position(|&x| x == a));
        let n = open.len();
        let i = match (cur, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        Some(open[i])
    }

    // ---- "ExpertLayout": save and load ----

    pub fn to_text(&self) -> String {
        let mut s = format!("{LAYOUT_NAME} 1\norder {}\n", self.order.iter().map(|p| p.key()).collect::<Vec<_>>().join(","));
        for &(id, st) in &self.states {
            s += &format!(
                "pane {} {} {} {} {} {} {} {:.1} {:.1} {:.1}\n",
                id.key(),
                if st.side == Side::Left { "L" } else { "R" },
                st.open as u8,
                st.collapsed as u8,
                st.floating as u8,
                st.auto_hide as u8,
                st.no_dock as u8,
                st.float_pos.x,
                st.float_pos.y,
                st.height
            );
        }
        s
    }

    pub fn from_text(text: &str) -> Option<Dock> {
        let mut d = Dock::default();
        let mut lines = text.lines();
        if !lines.next()?.starts_with(LAYOUT_NAME) {
            return None;
        }
        for line in lines {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.first() {
                Some(&"order") => {
                    let ord: Vec<PaneId> = f.get(1)?.split(',').filter_map(PaneId::from_key).collect();
                    if ord.len() == ALL.len() {
                        d.order = ord;
                    }
                }
                Some(&"pane") if f.len() == 11 => {
                    let id = PaneId::from_key(f[1])?;
                    let b = |s: &str| s == "1";
                    let st = d.get_mut(id);
                    st.side = if f[2] == "R" { Side::Right } else { Side::Left };
                    st.open = b(f[3]);
                    st.collapsed = b(f[4]);
                    st.floating = b(f[5]);
                    st.auto_hide = b(f[6]);
                    st.no_dock = b(f[7]);
                    st.float_pos = Pos2::new(f[8].parse().ok()?, f[9].parse().ok()?);
                    st.height = f[10].parse::<f32>().ok()?.max(MIN_BODY);
                }
                _ => {}
            }
        }
        // Every workspace keeps at least one pane open.
        for ws in [Workspace::Nesting, Workspace::Part, Workspace::Jobs] {
            if d.open_panes(ws).is_empty() {
                let first = PaneId::for_workspace(ws)[0];
                d.get_mut(first).open = true;
            }
        }
        d.dirty = false;
        Some(d)
    }

    fn layout_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
        Some(std::path::PathBuf::from(base).join("NestPro").join(format!("{LAYOUT_NAME}.txt")))
    }

    /// Load the saved layout (at start, like the original, D6).
    pub fn load() -> Option<Dock> {
        Dock::from_text(&std::fs::read_to_string(Dock::layout_path()?).ok()?)
    }

    /// Save the layout (D6).
    pub fn save(&mut self) {
        if let Some(p) = Dock::layout_path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, self.to_text());
        }
        self.dirty = false;
    }
}

/// What the user did on a pane's title bar.
#[derive(Default)]
struct HeaderActs {
    pin: bool,
    close: bool,
    toggle_docking: bool,
    activate: bool,
    drag_start: Option<Vec2>,
    menu: Option<MenuAct>,
}

#[derive(Clone, Copy)]
enum MenuAct {
    Floating,
    Docking,
    AutoHide,
    Hide,
    Collapse,
    Toggle(PaneId),
    Reset,
}

impl App {
    /// Lay out the side docks, auto-hide tabs and floating panes of the current
    /// workspace. Call it before the central panel.
    pub fn dock_panels(&mut self, ui: &mut egui::Ui) {
        let ws = self.workspace;
        self.dock.pane_rects.clear();
        self.dock.tab_rects.clear();
        self.dock.side_rects = [None, None];
        self.dock.area = ui.available_rect_before_wrap();
        self.dock_keys(ui.ctx());
        // Take the layout as it is now; changes made while drawing show next frame.
        self.dock.drawn.clear();
        let snap_auto = [self.dock.auto_hidden(ws, Side::Left), self.dock.auto_hidden(ws, Side::Right)];
        let snap_docked = [self.dock.docked(ws, Side::Left), self.dock.docked(ws, Side::Right)];
        let snap_float = self.dock.floating(ws);
        let snap_slide = self.dock.slide;
        // Auto-hide tab strips on the outer edges.
        for side in [Side::Left, Side::Right] {
            let ids = snap_auto[side as usize].clone();
            if ids.is_empty() {
                continue;
            }
            let panel = match side {
                Side::Left => egui::Panel::left(("autohide_left", ws as u8)),
                Side::Right => egui::Panel::right(("autohide_right", ws as u8)),
            };
            panel.resizable(false).exact_size(STRIP_W).frame(egui::Frame::new().fill(th().pane_header)).show(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let mut y = full.top() + 6.0;
                for id in ids {
                    let g = ui.painter().layout_no_wrap(id.title().to_string(), FontId::proportional(12.5), th().text);
                    let r = Rect::from_min_size(Pos2::new(full.left() + 2.0, y), Vec2::new(STRIP_W - 4.0, g.size().x + 16.0));
                    let resp = ui.interact(r, Id::unique(("autohide_tab", id)), Sense::click());
                    let on = self.dock.slide == Some(id) || resp.hovered();
                    ui.painter().rect_filled(r, CornerRadius::same(5), if on { th().accent_soft } else { th().panel });
                    ui.painter().rect_filled(
                        Rect::from_min_size(Pos2::new(if side == Side::Left { r.left() } else { r.right() - 3.0 }, r.top()), Vec2::new(3.0, r.height())),
                        CornerRadius::same(1),
                        if on { th().accent } else { th().border },
                    );
                    let pos = Pos2::new(r.center().x + g.size().y / 2.0, r.top() + 8.0);
                    ui.painter().add(egui::epaint::TextShape::new(pos, g, th().text).with_angle(std::f32::consts::FRAC_PI_2));
                    if resp.hovered() || resp.clicked() {
                        self.dock.slide = Some(id);
                        self.dock.slide_seen = ui.input(|i| i.time);
                        self.dock.active = Some(id);
                    }
                    self.dock.tab_rects.push((id, r));
                    y = r.bottom() + 6.0;
                }
                ui.allocate_rect(full, Sense::hover());
            });
        }
        for (k, side) in [Side::Left, Side::Right].into_iter().enumerate() {
            let ids = snap_docked[k].clone();
            if ids.is_empty() {
                continue;
            }
            let panel = match side {
                Side::Left => egui::Panel::left(("dock_left", ws as u8)),
                Side::Right => egui::Panel::right(("dock_right", ws as u8)),
            };
            panel.resizable(true).default_size(NEW_PANE_W).min_size(180.0).frame(egui::Frame::new().fill(th().panel)).show(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                self.dock.side_rects[k] = Some(full);
                self.dock_column(ui, side, &ids, full);
                ui.allocate_rect(full, Sense::hover());
            });
        }
        let ctx = ui.ctx().clone();
        self.slide_out(&ctx, snap_slide);
        for id in snap_float {
            self.floating_pane(&ctx, id);
        }
    }

    /// Ctrl+Tab / Alt+F6 step through the panes (Shift goes back).
    fn dock_keys(&mut self, ctx: &egui::Context) {
        if KEYBOARD_NAVIGATE == 0 || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (step, back) = ctx.input(|i| {
            let ctrl_tab = i.modifiers.ctrl && i.key_pressed(Key::Tab);
            let alt_f6 = i.modifiers.alt && i.key_pressed(Key::F6);
            (ctrl_tab || alt_f6, i.modifiers.shift)
        });
        if !step {
            return;
        }
        let ws = self.workspace;
        if let Some(id) = self.dock.next_pane(ws, back) {
            self.dock.active = Some(id);
            if self.dock.get(id).auto_hide {
                self.dock.slide = Some(id);
                self.dock.slide_seen = ctx.input(|i| i.time) + 2.0;
            }
            if self.dock.get(id).collapsed {
                self.dock.get_mut(id).collapsed = false;
            }
            self.status = format!("Pane: {}", id.title());
        }
    }

    fn dock_column(&mut self, ui: &mut egui::Ui, side: Side, ids: &[PaneId], full: Rect) {
        let n = ids.len();
        let expanded: Vec<usize> = (0..n).filter(|&k| !self.dock.get(ids[k]).collapsed).collect();
        let fill = expanded.last().copied();
        let fixed_total: f32 = expanded.iter().filter(|&&k| Some(k) != fill).map(|&k| self.dock.get(ids[k]).height).sum();
        let chrome = n as f32 * HEADER_H + n.saturating_sub(1) as f32 * SPLIT_H;
        let avail = (full.height() - chrome).max(0.0);
        let fill_min = if fill.is_some() { MIN_BODY } else { 0.0 };
        let scale = if fixed_total > 0.0 && fixed_total > avail - fill_min { ((avail - fill_min).max(0.0) / fixed_total).max(0.0) } else { 1.0 };
        let fill_h = (avail - fixed_total * scale).max(0.0);
        let mut y = full.top();
        for (k, &id) in ids.iter().enumerate() {
            self.dock.drawn.push(id);
            let st = self.dock.get(id);
            let body_h = if st.collapsed {
                0.0
            } else if Some(k) == fill {
                fill_h
            } else {
                st.height * scale
            };
            let hdr = Rect::from_min_size(Pos2::new(full.left(), y), Vec2::new(full.width(), HEADER_H));
            let pane = Rect::from_min_size(hdr.min, Vec2::new(full.width(), HEADER_H + body_h));
            self.dock.pane_rects.push((id, side, pane));
            let acts = self.pane_title_bar(ui, hdr, id);
            self.handle_header(ui.ctx(), id, hdr, pane.size(), acts);
            if body_h > 0.0 {
                let body = Rect::from_min_max(Pos2::new(full.left(), hdr.bottom()), Pos2::new(full.right(), hdr.bottom() + body_h));
                self.pane_body_in(ui, id, body);
            }
            if ui.input(|i| i.pointer.primary_pressed()) && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| pane.contains(p)) {
                self.dock.active = Some(id);
            }
            y = pane.bottom();
            if k + 1 < n {
                let split = Rect::from_min_size(Pos2::new(full.left(), y), Vec2::new(full.width(), SPLIT_H));
                let resizable = !st.collapsed && Some(k) != fill && fill.is_some_and(|f| f > k);
                let r = ui.interact(split, Id::unique(("pane_split", id)), if resizable { Sense::drag() } else { Sense::hover() });
                let hot = resizable && (r.hovered() || r.dragged());
                if resizable && r.dragged() {
                    let s = self.dock.get_mut(id);
                    s.height = (body_h + r.drag_delta().y).clamp(MIN_BODY, (avail - fill_min).max(MIN_BODY));
                }
                let cy = split.center().y;
                if resizable {
                    if hot {
                        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                    }
                    let c = if hot { th().accent } else { th().border };
                    ui.painter().line_segment([Pos2::new(split.left(), cy), Pos2::new(split.right(), cy)], Stroke::new(if hot { 2.0 } else { 1.0 }, c));
                    for dx in [-8.0, 0.0, 8.0] {
                        ui.painter().circle_filled(Pos2::new(split.center().x + dx, cy), 1.4, th().text_dim);
                    }
                } else {
                    ui.painter().line_segment([Pos2::new(split.left(), cy), Pos2::new(split.right(), cy)], Stroke::new(1.0, th().border));
                }
                y = split.bottom();
            }
        }
    }

    fn pane_body_in(&mut self, ui: &mut egui::Ui, id: PaneId, body: Rect) {
        let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt(("pane_body", id)));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        let out = egui::ScrollArea::vertical().id_salt(("pane_scroll", id)).auto_shrink([false, false]).show(&mut child, |ui| {
            ui.add_space(4.0);
            self.pane_body(ui, id);
            ui.add_space(6.0);
        });
        crate::ui_parts::fade_if_more(&child, &out);
    }

    /// An auto-hidden pane slid out over the work area from its tab.
    fn slide_out(&mut self, ctx: &egui::Context, slide: Option<PaneId>) {
        self.dock.slide_rect = None;
        let Some(id) = slide.filter(|_| self.dock.slide == slide) else { return };
        if self.dock.drawn.contains(&id) {
            return;
        }
        self.dock.drawn.push(id);
        let ws = self.workspace;
        let st = self.dock.get(id);
        if !st.open || !st.auto_hide || !PaneId::for_workspace(ws).contains(&id) {
            self.dock.slide = None;
            return;
        }
        let Some(&(_, tab)) = self.dock.tab_rects.iter().find(|t| t.0 == id) else { return };
        let a = self.dock.area;
        let w = NEW_PANE_W + 20.0;
        let x = if st.side == Side::Left { tab.right() + 6.0 } else { tab.left() - 6.0 - w };
        let rect = Rect::from_min_size(Pos2::new(x, a.top()), Vec2::new(w, a.height()));
        self.dock.slide_rect = Some(rect);
        egui::Area::new(Id::unique(("slide_out", id))).order(Order::Foreground).fixed_pos(rect.min).constrain(false).show(ctx, |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            ui.painter().add(EShape::rect_filled(rect.expand(1.0), CornerRadius::same(6), th().border));
            ui.painter().add(egui::epaint::RectShape::filled(rect, CornerRadius::same(6), th().panel).with_blur_width(0.0));
            let hdr = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEADER_H));
            let acts = self.pane_title_bar(ui, hdr, id);
            self.handle_header(ctx, id, hdr, rect.size(), acts);
            let body = Rect::from_min_max(Pos2::new(rect.left(), hdr.bottom()), rect.max);
            self.pane_body_in(ui, id, body);
            ui.allocate_rect(rect, Sense::hover());
        });
        // Slide back in shortly after the pointer leaves the pane and its tab.
        let now = ctx.input(|i| i.time);
        if self.dock.slide_hold {
            self.dock.slide_seen = now + 2.0;
            self.dock.slide_hold = false;
        }
        let over = ctx.pointer_latest_pos().is_some_and(|p| rect.contains(p) || tab.contains(p));
        if over || egui::Popup::is_any_open(ctx) || self.dock.dragging.is_some() {
            self.dock.slide_seen = self.dock.slide_seen.max(now);
        } else if now - self.dock.slide_seen > 0.6 {
            self.dock.slide = None;
        }
        ctx.request_repaint();
    }

    fn floating_pane(&mut self, ctx: &egui::Context, id: PaneId) {
        if self.dock.drawn.contains(&id) {
            return;
        }
        self.dock.drawn.push(id);
        let st = self.dock.get(id);
        let dragged = self.dock.dragging.is_some_and(|d| d.id == id);
        let frame = egui::Frame::new()
            .fill(if dragged && ALPHA_DOCKING_CONTEXT { with_alpha(th().panel, 235) } else { th().panel })
            .stroke(Stroke::new(1.0, if dragged { th().accent } else { th().border }))
            .corner_radius(CornerRadius::same(10))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(70) });
        egui::Window::new(id.title())
            .id(Id::unique(("pane_win", id)))
            .title_bar(false)
            .movable(false)
            .resizable(true)
            .current_pos(st.float_pos)
            .default_size(Vec2::new(NEW_PANE_W + 30.0, if st.collapsed { HEADER_H } else { 380.0 }))
            .min_size(Vec2::new(200.0, HEADER_H))
            .frame(frame)
            .show(ctx, |ui| {
                let (hdr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), HEADER_H), Sense::hover());
                let acts = self.pane_title_bar(ui, hdr, id);
                self.handle_header(ctx, id, hdr, Vec2::ZERO, acts);
                if !self.dock.get(id).collapsed {
                    let out = egui::ScrollArea::vertical().id_salt(("float_scroll", id)).auto_shrink([false, false]).show(ui, |ui| {
                        ui.add_space(4.0);
                        self.pane_body(ui, id);
                        ui.add_space(6.0);
                    });
                    crate::ui_parts::fade_if_more(ui, &out);
                }
            });
    }

    fn handle_header(&mut self, ctx: &egui::Context, id: PaneId, hdr: Rect, _size: Vec2, a: HeaderActs) {
        let ws = self.workspace;
        if a.activate {
            self.dock.active = Some(id);
        }
        if a.pin {
            self.dock.toggle_auto_hide(id);
            self.status = if self.dock.get(id).auto_hide {
                format!("'{}' set to Auto Hide — point at its tab on the edge to slide it out.", id.title())
            } else {
                format!("'{}' pinned back in place.", id.title())
            };
        }
        if a.close {
            self.hide_pane(id);
        }
        if a.toggle_docking {
            let was = self.dock.get(id);
            self.dock.toggle_docking(id);
            let now = self.dock.get(id);
            if now.floating && !was.floating {
                self.dock.get_mut(id).float_pos = hdr.min + Vec2::new(hdr.width() + 24.0, 0.0);
                self.status = format!("'{}' floats — double-click its title to dock it again.", id.title());
            } else if was.floating && !now.floating {
                self.status = format!("'{}' docked on the {}.", id.title(), if now.side == Side::Left { "left" } else { "right" });
            } else if was.no_dock {
                self.status = format!("'{}' is set to Floating — choose Docking in its ▾ menu first.", id.title());
            }
        }
        match a.menu {
            Some(MenuAct::Floating) => {
                if !self.dock.get(id).floating {
                    self.dock.get_mut(id).float_pos = hdr.min + Vec2::new(hdr.width() + 24.0, 0.0);
                }
                self.dock.menu_floating(id);
                if action_posts_layout(ACTION_FLOATING) {
                    self.dock.dirty = true;
                }
                self.status = format!("'{}' floats and won't dock until you choose Docking.", id.title());
            }
            Some(MenuAct::Docking) => {
                self.dock.menu_docking(id);
                self.status = format!("'{}' can dock again — drag it onto a sticker or double-click its title.", id.title());
            }
            Some(MenuAct::AutoHide) => {
                self.dock.toggle_auto_hide(id);
                if action_posts_layout(ACTION_PINNING) {
                    self.dock.dirty = true;
                }
            }
            Some(MenuAct::Hide) => self.hide_pane(id),
            Some(MenuAct::Collapse) => {
                let s = self.dock.get_mut(id);
                s.collapsed = !s.collapsed;
                let act = if s.collapsed { ACTION_COLLAPSING } else { ACTION_EXPANDING };
                if action_posts_layout(act) {
                    self.dock.dirty = true;
                }
            }
            Some(MenuAct::Toggle(p)) => {
                if self.dock.get(p).open {
                    self.hide_pane(p);
                } else {
                    self.dock.reopen(p);
                }
            }
            Some(MenuAct::Reset) => {
                self.dock.reset(ws);
                self.status = "Panels back to their usual places.".into();
            }
            None => {}
        }
        if let Some(grab) = a.drag_start {
            self.dock.dragging = Some(Dragging { id, grab });
            self.dock.active = Some(id);
            if self.dock.slide == Some(id) {
                self.dock.slide = None;
            }
            // Show contents while dragging: the pane itself follows the pointer.
            if SHOW_CONTENTS_WHILE_DRAGGING && !self.dock.get(id).floating {
                let p = ctx.pointer_latest_pos().unwrap_or(hdr.min);
                let s = self.dock.get_mut(id);
                s.floating = true;
                s.auto_hide = false;
                s.float_pos = p - grab;
            }
        }
    }

    fn hide_pane(&mut self, id: PaneId) {
        let ws = self.workspace;
        if self.dock.close(ws, id) {
            self.status = format!("'{}' hidden — View → Side panels (or right-click a pane title) shows it again.", id.title());
        } else {
            self.status = format!("'{}' is the last open panel, so it stays.", id.title());
        }
    }

    /// While a pane is dragged: move it, draw the stickers and the translucent
    /// preview, and dock or float it on release. Drawn above everything.
    pub fn dock_overlay(&mut self, ctx: &egui::Context) {
        let ws = self.workspace;
        let Some(d) = self.dock.dragging else {
            if self.dock.dirty && cfg!(not(test)) && !self.drag_test_on && !ctx.input(|i| i.pointer.any_down()) {
                self.dock.save();
            }
            return;
        };
        let Some(p) = ctx.pointer_latest_pos() else { return };
        if !ctx.input(|i| i.pointer.any_down()) {
            self.dock.dragging = None;
            let target = self.dock.drop_target(ws, d.id, p, d.grab);
            self.dock.apply_drop(ws, d.id, target);
            self.status = match target {
                Drop::Dock(Side::Left, _) => format!("'{}' docked on the left.", d.id.title()),
                Drop::Dock(Side::Right, _) => format!("'{}' docked on the right.", d.id.title()),
                Drop::Float(_) => format!("'{}' floats — drag it onto a sticker to dock it.", d.id.title()),
            };
            return;
        }
        // The pane follows the pointer.
        self.dock.get_mut(d.id).float_pos = p - d.grab;
        ctx.set_cursor_icon(CursorIcon::Grabbing);
        if self.dock.get(d.id).no_dock || !SHOW_DOCKING_STICKERS {
            return;
        }
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("dock_overlay")));
        let t = th();
        let target = self.dock.drop_target(ws, d.id, p, d.grab);
        // Translucent preview of where it will dock.
        if let (Drop::Dock(side, n), true) = (target, ALPHA_DOCKING_CONTEXT) {
            let a = self.dock.area;
            let k = side as usize;
            let others: Vec<Rect> = self.dock.pane_rects.iter().filter(|(pid, s, _)| *s == side && *pid != d.id).map(|x| x.2).collect();
            let r = match (self.dock.side_rects[k], others.is_empty()) {
                (Some(col), false) => {
                    if n < others.len() {
                        let o = others[n];
                        Rect::from_min_size(o.min, Vec2::new(col.width(), (o.height() / 2.0).max(60.0)))
                    } else {
                        let o = *others.last().unwrap();
                        Rect::from_min_max(Pos2::new(col.left(), o.center().y), o.max)
                    }
                }
                _ => match side {
                    Side::Left => Rect::from_min_size(a.min, Vec2::new(NEW_PANE_W, a.height())),
                    Side::Right => Rect::from_min_size(Pos2::new(a.right() - NEW_PANE_W, a.top()), Vec2::new(NEW_PANE_W, a.height())),
                },
            };
            painter.rect_filled(r, CornerRadius::same(6), with_alpha(t.accent, 60));
            painter.rect_stroke(r, CornerRadius::same(6), Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
        }
        // Stickers.
        for (r, drop, dir) in self.dock.stickers(ws, d.id, p) {
            let hot = r.contains(p) && target == drop;
            painter.rect_filled(r.expand(3.0), CornerRadius::same(8), with_alpha(Color32::BLACK, 50));
            painter.rect_filled(r, CornerRadius::same(7), if hot { t.accent } else { t.panel });
            painter.rect_stroke(r, CornerRadius::same(7), Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            let ink = if hot { Color32::WHITE } else { t.accent_fg };
            // A small pane with an arrow pointing where it docks.
            let c = r.center();
            let s = Stroke::new(2.0, ink);
            let (tip, wing) = match dir {
                "left" => (Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
                "right" => (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)),
                "up" => (Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0)),
                _ => (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)),
            };
            let bar = Rect::from_center_size(c + tip * 8.0, if wing.x == 0.0 { Vec2::new(4.0, 16.0) } else { Vec2::new(16.0, 4.0) });
            painter.rect_filled(bar, CornerRadius::same(1), ink);
            painter.line_segment([c - tip * 7.0, c + tip * 3.0], s);
            painter.line_segment([c + tip * 3.0, c + tip * -1.0 + wing * 4.0], s);
            painter.line_segment([c + tip * 3.0, c + tip * -1.0 - wing * 4.0], s);
        }
    }

    /// The "Side panels" list shown in the View tab.
    pub fn panels_menu(&mut self, ui: &mut egui::Ui) {
        let ws = self.workspace;
        for &id in PaneId::for_workspace(ws) {
            let mut on = self.dock.get(id).open;
            let last = self.dock.options(ws, id) & OPT_NO_CLOSEABLE != 0;
            if ui.add_enabled(!last, egui::Checkbox::new(&mut on, id.title())).changed() {
                if on {
                    self.dock.reopen(id);
                } else {
                    self.hide_pane(id);
                }
            }
        }
        ui.separator();
        if ui.button("Reset panel layout").clicked() {
            self.dock.reset(ws);
            ui.close();
        }
    }

    /// Draw a pane's title bar and read its buttons.
    fn pane_title_bar(&self, ui: &mut egui::Ui, hdr: Rect, id: PaneId) -> HeaderActs {
        let t = th();
        let ws = self.workspace;
        let st = self.dock.get(id);
        let opts = self.dock.options(ws, id);
        let mut acts = HeaderActs::default();
        let resp = ui.interact(hdr, Id::unique(("pane_hdr", id)), Sense::click_and_drag());
        let p = ui.painter().clone();
        let hovered = resp.hovered() || resp.dragged();
        let active = self.dock.active == Some(id);
        let bg = if active {
            crate::ui_parts::lerp_color(t.pane_header, t.accent_soft, 0.85)
        } else if hovered {
            crate::ui_parts::lerp_color(t.pane_header, t.accent_soft, 0.45)
        } else {
            t.pane_header
        };
        let cr = if st.floating || st.auto_hide { CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 } } else { CornerRadius::ZERO };
        p.rect_filled(hdr, cr, bg);
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], Stroke::new(1.0, if active { t.accent } else { t.border }));
        for (dx, dy) in [(0.0, -4.0), (0.0, 0.0), (0.0, 4.0), (4.0, -4.0), (4.0, 0.0), (4.0, 4.0)] {
            p.circle_filled(Pos2::new(hdr.left() + 8.0 + dx, hdr.center().y + dy), 1.1, if hovered { t.text } else { t.text_dim });
        }
        p.text(hdr.left_center() + Vec2::new(22.0, 0.0), Align2::LEFT_CENTER, id.title(), FontId::proportional(13.0), if active { t.accent_fg } else { t.text_strong });

        // Buttons right to left: × (hide), pin (auto hide; not on floating panes), ▾ menu.
        let y = hdr.center().y;
        let mut slot = 0.0;
        let mut button = |tag: &str, tip: &str, enabled: bool| -> (bool, Rect, bool) {
            let r = Rect::from_center_size(Pos2::new(hdr.right() - 13.0 - slot * 22.0, y), Vec2::splat(20.0));
            slot += 1.0;
            let b = ui.interact(r, Id::unique(("pane_btn", id, tag)), if enabled { Sense::click() } else { Sense::hover() }).on_hover_text(tip);
            if b.hovered() && enabled {
                p.rect_filled(r, CornerRadius::same(5), with_alpha(t.accent, 40));
            }
            (b.clicked() && enabled, r, b.hovered() && enabled)
        };
        let can_close = opts & OPT_NO_CLOSEABLE == 0;
        let (close, rc, hc) = button("close", if can_close { "Hide this panel" } else { "The last open panel can't be hidden" }, can_close);
        let col = |h: bool, en: bool| Stroke::new(1.4, if !en { with_alpha(t.text_dim, 90) } else if h { t.accent_fg } else { t.text_dim });
        let c = rc.center();
        p.line_segment([c + Vec2::new(-4.0, -4.0), c + Vec2::new(4.0, 4.0)], col(hc, can_close));
        p.line_segment([c + Vec2::new(4.0, -4.0), c + Vec2::new(-4.0, 4.0)], col(hc, can_close));
        let mut pin = false;
        if !st.floating {
            let (clicked, rp, hp) = button("pin", if st.auto_hide { "Dock (turn Auto Hide off)" } else { "Auto Hide" }, true);
            pin = clicked;
            let c = rp.center();
            let s = col(hp, true);
            if st.auto_hide {
                // Pin lying down.
                p.rect_stroke(Rect::from_center_size(c + Vec2::new(1.0, 0.0), Vec2::new(7.0, 5.0)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
                p.line_segment([c + Vec2::new(4.5, -3.5), c + Vec2::new(4.5, 3.5)], s);
                p.line_segment([c + Vec2::new(-2.5, 0.0), c + Vec2::new(-6.0, 0.0)], s);
            } else {
                // Pin standing up.
                p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, -1.0), Vec2::new(5.0, 7.0)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
                p.line_segment([c + Vec2::new(-3.5, 2.5), c + Vec2::new(3.5, 2.5)], s);
                p.line_segment([c + Vec2::new(0.0, 2.5), c + Vec2::new(0.0, 6.0)], s);
            }
        }
        let has_menu = opts & OPT_HAS_MENU_BUTTON != 0;
        let mut menu = None;
        if has_menu {
            let r = Rect::from_center_size(Pos2::new(hdr.right() - 13.0 - slot * 22.0, y), Vec2::splat(20.0));
            let b = ui.interact(r, Id::unique(("pane_btn", id, "menu")), Sense::click()).on_hover_text("Window position");
            if b.hovered() {
                p.rect_filled(r, CornerRadius::same(5), with_alpha(t.accent, 40));
            }
            let c = r.center();
            p.add(EShape::convex_polygon(vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 2.5)], col(b.hovered(), true).color, Stroke::NONE));
            egui::Popup::menu(&b).show(|ui| {
                menu = pane_menu_items(ui, st, opts);
            });
        }

        acts.close = close;
        acts.pin = pin;
        acts.toggle_docking = resp.double_clicked();
        acts.activate = resp.clicked() || resp.drag_started();
        acts.menu = menu;
        if resp.hovered() && !(hc) {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        if resp.drag_started() {
            if let Some(pp) = resp.interact_pointer_pos() {
                acts.drag_start = Some(pp - hdr.min);
            }
        }
        let mut ctx_menu = None;
        // A right-click on a pane shows the same pane menu at the pointer
        // (D7).
        resp.context_menu(|ui| {
            ctx_menu = pane_menu_items(ui, st, opts);
            ui.separator();
            ui.label(egui::RichText::new("Side panels").strong());
            for &pid in PaneId::for_workspace(ws) {
                let on = self.dock.get(pid).open;
                let last = self.dock.options(ws, pid) & OPT_NO_CLOSEABLE != 0;
                if ui.add_enabled(!last, egui::Checkbox::new(&mut on.clone(), pid.title())).clicked() {
                    ctx_menu = Some(MenuAct::Toggle(pid));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Reset panel layout").clicked() {
                ctx_menu = Some(MenuAct::Reset);
                ui.close();
            }
        });
        if ctx_menu.is_some() {
            acts.menu = ctx_menu;
        }
        acts
    }

    /// The contents of each pane.
    pub fn pane_body(&mut self, ui: &mut egui::Ui, id: PaneId) {
        match id {
            PaneId::NestProps | PaneId::PartProps => self.props_body(ui),
            _ => {
                egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 4, top: 0, bottom: 0 }).show(ui, |ui| match id {
                    PaneId::NestTree => self.nest_tree_body(ui),
                    PaneId::PartList => self.part_list_body(ui),
                    PaneId::JobTree => self.job_tree_body(ui),
                    _ => self.job_summary_body(ui),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_reorder_move_sides_float_and_reset() {
        let ws = Workspace::Part;
        let mut d = Dock::default();
        assert_eq!(d.docked(ws, Side::Left), vec![PaneId::PartList, PaneId::PartProps]);
        d.apply_drop(ws, PaneId::PartProps, Drop::Dock(Side::Left, 0));
        assert_eq!(d.docked(ws, Side::Left), vec![PaneId::PartProps, PaneId::PartList]);
        d.apply_drop(ws, PaneId::PartList, Drop::Dock(Side::Right, 0));
        assert_eq!(d.docked(ws, Side::Left), vec![PaneId::PartProps]);
        assert_eq!(d.docked(ws, Side::Right), vec![PaneId::PartList]);
        d.apply_drop(ws, PaneId::PartProps, Drop::Float(Pos2::new(400.0, 300.0)));
        assert!(d.docked(ws, Side::Left).is_empty());
        assert_eq!(d.floating(ws), vec![PaneId::PartProps]);
        assert_eq!(d.docked(Workspace::Nesting, Side::Left), vec![PaneId::NestTree, PaneId::NestProps]);
        d.reset(ws);
        assert_eq!(d.docked(ws, Side::Left), vec![PaneId::PartList, PaneId::PartProps]);
        assert!(d.floating(ws).is_empty());
    }

    #[test]
    fn panes_dock_only_on_stickers() {
        let ws = Workspace::Nesting;
        let mut d = Dock::default();
        d.area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0));
        d.side_rects[0] = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(250.0, 900.0)));
        d.pane_rects = vec![
            (PaneId::NestTree, Side::Left, Rect::from_min_size(Pos2::ZERO, Vec2::new(250.0, 400.0))),
            (PaneId::NestProps, Side::Left, Rect::from_min_size(Pos2::new(0.0, 400.0), Vec2::new(250.0, 500.0))),
        ];
        // Over the Explorer: its "up" sticker docks above it, "down" below it.
        let c = Pos2::new(125.0, 200.0);
        assert_eq!(d.drop_target(ws, PaneId::NestProps, c - Vec2::new(0.0, STICKER * 0.6), Vec2::ZERO), Drop::Dock(Side::Left, 0));
        assert_eq!(d.drop_target(ws, PaneId::NestProps, c + Vec2::new(0.0, STICKER * 0.6), Vec2::ZERO), Drop::Dock(Side::Left, 1));
        // The right-edge sticker docks on the right.
        assert_eq!(d.drop_target(ws, PaneId::NestTree, Pos2::new(1570.0, 450.0), Vec2::ZERO), Drop::Dock(Side::Right, 0));
        // Anywhere else it floats where it was dropped.
        assert_eq!(d.drop_target(ws, PaneId::NestTree, Pos2::new(800.0, 300.0), Vec2::new(10.0, 10.0)), Drop::Float(Pos2::new(790.0, 290.0)));
        // A pane set to Floating never docks, even on a sticker.
        d.menu_floating(PaneId::NestTree);
        assert!(d.get(PaneId::NestTree).floating && d.get(PaneId::NestTree).no_dock);
        assert!(matches!(d.drop_target(ws, PaneId::NestTree, Pos2::new(1570.0, 450.0), Vec2::ZERO), Drop::Float(_)));
        d.menu_docking(PaneId::NestTree);
        assert!(!d.get(PaneId::NestTree).no_dock);
        assert!(d.get(PaneId::NestTree).floating, "Docking only lets it dock again");
    }

    #[test]
    fn last_open_pane_cannot_be_hidden_and_auto_hide_folds_to_a_tab() {
        let ws = Workspace::Jobs;
        let mut d = Dock::default();
        assert_eq!(d.options(ws, PaneId::JobTree) & OPT_NO_CLOSEABLE, 0);
        assert!(d.close(ws, PaneId::JobSummary));
        assert_ne!(d.options(ws, PaneId::JobTree) & OPT_NO_CLOSEABLE, 0);
        assert!(!d.close(ws, PaneId::JobTree));
        assert!(d.get(PaneId::JobTree).open);
        d.get_mut(PaneId::JobSummary).open = true;
        assert_eq!(d.options(ws, PaneId::JobTree) & OPT_NO_CLOSEABLE, 0);
        d.toggle_auto_hide(PaneId::JobTree);
        assert_eq!(d.docked(ws, Side::Left), vec![PaneId::JobSummary]);
        assert_eq!(d.auto_hidden(ws, Side::Left), vec![PaneId::JobTree]);
        assert_ne!(d.options(ws, PaneId::JobTree) & OPT_HAS_MENU_BUTTON, 0);
    }

    #[test]
    fn layout_survives_save_and_load_and_keyboard_steps_through_panes() {
        let ws = Workspace::Part;
        let mut d = Dock::default();
        d.apply_drop(ws, PaneId::PartProps, Drop::Dock(Side::Right, 0));
        d.toggle_auto_hide(PaneId::PartList);
        d.get_mut(PaneId::NestTree).height = 333.0;
        let back = Dock::from_text(&d.to_text()).unwrap();
        for id in ALL {
            assert_eq!(back.get(id), d.get(id), "{id:?}");
        }
        assert_eq!(back.order, d.order);
        assert!(Dock::from_text("something else").is_none());
        let mut d = Dock::default();
        assert_eq!(d.next_pane(ws, false), Some(PaneId::PartList));
        d.active = Some(PaneId::PartList);
        assert_eq!(d.next_pane(ws, false), Some(PaneId::PartProps));
        assert_eq!(d.next_pane(ws, true), Some(PaneId::PartProps));
    }
}

/// A scripted mouse session over the panes (`--drag-test`): drag a pane onto the
/// right sticker, drag one out to float, drop the floating one back on a pane's
/// sticker, pin one to Auto Hide, point at its tab, and step with Ctrl+Tab.
pub struct DragTest {
    from: Pos2,
    to: Pos2,
    pub log: Vec<String>,
}

impl Default for DragTest {
    fn default() -> Self {
        DragTest { from: Pos2::ZERO, to: Pos2::ZERO, log: vec![] }
    }
}

impl App {
    pub fn drag_test_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput, f: u32) {
        use egui::{Event, Modifiers, PointerButton};
        let ws = self.workspace;
        let a = self.dock.area;
        let first_docked = |d: &Dock| d.pane_rects.first().map(|x| x.2);
        let press = |raw: &mut egui::RawInput, pos: Pos2, pressed: bool| {
            raw.events.push(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        };
        // (start frame, kind)
        let seg = |start: u32| f >= start && f <= start + 32;
        let mut plan: Option<(Pos2, Pos2)> = None;
        if f == 20 {
            if let Some(r) = first_docked(&self.dock) {
                plan = Some((Pos2::new(r.left() + 60.0, r.top() + 13.0), Pos2::new(a.right() - 30.0, a.center().y)));
            }
        } else if f == 70 {
            if let Some(r) = first_docked(&self.dock) {
                plan = Some((Pos2::new(r.left() + 60.0, r.top() + 13.0), a.center()));
            }
        } else if f == 120 {
            if let (Some(&fl), Some(r)) = (self.dock.floating(ws).first(), first_docked(&self.dock)) {
                let s = self.dock.get(fl);
                plan = Some((s.float_pos + Vec2::new(60.0, 13.0), r.center() - Vec2::new(0.0, STICKER * 0.6)));
            }
        }
        if let Some((from, to)) = plan {
            self.drag_test.from = from;
            self.drag_test.to = to;
            self.drag_test.log.push(format!("frame {f}: drag {from:?} -> {to:?}"));
        }
        for start in [20u32, 70, 120] {
            if seg(start) {
                let k = f - start;
                let (from, to) = (self.drag_test.from, self.drag_test.to);
                let t = (k.saturating_sub(4) as f32 / 24.0).clamp(0.0, 1.0);
                let pos = from + (to - from) * t;
                raw.events.push(Event::PointerMoved(pos));
                if k == 2 {
                    press(raw, pos, true);
                }
                if k == 31 {
                    press(raw, pos, false);
                }
            }
        }
        // Pin the first docked pane (Auto Hide), then point at its tab and away.
        if f == 170 {
            if let Some(r) = first_docked(&self.dock) {
                self.drag_test.from = Pos2::new(r.right() - 13.0 - 22.0, r.top() + 13.0);
                self.drag_test.log.push(format!("frame {f}: pin at {:?}", self.drag_test.from));
            }
        }
        if (171..=174).contains(&f) {
            let p = self.drag_test.from;
            raw.events.push(Event::PointerMoved(p));
            if f == 172 {
                press(raw, p, true);
            }
            if f == 173 {
                press(raw, p, false);
            }
        }
        if (190..=215).contains(&f) {
            if let Some(&(_, r)) = self.dock.tab_rects.first() {
                raw.events.push(Event::PointerMoved(r.center()));
            }
        }
        if (216..=250).contains(&f) {
            raw.events.push(Event::PointerMoved(a.center()));
        }
        if f == 260 || f == 262 {
            raw.events.push(Event::Key { key: Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::CTRL });
        }
        // Double-click a docked title (floats it), then the floating title (docks it).
        if f == 268 || f == 280 {
            let target = if f == 268 { first_docked(&self.dock).map(|r| Pos2::new(r.left() + 60.0, r.top() + 13.0)) } else { self.dock.floating(ws).first().map(|&id| self.dock.get(id).float_pos + Vec2::new(60.0, 13.0)) };
            if let Some(p) = target {
                self.drag_test.log.push(format!("frame {f}: double-click {p:?}"));
                raw.events.push(Event::PointerMoved(p));
                for pressed in [true, false, true, false] {
                    press(raw, p, pressed);
                }
            }
        }
        if f == 290 {
            self.drag_test.log.push(format!("done: {}", self.dock.to_text().replace('\n', " | ")));
            for l in &self.drag_test.log {
                eprintln!("drag-test {l}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

/// The original's pane menu (D2, D3): Floating / Docking are a radio pair,
/// Auto Hide is ticked when on, Hide is greyed out for panes that can't close.
/// Collapse/Expand is the rebuild's extra.
fn pane_menu_items(ui: &mut egui::Ui, st: PaneState, opts: u32) -> Option<MenuAct> {
    let mut act = None;
    let hidden = st.auto_hide;
    let can_float = !hidden && opts & OPT_NO_FLOATABLE == 0; // option 4: only the main work panes
    let can_close = opts & OPT_NO_CLOSEABLE == 0;
    if ui.add_enabled(can_float, egui::Button::selectable(st.no_dock && !hidden, "Floating")).clicked() {
        act = Some(MenuAct::Floating);
    }
    if ui.add_enabled(!hidden, egui::Button::selectable(!st.no_dock && !hidden, "Docking")).clicked() {
        act = Some(MenuAct::Docking);
    }
    if ui.add(egui::Button::selectable(hidden, "Auto Hide")).clicked() {
        act = Some(MenuAct::AutoHide);
    }
    if ui.add_enabled(can_close, egui::Button::new("Hide")).clicked() {
        act = Some(MenuAct::Hide);
    }
    ui.separator();
    if ui.add_enabled(!hidden, egui::Button::new(if st.collapsed { "Expand" } else { "Collapse" })).clicked() {
        act = Some(MenuAct::Collapse);
    }
    if act.is_some() {
        ui.close();
    }
    act
}

#[cfg(test)]
mod notify_tests {
    use super::*;

    #[test]
    fn hiding_a_pane_activates_the_next_open_one() {
        let ws = Workspace::Nesting;
        let mut d = Dock::default();
        d.active = Some(PaneId::NestTree);
        d.toggle_auto_hide(PaneId::NestProps);
        assert!(d.close(ws, PaneId::NestTree));
        assert_eq!(d.active, Some(PaneId::NestProps));
        // The next pane is auto-hidden, so it slides out.
        assert_eq!(d.slide, Some(PaneId::NestProps));
        // A collapsed next pane opens up.
        let mut d = Dock::default();
        d.get_mut(PaneId::PartProps).collapsed = true;
        assert!(d.close(Workspace::Part, PaneId::PartList));
        assert_eq!(d.active, Some(PaneId::PartProps));
        assert!(!d.get(PaneId::PartProps).collapsed);
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;

    #[test]
    fn layout_action_mask_matches_the_original() {
        let on: Vec<u32> = (0..=0x12).filter(|&a| action_posts_layout(a)).collect();
        assert_eq!(on, vec![0, 8, 10, 12, 17, 18]);
        assert!(!action_posts_layout(9) && !action_posts_layout(14) && !action_posts_layout(30));
    }
}
