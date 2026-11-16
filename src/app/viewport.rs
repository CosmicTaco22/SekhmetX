use super::*;

impl ViewerApp {
    pub(super) fn viewport_grid(&mut self, ui: &mut egui::Ui) {
        let full_rect = ui.available_rect_before_wrap();
        let rows = self.tiling.rows.max(1);
        let cols = self.tiling.columns.max(1);
        let cell_w = full_rect.width() / cols as f32;
        let cell_h = full_rect.height() / rows as f32;

        self.cursor_info = None;
        for r in 0..rows {
            for c in 0..cols {
                let vp_idx = r * cols + c;
                let min = pos2(
                    full_rect.min.x + c as f32 * cell_w,
                    full_rect.min.y + r as f32 * cell_h,
                );
                let max = pos2(min.x + cell_w, min.y + cell_h);
                let cell_rect = Rect::from_min_max(min, max).shrink(1.0);
                self.render_single_viewport(ui, vp_idx, cell_rect);
            }
        }
    }

    pub(super) fn image_rect(&self, vp_idx: usize, view: Rect, f: &Frame) -> Rect {
        let vp = &self.viewports[vp_idx];
        let (rs, cs) = f.spacing.unwrap_or((1.0, 1.0));
        let m = rs.min(cs).max(1e-6);
        let mut w = f.width as f32 * (cs / m) as f32;
        let mut h = f.height as f32 * (rs / m) as f32;
        // Swap aspect ratio when rotated near 90° or 270°.
        let rot = vp.rotation.rem_euclid(180.0);
        if (45.0..135.0).contains(&rot) {
            std::mem::swap(&mut w, &mut h);
        }
        let fit = (view.width() / w).min(view.height() / h);
        Rect::from_center_size(view.center() + vp.pan, vec2(w, h) * fit * vp.zoom)
    }

    /// Maps a screen position inside `img` to unrotated image pixel coordinates `(0..width, 0..height)`.
    pub(super) fn screen_to_pixel(&self, vp_idx: usize, img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        let vp = &self.viewports[vp_idx];
        let rel_x = (p.x - img.min.x) / img.width().max(1.0);
        let rel_y = (p.y - img.min.y) / img.height().max(1.0);
        let uv_map = QuadUvMapping::from_orientation(vp.rotation, vp.x_flipped, vp.y_flipped);
        let tc_x = uv_map.origin[0] + rel_x * uv_map.u[0] + rel_y * uv_map.v[0];
        let tc_y = uv_map.origin[1] + rel_x * uv_map.u[1] + rel_y * uv_map.v[1];
        pos2(tc_x * f.width as f32, tc_y * f.height as f32)
    }

    /// Maps unrotated image pixel coordinates `(0..width, 0..height)` to screen coordinates inside `img`.
    pub(super) fn pixel_to_screen(&self, vp_idx: usize, img: Rect, f: &Frame, p: Pos2) -> Pos2 {
        let vp = &self.viewports[vp_idx];
        let mut dx = (p.x / f.width.max(1) as f32) - 0.5;
        let mut dy = (p.y / f.height.max(1) as f32) - 0.5;
        if vp.x_flipped {
            dx = -dx;
        }
        if vp.y_flipped {
            dy = -dy;
        }
        let rad = (vp.rotation as f64) * std::f64::consts::PI / 180.0;
        let c = rad.cos() as f32;
        let s = rad.sin() as f32;
        let rx = dx * c - dy * s + 0.5;
        let ry = dx * s + dy * c + 0.5;
        pos2(
            img.min.x + rx * img.width(),
            img.min.y + ry * img.height(),
        )
    }

    pub(super) fn viewport_scrubber_rects(view: Rect, slice_count: usize) -> Option<(Rect, Rect, Rect, Rect)> {
        if slice_count <= 1 || view.width() < 220.0 || view.height() < 140.0 {
            return None;
        }
        let bar_w = (view.width() * 0.44).clamp(160.0, 320.0);
        // Sits above the bottom ruler (drawn at the very edge) so the two never overlap.
        let bar_h = 20.0;
        let bar_rect = Rect::from_center_size(
            pos2(view.center().x, view.bottom() - 24.0),
            vec2(bar_w, bar_h),
        );
        let prev_rect = Rect::from_min_size(bar_rect.min + vec2(3.0, 2.0), vec2(20.0, bar_h - 4.0));
        let next_rect = Rect::from_min_size(
            pos2(bar_rect.max.x - 23.0, bar_rect.min.y + 2.0),
            vec2(20.0, bar_h - 4.0),
        );
        let track_rect = Rect::from_min_max(
            pos2(prev_rect.max.x + 6.0, bar_rect.min.y + 4.0),
            pos2(next_rect.min.x - 6.0, bar_rect.max.y - 4.0),
        );
        Some((bar_rect, prev_rect, track_rect, next_rect))
    }

    fn render_single_viewport(&mut self, ui: &mut egui::Ui, vp_idx: usize, view: Rect) {
        if let Some(slot) = self.viewport_rects.get_mut(vp_idx) {
            *slot = view;
        }
        let response = ui.allocate_rect(view, Sense::click_and_drag());
        if (response.clicked() || response.drag_started() || response.secondary_clicked())
            && self.active_viewport != vp_idx {
                self.active_viewport = vp_idx;
                self.drawing.clear();
                self.angle_pending = None;
            }

        let painter = ui.painter_at(view);
        painter.rect_filled(view, 0.0, Color32::BLACK);

        if self.viewport_series(vp_idx).is_none() {
            let tr = crate::i18n::tr(self.lang);
            let text = if self.scan.is_some() {
                tr.indexing_short
            } else if vp_idx == 0 {
                tr.drop_hint
            } else {
                tr.assign_vp_hint
            };
            painter.text(
                view.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                Color32::GRAY,
            );
            self.draw_viewport_border(&painter, vp_idx, view);
            self.handle_series_drop(ui, &painter, vp_idx, view);
            return;
        }

        self.handle_wheel(ui, vp_idx, &response);

        let slice_count = self.viewport_slice_count(vp_idx);
        let scrubber = Self::viewport_scrubber_rects(view, slice_count);
        let press_origin = ui.input(|i| i.pointer.press_origin());
        let interact_pos = response.interact_pointer_pos();
        let on_scrubber = scrubber.is_some_and(|(bar, _, _, _)| {
            press_origin.is_some_and(|p| bar.contains(p))
                || (response.clicked() && interact_pos.is_some_and(|p| bar.contains(p)))
        });

        if on_scrubber
            && let Some((_, prev_r, track_r, next_r)) = scrubber {
                if response.clicked() {
                    if let Some(pos) = interact_pos {
                        if prev_r.contains(pos) {
                            self.step_viewport_slice(vp_idx, -1);
                        } else if next_r.contains(pos) {
                            self.step_viewport_slice(vp_idx, 1);
                        } else if track_r.expand(4.0).contains(pos) {
                            let t = ((pos.x - track_r.min.x) / track_r.width().max(1.0)).clamp(0.0, 1.0);
                            let idx = (t * (slice_count.saturating_sub(1)) as f32).round() as isize;
                            self.set_viewport_slice(vp_idx, idx, true);
                        }
                    }
                } else if (response.dragged_by(egui::PointerButton::Primary)
                    || response.drag_started())
                    && press_origin.is_some_and(|p| track_r.expand(6.0).contains(p))
                    && let Some(pos) = interact_pos {
                        let t = ((pos.x - track_r.min.x) / track_r.width().max(1.0)).clamp(0.0, 1.0);
                        let idx = (t * (slice_count.saturating_sub(1)) as f32).round() as isize;
                        self.set_viewport_slice(vp_idx, idx, true);
                    }
            }

        if self.viewports[vp_idx].vr_enabled
            && let Some(vol) = self.viewport_volume(vp_idx).cloned()
        {
            self.render_vr_viewport(ui, &painter, vp_idx, view, &response, vol);
            return;
        }

        let mut gpu_mpr = self.mpr_gpu_target(vp_idx);
        let mpr_pair = if gpu_mpr.is_none() {
            self.ensure_mpr_frame(vp_idx)
        } else {
            None
        };
        let cur_slice = self.viewports[vp_idx].slice;
        let cur_key = mpr_pair
            .as_ref()
            .map(|(_, k)| *k)
            .or_else(|| self.viewport_slice_key(vp_idx, cur_slice));
        let frame_opt = gpu_mpr
            .as_ref()
            .map(|(_, _, f)| f.clone())
            .or_else(|| mpr_pair.map(|(f, _)| f))
            .or_else(|| self.viewport_frame(vp_idx).cloned());
        let Some(mut frame) = frame_opt else {
            let text = match cur_key.and_then(|k| self.failed.get(&k)) {
                Some(err) => format!("Could not display this image:\n{err}"),
                None => "Loading…".to_string(),
            };
            // Keep showing the previous image while the next slice decodes.
            if let Some((prev, key)) = self.viewports[vp_idx].last_gpu.clone() {
                let r = self.image_rect(vp_idx, view, &prev);
                let w = self.effective_window(vp_idx, &prev);
                painter.add(
                    self.gpu_callback(ui, vp_idx, prev.clone(), key, r, w)
                        .into_shape(view),
                );
            } else if let Some(tex) = &self.viewports[vp_idx].texture {
                painter.image(
                    tex.id(),
                    view,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::from_gray(160),
                );
            }
            painter.text(
                view.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                Color32::LIGHT_GRAY,
            );
            if self.show_annotations {
                self.draw_overlay(&painter, vp_idx, view, None);
            }
            self.draw_scrubber_bar(ui, &painter, vp_idx, view);
            self.draw_viewport_border(&painter, vp_idx, view);
            return;
        };

        let mut cur_key = cur_key;
        if !on_scrubber {
            let consumed = self.handle_ortho_crosshair_drag(vp_idx, &response, view, &frame);
            if !consumed {
                self.handle_drag(vp_idx, &response, view, &frame);
            }
            if gpu_mpr.is_some() {
                gpu_mpr = self.mpr_gpu_target(vp_idx);
                if let Some((_, _, ref updated_f)) = gpu_mpr {
                    frame = updated_f.clone();
                }
            } else if let Some((updated_f, updated_k)) = self.ensure_mpr_frame(vp_idx) {
                frame = updated_f;
                cur_key = Some(updated_k);
            }
        }

        // Preserve the unconvolved source frame for pixel value readout & ROI statistics
        // (matching IsiX DCMPix+DisplaySource: convolution is presentation-only).
        let source_frame = frame.clone();

        // Apply 2D Spatial Convolution Filter if active on this viewport (cached per slice & filter).
        if let (Some(conv_idx), Some(base_key)) = (self.viewports[vp_idx].convolution_idx, cur_key)
            && let Some(kernel_def) = FACTORY_CONVOLUTIONS.get(conv_idx) {
                let hit = self
                    .viewports[vp_idx]
                    .conv_cached
                    .as_ref()
                    .filter(|(k, c_idx, _, _)| *k == base_key && *c_idx == conv_idx)
                    .map(|(_, _, cf, ck)| (cf.clone(), *ck));
                if let Some((cf, ck)) = hit {
                    frame = cf;
                    cur_key = Some(ck);
                } else {
                    let conv = kernel_def.to_convolution();
                    let cf = Arc::new(frame.with_convolution(&conv));
                    let mut ck = base_key;
                    ck.slice ^= (conv_idx + 1) << 24;
                    self.viewports[vp_idx].conv_cached =
                        Some((base_key, conv_idx, cf.clone(), ck));
                    frame = cf;
                    cur_key = Some(ck);
                }
            }

        let window = self.effective_window(vp_idx, &frame);
        let img = self.image_rect(vp_idx, view, &frame);
        if let Some((ref gpu_vol, ref gpu_geom, _)) = gpu_mpr {
            painter.add(
                self.gpu_mpr_callback(ui, vp_idx, gpu_vol.clone(), gpu_geom.clone(), img, window, frame.inverts)
                    .into_shape(view),
            );
        } else {
            let on_gpu = self.gpu.is_some_and(|g| {
                matches!(frame.pixels, crate::frame::Pixels::Gray(_))
                    && frame.width <= g.max_texture
                    && frame.height <= g.max_texture
            });
            if let (true, Some(key)) = (on_gpu, cur_key) {
                painter.add(
                    self.gpu_callback(ui, vp_idx, frame.clone(), key, img, window)
                        .into_shape(view),
                );
                self.viewports[vp_idx].last_gpu = Some((frame.clone(), key));
            } else if let Some(key) = cur_key {
                self.draw_on_cpu(ui, vp_idx, &painter, &frame, key, img, window);
            }
        }

        // Live cursor readout with calibrated HU / pixel value and 3D patient coordinates.
        if let Some(p) = response.hover_pos() {
            let px = self.screen_to_pixel(vp_idx, img, &frame, p);
            if vp_idx == self.active_viewport {
                self.hover_px = Some(px);
            }
            if px.x >= 0.0 && px.y >= 0.0 {
                let (x, y) = (px.x as u32, px.y as u32);
                let val_str_opt = if let Some((ref gpu_vol, ref gpu_geom, _)) = gpu_mpr {
                    gpu_vol.sample_plane_at(gpu_geom, px.x as f64, px.y as f64).map(|v| {
                        let unit = gpu_vol.value_unit;
                        if unit.is_empty() {
                            format!("{v:.1}")
                        } else {
                            format!("{v:.1} {unit}")
                        }
                    })
                } else {
                    source_frame.value_at(x, y)
                };
                if let Some(val_str) = val_str_opt {
                    let coords_3d = source_frame
                        .patient_coords_at(px.x as f64, px.y as f64)
                        .map(|[cx, cy, cz]| format!("   XYZ: ({cx:.1}, {cy:.1}, {cz:.1}) mm"))
                        .unwrap_or_default();
                    self.cursor_info =
                        Some(format!("X: {x}  Y: {y}   Val: {val_str}{coords_3d}"));
                }
            }
        } else if vp_idx == self.active_viewport {
            self.hover_px = None;
        }

        if self.ortho_mpr_active && vp_idx < 3 {
            self.draw_ortho_crosshairs(&painter, vp_idx, view, img, &frame);
        } else if self.show_reference_lines && self.tiling.cell_count() > 1 {
            self.draw_reference_lines(&painter, vp_idx, img, &frame);
        }
        if let Some((a_vp, anchor_pos, _)) = self.orbit_anchor
            && a_vp == vp_idx {
                let clipped = painter.with_clip_rect(view);
                let col = Color32::from_rgb(255, 200, 75);
                clipped.circle_stroke(anchor_pos, 10.0, Stroke::new(1.5, col));
                clipped.circle_filled(anchor_pos, 2.5, col);
                clipped.line_segment(
                    [pos2(anchor_pos.x - 14.0, anchor_pos.y), pos2(anchor_pos.x + 14.0, anchor_pos.y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 160)),
                );
                clipped.line_segment(
                    [pos2(anchor_pos.x, anchor_pos.y - 14.0), pos2(anchor_pos.x, anchor_pos.y + 14.0)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 160)),
                );
                if let Some(cur) = response.interact_pointer_pos() {
                    clipped.line_segment(
                        [anchor_pos, cur],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 75, 140)),
                    );
                }
            }

        let unverified_geom = self.viewports.get(vp_idx).is_some_and(|v| v.mpr_enabled)
            && self
                .viewport_volume(vp_idx)
                .is_some_and(|v| !v.geometry_verified);
        if unverified_geom {
            painter.text(
                pos2(view.center().x, view.max.y - 26.0),
                Align2::CENTER_BOTTOM,
                "Geometry not verified: measurements disabled",
                FontId::proportional(11.5),
                Color32::from_rgb(255, 195, 80),
            );
        }
        if self.tiling.cell_count() > 1 && !self.ortho_mpr_active {
            let leader_idx = self.active_viewport.min(self.tiling.cell_count() - 1);
            if let (Some(leader_sel), Some(cur_sel)) = (
                self.viewports[leader_idx].selected,
                self.viewports[vp_idx].selected,
            )
                && leader_sel != cur_sel {
                    let align_text = if self.registrations_in_flight.contains(&(leader_sel, cur_sel))
                    {
                        Some(("Aligning…".to_string(), Color32::from_rgb(130, 210, 255)))
                    } else if let Some(reg) = self.registrations.get(&(leader_sel, cur_sel)) {
                        let same_for = matches!(
                            (
                                self.volumes.get(&leader_sel).and_then(|v| v.frame_of_reference_uid.as_deref()),
                                self.volumes.get(&cur_sel).and_then(|v| v.frame_of_reference_uid.as_deref()),
                            ),
                            (Some(a), Some(b)) if !a.is_empty() && a == b
                        );
                        if same_for {
                            Some((
                                "Same Frame of Reference".to_string(),
                                Color32::from_rgb(130, 220, 155),
                            ))
                        } else if reg.is_reliable() {
                            Some((
                                format!("Auto-aligned (approximate) · score {:.2}", reg.correlation),
                                Color32::from_rgb(130, 220, 155),
                            ))
                        } else {
                            Some((
                                "Not aligned: synced by position".to_string(),
                                Color32::from_rgb(255, 200, 80),
                            ))
                        }
                    } else {
                        None
                    };
                    if let Some((txt, col)) = align_text {
                        painter.text(
                            pos2(view.center().x, view.min.y + 42.0),
                            Align2::CENTER_TOP,
                            txt,
                            FontId::proportional(11.0),
                            col,
                        );
                    }
                }
        }
        self.draw_measures(&painter, vp_idx, view, img, &source_frame);
        if self.show_annotations {
            self.draw_scale_bar(&painter, vp_idx, view, img, &frame);
            self.draw_orientation_letters(&painter, vp_idx, view, &frame);
            self.draw_overlay(&painter, vp_idx, view, Some(&frame));
        }
        self.draw_scrubber_bar(ui, &painter, vp_idx, view);
        self.draw_viewport_border(&painter, vp_idx, view);
        self.draw_link_badge(ui, &painter, vp_idx, view);
        self.handle_series_drop(ui, &painter, vp_idx, view);
        self.viewport_context_menu(&response, vp_idx);
    }

    fn ortho_crosshair_screen_pos(
        &self,
        vp_idx: usize,
        img: Rect,
        frame: &Frame,
    ) -> Option<Pos2> {
        let pt_lps = self.ortho_crosshair_lps?;
        let plane = self.viewports.get(vp_idx)?.mpr_plane.as_ref()?;
        let slice_center = plane.slice_center_lps();
        let rel = sub3(pt_lps, slice_center);
        let u_mm = dot3(rel, plane.u_axis);
        let v_mm = dot3(rel, plane.v_axis);
        let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
        let sx = sx.max(1e-6);
        let sy = sy.max(1e-6);
        let px = (frame.width.saturating_sub(1) as f64) * 0.5 + u_mm / sx;
        let py = (frame.height.saturating_sub(1) as f64) * 0.5 + v_mm / sy;
        Some(self.pixel_to_screen(vp_idx, img, frame, pos2(px as f32, py as f32)))
    }

    fn handle_ortho_crosshair_drag(
        &mut self,
        vp_idx: usize,
        response: &egui::Response,
        view: Rect,
        frame: &Frame,
    ) -> bool {
        if !self.ortho_mpr_active || vp_idx >= 3 {
            return false;
        }
        if response.drag_stopped() && self.crosshair_drag == Some(vp_idx) {
            self.crosshair_drag = None;
            return true;
        }
        let img = self.image_rect(vp_idx, view, frame);
        let cross = self.ortho_crosshair_screen_pos(vp_idx, img, frame);
        let pointer_pos = response.interact_pointer_pos();
        let shift_held = response.ctx.input(|i| i.modifiers.shift);

        let start_drag = response.drag_started_by(egui::PointerButton::Primary)
            && (shift_held
                || pointer_pos.is_some_and(|p| cross.is_some_and(|c| c.distance(p) <= 18.0)));
        if start_drag {
            self.crosshair_drag = Some(vp_idx);
        }

        let clicked_near = response.clicked_by(egui::PointerButton::Primary)
            && (shift_held
                || pointer_pos.is_some_and(|p| cross.is_some_and(|c| c.distance(p) <= 18.0)));

        if (self.crosshair_drag == Some(vp_idx) || clicked_near)
            && let Some(pos) = pointer_pos
                && let Some(plane) = self.viewports.get(vp_idx).and_then(|v| v.mpr_plane.clone()) {
                    let px = self.screen_to_pixel(vp_idx, img, frame, pos);
                    let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
                    let sx = sx.max(1e-6);
                    let sy = sy.max(1e-6);
                    let u_mm = (px.x as f64 - (frame.width.saturating_sub(1) as f64) * 0.5) * sx;
                    let v_mm = (px.y as f64 - (frame.height.saturating_sub(1) as f64) * 0.5) * sy;
                    let new_lps = add3(
                        plane.slice_center_lps(),
                        add3(scale3(plane.u_axis, u_mm), scale3(plane.v_axis, v_mm)),
                    );
                    self.set_ortho_crosshair_lps(new_lps);
                    return true;
                }
        false
    }

    fn draw_ortho_crosshairs(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        img: Rect,
        frame: &Frame,
    ) {
        let Some(c) = self.ortho_crosshair_screen_pos(vp_idx, img, frame) else {
            return;
        };
        let clipped = painter.with_clip_rect(view);
        let (h_color, v_color, badge, badge_color) = match vp_idx {
            0 => (
                Color32::from_rgba_unmultiplied(74, 222, 128, 210), // Coronal (Green)
                Color32::from_rgba_unmultiplied(248, 113, 113, 210), // Sagittal (Red)
                "AXIAL",
                Color32::from_rgb(74, 168, 255),
            ),
            1 => (
                Color32::from_rgba_unmultiplied(74, 168, 255, 210), // Axial (Blue)
                Color32::from_rgba_unmultiplied(248, 113, 113, 210), // Sagittal (Red)
                "CORONAL",
                Color32::from_rgb(74, 222, 128),
            ),
            _ => (
                Color32::from_rgba_unmultiplied(74, 168, 255, 210), // Axial (Blue)
                Color32::from_rgba_unmultiplied(74, 222, 128, 210), // Coronal (Green)
                "SAGITTAL",
                Color32::from_rgb(248, 113, 113),
            ),
        };
        let gap = 10.0;
        // Horizontal line (with open center gap)
        if c.x - gap > view.min.x {
            clipped.line_segment(
                [pos2(view.min.x, c.y), pos2(c.x - gap, c.y)],
                Stroke::new(1.25, h_color),
            );
        }
        if c.x + gap < view.max.x {
            clipped.line_segment(
                [pos2(c.x + gap, c.y), pos2(view.max.x, c.y)],
                Stroke::new(1.25, h_color),
            );
        }
        // Vertical line (with open center gap)
        if c.y - gap > view.min.y {
            clipped.line_segment(
                [pos2(c.x, view.min.y), pos2(c.x, c.y - gap)],
                Stroke::new(1.25, v_color),
            );
        }
        if c.y + gap < view.max.y {
            clipped.line_segment(
                [pos2(c.x, c.y + gap), pos2(c.x, view.max.y)],
                Stroke::new(1.25, v_color),
            );
        }
        clipped.circle_stroke(c, 4.0, Stroke::new(1.25, badge_color));
        clipped.text(
            pos2(view.center().x, view.min.y + 26.0),
            Align2::CENTER_TOP,
            badge,
            FontId::proportional(12.0),
            badge_color,
        );
    }

    fn draw_scrubber_bar(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
    ) {
        let slice_count = self.viewport_slice_count(vp_idx);
        let Some((bar_rect, prev_rect, track_rect, next_rect)) =
            Self::viewport_scrubber_rects(view, slice_count)
        else {
            return;
        };
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        painter.rect_filled(
            bar_rect,
            11.0,
            Color32::from_rgba_unmultiplied(18, 22, 28, 210),
        );
        painter.rect_stroke(
            bar_rect,
            11.0,
            Stroke::new(1.0, Color32::from_gray(75)),
            egui::StrokeKind::Inside,
        );

        let prev_hover = hover_pos.is_some_and(|p| prev_rect.contains(p));
        let next_hover = hover_pos.is_some_and(|p| next_rect.contains(p));
        for (rect, icon, hovered) in [
            (prev_rect, crate::ui::icons::Icon::Previous, prev_hover),
            (next_rect, crate::ui::icons::Icon::Next, next_hover),
        ] {
            crate::ui::icons::paint(
                painter,
                Rect::from_center_size(rect.center(), vec2(9.0, 9.0)),
                icon,
                if hovered {
                    crate::ui::theme::accent_bright()
                } else {
                    Color32::from_gray(210)
                },
            );
        }

        let cy = track_rect.center().y;
        painter.line_segment(
            [pos2(track_rect.min.x, cy), pos2(track_rect.max.x, cy)],
            Stroke::new(3.0, Color32::from_gray(65)),
        );
        let t = (self.viewports[vp_idx].slice as f32
            / (slice_count.saturating_sub(1)).max(1) as f32)
            .clamp(0.0, 1.0);
        let thumb_x = track_rect.min.x + t * track_rect.width();
        painter.line_segment(
            [pos2(track_rect.min.x, cy), pos2(thumb_x, cy)],
            Stroke::new(3.0, crate::ui::theme::accent()),
        );
        painter.circle_filled(pos2(thumb_x, cy), 5.0, crate::ui::theme::accent_bright());
        painter.circle_stroke(
            pos2(thumb_x, cy),
            5.0,
            Stroke::new(1.0, Color32::from_gray(20)),
        );
    }

    fn draw_viewport_border(&self, painter: &egui::Painter, vp_idx: usize, view: Rect) {
        if self.tiling.cell_count() > 1 {
            let is_active = vp_idx == self.active_viewport;
            let color = if is_active {
                crate::ui::theme::accent_bright()
            } else {
                Color32::from_gray(55)
            };
            let width = if is_active { 1.5 } else { 1.0 };
            painter.rect_stroke(view, 0.0, Stroke::new(width, color), egui::StrokeKind::Inside);
        }
    }

    fn draw_link_badge(&mut self, ui: &egui::Ui, painter: &egui::Painter, vp_idx: usize, view: Rect) {
        let cell_count = self.tiling.cell_count();
        if cell_count <= 1 || self.ortho_mpr_active {
            return;
        }
        let any_link = self.any_link_enabled();
        let raw_group = self.viewports[vp_idx].link_group;
        let eff_group = if raw_group > 0 {
            raw_group
        } else if cell_count == 2 && any_link {
            1
        } else {
            0
        };
        if eff_group == 0 && !any_link {
            return;
        }

        let mut flags = Vec::new();
        if self.link_zoom {
            flags.push("Z");
        }
        if self.link_wl {
            flags.push("WL");
        }
        if self.link_cut {
            flags.push("Cut");
        }

        if eff_group > 0 && any_link {
            let col = Self::link_group_color(eff_group);
            painter.line_segment(
                [view.left_top() + vec2(2.0, 2.0), view.right_top() + vec2(-2.0, 2.0)],
                Stroke::new(2.5, col),
            );
            let label = format!("🔗{} {}", eff_group, flags.join("·"));
            let font = FontId::proportional(10.5);
            let galley = painter.layout_no_wrap(label.clone(), font.clone(), col);
            let pill_size = galley.size() + vec2(12.0, 4.0);
            let pill_rect = Rect::from_center_size(
                pos2(view.center().x, view.top() + 28.0),
                pill_size,
            );
            painter.rect_filled(pill_rect, 8.0, Color32::from_rgba_unmultiplied(12, 16, 22, 200));
            painter.rect_stroke(pill_rect, 8.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
            painter.text(pill_rect.center(), Align2::CENTER_CENTER, label, font, col);

            if cell_count > 2 {
                let resp = ui.interact(
                    pill_rect,
                    ui.scope_id().with(("vp_link_badge", vp_idx)),
                    Sense::click(),
                );
                if resp.clicked() {
                    self.viewports[vp_idx].link_group = (raw_group + 1) % 5;
                }
                resp.on_hover_text("Click to cycle Link Group (1–4 or Off)");
            }
        } else if cell_count > 2 && any_link {
            let col = Color32::from_gray(145);
            let label = "○ Link".to_string();
            let font = FontId::proportional(10.5);
            let galley = painter.layout_no_wrap(label.clone(), font.clone(), col);
            let pill_size = galley.size() + vec2(12.0, 4.0);
            let pill_rect = Rect::from_center_size(
                pos2(view.center().x, view.top() + 28.0),
                pill_size,
            );
            painter.rect_filled(pill_rect, 8.0, Color32::from_rgba_unmultiplied(12, 16, 22, 175));
            painter.rect_stroke(pill_rect, 8.0, Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Inside);
            painter.text(pill_rect.center(), Align2::CENTER_CENTER, label, font, col);
            let resp = ui.interact(
                pill_rect,
                ui.scope_id().with(("vp_link_badge", vp_idx)),
                Sense::click(),
            );
            if resp.clicked() {
                self.viewports[vp_idx].link_group = 1;
            }
            resp.on_hover_text("Click to assign this viewport to a Link Group");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gpu_mpr_callback(
        &self,
        ui: &egui::Ui,
        vp_idx: usize,
        volume: Arc<crate::volume::Volume3D>,
        geom: crate::volume::MprPlaneGeometry,
        img: Rect,
        window: Window,
        frame_inverts: bool,
    ) -> VolumeMprCallback {
        let ppp = ui.ctx().pixels_per_point();
        let vp = &self.viewports[vp_idx];
        let invert = vp.effective_invert() != frame_inverts;
        let clut = self.active_clut_table(vp_idx);
        let uv_map = QuadUvMapping::from_orientation(vp.rotation, vp.x_flipped, vp.y_flipped);
        VolumeMprCallback {
            viewport_id: vp_idx,
            volume,
            geom,
            image_rect_px: Rect::from_min_max(
                pos2(img.min.x * ppp, img.min.y * ppp),
                pos2(img.max.x * ppp, img.max.y * ppp),
            ),
            window,
            invert,
            clut,
            uv_map,
        }
    }

    fn render_vr_viewport(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        response: &egui::Response,
        vol: Arc<crate::volume::Volume3D>,
    ) {
        // Double-click resets 3D VR orbit & zoom
        if response.double_clicked() {
            self.viewports[vp_idx].reset_view();
        }

        // Scroll wheel zooms 3D VR camera
        let hovered = response.hovered();
        if hovered {
            let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_y.abs() > 0.1 {
                let factor = (scroll_y / 240.0).exp();
                let z = &mut self.viewports[vp_idx].zoom;
                *z = (*z * factor).clamp(0.25, 8.0);
                self.sync_zoom_from(vp_idx, factor);
            }
        }

        // Left-drag orbits 3D camera; Right-drag adjusts WL/WW
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            let vp = &mut self.viewports[vp_idx];
            vp.vr_yaw -= delta.x * 0.008;
            vp.vr_pitch = (vp.vr_pitch + delta.y * 0.008).clamp(-1.52, 1.52);
        } else if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            let base = if let Some(w) = self.viewports[vp_idx].window {
                Some(w)
            } else {
                self.viewport_frame(vp_idx).map(|f| self.effective_window(vp_idx, f))
            };
            if let Some(w) = base {
                let scale = (w.width.abs() / 250.0).max(0.5);
                let d_width = delta.x * scale;
                let d_level = -delta.y * scale;
                self.viewports[vp_idx].window = Some(Window {
                    level: w.level + d_level,
                    width: (w.width + d_width).max(1.0),
                });
                self.sync_window_delta(vp_idx, d_level, d_width);
            }
        }

        let ppp = ui.ctx().pixels_per_point();
        let rect_px = Rect::from_min_max(
            pos2(view.min.x * ppp, view.min.y * ppp),
            pos2(view.max.x * ppp, view.max.y * ppp),
        );
        let fallback_win = Window {
            level: 300.0,
            width: 1200.0,
        };
        let window = self.viewports[vp_idx]
            .window
            .or_else(|| self.viewport_frame(vp_idx).map(|f| self.effective_window(vp_idx, f)))
            .unwrap_or(fallback_win);
        let vp = &self.viewports[vp_idx];
        let invert = vp.effective_invert();
        let clut = self.active_clut_table(vp_idx);
        let vr_tf_lut =
            crate::imaging::vr_presets::build_lut_from_stops(&vp.vr_custom_stops, clut.as_ref());
        let cb = VolumeVrCallback {
            viewport_id: vp_idx,
            volume: vol,
            rect_px,
            window,
            invert,
            clut,
            yaw: vp.vr_yaw,
            pitch: vp.vr_pitch,
            zoom: vp.zoom,
            vr_mip: vp.vr_mip,
            vr_tf_lut,
            vr_shading: vp.vr_shading,
            vr_ambient: vp.vr_ambient,
            vr_diffuse: vp.vr_diffuse,
            vr_specular: vp.vr_specular,
            vr_clip_min: vp.vr_clip_min,
            vr_clip_max: vp.vr_clip_max,
            vr_bg_color: vp.vr_bg_color,
            rotation_deg: vp.rotation,
            x_flipped: vp.x_flipped,
            y_flipped: vp.y_flipped,
        };
        painter.add(cb.into_shape(view));

        // Clone `vol`? No, `vol` was moved into `cb`; read orientation from `self.viewport_volume(vp_idx)` for cube!
        if let Some(v_ref) = self.viewport_volume(vp_idx) {
            Self::draw_vr_orientation_cube(
                painter,
                view,
                v_ref,
                &self.viewports[vp_idx],
            );
        }

        if self.show_annotations {
            let font = FontId::proportional(11.5);
            let mode_str = if self.viewports[vp_idx].vr_mip {
                "3D MIP".to_string()
            } else {
                format!("3D VR · {}", self.viewports[vp_idx].vr_preset.label())
            };
            let tl = format!(
                "{mode_str}\nWL: {:.0}  WW: {:.0}\nZoom: {:.0}%  (Drag to orbit · Right-drag WL/WW)",
                window.level,
                window.width,
                self.viewports[vp_idx].zoom * 100.0
            );
            painter.text(
                view.left_top() + vec2(11.0, 11.0),
                Align2::LEFT_TOP,
                &tl,
                font.clone(),
                Color32::from_black_alpha(220),
            );
            painter.text(
                view.left_top() + vec2(10.0, 10.0),
                Align2::LEFT_TOP,
                tl,
                font,
                Color32::from_gray(232),
            );
        }

        self.draw_viewport_border(painter, vp_idx, view);
        self.draw_link_badge(ui, painter, vp_idx, view);
        self.handle_series_drop(ui, painter, vp_idx, view);
        self.viewport_context_menu(response, vp_idx);
    }

    fn draw_vr_orientation_cube(
        painter: &egui::Painter,
        view: Rect,
        vol: &crate::volume::Volume3D,
        vp: &ViewportCell,
    ) {
        if view.width() < 140.0 || view.height() < 140.0 {
            return;
        }
        let norm3 = |v: [f32; 3]| -> [f32; 3] {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-5);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let cross3 = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] };

        let v_left = norm3([
            vol.row_dir[0] as f32,
            vol.col_dir[0] as f32,
            vol.slice_dir[0] as f32,
        ]);
        let v_sup = norm3([
            vol.row_dir[2] as f32,
            vol.col_dir[2] as f32,
            vol.slice_dir[2] as f32,
        ]);
        let v_ant = norm3(cross3(v_left, v_sup));

        let (sy, cy) = vp.vr_yaw.sin_cos();
        let (sp, cp) = vp.vr_pitch.sin_cos();
        let cam_dir = [
            v_ant[0] * cy * cp + v_left[0] * sy * cp + v_sup[0] * sp,
            v_ant[1] * cy * cp + v_left[1] * sy * cp + v_sup[1] * sp,
            v_ant[2] * cy * cp + v_left[2] * sy * cp + v_sup[2] * sp,
        ];
        let fwd = norm3([-cam_dir[0], -cam_dir[1], -cam_dir[2]]);
        let right0 = norm3(cross3(fwd, v_sup));
        let up0 = norm3(cross3(right0, fwd));

        let rad = vp.rotation.to_radians();
        let (sr, cr) = rad.sin_cos();
        let r_rot = [
            right0[0] * cr + up0[0] * sr,
            right0[1] * cr + up0[1] * sr,
            right0[2] * cr + up0[2] * sr,
        ];
        let u_rot = [
            -right0[0] * sr + up0[0] * cr,
            -right0[1] * sr + up0[1] * cr,
            -right0[2] * sr + up0[2] * cr,
        ];
        let right = if vp.x_flipped {
            [-r_rot[0], -r_rot[1], -r_rot[2]]
        } else {
            r_rot
        };
        let up = if vp.y_flipped {
            [-u_rot[0], -u_rot[1], -u_rot[2]]
        } else {
            u_rot
        };

        let center = view.right_bottom() - vec2(40.0, 40.0);
        let radius = 20.0;
        let project = |v: [f32; 3]| -> (Pos2, f32) {
            let sx = dot(v, right) * radius;
            let sy = -dot(v, up) * radius;
            let sz = -dot(v, fwd);
            (center + vec2(sx, sy), sz)
        };

        // Draw subtle backdrop disc
        painter.circle_filled(
            center,
            28.0,
            Color32::from_rgba_unmultiplied(12, 16, 22, 175),
        );
        painter.circle_stroke(center, 28.0, Stroke::new(1.0, Color32::from_gray(55)));

        // Draw the 3 anatomical axes + 6 face labels sorted back-to-front
        let faces: [([f32; 3], &str, Color32); 6] = [
            (v_left, "L", Color32::from_rgb(120, 225, 160)),
            ([-v_left[0], -v_left[1], -v_left[2]], "R", Color32::from_rgb(120, 225, 160)),
            (v_ant, "A", Color32::from_rgb(255, 195, 95)),
            ([-v_ant[0], -v_ant[1], -v_ant[2]], "P", Color32::from_rgb(255, 195, 95)),
            (v_sup, "S", Color32::from_rgb(100, 195, 255)),
            ([-v_sup[0], -v_sup[1], -v_sup[2]], "I", Color32::from_rgb(100, 195, 255)),
        ];
        let mut projected: Vec<(Pos2, f32, &str, Color32)> = faces
            .iter()
            .map(|&(dir, lbl, col)| {
                let (p, z) = project(dir);
                (p, z, lbl, col)
            })
            .collect();
        projected.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (p, z, lbl, col) in projected {
            let alpha = if z >= 0.0 { 255 } else { 95 };
            let line_col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha);
            painter.line_segment([center, p], Stroke::new(if z >= 0.0 { 1.8 } else { 1.0 }, line_col));
            if z >= -0.15 {
                painter.text(
                    p,
                    Align2::CENTER_CENTER,
                    lbl,
                    FontId::proportional(10.5),
                    line_col,
                );
            }
        }
    }

    fn handle_series_drop(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
    ) {
        let Some(payload) = egui::DragAndDrop::payload::<(usize, usize)>(ui.ctx()) else {
            return;
        };
        let pointer_in = ui
            .input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()))
            .is_some_and(|p| view.contains(p));
        if !pointer_in {
            return;
        }
        painter.rect_stroke(
            view.shrink(2.0),
            0.0,
            Stroke::new(3.0, Color32::from_rgb(80, 200, 255)),
            egui::StrokeKind::Inside,
        );
        let drop_lbl = match self.lang {
            crate::i18n::Lang::Es => "Soltar para cargar serie",
            crate::i18n::Lang::En => "Drop to load series",
        };
        let pill = Rect::from_center_size(view.center(), vec2(170.0, 28.0));
        painter.rect_filled(pill, 6.0, Color32::from_rgba_unmultiplied(20, 65, 110, 225));
        painter.rect_stroke(
            pill,
            6.0,
            Stroke::new(1.5, Color32::from_rgb(100, 205, 255)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            pill.center(),
            Align2::CENTER_CENTER,
            drop_lbl,
            FontId::proportional(12.5),
            Color32::WHITE,
        );

        if ui.input(|i| i.pointer.any_released()) {
            let (si, ri) = *payload;
            egui::DragAndDrop::clear_payload(ui.ctx());
            self.active_viewport = vp_idx;
            self.assign_series_to_viewport(vp_idx, Some((si, ri)));
            if ui.input(|i| i.modifiers.shift) {
                self.viewports[vp_idx].mpr_enabled = true;
            }
        }
    }

    fn viewport_context_menu(&mut self, response: &egui::Response, vp_idx: usize) {
        response.context_menu(|ui| {
            self.active_viewport = vp_idx;
            let tr = crate::i18n::tr(self.lang);
            let copy_lbl = match self.lang {
                crate::i18n::Lang::Es => "📋 Copiar Imagen Anonimizada (Ctrl+C)",
                crate::i18n::Lang::En => "📋 Copy Anonymized Image (Ctrl+C)",
            };
            if ui.button(copy_lbl).clicked() {
                self.request_copy_active_viewport(ui.ctx());
                ui.close();
            }
            ui.menu_button(tr.export_menu, |ui| {
                if ui.button(tr.export_png).clicked() {
                    self.export_active_png();
                    ui.close();
                }
                if ui.button(tr.export_anon_dicom).clicked() {
                    self.export_active_anonymized_dicom();
                    ui.close();
                }
            });
            ui.separator();

            let has_series = self.viewports[vp_idx].selected.is_some();
            let has_vol = self.viewport_volume(vp_idx).is_some();
            if has_series {
                let mut mpr_on = self.viewports[vp_idx].mpr_enabled;
                if ui.toggle_value(&mut mpr_on, format!("🧊 {}", tr.mpr_3d)).changed() {
                    if mpr_on {
                        self.viewports[vp_idx].vr_enabled = false;
                        self.request_mpr_mode(vp_idx, false);
                    } else {
                        self.disable_viewport_mpr(vp_idx);
                    }
                    ui.close();
                }
                if has_vol && !self.ortho_mpr_active {
                    let mut vr_on = self.viewports[vp_idx].vr_enabled;
                    if ui.toggle_value(&mut vr_on, "🧊 VR").changed() {
                        self.viewports[vp_idx].vr_enabled = vr_on;
                        if vr_on {
                            self.disable_viewport_mpr(vp_idx);
                        }
                        ui.close();
                    }
                }
                let mut ortho_on = self.ortho_mpr_active;
                if ui.toggle_value(&mut ortho_on, format!("⊞ {}", tr.ortho_mpr)).changed() {
                    if ortho_on {
                        self.request_mpr_mode(vp_idx, true);
                    } else {
                        self.toggle_orthogonal_mpr();
                    }
                    ui.close();
                }
                ui.separator();
            }

            let presets_list: Vec<(String, Window)> = self
                .presets
                .hot_key_order()
                .into_iter()
                .filter_map(|e| Some((e.name.clone(), e.window?)))
                .collect();
            if !presets_list.is_empty() {
                ui.menu_button("WL / WW", |ui| {
                    if ui.button(tr.preset_default).clicked() {
                        self.set_active_window(None);
                        ui.close();
                    }
                    ui.separator();
                    for (name, w) in &presets_list {
                        if ui.button(name).clicked() {
                            self.set_active_window(Some(*w));
                            ui.close();
                        }
                    }
                });
            }

            let transform_lbl = match self.lang {
                crate::i18n::Lang::Es => "Transformar",
                crate::i18n::Lang::En => "Transform",
            };
            ui.menu_button(transform_lbl, |ui| {
                if ui.button(tr.rotate_ccw).clicked() {
                    let vp = &mut self.viewports[vp_idx];
                    vp.rotation = (vp.rotation - 90.0).rem_euclid(360.0);
                    ui.close();
                }
                if ui.button(tr.rotate_cw).clicked() {
                    let vp = &mut self.viewports[vp_idx];
                    vp.rotation = (vp.rotation + 90.0).rem_euclid(360.0);
                    ui.close();
                }
                let mut xf = self.viewports[vp_idx].x_flipped;
                if ui.toggle_value(&mut xf, format!("⇄ {}", tr.flip_h)).changed() {
                    self.viewports[vp_idx].x_flipped = xf;
                }
                let mut yf = self.viewports[vp_idx].y_flipped;
                if ui.toggle_value(&mut yf, format!("⇅ {}", tr.flip_v)).changed() {
                    self.viewports[vp_idx].y_flipped = yf;
                }
                ui.toggle_value(&mut self.viewports[vp_idx].invert, tr.invert);
            });

            if !self.viewports[vp_idx].vr_enabled
                && ui.button(format!("🗑 {}", tr.clear_rois)).clicked()
            {
                let cur_slice = self.viewports[vp_idx].slice;
                if let Some(key) = self.viewport_slice_key(vp_idx, cur_slice)
                    && let Some(sref) = self.index.as_ref().and_then(|idx| {
                        idx.studies
                            .get(key.study)?
                            .series
                            .get(key.series)?
                            .slices
                            .get(key.slice)
                    })
                {
                    let mk = (sref.path.clone(), sref.frame);
                    self.measures.remove(&mk);
                }
                ui.close();
            }
            ui.separator();
            if ui.button(format!("↺ {}", tr.reset_viewport)).clicked() {
                let vp = &mut self.viewports[vp_idx];
                vp.reset_view();
                vp.reset_clut();
                vp.invert = false;
                vp.mpr_enabled = false;
                vp.mpr_plane = None;
                vp.mpr_cached = None;
                self.set_active_window(None);
                ui.close();
            }
        });
    }

    fn gpu_callback(
        &self,
        ui: &egui::Ui,
        vp_idx: usize,
        frame: Arc<Frame>,
        key: SliceKey,
        img: Rect,
        window: Window,
    ) -> ImageCallback {
        let ppp = ui.ctx().pixels_per_point();
        let vp = &self.viewports[vp_idx];
        let invert = vp.effective_invert() != frame.inverts;
        let clut = self.active_clut_table(vp_idx);
        let uv_map = QuadUvMapping::from_orientation(vp.rotation, vp.x_flipped, vp.y_flipped);
        ImageCallback {
            viewport_id: vp_idx,
            frame,
            key,
            image_rect_px: Rect::from_min_max(
                pos2(img.min.x * ppp, img.min.y * ppp),
                pos2(img.max.x * ppp, img.max.y * ppp),
            ),
            window,
            invert,
            clut,
            uv_map,
            bilinear: self.gpu_bilinear,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_on_cpu(
        &mut self,
        ui: &egui::Ui,
        vp_idx: usize,
        painter: &egui::Painter,
        frame: &Frame,
        slice_key: SliceKey,
        img: Rect,
        window: Window,
    ) {
        let invert = self.viewports[vp_idx].effective_invert();
        let clut_name = self.viewports[vp_idx]
            .active_clut_name()
            .map(str::to_owned);
        let convolution_idx = self.viewports[vp_idx].convolution_idx;
        let clut = self.active_clut_table(vp_idx);
        let key = RenderKey {
            key: slice_key,
            center: window.level.to_bits() as u64,
            width: window.width.to_bits() as u64,
            invert,
            clut_name,
            convolution_idx,
        };
        let vp = &mut self.viewports[vp_idx];
        if vp.rendered.as_ref() != Some(&key) || vp.texture.is_none() {
            let rgba = frame.render_with_clut(window, invert, clut.as_ref());
            let image = ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &rgba,
            );
            match &mut vp.texture {
                Some(t) => t.set(image, TextureOptions::LINEAR),
                None => {
                    let name = format!("slice-vp-{vp_idx}");
                    vp.texture = Some(ui.ctx().load_texture(name, image, TextureOptions::LINEAR));
                }
            }
            vp.rendered = Some(key);
        }
        if let Some(tex) = &vp.texture {
            painter.image(
                tex.id(),
                img,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        vp.last_gpu = None;
    }

    fn handle_wheel(&mut self, ui: &egui::Ui, vp_idx: usize, response: &egui::Response) {
        if !response.hovered() {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            {
                let lines = match unit {
                    egui::MouseWheelUnit::Line => delta.y,
                    egui::MouseWheelUnit::Point => delta.y / 40.0,
                    egui::MouseWheelUnit::Page => delta.y * 10.0,
                };
                if modifiers.ctrl || modifiers.command {
                    let z = &mut self.viewports[vp_idx].zoom;
                    *z = (*z * (1.0 + 0.1 * lines)).clamp(0.05, 50.0);
                } else {
                    self.viewports[vp_idx].wheel_accum += lines;
                }
            }
        }
        let steps = self.viewports[vp_idx].wheel_accum.trunc();
        if steps != 0.0 {
            self.viewports[vp_idx].wheel_accum -= steps;
            self.active_viewport = vp_idx;
            self.step_viewport_slice(vp_idx, -steps as isize);
        }
    }

    fn handle_drag(
        &mut self,
        vp_idx: usize,
        response: &egui::Response,
        view: Rect,
        frame: &Frame,
    ) {
        if response.double_clicked() {
            if self.tool == Tool::Polygon && self.drawing.len() >= 3 {
                let pts = std::mem::take(&mut self.drawing);
                if let Some(k) = self.viewport_measure_key(vp_idx) {
                    self.measures
                        .entry(k.clone())
                        .or_default()
                        .push(Measure::Polygon(pts));
                    self.persist_measures_for(&k);
                }
            } else {
                self.viewports[vp_idx].reset_view();
                if self.ortho_mpr_active || self.link_zoom {
                    let count = self.tiling.cell_count();
                    for i in 0..count {
                        if self.are_viewports_linked(vp_idx, i) && self.viewports[i].selected.is_some() {
                            self.viewports[i].reset_view();
                        }
                    }
                }
            }
            return;
        }
        let delta = response.drag_delta();
        let img = self.image_rect(vp_idx, view, frame);
        let pointer_px = response
            .interact_pointer_pos()
            .map(|p| self.screen_to_pixel(vp_idx, img, frame, p));

        if response.dragged_by(egui::PointerButton::Middle) {
            self.viewports[vp_idx].pan += delta;
            self.sync_pan_from(vp_idx, delta);
            return;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let factor = (-delta.y / 200.0).exp();
            let z = &mut self.viewports[vp_idx].zoom;
            *z = (*z * factor).clamp(0.05, 50.0);
            self.sync_zoom_from(vp_idx, factor);
            return;
        }

        let is_drag =
            response.dragged_by(egui::PointerButton::Primary) || response.drag_stopped();
        let is_click = response.clicked();
        if !is_drag && !is_click {
            return;
        }

        match self.tool {
            Tool::WindowLevel => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    || self.wl_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        let from = self.effective_window(vp_idx, frame);
                        self.wl_drag =
                            Some((vp_idx, start, drag::Drag::new(drag::Variant::Image, from)));
                    }
                }
                if let (Some((id, start, d)), Some(now)) =
                    (self.wl_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let prev_w = self.effective_window(vp_idx, frame);
                        let dx = (now.x - start.x) as f64;
                        let dy = (start.y - now.y) as f64;
                        let next_w = d.window(dx, dy, None, 0, frame.slope);
                        self.viewports[vp_idx].window = Some(next_w);
                        let d_level = next_w.level - prev_w.level;
                        let d_width = next_w.width - prev_w.width;
                        if d_level.abs() > 1e-6 || d_width.abs() > 1e-6 {
                            self.sync_window_delta(vp_idx, d_level, d_width);
                        }
                    }
                if response.drag_stopped() {
                    self.wl_drag = None;
                }
            }
            Tool::Scroll => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    || self.scroll_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        self.scroll_drag = Some((vp_idx, start.y, self.viewports[vp_idx].slice));
                    }
                }
                if let (Some((id, start_y, start_slice)), Some(now)) =
                    (self.scroll_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let n = self.viewport_slice_count(vp_idx);
                        if n > 1 {
                            let px_per_slice = (view.height() * 0.75 / n as f32).clamp(2.5, 14.0);
                            let delta_slices = ((now.y - start_y) / px_per_slice).round() as isize;
                            self.set_viewport_slice(
                                vp_idx,
                                start_slice as isize + delta_slices,
                                true,
                            );
                        }
                    }
                if response.drag_stopped() {
                    self.scroll_drag = None;
                }
            }
            Tool::Pan => {
                if is_drag {
                    self.viewports[vp_idx].pan += delta;
                    self.sync_pan_from(vp_idx, delta);
                }
            }
            Tool::Zoom => {
                if is_drag {
                    let factor = (-delta.y / 200.0).exp();
                    let z = &mut self.viewports[vp_idx].zoom;
                    *z = (*z * factor).clamp(0.05, 50.0);
                    self.sync_zoom_from(vp_idx, factor);
                }
            }
            Tool::Rotate => {
                if !is_drag {
                    return;
                }
                let center = img.center();
                if response.drag_started()
                    || self.rot_drag.map(|(id, _, _)| id) != Some(vp_idx)
                {
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(start) = origin.or(response.interact_pointer_pos()) {
                        let a0 = (start.y - center.y).atan2(start.x - center.x).to_degrees();
                        self.rot_drag = Some((vp_idx, a0, self.viewports[vp_idx].rotation));
                    }
                }
                if let (Some((id, a0, init_rot)), Some(now)) =
                    (self.rot_drag, response.interact_pointer_pos())
                    && id == vp_idx {
                        let a1 = (now.y - center.y).atan2(now.x - center.x).to_degrees();
                        self.viewports[vp_idx].rotation =
                            (init_rot + (a1 - a0)).rem_euclid(360.0);
                    }
                if response.drag_stopped() {
                    self.rot_drag = None;
                }
            }
            Tool::Orbit3D => {
                if !is_drag {
                    return;
                }
                if response.drag_started()
                    && let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                        let press_pos = response
                            .ctx
                            .input(|i| i.pointer.press_origin())
                            .or(response.interact_pointer_pos())
                            .unwrap_or_else(|| view.center());
                        let px = self.screen_to_pixel(vp_idx, img, frame, press_pos);
                        let (sy, sx) = frame.spacing.unwrap_or((1.0, 1.0));
                        let u_mm = (px.x as f64 - (frame.width.saturating_sub(1) as f64) * 0.5)
                            * sx.max(1e-6);
                        let v_mm = (px.y as f64 - (frame.height.saturating_sub(1) as f64) * 0.5)
                            * sy.max(1e-6);
                        let anchor_lps = add3(
                            plane.slice_center_lps(),
                            add3(scale3(plane.u_axis, u_mm), scale3(plane.v_axis, v_mm)),
                        );
                        self.orbit_anchor = Some((vp_idx, press_pos, anchor_lps));
                        plane.offset_mm = dot3(sub3(anchor_lps, plane.center_lps), plane.normal);
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                if delta.length_sq() > 0.0
                    && let Some(mut plane) = self.effective_mpr_plane(vp_idx) {
                        let (shift, alt) = response
                            .ctx
                            .input(|i| (i.modifiers.shift, i.modifiers.alt));
                        let speed = if alt { 0.08 } else { 0.18 };
                        if shift {
                            plane.orbit_3d(0.0, 0.0, delta.x as f64 * speed);
                        } else {
                            plane.orbit_3d(delta.x as f64 * speed, -delta.y as f64 * speed, 0.0);
                        }
                        if let Some((a_vp, _, anchor_lps)) = self.orbit_anchor
                            && a_vp == vp_idx {
                                plane.offset_mm =
                                    dot3(sub3(anchor_lps, plane.center_lps), plane.normal);
                            }
                        self.set_viewport_mpr_plane(vp_idx, plane, true);
                    }
                if response.drag_stopped() {
                    self.orbit_drag = None;
                    self.orbit_anchor = None;
                }
            }
            Tool::Length => {
                let cur_px = pointer_px.or(self.hover_px);
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(b) > 2.0 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Length(a, b));
                                    self.persist_measures_for(&k);
                                }
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, b) = (self.drawing[0], self.drawing[1]);
                        if a.distance(b) > 3.0 {
                            if let Some(k) = self.viewport_measure_key(vp_idx) {
                                self.measures
                                    .entry(k.clone())
                                    .or_default()
                                    .push(Measure::Length(a, b));
                                self.persist_measures_for(&k);
                            }
                            self.drawing.clear();
                        } else {
                            // Tiny movement on first click: keep start point and wait for second click.
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Angle => {
                let cur_px = pointer_px.or(self.hover_px);
                // Step 2: First arm (a -> v) is already placed in `angle_pending`.
                // Commit the angle on either click or drag-release of the third point `b`.
                if let Some((a, v)) = self.angle_pending {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px
                            && v.distance(b) > 2.0 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Angle(a, v, b));
                                    self.persist_measures_for(&k);
                                }
                                self.angle_pending = None;
                                self.drawing.clear();
                            }
                    return;
                }

                // Step 1: Placing the first arm (a -> v) via either 2 clicks or click-and-drag.
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(v) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(v) > 2.0 {
                                self.angle_pending = Some((a, v));
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, v) = (self.drawing[0], self.drawing[1]);
                        if a.distance(v) > 3.0 {
                            self.angle_pending = Some((a, v));
                            self.drawing.clear();
                        } else {
                            // Tiny movement on first click: keep start point `a` and wait for click at vertex `v`.
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Oval | Tool::Rect => {
                let is_oval = self.tool == Tool::Oval;
                let cur_px = pointer_px.or(self.hover_px);
                if self.drawing.is_empty() {
                    if response.drag_started() {
                        let origin = response.ctx.input(|i| i.pointer.press_origin());
                        if let Some(p) = origin.or(response.interact_pointer_pos()) {
                            let px = self.screen_to_pixel(vp_idx, img, frame, p);
                            self.drawing.push(px);
                            self.drawing.push(px);
                        }
                    } else if is_click
                        && let Some(px) = cur_px {
                            self.drawing.push(px);
                        }
                } else if self.drawing.len() == 1 {
                    if (is_click || response.drag_stopped())
                        && let Some(b) = cur_px {
                            let a = self.drawing[0];
                            if a.distance(b) > 2.5 {
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    let m = if is_oval {
                                        Measure::Oval(a, b)
                                    } else {
                                        Measure::Rect(a, b)
                                    };
                                    self.measures.entry(k.clone()).or_default().push(m);
                                    self.persist_measures_for(&k);
                                }
                                self.drawing.clear();
                            }
                        }
                } else if self.drawing.len() == 2 {
                    if let Some(p) = cur_px {
                        self.drawing[1] = p;
                    }
                    if response.drag_stopped() {
                        let (a, b) = (self.drawing[0], self.drawing[1]);
                        if a.distance(b) > 3.0 {
                            if let Some(k) = self.viewport_measure_key(vp_idx) {
                                let m = if is_oval {
                                    Measure::Oval(a, b)
                                } else {
                                    Measure::Rect(a, b)
                                };
                                self.measures.entry(k.clone()).or_default().push(m);
                                self.persist_measures_for(&k);
                            }
                            self.drawing.clear();
                        } else {
                            self.drawing.truncate(1);
                        }
                    }
                }
            }
            Tool::Polygon => {
                let cur_px = pointer_px.or(self.hover_px);
                if is_click
                    && let Some(px) = cur_px {
                        // Close polygon if clicking near the first vertex and we already have >= 3 points.
                        if self.drawing.len() >= 3 {
                            let first_scr = self.pixel_to_screen(vp_idx, img, frame, self.drawing[0]);
                            let cur_scr = self.pixel_to_screen(vp_idx, img, frame, px);
                            if first_scr.distance(cur_scr) <= 10.0 {
                                let pts = std::mem::take(&mut self.drawing);
                                if let Some(k) = self.viewport_measure_key(vp_idx) {
                                    self.measures
                                        .entry(k.clone())
                                        .or_default()
                                        .push(Measure::Polygon(pts));
                                    self.persist_measures_for(&k);
                                }
                                return;
                            }
                        }
                        self.drawing.push(px);
                    }
            }
            Tool::Pencil => {
                let cur_px = pointer_px.or(self.hover_px);
                if response.drag_started() {
                    self.drawing.clear();
                    let origin = response.ctx.input(|i| i.pointer.press_origin());
                    if let Some(p) = origin.or(response.interact_pointer_pos()) {
                        self.drawing.push(self.screen_to_pixel(vp_idx, img, frame, p));
                    }
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && let Some(px) = cur_px {
                        let add = self
                            .drawing
                            .last()
                            .is_none_or(|last| last.distance(px) >= 1.5);
                        if add {
                            self.drawing.push(px);
                        }
                    }
                if response.drag_stopped() {
                    if self.drawing.len() >= 3 {
                        let pts = std::mem::take(&mut self.drawing);
                        if let Some(k) = self.viewport_measure_key(vp_idx) {
                            self.measures
                                .entry(k.clone())
                                .or_default()
                                .push(Measure::Pencil(pts));
                            self.persist_measures_for(&k);
                        }
                    } else {
                        self.drawing.clear();
                    }
                }
            }
            Tool::Point => {
                let cur_px = pointer_px.or(self.hover_px);
                if (is_click || response.drag_stopped())
                    && let Some(px) = cur_px
                        && let Some(k) = self.viewport_measure_key(vp_idx) {
                            self.measures
                                .entry(k.clone())
                                .or_default()
                                .push(Measure::Point(px));
                            self.persist_measures_for(&k);
                        }
            }
        }
    }
}
