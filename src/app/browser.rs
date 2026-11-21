use super::*;
use crate::ui::icons::{self, Icon};
use crate::ui::theme;

/// Size of a tag (badge) around `g`: its letters' ink plus `pad_x` on each side.
fn tag_size(g: &egui::Galley, pad_x: f32) -> egui::Vec2 {
    vec2(g.mesh_bounds.width() + 2.0 * pad_x, g.size().y + 3.0)
}

/// Paints `g` centred on the letters' ink in `r`. Centring the text box instead
/// left the letters looking pushed to the right (the box carries trailing spacing).
fn paint_tag_text(painter: &egui::Painter, r: Rect, g: std::sync::Arc<egui::Galley>, color: Color32) {
    let pos = r.center() - g.mesh_bounds.center().to_vec2();
    painter.galley(pos, g, color);
}

fn paint_series_drag_preview(ui: &egui::Ui, index: &crate::dicom_index::Index) {
    let Some(payload) = egui::DragAndDrop::payload::<(usize, usize)>(ui.ctx()) else {
        return;
    };
    let (si, ri) = *payload;
    let Some(series) = index.studies.get(si).and_then(|st| st.series.get(ri)) else {
        return;
    };
    let Some(pos) = ui.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos())) else {
        return;
    };
    let desc = if series.description.is_empty() {
        "(no description)"
    } else {
        series.description.as_str()
    };
    let label = format!(
        "{} [{}] · {}",
        series.modality,
        crate::dicom_index::study_cut_badge(&index.studies[si].series, ri),
        desc
    );
    egui::Area::new(egui::Id::unique("series_drag_preview"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(pos + vec2(14.0, 14.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme::mix(ui.visuals().window_fill, theme::accent(), 0.22))
                .stroke(Stroke::new(1.5, theme::accent_bright()))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(label).strong().color(Color32::WHITE));
                });
        });
}

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

        if self.sidebar_compact {
            self.browser_compact(ui);
            return;
        }

        // Compact-sidebar toggle button (Back to Library is in the top-left toolbar)
        let active_si = self
            .active_cell()
            .selected
            .map(|(si, _)| si)
            .or(self.selected_db_study);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let compact_hover = match self.lang {
                crate::i18n::Lang::En => "Compact sidebar to thumbnail strip",
                crate::i18n::Lang::Es => "Compactar barra lateral a tira de miniaturas",
            };
            if icons::small_tool_button(ui, Icon::SidebarCompact, false)
                .on_hover_text(compact_hover)
                .clicked()
            {
                self.sidebar_compact = true;
            }
            let clear_w = if self.browser_search.is_empty() { 0.0 } else { 28.0 };
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width((ui.available_width() - clear_w - 4.0).max(60.0));
            ui.add(edit);
            if !self.browser_search.is_empty()
                && icons::small_tool_button(ui, Icon::Close, false).clicked()
            {
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
                .color(theme::dim_text(ui)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(tr.clear)
                    .on_hover_text(if self.lang == crate::i18n::Lang::Es { "Quitar los estudios de la biblioteca local (los archivos en el disco no se tocan)" } else { "Clear studies from the local library (files on disk are untouched)" })
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
                        || s.cut_badge().to_ascii_lowercase().contains(&query)
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
                        theme::warn_color(ui),
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
                                    match self.lang {
                                        crate::i18n::Lang::Es => format!("⚠ [faltan archivos] {desc} — {}", study.date),
                                        crate::i18n::Lang::En => format!("⚠ [files missing] {desc} — {}", study.date),
                                    }
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
                            // `compact`: a smaller, indented card, for reconstructions listed
                            // under the complete volume they were made from.
                            let draw_series_row = |ui: &mut egui::Ui,
                                                   ri: usize,
                                                   highlight_3d: bool,
                                                   compact: bool,
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

                                // Series card: thumbnail, description, then modality / image count / plane.
                                let row_w = ui.available_width().max(140.0);
                                // Reconstructions folded into this card (complete volumes only).
                                let kids: &[usize] = if compact {
                                    &[]
                                } else {
                                    study
                                        .reformats
                                        .iter()
                                        .find(|(p, _)| *p == ri)
                                        .map_or(&[][..], |(_, k)| k.as_slice())
                                };
                                let card_h = if compact { 40.0 } else { 60.0 };
                                let (row_rect, resp) = ui.allocate_exact_size(
                                    vec2(row_w, card_h),
                                    Sense::click_and_drag(),
                                );
                                let row_rect = if compact {
                                    row_rect.with_min_x(row_rect.min.x + 18.0)
                                } else {
                                    row_rect
                                };
                                if ui.is_rect_visible(row_rect) {
                                    let painter = ui.painter();
                                    let dark = ui.visuals().dark_mode;
                                    let bg = if is_active {
                                        theme::accent().gamma_multiply(if dark { 0.35 } else { 0.25 })
                                    } else if resp.hovered() {
                                        ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5)
                                    } else if compact {
                                        // A reconstruction: a lighter tint of the volume's colour.
                                        theme::accent().gamma_multiply(if dark { 0.14 } else { 0.10 })
                                    } else if highlight_3d {
                                        theme::accent().gamma_multiply(if dark { 0.08 } else { 0.06 })
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    painter.rect_filled(row_rect, 6.0, bg);
                                    if compact {
                                        // Thin line in the indent tying it to the volume above.
                                        let x = row_rect.min.x - 9.0;
                                        painter.line_segment(
                                            [pos2(x, row_rect.min.y - 2.0), pos2(x, row_rect.max.y + 2.0)],
                                            Stroke::new(1.5, theme::accent().gamma_multiply(0.55)),
                                        );
                                    }
                                    if is_active || highlight_3d {
                                        painter.rect_filled(
                                            Rect::from_min_size(
                                                row_rect.min + vec2(0.0, 6.0),
                                                vec2(if is_active { 3.0 } else { 2.0 }, row_rect.height() - 12.0),
                                            ),
                                            2.0,
                                            if is_active {
                                                theme::accent_bright()
                                            } else {
                                                theme::accent().gamma_multiply(0.6)
                                            },
                                        );
                                    }
                                    let thumb_side = if compact { 32.0 } else { 52.0 };
                                    let thumb_rect = Rect::from_min_size(
                                        row_rect.min + vec2(8.0, 4.0),
                                        vec2(thumb_side, thumb_side),
                                    );
                                    painter.rect_filled(thumb_rect, 4.0, Color32::BLACK);
                                    if let Some(tex) = self.thumbnails.get(&tkey) {
                                        paint_aspect_fit_thumb(painter, thumb_rect, tex);
                                    } else {
                                        painter.text(
                                            thumb_rect.center(),
                                            Align2::CENTER_CENTER,
                                            &series.modality,
                                            FontId::proportional(11.0),
                                            Color32::from_gray(120),
                                        );
                                    }
                                    painter.rect_stroke(
                                        thumb_rect,
                                        4.0,
                                        Stroke::new(
                                            1.0,
                                            if is_active {
                                                theme::accent_bright()
                                            } else {
                                                Color32::from_gray(70)
                                            },
                                        ),
                                        egui::StrokeKind::Inside,
                                    );

                                    let text_x = thumb_rect.max.x + 9.0;
                                    let text_w = (row_rect.max.x - text_x - 6.0).max(20.0);
                                    let title_color = if is_active {
                                        ui.visuals().strong_text_color()
                                    } else {
                                        ui.visuals().text_color()
                                    };
                                    let mut job = egui::text::LayoutJob::simple_singleline(
                                        desc.to_owned(),
                                        FontId::proportional(if compact { 11.5 } else { 12.5 }),
                                        title_color,
                                    );
                                    job.wrap = egui::text::TextWrapping {
                                        max_width: text_w,
                                        max_rows: if compact || !kids.is_empty() { 1 } else { 2 },
                                        break_anywhere: false,
                                        overflow_character: Some('…'),
                                    };
                                    let galley = painter.layout_job(job);
                                    let title_h = galley.size().y;
                                    painter.galley(
                                        pos2(text_x, row_rect.min.y + if compact { 3.0 } else { 6.0 }),
                                        galley,
                                        title_color,
                                    );

                                    // Second line: plane badge + "CT · #2 · 80 images".
                                    let line_y = (row_rect.min.y + if compact { 5.0 } else { 8.0 } + title_h)
                                        .min(row_rect.max.y - if compact { 17.0 } else if kids.is_empty() { 19.0 } else { 37.0 });
                                    let badge = crate::dicom_index::study_cut_badge(&study.series, ri);
                                    let badge_color = theme::plane_color(badge);
                                    let badge_galley = painter.layout_no_wrap(
                                        badge.to_owned(),
                                        FontId::proportional(9.5),
                                        badge_color,
                                    );
                                    let badge_rect = Rect::from_min_size(
                                        pos2(text_x, line_y),
                                        tag_size(&badge_galley, 4.0),
                                    );
                                    painter.rect_stroke(
                                        badge_rect,
                                        3.0,
                                        Stroke::new(1.0, badge_color.gamma_multiply(0.7)),
                                        egui::StrokeKind::Inside,
                                    );
                                    paint_tag_text(painter, badge_rect, badge_galley, badge_color);
                                    let info = format!(
                                        "{} · #{} · {} {}{}",
                                        series.modality,
                                        series.number,
                                        series.slices.len(),
                                        tr.images,
                                        if mpr_tag.is_empty() { "" } else { " · 3D" }
                                    );
                                    painter.text(
                                        pos2(badge_rect.max.x + 6.0, badge_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        info,
                                        FontId::proportional(10.5),
                                        theme::dim_text(ui),
                                    );
                                }
                                // Third line, inside the card: the planes of the reconstructions made
                                // from this volume. Clicking them shows or hides those series
                                // (hidden at every start).
                                if !kids.is_empty() {
                                    let open_id = egui::Id::unique(("reformats_open", &study.uid, ri));
                                    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
                                    // Complete volumes of the same reconstruction in other planes
                                    // read MPR-A/C/S (this card's own plane included); other
                                    // reconstructions read AX/COR/SAG.
                                    let mut planes: Vec<&'static str> = Vec::new();
                                    let has_twins = kids.iter().any(|&k| study.series[k].is_full_volume());
                                    if has_twins {
                                        planes.push(crate::dicom_index::lettered_badge(series));
                                    }
                                    for &k in kids {
                                        let ks = &study.series[k];
                                        let b = if ks.is_full_volume() {
                                            crate::dicom_index::lettered_badge(ks)
                                        } else {
                                            ks.dominant_plane.badge()
                                        };
                                        if !planes.contains(&b) {
                                            planes.push(b);
                                        }
                                    }
                                    planes.sort_by_key(|b| match *b {
                                        "MPR-A" => 0,
                                        "MPR-C" => 1,
                                        "MPR-S" => 2,
                                        "AX" => 3,
                                        "COR" => 4,
                                        "SAG" => 5,
                                        _ => 6,
                                    });
                                    let painter = ui.painter();
                                    // Bottom right of the card.
                                    let galleys: Vec<_> = planes
                                        .into_iter()
                                        .map(|b| {
                                            let color = theme::plane_color(b);
                                            (painter.layout_no_wrap(b.to_owned(), FontId::proportional(9.0), color), color)
                                        })
                                        .collect();
                                    let total: f32 = galleys.iter().map(|(g, _)| tag_size(g, 3.0).x).sum::<f32>()
                                        + 4.0 * galleys.len().saturating_sub(1) as f32;
                                    let mut x = row_rect.max.x - 7.0 - total;
                                    let y = row_rect.max.y - 18.0;
                                    let mut chips = Vec::new();
                                    for (g, color) in galleys {
                                        let r = Rect::from_min_size(pos2(x, y), tag_size(&g, 3.0));
                                        x = r.max.x + 4.0;
                                        chips.push((r, g, color));
                                    }
                                    let all = chips
                                        .iter()
                                        .fold(Rect::NOTHING, |acc, (r, _, _)| acc.union(*r))
                                        .expand(2.0);
                                    let chip_resp = ui
                                        .interact(all, egui::Id::unique(("reformats_chips", &study.uid, ri)), Sense::click())
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(tr.reformats_hint);
                                    if chip_resp.clicked() {
                                        ui.data_mut(|d| d.insert_temp(open_id, !open));
                                    }
                                    if ui.is_rect_visible(all) {
                                        let lit = open || chip_resp.hovered();
                                        for (r, g, color) in chips {
                                            if lit {
                                                ui.painter().rect_filled(r, 3.0, color.gamma_multiply(0.18));
                                            }
                                            ui.painter().rect_stroke(
                                                r,
                                                3.0,
                                                Stroke::new(1.0, color.gamma_multiply(if lit { 0.9 } else { 0.5 })),
                                                egui::StrokeKind::Inside,
                                            );
                                            paint_tag_text(ui.painter(), r, g, color);
                                        }
                                    }
                                }
                                let resp = resp.on_hover_text(format!(
                                    "{desc}\n{} {}, {}×{}, {}{}",
                                    series.slices.len(),
                                    tr.images,
                                    series.columns,
                                    series.rows,
                                    crate::dicom_index::study_cut_badge(&study.series, ri),
                                    if series.is_full_volume() {
                                        tr.full_volume_hint
                                    } else if series.is_mpr_capable() {
                                        " · 3D MPR"
                                    } else {
                                        ""
                                    }
                                ));
                                if resp.drag_started() {
                                    egui::DragAndDrop::set_payload(ui.ctx(), (si, ri));
                                }
                                if resp.clicked() {
                                    *clicked = Some((self.active_viewport, (si, ri), false));
                                }
                                resp.context_menu(|ui| {
                                    for vp_idx in 0..cell_count {
                                        let lbl = match self.lang {
                                            crate::i18n::Lang::En => format!("Open in Viewport {}", vp_idx + 1),
                                            crate::i18n::Lang::Es => format!("Abrir en visor {}", vp_idx + 1),
                                        };
                                        if ui.button(lbl).clicked() {
                                            *clicked = Some((vp_idx, (si, ri), false));
                                            ui.close();
                                        }
                                    }
                                });
                            };

                            // Reconstructions are listed under the complete volume they were made
                            // from (folded away by default), not on their own.
                            let folded: std::collections::HashSet<usize> = study
                                .reformats
                                .iter()
                                .filter(|(parent, _)| matching_series.contains(parent))
                                .flat_map(|(_, kids)| kids.iter().copied())
                                .collect();
                            let visible_series: Vec<usize> = matching_series
                                .iter()
                                .copied()
                                .filter(|ri| !folded.contains(ri))
                                .collect();
                            let matching_series = &visible_series;
                            let draw_reformats = |ui: &mut egui::Ui,
                                                  ri: usize,
                                                  clicked: &mut Option<(usize, (usize, usize), bool)>,
                                                  thumb_jobs: &mut Vec<Job>| {
                                let Some((_, kids)) = study.reformats.iter().find(|(p, _)| *p == ri) else {
                                    return;
                                };
                                let open_id = egui::Id::unique(("reformats_open", &study.uid, ri));
                                if ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
                                    for &k in kids {
                                        draw_series_row(ui, k, false, true, clicked, thumb_jobs);
                                    }
                                }
                            };

                            // Render series in list order, grouping multi-plane AX/COR/SAG reconstructions
                            // into a compact Horos-style block (no duplicate rows outside the block).
                            let mut rendered_series = std::collections::HashSet::new();
                            let mut single_image_series: Vec<usize> = Vec::new();
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

                                if grp.is_none() && crate::dicom_index::is_auxiliary(&study.series[ri]) {
                                    rendered_series.insert(ri);
                                    single_image_series.push(ri);
                                    continue;
                                }

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
                                            .fill(ui.visuals().faint_bg_color)
                                            .stroke(Stroke::new(
                                                1.0,
                                                if grp_active {
                                                    theme::accent().gamma_multiply(0.8)
                                                } else {
                                                    ui.visuals().widgets.noninteractive.bg_stroke.color
                                                },
                                            ))
                                            .corner_radius(7)
                                            .inner_margin(egui::Margin::symmetric(4, 4))
                                            .show(ui, |ui| {
                                                let grp_text = format!(
                                                    "{}  ·  {}",
                                                    group.label, group.planes_badge
                                                );
                                                let hdr = ui
                                                    .selectable_label(
                                                        grp_active,
                                                        egui::RichText::new(grp_text)
                                                            .strong()
                                                            .size(11.0)
                                                            .color(theme::accent_bright()),
                                                    )
                                                    .on_hover_text(if self.lang == crate::i18n::Lang::Es {
                                                        "Reconstrucciones axial / coronal / sagital agrupadas. Clic para abrir en MPR 3D."
                                                    } else {
                                                        "Grouped Axial / Coronal / Sagittal reconstructions. Click to open in 3D MPR."
                                                    });
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
                                                        false,
                                                        false,
                                                        &mut clicked,
                                                        &mut thumb_jobs,
                                                    );
                                                    draw_reformats(ui, pri, &mut clicked, &mut thumb_jobs);
                                                }
                                            });
                                    });
                                } else {
                                    rendered_series.insert(ri);
                                    let is_3d_standalone = study.series[ri].is_mpr_capable();
                                    draw_series_row(
                                        ui,
                                        ri,
                                        is_3d_standalone,
                                        false,
                                        &mut clicked,
                                        &mut thumb_jobs,
                                    );
                                    draw_reformats(ui, ri, &mut clicked, &mut thumb_jobs);
                                }
                            }
                            if !single_image_series.is_empty() {
                                let single_lbl = match self.lang {
                                    crate::i18n::Lang::Es => format!(
                                        "Extras: localizadores, dosis e imágenes sueltas ({})",
                                        single_image_series.len()
                                    ),
                                    crate::i18n::Lang::En => format!(
                                        "Extras: scouts, dose and single images ({})",
                                        single_image_series.len()
                                    ),
                                };
                                egui::CollapsingHeader::new(
                                    egui::RichText::new(single_lbl)
                                        .size(11.5)
                                        .color(theme::dim_text(ui)),
                                )
                                .id_salt(format!("single_imgs_{}", study.uid))
                                .default_open(false)
                                .show(ui, |ui| {
                                    for s_ri in single_image_series {
                                        draw_series_row(
                                            ui,
                                            s_ri,
                                            false,
                                            false,
                                            &mut clicked,
                                            &mut thumb_jobs,
                                        );
                                    }
                                });
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
                        .color(theme::warn_color(ui)),
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
            self.pending_clear_library = true;
            return;
        }
        if let Some(uid) = remove_study_uid {
            self.queue_remove_study(&uid);
            return;
        }

        if let Some(ref idx) = self.index {
            paint_series_drag_preview(ui, idx);
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

    /// Compact thumbnail-strip sidebar showing the current patient's acquisitions
    /// with a cut badge (`AX`/`COR`/`SAG`) under each thumbnail.
    fn browser_compact(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.vertical_centered(|ui| {
            let exp_hover = match self.lang {
                crate::i18n::Lang::En => "Expand sidebar",
                crate::i18n::Lang::Es => "Expandir barra lateral",
            };
            if icons::small_tool_button(ui, Icon::Sidebar, false)
                .on_hover_text(exp_hover)
                .clicked()
            {
                self.sidebar_compact = false;
            }
        });
        ui.separator();

        let active_si = self
            .active_cell()
            .selected
            .map(|(si, _)| si)
            .or(self.selected_db_study);
        let active_sel = self.active_cell().selected;
        let scan_gen = self.scan_gen;

        let Some(index) = &self.index else {
            return;
        };

        let patient_key = active_si
            .and_then(|si| index.studies.get(si))
            .map(|st| self.patient_group_key(st));
        let study_indices: Vec<usize> = index
            .studies
            .iter()
            .enumerate()
            .filter(|(_, st)| {
                patient_key
                    .as_ref()
                    .is_none_or(|pk| &self.patient_group_key(st) == pk)
            })
            .map(|(si, _)| si)
            .collect();

        let mut clicked: Option<(usize, (usize, usize))> = None;
        let mut thumb_jobs: Vec<Job> = Vec::new();
        let multi_study = study_indices.len() > 1;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for &si in &study_indices {
                    let study = &index.studies[si];
                    if multi_study {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(study.date.as_str())
                                    .size(9.5)
                                    .color(theme::dim_text(ui)),
                            );
                        });
                    }
                    for (ri, series) in study.series.iter().enumerate() {
                        let tkey = (scan_gen, si, ri);
                        self.queue_thumb_for_series(&mut thumb_jobs, si, ri, series);
                        let is_active = active_sel == Some((si, ri));

                        ui.vertical_centered(|ui| {
                            let thumb_size = vec2(68.0, 68.0);
                            let (thumb_rect, thumb_resp) =
                                ui.allocate_exact_size(thumb_size, Sense::click_and_drag());
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
                                    FontId::monospace(11.0),
                                    Color32::from_gray(145),
                                );
                            }
                            let border_col = if is_active {
                                theme::accent_bright()
                            } else if thumb_resp.hovered() {
                                Color32::from_gray(130)
                            } else {
                                Color32::from_gray(60)
                            };
                            painter.rect_stroke(
                                thumb_rect,
                                4.0,
                                Stroke::new(if is_active { 2.0 } else { 1.0 }, border_col),
                                egui::StrokeKind::Inside,
                            );

                            let desc = if series.description.is_empty() {
                                "(no description)"
                            } else {
                                series.description.as_str()
                            };
                            let cut_badge = crate::dicom_index::study_cut_badge(&study.series, ri);
                            let hover_txt = format!(
                                "{} · {} ({cut_badge})\n{} im",
                                series.modality,
                                desc,
                                series.slices.len()
                            );
                            let thumb_resp = thumb_resp.on_hover_text(hover_txt);
                            if thumb_resp.drag_started() {
                                egui::DragAndDrop::set_payload(ui.ctx(), (si, ri));
                            }
                            if thumb_resp.clicked() {
                                clicked = Some((self.active_viewport, (si, ri)));
                            }

                            let badge_color = theme::plane_color(cut_badge);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{cut_badge} · {}",
                                    series.slices.len()
                                ))
                                .size(10.0)
                                .strong()
                                .color(if is_active {
                                    Color32::WHITE
                                } else {
                                    badge_color
                                }),
                            );
                        });
                        ui.add_space(4.0);
                    }
                    if multi_study {
                        ui.separator();
                    }
                }
            });

        if let Some(ref idx) = self.index {
            paint_series_drag_preview(ui, idx);
        }

        for job in thumb_jobs {
            self.thumbnails_queued
                .insert((job.key.scan_gen, job.key.study, job.key.series));
            self.loader.request(job);
        }
        if let Some((vp_idx, sel)) = clicked {
            self.active_viewport = vp_idx;
            self.assign_series_to_viewport(vp_idx, Some(sel));
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
        let es = self.lang == crate::i18n::Lang::Es;
        ui.horizontal(|ui| {
            let (icon_rect, _) = ui.allocate_exact_size(vec2(16.0, 22.0), Sense::hover());
            icons::paint(
                ui.painter(),
                Rect::from_center_size(icon_rect.center(), vec2(13.0, 13.0)),
                Icon::Search,
                theme::dim_text(ui),
            );
            let edit = egui::TextEdit::singleline(&mut self.browser_search)
                .hint_text(tr.search_hint)
                .desired_width(280.0);
            ui.add(edit);
            if !self.browser_search.is_empty()
                && icons::small_tool_button(ui, Icon::Close, false).clicked()
            {
                self.browser_search.clear();
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} · {} {} ({disk_mb:.1} MB)",
                    tr.library, study_count, tr.studies
                ))
                .size(11.5)
                .color(theme::dim_text(ui)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr.clear).clicked() {
                    self.pending_clear_library = true;
                }
                let refresh_lbl = if es { "Actualizar" } else { "Refresh" };
                if ui.button(refresh_lbl).clicked() {
                    let dir = crate::db::managed_studies_dir();
                    if dir.is_dir() {
                        self.start_scan(vec![dir]);
                    }
                }
                if ui.button("PACS / DICOMweb…").clicked() {
                    self.show_pacs_modal = true;
                }
            });
        });
        ui.add_space(4.0);

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

        // Smart-album date helper (YYYYMMDD from UNIX epoch days, Howard Hinnant civil calendar).
        let yyyymmdd_days_ago = |days_ago: i64| -> String {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let z = (secs / 86_400) - days_ago + 719_468;
            let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
            let doe = (z - era * 146_097) as u64;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = y + i64::from(m <= 2);
            format!("{year:04}{m:02}{d:02}")
        };
        let cut_today = yyyymmdd_days_ago(0);
        let cut_7d = yyyymmdd_days_ago(7);
        let cut_30d = yyyymmdd_days_ago(30);

        let study_matches_album = |st: &crate::dicom_index::Study, code: &str| -> bool {
            let digits: String = st.date.chars().filter(|c| c.is_ascii_digit()).collect();
            match code {
                "ALL" => true,
                "TODAY" => digits.len() >= 8 && digits[..8] >= cut_today[..],
                "LAST7" => digits.len() >= 8 && digits[..8] >= cut_7d[..],
                "LAST30" => digits.len() >= 8 && digits[..8] >= cut_30d[..],
                "KEYS" => st.series.iter().any(|se| {
                    se.slices
                        .iter()
                        .any(|sl| self.key_images.contains(&(sl.path.clone(), sl.frame)))
                }),
                m => st.series.iter().any(|se| modality_matches(se, m)),
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
            let m_ok = study_matches_album(st, &mod_filter);
            if q_ok && m_ok {
                matching_studies.push(si);
            }
        }

        // Sort by the chosen column; ties keep a stable, predictable order (name, then date).
        {
            let (col, asc) = self.db_sort;
            let images = |st: &crate::dicom_index::Study| -> usize {
                st.series.iter().map(|s| s.slices.len()).sum()
            };
            let modality = |st: &crate::dicom_index::Study| -> String {
                let mut v: Vec<&str> = st.series.iter().map(|s| s.modality.as_str()).collect();
                v.sort_unstable();
                v.dedup();
                v.join("/")
            };
            matching_studies.sort_by(|&a, &b| {
                let (sa, sb) = (&index.studies[a], &index.studies[b]);
                let primary = match col {
                    1 => sa.series.len().cmp(&sb.series.len()),
                    2 => images(sa).cmp(&images(sb)),
                    3 => sa.description.to_lowercase().cmp(&sb.description.to_lowercase()),
                    4 => modality(sa).cmp(&modality(sb)),
                    5 => sa.date.cmp(&sb.date),
                    6 => sa.patient_id.cmp(&sb.patient_id),
                    _ => sa.patient_name.to_lowercase().cmp(&sb.patient_name.to_lowercase()),
                };
                let primary = if asc { primary } else { primary.reverse() };
                primary
                    .then_with(|| sa.patient_name.to_lowercase().cmp(&sb.patient_name.to_lowercase()))
                    .then_with(|| sb.date.cmp(&sa.date))
            });
        }

        let mut open_series: Option<(usize, usize)> = None;
        let mut tile_study: Option<usize> = None;
        let mut remove_uid: Option<String> = None;
        let mut thumb_jobs: Vec<Job> = Vec::new();
        let avail_h = ui.available_height();
        let bottom_strip_h = 204.0f32.min((avail_h * 0.38).max(150.0));
        let top_area_h = (avail_h - bottom_strip_h - 8.0).max(140.0);

        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), top_area_h),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                // Left albums list
                ui.allocate_ui_with_layout(
                    vec2(170.0, top_area_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(if es { "ÁLBUMES" } else { "ALBUMS" })
                                .size(10.0)
                                .strong()
                                .color(theme::dim_text(ui)),
                        );
                        ui.add_space(2.0);
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
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(162.0, 24.0), Sense::click());
                            let fill = if selected {
                                theme::accent()
                            } else if resp.hovered() {
                                ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 5.0, fill);
                            let fg = if selected {
                                Color32::WHITE
                            } else {
                                ui.visuals().text_color()
                            };
                            ui.painter().text(
                                pos2(rect.min.x + 10.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(13.0),
                                fg,
                            );
                            ui.painter().text(
                                pos2(rect.max.x - 10.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                cnt.to_string(),
                                FontId::proportional(11.5),
                                if selected { fg } else { theme::dim_text(ui) },
                            );
                            if resp.clicked() {
                                self.browser_modality = code.to_string();
                            }
                        }
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(if es {
                                "ÁLBUMES INTELIGENTES"
                            } else {
                                "SMART ALBUMS"
                            })
                            .size(10.0)
                            .strong()
                            .color(theme::dim_text(ui)),
                        );
                        ui.add_space(2.0);
                        for (code, label) in [
                            ("TODAY", if es { "Hoy" } else { "Today" }),
                            ("LAST7", if es { "Últimos 7 días" } else { "Last 7 Days" }),
                            ("LAST30", if es { "Últimos 30 días" } else { "Last 30 Days" }),
                            ("KEYS", if es { "Imágenes clave" } else { "Key Images" }),
                        ] {
                            let cnt = index
                                .studies
                                .iter()
                                .filter(|st| study_matches_album(st, code))
                                .count();
                            let selected = self.browser_modality == code;
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(162.0, 24.0), Sense::click());
                            let fill = if selected {
                                theme::accent()
                            } else if resp.hovered() {
                                ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 5.0, fill);
                            let fg = if selected {
                                Color32::WHITE
                            } else {
                                ui.visuals().text_color()
                            };
                            ui.painter().text(
                                pos2(rect.min.x + 10.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(12.5),
                                fg,
                            );
                            ui.painter().text(
                                pos2(rect.max.x - 10.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                cnt.to_string(),
                                FontId::proportional(11.5),
                                if selected { fg } else { theme::dim_text(ui) },
                            );
                            if resp.clicked() {
                                self.browser_modality = code.to_string();
                            }
                        }
                    },
                );
                ui.add_space(4.0);

                // Study table
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let table_w = ui.available_width().max(640.0);
                    // name | # | # im | description | modality | date | patient id | actions
                    let (w_ser, w_im, w_mod, w_date, w_id, w_act) =
                        (44.0, 60.0, 80.0, 104.0, 130.0, 92.0);
                    let pad = 12.0;
                    let fixed_w = w_ser + w_im + w_mod + w_date + w_id + w_act + pad * 2.0;
                    let flex_w = (table_w - fixed_w).max(260.0);
                    let w_name = (flex_w * 0.42).max(150.0);
                    let w_desc = (flex_w - w_name).max(110.0);
                    let cols = [w_name, w_ser, w_im, w_desc, w_mod, w_date, w_id];
                    let headers: [&str; 7] = if es {
                        ["Paciente", "Ser.", "Imág.", "Descripción del estudio", "Mod.", "Fecha", "ID paciente"]
                    } else {
                        ["Patient name", "Ser.", "Images", "Study description", "Modality", "Date", "Patient ID"]
                    };
                    let right_aligned = [false, true, true, false, false, false, false];
                    let dark = ui.visuals().dark_mode;

                    // Header strip
                    let (hdr_rect, _) =
                        ui.allocate_exact_size(vec2(table_w, 24.0), Sense::hover());
                    ui.painter().rect_filled(
                        hdr_rect,
                        egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 },
                        if dark {
                            if theme::oled_mode() {
                                Color32::from_rgb(12, 12, 15)
                            } else {
                                theme::RAISED
                            }
                        } else {
                            Color32::from_rgb(220, 222, 227)
                        },
                    );
                    // Click a header to sort by it; click again to reverse.
                    let mut x = hdr_rect.min.x + pad;
                    for (i, w) in cols.iter().enumerate() {
                        let cell = Rect::from_min_max(
                            pos2(x - 4.0, hdr_rect.min.y),
                            pos2(x + w - 6.0, hdr_rect.max.y),
                        );
                        let resp = ui.interact(
                            cell,
                            ui.scope_id().with(("db_hdr", i)),
                            Sense::click(),
                        );
                        let sorted = self.db_sort.0 == i;
                        let color = if sorted || resp.hovered() {
                            ui.visuals().strong_text_color()
                        } else {
                            theme::dim_text(ui)
                        };
                        let (px, align) = if right_aligned[i] {
                            (x + w - 14.0, Align2::RIGHT_CENTER)
                        } else {
                            (x, Align2::LEFT_CENTER)
                        };
                        let text_rect = ui.painter().text(
                            pos2(px, hdr_rect.center().y),
                            align,
                            headers[i],
                            FontId::proportional(11.0),
                            color,
                        );
                        if sorted {
                            // Small triangle: up = ascending, down = descending.
                            let cx = if right_aligned[i] {
                                text_rect.min.x - 8.0
                            } else {
                                text_rect.max.x + 8.0
                            };
                            let cy = hdr_rect.center().y;
                            let d = if self.db_sort.1 { -1.0 } else { 1.0 };
                            ui.painter().add(egui::Shape::convex_polygon(
                                vec![
                                    pos2(cx - 3.5, cy - 2.0 * d),
                                    pos2(cx + 3.5, cy - 2.0 * d),
                                    pos2(cx, cy + 2.5 * d),
                                ],
                                color,
                                Stroke::NONE,
                            ));
                        }
                        if resp.clicked() {
                            self.db_sort = if sorted { (i, !self.db_sort.1) } else { (i, true) };
                            if let Some(ref db) = self.db {
                                let _ = db.set_setting(
                                    "db_sort",
                                    &format!("{i},{}", if self.db_sort.1 { 1 } else { 0 }),
                                );
                            }
                        }
                        x += w;
                    }

                    let body_fill = ui.visuals().extreme_bg_color;
                    egui::Frame::new().fill(body_fill).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("db_study_table")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if matching_studies.is_empty() {
                                ui.add_space(16.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(pad);
                                    ui.label(tr.no_studies);
                                });
                            }
                            for (row_i, &si) in matching_studies.iter().enumerate() {
                                let st = &index.studies[si];
                                let is_selected = self.selected_db_study == Some(si);
                                let im_total: usize = st.series.iter().map(|s| s.slices.len()).sum();
                                let mods: Vec<&str> = {
                                    let mut v: Vec<&str> = st.series.iter().map(|s| s.modality.as_str()).collect();
                                    v.sort_unstable();
                                    v.dedup();
                                    v
                                };
                                let mod_str = mods.join(" / ");
                                let pname = if st.patient_name.trim().is_empty() {
                                    if es { "(Sin nombre)".to_string() } else { "(Unnamed)".to_string() }
                                } else {
                                    st.patient_name.replace('^', " ")
                                };

                                let (row_rect, row_resp) = ui.allocate_exact_size(
                                    vec2(table_w, 26.0),
                                    Sense::click(),
                                );
                                let row_bg = if is_selected {
                                    theme::accent()
                                } else if row_resp.hovered() {
                                    ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.45)
                                } else if row_i % 2 == 1 {
                                    ui.visuals().faint_bg_color
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(row_rect, 0.0, row_bg);
                                let fg = if is_selected {
                                    Color32::WHITE
                                } else {
                                    ui.visuals().text_color()
                                };
                                let fg_dim = if is_selected {
                                    Color32::from_gray(235)
                                } else {
                                    theme::dim_text(ui)
                                };
                                let cells: [(String, bool); 7] = [
                                    (pname, true),
                                    (st.series.len().to_string(), false),
                                    (im_total.to_string(), false),
                                    (st.description.clone(), false),
                                    (mod_str, false),
                                    (st.date.clone(), false),
                                    (st.patient_id.clone(), false),
                                ];
                                let mut x = row_rect.min.x + pad;
                                for (i, (text, primary)) in cells.into_iter().enumerate() {
                                    let w = cols[i];
                                    let color = if primary || i == 3 { fg } else { fg_dim };
                                    let mut job = egui::text::LayoutJob::simple_singleline(
                                        text,
                                        FontId::proportional(if primary { 13.0 } else { 12.5 }),
                                        color,
                                    );
                                    job.wrap = egui::text::TextWrapping {
                                        max_width: w - 14.0,
                                        max_rows: 1,
                                        break_anywhere: true,
                                        overflow_character: Some('…'),
                                    };
                                    let galley = ui.painter().layout_job(job);
                                    let gx = if right_aligned[i] {
                                        x + w - 14.0 - galley.size().x
                                    } else {
                                        x
                                    };
                                    ui.painter().galley(
                                        pos2(gx, row_rect.center().y - galley.size().y * 0.5),
                                        galley,
                                        color,
                                    );
                                    x += w;
                                }

                                // Row actions, shown on the selected or hovered row.
                                let act_rect = Rect::from_min_max(
                                    pos2(row_rect.max.x - w_act - pad, row_rect.min.y + 2.0),
                                    pos2(row_rect.max.x - pad, row_rect.max.y - 2.0),
                                );
                                let show_actions = is_selected
                                    || ui.rect_contains_pointer(row_rect);
                                if show_actions {
                                    let mut act_ui = ui.new_child(
                                        egui::UiBuilder::new().max_rect(act_rect).layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                        ),
                                    );
                                    act_ui.spacing_mut().item_spacing.x = 2.0;
                                    if is_selected {
                                        act_ui.visuals_mut().widgets.inactive.fg_stroke.color =
                                            Color32::WHITE;
                                    }
                                    let (h_remove, h_tile, h_open) = if es {
                                        ("Quitar de la Biblioteca", "Abrir todas las series en mosaico", "Abrir en el visor")
                                    } else {
                                        ("Remove from Library", "Open all series tiled", "Open in viewer")
                                    };
                                    if icons::small_tool_button(&mut act_ui, Icon::Trash, false)
                                        .on_hover_text(h_remove)
                                        .clicked()
                                    {
                                        remove_uid = Some(st.uid.clone());
                                    }
                                    if icons::small_tool_button(&mut act_ui, Icon::Grid, false)
                                        .on_hover_text(h_tile)
                                        .clicked()
                                    {
                                        tile_study = Some(si);
                                    }
                                    if icons::small_tool_button(&mut act_ui, Icon::Eye, false)
                                        .on_hover_text(h_open)
                                        .clicked()
                                    {
                                        open_series = Some((si, 0));
                                    }
                                }

                                if row_resp.clicked() {
                                    self.selected_db_study = Some(si);
                                }
                                if row_resp.double_clicked() && !st.series.is_empty() {
                                    open_series = Some((si, 0));
                                }
                            }
                        });
                    });
                });
            },
        );

        ui.add_space(6.0);

        // Bottom Series Thumbnail Grid for the selected study
        let sel_si = self
            .selected_db_study
            .filter(|&i| i < index.studies.len())
            .or_else(|| matching_studies.first().copied());
        if let Some(si) = sel_si {
            let st = &index.studies[si];
            let strip_top = ui.cursor().min.y;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(st.patient_name.replace('^', " "))
                        .strong()
                        .size(13.0),
                );
                let mut sub = Vec::new();
                if !st.description.trim().is_empty() {
                    sub.push(st.description.trim().to_string());
                }
                if !st.date.is_empty() {
                    sub.push(st.date.clone());
                }
                sub.push(format!("{} {}", st.series.len(), "series"));
                ui.label(
                    egui::RichText::new(sub.join("  ·  "))
                        .size(11.5)
                        .color(theme::dim_text(ui)),
                );
            });
            // Study details on the right, series thumbnails on the left.
            let rest = ui.available_rect_before_wrap();
            let details_w = 340.0f32.min(rest.width() * 0.4);
            let details_rect = Rect::from_min_max(
                pos2(rest.max.x - details_w, strip_top),
                pos2(rest.max.x, rest.max.y - 2.0),
            );
            {
                let mut d_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(details_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                egui::Frame::new()
                    .fill(d_ui.visuals().faint_bg_color)
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(10))
                    .show(&mut d_ui, |ui| {
                        ui.set_width(details_w - 20.0);
                        let im_total: usize = st.series.iter().map(|s| s.slices.len()).sum();
                        let rows: [(&str, String); 7] = if es {
                            [
                                ("Paciente", st.patient_name.replace('^', " ")),
                                ("ID", st.patient_id.clone()),
                                (
                                    "Nacimiento",
                                    crate::app::overlays::format_dob_with_age(
                                        &st.birth_date,
                                        &st.date,
                                    ),
                                ),
                                ("Estudio", st.description.clone()),
                                ("Fecha", st.date.clone()),
                                ("N.º de acceso", st.accession.clone()),
                                ("Contenido", format!("{} series · {im_total} {}", st.series.len(), tr.images)),
                            ]
                        } else {
                            [
                                ("Patient", st.patient_name.replace('^', " ")),
                                ("ID", st.patient_id.clone()),
                                (
                                    "Birth date",
                                    crate::app::overlays::format_dob_with_age(
                                        &st.birth_date,
                                        &st.date,
                                    ),
                                ),
                                ("Study", st.description.clone()),
                                ("Date", st.date.clone()),
                                ("Accession", st.accession.clone()),
                                ("Contents", format!("{} series · {im_total} {}", st.series.len(), tr.images)),
                            ]
                        };
                        egui::Grid::new("db_study_details")
                            .num_columns(2)
                            .min_row_height(17.0)
                            .spacing([12.0, 2.0])
                            .show(ui, |ui| {
                                for (k, v) in rows {
                                    ui.label(
                                        egui::RichText::new(k).size(11.0).color(theme::dim_text(ui)),
                                    );
                                    let v = if v.trim().is_empty() { "—".to_string() } else { v };
                                    ui.add(egui::Label::new(egui::RichText::new(v).size(12.0)).truncate());
                                    ui.end_row();
                                }
                            });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let (l_open, l_tile) = if es {
                                ("Abrir estudio", "Abrir en mosaico")
                            } else {
                                ("Open study", "Open tiled")
                            };
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(l_open).color(Color32::WHITE),
                                    )
                                    .fill(theme::accent()),
                                )
                                .clicked()
                                && !st.series.is_empty()
                            {
                                open_series = Some((si, 0));
                            }
                            if st.series.len() > 1 && ui.button(l_tile).clicked() {
                                tile_study = Some(si);
                            }
                        });
                    });
            }
            egui::ScrollArea::horizontal()
                .id_salt("db_series_grid")
                .max_width((rest.width() - details_w - 8.0).max(120.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (ri, series) in st.series.iter().enumerate() {
                            let tkey = self.queue_thumb_for_series(&mut thumb_jobs, si, ri, series);
                            let card_size = vec2(114.0, 126.0);
                            let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());
                            let painter = ui.painter();
                            let bg = if resp.hovered() {
                                ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.6)
                            } else {
                                ui.visuals().faint_bg_color
                            };
                            painter.rect_filled(rect, 6.0, bg);
                            if resp.hovered() {
                                painter.rect_stroke(
                                    rect,
                                    6.0,
                                    Stroke::new(1.0, theme::accent_bright()),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            let img_rect = Rect::from_min_size(rect.min + vec2(13.0, 6.0), vec2(88.0, 88.0));
                            painter.rect_filled(img_rect, 3.0, Color32::BLACK);
                            if let Some(tex) = self.thumbnails.get(&tkey) {
                                paint_aspect_fit_thumb(painter, img_rect, tex);
                            } else {
                                painter.text(
                                    img_rect.center(),
                                    Align2::CENTER_CENTER,
                                    &series.modality,
                                    FontId::proportional(13.0),
                                    Color32::from_gray(120),
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
                                FontId::proportional(10.5),
                                ui.visuals().text_color(),
                            );
                            painter.text(
                                pos2(rect.center().x, img_rect.max.y + 17.0),
                                Align2::CENTER_TOP,
                                format!("{} · {} {}", series.modality, series.slices.len(), tr.images),
                                FontId::proportional(9.5),
                                theme::dim_text(ui),
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
            self.queue_remove_study(&uid);
        } else if let Some(si) = tile_study {
            if !self.apply_hanging_protocol_for_study(si, None) {
                self.tile_study_series(si);
            }
            self.show_database_view = false;
        } else if let Some((si, ri)) = open_series {
            if !self.apply_hanging_protocol_for_study(si, Some(ri)) {
                self.active_viewport = 0;
                self.assign_series_to_viewport(0, Some((si, ri)));
                self.populate_unassigned_viewports();
            }
            self.show_database_view = false;
        }
    }

    pub(crate) fn queue_remove_study(&mut self, uid: &str) {
        let label = self
            .index
            .as_ref()
            .and_then(|idx| idx.studies.iter().find(|s| s.uid == uid))
            .map(|s| {
                let name = if s.patient_name.trim().is_empty() {
                    "Unknown"
                } else {
                    s.patient_name.trim()
                };
                if s.description.trim().is_empty() {
                    name.to_string()
                } else {
                    format!("{name} — {}", s.description.trim())
                }
            })
            .unwrap_or_else(|| uid.to_string());
        self.pending_remove_study = Some((uid.to_string(), label));
    }

    pub(crate) fn study_disk_paths(&self, uid: &str) -> Vec<std::path::PathBuf> {
        let mut set: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        if let Some(db) = &self.db {
            for p in db.study_source_paths(uid) {
                if p.exists() {
                    set.insert(p);
                }
            }
        }
        let managed_root = crate::db::managed_studies_dir();
        if let Some(idx) = &self.index
            && let Some(study) = idx.studies.iter().find(|s| s.uid == uid)
        {
            for ser in &study.series {
                for sl in &ser.slices {
                    if let Some(zip_p) =
                        crate::dicom_index::original_zip_for_extracted_path(&sl.path)
                    {
                        if zip_p.exists() {
                            set.insert(zip_p);
                        }
                    } else if !sl.path.starts_with(&managed_root) {
                        let p_str = sl.path.to_string_lossy();
                        if !p_str.contains("sekhmetx-zip-")
                            && !p_str.contains("rdicom-zip-cache")
                            && sl.path.exists()
                        {
                            set.insert(sl.path.clone());
                        }
                    }
                }
            }
        }
        set.into_iter().collect()
    }

    pub(crate) fn study_file_count(&self, uid: &str) -> usize {
        let disk = self.study_disk_paths(uid);
        if !disk.is_empty() {
            return disk.len();
        }
        let Some(idx) = &self.index else {
            return 0;
        };
        let Some(study) = idx.studies.iter().find(|s| s.uid == uid) else {
            return 0;
        };
        let paths: std::collections::HashSet<&std::path::PathBuf> = study
            .series
            .iter()
            .flat_map(|ser| ser.slices.iter().map(|sl| &sl.path))
            .collect();
        paths.len()
    }

    pub(crate) fn execute_remove_study(&mut self, uid: &str, delete_files: bool) {
        let mut deleted = 0usize;
        let mut failed = 0usize;
        let mut had_source_paths = false;
        if delete_files {
            let paths = self.study_disk_paths(uid);
            had_source_paths = !paths.is_empty();
            if !paths.is_empty() {
                match trash::delete_all(&paths) {
                    Ok(()) => deleted = paths.len(),
                    Err(_) => {
                        for p in paths {
                            match trash::delete(&p) {
                                Ok(()) => deleted += 1,
                                Err(_) => failed += 1,
                            }
                        }
                    }
                }
            }
        }
        if let Some(db) = &self.db {
            let _ = db.remove_study(uid);
        }
        if let Some(idx) = &mut self.index {
            idx.studies.retain(|s| s.uid != uid);
            if idx.studies.is_empty() {
                self.index = None;
                self.selected_db_study = None;
            } else if let Some(sel) = self.selected_db_study
                && sel >= idx.studies.len()
            {
                self.selected_db_study = Some(idx.studies.len() - 1);
            }
        }
        self.scan_gen = self.scan_gen.wrapping_add(1);
        self.thumbnails.clear();
        self.thumbnails_queued.clear();
        self.volumes.clear();
        self.series_spacing_override.clear();
        for vp in &mut self.viewports {
            vp.reset_series_state(None);
        }
        if !self.show_database_view && self.index.is_some() {
            self.assign_series_to_viewport(0, Some((0, 0)));
        }
        if delete_files {
            self.message = Some(match self.lang {
                crate::i18n::Lang::Es => {
                    if !had_source_paths {
                        "Estudio eliminado de la biblioteca (no había ruta original en disco registrada para esta importación previa).".to_string()
                    } else if failed == 0 {
                        format!("Estudio eliminado y {deleted} archivo(s) enviado(s) a la papelera.")
                    } else {
                        format!("Estudio eliminado: {deleted} archivo(s) a la papelera, {failed} con error.")
                    }
                }
                crate::i18n::Lang::En => {
                    if !had_source_paths {
                        "Study removed from library (original disk path was not recorded for this older import).".to_string()
                    } else if failed == 0 {
                        format!("Study removed and {deleted} file(s) moved to Recycle Bin.")
                    } else {
                        format!("Study removed: {deleted} file(s) moved to Recycle Bin, {failed} failed.")
                    }
                }
            });
        }
    }
}
