//! App-wide look: one palette, one set of spacings and text sizes.
//!
//! Screens are rebuilt (not ported): the target is the Horos viewer's neutral
//! dark chrome, with thin separators, flat toolbar buttons and one accent colour.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme};

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Accent colours offered in Settings (English name, Spanish name, colour).
pub const ACCENT_PRESETS: [(&str, &str, Color32); 8] = [
    ("Blue", "Azul", Color32::from_rgb(47, 125, 225)),
    ("Teal", "Verde azulado", Color32::from_rgb(20, 150, 150)),
    ("Green", "Verde", Color32::from_rgb(52, 150, 80)),
    ("Amber", "Ámbar", Color32::from_rgb(205, 135, 30)),
    ("Red", "Rojo", Color32::from_rgb(200, 60, 60)),
    ("Pink", "Rosa", Color32::from_rgb(200, 70, 140)),
    ("Purple", "Morado", Color32::from_rgb(125, 85, 210)),
    ("Graphite", "Grafito", Color32::from_rgb(105, 110, 122)),
];
pub const DEFAULT_ACCENT: Color32 = ACCENT_PRESETS[0].2;

static ACCENT_RGB: AtomicU32 = AtomicU32::new(0x002F_7DE1); // DEFAULT_ACCENT
static TINT_CHROME: AtomicBool = AtomicBool::new(false);
static OLED_MODE: AtomicBool = AtomicBool::new(false);

pub fn oled_mode() -> bool {
    OLED_MODE.load(Ordering::Relaxed)
}

pub fn set_oled(ctx: &egui::Context, oled: bool) {
    OLED_MODE.store(oled, Ordering::Relaxed);
    install(ctx);
}

/// Selection / active-tool accent (chosen in Settings).
pub fn accent() -> Color32 {
    let v = ACCENT_RGB.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Lighter accent for borders and text on dark backgrounds.
pub fn accent_bright() -> Color32 {
    let c = accent();
    let lift = |x: u8| x.saturating_add(((255 - x) as f32 * 0.4) as u8);
    Color32::from_rgb(lift(c.r()), lift(c.g()), lift(c.b()))
}

/// Changes the accent and whether the toolbars are tinted with it, then
/// re-installs the looks so every widget picks it up on the next frame.
pub fn set_accent(ctx: &egui::Context, color: Color32, tint_chrome: bool) {
    let v = ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | color.b() as u32;
    ACCENT_RGB.store(v, Ordering::Relaxed);
    TINT_CHROME.store(tint_chrome, Ordering::Relaxed);
    install(ctx);
}

pub fn tint_chrome() -> bool {
    TINT_CHROME.load(Ordering::Relaxed)
}

/// `#rrggbb` for storing in settings.
pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Parses `#rrggbb` (the `#` is optional).
pub fn from_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

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
        // A complete volume: every plane available.
        "MPR" => Color32::from_rgb(236, 178, 72),
        // The same reconstruction as complete volumes in several planes: the gold of
        // MPR leaning towards the plane's colour. Gold and blue are opposites (an even
        // mix is grey), so the axial one is a soft sky blue with a warm cast.
        "MPR-A" => Color32::from_rgb(150, 178, 222),
        "MPR-C" => Color32::from_rgb(176, 202, 92),
        "MPR-S" => Color32::from_rgb(240, 146, 86),
        _ => Color32::from_rgb(170, 174, 182),
    }
}

/// Installs the dark and light looks and the shared spacing. Call once at startup.
pub fn install(ctx: &egui::Context) {
    let oled = oled_mode();
    let mut dark = egui::Visuals::dark();
    dark.panel_fill = if oled { Color32::BLACK } else { PANEL };
    dark.window_fill = if oled {
        Color32::from_rgb(8, 8, 10)
    } else {
        Color32::from_rgb(38, 39, 43)
    };
    dark.extreme_bg_color = if oled { Color32::BLACK } else { INSET };
    dark.faint_bg_color = if oled {
        Color32::from_rgb(10, 10, 12)
    } else {
        ROW_ALT
    };
    dark.override_text_color = None;
    dark.window_stroke = Stroke::new(
        1.0,
        if oled {
            Color32::from_rgb(38, 40, 46)
        } else {
            Color32::from_rgb(70, 72, 78)
        },
    );
    dark.window_corner_radius = CornerRadius::same(8);
    dark.menu_corner_radius = CornerRadius::same(6);
    dark.selection.bg_fill = accent();
    dark.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    dark.hyperlink_color = accent_bright();

    let r = CornerRadius::same(5);
    let w = &mut dark.widgets;
    w.noninteractive.bg_fill = if oled { Color32::BLACK } else { PANEL };
    w.noninteractive.bg_stroke = Stroke::new(
        1.0,
        if oled {
            Color32::from_rgb(32, 34, 40)
        } else {
            Color32::from_rgb(58, 60, 66)
        },
    );
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;

    let inactive_fill = if oled {
        Color32::from_rgb(20, 21, 24)
    } else {
        Color32::from_rgb(62, 64, 70)
    };
    w.inactive.bg_fill = inactive_fill;
    w.inactive.weak_bg_fill = inactive_fill;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    let hovered_fill = if oled {
        Color32::from_rgb(36, 38, 44)
    } else {
        Color32::from_rgb(80, 83, 91)
    };
    w.hovered.bg_fill = hovered_fill;
    w.hovered.weak_bg_fill = hovered_fill;
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(110, 114, 124));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    let active_fill = if oled {
        Color32::from_rgb(52, 55, 64)
    } else {
        Color32::from_rgb(96, 100, 110)
    };
    w.active.bg_fill = active_fill;
    w.active.weak_bg_fill = active_fill;
    w.active.bg_stroke = Stroke::new(1.0, accent_bright());
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    let open_fill = if oled {
        Color32::from_rgb(28, 30, 35)
    } else {
        Color32::from_rgb(72, 75, 82)
    };
    w.open.bg_fill = open_fill;
    w.open.weak_bg_fill = open_fill;
    w.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(110, 114, 124));
    w.open.corner_radius = r;
    ctx.set_visuals_of(Theme::Dark, dark);

    let mut light = egui::Visuals::light();
    light.selection.bg_fill = accent();
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
    // Buttons need a visible body on the pale panels.
    light.widgets.inactive.bg_fill = Color32::from_rgb(212, 215, 221);
    light.widgets.inactive.weak_bg_fill = Color32::from_rgb(212, 215, 221);
    light.widgets.hovered.bg_fill = Color32::from_rgb(196, 200, 208);
    light.widgets.hovered.weak_bg_fill = Color32::from_rgb(196, 200, 208);
    light.widgets.active.bg_fill = Color32::from_rgb(180, 185, 195);
    light.widgets.active.weak_bg_fill = Color32::from_rgb(180, 185, 195);
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
    let base = if ctx.global_style().visuals.dark_mode {
        if oled_mode() {
            Color32::BLACK
        } else {
            CHROME
        }
    } else {
        Color32::from_rgb(232, 233, 236)
    };
    if tint_chrome() {
        mix(base, accent(), 0.16)
    } else {
        base
    }
}

/// `a` moved `t` of the way towards `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Secondary text colour for the current theme.
pub fn dim_text(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) {
        TEXT_DIM
    } else {
        Color32::from_rgb(95, 98, 106)
    }
}

/// Primary (bright) text colour for custom-coloured labels in panels.
pub fn strong_text(ui: &egui::Ui) -> Color32 {
    ui.visuals().strong_text_color()
}

/// Green used for "done / connected / up to date" messages.
pub fn ok_color(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) { Color32::from_rgb(120, 225, 150) } else { Color32::from_rgb(20, 130, 60) }
}

/// Amber used for warnings and notices.
pub fn warn_color(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) { Color32::from_rgb(255, 200, 80) } else { Color32::from_rgb(170, 105, 0) }
}

/// Red used for errors and destructive confirmations.
pub fn error_color(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) { Color32::from_rgb(240, 115, 115) } else { Color32::from_rgb(190, 35, 35) }
}

/// Gold used for key-image stars and other "marked" highlights.
pub fn star_color(ui: &egui::Ui) -> Color32 {
    if is_dark(ui) { Color32::from_rgb(255, 205, 65) } else { Color32::from_rgb(200, 140, 0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_rejects_bad_input() {
        for (_, _, c) in ACCENT_PRESETS {
            assert_eq!(from_hex(&to_hex(c)), Some(c));
        }
        assert_eq!(from_hex("2f7de1"), Some(DEFAULT_ACCENT));
        assert_eq!(from_hex("#12345"), None);
        assert_eq!(from_hex("#gg0000"), None);
    }

    #[test]
    fn default_accent_matches_initial_value() {
        assert_eq!(accent(), DEFAULT_ACCENT);
    }
}
