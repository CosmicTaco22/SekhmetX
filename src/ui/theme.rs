//! App-wide look: one palette, one set of spacings and text sizes.
//!
//! Screens are rebuilt (not ported): the target is the Horos viewer's neutral
//! dark chrome, with thin separators, flat toolbar buttons and one accent colour.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme};

/// Selection / active-tool accent.
pub const ACCENT: Color32 = Color32::from_rgb(47, 125, 225);
/// Brighter accent for borders and text on dark backgrounds.
pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(110, 175, 255);

/// Background of ordinary panels (sidebar, library).
pub const PANEL: Color32 = Color32::from_rgb(30, 31, 34);
/// Background of the toolbar and status bar.
pub const CHROME: Color32 = Color32::from_rgb(43, 44, 48);
/// Inset surfaces: text fields, table bodies, thumbnail wells.
pub const INSET: Color32 = Color32::from_rgb(20, 21, 23);
/// Alternate table row.
pub const ROW_ALT: Color32 = Color32::from_rgb(26, 27, 30);
/// Table header strip and cards.
pub const RAISED: Color32 = Color32::from_rgb(52, 54, 59);

pub const TEXT: Color32 = Color32::from_rgb(222, 224, 228);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 154, 162);

/// Colour of a plane badge (AX / COR / SAG / other), matching the MPR pane colours.
pub fn plane_color(badge: &str) -> Color32 {
    match badge {
        "AX" => Color32::from_rgb(90, 160, 255),
        "COR" => Color32::from_rgb(95, 200, 120),
        "SAG" => Color32::from_rgb(240, 110, 100),
        _ => Color32::from_rgb(170, 174, 182),
    }
}

/// Installs the dark and light looks and the shared spacing. Call once at startup.
pub fn install(ctx: &egui::Context) {
    let mut dark = egui::Visuals::dark();
    dark.panel_fill = PANEL;
    dark.window_fill = Color32::from_rgb(38, 39, 43);
    dark.extreme_bg_color = INSET;
    dark.faint_bg_color = ROW_ALT;
    dark.override_text_color = None;
    dark.window_stroke = Stroke::new(1.0, Color32::from_rgb(70, 72, 78));
    dark.window_corner_radius = CornerRadius::same(8);
    dark.menu_corner_radius = CornerRadius::same(6);
    dark.selection.bg_fill = ACCENT;
    dark.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    dark.hyperlink_color = ACCENT_BRIGHT;

    let r = CornerRadius::same(5);
    let w = &mut dark.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(58, 60, 66));
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = Color32::from_rgb(62, 64, 70);
    w.inactive.weak_bg_fill = Color32::from_rgb(62, 64, 70);
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = Color32::from_rgb(80, 83, 91);
    w.hovered.weak_bg_fill = Color32::from_rgb(80, 83, 91);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(110, 114, 124));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = Color32::from_rgb(96, 100, 110);
    w.active.weak_bg_fill = Color32::from_rgb(96, 100, 110);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT_BRIGHT);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = Color32::from_rgb(72, 75, 82);
    w.open.weak_bg_fill = Color32::from_rgb(72, 75, 82);
    w.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(110, 114, 124));
    w.open.corner_radius = r;
    ctx.set_visuals_of(Theme::Dark, dark);

    let mut light = egui::Visuals::light();
    light.selection.bg_fill = ACCENT;
    light.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    light.window_corner_radius = CornerRadius::same(8);
    light.menu_corner_radius = CornerRadius::same(6);
    for wv in [
        &mut light.widgets.noninteractive,
        &mut light.widgets.inactive,
        &mut light.widgets.hovered,
        &mut light.widgets.active,
        &mut light.widgets.open,
    ] {
        wv.corner_radius = r;
    }
    light.widgets.hovered.expansion = 0.0;
    light.widgets.active.expansion = 0.0;
    ctx.set_visuals_of(Theme::Light, light);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size = egui::vec2(36.0, 22.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.window_margin = egui::Margin::same(12);
        style.spacing.combo_height = 320.0;
        style.spacing.scroll.bar_width = 8.0;
        style.text_styles = [
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

/// True when the dark look is active (custom-painted chrome picks its colours from this).
pub fn is_dark(ui: &egui::Ui) -> bool {
    ui.visuals().dark_mode
}

/// Toolbar / status-bar background for the current theme.
pub fn chrome_fill(ctx: &egui::Context) -> Color32 {
    if ctx.global_style().visuals.dark_mode {
        CHROME
    } else {
        Color32::from_rgb(232, 233, 236)
    }
}

/// Secondary text colour for the current theme.
pub fn dim_text(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) {
        TEXT_DIM
    } else {
        Color32::from_rgb(95, 98, 106)
    }
}
