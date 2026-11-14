use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};

use crate::app::{Tool, ViewerApp};
use crate::imaging::clut::ColourTableBook;
use crate::imaging::convolution::FACTORY_CONVOLUTIONS;
use crate::imaging::Window;
use crate::navigation::{SliceSynchronization, ViewportTiling};
use crate::volume::{MprPlane3D, SlabMode};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarGroup {
    DatabaseOpen,
    Tiling,
    MouseTools,
    RoiTools,
    Presentation,
    ThreeDAndView,
    Cine,
    App,
}

const TOOLBAR_GROUPS: [ToolbarGroup; 8] = [
    ToolbarGroup::DatabaseOpen,
    ToolbarGroup::Tiling,
    ToolbarGroup::MouseTools,
    ToolbarGroup::RoiTools,
    ToolbarGroup::Presentation,
    ToolbarGroup::ThreeDAndView,
    ToolbarGroup::Cine,
    ToolbarGroup::App,
];

impl ToolbarGroup {
    fn index(self) -> usize {
        match self {
            Self::DatabaseOpen => 0,
            Self::Tiling => 1,
            Self::MouseTools => 2,
            Self::RoiTools => 3,
            Self::Presentation => 4,
            Self::ThreeDAndView => 5,
            Self::Cine => 6,
            Self::App => 7,
        }
    }

    fn default_width(self) -> f32 {
        match self {
            Self::DatabaseOpen => 104.0,
            Self::Tiling => 66.0,
            Self::MouseTools => 240.0,
            Self::RoiTools => 150.0,
            Self::Presentation => 350.0,
            Self::ThreeDAndView => 210.0,
            Self::Cine => 88.0,
            Self::App => 96.0,
        }
    }
}

impl ViewerApp {
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let widths_id = ui.make_persistent_id("sekhmetx_toolbar_group_widths");
        let mut widths: [f32; 8] = ui
            .ctx()
            .data(|d| d.get_temp(widths_id))
            .unwrap_or_else(|| TOOLBAR_GROUPS.map(|g| g.default_width()));

        let avail_w = ui.available_width().max(120.0);
        let overflow_btn_reserve = 36.0;
        let sep_w = 10.0;

        let mut visible_count = 0usize;
        let mut used_w = 0.0f32;
        for (i, group) in TOOLBAR_GROUPS.iter().enumerate() {
            let gw = widths[group.index()];
            let need_sep = if i > 0 { sep_w } else { 0.0 };
            let remaining_after = TOOLBAR_GROUPS.len() - (i + 1);
            let reserve = if remaining_after > 0 {
                overflow_btn_reserve
            } else {
                0.0
            };
            if i == 0 || used_w + need_sep + gw + reserve <= avail_w {
                used_w += need_sep + gw;
                visible_count += 1;
            } else {
                break;
            }
        }

        // Row 1: Horos-ordered toolbar with automatic "»" overflow menu.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, &group) in TOOLBAR_GROUPS[..visible_count].iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                let resp = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    self.render_toolbar_group(ui, group);
                });
                widths[group.index()] = resp.response.rect.width().max(24.0);
            }

            if visible_count < TOOLBAR_GROUPS.len() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("»", |ui| {
                        ui.set_min_width(220.0);
                        for (j, &group) in TOOLBAR_GROUPS[visible_count..].iter().enumerate() {
                            if j > 0 {
                                ui.separator();
                            }
                            ui.horizontal_wrapped(|ui| {
                                self.render_toolbar_group(ui, group);
                            });
                        }
                    })
                    .response
                    .on_hover_text("More toolbar items");
                });
            }
        });

        ui.ctx().data_mut(|d| d.insert_temp(widths_id, widths));

        // Row 2: Slim Slice Scrubber & Contextual MPR Strip
        self.render_scrubber_and_mpr_strip(ui);
    }

    fn render_toolbar_group(&mut self, ui: &mut egui::Ui, group: ToolbarGroup) {
        let tr = crate::i18n::tr(self.lang);
        match group {
            ToolbarGroup::DatabaseOpen => {
                ui.toggle_value(&mut self.show_browser, "◧")
                    .on_hover_text(tr.browser_hover);
                ui.menu_button(format!("📂 {}", tr.open), |ui| {
                    let (lbl_folder, lbl_files) = match self.lang {
                        crate::i18n::Lang::En => ("Open Folder…", "Open Files / ZIP…"),
                        crate::i18n::Lang::Es => ("Abrir Carpeta…", "Abrir Archivos / ZIP…"),
                    };
                    if ui.button(lbl_folder).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.start_scan(vec![dir]);
                        }
                        ui.close();
                    }
                    if ui.button(lbl_files).clicked() {
                        if let Some(files) = rfd::FileDialog::new().pick_files() {
                            self.start_scan(files);
                        }
                        ui.close();
                    }
                })
                .response
                .on_hover_text(tr.open_hover);
            }
            ToolbarGroup::Tiling => {
                self.render_tiling_menu(ui);
            }
            ToolbarGroup::MouseTools => {
                let prev_tool = self.tool;
                ui.selectable_value(&mut self.tool, Tool::WindowLevel, tr.wl)
                    .on_hover_text("Window / Level (W)");
                ui.selectable_value(&mut self.tool, Tool::Scroll, tr.scroll)
                    .on_hover_text("Stack scroll (S)");
                ui.selectable_value(&mut self.tool, Tool::Pan, tr.pan)
                    .on_hover_text("Pan (Middle mouse button)");
                ui.selectable_value(&mut self.tool, Tool::Zoom, tr.zoom)
                    .on_hover_text("Zoom (Right mouse button / Ctrl+wheel) (Z)");
                ui.selectable_value(&mut self.tool, Tool::Rotate, tr.rot)
                    .on_hover_text("In-plane 2D rotation");
                ui.selectable_value(&mut self.tool, Tool::Orbit3D, tr.orbit_3d)
                    .on_hover_text("3D Trackball orbit rotation (O)");
                if self.tool != prev_tool {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
            }
            ToolbarGroup::RoiTools => {
                let prev_tool = self.tool;
                ui.selectable_value(&mut self.tool, Tool::Length, "📏")
                    .on_hover_text("Length (mm) (L / M)");
                ui.selectable_value(&mut self.tool, Tool::Oval, "◯")
                    .on_hover_text("Oval / Ellipse ROI (E)");
                ui.selectable_value(&mut self.tool, Tool::Rect, "▭")
                    .on_hover_text("Rectangle ROI (B)");
                let roi_label = match self.tool {
                    Tool::Angle => format!("{}: ∠", tr.roi),
                    Tool::Polygon => format!("{}: ⬠", tr.roi),
                    Tool::Pencil => format!("{}: ✏", tr.roi),
                    Tool::Point => format!("{}: ⌖", tr.roi),
                    _ => format!("{} ▾", tr.roi),
                };
                ui.menu_button(roi_label, |ui| {
                    ui.selectable_value(&mut self.tool, Tool::Length, "📏 Length (L)");
                    ui.selectable_value(&mut self.tool, Tool::Angle, "∠ Angle (A)");
                    ui.selectable_value(&mut self.tool, Tool::Oval, "◯ Oval ROI (E)");
                    ui.selectable_value(&mut self.tool, Tool::Rect, "▭ Rect ROI (B)");
                    ui.selectable_value(&mut self.tool, Tool::Polygon, "⬠ Polygon ROI (G)");
                    ui.selectable_value(&mut self.tool, Tool::Pencil, "✏ Pencil ROI (F)");
                    ui.selectable_value(&mut self.tool, Tool::Point, "⌖ Point Probe (T)");
                    if self
                        .viewport_measure_key(self.active_viewport)
                        .and_then(|k| self.measures.get(&k))
                        .is_some_and(|m| !m.is_empty())
                    {
                        ui.separator();
                        if ui.button(format!("🗑 {}", tr.clear_rois)).clicked() {
                            if let Some(k) = self.viewport_measure_key(self.active_viewport) {
                                self.measures.remove(&k);
                                self.persist_measures_for(&k);
                            }
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Measurement & ROI tools");
                if self.tool != prev_tool {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
            }
            ToolbarGroup::Presentation => {
                // WL/WW Presets ComboBox
                let active_win = self.active_cell().window;
                let mut chosen_win: Option<Option<Window>> = None;
                egui::ComboBox::from_id_salt("presets")
                    .selected_text(self.preset_label())
                    .width(106.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(active_win.is_none(), "0   Default")
                            .clicked()
                        {
                            chosen_win = Some(None);
                        }
                        if let Some(f) = self.viewport_frame(self.active_viewport) {
                            let full = f.full_window;
                            if ui
                                .selectable_label(
                                    active_win == Some(full),
                                    format!(
                                        "~   Full dynamic   L {:.0} W {:.0}",
                                        full.level, full.width
                                    ),
                                )
                                .clicked()
                            {
                                chosen_win = Some(Some(full));
                            }
                        }
                        ui.separator();
                        for (i, entry) in self.presets.hot_key_order().into_iter().enumerate() {
                            let Some(w) = entry.window else { continue };
                            let key = if i < 9 {
                                format!("{}", i + 1)
                            } else {
                                " ".into()
                            };
                            let text =
                                format!("{key}   {}   L {} W {}", entry.name, w.level, w.width);
                            if ui.selectable_label(active_win == Some(w), text).clicked() {
                                chosen_win = Some(Some(w));
                            }
                        }
                    });
                if let Some(w) = chosen_win {
                    self.set_active_window(w);
                }

                // CLUT Selector ComboBox
                let current_clut_name = self.active_cell().clut_name.clone();
                let mut chosen_clut: Option<String> = None;
                egui::ComboBox::from_id_salt("clut_selector")
                    .selected_text(format!("CLUT: {current_clut_name}"))
                    .width(104.0)
                    .show_ui(ui, |ui| {
                        for name in ColourTableBook::menu_names() {
                            if ui
                                .selectable_label(current_clut_name == name, name)
                                .clicked()
                            {
                                chosen_clut = Some(name.to_string());
                            }
                        }
                    });
                if let Some(name) = chosen_clut {
                    self.active_cell_mut().clut_name = name;
                }

                // 2D Spatial Convolution Filter Selector
                let cur_conv = self.active_cell().convolution_idx;
                let conv_label = cur_conv
                    .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
                    .map(|k| k.name)
                    .unwrap_or("None");
                let mut chosen_conv: Option<Option<usize>> = None;
                egui::ComboBox::from_id_salt("conv_selector")
                    .selected_text(format!("Filter: {conv_label}"))
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(cur_conv.is_none(), "No Filter")
                            .clicked()
                        {
                            chosen_conv = Some(None);
                        }
                        ui.separator();
                        for (idx, k) in FACTORY_CONVOLUTIONS.iter().enumerate() {
                            if ui
                                .selectable_label(cur_conv == Some(idx), k.name)
                                .clicked()
                            {
                                chosen_conv = Some(Some(idx));
                            }
                        }
                    });
                if let Some(next_conv) = chosen_conv {
                    let vp = self.active_cell_mut();
                    vp.convolution_idx = next_conv;
                    vp.conv_cached = None;
                }
            }
            ToolbarGroup::ThreeDAndView => {
                let has_series = self.active_cell().selected.is_some();
                ui.add_enabled_ui(has_series || self.ortho_mpr_active, |ui| {
                    let mut ortho_on = self.ortho_mpr_active;
                    if ui
                        .toggle_value(&mut ortho_on, format!("⊞ {}", tr.ortho_mpr))
                        .on_hover_text("Classic Horos 3-pane Orthogonal MPR (Axial | Coronal | Sagittal) with interactive crosshairs")
                        .changed()
                    {
                        if ortho_on {
                            self.request_mpr_mode(self.active_viewport, true);
                        } else if self.ortho_mpr_active {
                            self.toggle_orthogonal_mpr();
                        }
                    }
                });
                ui.add_enabled_ui(has_series, |ui| {
                    let mut mpr_on = self.active_cell().mpr_enabled;
                    if ui
                        .toggle_value(&mut mpr_on, format!("🧊 {}", tr.mpr_3d))
                        .on_hover_text("In-place 3D volume slicing inside active viewport")
                        .changed()
                    {
                        let vp_idx = self.active_viewport;
                        if mpr_on {
                            self.request_mpr_mode(vp_idx, false);
                        } else {
                            self.disable_viewport_mpr(vp_idx);
                            if self.pending_mpr_activate.is_some_and(|(v, _, o)| v == vp_idx && !o)
                            {
                                self.pending_mpr_activate = None;
                            }
                        }
                    }
                });

                ui.menu_button(format!("{} ▾", tr.view), |ui| {
                    ui.toggle_value(&mut self.active_cell_mut().invert, tr.invert);
                    ui.separator();
                    if ui.button("↺ Rotate 90° CCW").clicked() {
                        let vp = self.active_cell_mut();
                        vp.rotation = (vp.rotation - 90.0).rem_euclid(360.0);
                        ui.close();
                    }
                    if ui.button("↻ Rotate 90° CW").clicked() {
                        let vp = self.active_cell_mut();
                        vp.rotation = (vp.rotation + 90.0).rem_euclid(360.0);
                        ui.close();
                    }
                    let mut x_flip = self.active_cell().x_flipped;
                    if ui.toggle_value(&mut x_flip, format!("⇄ {}", tr.flip_h)).changed() {
                        self.active_cell_mut().x_flipped = x_flip;
                    }
                    let mut y_flip = self.active_cell().y_flipped;
                    if ui.toggle_value(&mut y_flip, format!("⇅ {}", tr.flip_v)).changed() {
                        self.active_cell_mut().y_flipped = y_flip;
                    }
                    ui.separator();
                    ui.toggle_value(&mut self.show_reference_lines, tr.ref_lines);
                    ui.toggle_value(&mut self.show_annotations, tr.overlays);
                    if ui
                        .toggle_value(&mut self.gpu_bilinear, "Smooth Interpolation (Bilinear)")
                        .changed()
                    {
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting(
                                "gpu_bilinear",
                                if self.gpu_bilinear { "1" } else { "0" },
                            );
                        }
                    }
                    ui.separator();
                    ui.label(tr.sync_label);
                    for mode in [
                        SliceSynchronization::Absolute,
                        SliceSynchronization::Location,
                        SliceSynchronization::Relative,
                        SliceSynchronization::Off,
                    ] {
                        ui.selectable_value(&mut self.sync_mode, mode, mode.label());
                    }
                    ui.separator();
                    ui.menu_button(tr.export_menu, |ui| {
                        if ui.button(tr.export_png).clicked() {
                            self.export_active_png();
                            ui.close();
                        }
                        if ui.button(tr.export_series_png).clicked() {
                            self.export_active_series_png();
                            ui.close();
                        }
                        if ui.button(tr.export_anon_dicom).clicked() {
                            self.export_active_anonymized_dicom();
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui.button(tr.reset_viewport).clicked() {
                        let vp = self.active_cell_mut();
                        vp.reset_view();
                        vp.reset_clut();
                        vp.invert = false;
                        vp.mpr_enabled = false;
                        vp.mpr_plane = None;
                        vp.mpr_cached = None;
                        self.set_active_window(None);
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Orientation, Invert, Reference Lines, HUD, Sync & Reset");
            }
            ToolbarGroup::Cine => {
                let cine_label = if self.playing { "⏸" } else { "▶" };
                ui.toggle_value(&mut self.playing, cine_label)
                    .on_hover_text("Play / pause cine loop (Space / P)");
                ui.menu_button(format!("{} fps", self.fps as i32), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.fps, 1.0..=60.0)
                            .text("fps")
                            .integer(),
                    );
                })
                .response
                .on_hover_text("Cine playback speed");
            }
            ToolbarGroup::App => {
                let lang_btn = match self.lang {
                    crate::i18n::Lang::En => "🌐 EN",
                    crate::i18n::Lang::Es => "🌐 ES",
                };
                if ui
                    .button(lang_btn)
                    .on_hover_text("Switch language (English / Español)")
                    .clicked()
                {
                    self.lang = self.lang.toggle();
                    if let Some(db) = &self.db {
                        let _ = db.set_setting("lang", self.lang.code());
                    }
                }

                let (upd_label, upd_hover) = match &self.update_status {
                    crate::updater::UpdateStatus::Available(info) => (
                        egui::RichText::new(format!("⬆ {}", info.tag_name))
                            .strong()
                            .color(Color32::from_rgb(120, 225, 150)),
                        format!("Update {} available — click to install", info.tag_name),
                    ),
                    crate::updater::UpdateStatus::Checking => (
                        egui::RichText::new("🔄 …"),
                        "Checking GitHub for updates…".to_string(),
                    ),
                    crate::updater::UpdateStatus::Installing => (
                        egui::RichText::new("⬇ …"),
                        "Downloading and installing update…".to_string(),
                    ),
                    crate::updater::UpdateStatus::Installed(tag) => (
                        egui::RichText::new(format!("✅ {tag}"))
                            .color(Color32::from_rgb(120, 225, 150)),
                        "Update installed — click to restart SekhmetX".to_string(),
                    ),
                    _ => (
                        egui::RichText::new("🔄"),
                        format!(
                            "Check for SekhmetX updates (current v{})",
                            env!("CARGO_PKG_VERSION")
                        ),
                    ),
                };
                if ui.button(upd_label).on_hover_text(upd_hover).clicked() {
                    self.show_update_modal = true;
                    if matches!(
                        self.update_status,
                        crate::updater::UpdateStatus::Idle
                            | crate::updater::UpdateStatus::UpToDate(_)
                            | crate::updater::UpdateStatus::Error(_)
                    ) {
                        let ctx = ui.ctx().clone();
                        self.start_update_check(true, &ctx);
                    }
                }
            }
        }
    }

    fn render_tiling_menu(&mut self, ui: &mut egui::Ui) {
        let mut new_tiling = None;
        let cur_rows = self.tiling.rows;
        let cur_cols = self.tiling.columns;
        ui.menu_button(format!("⊞ {}", self.tiling.label()), |ui| {
            ui.set_min_width(132.0);
            let grid_max = 4usize;
            let cell_size = 24.0;
            let gap = 4.0;
            let total_side = grid_max as f32 * cell_size + (grid_max - 1) as f32 * gap;
            let (grid_rect, _) =
                ui.allocate_exact_size(vec2(total_side, total_side), Sense::hover());

            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let clicked = ui.input(|i| i.pointer.primary_clicked());
            let mut hovered_rc: Option<(usize, usize)> = None;

            for r in 0..grid_max {
                for c in 0..grid_max {
                    let min = pos2(
                        grid_rect.min.x + c as f32 * (cell_size + gap),
                        grid_rect.min.y + r as f32 * (cell_size + gap),
                    );
                    let cell_rect = Rect::from_min_size(min, vec2(cell_size, cell_size));
                    if pointer_pos.is_some_and(|p| cell_rect.expand(gap * 0.5).contains(p)) {
                        hovered_rc = Some((r, c));
                    }
                }
            }
            self.tiling_hover = hovered_rc;

            let (active_r, active_c) =
                hovered_rc.unwrap_or((cur_rows.saturating_sub(1), cur_cols.saturating_sub(1)));

            let painter = ui.painter();
            for r in 0..grid_max {
                for c in 0..grid_max {
                    let min = pos2(
                        grid_rect.min.x + c as f32 * (cell_size + gap),
                        grid_rect.min.y + r as f32 * (cell_size + gap),
                    );
                    let cell_rect = Rect::from_min_size(min, vec2(cell_size, cell_size));
                    let in_selection = r <= active_r && c <= active_c;
                    let in_current = r < cur_rows && c < cur_cols;

                    let fill = if in_selection {
                        if hovered_rc.is_some() {
                            Color32::from_rgb(70, 165, 255)
                        } else {
                            Color32::from_rgb(55, 130, 210)
                        }
                    } else if in_current {
                        Color32::from_rgb(38, 62, 92)
                    } else {
                        Color32::from_gray(42)
                    };

                    let stroke = if in_selection {
                        Stroke::new(1.5, Color32::from_rgb(160, 220, 255))
                    } else if in_current {
                        Stroke::new(1.0, Color32::from_rgb(90, 145, 200))
                    } else {
                        Stroke::new(1.0, Color32::from_gray(95))
                    };

                    painter.rect_filled(cell_rect, 3.0, fill);
                    painter.rect_stroke(cell_rect, 3.0, stroke, egui::StrokeKind::Inside);
                }
            }

            ui.add_space(4.0);
            let disp_r = active_r + 1;
            let disp_c = active_c + 1;
            let count = disp_r * disp_c;
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{disp_r} × {disp_c}  ({count} viewport{})",
                        if count == 1 { "" } else { "s" }
                    ))
                    .strong()
                    .color(Color32::from_rgb(150, 210, 255)),
                );
            });

            if clicked {
                if let Some((hr, hc)) = hovered_rc {
                    new_tiling = Some(ViewportTiling::new(hr + 1, hc + 1));
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Viewport grid layout");
        if let Some(t) = new_tiling {
            self.ortho_mpr_active = false;
            self.ortho_crosshair_lps = None;
            self.set_tiling(t);
        }
    }

    fn render_scrubber_and_mpr_strip(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let series_count = self.all_series_pairs().len();
            ui.add_enabled_ui(series_count > 1, |ui| {
                if ui
                    .small_button("◀ Se")
                    .on_hover_text("Previous series ([)")
                    .clicked()
                {
                    self.step_active_series(-1);
                }
                if ui
                    .small_button("Se ▶")
                    .on_hover_text("Next series (])")
                    .clicked()
                {
                    self.step_active_series(1);
                }
            });

            let vp_idx = self.active_viewport;
            let slice_count = self.viewport_slice_count(vp_idx);
            ui.add_enabled_ui(slice_count > 1, |ui| {
                if ui
                    .small_button("⏮")
                    .on_hover_text("First slice (Home)")
                    .clicked()
                {
                    self.set_viewport_slice(vp_idx, 0, true);
                }
                if ui
                    .small_button("◀")
                    .on_hover_text("Previous slice")
                    .clicked()
                {
                    self.step_viewport_slice(vp_idx, -1);
                }
                let max_s = slice_count.max(1);
                let mut cur_1based = (self.viewports[vp_idx].slice + 1).clamp(1, max_s);
                let slider_resp = ui
                    .add(
                        egui::Slider::new(&mut cur_1based, 1..=max_s)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .on_hover_text(format!("Slice {cur_1based} / {slice_count}"));
                if slider_resp.changed() {
                    self.set_viewport_slice(vp_idx, cur_1based as isize - 1, true);
                }
                ui.monospace(format!("{cur_1based}/{slice_count}"));
                if ui.small_button("▶").on_hover_text("Next slice").clicked() {
                    self.step_viewport_slice(vp_idx, 1);
                }
                if ui
                    .small_button("⏭")
                    .on_hover_text("Last slice (End)")
                    .clicked()
                {
                    self.set_viewport_slice(vp_idx, isize::MAX, true);
                }
            });

            if self.active_cell().mpr_enabled {
                ui.separator();
                if ui.small_button("Ax").on_hover_text("Axial plane").clicked() {
                    if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
                        self.set_viewport_mpr_plane(
                            vp_idx,
                            MprPlane3D::axial(vol.center_lps),
                            true,
                        );
                    }
                }
                if ui
                    .small_button("Cor")
                    .on_hover_text("Coronal plane")
                    .clicked()
                {
                    if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
                        self.set_viewport_mpr_plane(
                            vp_idx,
                            MprPlane3D::coronal(vol.center_lps),
                            true,
                        );
                    }
                }
                if ui
                    .small_button("Sag")
                    .on_hover_text("Sagittal plane")
                    .clicked()
                {
                    if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
                        self.set_viewport_mpr_plane(
                            vp_idx,
                            MprPlane3D::sagittal(vol.center_lps),
                            true,
                        );
                    }
                }
                if ui
                    .small_button("Nat")
                    .on_hover_text("Native orientation")
                    .clicked()
                {
                    if let Some(vol) = self.viewport_volume(vp_idx).cloned() {
                        self.set_viewport_mpr_plane(
                            vp_idx,
                            MprPlane3D::from_volume_native(&vol),
                            true,
                        );
                    }
                }
                if let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("mpr_slab_mode")
                        .selected_text(format!("Slab: {}", plane.slab_mode.label()))
                        .width(82.0)
                        .show_ui(ui, |ui| {
                            for m in [
                                SlabMode::Thin,
                                SlabMode::Mip,
                                SlabMode::MinIp,
                                SlabMode::Mean,
                            ] {
                                if ui.selectable_value(&mut plane.slab_mode, m, m.label()).changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if plane.slab_mode != SlabMode::Thin {
                        if ui
                            .add(
                                egui::Slider::new(&mut plane.slab_thickness_mm, 1.0..=50.0)
                                    .suffix(" mm")
                                    .fixed_decimals(1),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                    if changed {
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                }
                let sib_count = self.viewport_sibling_volumes(vp_idx).len();
                if sib_count > 0 {
                    if ui
                        .checkbox(&mut self.blend_siblings, "Blend Siblings")
                        .on_hover_text(
                            "Sample neighboring series to fill gaps in oblique MPR (disables ROI stats)",
                        )
                        .changed()
                    {
                        for vp in &mut self.viewports {
                            vp.mpr_cached = None;
                        }
                    }
                    if self.blend_siblings {
                        ui.colored_label(
                            Color32::from_rgb(255, 200, 80),
                            format!(
                                "Blended from {} series (ROI stats disabled)",
                                sib_count + 1
                            ),
                        );
                    }
                }
            }

            if self.tiling.cell_count() > 1 && !self.ortho_mpr_active {
                ui.separator();
                if ui
                    .small_button("⚓ Lock 3D Sync")
                    .on_hover_text(
                        "Lock current slice height & 3D tilt angle between comparison viewports",
                    )
                    .clicked()
                {
                    self.lock_current_3d_alignment();
                }
            }
        });
    }
}
