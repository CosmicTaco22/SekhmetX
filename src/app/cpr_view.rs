//! Curved MPR screens: the path tool's result shown in its own window, with turn and
//! width controls and a length measurement on the straightened image. The sampling
//! is ported from IsiX (`src/imaging/cpr.rs`); the window is rebuilt with egui.

use super::*;
use crate::imaging::cpr;
use crate::ui::theme;

/// A curved MPR made from a path drawn in viewport `vp`.
pub(crate) struct CprView {
    pub(crate) vp: usize,
    /// The clicked points, patient LPS mm.
    pub(crate) nodes: Vec<[f64; 3]>,
    /// The normal of the plane the path was drawn on: rows run along it at turn 0°.
    pub(crate) plane_normal: [f64; 3],
    pub(crate) angle_deg: f64,
    pub(crate) half_width_mm: f64,
    pub(crate) slab_mode: crate::volume::SlabMode,
    pub(crate) slab_thickness_mm: f64,
    pub(crate) path_name: String,
    pub(crate) result: Option<cpr::Straightened>,
    pub(crate) window: Window,
    pub(crate) invert: bool,
    pub(crate) texture: Option<TextureHandle>,
    pub(crate) tex_key: Option<(i64, i64, u8, i64, u32, u32, bool)>,
    pub(crate) show_cross_section: bool,
    pub(crate) cross_col: usize,
    pub(crate) cross_texture: Option<TextureHandle>,
    pub(crate) cross_tex_key: Option<(usize, i64, u32, u32, bool)>,
    /// A length measured on the straightened image, image pixels.
    pub(crate) measure: Option<(Pos2, Pos2)>,
    pub(crate) open: bool,
}

impl CprView {
    fn rebuild(&mut self, vol: &crate::volume::Volume3D) {
        self.result = cpr::straighten_slab(
            vol,
            &self.nodes,
            self.plane_normal,
            self.angle_deg,
            self.half_width_mm,
            self.slab_mode,
            self.slab_thickness_mm,
        );
        self.texture = None;
        self.cross_texture = None;
        self.cross_tex_key = None;
        if let Some(ref r) = self.result {
            self.cross_col = self.cross_col.min(r.path.len().saturating_sub(1));
        }
        self.measure = None;
    }
}

impl ViewerApp {
    /// Ends the curved path drawn in `vp_idx` (image pixel points in `self.drawing`)
    /// and opens its straightened view.
    pub(super) fn finish_curved_path(&mut self, vp_idx: usize, frame: &Frame) {
        let pts = std::mem::take(&mut self.drawing);
        let es = self.lang == crate::i18n::Lang::Es;
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            self.message = Some(
                if es {
                    "MPR curva: esta serie no tiene volumen 3D (active 3D MPR)"
                } else {
                    "Curved MPR: this series has no 3D volume (switch on 3D MPR)"
                }
                .into(),
            );
            return;
        };
        let nodes: Vec<[f64; 3]> = pts
            .iter()
            .filter_map(|p| frame.patient_coords_at(p.x as f64, p.y as f64))
            .collect();
        if nodes.len() < 3 {
            self.message = Some(
                if es { "MPR curva: marque al menos 3 puntos" } else { "Curved MPR: place at least 3 points" }.into(),
            );
            return;
        }
        let o = frame.orientation;
        let plane_normal = crate::volume::cross3([o[0], o[1], o[2]], [o[3], o[4], o[5]]);
        let window = self.effective_window(vp_idx, frame);
        let mut view = CprView {
            vp: vp_idx,
            nodes,
            plane_normal,
            angle_deg: 0.0,
            half_width_mm: 40.0,
            slab_mode: crate::volume::SlabMode::Thin,
            slab_thickness_mm: 10.0,
            path_name: "Path 1".to_string(),
            result: None,
            window,
            invert: self.viewports[vp_idx].effective_invert(),
            texture: None,
            tex_key: None,
            show_cross_section: true,
            cross_col: 0,
            cross_texture: None,
            cross_tex_key: None,
            measure: None,
            open: true,
        };
        view.rebuild(&vol);
        if view.result.is_none() {
            let diag = cpr::check_path(&view.nodes, 100).diagnosis;
            self.message = Some(format!("{}: {diag}", if es { "MPR curva no válida" } else { "Curved MPR not possible" }));
            return;
        }
        self.cpr = Some(view);
    }

    /// The path of the open curved MPR, drawn on the viewport it was made in.
    pub(super) fn draw_cpr_path(&self, painter: &egui::Painter, vp_idx: usize, img: Rect, f: &Frame) {
        let Some(view) = self.cpr.as_ref().filter(|v| v.open && v.vp == vp_idx) else {
            return;
        };
        let (Some(origin), Some((rs, cs))) = (f.origin, f.spacing) else {
            return;
        };
        if !f.is_3d_plane {
            return;
        }
        let o = f.orientation;
        let to_px = |p: [f64; 3]| {
            let rel = sub3(p, origin);
            pos2(
                (dot3(rel, [o[0], o[1], o[2]]) / cs + 0.5) as f32,
                (dot3(rel, [o[3], o[4], o[5]]) / rs + 0.5) as f32,
            )
        };
        let col = theme::accent_bright();
        let path = cpr::smooth_path(&view.nodes, 2.0);
        let pts: Vec<Pos2> = path.iter().map(|&p| self.pixel_to_screen(vp_idx, img, f, to_px(p))).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.6, col)));
        for &n in &view.nodes {
            painter.circle_filled(self.pixel_to_screen(vp_idx, img, f, to_px(n)), 3.0, col);
        }
    }

    /// The curved MPR window: the straightened image, turn and width controls, and a
    /// length measurement (drag on the image).
    pub(super) fn draw_cpr_window(&mut self, ctx: &egui::Context) {
        let Some(mut view) = self.cpr.take() else {
            return;
        };
        let es = self.lang == crate::i18n::Lang::Es;
        let t = |en: &'static str, sp: &'static str| if es { sp } else { en };
        let vol = self
            .viewports
            .get(view.vp)
            .and_then(|v| v.selected)
            .and_then(|sel| self.volumes.get(&sel).cloned());
        let mut rebuild = false;
        let mut open = view.open;
        egui::Window::new(t("Curved MPR", "MPR curva"))
            .open(&mut open)
            .default_size(vec2(720.0, 420.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Turn", "Girar"));
                    if ui
                        .add(egui::Slider::new(&mut view.angle_deg, -90.0..=90.0).suffix("°").step_by(1.0))
                        .on_hover_text(t(
                            "Turns the cut about the path: 0° across the plane you drew on, 90° within it",
                            "Gira el corte alrededor del trazado: 0° a través del plano dibujado, 90° dentro de él",
                        ))
                        .drag_stopped()
                    {
                        rebuild = true;
                    }
                    for a in [0.0, 90.0] {
                        if ui.small_button(format!("{a:.0}°")).clicked() {
                            view.angle_deg = a;
                            rebuild = true;
                        }
                    }
                    ui.separator();
                    ui.label(t("Width", "Ancho"));
                    if ui
                        .add(egui::Slider::new(&mut view.half_width_mm, 10.0..=120.0).suffix(" mm").step_by(5.0))
                        .on_hover_text(t("How far the image reaches to each side of the path", "Cuánto abarca la imagen a cada lado del trazado"))
                        .drag_stopped()
                    {
                        rebuild = true;
                    }
                    if let Some(r) = &view.result {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("{}: {:.1} mm", t("Path", "Trazado"), r.length_mm))
                                .color(theme::dim_text(ui)),
                        );
                        if r.check.diagnosis == "self-intersecting loop" {
                            ui.label(
                                egui::RichText::new(t("(the path crosses itself)", "(el trazado se cruza)"))
                                    .color(Color32::from_rgb(240, 190, 90)),
                            );
                        }
                    }
                });
                // Thick-slab projection (Thin / MIP / MinIP / Mean) & Save/Load Path controls.
                let series_uid = self
                    .viewport_series(view.vp)
                    .map(|se| se.uid.clone())
                    .unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Slab", "Lámina"));
                    for (mode, lbl) in [
                        (crate::volume::SlabMode::Thin, t("Thin", "Fina")),
                        (crate::volume::SlabMode::Mip, "MIP"),
                        (crate::volume::SlabMode::MinIp, "MinIP"),
                        (crate::volume::SlabMode::Mean, t("Mean", "Media")),
                    ] {
                        if ui.selectable_label(view.slab_mode == mode, lbl).clicked() {
                            view.slab_mode = mode;
                            rebuild = true;
                        }
                    }
                    if view.slab_mode != crate::volume::SlabMode::Thin
                        && ui
                            .add(
                                egui::Slider::new(&mut view.slab_thickness_mm, 2.0..=50.0)
                                    .suffix(" mm")
                                    .step_by(1.0),
                            )
                            .drag_stopped()
                        {
                            rebuild = true;
                        }
                    ui.separator();
                    ui.checkbox(&mut view.show_cross_section, t("Cross-section", "Transversal"));
                    if view.show_cross_section
                        && let Some(ref r) = view.result
                        && r.path.len() > 1
                    {
                        let max_col = r.path.len() - 1;
                        view.cross_col = view.cross_col.min(max_col);
                        ui.add(
                            egui::Slider::new(&mut view.cross_col, 0..=max_col)
                                .show_value(false),
                        )
                        .on_hover_text(t(
                            "Slide along the path to inspect perpendicular cross-sections",
                            "Deslice a lo largo del trazado para ver cortes transversales",
                        ));
                    }
                    if let Some(ref db) = self.db
                        && !series_uid.is_empty()
                    {
                        ui.separator();
                        ui.add(
                            egui::TextEdit::singleline(&mut view.path_name)
                                .desired_width(88.0)
                                .hint_text(t("Path name", "Nombre")),
                        );
                        if ui.small_button(t("Save", "Guardar")).clicked() {
                            let name = view.path_name.trim();
                            if !name.is_empty()
                                && db.save_cpr_path(&series_uid, name, &view.nodes, view.plane_normal).is_ok()
                            {
                                self.message = Some(format!(
                                    "{}: {}",
                                    t("Saved CPR path", "Trazado CPR guardado"),
                                    name
                                ));
                            }
                        }
                        let saved = db.load_cpr_paths(&series_uid).unwrap_or_default();
                        if !saved.is_empty() {
                            egui::ComboBox::from_id_salt("cpr_saved_paths")
                                .selected_text(t("Saved…", "Guardados…"))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for (name, nodes, normal) in saved {
                                        ui.horizontal(|ui| {
                                            if ui.selectable_label(false, &name).clicked() {
                                                view.nodes = nodes;
                                                view.plane_normal = normal;
                                                view.path_name = name.clone();
                                                rebuild = true;
                                            }
                                            if ui.small_button("×").clicked() {
                                                let _ = db.delete_cpr_path(&series_uid, &name);
                                            }
                                        });
                                    }
                                });
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(t(
                        "Drag on the image to measure a length. The path runs along the middle row.",
                        "Arrastre sobre la imagen para medir una longitud. El trazado corre por la fila central.",
                    ))
                    .small()
                    .color(theme::dim_text(ui)),
                );
                let Some(result) = &view.result else {
                    return;
                };
                let f = result.frame.clone();
                let key = (
                    (view.angle_deg * 10.0) as i64,
                    (view.half_width_mm * 10.0) as i64,
                    view.slab_mode as u8,
                    (view.slab_thickness_mm * 10.0) as i64,
                    view.window.level.to_bits(),
                    view.window.width.to_bits(),
                    view.invert,
                );
                if view.texture.is_none() || view.tex_key != Some(key) {
                    let rgba = f.render_with_clut(view.window, view.invert != f.inverts, None);
                    let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &rgba);
                    view.texture = Some(ctx.load_texture("curved-mpr", img, egui::TextureOptions::LINEAR));
                    view.tex_key = Some(key);
                }
                let Some(tex) = &view.texture else {
                    return;
                };
                let avail = ui.available_size();
                let cross_side = if view.show_cross_section && vol.is_some() {
                    (avail.y.min(avail.x * 0.32)).clamp(110.0, 240.0)
                } else {
                    0.0
                };
                let main_avail_x = if cross_side > 0.0 {
                    (avail.x - cross_side - 10.0).max(100.0)
                } else {
                    avail.x.max(100.0)
                };
                let aspect = f.height as f32 / f.width.max(1) as f32;
                let w = main_avail_x;
                let h = (w * aspect).min(avail.y.max(80.0));
                let w = h / aspect.max(1e-3);

                let (rect, resp) = ui
                    .horizontal(|ui| {
                        let (r_main, resp_main) = ui.allocate_exact_size(vec2(w, h), Sense::drag());
                        if cross_side > 0.0
                            && let Some(ref v_ref) = vol
                        {
                            ui.add_space(6.0);
                            let c_key = (
                                view.cross_col,
                                (view.half_width_mm * 10.0) as i64,
                                view.window.level.to_bits(),
                                view.window.width.to_bits(),
                                view.invert,
                            );
                            if (view.cross_texture.is_none() || view.cross_tex_key != Some(c_key))
                                && let Some(cf) =
                                    result.cross_section_at(v_ref, view.cross_col, view.half_width_mm)
                            {
                                let rgba = cf.render_with_clut(view.window, view.invert != cf.inverts, None);
                                let cimg = egui::ColorImage::from_rgba_unmultiplied(
                                    [cf.width as usize, cf.height as usize],
                                    &rgba,
                                );
                                view.cross_texture = Some(ctx.load_texture(
                                    "curved-mpr-cross",
                                    cimg,
                                    egui::TextureOptions::LINEAR,
                                ));
                                view.cross_tex_key = Some(c_key);
                            }
                            if let Some(ref ctex) = view.cross_texture {
                                let (crect, _) = ui.allocate_exact_size(vec2(cross_side, cross_side), Sense::hover());
                                ui.painter().image(
                                    ctex.id(),
                                    crect,
                                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                let c = crect.center();
                                let ch_col = theme::accent_bright().gamma_multiply(0.7);
                                ui.painter().line_segment(
                                    [pos2(c.x - 8.0, c.y), pos2(c.x + 8.0, c.y)],
                                    Stroke::new(1.0, ch_col),
                                );
                                ui.painter().line_segment(
                                    [pos2(c.x, c.y - 8.0), pos2(c.x, c.y + 8.0)],
                                    Stroke::new(1.0, ch_col),
                                );
                                ui.painter().rect_stroke(
                                    crect,
                                    2.0,
                                    Stroke::new(1.0, theme::accent()),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                        (r_main, resp_main)
                    })
                    .inner;
                ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if view.show_cross_section && result.path.len() > 1 {
                    let frac = view.cross_col as f32 / (result.path.len() - 1) as f32;
                    let cx = rect.min.x + frac * rect.width();
                    ui.painter().line_segment(
                        [pos2(cx, rect.min.y), pos2(cx, rect.max.y)],
                        Stroke::new(1.2, theme::accent_bright().gamma_multiply(0.8)),
                    );
                }
                let to_px = |p: Pos2| {
                    pos2((p.x - rect.min.x) / rect.width() * f.width as f32, (p.y - rect.min.y) / rect.height() * f.height as f32)
                };
                let to_scr = |p: Pos2| {
                    pos2(rect.min.x + p.x / f.width as f32 * rect.width(), rect.min.y + p.y / f.height as f32 * rect.height())
                };
                // The path itself: the middle row.
                let mid_y = rect.center().y;
                ui.painter().line_segment(
                    [pos2(rect.min.x, mid_y), pos2(rect.min.x + 10.0, mid_y)],
                    Stroke::new(2.0, theme::accent_bright()),
                );
                ui.painter().line_segment(
                    [pos2(rect.max.x - 10.0, mid_y), pos2(rect.max.x, mid_y)],
                    Stroke::new(2.0, theme::accent_bright()),
                );
                if resp.drag_started()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let px = to_px(p);
                    view.measure = Some((px, px));
                }
                if resp.dragged()
                    && let (Some(p), Some((a, _))) = (resp.interact_pointer_pos(), view.measure)
                {
                    view.measure = Some((a, to_px(p)));
                }
                if let Some((a, b)) = view.measure {
                    let col = Color32::from_rgb(255, 220, 60);
                    ui.painter().line_segment([to_scr(a), to_scr(b)], Stroke::new(1.6, col));
                    let mm = a.distance(b) as f64 * result.spacing;
                    ui.painter().text(
                        to_scr(b) + vec2(8.0, -8.0),
                        Align2::LEFT_BOTTOM,
                        format!("{mm:.1} mm"),
                        FontId::monospace(12.0),
                        col,
                    );
                }
                if let Some(p) = resp.hover_pos() {
                    let px = to_px(p);
                    if let Some(v) = f.value_at(px.x.max(0.0) as u32, px.y.max(0.0) as u32) {
                        let along = px.x as f64 * result.spacing;
                        let across = (px.y as f64 - (f.height as f64 - 1.0) / 2.0) * result.spacing;
                        resp.on_hover_text(format!(
                            "{v}\n{}: {along:.1} mm · {}: {across:+.1} mm",
                            t("along the path", "a lo largo"),
                            t("across", "transversal")
                        ));
                    }
                }
            });
        view.open = open;
        if rebuild && let Some(vol) = vol {
            view.rebuild(&vol);
        }
        if view.open {
            self.cpr = Some(view);
        }
    }
}

impl ViewerApp {
    /// Cuts away, in the 3D view of `vp_idx`, everything inside (or outside) the loop
    /// drawn on screen, through the whole depth; the earlier state is kept for Undo.
    pub(super) fn apply_sculpt(&mut self, vp_idx: usize, view: Rect, lasso: &[Pos2], side: crate::sculpt::CutSide) {
        if lasso.len() < 3 || view.width() < 1.0 || view.height() < 1.0 {
            return;
        }
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            return;
        };
        let vp = &self.viewports[vp_idx];
        let cam = crate::sculpt::vr_camera(&vol, vp.vr_yaw, vp.vr_pitch, vp.rotation, vp.x_flipped, vp.y_flipped);
        let pts: Vec<[f32; 2]> = lasso
            .iter()
            .map(|p| [(p.x - view.min.x) / view.width(), (p.y - view.min.y) / view.height()])
            .collect();
        let previous = vp.vr_sculpt.clone().filter(|m| m.volume_id == vol.gpu_id);
        let mut mask = previous.as_deref().cloned().unwrap_or_else(|| crate::sculpt::SculptMask::new(&vol));
        let cut = mask.cut(&cam, vp.zoom, view.width() / view.height(), &pts, side);
        let es = self.lang == crate::i18n::Lang::Es;
        if cut == 0 {
            self.message = Some(if es { "Tijeras: el lazo no cubre el volumen" } else { "Scissors: the loop does not cover the volume" }.into());
            return;
        }
        let pct = 100.0 * mask.cut_count() as f32 / mask.data.len().max(1) as f32;
        self.message = Some(if es {
            format!("Tijeras: {pct:.0} % del volumen recortado (Deshacer en el menú Tijeras)")
        } else {
            format!("Scissors: {pct:.0} % of the volume cut away (Undo in the Scissors menu)")
        });
        let vp = &mut self.viewports[vp_idx];
        if let Some(prev) = previous {
            vp.vr_sculpt_undo.push(prev);
            if vp.vr_sculpt_undo.len() > 10 {
                vp.vr_sculpt_undo.remove(0);
            }
        } else {
            vp.vr_sculpt_undo.clear();
            vp.vr_sculpt_undo.push(Arc::new(crate::sculpt::SculptMask::new(&vol)));
        }
        vp.vr_sculpt = Some(Arc::new(mask));
    }
}
