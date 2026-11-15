use super::*;

impl ViewerApp {
    pub(super) fn browser(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr(self.lang);
        if self.index.is_none() {
            ui.add_space(8.0);
            ui.label(tr.no_studies);
            ui.label(tr.drop_hint);
            return;
        }

        // Horos-style Search & Modality Filter Bar
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("ðŸ”");
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width(ui.available_width() - 28.0);
            ui.add(edit);
            if !self.browser_search.is_empty() && ui.small_button("âœ•").clicked() {
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
                    "{} Â· {} {} ({disk_mb:.1} MB)",
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

        // Pre-group matching studies by Patient (manual link override -> Patient ID -> normalized Patient Name).
        struct BrowserPatientGroup {
            key: String,
            display_name: String,
            display_id: String,
            studies: Vec<(usize, Vec<usize>)>,
        }
        let mut patient_groups: Vec<BrowserPatientGroup> = Vec::new();
        let mut group_idx_by_key: HashMap<String, usize> = HashMap::new();
        for (si, study) in index.studies.iter().enumerate() {
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

            let key = if let Some(over) = self.patient_links.get(&study.uid) {
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
            };

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
                        pt_label.push_str(&format!(" Â· ID {}", pg.display_id));
                    }
                    pt_label.push_str(&format!(" ({})", pg.studies.len()));

                    let is_linking = self.linking_source.is_some();
                    let pg_resp = ui.horizontal(|ui| {
                        let resp = egui::CollapsingHeader::new(
                            egui::RichText::new(&pt_label).strong(),
                        )
                        .id_salt(format!("pt_{}", pg.key))
                        .default_open(false)
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
                                    format!("âš  [files missing] {desc} â€” {}", study.date)
                                } else {
                                    format!("{desc} â€” {}", study.date)
                                };
                                let hdr_resp =
                                    egui::CollapsingHeader::new(egui::RichText::new(title))
                                        .id_salt(&study.uid)
                                        .default_open(false)
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
                                ui.small(sub.join(" Â· "));
                            }
                            if study.series.len() >= 2 {
                                ui.horizontal(|ui| {
                                    ui.small(tr.compare);
                                    if ui.small_button("1Ã—2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(1, 2)));
                                    }
                                    if ui.small_button("2Ã—2").clicked() {
                                        tile_study = Some((si, ViewportTiling::new(2, 2)));
                                    }
                                    if study.series.len() >= 5 && ui.small_button("2Ã—3").clicked()
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
                                let mpr_tag = if series.is_mpr_capable() { "Â·MPR" } else { "" };
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
                                    let thumb_size = vec2(38.0, 38.0);
                                    let (thumb_rect, thumb_resp) =
                                        ui.allocate_exact_size(thumb_size, Sense::click());
                                    let painter = ui.painter();
                                    painter.rect_filled(
                                        thumb_rect,
                                        4.0,
                                        Color32::from_rgb(12, 14, 18),
                                    );
                                    if let Some(tex) = self.thumbnails.get(&tkey) {
                                        painter.image(
                                            tex.id(),
                                            thumb_rect.shrink(1.0),
                                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
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
                                        "{:<2} {:>3} [{}{}] {}\n{} images Â· {}Ã—{}",
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
                                            "{} {}, {}Ã—{}, plane: {}{} (click for active viewport, right-click to assign)",
                                            series.slices.len(),
                                            tr.images,
                                            series.columns,
                                            series.rows,
                                            series.dominant_plane.badge(),
                                            if series.is_mpr_capable() { " Â· 3D MPR Ready" } else { "" }
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
                                                    "ðŸ§Š {}  [{}]",
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
}
