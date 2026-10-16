//! The viewer window: study/series browser on the left, image viewport in the middle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, ColorImage, FontId, Key, Pos2, Rect, Sense, Stroke, TextureHandle,
    TextureOptions, Vec2, pos2, vec2,
};

use crate::dicom_index::{self, Index, Series, Study};
use crate::frame::Frame;
use crate::loader::{Job, Loader};

/// Window presets: (name, width, level). Keys 1–8.
pub const PRESETS: &[(&str, f64, f64)] = &[
    ("Abdomen", 400.0, 40.0),
    ("Lung", 1500.0, -600.0),
    ("Bone", 2000.0, 400.0),
    ("Brain", 80.0, 40.0),
    ("Mediastinum", 350.0, 50.0),
    ("Liver", 150.0, 60.0),
    ("Stroke", 40.0, 40.0),
    ("Angio", 600.0, 150.0),
];

/// Memory budget for decoded slices of the open series.
const CACHE_BUDGET_BYTES: usize = 1536 * 1024 * 1024;
const PREFETCH_RADIUS: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    WindowLevel,
    Pan,
    Zoom,
    Length,
    Angle,
}

#[derive(Clone)]
enum Measure {
    /// End points in image pixel coordinates.
    Length(Pos2, Pos2),
    /// Vertex in the middle.
    Angle(Pos2, Pos2, Pos2),
}

struct Scan {
    rx: Receiver<Index>,
    done: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
}

#[derive(PartialEq, Clone, Copy)]
struct RenderKey {
    idx: usize,
    center: u64,
    width: u64,
    invert: bool,
}

pub struct ViewerApp {
    index: Option<Index>,
    scan: Option<Scan>,
    message: Option<String>,

    selected: Option<(usize, usize)>,
    slice: usize,

    loader: Loader,
    cache: HashMap<usize, Arc<Frame>>,
    failed: HashMap<usize, String>,
    keep_radius: usize,

    texture: Option<TextureHandle>,
    rendered: Option<RenderKey>,

    /// User-chosen window (center, width). `None` = use each image's default.
    window: Option<(f64, f64)>,
    invert: bool,
    zoom: f32,
    pan: Vec2,
    tool: Tool,

    measures: HashMap<(PathBuf, u32), Vec<Measure>>,
    drawing: Vec<Pos2>,
    angle_pending: Option<(Pos2, Pos2)>,

    playing: bool,
    fps: f32,
    last_tick: f64,
    wheel_accum: f32,
    cursor_info: Option<String>,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Vec<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let ctx = cc.egui_ctx.clone();
        let mut app = Self {
            index: None,
            scan: None,
            message: None,
            selected: None,
            slice: 0,
            loader: Loader::new(move || ctx.request_repaint()),
            cache: HashMap::new(),
            failed: HashMap::new(),
            keep_radius: 200,
            texture: None,
            rendered: None,
            window: None,
            invert: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tool: Tool::WindowLevel,
            measures: HashMap::new(),
            drawing: Vec::new(),
            angle_pending: None,
            playing: false,
            fps: 15.0,
            last_tick: 0.0,
            wheel_accum: 0.0,
            cursor_info: None,
        };
        if !initial.is_empty() {
            app.start_scan(initial);
        }
        app
    }

    // ---------------------------------------------------------------- state

    fn start_scan(&mut self, paths: Vec<PathBuf>) {
        let done = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let (d, t) = (done.clone(), total.clone());
        std::thread::spawn(move || {
            let index = dicom_index::scan(&paths, &d, &t);
            let _ = tx.send(index);
        });
        self.message = None;
        self.scan = Some(Scan { rx, done, total });
    }

    fn current_study(&self) -> Option<&Study> {
        let (st, _) = self.selected?;
        self.index.as_ref()?.studies.get(st)
    }

    fn current_series(&self) -> Option<&Series> {
        let (st, se) = self.selected?;
        self.index.as_ref()?.studies.get(st)?.series.get(se)
    }

    fn slice_count(&self) -> usize {
        self.current_series().map_or(0, |s| s.slices.len())
    }

    fn select_series(&mut self, sel: (usize, usize)) {
        if self.selected == Some(sel) {
            return;
        }
        self.selected = Some(sel);
        self.loader.reset();
        self.cache.clear();
        self.failed.clear();
        self.rendered = None;
        self.window = None;
        self.invert = false;
        self.playing = false;
        self.drawing.clear();
        self.angle_pending = None;
        self.reset_view();
        self.slice = 0;
    }

    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    fn set_slice(&mut self, idx: isize) {
        let n = self.slice_count();
        if n == 0 {
            return;
        }
        self.slice = idx.clamp(0, n as isize - 1) as usize;
        self.drawing.clear();
        self.angle_pending = None;
    }

    fn step_slice(&mut self, delta: isize) {
        self.set_slice(self.slice as isize + delta);
    }

    fn current_frame(&self) -> Option<&Arc<Frame>> {
        self.cache.get(&self.slice)
    }

    fn effective_window(&self, f: &Frame) -> (f64, f64) {
        self.window.unwrap_or_else(|| f.default_window())
    }

    fn current_key(&self) -> Option<(PathBuf, u32)> {
        let s = self.current_series()?.slices.get(self.slice)?;
        Some((s.path.clone(), s.frame))
    }

    // ------------------------------------------------------- background work

    fn poll_background(&mut self, ctx: &egui::Context) {
        if let Some(scan) = &self.scan {
            match scan.rx.try_recv() {
                Ok(index) => {
                    self.scan = None;
                    if index.studies.is_empty() {
                        self.message = Some(format!(
                            "No DICOM images found ({} files checked).",
                            index.files_seen
                        ));
                    }
                    let first = (!index.studies.is_empty() && !index.studies[0].series.is_empty())
                        .then_some((0, 0));
                    self.index = Some(index);
                    self.selected = None;
                    if let Some(sel) = first {
                        self.select_series(sel);
                    }
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }

        for (idx, result) in self.loader.poll() {
            match result {
                Ok(frame) => {
                    if self.cache.is_empty() {
                        let per = frame.bytes().max(1);
                        self.keep_radius = (CACHE_BUDGET_BYTES / per / 2).clamp(10, 5000);
                    }
                    self.cache.insert(idx, frame);
                }
                Err(e) => {
                    self.failed.insert(idx, e);
                }
            }
        }

        // Keep memory bounded: drop slices far from the current one.
        let (slice, keep) = (self.slice, self.keep_radius);
        self.cache.retain(|&i, _| i.abs_diff(slice) <= keep);

        // Queue the current slice and its neighbours, nearest first.
        let radius = PREFETCH_RADIUS.min(self.keep_radius);
        self.loader.set_focus(self.slice, radius);
        let jobs: Vec<Job> = match self.current_series() {
            Some(series) => {
                let n = series.slices.len();
                let mut jobs = Vec::new();
                for d in 0..=radius {
                    for idx in [self.slice.checked_sub(d), Some(self.slice + d)]
                        .into_iter()
                        .flatten()
                    {
                        if idx >= n
                            || (d == 0 && jobs.iter().any(|j: &Job| j.idx == idx))
                            || self.cache.contains_key(&idx)
                            || self.failed.contains_key(&idx)
                            || self.loader.is_pending(idx)
                        {
                            continue;
                        }
                        let s = &series.slices[idx];
                        jobs.push(Job {
                            idx,
                            path: s.path.clone(),
                            frame: s.frame,
                        });
                    }
                }
                jobs
            }
            None => Vec::new(),
        };
        for job in jobs {
            self.loader.request(job);
        }

        // Cine playback.
        if self.playing && self.slice_count() > 1 {
            let now = ctx.input(|i| i.time);
            if now - self.last_tick >= 1.0 / self.fps as f64 {
                let next = (self.slice + 1) % self.slice_count();
                if self.cache.contains_key(&next) || self.failed.contains_key(&next) {
                    self.slice = next;
                    self.last_tick = now;
                }
            }
            ctx.request_repaint();
        }
    }

    // ------------------------------------------------------------- input

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.start_scan(dropped);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = [
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::PageUp,
            Key::PageDown,
            Key::Home,
            Key::End,
            Key::I,
            Key::R,
            Key::P,
            Key::Space,
            Key::Delete,
            Key::Backspace,
            Key::Escape,
            Key::W,
            Key::M,
            Key::A,
            Key::Z,
            Key::Num0,
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
        ];
        let pressed: Vec<Key> = ctx.input(|i| keys.iter().copied().filter(|k| i.key_pressed(*k)).collect());
        for key in pressed {
            match key {
                Key::ArrowUp | Key::ArrowLeft => self.step_slice(-1),
                Key::ArrowDown | Key::ArrowRight => self.step_slice(1),
                Key::PageUp => self.step_slice(-10),
                Key::PageDown => self.step_slice(10),
                Key::Home => self.set_slice(0),
                Key::End => self.set_slice(isize::MAX),
                Key::I => self.invert = !self.invert,
                Key::R => {
                    self.reset_view();
                    self.window = None;
                }
                Key::P | Key::Space => self.playing = !self.playing,
                Key::Delete | Key::Backspace => {
                    if let Some(k) = self.current_key() {
                        if let Some(list) = self.measures.get_mut(&k) {
                            list.pop();
                        }
                    }
                }
                Key::Escape => {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
                Key::W => self.tool = Tool::WindowLevel,
                Key::M => self.tool = Tool::Length,
                Key::A => self.tool = Tool::Angle,
                Key::Z => self.tool = Tool::Zoom,
                Key::Num0 => self.window = None,
                k => {
                    let n = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8]
                        .iter()
                        .position(|x| *x == k);
                    if let Some((_, w, l)) = n.and_then(|n| PRESETS.get(n)) {
                        self.window = Some((*l, *w));
                    }
                }
            }
        }
    }

    // ----------------------------------------------------------------- UI

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open folder…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.start_scan(vec![dir]);
                }
            }
            if ui.button("Open files…").clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.start_scan(files);
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.tool, Tool::WindowLevel, "W/L")
                .on_hover_text("Drag: left/right = width, up/down = level  (W)");
            ui.selectable_value(&mut self.tool, Tool::Pan, "Pan")
                .on_hover_text("Middle mouse button also pans");
            ui.selectable_value(&mut self.tool, Tool::Zoom, "Zoom")
                .on_hover_text("Right mouse button or Ctrl+wheel also zoom  (Z)");
            ui.selectable_value(&mut self.tool, Tool::Length, "Length")
                .on_hover_text("Drag to measure, in mm when pixel spacing is known  (M)");
            ui.selectable_value(&mut self.tool, Tool::Angle, "Angle")
                .on_hover_text("Drag the first arm, then click the end of the second arm  (A)");
            ui.separator();

            egui::ComboBox::from_id_salt("presets")
                .selected_text("Window presets")
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.window.is_none(), "0  Default (from image)").clicked() {
                        self.window = None;
                    }
                    for (i, (name, w, l)) in PRESETS.iter().enumerate() {
                        let text = format!("{}  {name}   W {w}  L {l}", i + 1);
                        if ui.selectable_label(self.window == Some((*l, *w)), text).clicked() {
                            self.window = Some((*l, *w));
                        }
                    }
                });
            ui.toggle_value(&mut self.invert, "Invert");
            if ui.button("Reset").on_hover_text("Reset zoom, pan and window  (R)").clicked() {
                self.reset_view();
                self.window = None;
            }
            ui.separator();
            let label = if self.playing { "Stop cine" } else { "Play cine" };
            ui.toggle_value(&mut self.playing, label);
            ui.add(egui::Slider::new(&mut self.fps, 1.0..=60.0).text("fps").integer());
            ui.separator();
            ui.menu_button("Shortcuts", |ui| {
                for line in [
                    "Mouse wheel / arrows: previous / next image",
                    "Page Up / Down: jump 10 images; Home / End: first / last",
                    "Left drag: active tool   Middle drag: pan   Right drag: zoom",
                    "Ctrl + wheel: zoom   Double-click: reset view",
                    "1–8: window presets   0: image default   I: invert",
                    "W / M / A / Z: W/L, length, angle, zoom tool",
                    "Delete: remove last measurement on this image   Esc: cancel",
                    "P or Space: play / stop cine   R: reset",
                    "Drop folders or files onto the window to open them",
                ] {
                    ui.label(line);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(scan) = &self.scan {
                let done = scan.done.load(Ordering::Relaxed);
                let total = scan.total.load(Ordering::Relaxed);
                ui.spinner();
                if total == 0 {
                    ui.label("Looking for files…");
                } else {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / total as f32)
                            .desired_width(200.0)
                            .text(format!("Indexing {done} / {total} files")),
                    );
                }
            } else if let Some(index) = &self.index {
                ui.label(format!(
                    "{} studies · {} images · {} other files skipped",
                    index.studies.len(),
                    index.image_count(),
                    index.files_skipped
                ));
            }
            if let Some(msg) = &self.message {
                ui.separator();
                ui.colored_label(Color32::from_rgb(255, 180, 80), msg);
            }
            if let Some(info) = &self.cursor_info {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(info);
                });
            }
        });
    }

    fn browser(&mut self, ui: &mut egui::Ui) {
        let Some(index) = &self.index else {
            ui.add_space(8.0);
            ui.label("No studies loaded.");
            ui.label("Use “Open folder…” or drop a folder onto the window.");
            return;
        };
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for (si, study) in index.studies.iter().enumerate() {
                let title = format!("{}   {}", study.patient_name, study.date);
                egui::CollapsingHeader::new(egui::RichText::new(title).strong())
                    .id_salt(&study.uid)
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut sub = Vec::new();
                        if !study.description.is_empty() {
                            sub.push(study.description.clone());
                        }
                        if !study.patient_id.is_empty() {
                            sub.push(format!("ID {}", study.patient_id));
                        }
                        if !study.accession.is_empty() {
                            sub.push(format!("Acc {}", study.accession));
                        }
                        if !sub.is_empty() {
                            ui.small(sub.join(" · "));
                        }
                        for (ri, series) in study.series.iter().enumerate() {
                            let desc = if series.description.is_empty() {
                                "(no description)"
                            } else {
                                &series.description
                            };
                            let text = format!(
                                "{:<3} {:>3}  {}  ({})",
                                series.modality,
                                series.number,
                                desc,
                                series.slices.len()
                            );
                            let selected = self.selected == Some((si, ri));
                            if ui
                                .selectable_label(selected, egui::RichText::new(text).monospace())
                                .on_hover_text(format!(
                                    "{} images, {}×{}",
                                    series.slices.len(),
                                    series.columns,
                                    series.rows
                                ))
                                .clicked()
                            {
                                clicked = Some((si, ri));
                            }
                        }
                    });
            }
        });
        if let Some(sel) = clicked {
            self.select_series(sel);
        }
    }

    // ------------------------------------------------------------ viewport

    fn image_rect(&self, view: Rect, f: &Frame) -> Rect {
        let (rs, cs) = f.spacing.unwrap_or((1.0, 1.0));
        let m = rs.min(cs);
        let w = f.width as f32 * (cs / m) as f32;
        let h = f.height as f32 * (rs / m) as f32;
        let fit = (view.width() / w).min(view.height() / h);
        Rect::from_center_size(view.center() + self.pan, vec2(w, h) * fit * self.zoom)
    }

    fn to_pixel(img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        pos2(
            (p.x - img.min.x) / img.width() * f.width as f32,
            (p.y - img.min.y) / img.height() * f.height as f32,
        )
    }

    fn to_screen(img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        pos2(
            img.min.x + p.x / f.width as f32 * img.width(),
            img.min.y + p.y / f.height as f32 * img.height(),
        )
    }

    fn length_label(f: &Frame, a: Pos2, b: Pos2) -> String {
        let (dx, dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
        match f.spacing {
            Some((rs, cs)) => format!("{:.1} mm", ((dx * cs).powi(2) + (dy * rs).powi(2)).sqrt()),
            None => format!("{:.1} px", (dx * dx + dy * dy).sqrt()),
        }
    }

    fn angle_label(f: &Frame, a: Pos2, v: Pos2, b: Pos2) -> String {
        let (rs, cs) = f.spacing.unwrap_or((1.0, 1.0));
        let u = (((a.x - v.x) as f64) * cs, ((a.y - v.y) as f64) * rs);
        let w = (((b.x - v.x) as f64) * cs, ((b.y - v.y) as f64) * rs);
        let ang = (u.0 * w.1 - u.1 * w.0).atan2(u.0 * w.0 + u.1 * w.1).abs().to_degrees();
        format!("{ang:.1}°")
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let view = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(view, Sense::click_and_drag());
        let painter = ui.painter_at(view);
        painter.rect_filled(view, 0.0, Color32::BLACK);

        if self.current_series().is_none() {
            let text = if self.scan.is_some() {
                "Indexing…"
            } else {
                "Open a folder of DICOM files, or drop one here"
            };
            painter.text(view.center(), Align2::CENTER_CENTER, text, FontId::proportional(18.0), Color32::GRAY);
            return;
        }

        self.handle_wheel(ui, &response);

        let Some(frame) = self.current_frame().cloned() else {
            let text = match self.failed.get(&self.slice) {
                Some(err) => format!("Could not display this image:\n{err}"),
                None => "Loading…".to_string(),
            };
            // Keep showing the previous image while the next one decodes.
            if let (Some(tex), Some(prev)) = (&self.texture, self.rendered) {
                if let Some(pf) = self.cache.get(&prev.idx) {
                    let r = self.image_rect(view, pf);
                    painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_gray(160));
                }
            }
            painter.text(view.center(), Align2::CENTER_CENTER, text, FontId::proportional(16.0), Color32::LIGHT_GRAY);
            self.draw_overlay(&painter, view, None);
            return;
        };

        self.handle_drag(&response, view, &frame);

        // Re-render the texture only when something visible changed.
        let window = self.effective_window(&frame);
        let key = RenderKey {
            idx: self.slice,
            center: window.0.to_bits(),
            width: window.1.to_bits(),
            invert: self.invert,
        };
        if self.rendered != Some(key) || self.texture.is_none() {
            let rgba = frame.render(window, self.invert);
            let image = ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &rgba);
            match &mut self.texture {
                Some(t) => t.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ui.ctx().load_texture("slice", image, TextureOptions::LINEAR)),
            }
            self.rendered = Some(key);
        }

        let img = self.image_rect(view, &frame);
        if let Some(tex) = &self.texture {
            painter.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }

        // Cursor readout.
        self.cursor_info = response.hover_pos().and_then(|p| {
            let px = Self::to_pixel(img, &frame, p);
            if px.x < 0.0 || px.y < 0.0 {
                return None;
            }
            let (x, y) = (px.x as u32, px.y as u32);
            frame.value_at(x, y).map(|v| format!("x {x}  y {y}   {v}"))
        });

        self.draw_measures(&painter, img, &frame);
        self.draw_overlay(&painter, view, Some(&frame));
    }

    fn handle_wheel(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = event {
                let lines = match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 40.0,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                };
                if modifiers.ctrl || modifiers.command {
                    self.zoom = (self.zoom * (1.0 + 0.1 * lines)).clamp(0.05, 50.0);
                } else {
                    self.wheel_accum += lines;
                }
            }
        }
        let steps = self.wheel_accum.trunc();
        if steps != 0.0 {
            self.wheel_accum -= steps;
            // Scrolling down moves to the next image.
            self.step_slice(-steps as isize);
        }
    }

    fn handle_drag(&mut self, response: &egui::Response, view: Rect, frame: &Frame) {
        if response.double_clicked() {
            self.reset_view();
            return;
        }
        let delta = response.drag_delta();
        let img = self.image_rect(view, frame);
        let pointer_px = response
            .interact_pointer_pos()
            .map(|p| Self::to_pixel(img, frame, p));

        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan += delta;
            return;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.zoom = (self.zoom * (-delta.y / 200.0).exp()).clamp(0.05, 50.0);
            return;
        }

        // Second click of the angle tool.
        if self.tool == Tool::Angle {
            if let (Some((a, v)), true) = (self.angle_pending, response.clicked()) {
                if let (Some(b), Some(k)) = (pointer_px, self.current_key()) {
                    self.measures.entry(k).or_default().push(Measure::Angle(a, v, b));
                }
                self.angle_pending = None;
                return;
            }
        }

        if !response.dragged_by(egui::PointerButton::Primary) && !response.drag_stopped() {
            return;
        }

        match self.tool {
            Tool::WindowLevel => {
                if frame.is_gray() && delta != Vec2::ZERO {
                    let (c, w) = self.effective_window(frame);
                    let step = (w.abs().max(20.0) / 250.0) as f64;
                    let w = (w + delta.x as f64 * step).max(1.0);
                    let c = c + delta.y as f64 * step;
                    self.window = Some((c, w));
                }
            }
            Tool::Pan => self.pan += delta,
            Tool::Zoom => self.zoom = (self.zoom * (-delta.y / 200.0).exp()).clamp(0.05, 50.0),
            Tool::Length | Tool::Angle => {
                if response.drag_started() {
                    self.drawing.clear();
                    self.angle_pending = None;
                    // Anchor at the press point, not where the drag was recognised.
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(p) = origin.or(response.interact_pointer_pos()) {
                        self.drawing.push(Self::to_pixel(img, frame, p));
                    }
                }
                if let Some(p) = pointer_px {
                    if self.drawing.len() == 1 {
                        self.drawing.push(p);
                    } else if self.drawing.len() == 2 {
                        self.drawing[1] = p;
                    }
                }
                if response.drag_stopped() && self.drawing.len() == 2 {
                    let (a, b) = (self.drawing[0], self.drawing[1]);
                    self.drawing.clear();
                    if a.distance(b) > 1.0 {
                        if self.tool == Tool::Length {
                            if let Some(k) = self.current_key() {
                                self.measures.entry(k).or_default().push(Measure::Length(a, b));
                            }
                        } else {
                            // First arm drawn from the far end (a) to the vertex (b).
                            self.angle_pending = Some((a, b));
                        }
                    }
                }
            }
        }
    }

    fn draw_measures(&self, painter: &egui::Painter, img: Rect, f: &Frame) {
        let color = Color32::from_rgb(255, 220, 60);
        let stroke = Stroke::new(1.5, color);
        let s = |p: Pos2| Self::to_screen(img, f, p);
        let label = |at: Pos2, text: String| {
            let pos = at + vec2(8.0, -8.0);
            painter.text(pos + vec2(1.0, 1.0), Align2::LEFT_BOTTOM, &text, FontId::proportional(14.0), Color32::BLACK);
            painter.text(pos, Align2::LEFT_BOTTOM, text, FontId::proportional(14.0), color);
        };

        let mut items: Vec<Measure> = self
            .current_key()
            .and_then(|k| self.measures.get(&k).cloned())
            .unwrap_or_default();
        if self.drawing.len() == 2 {
            items.push(Measure::Length(self.drawing[0], self.drawing[1]));
        }
        for m in &items {
            match *m {
                Measure::Length(a, b) => {
                    painter.line_segment([s(a), s(b)], stroke);
                    painter.circle_filled(s(a), 2.5, color);
                    painter.circle_filled(s(b), 2.5, color);
                    label(s(b), Self::length_label(f, a, b));
                }
                Measure::Angle(a, v, b) => {
                    painter.line_segment([s(a), s(v)], stroke);
                    painter.line_segment([s(v), s(b)], stroke);
                    label(s(v), Self::angle_label(f, a, v, b));
                }
            }
        }
        if let Some((a, v)) = self.angle_pending {
            painter.line_segment([s(a), s(v)], stroke);
            painter.circle_filled(s(v), 3.0, color);
        }
    }

    fn draw_overlay(&self, painter: &egui::Painter, view: Rect, frame: Option<&Frame>) {
        let font = FontId::proportional(14.0);
        let pad = 10.0;
        let text = |pos: Pos2, align: Align2, s: String| {
            painter.text(pos + vec2(1.0, 1.0), align, &s, font.clone(), Color32::BLACK);
            painter.text(pos, align, s, font.clone(), Color32::from_gray(230));
        };

        if let Some(study) = self.current_study() {
            let mut tl = vec![study.patient_name.clone()];
            if !study.patient_id.is_empty() {
                tl.push(format!("ID: {}", study.patient_id));
            }
            if !study.birth_date.is_empty() {
                tl.push(format!("DOB: {}", study.birth_date));
            }
            text(view.left_top() + vec2(pad, pad), Align2::LEFT_TOP, tl.join("\n"));

            let mut tr = Vec::new();
            if !study.description.is_empty() {
                tr.push(study.description.clone());
            }
            if !study.date.is_empty() {
                tr.push(study.date.clone());
            }
            if let Some(series) = self.current_series() {
                tr.push(format!("{} · Se {} · {}", series.modality, series.number, series.description));
            }
            text(view.right_top() + vec2(-pad, pad), Align2::RIGHT_TOP, tr.join("\n"));
        }

        let mut bl = vec![format!("Im: {} / {}", self.slice + 1, self.slice_count())];
        if let Some(f) = frame {
            if let Some(loc) = f.slice_location {
                bl.push(format!("Loc: {loc:.1} mm"));
            }
            if let Some(t) = f.slice_thickness {
                bl.push(format!("Thk: {t:.1} mm"));
            }
            bl.push(format!("{} × {}", f.width, f.height));
        }
        text(view.left_bottom() + vec2(pad, -pad), Align2::LEFT_BOTTOM, bl.join("\n"));

        let mut br = Vec::new();
        if let Some(f) = frame {
            if f.is_gray() {
                let (c, w) = self.effective_window(f);
                br.push(format!("WL: {c:.0}  WW: {w:.0}"));
            }
        }
        br.push(format!("Zoom: {:.0}%", self.zoom * 100.0));
        if self.invert {
            br.push("Inverted".into());
        }
        text(view.right_bottom() + vec2(-pad, -pad), Align2::RIGHT_BOTTOM, br.join("\n"));
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_background(&ctx);
        self.handle_dropped_files(&ctx);
        self.handle_keys(&ctx);

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("browser")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| self.browser(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport(ui));
    }
}
