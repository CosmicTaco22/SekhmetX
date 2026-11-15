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
            Self::DatabaseOpen => 196.0,
            Self::Tiling => 64.0,
            Self::MouseTools => 228.0,
            Self::RoiTools => 148.0,
            Self::Presentation => 286.0,
            Self::ThreeDAndView => 268.0,
            Self::Cine => 88.0,
            Self::App => 96.0,
        }
    }

    fn caption(self, tr: &crate::i18n::Strings) -> &'static str {
        match self {
            Self::DatabaseOpen => tr.grp_database,
            Self::Tiling => tr.grp_windows,
            Self::MouseTools => tr.grp_mouse,
            Self::RoiTools => tr.grp_roi,
            Self::Presentation => tr.grp_wl_clut,
            Self::ThreeDAndView => tr.grp_view_3d,
            Self::Cine => tr.grp_cine,
            Self::App => tr.grp_app,
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

        // Row 1: OsiriX-style grouped toolbar with category captions and automatic "»" overflow menu.
        let tr = crate::i18n::tr(self.lang);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, &group) in TOOLBAR_GROUPS[..visible_count].iter().enumerate() {
                if i > 0 {
                    let (sep_rect, _) = ui.allocate_exact_size(vec2(6.0, 48.0), Sense::hover());
                    ui.painter().vline(
                        sep_rect.center().x,
                        sep_rect.y_range().shrink(4.0),
                        Stroke::new(1.0, Color32::from_gray(60)),
                    );
                }
                let caption = group.caption(tr);
                let resp = ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let row_resp = ui.horizontal(|ui| {
                        ui.set_min_height(36.0);
                        ui.spacing_mut().item_spacing.x = 3.0;
                        self.render_toolbar_group(ui, group);
                    });
                    let group_w = row_resp.response.rect.width().max(48.0);
                    ui.allocate_ui_with_layout(
                        vec2(group_w, 12.0),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new(caption)
                                    .size(9.5)
                                    .color(Color32::from_gray(145)),
                            );
                        },
                    );
                });
                widths[group.index()] = resp.response.rect.width().clamp(24.0, 420.0);
            }

            if visible_count < TOOLBAR_GROUPS.len() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("»", |ui| {
                        ui.set_min_width(240.0);
                        for (j, &group) in TOOLBAR_GROUPS[visible_count..].iter().enumerate() {
                            if j > 0 {
                                ui.separator();
                            }
                            ui.label(
                                egui::RichText::new(group.caption(tr))
                                    .size(10.0)
                                    .color(Color32::from_gray(155)),
                            );
                            ui.horizontal_wrapped(|ui| {
                                self.render_toolbar_group(ui, group);
                            });
                        }
                    })
                    .response
                    .on_hover_text(tr.more_tools_hover);
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
                let db_lbl = match self.lang {
                    crate::i18n::Lang::En => "🗄 Database",
                    crate::i18n::Lang::Es => "🗄 Base de datos",
                };
                ui.toggle_value(&mut self.show_database_view, db_lbl)
                    .on_hover_text("Toggle full Database Library view");
                if !self.show_database_view {
                    ui.toggle_value(&mut self.show_browser, "◧")
                        .on_hover_text(tr.browser_hover);
                }
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
                    .on_hover_text(tr.wl_hover);
                ui.selectable_value(&mut self.tool, Tool::Scroll, tr.scroll)
                    .on_hover_text(tr.scroll_hover);
                ui.selectable_value(&mut self.tool, Tool::Pan, tr.pan)
                    .on_hover_text(tr.pan_hover);
                ui.selectable_value(&mut self.tool, Tool::Zoom, tr.zoom)
                    .on_hover_text(tr.zoom_hover);
                ui.selectable_value(&mut self.tool, Tool::Rotate, tr.rot)
                    .on_hover_text(tr.rot_hover);
                ui.selectable_value(&mut self.tool, Tool::Orbit3D, tr.orbit_3d)
                    .on_hover_text(tr.orbit_3d_hover);
                if self.tool != prev_tool {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
            }
            ToolbarGroup::RoiTools => {
                let prev_tool = self.tool;
                ui.selectable_value(&mut self.tool, Tool::Length, "📏")
                    .on_hover_text(tr.length_hover);
                ui.selectable_value(&mut self.tool, Tool::Oval, "◯")
                    .on_hover_text(tr.oval_hover);
                ui.selectable_value(&mut self.tool, Tool::Rect, "▭")
                    .on_hover_text(tr.rect_hover);
                let roi_label = match self.tool {
                    Tool::Angle => format!("{}: ∠", tr.roi),
                    Tool::Polygon => format!("{}: ⬠", tr.roi),
                    Tool::Pencil => format!("{}: ✏", tr.roi),
                    Tool::Point => format!("{}: ⌖", tr.roi),
                    _ => format!("{} ▾", tr.roi),
                };
                ui.menu_button(roi_label, |ui| {
                    let items: [(Tool, &str, &str); 7] = [
                        (Tool::Length, tr.roi_length, "L"),
                        (Tool::Angle, tr.roi_angle, "—"),
                        (Tool::Oval, tr.roi_oval, "E"),
                        (Tool::Rect, tr.roi_rect, "B"),
                        (Tool::Polygon, tr.roi_polygon, "G"),
                        (Tool::Pencil, tr.roi_pencil, "F"),
                        (Tool::Point, tr.roi_point, "T"),
                    ];
                    let font_id = egui::TextStyle::Button.resolve(ui.style());
                    let max_label_w = items
                        .iter()
                        .map(|(_, raw_label, _)| {
                            let clean = raw_label
                                .rfind(" (")
                                .map(|idx| &raw_label[..idx])
                                .unwrap_or(raw_label);
                            ui.painter()
                                .layout_no_wrap(
                                    format!("✓ {clean}"),
                                    font_id.clone(),
                                    Color32::WHITE,
                                )
                                .size()
                                .x
                        })
                        .fold(0.0f32, f32::max);
                    let menu_w = (max_label_w + 38.0).ceil();
                    ui.set_min_width(menu_w);
                    ui.set_max_width(menu_w);
                    for (target, raw_label, shortcut) in items {
                        // Strip trailing " (X)" from legacy label so shortcut sits cleanly right-aligned
                        let clean_label = raw_label
                            .rfind(" (")
                            .map(|idx| &raw_label[..idx])
                            .unwrap_or(raw_label);
                        let selected = self.tool == target;
                        let clicked = ui
                            .allocate_ui_with_layout(
                                vec2(menu_w, 20.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let prefix = if selected { "✓ " } else { "   " };
                                    let btn = ui.selectable_label(
                                        selected,
                                        format!("{prefix}{clean_label}"),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(shortcut)
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(Color32::from_gray(145)),
                                            );
                                        },
                                    );
                                    btn.clicked()
                                },
                            )
                            .inner;
                        if clicked {
                            self.tool = target;
                            ui.close();
                        }
                    }
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
                .on_hover_text(tr.roi_menu_hover);
                if self.tool != prev_tool {
                    self.drawing.clear();
                    self.angle_pending = None;
                }
            }
            ToolbarGroup::Presentation => {
                // OsiriX-style 2-row compact stack: WL/WW + CLUT on left, Filter + quick toggles on right
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;

                    // Row 1: WL/WW Presets + Filter
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            egui::RichText::new("WL/WW:")
                                .size(10.0)
                                .color(Color32::from_gray(155)),
                        );
                        let active_win = self.active_cell().window;
                        let mut chosen_win: Option<Option<Window>> = None;
                        egui::ComboBox::from_id_salt("presets")
                            .selected_text(self.preset_label())
                            .width(106.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(active_win.is_none(), tr.preset_default)
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
                                                "~   {}   L {:.0} W {:.0}",
                                                tr.preset_full_dyn, full.level, full.width
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

                        // Filter
                        let cur_conv = self.active_cell().convolution_idx;
                        let conv_label = cur_conv
                            .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
                            .map(|k| k.name)
                            .unwrap_or(tr.filter_none);
                        let mut chosen_conv: Option<Option<usize>> = None;
                        egui::ComboBox::from_id_salt("conv_selector")
                            .selected_text(format!("{}: {conv_label}", tr.filter_prefix))
                            .width(96.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(cur_conv.is_none(), tr.filter_no_filter)
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
                    });

                    // Row 2: CLUT + quick Invert / Overlays / Ref lines pills
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            egui::RichText::new("CLUT:")
                                .size(10.0)
                                .color(Color32::from_gray(155)),
                        );
                        let current_clut_name = self.active_cell().clut_name.clone();
                        let mut chosen_clut: Option<String> = None;
                        egui::ComboBox::from_id_salt("clut_selector")
                            .selected_text(&current_clut_name)
                            .width(106.0)
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

                        ui.toggle_value(&mut self.active_cell_mut().invert, tr.invert);
                        ui.toggle_value(&mut self.show_reference_lines, "╋")
                            .on_hover_text(tr.ref_lines);
                        ui.toggle_value(&mut self.show_annotations, "HUD")
                            .on_hover_text(tr.overlays);
                    });
                });
            }
            ToolbarGroup::ThreeDAndView => {
                let has_series = self.active_cell().selected.is_some();
                ui.add_enabled_ui(has_series || self.ortho_mpr_active, |ui| {
                    let mut ortho_on = self.ortho_mpr_active;
                    if ui
                        .toggle_value(&mut ortho_on, format!("⊞ {}", tr.ortho_mpr))
                        .on_hover_text(tr.ortho_mpr_hover)
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
                        .on_hover_text(tr.mpr_3d_hover)
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
                    if ui.button(tr.rotate_ccw).clicked() {
                        let vp = self.active_cell_mut();
                        vp.rotation = (vp.rotation - 90.0).rem_euclid(360.0);
                        ui.close();
                    }
                    if ui.button(tr.rotate_cw).clicked() {
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
                        .toggle_value(&mut self.gpu_bilinear, tr.smooth_interp)
                        .changed()
                        && let Some(ref db) = self.db {
                            let _ = db.set_setting(
                                "gpu_bilinear",
                                if self.gpu_bilinear { "1" } else { "0" },
                            );
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
                .on_hover_text(tr.view_hover);
                let meta_lbl = match self.lang {
                    crate::i18n::Lang::En => "ℹ Meta-Data",
                    crate::i18n::Lang::Es => "ℹ Metadatos",
                };
                ui.add_enabled_ui(!self.show_database_view && has_series, |ui| {
                    ui.toggle_value(&mut self.show_metadata_inspector, meta_lbl)
                        .on_hover_text("Toggle side-by-side DICOM Meta-Data Inspector");
                });
            }
            ToolbarGroup::Cine => {
                let cine_label = if self.playing { "⏸" } else { "▶" };
                ui.toggle_value(&mut self.playing, cine_label)
                    .on_hover_text(tr.cine_hover);
                ui.menu_button(format!("{} fps", self.fps as i32), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.fps, 1.0..=60.0)
                            .text("fps")
                            .integer(),
                    );
                })
                .response
                .on_hover_text(tr.cine_speed_hover);
            }
            ToolbarGroup::App => {
                let lang_btn = match self.lang {
                    crate::i18n::Lang::En => "🌐 EN",
                    crate::i18n::Lang::Es => "🌐 ES",
                };
                if ui
                    .button(lang_btn)
                    .on_hover_text(tr.lang_hover)
                    .clicked()
                {
                    self.lang = self.lang.toggle();
                    if let Some(db) = &self.db {
                        let _ = db.set_setting("lang", self.lang.code());
                    }
                }
                if ui
                    .button("⚙")
                    .on_hover_text(tr.settings_hover)
                    .clicked()
                {
                    self.show_settings_modal = !self.show_settings_modal;
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
            let tr = crate::i18n::tr(self.lang);
            let disp_r = active_r + 1;
            let disp_c = active_c + 1;
            let count = disp_r * disp_c;
            let vp_word = if count == 1 {
                tr.viewport_singular
            } else {
                tr.viewport_plural
            };
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("{disp_r} × {disp_c}  ({count} {vp_word})"))
                        .strong()
                        .color(Color32::from_rgb(150, 210, 255)),
                );
            });

            if clicked
                && let Some((hr, hc)) = hovered_rc {
                    new_tiling = Some(ViewportTiling::new(hr + 1, hc + 1));
                    ui.close();
                }
        })
        .response
        .on_hover_text(crate::i18n::tr(self.lang).tiling_hover);
        if let Some(t) = new_tiling {
            self.ortho_mpr_active = false;
            self.ortho_crosshair_lps = None;
            self.set_tiling(t);
        }
    }

    fn render_scrubber_and_mpr_strip(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let series_count = self.all_series_pairs().len();
            ui.add_enabled_ui(series_count > 1, |ui| {
                if ui
                    .small_button("◀ Se")
                    .on_hover_text(tr.prev_series)
                    .clicked()
                {
                    self.step_active_series(-1);
                }
                if ui
                    .small_button("Se ▶")
                    .on_hover_text(tr.next_series)
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
                    .on_hover_text(tr.first_slice)
                    .clicked()
                {
                    self.set_viewport_slice(vp_idx, 0, true);
                }
                if ui
                    .small_button("◀")
                    .on_hover_text(tr.prev_slice)
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
                    .on_hover_text(format!("{} {cur_1based} / {slice_count}", tr.slice_word));
                if slider_resp.changed() {
                    self.set_viewport_slice(vp_idx, cur_1based as isize - 1, true);
                }
                ui.monospace(format!("{cur_1based}/{slice_count}"));
                if ui.small_button("▶").on_hover_text(tr.next_slice).clicked() {
                    self.step_viewport_slice(vp_idx, 1);
                }
                if ui
                    .small_button("⏭")
                    .on_hover_text(tr.last_slice)
                    .clicked()
                {
                    self.set_viewport_slice(vp_idx, isize::MAX, true);
                }
            });

            if self.active_cell().mpr_enabled {
                ui.separator();
                if ui.small_button("Ax").on_hover_text(tr.ax_hover).clicked() {
                    self.switch_viewport_to_plane(vp_idx, crate::dicom_index::DominantPlane::Axial);
                }
                if ui
                    .small_button("Cor")
                    .on_hover_text(tr.cor_hover)
                    .clicked()
                {
                    self.switch_viewport_to_plane(vp_idx, crate::dicom_index::DominantPlane::Coronal);
                }
                if ui
                    .small_button("Sag")
                    .on_hover_text(tr.sag_hover)
                    .clicked()
                {
                    self.switch_viewport_to_plane(vp_idx, crate::dicom_index::DominantPlane::Sagittal);
                }
                if ui
                    .small_button("Nat")
                    .on_hover_text(tr.nat_hover)
                    .clicked()
                    && let Some(vol) = self.viewport_volume(vp_idx).cloned() {
                        self.set_viewport_mpr_plane(
                            vp_idx,
                            MprPlane3D::from_volume_native(&vol),
                            true,
                        );
                    }
                if let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("mpr_slab_mode")
                        .selected_text(format!("{} {}", tr.slab_prefix, plane.slab_mode.label()))
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
                    if plane.slab_mode != SlabMode::Thin
                        && ui
                            .add(
                                egui::Slider::new(&mut plane.slab_thickness_mm, 1.0..=50.0)
                                    .suffix(" mm")
                                    .fixed_decimals(1),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    if changed {
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                }
                let sib_count = self.viewport_sibling_volumes(vp_idx).len();
                if sib_count > 0 {
                    if ui
                        .checkbox(&mut self.blend_siblings, tr.blend_siblings)
                        .on_hover_text(tr.blend_siblings_hover)
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
                                "{} {} {}",
                                tr.blended_from,
                                sib_count + 1,
                                tr.roi_stats_disabled
                            ),
                        );
                    }
                }
            }

            let cell_count = self.tiling.cell_count();
            if cell_count > 1 && !self.ortho_mpr_active {
                ui.separator();
                let (lbl_zoom, lbl_wl, lbl_cut) = match self.lang {
                    crate::i18n::Lang::En => ("🔗 Zoom", "🔗 WL", "🔗 Cut"),
                    crate::i18n::Lang::Es => ("🔗 Zoom", "🔗 NV", "🔗 Corte"),
                };
                if ui
                    .toggle_value(&mut self.link_zoom, lbl_zoom)
                    .on_hover_text("Link Zoom & Pan across linked viewports")
                    .changed()
                {
                    self.ensure_default_link_pair();
                }
                if ui
                    .toggle_value(&mut self.link_wl, lbl_wl)
                    .on_hover_text("Link Window Level / Width across linked viewports")
                    .changed()
                {
                    self.ensure_default_link_pair();
                }
                if ui
                    .toggle_value(&mut self.link_cut, lbl_cut)
                    .on_hover_text("Link Slice / Cut position across linked viewports (without forcing Ax/Cor/Sag)")
                    .changed()
                {
                    self.ensure_default_link_pair();
                }
                if cell_count > 2 {
                    ui.separator();
                    let active_g = self.viewports[self.active_viewport].link_group;
                    ui.label(
                        egui::RichText::new("Link:")
                            .size(10.5)
                            .color(Color32::from_gray(160)),
                    );
                    if ui
                        .selectable_label(active_g == 0, "○")
                        .on_hover_text("Unlink active viewport")
                        .clicked()
                    {
                        self.viewports[self.active_viewport].link_group = 0;
                    }
                    for g in 1u8..=4 {
                        let col = Self::link_group_color(g);
                        let txt = egui::RichText::new(format!("{g}")).strong().color(col);
                        if ui
                            .selectable_label(active_g == g, txt)
                            .on_hover_text(format!("Assign active viewport to Link Group {g}"))
                            .clicked()
                        {
                            self.viewports[self.active_viewport].link_group = g;
                        }
                    }
                }
                ui.separator();
                if ui
                    .small_button(tr.lock_3d_sync)
                    .on_hover_text(tr.lock_3d_sync_hover)
                    .clicked()
                {
                    self.lock_current_3d_alignment();
                }
            }
        });
    }
}
