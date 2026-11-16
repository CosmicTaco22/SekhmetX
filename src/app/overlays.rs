use super::*;

impl ViewerApp {
    pub(super) fn draw_reference_lines(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        img: Rect,
        self_frame: &Frame,
    ) {
        if !self_frame.is_3d_plane {
            return;
        }
        let Some(self_se) = self.viewport_series(vp_idx) else {
            return;
        };
        let self_sel = self.viewports[vp_idx].selected;
        let (self_rs, self_cs) = self_frame.spacing.unwrap_or((1.0, 1.0));
        let self_orient = self_frame.orientation.map(|v| v as f32);
        let self_origin = self_frame.origin.unwrap_or([0.0; 3]).map(|v| v as f32);

        // Which other viewport's plane(s) should we project onto `vp_idx`?
        // - If `vp_idx` is a follower viewport, project the active viewport's current slice
        //   (plus its first/last bounding slices).
        // - If `vp_idx` is the active viewport, project any non-parallel follower viewport's current slice.
        let cell_count = self.tiling.cell_count();
        for other_idx in 0..cell_count {
            if other_idx == vp_idx {
                continue;
            }
            if vp_idx != self.active_viewport && other_idx != self.active_viewport {
                continue;
            }
            let Some(other_se) = self.viewport_series(other_idx) else {
                continue;
            };
            let (other_orient, other_origin, other_rs, other_cs, other_cols, other_rows, other_is_3d) =
                if let Some(of) = self.viewport_frame(other_idx) {
                    let (rs, cs) = of.spacing.unwrap_or((1.0, 1.0));
                    (
                        of.orientation.map(|v| v as f32),
                        of.origin.unwrap_or([0.0; 3]).map(|v| v as f32),
                        rs,
                        cs,
                        of.width as isize,
                        of.height as isize,
                        of.is_3d_plane,
                    )
                } else {
                    let other_slice_idx = self.viewports[other_idx].slice;
                    let Some(other_cur) = other_se.slices.get(other_slice_idx) else {
                        continue;
                    };
                    let (rs, cs) = other_cur.spacing.unwrap_or((1.0, 1.0));
                    (
                        other_cur.orientation.map(|v| v as f32),
                        other_cur.position.unwrap_or([0.0; 3]).map(|v| v as f32),
                        rs,
                        cs,
                        other_se.columns as isize,
                        other_se.rows as isize,
                        other_cur.is_3d_plane,
                    )
                };
            if !other_is_3d {
                continue;
            }
            let same_for = match (
                &self_se.frame_of_reference_uid,
                &other_se.frame_of_reference_uid,
            ) {
                (Some(a), Some(b)) => a == b,
                (None, None) => {
                    self_sel.map(|s| s.0) == self.viewports[other_idx].selected.map(|s| s.0)
                }
                _ => false,
            };

            let pair = ReferenceLines::compute(
                same_for,
                || PlaneGeometry::plane_normal_difference_degrees(&self_orient, &other_orient),
                || {
                    let line_a = PlaneGeometry::slice_intersection(
                        self_origin,
                        &self_orient,
                        self_cs,
                        self_rs,
                        self_frame.is_3d_plane,
                        other_origin,
                        &other_orient,
                        other_cs,
                        other_rs,
                        other_cols,
                        other_rows,
                        other_is_3d,
                    )
                    .map(|(a, b)| [a, b])?;
                    // Also compute first/last bounding slice intersection when `other` is the active stack.
                    let line_b = if other_idx == self.active_viewport
                        && !self.viewports[other_idx].mpr_enabled
                        && other_se.slices.len() > 2
                    {
                        let first = other_se.slices.first()?;
                        let last = other_se.slices.last()?;
                        let first_origin = first.position.unwrap_or([0.0; 3]).map(|v| v as f32);
                        let last_origin = last.position.unwrap_or([0.0; 3]).map(|v| v as f32);
                        let first_orient = first.orientation.map(|v| v as f32);
                        let last_orient = last.orientation.map(|v| v as f32);
                        let (first_rs, first_cs) = first.spacing.unwrap_or((other_rs, other_cs));
                        let (last_rs, last_cs) = last.spacing.unwrap_or((other_rs, other_cs));
                        let l_first = PlaneGeometry::slice_intersection(
                            self_origin,
                            &self_orient,
                            self_cs,
                            self_rs,
                            self_frame.is_3d_plane,
                            first_origin,
                            &first_orient,
                            first_cs,
                            first_rs,
                            other_se.columns as isize,
                            other_se.rows as isize,
                            first.is_3d_plane,
                        );
                        let l_last = PlaneGeometry::slice_intersection(
                            self_origin,
                            &self_orient,
                            self_cs,
                            self_rs,
                            self_frame.is_3d_plane,
                            last_origin,
                            &last_orient,
                            last_cs,
                            last_rs,
                            other_se.columns as isize,
                            other_se.rows as isize,
                            last.is_3d_plane,
                        )
                        .map(|(a, b)| [a, b]);
                        // Draw the first bounding slice directly if available.
                        if let Some((lf0, lf1)) = l_first {
                            let p0 = self.pixel_to_screen(vp_idx, img, self_frame, pos2(lf0[0], lf0[1]));
                            let p1 = self.pixel_to_screen(vp_idx, img, self_frame, pos2(lf1[0], lf1[1]));
                            Self::draw_dashed_line(
                                painter,
                                p0,
                                p1,
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 200, 255, 130)),
                            );
                        }
                        l_last
                    } else {
                        None
                    };
                    Some((line_a, line_b))
                },
            );

            if pair.show_b {
                let p0 = self.pixel_to_screen(
                    vp_idx,
                    img,
                    self_frame,
                    pos2(pair.b[0][0], pair.b[0][1]),
                );
                let p1 = self.pixel_to_screen(
                    vp_idx,
                    img,
                    self_frame,
                    pos2(pair.b[1][0], pair.b[1][1]),
                );
                Self::draw_dashed_line(
                    painter,
                    p0,
                    p1,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 200, 255, 130)),
                );
            }
            if pair.show_a {
                let p0 = self.pixel_to_screen(
                    vp_idx,
                    img,
                    self_frame,
                    pos2(pair.a[0][0], pair.a[0][1]),
                );
                let p1 = self.pixel_to_screen(
                    vp_idx,
                    img,
                    self_frame,
                    pos2(pair.a[1][0], pair.a[1][1]),
                );
                let color = if other_idx == self.active_viewport {
                    Color32::from_rgb(80, 235, 140)
                } else {
                    Color32::from_rgba_unmultiplied(255, 200, 80, 170)
                };
                painter.line_segment([p0, p1], Stroke::new(1.5, color));
            }
        }
    }

    pub(super) fn draw_dashed_line(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
        let dir = b - a;
        let len = dir.length();
        if len < 1.0 {
            return;
        }
        let u = dir / len;
        let dash = 6.0;
        let gap = 4.0;
        let mut t = 0.0;
        while t < len {
            let t_end = (t + dash).min(len);
            painter.line_segment([a + u * t, a + u * t_end], stroke);
            t += dash + gap;
        }
    }

    pub(super) fn draw_orientation_letters(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        frame: &Frame,
    ) {
        if !frame.is_3d_plane {
            return;
        }
        let vp = &self.viewports[vp_idx];
        let corrected = ImagePlaneCoordinates::corrected_orientation_standard(
            &frame.orientation,
            vp.rotation,
            vp.x_flipped,
            vp.y_flipped,
        );
        let edges = OrientationLetters::edge_letters(&corrected);
        let font = FontId::proportional(14.0);
        let color = Color32::from_rgb(255, 215, 90);
        let draw = |pos: Pos2, align: Align2, s: &str| {
            if !s.is_empty() {
                painter.text(pos + vec2(1.0, 1.0), align, s, font.clone(), Color32::BLACK);
                painter.text(pos, align, s, font.clone(), color);
            }
        };
        let pad = 8.0;
        let bottom_pad = if Self::viewport_scrubber_rects(view, self.viewport_slice_count(vp_idx)).is_some() {
            42.0
        } else {
            pad
        };
        draw(
            pos2(view.center().x, view.top() + pad),
            Align2::CENTER_TOP,
            &edges.top,
        );
        draw(
            pos2(view.center().x, view.bottom() - bottom_pad),
            Align2::CENTER_BOTTOM,
            &edges.bottom,
        );
        draw(
            pos2(view.left() + pad + 12.0, view.center().y),
            Align2::LEFT_CENTER,
            &edges.left,
        );
        draw(
            pos2(view.right() - pad, view.center().y),
            Align2::RIGHT_CENTER,
            &edges.right,
        );
    }

    pub(super) fn draw_scale_bar(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        img: Rect,
        frame: &Frame,
    ) {
        let Some((rs, cs)) = frame.spacing else {
            return;
        };
        if rs <= 0.0 || cs <= 0.0 || view.height() < 160.0 {
            return;
        }
        let vp = &self.viewports[vp_idx];
        let rot = vp.rotation.rem_euclid(180.0);
        let vertical_mm = if (45.0..135.0).contains(&rot) {
            frame.width as f64 * cs
        } else {
            frame.height as f64 * rs
        };
        if vertical_mm <= 0.0 {
            return;
        }
        let px_per_mm = img.height() as f64 / vertical_mm;
        if px_per_mm <= 0.05 {
            return;
        }

        let ruler_col = Color32::from_rgb(48, 209, 88);
        let stroke = Stroke::new(1.2, ruler_col);

        // Left-edge vertical green ruler (OsiriX style)
        let target_v_px = (view.height() * 0.38) as f64;
        let candidates_mm = [10.0, 20.0, 50.0, 100.0, 200.0];
        let v_bar_mm = candidates_mm
            .into_iter()
            .min_by(|a, b| {
                ((a * px_per_mm) - target_v_px)
                    .abs()
                    .total_cmp(&((b * px_per_mm) - target_v_px).abs())
            })
            .unwrap_or(50.0);
        let v_bar_px = (v_bar_mm * px_per_mm) as f32;
        if v_bar_px >= 24.0 && v_bar_px <= view.height() * 0.72 {
            let x = view.left() + 5.0;
            let y_top = view.center().y - v_bar_px * 0.5;
            let y_bot = view.center().y + v_bar_px * 0.5;
            painter.line_segment([pos2(x, y_top), pos2(x, y_bot)], stroke);

            let ticks = if v_bar_mm >= 50.0 { 10 } else { 4 };
            for i in 0..=ticks {
                let ty = y_top + (i as f32 / ticks as f32) * v_bar_px;
                let is_major = i == 0 || i == ticks || (ticks >= 10 && i % 2 == 0);
                let tick_w = if is_major { 6.5 } else { 3.5 };
                painter.line_segment([pos2(x, ty), pos2(x + tick_w, ty)], stroke);
            }

            let label = if v_bar_mm >= 10.0 {
                format!("{:.0} cm", v_bar_mm / 10.0)
            } else {
                format!("{v_bar_mm:.0} mm")
            };
            painter.text(
                pos2(x + 2.0, y_bot + 3.0),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(10.0),
                ruler_col,
            );
        }

        // Bottom-edge horizontal green ruler (OsiriX style)
        let horiz_mm = if (45.0..135.0).contains(&rot) {
            frame.height as f64 * rs
        } else {
            frame.width as f64 * cs
        };
        if horiz_mm > 0.0 && view.width() >= 220.0 {
            let h_px_per_mm = img.width() as f64 / horiz_mm;
            let target_h_px = (view.width() * 0.34) as f64;
            let h_bar_mm = candidates_mm
                .into_iter()
                .min_by(|a, b| {
                    ((a * h_px_per_mm) - target_h_px)
                        .abs()
                        .total_cmp(&((b * h_px_per_mm) - target_h_px).abs())
                })
                .unwrap_or(50.0);
            let h_bar_px = (h_bar_mm * h_px_per_mm) as f32;
            if h_bar_px >= 24.0 && h_bar_px <= view.width() * 0.60 {
                let y = view.bottom() - 4.0;
                let x_left = view.center().x - h_bar_px * 0.5;
                let x_right = view.center().x + h_bar_px * 0.5;
                painter.line_segment([pos2(x_left, y), pos2(x_right, y)], stroke);

                let ticks = if h_bar_mm >= 50.0 { 10 } else { 4 };
                for i in 0..=ticks {
                    let tx = x_left + (i as f32 / ticks as f32) * h_bar_px;
                    let is_major = i == 0 || i == ticks || (ticks >= 10 && i % 2 == 0);
                    let tick_h = if is_major { 6.0 } else { 3.2 };
                    painter.line_segment([pos2(tx, y - tick_h), pos2(tx, y)], stroke);
                }
            }
        }
    }

    pub(super) fn draw_measures(
        &mut self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        img: Rect,
        f: &Frame,
    ) {
        let color = Color32::from_rgb(255, 220, 60);
        let roi_color = Color32::from_rgb(90, 235, 130);
        let stroke = Stroke::new(1.6, color);
        let roi_stroke = Stroke::new(1.6, roi_color);
        let s = |p: Pos2| self.pixel_to_screen(vp_idx, img, f, p);

        let safe_bounds = view.shrink(6.0);
        let offsets = &self.roi_callout_offsets;
        let mut new_hit_rects: Vec<((PathBuf, u32, usize), Rect)> = Vec::new();
        let mut draw_callout =
            |anchor_scr: Pos2,
             lines: &[String],
             border_col: Color32,
             ckey: Option<(PathBuf, u32, usize)>| {
                if lines.is_empty() {
                    return;
                }
                let text = lines.join("\n");
                let font = FontId::monospace(11.0);
                let galley = painter.layout_no_wrap(text.clone(), font.clone(), Color32::WHITE);
                let box_size = galley.size() + vec2(10.0, 6.0);
                let offset = ckey
                    .as_ref()
                    .and_then(|k| offsets.get(k))
                    .copied()
                    .unwrap_or(Vec2::ZERO);
                let raw_min = anchor_scr + vec2(10.0, -box_size.y - 4.0) + offset;
                let max_x = (safe_bounds.right() - box_size.x).max(safe_bounds.left());
                let max_y = (safe_bounds.bottom() - box_size.y).max(safe_bounds.top());
                let box_min = pos2(
                    raw_min.x.clamp(safe_bounds.left(), max_x),
                    raw_min.y.clamp(safe_bounds.top(), max_y),
                );
                let box_rect = Rect::from_min_size(box_min, box_size);
                if offset.length() > 8.0 {
                    let leader_col = Color32::from_rgba_unmultiplied(
                        border_col.r(),
                        border_col.g(),
                        border_col.b(),
                        105,
                    );
                    painter.line_segment([anchor_scr, box_rect.center()], Stroke::new(1.0, leader_col));
                }
                let bg = if border_col == color {
                    Color32::from_rgba_unmultiplied(36, 18, 18, 145)
                } else {
                    Color32::from_rgba_unmultiplied(14, 17, 22, 145)
                };
                let soft_border = Color32::from_rgba_unmultiplied(
                    border_col.r(),
                    border_col.g(),
                    border_col.b(),
                    105,
                );
                painter.rect_filled(box_rect, 3.0, bg);
                painter.rect_stroke(
                    box_rect,
                    3.0,
                    Stroke::new(0.75, soft_border),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    box_rect.min + vec2(5.0, 3.0),
                    Align2::LEFT_TOP,
                    text,
                    font,
                    Color32::from_rgb(238, 242, 246),
                );
                if let Some(k) = ckey {
                    new_hit_rects.push((k, box_rect));
                }
            };

        let unverified_geom = self.viewports.get(vp_idx).is_some_and(|v| v.mpr_enabled)
            && self
                .viewport_volume(vp_idx)
                .is_some_and(|v| !v.geometry_verified);
        let roi_stats_disabled = unverified_geom
            || (self.blend_siblings
                && self.viewports.get(vp_idx).is_some_and(|v| v.mpr_enabled)
                && !self.viewport_sibling_volumes(vp_idx).is_empty());

        let unit = self
            .viewport_series(vp_idx)
            .map(|se| {
                if se.modality.trim().eq_ignore_ascii_case("CT") {
                    "HU"
                } else {
                    f.value_unit
                }
            })
            .unwrap_or(f.value_unit);

        let measure_key = self.viewport_measure_key(vp_idx);
        let mut items: Vec<Measure> = measure_key
            .as_ref()
            .and_then(|k| self.measures.get(k).cloned())
            .unwrap_or_default();
        let saved_count = items.len();

        if vp_idx == self.active_viewport {
            match self.tool {
                Tool::Length => {
                    if self.drawing.len() == 2 {
                        items.push(Measure::Length(self.drawing[0], self.drawing[1]));
                    } else if self.drawing.len() == 1 {
                        if let Some(b) = self.hover_px {
                            items.push(Measure::Length(self.drawing[0], b));
                        } else {
                            painter.circle_filled(s(self.drawing[0]), 3.0, color);
                        }
                    }
                }
                Tool::Oval | Tool::Rect => {
                    let is_oval = self.tool == Tool::Oval;
                    if self.drawing.len() == 2 {
                        let (a, b) = (self.drawing[0], self.drawing[1]);
                        items.push(if is_oval {
                            Measure::Oval(a, b)
                        } else {
                            Measure::Rect(a, b)
                        });
                    } else if self.drawing.len() == 1 {
                        if let Some(b) = self.hover_px {
                            let a = self.drawing[0];
                            items.push(if is_oval {
                                Measure::Oval(a, b)
                            } else {
                                Measure::Rect(a, b)
                            });
                        } else {
                            painter.circle_filled(s(self.drawing[0]), 3.0, roi_color);
                        }
                    }
                }
                Tool::Pencil
                    if self.drawing.len() >= 2 => {
                        items.push(Measure::Pencil(self.drawing.clone()));
                    }
                _ => {}
            }
        }

        for (m_idx, m) in items.iter().enumerate() {
            let ckey = if m_idx < saved_count {
                measure_key.as_ref().map(|(p, fr)| (p.clone(), *fr, m_idx))
            } else {
                None
            };
            match m {
                &Measure::Length(a, b) => {
                    painter.line_segment([s(a), s(b)], stroke);
                    painter.circle_filled(s(a), 2.5, color);
                    painter.circle_filled(s(b), 2.5, color);
                    let lines = if unverified_geom {
                        vec!["Measurements disabled (unverified 3D geometry)".to_string()]
                    } else {
                        let eval = roi::evaluate_length(
                            f,
                            RoiPoint::new(a.x as f64, a.y as f64),
                            RoiPoint::new(b.x as f64, b.y as f64),
                        );
                        eval.format_callout(RoiKind::Length, unit, "")
                    };
                    draw_callout(s(b), &lines, color, ckey);
                }
                &Measure::Angle(a, v, b) => {
                    painter.line_segment([s(a), s(v)], stroke);
                    painter.line_segment([s(v), s(b)], stroke);
                    painter.circle_filled(s(a), 2.5, color);
                    painter.circle_filled(s(v), 3.0, color);
                    painter.circle_filled(s(b), 2.5, color);
                    let lines = if unverified_geom {
                        vec!["Measurements disabled (unverified 3D geometry)".to_string()]
                    } else {
                        let eval = roi::evaluate_angle(
                            f,
                            RoiPoint::new(a.x as f64, a.y as f64),
                            RoiPoint::new(v.x as f64, v.y as f64),
                            RoiPoint::new(b.x as f64, b.y as f64),
                        );
                        eval.format_callout(RoiKind::Angle, unit, "")
                    };
                    draw_callout(s(v), &lines, color, ckey);
                }
                &Measure::Oval(a, b) => {
                    let roi_rect = RoiBoundingBox::rect(
                        RoiPoint::new(a.x as f64, a.y as f64),
                        RoiPoint::new(b.x as f64, b.y as f64),
                        true,
                    );
                    let outline = RoiPlanarGeometry::outline_of_oval(roi_rect, 64);
                    let scr_pts: Vec<Pos2> = outline
                        .iter()
                        .map(|p| s(pos2(p.x as f32, p.y as f32)))
                        .collect();
                    if scr_pts.len() >= 3 {
                        painter.add(egui::Shape::closed_line(scr_pts, roi_stroke));
                    }
                    painter.circle_filled(s(a), 2.5, roi_color);
                    painter.circle_filled(s(b), 2.5, roi_color);
                    let lines = if roi_stats_disabled {
                        vec!["ROI stats disabled (blended)".to_string()]
                    } else {
                        let eval = roi::evaluate_region(
                            f,
                            RoiKind::Oval,
                            &outline,
                            Some(RoiSize::new(roi_rect.size.width, roi_rect.size.height)),
                        );
                        eval.format_callout(RoiKind::Oval, unit, "")
                    };
                    let anchor = pos2(
                        (roi_rect.origin.x + roi_rect.size.width) as f32,
                        roi_rect.origin.y as f32,
                    );
                    draw_callout(s(anchor), &lines, roi_color, ckey);
                }
                &Measure::Rect(a, b) => {
                    let roi_rect = RoiBoundingBox::rect(
                        RoiPoint::new(a.x as f64, a.y as f64),
                        RoiPoint::new(b.x as f64, b.y as f64),
                        false,
                    );
                    let outline = RoiPlanarGeometry::outline_of_rectangle(roi_rect);
                    let scr_pts: Vec<Pos2> = outline
                        .iter()
                        .map(|p| s(pos2(p.x as f32, p.y as f32)))
                        .collect();
                    painter.add(egui::Shape::closed_line(scr_pts, roi_stroke));
                    for corner in &outline {
                        painter.circle_filled(
                            s(pos2(corner.x as f32, corner.y as f32)),
                            2.5,
                            roi_color,
                        );
                    }
                    let lines = if roi_stats_disabled {
                        vec!["ROI stats disabled (blended)".to_string()]
                    } else {
                        let eval = roi::evaluate_region(f, RoiKind::Rectangle, &outline, None);
                        eval.format_callout(RoiKind::Rectangle, unit, "")
                    };
                    let anchor = pos2(roi_rect.max_x() as f32, roi_rect.min_y() as f32);
                    draw_callout(s(anchor), &lines, roi_color, ckey);
                }
                Measure::Polygon(pts) => {
                    if pts.len() >= 3 {
                        let outline: Vec<RoiPoint> = pts
                            .iter()
                            .map(|p| RoiPoint::new(p.x as f64, p.y as f64))
                            .collect();
                        let scr_pts: Vec<Pos2> = pts.iter().copied().map(&s).collect();
                        painter.add(egui::Shape::closed_line(scr_pts, roi_stroke));
                        for &pt in pts {
                            painter.circle_filled(s(pt), 2.5, roi_color);
                        }
                        let lines = if roi_stats_disabled {
                            vec!["ROI stats disabled (blended)".to_string()]
                        } else {
                            let eval = roi::evaluate_region(f, RoiKind::ClosedPolygon, &outline, None);
                            eval.format_callout(RoiKind::ClosedPolygon, unit, "")
                        };
                        draw_callout(s(pts[0]), &lines, roi_color, ckey);
                    }
                }
                Measure::Pencil(pts) => {
                    if pts.len() >= 2 {
                        let raw_outline: Vec<RoiPoint> = pts
                            .iter()
                            .map(|p| RoiPoint::new(p.x as f64, p.y as f64))
                            .collect();
                        let (smoothed, _) = RoiSpline::smooth(&raw_outline, 1.5);
                        let active_outline = if smoothed.len() >= 3 {
                            &smoothed
                        } else {
                            &raw_outline
                        };
                        let scr_pts: Vec<Pos2> = active_outline
                            .iter()
                            .map(|p| s(pos2(p.x as f32, p.y as f32)))
                            .collect();
                        painter.add(egui::Shape::closed_line(scr_pts, roi_stroke));
                        if active_outline.len() >= 3 {
                            let lines = if roi_stats_disabled {
                                vec!["ROI stats disabled (blended)".to_string()]
                            } else {
                                let eval =
                                    roi::evaluate_region(f, RoiKind::Pencil, active_outline, None);
                                eval.format_callout(RoiKind::Pencil, unit, "")
                            };
                            draw_callout(s(pts[0]), &lines, roi_color, ckey);
                        }
                    }
                }
                &Measure::Point(pt) => {
                    let sp = s(pt);
                    let r = 6.0;
                    painter.line_segment([sp - vec2(r, 0.0), sp + vec2(r, 0.0)], stroke);
                    painter.line_segment([sp - vec2(0.0, r), sp + vec2(0.0, r)], stroke);
                    painter.circle_stroke(sp, 3.0, stroke);
                    let lines = if roi_stats_disabled {
                        vec!["ROI stats disabled (blended)".to_string()]
                    } else {
                        let eval = roi::evaluate_point(f, RoiPoint::new(pt.x as f64, pt.y as f64));
                        eval.format_callout(RoiKind::Point, unit, "")
                    };
                    draw_callout(sp, &lines, color, ckey);
                }
            }
        }

        if vp_idx == self.active_viewport && self.tool == Tool::Polygon && !self.drawing.is_empty()
        {
            let mut scr_pts: Vec<Pos2> = self.drawing.iter().copied().map(&s).collect();
            for &pt in &self.drawing {
                painter.circle_filled(s(pt), 3.0, roi_color);
            }
            if let Some(hp) = self.hover_px {
                scr_pts.push(s(hp));
            }
            if scr_pts.len() >= 2 {
                painter.add(egui::Shape::line(scr_pts, roi_stroke));
            }
        }

        if vp_idx == self.active_viewport && self.tool == Tool::Angle {
            if let Some((a, v)) = self.angle_pending {
                painter.line_segment([s(a), s(v)], stroke);
                painter.circle_filled(s(a), 2.5, color);
                painter.circle_filled(s(v), 3.5, color);
                if let Some(b) = self.hover_px {
                    painter.line_segment([s(v), s(b)], stroke);
                    painter.circle_filled(s(b), 2.5, color);
                    if v.distance(b) > 1.0 {
                        let eval = roi::evaluate_angle(
                            f,
                            RoiPoint::new(a.x as f64, a.y as f64),
                            RoiPoint::new(v.x as f64, v.y as f64),
                            RoiPoint::new(b.x as f64, b.y as f64),
                        );
                        let lines = eval.format_callout(RoiKind::Angle, unit, "");
                        draw_callout(s(v), &lines, color, None);
                    }
                }
            } else if self.drawing.len() == 2 {
                let (a, v) = (self.drawing[0], self.drawing[1]);
                painter.line_segment([s(a), s(v)], stroke);
                painter.circle_filled(s(a), 2.5, color);
                painter.circle_filled(s(v), 3.0, color);
            } else if self.drawing.len() == 1 {
                let a = self.drawing[0];
                painter.circle_filled(s(a), 3.0, color);
                if let Some(v) = self.hover_px {
                    painter.line_segment([s(a), s(v)], stroke);
                    painter.circle_filled(s(v), 3.0, color);
                }
            }
        }
        drop(draw_callout);
        self.callout_hit_rects.extend(new_hit_rects);
    }

    pub(super) fn draw_overlay(
        &self,
        painter: &egui::Painter,
        vp_idx: usize,
        view: Rect,
        frame: Option<&Frame>,
    ) {
        let font = FontId::proportional(11.5);
        let pad = 10.0;
        let text = |pos: Pos2, align: Align2, s: String| {
            if s.is_empty() {
                return;
            }
            painter.text(
                pos + vec2(1.0, 1.0),
                align,
                &s,
                font.clone(),
                Color32::from_black_alpha(220),
            );
            painter.text(pos, align, s, font.clone(), Color32::from_gray(232));
        };

        let vp = &self.viewports[vp_idx];

        // Top-Left: Image dimensions, WL/WW (+ CLUT/Filter), and live cursor readout (OsiriX style)
        let mut tl = Vec::new();
        if let Some(f) = frame {
            tl.push(format!("Image size: {} × {}", f.width, f.height));
            let w = self.effective_window(vp_idx, f);
            let digits = if w.width.abs() < 10.0 { 2 } else { 0 };
            let win_name = self.window_name(vp_idx, f);
            let preset_name = win_name.label();
            tl.push(format!(
                "WL: {:.*}  WW: {:.*}  ({})",
                digits, w.level, digits, w.width, preset_name
            ));
            if let Some(clut_name) = vp.active_clut_name() {
                tl.push(format!("CLUT: {clut_name}"));
            }
            if let Some(conv_name) = vp
                .convolution_idx
                .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
                .map(|k| k.name)
            {
                tl.push(format!("Filter: {conv_name}"));
            }
            if vp_idx == self.active_viewport
                && let Some(hp) = self.hover_px
            {
                let px = hp.x.round().clamp(0.0, (f.width.saturating_sub(1)) as f32) as u32;
                let py = hp.y.round().clamp(0.0, (f.height.saturating_sub(1)) as f32) as u32;
                if let Some(val) = f.value_at(px, py) {
                    tl.push(format!("X: {px} px  Y: {py} px  Val: {val}"));
                }
            }
        }
        if self.is_viewport_key_image(vp_idx) {
            tl.push("★ KEY IMAGE".to_string());
        }
        text(view.left_top() + vec2(pad, pad), Align2::LEFT_TOP, tl.join("\n"));

        // Top-Right: Patient & Study / Series identity (hidden during anonymized Ctrl+C capture)
        if !self.copy_anonymize_frame
            && let Some(study) = self.viewport_study(vp_idx)
        {
            let mut tr = vec![study.patient_name.clone()];
            let mut id_line = Vec::new();
            if !study.patient_id.is_empty() {
                id_line.push(format!("ID: {}", study.patient_id));
            }
            if !study.birth_date.is_empty() {
                id_line.push(format!("DOB: {}", study.birth_date));
            }
            if !id_line.is_empty() {
                tr.push(id_line.join("  "));
            }
            if !study.description.is_empty() {
                tr.push(study.description.clone());
            }
            if let Some(series) = self.viewport_series(vp_idx) {
                tr.push(format!(
                    "{} · Se {} · {}",
                    series.modality, series.number, series.description
                ));
            }
            text(view.right_top() + vec2(-pad, pad), Align2::RIGHT_TOP, tr.join("\n"));
        }

        // Bottom-Left: Slice index, MPR/Slab, Thickness/Location, Zoom & presentation flags
        let slice_count = self.viewport_slice_count(vp_idx);
        let mut bl = vec![format!("Im: {} / {}", vp.slice + 1, slice_count)];
        if vp.mpr_enabled {
            let slab_info = vp
                .mpr_plane
                .as_ref()
                .filter(|p| p.slab_mode != SlabMode::Thin)
                .map(|p| {
                    format!(
                        " · {} {:.1} mm",
                        crate::ui::toolbar::slab_label(p.slab_mode, self.lang),
                        p.slab_thickness_mm
                    )
                })
                .unwrap_or_default();
            let reg_corr = vp.selected.and_then(|sel| {
                self.registrations
                    .iter()
                    .find(|&(&(a, b), _)| a == sel || b == sel)
                    .map(|(_, r)| r.correlation)
            });
            if let Some(corr) = reg_corr.filter(|_| self.tiling.cell_count() > 1) {
                bl.push(format!("3D MPR{slab_info} · Sync Locked (NCC {corr:.2})"));
            } else {
                bl.push(format!("3D MPR{slab_info}"));
            }
        }
        if let Some(f) = frame {
            let loc = f.slice_location.or_else(|| {
                self.viewport_series(vp_idx)
                    .and_then(|s| s.slices.get(vp.slice))
                    .and_then(|sr| sr.slice_location)
            });
            let mut geom_parts = Vec::new();
            if let Some(t) = f.slice_thickness {
                geom_parts.push(format!("Thk: {t:.1} mm"));
            }
            if let Some(loc) = loc {
                geom_parts.push(format!("Loc: {loc:.2} mm"));
            }
            if !geom_parts.is_empty() {
                bl.push(geom_parts.join("  "));
            }
        }
        let mut view_flags = vec![format!("Zoom: {:.0}%", vp.zoom * 100.0)];
        if vp.rotation.abs() > 0.05 {
            view_flags.push(format!("Rot: {:.0}°", vp.rotation));
        }
        if vp.effective_invert() {
            view_flags.push("Inverted".into());
        }
        if frame.is_some_and(|f| f.voi_lut_applied) {
            view_flags.push("VOI LUT".into());
        }
        bl.push(view_flags.join("  "));
        text(
            view.left_bottom() + vec2(pad, -pad),
            Align2::LEFT_BOTTOM,
            bl.join("\n"),
        );

        // Bottom-Right: Acquisition date & application stamp
        let mut br = Vec::new();
        if !self.copy_anonymize_frame
            && let Some(study) = self.viewport_study(vp_idx)
            && !study.date.is_empty()
        {
            br.push(study.date.clone());
        }
        br.push("SekhmetX".into());
        text(
            view.right_bottom() + vec2(-pad, -pad),
            Align2::RIGHT_BOTTOM,
            br.join("\n"),
        );
    }
}
