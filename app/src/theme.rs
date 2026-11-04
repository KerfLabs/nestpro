//! Light and dark colour schemes: neutral greys with an indigo accent.

use eframe::egui;
use egui::{Color32, CornerRadius, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Theme {
    pub dark: bool,
    /// Window background behind everything.
    pub bg: Color32,
    /// Panes (explorer, parts view, program).
    pub panel: Color32,
    /// Title row, tabs, ribbon and status bar.
    pub chrome: Color32,
    /// Pane title bars.
    pub pane_header: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_dim: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    /// Filled accent (primary buttons, toggles).
    pub accent: Color32,
    /// Accent for text and lines on the background.
    pub accent_fg: Color32,
    /// Very soft accent tint (selection, active toggles).
    pub accent_soft: Color32,
    pub accent_border: Color32,
    pub ok_bg: Color32,
    pub ok_fg: Color32,
    pub warn_bg: Color32,
    pub warn_fg: Color32,
    pub card: Color32,
    pub field: Color32,
    /// Floating bars over the sheet view.
    pub overlay: Color32,
    pub canvas_a: Color32,
    pub canvas_b: Color32,
    pub sheet: Color32,
    pub sheet_edge: Color32,
    pub part_edge: Color32,
    pub shadow: u8,
}

pub const LIGHT: Theme = Theme {
    dark: false,
    bg: Color32::from_rgb(244, 244, 246),
    panel: Color32::from_rgb(255, 255, 255),
    chrome: Color32::from_rgb(255, 255, 255),
    pane_header: Color32::from_rgb(249, 249, 251),
    border: Color32::from_rgb(228, 228, 233),
    text: Color32::from_rgb(39, 39, 46),
    text_strong: Color32::from_rgb(18, 18, 23),
    text_dim: Color32::from_rgb(113, 113, 125),
    hover: Color32::from_rgb(243, 243, 246),
    pressed: Color32::from_rgb(230, 230, 236),
    accent: Color32::from_rgb(79, 70, 229),
    accent_fg: Color32::from_rgb(79, 70, 229),
    accent_soft: Color32::from_rgb(238, 240, 255),
    accent_border: Color32::from_rgb(199, 210, 254),
    ok_bg: Color32::from_rgb(220, 252, 231),
    ok_fg: Color32::from_rgb(21, 128, 61),
    warn_bg: Color32::from_rgb(255, 237, 213),
    warn_fg: Color32::from_rgb(194, 65, 12),
    card: Color32::from_rgb(255, 255, 255),
    field: Color32::from_rgb(244, 244, 246),
    overlay: Color32::from_rgba_premultiplied(20, 20, 26, 225),
    canvas_a: Color32::from_rgb(15, 17, 23),
    canvas_b: Color32::from_rgb(28, 31, 44),
    sheet: Color32::from_rgb(34, 38, 52),
    sheet_edge: Color32::from_rgb(98, 106, 126),
    part_edge: Color32::from_rgba_premultiplied(150, 150, 150, 150),
    shadow: 28,
};

pub const DARK: Theme = Theme {
    dark: true,
    bg: Color32::from_rgb(9, 9, 12),
    panel: Color32::from_rgb(17, 17, 21),
    chrome: Color32::from_rgb(17, 17, 21),
    pane_header: Color32::from_rgb(22, 22, 27),
    border: Color32::from_rgb(38, 38, 45),
    text: Color32::from_rgb(220, 220, 226),
    text_strong: Color32::from_rgb(244, 244, 247),
    text_dim: Color32::from_rgb(139, 139, 150),
    hover: Color32::from_rgb(30, 30, 37),
    pressed: Color32::from_rgb(40, 40, 48),
    accent: Color32::from_rgb(99, 102, 241),
    accent_fg: Color32::from_rgb(165, 180, 252),
    accent_soft: Color32::from_rgb(30, 28, 70),
    accent_border: Color32::from_rgb(67, 56, 202),
    ok_bg: Color32::from_rgb(10, 46, 26),
    ok_fg: Color32::from_rgb(74, 222, 128),
    warn_bg: Color32::from_rgb(60, 24, 8),
    warn_fg: Color32::from_rgb(251, 146, 60),
    card: Color32::from_rgb(24, 24, 29),
    field: Color32::from_rgb(11, 11, 14),
    overlay: Color32::from_rgba_premultiplied(26, 26, 32, 235),
    canvas_a: Color32::from_rgb(6, 7, 10),
    canvas_b: Color32::from_rgb(16, 18, 26),
    sheet: Color32::from_rgb(26, 29, 40),
    sheet_edge: Color32::from_rgb(78, 84, 102),
    part_edge: Color32::from_rgba_premultiplied(130, 130, 130, 130),
    shadow: 90,
};

static IS_DARK: AtomicBool = AtomicBool::new(false);

/// The colour scheme in use right now.
pub fn th() -> &'static Theme {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    th().dark
}

/// Switch scheme and restyle egui's own widgets to match.
pub fn set_dark(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    let t = th();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.hover;
    v.code_bg_color = t.field;
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = Stroke::new(1.0, t.accent_fg);
    v.hyperlink_color = t.accent_fg;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow.color = Color32::from_black_alpha(if dark { 140 } else { 40 });
    v.popup_shadow.color = Color32::from_black_alpha(if dark { 120 } else { 30 });
    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.bg_fill = t.panel;
    for (st, fill) in [(&mut w.inactive, t.hover), (&mut w.hovered, t.pressed), (&mut w.active, t.accent_soft), (&mut w.open, t.hover)] {
        st.corner_radius = r;
        st.weak_bg_fill = fill;
        st.bg_fill = fill;
        st.expansion = 0.0;
    }
    w.noninteractive.corner_radius = r;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, t.text);
    w.hovered.bg_stroke = Stroke::new(1.0, t.accent_border);
    w.hovered.fg_stroke = Stroke::new(1.5, t.text_strong);
    w.active.bg_stroke = Stroke::new(1.0, t.accent);
    w.active.fg_stroke = Stroke::new(1.5, t.text_strong);
    w.open.bg_stroke = Stroke::new(1.0, t.border);
    w.open.fg_stroke = Stroke::new(1.0, t.text_strong);
    ctx.set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.all_styles_mut(|s| {
        s.visuals = v.clone();
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
        s.spacing.interact_size.y = 24.0;
    });
}
