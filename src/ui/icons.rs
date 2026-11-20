//! Toolbar icons drawn as vector shapes, and the buttons that carry them.
//!
//! The icons are our own drawings (the reference viewers' artwork is not used).
//! Each is painted inside a square, in the colour of the surrounding text, so
//! they follow the theme and stay sharp at any display scale.

use std::f32::consts::{PI, TAU};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui,
};

use super::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    WindowLevel,
    Scroll,
    Pan,
    Zoom,
    Rotate,
    Orbit,
    Length,
    Angle,
    Cobb,
    Arrow,
    Oval,
    Rectangle,
    Polygon,
    Pencil,
    Point,
    Library,
    Sidebar,
    SidebarCompact,
    Folder,
    Grid,
    Ortho,
    Cube,
    Volume,
    Eye,
    Info,
    Play,
    Pause,
    Invert,
    RefLines,
    Overlay,
    First,
    Previous,
    Next,
    Last,
    SeriesPrevious,
    SeriesNext,
    Search,
    Close,
    Trash,
    ChevronDown,
    Gear,
    Fusion,
    Curve,
}

/// Maps unit coordinates (0..1, y down) into `rect`.
struct Canvas {
    rect: Rect,
}

impl Canvas {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.min.x + x * self.rect.width(),
            self.rect.min.y + y * self.rect.height(),
        )
    }

    fn len(&self, v: f32) -> f32 {
        v * self.rect.width()
    }

    fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, from: f32, to: f32) -> Vec<Pos2> {
        let steps = 28;
        (0..=steps)
            .map(|i| {
                let a = from + (to - from) * i as f32 / steps as f32;
                self.p(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect()
    }
}

/// Paints `icon` centred in `rect` with the given colour.
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let c = Canvas {
        rect: Rect::from_center_size(rect.center(), vec2(side, side)),
    };
    let s = Stroke::new((side / 12.0).clamp(1.2, 2.0), color);
    let line = |pts: Vec<Pos2>| {
        painter.add(Shape::line(pts, s));
    };
    let closed = |mut pts: Vec<Pos2>| {
        // A full ellipse ends where it starts; a repeated point makes a spike at the join.
        if pts.len() > 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 0.01) {
            pts.pop();
        }
        painter.add(Shape::closed_line(pts, s));
    };
    let filled = |pts: Vec<Pos2>| {
        painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    };
    let seg = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], s);
    };
    // Arrow head at `tip`, pointing along (dx, dy) in unit space.
    let head = |tip: (f32, f32), dir: (f32, f32), size: f32| {
        let n = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-4);
        let (ux, uy) = (dir.0 / n, dir.1 / n);
        let (px, py) = (-uy, ux);
        let back = (tip.0 - ux * size, tip.1 - uy * size);
        filled(vec![
            c.p(tip.0, tip.1),
            c.p(back.0 + px * size * 0.6, back.1 + py * size * 0.6),
            c.p(back.0 - px * size * 0.6, back.1 - py * size * 0.6),
        ]);
    };

    match icon {
        Icon::WindowLevel => {
            // Contrast disc: outline with the left half filled.
            closed(c.ellipse(0.5, 0.5, 0.38, 0.38, 0.0, TAU));
            filled(c.ellipse(0.5, 0.5, 0.38, 0.38, PI * 0.5, PI * 1.5));
        }
        Icon::Scroll => {
            // A stack of slices.
            closed(vec![c.p(0.5, 0.1), c.p(0.92, 0.33), c.p(0.5, 0.56), c.p(0.08, 0.33)]);
            line(vec![c.p(0.08, 0.52), c.p(0.5, 0.75), c.p(0.92, 0.52)]);
            line(vec![c.p(0.08, 0.7), c.p(0.5, 0.93), c.p(0.92, 0.7)]);
        }
        Icon::Pan => {
            seg(c.p(0.5, 0.14), c.p(0.5, 0.86));
            seg(c.p(0.14, 0.5), c.p(0.86, 0.5));
            head((0.5, 0.04), (0.0, -1.0), 0.2);
            head((0.5, 0.96), (0.0, 1.0), 0.2);
            head((0.04, 0.5), (-1.0, 0.0), 0.2);
            head((0.96, 0.5), (1.0, 0.0), 0.2);
        }
        Icon::Zoom | Icon::Search => {
            closed(c.ellipse(0.42, 0.42, 0.3, 0.3, 0.0, TAU));
            painter.line_segment(
                [c.p(0.64, 0.64), c.p(0.93, 0.93)],
                Stroke::new(s.width * 1.6, color),
            );
            if icon == Icon::Zoom {
                let thin = Stroke::new(s.width * 0.85, color);
                painter.line_segment([c.p(0.3, 0.42), c.p(0.54, 0.42)], thin);
                painter.line_segment([c.p(0.42, 0.3), c.p(0.42, 0.54)], thin);
            }
        }
        Icon::Rotate => {
            line(c.ellipse(0.5, 0.52, 0.36, 0.36, -PI * 0.35, PI * 1.3));
            let a = -PI * 0.35;
            head(
                (0.5 + 0.36 * a.cos() + 0.03, 0.52 + 0.36 * a.sin() - 0.1),
                (-0.25, -1.0),
                0.26,
            );
        }
        Icon::Orbit => {
            closed(c.ellipse(0.5, 0.5, 0.4, 0.4, 0.0, TAU));
            closed(c.ellipse(0.5, 0.5, 0.4, 0.15, 0.0, TAU));
            closed(c.ellipse(0.5, 0.5, 0.15, 0.4, 0.0, TAU));
        }
        Icon::Length => {
            seg(c.p(0.16, 0.84), c.p(0.84, 0.16));
            seg(c.p(0.06, 0.74), c.p(0.26, 0.94));
            seg(c.p(0.74, 0.06), c.p(0.94, 0.26));
        }
        Icon::Angle => {
            line(vec![c.p(0.8, 0.12), c.p(0.12, 0.86), c.p(0.92, 0.86)]);
            line(c.ellipse(0.12, 0.86, 0.36, 0.36, -PI * 0.265, 0.0));
        }
        Icon::Cobb => {
            // Two endplate lines and their perpendiculars meeting below.
            seg(c.p(0.08, 0.22), c.p(0.5, 0.08));
            seg(c.p(0.5, 0.92), c.p(0.92, 0.78));
            let thin = Stroke::new(s.width * 0.8, color.gamma_multiply(0.75));
            painter.line_segment([c.p(0.29, 0.15), c.p(0.42, 0.55)], thin);
            painter.line_segment([c.p(0.71, 0.85), c.p(0.42, 0.55)], thin);
        }
        Icon::Arrow => {
            seg(c.p(0.14, 0.86), c.p(0.8, 0.2));
            head((0.9, 0.1), (1.0, -1.0), 0.32);
        }
        Icon::Oval => closed(c.ellipse(0.5, 0.5, 0.42, 0.3, 0.0, TAU)),
        Icon::Rectangle => {
            closed(vec![c.p(0.1, 0.22), c.p(0.9, 0.22), c.p(0.9, 0.78), c.p(0.1, 0.78)]);
        }
        Icon::Polygon => {
            closed(vec![
                c.p(0.5, 0.08),
                c.p(0.92, 0.4),
                c.p(0.76, 0.9),
                c.p(0.24, 0.9),
                c.p(0.08, 0.4),
            ]);
        }
        Icon::Pencil => {
            closed(vec![
                c.p(0.7, 0.1),
                c.p(0.9, 0.3),
                c.p(0.34, 0.86),
                c.p(0.1, 0.9),
                c.p(0.14, 0.66),
            ]);
            seg(c.p(0.58, 0.22), c.p(0.78, 0.42));
        }
        Icon::Curve => {
            // An S-shaped path through three points, with a straight line below:
            // a curve laid out flat.
            line(vec![
                c.p(0.1, 0.62),
                c.p(0.22, 0.34),
                c.p(0.38, 0.2),
                c.p(0.52, 0.34),
                c.p(0.6, 0.52),
                c.p(0.74, 0.6),
                c.p(0.9, 0.36),
            ]);
            for (x, y) in [(0.1, 0.62), (0.52, 0.34), (0.9, 0.36)] {
                painter.circle_filled(c.p(x, y), c.len(0.07), color);
            }
            seg(c.p(0.1, 0.88), c.p(0.9, 0.88));
        }
        Icon::Point => {
            closed(c.ellipse(0.5, 0.5, 0.26, 0.26, 0.0, TAU));
            painter.circle_filled(c.p(0.5, 0.5), c.len(0.07), color);
            seg(c.p(0.5, 0.04), c.p(0.5, 0.24));
            seg(c.p(0.5, 0.76), c.p(0.5, 0.96));
            seg(c.p(0.04, 0.5), c.p(0.24, 0.5));
            seg(c.p(0.76, 0.5), c.p(0.96, 0.5));
        }
        Icon::Library => {
            // Database cylinder.
            closed(c.ellipse(0.5, 0.22, 0.38, 0.14, 0.0, TAU));
            line(c.ellipse(0.5, 0.5, 0.38, 0.14, 0.0, PI));
            line(c.ellipse(0.5, 0.78, 0.38, 0.14, 0.0, PI));
            seg(c.p(0.12, 0.22), c.p(0.12, 0.78));
            seg(c.p(0.88, 0.22), c.p(0.88, 0.78));
        }
        Icon::Sidebar | Icon::SidebarCompact => {
            let split = if icon == Icon::Sidebar { 0.4 } else { 0.26 };
            closed(vec![c.p(0.08, 0.16), c.p(0.92, 0.16), c.p(0.92, 0.84), c.p(0.08, 0.84)]);
            filled(vec![c.p(0.08, 0.16), c.p(split, 0.16), c.p(split, 0.84), c.p(0.08, 0.84)]);
        }
        Icon::Folder => {
            closed(vec![
                c.p(0.08, 0.2),
                c.p(0.4, 0.2),
                c.p(0.5, 0.32),
                c.p(0.92, 0.32),
                c.p(0.92, 0.84),
                c.p(0.08, 0.84),
            ]);
        }
        Icon::Grid => {
            closed(vec![c.p(0.1, 0.14), c.p(0.9, 0.14), c.p(0.9, 0.86), c.p(0.1, 0.86)]);
            seg(c.p(0.5, 0.14), c.p(0.5, 0.86));
            seg(c.p(0.1, 0.5), c.p(0.9, 0.5));
        }
        Icon::Ortho => {
            // Three panes with a crosshair in the large one.
            closed(vec![c.p(0.06, 0.14), c.p(0.94, 0.14), c.p(0.94, 0.86), c.p(0.06, 0.86)]);
            seg(c.p(0.6, 0.14), c.p(0.6, 0.86));
            seg(c.p(0.6, 0.5), c.p(0.94, 0.5));
            let thin = Stroke::new(1.0, color.gamma_multiply(0.7));
            painter.line_segment([c.p(0.33, 0.26), c.p(0.33, 0.74)], thin);
            painter.line_segment([c.p(0.16, 0.5), c.p(0.5, 0.5)], thin);
        }
        Icon::Cube | Icon::Volume => {
            let top = vec![c.p(0.5, 0.06), c.p(0.9, 0.28), c.p(0.5, 0.5), c.p(0.1, 0.28)];
            let left = vec![c.p(0.1, 0.28), c.p(0.5, 0.5), c.p(0.5, 0.95), c.p(0.1, 0.72)];
            let right = vec![c.p(0.5, 0.5), c.p(0.9, 0.28), c.p(0.9, 0.72), c.p(0.5, 0.95)];
            if icon == Icon::Volume {
                painter.add(Shape::convex_polygon(
                    left.clone(),
                    color.gamma_multiply(0.45),
                    Stroke::NONE,
                ));
                painter.add(Shape::convex_polygon(
                    right.clone(),
                    color.gamma_multiply(0.2),
                    Stroke::NONE,
                ));
                painter.add(Shape::convex_polygon(
                    top.clone(),
                    color.gamma_multiply(0.75),
                    Stroke::NONE,
                ));
            } else {
                // A reslicing plane through the cube.
                painter.add(Shape::convex_polygon(
                    vec![c.p(0.1, 0.5), c.p(0.5, 0.72), c.p(0.9, 0.5), c.p(0.5, 0.28)],
                    color.gamma_multiply(0.35),
                    Stroke::NONE,
                ));
            }
            closed(top);
            closed(left);
            closed(right);
        }
        Icon::Eye => {
            line(c.ellipse(0.5, 0.72, 0.5, 0.5, PI * 1.17, PI * 1.83));
            line(c.ellipse(0.5, 0.28, 0.5, 0.5, PI * 0.17, PI * 0.83));
            painter.circle_filled(c.p(0.5, 0.5), c.len(0.13), color);
        }
        Icon::Fusion => {
            painter.circle_filled(c.p(0.62, 0.5), c.len(0.28), color.gamma_multiply(0.32));
            closed(c.ellipse(0.38, 0.5, 0.28, 0.28, 0.0, TAU));
            closed(c.ellipse(0.62, 0.5, 0.28, 0.28, 0.0, TAU));
        }
        Icon::Info => {
            closed(c.ellipse(0.5, 0.5, 0.4, 0.4, 0.0, TAU));
            painter.circle_filled(c.p(0.5, 0.29), c.len(0.06), color);
            painter.line_segment(
                [c.p(0.5, 0.44), c.p(0.5, 0.74)],
                Stroke::new(s.width * 1.3, color),
            );
        }
        Icon::Play => filled(vec![c.p(0.22, 0.1), c.p(0.9, 0.5), c.p(0.22, 0.9)]),
        Icon::Pause => {
            filled(vec![c.p(0.2, 0.12), c.p(0.42, 0.12), c.p(0.42, 0.88), c.p(0.2, 0.88)]);
            filled(vec![c.p(0.58, 0.12), c.p(0.8, 0.12), c.p(0.8, 0.88), c.p(0.58, 0.88)]);
        }
        Icon::Invert => {
            closed(vec![c.p(0.12, 0.12), c.p(0.88, 0.12), c.p(0.88, 0.88), c.p(0.12, 0.88)]);
            filled(vec![c.p(0.88, 0.12), c.p(0.88, 0.88), c.p(0.12, 0.88)]);
        }
        Icon::RefLines => {
            closed(vec![c.p(0.1, 0.1), c.p(0.9, 0.1), c.p(0.9, 0.9), c.p(0.1, 0.9)]);
            seg(c.p(0.1, 0.38), c.p(0.9, 0.38));
            seg(c.p(0.1, 0.66), c.p(0.9, 0.66));
        }
        Icon::Overlay => {
            closed(vec![c.p(0.08, 0.12), c.p(0.92, 0.12), c.p(0.92, 0.88), c.p(0.08, 0.88)]);
            seg(c.p(0.2, 0.28), c.p(0.52, 0.28));
            seg(c.p(0.2, 0.42), c.p(0.4, 0.42));
            seg(c.p(0.6, 0.72), c.p(0.8, 0.72));
        }
        Icon::First | Icon::Previous | Icon::Next | Icon::Last => {
            let flip = matches!(icon, Icon::Next | Icon::Last);
            let x = |v: f32| if flip { 1.0 - v } else { v };
            let bar = matches!(icon, Icon::First | Icon::Last);
            let (l, r) = if bar { (0.3, 0.86) } else { (0.2, 0.78) };
            filled(vec![c.p(x(l), 0.5), c.p(x(r), 0.14), c.p(x(r), 0.86)]);
            if bar {
                painter.line_segment(
                    [c.p(x(0.17), 0.14), c.p(x(0.17), 0.86)],
                    Stroke::new(s.width * 1.4, color),
                );
            }
        }
        Icon::SeriesPrevious | Icon::SeriesNext => {
            let flip = icon == Icon::SeriesNext;
            let x = |v: f32| if flip { 1.0 - v } else { v };
            line(vec![c.p(x(0.5), 0.14), c.p(x(0.14), 0.5), c.p(x(0.5), 0.86)]);
            line(vec![c.p(x(0.86), 0.14), c.p(x(0.5), 0.5), c.p(x(0.86), 0.86)]);
        }
        Icon::Close => {
            seg(c.p(0.2, 0.2), c.p(0.8, 0.8));
            seg(c.p(0.8, 0.2), c.p(0.2, 0.8));
        }
        Icon::Trash => {
            seg(c.p(0.1, 0.24), c.p(0.9, 0.24));
            line(vec![c.p(0.36, 0.24), c.p(0.4, 0.1), c.p(0.6, 0.1), c.p(0.64, 0.24)]);
            line(vec![c.p(0.2, 0.24), c.p(0.27, 0.92), c.p(0.73, 0.92), c.p(0.8, 0.24)]);
            seg(c.p(0.42, 0.4), c.p(0.42, 0.78));
            seg(c.p(0.58, 0.4), c.p(0.58, 0.78));
        }
        Icon::ChevronDown => {
            line(vec![c.p(0.2, 0.36), c.p(0.5, 0.66), c.p(0.8, 0.36)]);
        }
        Icon::Gear => {
            closed(c.ellipse(0.5, 0.5, 0.3, 0.3, 0.0, TAU));
            painter.circle_filled(c.p(0.5, 0.5), c.len(0.1), color);
            let teeth = Stroke::new(s.width * 1.5, color);
            for i in 0..8 {
                let a = TAU * i as f32 / 8.0;
                painter.line_segment(
                    [
                        c.p(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin()),
                        c.p(0.5 + 0.46 * a.cos(), 0.5 + 0.46 * a.sin()),
                    ],
                    teeth,
                );
            }
        }
    }
}

/// Size of an icon-only toolbar button.
pub const TOOL_SIZE: egui::Vec2 = vec2(30.0, 28.0);

fn button_colors(ui: &Ui, resp: &Response, selected: bool) -> (Color32, Color32) {
    let v = ui.visuals();
    if !ui.is_enabled() {
        return (Color32::TRANSPARENT, v.widgets.noninteractive.fg_stroke.color.gamma_multiply(0.35));
    }
    if selected {
        (theme::accent(), Color32::WHITE)
    } else if resp.is_pointer_button_down_on() {
        (v.widgets.active.bg_fill, v.widgets.active.fg_stroke.color)
    } else if resp.hovered() {
        (v.widgets.hovered.bg_fill, v.widgets.hovered.fg_stroke.color)
    } else {
        (Color32::TRANSPARENT, v.widgets.inactive.fg_stroke.color)
    }
}

/// Flat icon-only button (28 px high); filled with the accent when `selected`.
pub fn tool_button(ui: &mut Ui, icon: Icon, selected: bool) -> Response {
    sized_tool_button(ui, icon, selected, TOOL_SIZE)
}

/// Smaller icon-only button for strips and table rows.
pub fn small_tool_button(ui: &mut Ui, icon: Icon, selected: bool) -> Response {
    sized_tool_button(ui, icon, selected, vec2(24.0, 22.0))
}

fn sized_tool_button(ui: &mut Ui, icon: Icon, selected: bool, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let (bg, fg) = button_colors(ui, &resp, selected);
        ui.painter().rect_filled(rect, 5.0, bg);
        let side = (size.y - 10.0).max(10.0);
        paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(side, side)),
            icon,
            fg,
        );
    }
    resp
}

/// Icon tile with its own caption underneath (the reference toolbar's single items).
///
/// `tint` fills the tile with a fixed colour (used for the highlighted Library button);
/// `menu` adds a small chevron to show the button opens a menu.
pub fn captioned_button(
    ui: &mut Ui,
    icon: Icon,
    caption: &str,
    selected: bool,
    tint: Option<Color32>,
    menu: bool,
) -> Response {
    let font = FontId::proportional(10.0);
    let caption_color = theme::dim_text(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(caption.to_owned(), font, caption_color);
    let width = (galley.size().x + 10.0).max(44.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let tile = Rect::from_center_size(
            pos2(rect.center().x, rect.min.y + 16.0),
            vec2(if menu { 44.0 } else { 36.0 }, 30.0),
        );
        let (mut bg, mut fg) = button_colors(ui, &resp, false);
        if selected && ui.is_enabled() {
            bg = ui.visuals().widgets.active.bg_fill;
            fg = if theme::is_dark(ui) { theme::accent_bright() } else { theme::accent() };
        }
        if let Some(t) = tint
            && ui.is_enabled()
        {
            bg = if resp.hovered() { t.gamma_multiply(1.15) } else { t };
            fg = Color32::WHITE;
        }
        ui.painter().rect_filled(tile, 6.0, bg);
        let icon_center = if menu {
            pos2(tile.center().x - 5.0, tile.center().y)
        } else {
            tile.center()
        };
        paint(
            ui.painter(),
            Rect::from_center_size(icon_center, vec2(20.0, 20.0)),
            icon,
            fg,
        );
        if menu {
            paint(
                ui.painter(),
                Rect::from_center_size(pos2(tile.max.x - 8.0, tile.center().y + 1.0), vec2(8.0, 8.0)),
                Icon::ChevronDown,
                fg.gamma_multiply(0.8),
            );
        }
        let text_color = if !ui.is_enabled() {
            caption_color.gamma_multiply(0.5)
        } else if selected || resp.hovered() {
            ui.visuals().strong_text_color()
        } else {
            caption_color
        };
        ui.painter().text(
            pos2(rect.center().x, rect.max.y - 1.0),
            Align2::CENTER_BOTTOM,
            caption,
            FontId::proportional(10.0),
            text_color,
        );
    }
    resp
}

/// Rounded well that holds a row of `tool_button`s, like a segmented control.
pub fn segmented<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = if theme::is_dark(ui) {
        Color32::from_rgb(30, 31, 34)
    } else {
        Color32::from_rgb(214, 216, 221)
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(7)
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                add(ui)
            })
            .inner
        })
        .inner
}

/// Text toggle that looks like a button when off and takes the accent when on.
pub fn toggle_button(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let mut resp = ui.add(egui::Button::new(label).selected(*value));
    if resp.clicked() {
        *value = !*value;
        resp.mark_changed();
    }
    resp
}

/// One option in a row of mutually exclusive choices (looks like a button, accent when chosen).
pub fn choice(ui: &mut Ui, selected: bool, label: impl Into<egui::WidgetText>) -> Response {
    ui.add(egui::Button::new(label).selected(selected))
}

/// Small dim heading that starts a section in a dialog.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(text.trim_end_matches(':').to_uppercase())
            .size(10.5)
            .strong()
            .color(theme::dim_text(ui)),
    );
    ui.add_space(2.0);
}

/// A file-system path shown in an inset box, shortened in the middle if it is long.
pub fn path_box(ui: &mut Ui, path: &str) {
    let max_chars = 64;
    let count = path.chars().count();
    let shown = if count > max_chars {
        let head: String = path.chars().take(24).collect();
        let tail: String = path.chars().skip(count - (max_chars - 25)).collect();
        format!("{head}…{tail}")
    } else {
        path.to_owned()
    };
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(shown).monospace().size(11.5)).selectable(true))
                .on_hover_text(path);
        });
}
