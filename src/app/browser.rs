use super::*;

fn paint_aspect_fit_thumb(
    painter: &egui::Painter,
    box_rect: Rect,
    tex: &egui::TextureHandle,
) {
    let inner = box_rect.shrink(2.0);
    let sz = tex.size_vec2();
    let tw = sz.x.max(1.0);
    let th = sz.y.max(1.0);
    let scale = (inner.width() / tw).min(inner.height() / th);
    let fit_rect = Rect::from_center_size(inner.center(), vec2(tw * scale, th * scale));
    painter.image(
        tex.id(),
        fit_rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

impl ViewerApp {
    fn patient_group_key(&self, study: &crate::dicom_index::Study) -> String {
        if let Some(over) = self.patient_links.get(&study.uid) {
            over.clone()
        } else {
            let pid = study.patient_id.trim();
            if !pid.is_empty() {
                format!("id:{}", pid.to_ascii_uppercase())
            } else {
                let norm_name = study
                    .patient_name
                    .replace('^', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_ascii_uppercase();
                format!("name:{norm_name}")
            }
        }
    }

    pub(super) fn browser(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        if self.index.is_none() {
            ui.add_space(8.0);
            ui.label(tr.no_studies);
            ui.label(tr.drop_hint);
            return;
        }

        // Active-study header + Back to Database button
        let active_si = self
            .active_cell()
            .selected
            .map(|(si, _)| si)
            .or(self.selected_db_study);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let back_lbl = match self.lang {
                crate::i18n::Lang::En => "← Database",
                crate::i18n::Lang::Es => "← Base de datos",
            };
            if ui.small_button(back_lbl).clicked() {
                self.show_database_view = true;
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("🔍");
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width(ui.available_width() - 28.0);
            ui.add(edit);
            if !self.browser_search.is_empty() && ui.small_button("✕").clicked() {
                self.browser_search.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (code, label) in [
                ("ALL", tr.all),
                ("CT", "CT"),
                ("MR", "MR"),
                ("US", "US"),
                ("DX", "DX/CR"),
                ("PT", "PT/NM"),
            ] {
                let selected = self.browser_modality == code;
                if ui.selectable_label(selected, label).clicked() {
                    self.browser_modality = code.to_string();
                }
            }
        });
        ui.separator();

        let query = self.browser_search.trim().to_ascii_lowercase();
        let mod_filter = self.browser_modality.clone();
        let scan_gen = self.scan_gen;
        let mut clicked: Option<(usize, (usize, usize), bool)> = None;
        let mut tile_study: Option<(usize, ViewportTiling)> = None;
        let mut remove_study_uid: Option<String> = None;
        let mut clear_library = false;
        let mut thumb_jobs: Vec<Job> = Vec::new();
        let active_sel = self.active_cell().selected;
        let cell_count = self.tiling.cell_count();

        let Some(index) = &self.index else {
            return;
        };

        ui.horizontal(|ui| {
            let disk_mb = crate::db::library_disk_usage_bytes() as f64 / (1024.0 * 1024.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} · {} {} ({disk_mb:.1} MB)",
                    tr.library,
                    index.studies.len(),
                    tr.studies
                ))
                .small()
                .color(Color32::from_gray(165)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(tr.clear)
                    .on_hover_text("Clear studies from the local library (files on disk are untouched)")
                    .clicked()
                {
                    clear_library = true;
                }
            });
        });
        ui.add_space(2.0);
        if index.studies.is_empty() {
            ui.add_space(6.0);
            ui.label(tr.no_studies);
            ui.label(tr.drop_hint);
            return;
        }

        // Pre-group matching studies by Patient (manual link override -> Patient ID -> normalized Patient Name).
        struct BrowserPatientGroup {
            key: String,
            display_name: String,
            display_id: String,
            studies: Vec<(usize, Vec<usize>)>,
        }
        let mut patient_groups: Vec<BrowserPatientGroup> = Vec::new();
        let mut group_idx_by_key: HashMap<String, usize> = HashMap::new();
        let active_patient_key = active_si
            .and_then(|si| index.studies.get(si))
            .map(|st| self.patient_group_key(st));
        for (si, study) in index.studies.iter().enumerate() {
            if let Some(ref only_pkey) = active_patient_key
                && self.patient_group_key(study) != *only_pkey
            {
                continue;
            }
            let study_matches_query = query.is_empty()
                || study.patient_name.to_ascii_lowercase().contains(&query)
                || study.patient_id.to_ascii_lowercase().contains(&query)
                || study.description.to_ascii_lowercase().contains(&query)
                || study.accession.to_ascii_lowercase().contains(&query)
                || study.date.to_ascii_lowercase().contains(&query);

            let matching_series: Vec<usize> = study
                .series
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    let mod_ok = match mod_filter.as_str() {
                        "ALL" => true,
                        "DX" => matches!(s.modality.as_str(), "DX" | "CR" | "RG" | "MG"),
                        "PT" => matches!(s.modality.as_str(), "PT" | "NM" | "ST"),
                        m => s.modality.eq_ignore_ascii_case(m),
                    };
                    if !mod_ok {
                        return false;
                    }
                    study_matches_query
                        || s.description.to_ascii_lowercase().contains(&query)
                        || s.modality.to_ascii_lowercase().contains(&query)
                        || s.dominant_plane
                            .badge()
                            .to_ascii_lowercase()
                            .contains(&query)
                })
                .map(|(ri, _)| ri)
                .collect();

            if matching_series.is_empty() {
                continue;
            }

            let key = self.patient_group_key(study);

            if let Some(&gi) = group_idx_by_key.get(&key) {
                patient_groups[gi].studies.push((si, matching_series));
            } else {
                let gi = patient_groups.len();
                group_idx_by_key.insert(key.clone(), gi);
                patient_groups.push(BrowserPatientGroup {
                    key,
                    display_name: study.patient_name.replace('^', " "),
                    display_id: study.patient_id.trim().to_string(),
                    studies: vec![(si, matching_series)],
                });
            }
        }

        let mut start_link: Option<(String, Vec<String>)> = None;
        let mut commit_link_target: Option<String> = None;
        let mut unlink_study_uid: Option<String> = None;
        let mut cancel_linking = false;

        if let Some((ref src_name, _)) = self.linking_source {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(255, 210, 100),
                        format!("{} {src_name}", tr.linking_banner),
                    );
                    if ui.small_button(tr.cancel_link).clicked() {
                        cancel_linking = true;
                    }
                });
            });
            ui.add_space(2.0);
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for pg in &patient_groups {
                    let mut pt_label = pg.display_name.clone();
                    if !pg.display_id.is_empty() {
                        pt_label.push_str(&format!(" · ID {}", pg.display_id));
                    }
                    pt_label.push_str(&format!(" ({})", pg.studies.len()));

                    let is_linking = self.linking_source.is_some();
                    let pg_resp = ui.horizontal(|ui| {
                        let resp = egui::CollapsingHeader::new(
                            egui::RichText::new(&pt_label).strong(),
                        )
                        .id_salt(format!("pt_{}", pg.key))
                        .default_open(active_si.is_some())
                        .show(ui, |ui| {
                            for (si, matching_series) in &pg.studies {
                                let si = *si;
                                let study = &index.studies[si];
                                let desc = if study.description.trim().is_empty() {
                                    "Study"
                                } else {
                                    study.description.trim()
                                };
                                let title = if study.files_missing {
                                    format!("⚠ [files missing] {desc} — {}", study.date)
                                } else {
                                    format!("{desc} — {}", study.date)
                                };
                                let hdr_resp =
                                    egui::CollapsingHeader::new(egui::RichText::new(title))
                                        .id_salt(&study.uid)
                                        .default_open(active_si == Some(si))
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
                            if study.series.len() >= 2 {
                                ui.horizontal(|ui| {
                                    ui.small(tr.compare);
                                    if ui.small_button("1×2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(1, 2)));
                                    }
                                    if ui.small_button("2×2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(2, 2)));
                                    }
                                    if study.series.len() >= 5 && ui.small_button("2×3").clicked()
                                    {
                                        tile_study = Some((si, ViewportTiling::new(2, 3)));
                                    }
                                });
                            }

                            // Helper closure to render a single series row (with 38x38 thumbnail & click/context menu).
                            let draw_series_row = |ui: &mut egui::Ui,
                                                   ri: usize,
                                                   clicked: &mut Option<(usize, (usize, usize), bool)>,
                                                   thumb_jobs: &mut Vec<Job>| {
                                let series = &study.series[ri];
                                let desc = if series.description.is_empty() {
                                    "(no description)"
                                } else {
                                    &series.description
                                };
                                let mpr_tag = if series.is_mpr_capable() { "·MPR" } else { "" };
                                let is_active = active_sel == Some((si, ri));

                                // Queue low-priority background job for the middle slice thumbnail if not yet requested.
                                let tkey = (scan_gen, si, ri);
                                if !self.thumbnails.contains_key(&tkey)
                                    && !self.thumbnails_queued.contains(&tkey)
                                    && let Some(ref_idx) =
                                        defaults::reference_index(series.slices.len())
                                        && let Some(sref) = series.slices.get(ref_idx) {
                                            thumb_jobs.push(Job {
                                                key: SliceKey {
                                                    scan_gen,
                                                    study: si,
                                                    series: ri,
                                                    slice: ref_idx,
                                                },
                                                path: sref.path.clone(),
                                                frame: sref.frame,
                                                urgent: false,
                                                distance: 800 + ri,
                                            });
                                        }

                                ui.horizontal(|ui| {
                                    // Draw 38x38 Horos-style series thumbnail box
                                    let thumb_size = vec2(52.0, 52.0);
                                    let (thumb_rect, thumb_resp) =
                                        ui.allocate_exact_size(thumb_size, Sense::click());
                                    let painter = ui.painter();
                                    painter.rect_filled(
                                        thumb_rect,
                                        4.0,
                                        Color32::from_rgb(12, 14, 18),
                                    );
                                    if let Some(tex) = self.thumbnails.get(&tkey) {
                                        paint_aspect_fit_thumb(painter, thumb_rect, tex);
                                    } else {
                                        painter.text(
                                            thumb_rect.center(),
                                            Align2::CENTER_CENTER,
                                            &series.modality,
                                            FontId::monospace(10.5),
                                            Color32::from_gray(145),
                                        );
                                    }
                                    let border_col = if is_active {
                                        Color32::from_rgb(80, 175, 255)
                                    } else {
                                        Color32::from_gray(60)
                                    };
                                    painter.rect_stroke(
                                        thumb_rect,
                                        4.0,
                                        Stroke::new(if is_active { 1.5 } else { 1.0 }, border_col),
                                        egui::StrokeKind::Inside,
                                    );

                                    let text = format!(
                                        "{:<2} {:>3} [{}{}] {}\n{} images · {}×{}",
                                        series.modality,
                                        series.number,
                                        series.dominant_plane.badge(),
                                        mpr_tag,
                                        desc,
                                        series.slices.len(),
                                        series.columns,
                                        series.rows
                                    );
                                    let resp = ui
                                        .selectable_label(
                                            is_active,
                                            egui::RichText::new(text).monospace().size(11.5),
                                        )
                                        .on_hover_text(format!(
                                            "{} {}, {}×{}, plane: {}{} (click for active viewport, right-click to assign)",
                                            series.slices.len(),
                                            tr.images,
                                            series.columns,
                                            series.rows,
                                            series.dominant_plane.badge(),
                                            if series.is_mpr_capable() { " · 3D MPR Ready" } else { "" }
                                        ));
                                    if resp.clicked() || thumb_resp.clicked() {
                                        *clicked = Some((self.active_viewport, (si, ri), false));
                                    }
                                    resp.context_menu(|ui| {
                                        for vp_idx in 0..cell_count {
                                            if ui
                                                .button(format!("Open in Viewport {}", vp_idx + 1))
                                                .clicked()
                                            {
                                                *clicked = Some((vp_idx, (si, ri), false));
                                                ui.close();
                                            }
                                        }
                                    });
                                });
                            };

                            // Render series in list order, grouping multi-plane AX/COR/SAG reconstructions
                            // into a compact Horos-style block (no duplicate rows outside the block).
                            let mut rendered_series = std::collections::HashSet::new();
                            for &ri in matching_series {
                                if rendered_series.contains(&ri) {
                                    continue;
                                }
                                let grp = study.series[ri]
                                    .mpr_group_id
                                    .and_then(|gid| study.mpr_groups.iter().find(|g| g.id == gid))
                                    .filter(|g| {
                                        g.series_indices
                                            .iter()
                                            .filter(|idx| matching_series.contains(idx))
                                            .count()
                                            >= 2
                                    });

                                if let Some(group) = grp {
                                    let grp_active = active_sel.is_some_and(|(s_i, r_i)| {
                                        s_i == si && group.series_indices.contains(&r_i)
                                    });
                                    let mut ordered_planes = Vec::new();
                                    for opt_ri in [
                                        group.axial_series,
                                        group.coronal_series,
                                        group.sagittal_series,
                                    ] {
                                        if let Some(pri) = opt_ri
                                            && matching_series.contains(&pri)
                                                && !ordered_planes.contains(&pri)
                                            {
                                                ordered_planes.push(pri);
                                            }
                                    }
                                    for &gri in &group.series_indices {
                                        if matching_series.contains(&gri)
                                            && !ordered_planes.contains(&gri)
                                        {
                                            ordered_planes.push(gri);
                                        }
                                    }
                                    for &pri in &ordered_planes {
                                        rendered_series.insert(pri);
                                    }

                                    ui.push_id(("mpr_grp_block", si, group.id), |ui| {
                                        egui::Frame::group(ui.style())
                                            .fill(Color32::from_rgb(28, 34, 44))
                                            .stroke(Stroke::new(
                                                1.0,
                                                if grp_active {
                                                    Color32::from_rgb(70, 145, 230)
                                                } else {
                                                    Color32::from_rgb(52, 60, 74)
                                                },
                                            ))
                                            .inner_margin(egui::Margin::symmetric(6, 5))
                                            .show(ui, |ui| {
                                                let grp_text = format!(
                                                    "🧊 {}  [{}]",
                                                    group.label, group.planes_badge
                                                );
                                                let hdr = ui
                                                    .selectable_label(
                                                        grp_active,
                                                        egui::RichText::new(grp_text)
                                                            .monospace()
                                                            .strong()
                                                            .size(11.0)
                                                            .color(Color32::from_rgb(130, 210, 255)),
                                                    )
                                                    .on_hover_text(
                                                        "Grouped Axial / Coronal / Sagittal reconstructions. Click to open in 3D MPR.",
                                                    );
                                                if hdr.clicked() {
                                                    let target_ri = active_sel
                                                        .filter(|&(s_i, r_i)| {
                                                            s_i == si
                                                                && group.series_indices.contains(&r_i)
                                                        })
                                                        .map_or(group.primary_series, |(_, r_i)| r_i);
                                                    clicked = Some((
                                                        self.active_viewport,
                                                        (si, target_ri),
                                                        true,
                                                    ));
                                                }
                                                ui.add_space(2.0);
                                                for pri in ordered_planes {
                                                    draw_series_row(
                                                        ui,
                                                        pri,
                                                        &mut clicked,
                                                        &mut thumb_jobs,
                                                    );
                                                }
                                            });
                                    });
                                } else {
                                    rendered_series.insert(ri);
                                    draw_series_row(ui, ri, &mut clicked, &mut thumb_jobs);
                                }
                            }
                        });
                                    hdr_resp.header_response.context_menu(|ui| {
                                        if (pg.studies.len() > 1
                                            || self.patient_links.contains_key(&study.uid))
                                            && ui.button(tr.unlink_study).clicked() {
                                                unlink_study_uid = Some(study.uid.clone());
                                                ui.close();
                                            }
                                        if ui.button(tr.link_patient).clicked() {
                                            start_link = Some((
                                                format!("{} ({})", study.patient_name, study.date),
                                                vec![study.uid.clone()],
                                            ));
                                            ui.close();
                                        }
                                        if ui.button(tr.remove_from_library).clicked() {
                                            remove_study_uid = Some(study.uid.clone());
                                            ui.close();
                                        }
                                    });
                                }
                        });
                        if is_linking && ui.small_button(tr.link_here).clicked() {
                            commit_link_target = Some(pg.key.clone());
                        }
                        resp
                    });
                    pg_resp.inner.header_response.context_menu(|ui| {
                        if ui.button(tr.link_patient).clicked() {
                            let uids = pg
                                .studies
                                .iter()
                                .map(|(si, _)| index.studies[*si].uid.clone())
                                .collect();
                            start_link = Some((pg.display_name.clone(), uids));
                            ui.close();
                        }
                    });
                }
            });

        if cancel_linking {
            self.linking_source = None;
        }
        if let Some(src) = start_link {
            self.linking_source = Some(src);
        }
        if let Some(target_key) = commit_link_target
            && let Some((_, uids)) = self.linking_source.take() {
                for uid in uids {
                    self.patient_links.insert(uid, target_key.clone());
                }
                if let Some(ref db) = self.db
                    && let Ok(json) = serde_json::to_string(&self.patient_links) {
                        let _ = db.set_setting("patient_links", &json);
                    }
            }
        if let Some(uid) = unlink_study_uid {
            self.patient_links.insert(uid.clone(), format!("solo:{uid}"));
            if let Some(ref db) = self.db
                && let Ok(json) = serde_json::to_string(&self.patient_links) {
                    let _ = db.set_setting("patient_links", &json);
                }
        }

        if self.pending_copy_prompt {
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new(tr.copy_zip_prompt)
                        .small()
                        .strong()
                        .color(Color32::from_rgb(255, 210, 100)),
                );
                ui.horizontal(|ui| {
                    if ui.small_button(tr.copy_btn).clicked() {
                        self.copy_ephemeral_studies_to_library();
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.always_btn).clicked() {
                        self.set_copy_policy(crate::db::CopyToLibraryPolicy::Always);
                        self.copy_ephemeral_studies_to_library();
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.no_btn).clicked() {
                        self.pending_copy_prompt = false;
                    }
                    if ui.small_button(tr.never_btn).clicked() {
                        self.set_copy_policy(crate::db::CopyToLibraryPolicy::Never);
                        self.pending_copy_prompt = false;
                    }
                });
            });
        }

        if clear_library {
            if let Some(db) = &self.db {
                let _ = db.clear_all();
            }
            self.index = None;
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
            return;
        }
        if let Some(uid) = remove_study_uid {
            if let Some(db) = &self.db {
                let _ = db.remove_study(&uid);
            }
            if let Some(idx) = &mut self.index {
                idx.studies.retain(|s| s.uid != uid);
                if idx.studies.is_empty() {
                    self.index = None;
                }
            }
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
            if self.index.is_some() {
                self.assign_series_to_viewport(0, Some((0, 0)));
            }
            return;
        }

        for job in thumb_jobs {
            self.thumbnails_queued
                .insert((job.key.scan_gen, job.key.study, job.key.series));
            self.loader.request(job);
        }

        if let Some((si, tiling)) = tile_study {
            self.tiling = tiling;
            let count = tiling.cell_count();
            let series_len = self.index.as_ref().map_or(0, |idx| idx.studies[si].series.len());
            for vp_idx in 0..count {
                if vp_idx < series_len {
                    self.assign_series_to_viewport(vp_idx, Some((si, vp_idx)));
                } else {
                    self.assign_series_to_viewport(vp_idx, None);
                }
            }
            self.active_viewport = 0;
        } else if let Some((vp_idx, sel, enable_mpr)) = clicked {
            self.active_viewport = vp_idx;
            self.assign_series_to_viewport(vp_idx, Some(sel));
            if enable_mpr {
                self.viewports[vp_idx].mpr_enabled = true;
            }
        }
    }

    fn queue_thumb_for_series(
        &self,
        thumb_jobs: &mut Vec<Job>,
        si: usize,
        ri: usize,
        series: &crate::dicom_index::Series,
    ) -> (u64, usize, usize) {
        let ref_idx = series.slices.len() / 2;
        let thumb_key = (self.scan_gen, si, ri);
        if !self.thumbnails.contains_key(&thumb_key)
            && !self.thumbnails_queued.contains(&thumb_key)
            && let Some(sref) = series.slices.get(ref_idx)
        {
            thumb_jobs.push(Job {
                key: SliceKey {
                    scan_gen: self.scan_gen,
                    study: si,
                    series: ri,
                    slice: ref_idx,
                },
                path: sref.path.clone(),
                frame: sref.frame,
                urgent: false,
                distance: 800 + ri,
            });
        }
        thumb_key
    }

    fn tile_study_series(&mut self, si: usize) {
        let n = self.index.as_ref().map_or(1, |idx| idx.studies[si].series.len());
        let tiling = match n {
            0..=1 => ViewportTiling { rows: 1, columns: 1 },
            2 => ViewportTiling { rows: 1, columns: 2 },
            3..=4 => ViewportTiling { rows: 2, columns: 2 },
            _ => ViewportTiling { rows: 3, columns: 3 },
        };
        self.tiling = tiling;
        let count = tiling.cell_count();
        for vp_idx in 0..count {
            if vp_idx < n {
                self.assign_series_to_viewport(vp_idx, Some((si, vp_idx)));
            } else {
                self.assign_series_to_viewport(vp_idx, None);
            }
        }
        self.active_viewport = 0;
    }

    fn flush_thumb_jobs(&mut self, thumb_jobs: Vec<Job>) {
        for job in thumb_jobs {
            self.thumbnails_queued
                .insert((job.key.scan_gen, job.key.study, job.key.series));
            self.loader.request(job);
        }
    }

    /// OsiriX-style full Database Library view with Albums sidebar, Study Table, and Series Grid.
    pub(super) fn database_view(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        let disk_mb = crate::db::library_disk_usage_bytes() as f64 / (1024.0 * 1024.0);
        let study_count = self.index.as_ref().map_or(0, |i| i.studies.len());

        // Top Database toolbar strip
        ui.horizontal(|ui| {
            ui.label("🔍");
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width(240.0);
            ui.add(edit);
            if !self.browser_search.is_empty() && ui.small_button("✕").clicked() {
                self.browser_search.clear();
            }
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "{} · {} {} ({disk_mb:.1} MB)",
                    tr.library, study_count, tr.studies
                ))
                .small()
                .color(Color32::from_gray(180)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr.clear).clicked() {
                    if let Some(db) = &self.db {
                        let _ = db.clear_all();
                    }
                    self.index = None;
                    self.selected_db_study = None;
                    for vp in &mut self.viewports {
                        vp.reset_series_state(None);
                    }
                }
                let refresh_lbl = match self.lang {
                    crate::i18n::Lang::En => "⟳ Refresh",
                    crate::i18n::Lang::Es => "⟳ Actualizar",
                };
                if ui.button(refresh_lbl).clicked() {
                    let dir = crate::db::managed_studies_dir();
                    if dir.is_dir() {
                        self.start_scan(vec![dir]);
                    }
                }
            });
        });
        ui.separator();

        let Some(index) = &self.index else {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new(tr.no_studies).size(15.0));
                    ui.label(tr.drop_hint);
                });
            });
            return;
        };

        let query = self.browser_search.trim().to_ascii_lowercase();
        let mod_filter = self.browser_modality.clone();

        // Count studies per modality album
        let modality_matches = |s: &crate::dicom_index::Series, code: &str| -> bool {
            match code {
                "ALL" => true,
                "DX" => matches!(s.modality.as_str(), "DX" | "CR" | "RG" | "MG"),
                "PT" => matches!(s.modality.as_str(), "PT" | "NM" | "ST"),
                m => s.modality.eq_ignore_ascii_case(m),
            }
        };

        let mut matching_studies: Vec<usize> = Vec::new();
        for (si, st) in index.studies.iter().enumerate() {
            let q_ok = query.is_empty()
                || st.patient_name.to_ascii_lowercase().contains(&query)
                || st.patient_id.to_ascii_lowercase().contains(&query)
                || st.description.to_ascii_lowercase().contains(&query)
                || st.accession.to_ascii_lowercase().contains(&query)
                || st.date.to_ascii_lowercase().contains(&query)
                || st.series.iter().any(|se| {
                    se.description.to_ascii_lowercase().contains(&query)
                        || se.modality.to_ascii_lowercase().contains(&query)
                });
            let m_ok = mod_filter == "ALL"
                || st.series.iter().any(|se| modality_matches(se, &mod_filter));
            if q_ok && m_ok {
                matching_studies.push(si);
            }
        }

        let mut open_series: Option<(usize, usize)> = None;
        let mut tile_study: Option<usize> = None;
        let mut remove_uid: Option<String> = None;
        let mut thumb_jobs: Vec<Job> = Vec::new();
        let avail_h = ui.available_height();
        let bottom_strip_h = 166.0f32.min((avail_h * 0.34).max(120.0));
        let top_area_h = (avail_h - bottom_strip_h - 8.0).max(140.0);

        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), top_area_h),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                // Left Albums Sidebar
                ui.allocate_ui_with_layout(
                    vec2(165.0, top_area_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let albums_lbl = match self.lang {
                            crate::i18n::Lang::En => "Albums",
                            crate::i18n::Lang::Es => "Álbumes",
                        };
                        ui.strong(albums_lbl);
                        ui.add_space(4.0);
                        for (code, label) in [
                            ("ALL", tr.all),
                            ("CT", "CT"),
                            ("MR", "MR"),
                            ("US", "US"),
                            ("DX", "DX / CR"),
                            ("PT", "PT / NM"),
                        ] {
                            let cnt = if code == "ALL" {
                                index.studies.len()
                            } else {
                                index
                                    .studies
                                    .iter()
                                    .filter(|st| st.series.iter().any(|se| modality_matches(se, code)))
                                    .count()
                            };
                            let selected = self.browser_modality == code;
                            if ui
                                .selectable_label(selected, format!("{label}  ({cnt})"))
                                .clicked()
                            {
                                self.browser_modality = code.to_string();
                            }
                        }
                    },
                );
                ui.separator();

                // Center Study Table
                ui.vertical(|ui| {
                    let table_w = ui.available_width().max(640.0);
                    let fixed_w = 40.0 + 52.0 + 74.0 + 96.0 + 116.0 + 96.0 + 48.0;
                    let flex_w = (table_w - fixed_w).max(260.0);
                    let name_w = (flex_w * 0.44).max(160.0);
                    let desc_w = (flex_w - name_w).max(140.0);
                    let hdr = |text: &str| egui::RichText::new(text).strong().size(12.0);
                    let cell_txt = |text: &str| egui::RichText::new(text).size(12.0);
                    let cell_mono = |text: String| egui::RichText::new(text).monospace().size(11.5);

                    // Column headers
                    egui::Frame::new()
                        .fill(Color32::from_rgb(32, 36, 44))
                        .inner_margin(egui::Margin::symmetric(8, 5))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized([name_w, 18.0], egui::Label::new(hdr("Patient Name")));
                                ui.add_sized([40.0, 18.0], egui::Label::new(hdr("#")));
                                ui.add_sized([52.0, 18.0], egui::Label::new(hdr("# im")));
                                ui.add_sized([desc_w, 18.0], egui::Label::new(hdr("Study Description")));
                                ui.add_sized([74.0, 18.0], egui::Label::new(hdr("Modality")));
                                ui.add_sized([96.0, 18.0], egui::Label::new(hdr("Date")));
                                ui.add_sized([116.0, 18.0], egui::Label::new(hdr("Patient ID")));
                                ui.add_sized([96.0, 18.0], egui::Label::new(hdr("Actions")));
                            });
                        });

                    egui::ScrollArea::vertical()
                        .id_salt("db_study_table")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if matching_studies.is_empty() {
                                ui.add_space(16.0);
                                ui.label(tr.no_studies);
                            }
                            for (row_i, &si) in matching_studies.iter().enumerate() {
                                let st = &index.studies[si];
                                let is_selected = self.selected_db_study == Some(si);
                                let row_bg = if is_selected {
                                    Color32::from_rgb(42, 76, 128)
                                } else if row_i % 2 == 1 {
                                    Color32::from_rgb(24, 27, 33)
                                } else {
                                    Color32::from_rgb(19, 21, 26)
                                };
                                let im_total: usize = st.series.iter().map(|s| s.slices.len()).sum();
                                let mods: Vec<&str> = {
                                    let mut v: Vec<&str> = st.series.iter().map(|s| s.modality.as_str()).collect();
                                    v.sort_unstable();
                                    v.dedup();
                                    v
                                };
                                let mod_str = mods.join("/");
                                let pname = if st.patient_name.trim().is_empty() {
                                    "(Unnamed)"
                                } else {
                                    &st.patient_name
                                };

                                let frame_resp = egui::Frame::new()
                                    .fill(row_bg)
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add_sized([name_w, 20.0], egui::Label::new(cell_txt(pname)).truncate());
                                            ui.add_sized([40.0, 20.0], egui::Label::new(cell_mono(format!("{}", st.series.len()))));
                                            ui.add_sized([52.0, 20.0], egui::Label::new(cell_mono(format!("{im_total}"))));
                                            ui.add_sized([desc_w, 20.0], egui::Label::new(cell_txt(&st.description)).truncate());
                                            ui.add_sized([74.0, 20.0], egui::Label::new(cell_mono(mod_str)));
                                            ui.add_sized([96.0, 20.0], egui::Label::new(cell_mono(st.date.clone())));
                                            ui.add_sized([116.0, 20.0], egui::Label::new(cell_mono(st.patient_id.clone())).truncate());
                                            ui.horizontal(|ui| {
                                                if ui.small_button("2D").on_hover_text("Open in 2D Viewer").clicked() {
                                                    open_series = Some((si, 0));
                                                }
                                                if ui.small_button("⊞").on_hover_text("Tile all series").clicked() {
                                                    tile_study = Some(si);
                                                }
                                                if ui.small_button("✕").on_hover_text("Remove from Library").clicked() {
                                                    remove_uid = Some(st.uid.clone());
                                                }
                                            });
                                        });
                                    });
                                let row_resp = ui.interact(
                                    frame_resp.response.rect,
                                    ui.scope_id().with(("db_row", si)),
                                    Sense::click(),
                                );
                                if row_resp.clicked() {
                                    self.selected_db_study = Some(si);
                                }
                                if row_resp.double_clicked() && !st.series.is_empty() {
                                    open_series = Some((si, 0));
                                }
                            }
                        });
                });
            },
        );

        ui.separator();

        // Bottom Series Thumbnail Grid for the selected study
        let sel_si = self
            .selected_db_study
            .filter(|&i| i < index.studies.len())
            .or_else(|| matching_studies.first().copied());
        if let Some(si) = sel_si {
            let st = &index.studies[si];
            ui.horizontal(|ui| {
                ui.strong(format!("{} — {} ({} series)", st.patient_name, st.description, st.series.len()));
            });
            egui::ScrollArea::horizontal()
                .id_salt("db_series_grid")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (ri, series) in st.series.iter().enumerate() {
                            let tkey = self.queue_thumb_for_series(&mut thumb_jobs, si, ri, series);
                            let card_size = vec2(114.0, 126.0);
                            let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());
                            let painter = ui.painter();
                            let bg = if resp.hovered() {
                                Color32::from_rgb(38, 44, 56)
                            } else {
                                Color32::from_rgb(24, 27, 34)
                            };
                            painter.rect_filled(rect, 5.0, bg);
                            painter.rect_stroke(
                                rect,
                                5.0,
                                Stroke::new(1.0, Color32::from_gray(55)),
                                egui::StrokeKind::Inside,
                            );
                            let img_rect = Rect::from_min_size(rect.min + vec2(13.0, 6.0), vec2(88.0, 88.0));
                            painter.rect_filled(img_rect, 3.0, Color32::BLACK);
                            if let Some(tex) = self.thumbnails.get(&tkey) {
                                paint_aspect_fit_thumb(painter, img_rect, tex);
                            } else {
                                painter.text(
                                    img_rect.center(),
                                    Align2::CENTER_CENTER,
                                    &series.modality,
                                    FontId::monospace(13.0),
                                    Color32::from_gray(145),
                                );
                            }
                            let desc = if series.description.trim().is_empty() {
                                format!("Series {}", series.number)
                            } else {
                                series.description.clone()
                            };
                            let short: String = desc.chars().take(16).collect();
                            painter.text(
                                pos2(rect.center().x, img_rect.max.y + 4.0),
                                Align2::CENTER_TOP,
                                short,
                                FontId::proportional(10.0),
                                Color32::from_gray(225),
                            );
                            painter.text(
                                pos2(rect.center().x, img_rect.max.y + 17.0),
                                Align2::CENTER_TOP,
                                format!("{} · {} im", series.modality, series.slices.len()),
                                FontId::proportional(9.5),
                                Color32::from_gray(155),
                            );
                            if resp.clicked() || resp.double_clicked() {
                                open_series = Some((si, ri));
                            }
                        }
                    });
                });
        }

        self.flush_thumb_jobs(thumb_jobs);

        if let Some(uid) = remove_uid {
            if let Some(db) = &self.db {
                let _ = db.remove_study(&uid);
            }
            if let Some(idx) = &mut self.index {
                idx.studies.retain(|s| s.uid != uid);
                if idx.studies.is_empty() {
                    self.index = None;
                    self.selected_db_study = None;
                }
            }
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
        } else if let Some(si) = tile_study {
            self.tile_study_series(si);
            self.show_database_view = false;
        } else if let Some((si, ri)) = open_series {
            self.active_viewport = 0;
            self.assign_series_to_viewport(0, Some((si, ri)));
            self.populate_unassigned_viewports();
            self.show_database_view = false;
        }
    }
}
