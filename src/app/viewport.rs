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
        // Swap aspect ratio when rotated near 90Â° or 270Â°.
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
        let bar_h = 22.0;
        let bar_rect = Rect::from_center_size(
            pos2(view.center().x, view.bottom() - 16.0),
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

        let mpr_pair = self.ensure_mpr_frame(vp_idx);
        let cur_slice = self.viewports[vp_idx].slice;
        let cur_key = mpr_pair
            .as_ref()
            .map(|(_, k)| *k)
            .or_else(|| self.viewport_slice_key(vp_idx, cur_slice));
        let frame_opt = mpr_pair
            .map(|(f, _)| f)
            .or_else(|| self.viewport_frame(vp_idx).cloned());
        let Some(mut frame) = frame_opt else {
            let text = match cur_key.and_then(|k| self.failed.get(&k)) {
                Some(err) => format!("Could not display this image:\n{err}"),
                None => "Loadingâ€¦".to_string(),
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
            if let Some((updated_f, updated_k)) = self.ensure_mpr_frame(vp_idx) {
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

        // Live cursor readout with calibrated HU / pixel value and 3D patient coordinates.
        if let Some(p) = response.hover_pos() {
            let px = self.screen_to_pixel(vp_idx, img, &frame, p);
            if vp_idx == self.active_viewport {
                self.hover_px = Some(px);
            }
            if px.x >= 0.0 && px.y >= 0.0 {
                let (x, y) = (px.x as u32, px.y as u32);
                if let Some(val_str) = source_frame.value_at(x, y) {
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
                        Some(("Aligningâ€¦".to_string(), Color32::from_rgb(130, 210, 255)))
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
                                format!("Auto-aligned (approximate) Â· score {:.2}", reg.correlation),
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
        self.draw_measures(&painter, vp_idx, img, &source_frame);
        if self.show_annotations {
            self.draw_scale_bar(&painter, vp_idx, view, img, &frame);
            self.draw_orientation_letters(&painter, vp_idx, view, &frame);
            self.draw_overlay(&painter, vp_idx, view, Some(&frame));
        }
        self.draw_scrubber_bar(ui, &painter, vp_idx, view);
        self.draw_viewport_border(&painter, vp_idx, view);
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
        painter.text(
            prev_rect.center(),
            Align2::CENTER_CENTER,
            "â—€",
            FontId::proportional(11.0),
            if prev_hover {
                Color32::from_rgb(110, 190, 255)
            } else {
                Color32::from_gray(210)
            },
        );
        painter.text(
            next_rect.center(),
            Align2::CENTER_CENTER,
            "â–¶",
            FontId::proportional(11.0),
            if next_hover {
                Color32::from_rgb(110, 190, 255)
            } else {
                Color32::from_gray(210)
            },
        );

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
            Stroke::new(3.0, Color32::from_rgb(70, 165, 255)),
        );
        painter.circle_filled(pos2(thumb_x, cy), 5.0, Color32::from_rgb(120, 195, 255));
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
                Color32::from_rgb(70, 165, 255)
            } else {
                Color32::from_gray(55)
            };
            let width = if is_active { 1.5 } else { 1.0 };
            painter.rect_stroke(view, 0.0, Stroke::new(width, color), egui::StrokeKind::Inside);
        }
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
                if self.should_sync_viewports() {
                    let count = self.tiling.cell_count();
                    for (i, vp) in self.viewports.iter_mut().enumerate().take(count) {
                        if i != vp_idx && vp.selected.is_some() {
                            vp.reset_view();
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
