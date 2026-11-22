use super::*;

impl ViewerApp {
    /// Modal confirmation dialog when the user forces 3D / Orthogonal MPR on a non-3D-capable series.
    pub(super) fn draw_force_mpr_modal(&mut self, ctx: &egui::Context) {
        let Some(req) = self.pending_force_mpr else {
            return;
        };
        let tr = crate::i18n::tr(self.lang);
        let mut open = true;
        let mut decision: Option<bool> = None;

        egui::Window::new(tr.force_3d_title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(egui::RichText::new(tr.force_3d_headline).strong());
                ui.add_space(6.0);
                if let Some(err) = self.volume_errors.get(&req.key) {
                    ui.colored_label(crate::ui::theme::warn_color(ui), err.reason());
                    ui.add_space(6.0);
                }
                ui.label(tr.force_3d_body);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new(tr.force_3d_yes).strong())
                        .clicked()
                    {
                        decision = Some(true);
                    }
                    if ui.button(tr.force_3d_no).clicked() {
                        decision = Some(false);
                    }
                });
            });

        if !open && decision.is_none() {
            decision = Some(false);
        }

        if let Some(yes) = decision {
            self.pending_force_mpr = None;
            if yes {
                self.forced_mpr_series.insert(req.key);
                self.viewports[req.vp_idx].selected = Some(req.key);
                self.pending_mpr_activate = Some((req.vp_idx, req.key, req.ortho));
            }
        }
    }

    pub(crate) fn start_update_check(&mut self, open_modal: bool, ctx: &egui::Context) {
        if open_modal {
            self.show_update_modal = true;
        }
        if self.update_rx.is_some() || self.install_rx.is_some() {
            return;
        }
        self.update_status = crate::updater::UpdateStatus::Checking;
        let ctx_clone = ctx.clone();
        self.update_rx = Some(crate::updater::spawn_check(move || {
            ctx_clone.request_repaint();
        }));
    }

    pub(super) fn draw_update_modal(&mut self, ctx: &egui::Context) {
        if !self.show_update_modal {
            return;
        }
        let tr = crate::i18n::tr(self.lang);
        let mut open = true;
        let status = self.update_status.clone();
        let mut action_check = false;
        let mut action_install: Option<(String, Option<String>, Option<String>, String)> = None;
        let mut action_restart = false;
        let es = self.lang == crate::i18n::Lang::Es;

        egui::Window::new(tr.updates_title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.set_max_width(460.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{}: v{}",
                        tr.current_version,
                        env!("CARGO_PKG_VERSION")
                    ))
                    .monospace()
                    .color(crate::ui::theme::dim_text(ui)),
                );
                ui.add_space(6.0);

                match &status {
                    crate::updater::UpdateStatus::Idle
                    | crate::updater::UpdateStatus::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr.checking_updates);
                        });
                    }
                    crate::updater::UpdateStatus::UpToDate(v) => {
                        ui.label(
                            egui::RichText::new(format!("{} (v{v}).", tr.up_to_date))
                                .strong()
                                .color(crate::ui::theme::ok_color(ui)),
                        );
                        ui.add_space(8.0);
                        if ui.button(tr.check_again).clicked() {
                            action_check = true;
                        }
                    }
                    crate::updater::UpdateStatus::Available(info) => {
                        ui.label(
                            egui::RichText::new(format!("{}: {}", tr.new_version, info.tag_name))
                                .strong()
                                .color(crate::ui::theme::accent()),
                        );
                        if !info.notes.trim().is_empty() {
                            ui.add_space(6.0);
                            egui::ScrollArea::vertical()
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    ui.label(info.notes.trim());
                                });
                        }
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if let Some(url) = &info.download_url
                                && ui
                                    .button(egui::RichText::new(tr.download_install).strong())
                                    .clicked()
                                {
                                    action_install = Some((
                                        url.clone(),
                                        info.asset_name.clone(),
                                        info.sha256sums_url.clone(),
                                        info.tag_name.clone(),
                                    ));
                                }
                            ui.hyperlink_to(tr.open_github, &info.html_url);
                        });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(if es {
                                "Nota: los ejecutables publicados no están firmados; su integridad se verifica con SHA256SUMS."
                            } else {
                                "Note: Release binaries are unsigned; integrity is verified via SHA256SUMS."
                            })
                            .small()
                            .color(crate::ui::theme::dim_text(ui)),
                        );
                    }
                    crate::updater::UpdateStatus::Installing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr.installing_update);
                        });
                    }
                    crate::updater::UpdateStatus::Installed(tag) => {
                        ui.label(
                            egui::RichText::new(if es {
                                format!("SekhmetX {tag} instalado. Reinicie para abrir la nueva versión.")
                            } else {
                                format!("SekhmetX {tag} installed! Restart to launch the new version.")
                            })
                            .strong()
                            .color(crate::ui::theme::ok_color(ui)),
                        );
                        ui.add_space(10.0);
                        if ui
                            .button(egui::RichText::new(tr.restart_sekhmetx).strong())
                            .clicked()
                        {
                            action_restart = true;
                        }
                    }
                    crate::updater::UpdateStatus::Error(err) => {
                        ui.label(
                            egui::RichText::new(if es {
                                format!("No se pudo buscar actualizaciones: {err}")
                            } else {
                                format!("Update check failed: {err}")
                            })
                            .color(crate::ui::theme::error_color(ui)),
                        );
                        ui.add_space(8.0);
                        if ui.button(tr.retry).clicked() {
                            action_check = true;
                        }
                    }
                }
                ui.add_space(8.0);
                ui.separator();
                if ui
                    .checkbox(
                        &mut self.check_updates_on_startup,
                        if es { "Buscar actualizaciones al iniciar" } else { "Check for updates on startup" },
                    )
                    .changed()
                    && let Some(ref db) = self.db {
                        crate::settings::AppSettings::new(db).set_check_updates_on_startup(self.check_updates_on_startup);
                    }
            });

        if !open {
            self.show_update_modal = false;
        }
        if action_check {
            self.start_update_check(true, ctx);
        }
        if let Some((url, asset_name, sha256sums_url, tag)) = action_install {
            self.update_status = crate::updater::UpdateStatus::Installing;
            let ctx_clone = ctx.clone();
            self.install_rx = Some(crate::updater::spawn_install(
                url,
                asset_name,
                sha256sums_url,
                tag,
                move || {
                    ctx_clone.request_repaint();
                },
            ));
        }
        if action_restart
            && let Err(e) = crate::updater::restart_application() {
                self.update_status = crate::updater::UpdateStatus::Error(format!("{e:#}"));
            }
    }

    pub(super) fn draw_crash_modal(&mut self, ctx: &egui::Context) {
        if !self.show_crash_modal {
            return;
        }
        let mut open = true;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Registro de Fallo Anterior",
            crate::i18n::Lang::En => "Previous Crash Log",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let desc = match self.lang {
                    crate::i18n::Lang::Es => "SekhmetX se cerró inesperadamente en la sesión anterior. Puede guardar el registro para diagnosticarlo; luego se eliminará automáticamente.",
                    crate::i18n::Lang::En => "SekhmetX closed unexpectedly on the previous run. Save the crash log to inspect or share it; it will be deleted automatically afterward.",
                };
                ui.label(desc);
                ui.add_space(6.0);
                if let Some(log_text) = crate::crash_log::read_log() {
                    let scrubbed = crate::crash_log::prepared_public_report(&log_text);
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        ui.monospace(scrubbed);
                    });
                    ui.add_space(4.0);
                    let pub_warn = match self.lang {
                        crate::i18n::Lang::Es => "Aviso: si envía el reporte en GitHub, el texto de arriba será público.",
                        crate::i18n::Lang::En => "Notice: submitting on GitHub makes the exact text above public.",
                    };
                    ui.colored_label(crate::ui::theme::warn_color(ui), pub_warn);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let tr = crate::i18n::tr(self.lang);
                    if self.crash_report_opt_in && ui.button(tr.crash_report_github).clicked() {
                        if let Some(text) = crate::crash_log::read_log() {
                            let url = crate::crash_log::github_issue_url(&text);
                            ctx.open_url(egui::OpenUrl::new_tab(url));
                        }
                        crate::crash_log::delete_log();
                        self.previous_crash_pending = false;
                        self.show_crash_modal = false;
                    }
                    let save_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Guardar Registro y Eliminar…",
                        crate::i18n::Lang::En => "Save Crash Log & Delete…",
                    };
                    if ui.button(save_lbl).clicked()
                        && let Some(dest) = rfd::FileDialog::new()
                            .set_file_name("sekhmetx-crash.log")
                            .save_file()
                        {
                            if let Some(text) = crate::crash_log::read_log() {
                                let _ = std::fs::write(dest, text);
                            }
                            crate::crash_log::delete_log();
                            self.previous_crash_pending = false;
                            self.show_crash_modal = false;
                        }
                    let dismiss_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Descartar y Eliminar",
                        crate::i18n::Lang::En => "Dismiss & Delete",
                    };
                    if ui.button(dismiss_lbl).clicked() {
                        crate::crash_log::delete_log();
                        self.previous_crash_pending = false;
                        self.show_crash_modal = false;
                    }
                });
            });
        if !open {
            self.show_crash_modal = false;
        }
    }

    pub(super) fn draw_settings_modal(&mut self, ctx: &egui::Context) {
        if !self.show_settings_modal {
            return;
        }
        let tr = crate::i18n::tr(self.lang);
        let mut open = true;
        egui::Window::new(tr.settings_title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr.settings_language);
                    if crate::ui::icons::choice(ui, self.lang == crate::i18n::Lang::En, "English")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::En;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_language(crate::i18n::Lang::En);
                        }
                    }
                    if crate::ui::icons::choice(ui, self.lang == crate::i18n::Lang::Es, "Español")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::Es;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_language(crate::i18n::Lang::Es);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let (theme_lbl, lbl_dark, lbl_light, lbl_sys) = match self.lang {
                        crate::i18n::Lang::Es => ("Tema:", "Oscuro", "Claro", "Sistema"),
                        crate::i18n::Lang::En => ("Theme:", "Dark", "Light", "System"),
                    };
                    ui.label(theme_lbl);
                    for (mode, lbl) in [
                        (super::ThemeMode::Dark, lbl_dark),
                        (super::ThemeMode::Oled, "OLED"),
                        (super::ThemeMode::Light, lbl_light),
                        (super::ThemeMode::System, lbl_sys),
                    ] {
                        if crate::ui::icons::choice(ui, self.theme_mode == mode, lbl).clicked() {
                            self.theme_mode = mode;
                            mode.apply(ctx);
                            if let Some(ref db) = self.db {
                                let _ = db.set_setting("theme_mode", mode.as_str());
                            }
                        }
                    }
                });
                self.draw_accent_settings(ui, ctx);

                crate::ui::icons::section_title(ui, tr.settings_studies_folder);
                ui.horizontal(|ui| {
                    let cur = crate::db::managed_studies_dir();
                    crate::ui::icons::path_box(ui, &cur.display().to_string());
                    if ui.button(tr.choose_folder).clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.select_studies_folder(dir);
                    }
                    if ui.button(tr.settings_reset_default).clicked() {
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_custom_studies_dir(None);
                        }
                        crate::db::set_custom_studies_dir(None);
                    }
                });

                crate::ui::icons::section_title(ui, tr.settings_on_open);
                ui.horizontal(|ui| {
                    ui.label(tr.settings_add_to_db);
                    for (policy, label) in [
                        (crate::db::CopyToLibraryPolicy::Ask, tr.settings_policy_ask),
                        (crate::db::CopyToLibraryPolicy::Always, tr.settings_policy_always),
                        (crate::db::CopyToLibraryPolicy::Never, tr.settings_policy_never),
                    ] {
                        if crate::ui::icons::choice(ui, self.copy_policy == policy, label).clicked() {
                            self.set_copy_policy(policy);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr.settings_storage_format);
                    if crate::ui::icons::choice(ui, !self.study_storage_zip, tr.settings_format_unzip)
                        .clicked()
                    {
                        self.study_storage_zip = false;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_study_storage_zip(false);
                        }
                    }
                    if crate::ui::icons::choice(ui, self.study_storage_zip, tr.settings_format_zip)
                        .clicked()
                    {
                        self.study_storage_zip = true;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_study_storage_zip(true);
                        }
                    }
                });

                ui.separator();
                let es = self.lang == crate::i18n::Lang::Es;
                let auto_zip_lbl = if es {
                    "Renombrar automáticamente archivos .zip al abrirlos (Paciente - ID - Estudio - Fecha.zip)"
                } else {
                    "Auto-rename opened .zip files (Patient - ID - Study - Date.zip)"
                };
                if ui.checkbox(&mut self.auto_rename_zips, auto_zip_lbl).changed()
                    && let Some(ref db) = self.db
                {
                    crate::settings::AppSettings::new(db)
                        .set_auto_rename_zips(self.auto_rename_zips);
                }
                ui.add_space(4.0);
                ui.separator();
                ui.label(
                    egui::RichText::new(if es {
                        "Preajustes personalizados de Ventana / Nivel"
                    } else {
                        "Custom Window / Level Presets"
                    })
                    .strong(),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_preset_name)
                            .desired_width(110.0)
                            .hint_text(if es { "Nombre" } else { "Name" }),
                    );
                    ui.label("WL:");
                    ui.add(egui::DragValue::new(&mut self.new_preset_wl).speed(1.0));
                    ui.label("WW:");
                    ui.add(egui::DragValue::new(&mut self.new_preset_ww).speed(1.0).range(1.0..=100_000.0));
                    if ui.small_button(if es { "Agregar" } else { "Add" }).clicked() {
                        let name = self.new_preset_name.trim().to_string();
                        if !name.is_empty() && self.new_preset_ww > 0.0 {
                            self.custom_presets.push(crate::settings::CustomPreset {
                                name,
                                level: self.new_preset_wl,
                                width: self.new_preset_ww,
                            });
                            self.new_preset_name.clear();
                            if let Some(ref db) = self.db {
                                crate::settings::AppSettings::new(db)
                                    .set_custom_presets(&self.custom_presets);
                            }
                        }
                    }
                });
                let mut remove_idx: Option<usize> = None;
                for (pi, cp) in self.custom_presets.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("★ {}  (WL {:.0} / WW {:.0})", cp.name, cp.level, cp.width));
                        if ui.small_button("×").clicked() {
                            remove_idx = Some(pi);
                        }
                    });
                }
                if let Some(ri) = remove_idx {
                    self.custom_presets.remove(ri);
                    if let Some(ref db) = self.db {
                        crate::settings::AppSettings::new(db)
                            .set_custom_presets(&self.custom_presets);
                    }
                }

                ui.separator();
                ui.label(
                    egui::RichText::new(if es {
                        "Protocolos de Colgado (diseño por modalidad)"
                    } else {
                        "Hanging Protocols (layout by modality)"
                    })
                    .strong(),
                );
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("hp_mod_combo")
                        .selected_text(&self.new_hp_modality)
                        .width(64.0)
                        .show_ui(ui, |ui| {
                            for m in ["CT", "MR", "PT", "US", "DX", "CR"] {
                                ui.selectable_value(&mut self.new_hp_modality, m.to_string(), m);
                            }
                        });
                    ui.add(egui::DragValue::new(&mut self.new_hp_rows).range(1..=4).prefix("R:"));
                    ui.add(egui::DragValue::new(&mut self.new_hp_cols).range(1..=4).prefix("C:"));
                    ui.checkbox(&mut self.new_hp_auto_mpr, "MPR");
                    if ui.small_button(if es { "Agregar" } else { "Add" }).clicked() {
                        let proto = crate::settings::HangingProtocol {
                            modality: self.new_hp_modality.clone(),
                            rows: self.new_hp_rows,
                            cols: self.new_hp_cols,
                            auto_mpr: self.new_hp_auto_mpr,
                        };
                        self.hanging_protocols.retain(|p| p.modality != proto.modality);
                        self.hanging_protocols.push(proto);
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db)
                                .set_hanging_protocols(&self.hanging_protocols);
                        }
                    }
                });
                let mut remove_hp: Option<usize> = None;
                for (hi, hp) in self.hanging_protocols.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{}: {}×{}{}",
                            hp.modality,
                            hp.rows,
                            hp.cols,
                            if hp.auto_mpr { " · MPR" } else { "" }
                        ));
                        if ui.small_button("×").clicked() {
                            remove_hp = Some(hi);
                        }
                    });
                }
                if let Some(hi) = remove_hp {
                    self.hanging_protocols.remove(hi);
                    if let Some(ref db) = self.db {
                        crate::settings::AppSettings::new(db)
                            .set_hanging_protocols(&self.hanging_protocols);
                    }
                }

                let es_s = self.lang == crate::i18n::Lang::Es;
                ui.collapsing(
                    if es_s {
                        "Plantilla de informe (Key Images)"
                    } else {
                        "Report template (Key Images)"
                    },
                    |ui| {
                        let resp = ui.add(
                            egui::TextEdit::multiline(&mut self.report_template)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text(if es_s {
                                    "ANTECEDENTES:\nTÉCNICA:\nHALLAZGOS:\nIMPRESIÓN:"
                                } else {
                                    "CLINICAL HISTORY:\nTECHNIQUE:\nFINDINGS:\nIMPRESSION:"
                                }),
                        );
                        if resp.changed()
                            && let Some(ref db) = self.db
                        {
                            crate::settings::AppSettings::new(db)
                                .set_report_template(&self.report_template);
                        }
                        if ui
                            .small_button(if es_s {
                                "Insertar plantilla estándar"
                            } else {
                                "Insert standard template"
                            })
                            .clicked()
                        {
                            self.report_template = if es_s {
                                "ANTECEDENTES:\n\nTÉCNICA:\n\nHALLAZGOS:\n\nIMPRESIÓN:\n".to_string()
                            } else {
                                "CLINICAL HISTORY:\n\nTECHNIQUE:\n\nFINDINGS:\n\nIMPRESSION:\n".to_string()
                            };
                            if let Some(ref db) = self.db {
                                crate::settings::AppSettings::new(db)
                                    .set_report_template(&self.report_template);
                            }
                        }
                    },
                );

                ui.collapsing(
                    if es_s {
                        "Atajos de teclado personalizados"
                    } else {
                        "Custom keyboard shortcuts"
                    },
                    |ui| {
                        let mut changed = false;
                        for (id, lbl_en, lbl_es, def_key) in [
                            ("wl", "Window / Level", "Ventana / Nivel", "W"),
                            ("scroll", "Scroll", "Desplazar", "Z"),
                            ("pan", "Pan", "Mover", "X"),
                            ("zoom", "Zoom", "Zoom", "Q"),
                            ("length", "Length ROI", "Medir longitud", "L"),
                            ("oval", "Oval ROI", "ROI ovalada", "E"),
                            ("reset", "Reset View", "Restablecer vista", "R"),
                            ("key_image", "Toggle Key Image", "Marcar imagen clave", "K"),
                            ("pacs", "PACS / DICOMweb", "PACS / DICOMweb", "N"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(if es_s { lbl_es } else { lbl_en });
                                let mut val = self
                                    .custom_shortcuts
                                    .get(id)
                                    .cloned()
                                    .unwrap_or_default();
                                let edit = ui.add(
                                    egui::TextEdit::singleline(&mut val)
                                        .desired_width(48.0)
                                        .hint_text(def_key),
                                );
                                if edit.changed() {
                                    let trimmed = val.trim().to_ascii_uppercase();
                                    if trimmed.is_empty() {
                                        self.custom_shortcuts.remove(id);
                                    } else {
                                        self.custom_shortcuts.insert(id.to_string(), trimmed);
                                    }
                                    changed = true;
                                }
                            });
                        }
                        if ui
                            .small_button(if es_s {
                                "Restablecer atajos por defecto"
                            } else {
                                "Reset default shortcuts"
                            })
                            .clicked()
                        {
                            self.custom_shortcuts.clear();
                            changed = true;
                        }
                        if changed
                            && let Some(ref db) = self.db
                        {
                            crate::settings::AppSettings::new(db)
                                .set_custom_shortcuts(&self.custom_shortcuts);
                        }
                    },
                );

                let (upd_hdr, lbl_auto, lbl_manual, lbl_check_now) = match self.lang {
                    crate::i18n::Lang::Es => (
                        "Actualizaciones",
                        "Buscar al iniciar",
                        "Solo manual",
                        "Buscar ahora",
                    ),
                    crate::i18n::Lang::En => (
                        "Updates",
                        "Check on startup",
                        "Manual only",
                        "Check now",
                    ),
                };
                crate::ui::icons::section_title(ui, upd_hdr);
                ui.horizontal(|ui| {
                    if crate::ui::icons::choice(ui, self.check_updates_on_startup, lbl_auto)
                        .clicked()
                    {
                        self.check_updates_on_startup = true;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_check_updates_on_startup(true);
                        }
                    }
                    if crate::ui::icons::choice(ui, !self.check_updates_on_startup, lbl_manual)
                        .clicked()
                    {
                        self.check_updates_on_startup = false;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_check_updates_on_startup(false);
                        }
                    }
                    ui.separator();
                    let checking_or_installing = matches!(
                        self.update_status,
                        crate::updater::UpdateStatus::Checking
                            | crate::updater::UpdateStatus::Installing
                    );
                    if ui
                        .add_enabled(!checking_or_installing, egui::Button::new(lbl_check_now))
                        .clicked()
                    {
                        self.show_update_modal = true;
                        self.start_update_check(true, ctx);
                    }
                });
                match &self.update_status {
                    crate::updater::UpdateStatus::Available(info) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                crate::ui::theme::ok_color(ui),
                                match self.lang {
                                    crate::i18n::Lang::Es => format!("{} disponible", info.tag_name),
                                    crate::i18n::Lang::En => format!("{} available", info.tag_name),
                                },
                            );
                            if ui
                                .small_button(match self.lang {
                                    crate::i18n::Lang::Es => "Instalar…",
                                    crate::i18n::Lang::En => "Install…",
                                })
                                .clicked()
                            {
                                self.show_update_modal = true;
                            }
                        });
                    }
                    crate::updater::UpdateStatus::Installed(tag) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                crate::ui::theme::ok_color(ui),
                                match self.lang {
                                    crate::i18n::Lang::Es => format!("{tag} instalada"),
                                    crate::i18n::Lang::En => format!("{tag} installed"),
                                },
                            );
                            if ui
                                .small_button(match self.lang {
                                    crate::i18n::Lang::Es => "Reiniciar…",
                                    crate::i18n::Lang::En => "Restart…",
                                })
                                .clicked()
                            {
                                self.show_update_modal = true;
                            }
                        });
                    }
                    crate::updater::UpdateStatus::UpToDate(tag) => {
                        ui.small(match self.lang {
                            crate::i18n::Lang::Es => format!("Al día: v{} ({tag})", env!("CARGO_PKG_VERSION")),
                            crate::i18n::Lang::En => format!("Up to date: v{} ({tag})", env!("CARGO_PKG_VERSION")),
                        });
                    }
                    crate::updater::UpdateStatus::Checking => {
                        ui.small(match self.lang {
                            crate::i18n::Lang::Es => "Buscando actualizaciones en GitHub…",
                            crate::i18n::Lang::En => "Checking GitHub for updates…",
                        });
                    }
                    crate::updater::UpdateStatus::Installing => {
                        ui.small(match self.lang {
                            crate::i18n::Lang::Es => "Descargando e instalando la actualización…",
                            crate::i18n::Lang::En => "Downloading and installing update…",
                        });
                    }
                    crate::updater::UpdateStatus::Error(err) => {
                        ui.colored_label(crate::ui::theme::error_color(ui), err);
                    }
                    crate::updater::UpdateStatus::Idle => {
                        ui.small(format!("v{}", env!("CARGO_PKG_VERSION")));
                    }
                }
                ui.add_space(4.0);

                let crash_opt_lbl = match self.lang {
                    crate::i18n::Lang::Es => "Permitir reportes de fallos (revisados antes de enviar)",
                    crate::i18n::Lang::En => "Enable crash reporting (user-reviewed before submit)",
                };
                if ui
                    .checkbox(&mut self.crash_report_opt_in, crash_opt_lbl)
                    .changed()
                    && let Some(ref db) = self.db
                {
                    crate::settings::AppSettings::new(db)
                        .set_crash_report_opt_in(self.crash_report_opt_in);
                }

                let data_lbl = match self.lang {
                    crate::i18n::Lang::Es => "Datos locales (base de datos y registros)",
                    crate::i18n::Lang::En => "Local data (database and logs)",
                };
                crate::ui::icons::section_title(ui, data_lbl);
                let data_dir = crate::crash_log::app_data_dir();
                crate::ui::icons::path_box(ui, &data_dir.display().to_string());
                ui.horizontal(|ui| {
                    let open_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Abrir carpeta",
                        crate::i18n::Lang::En => "Open folder",
                    };
                    if ui.button(open_lbl).clicked() {
                        open_in_file_manager(&data_dir);
                    }
                    let del_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Eliminar todos los datos locales…",
                        crate::i18n::Lang::En => "Delete all local data…",
                    };
                    if ui.button(del_lbl).clicked() {
                        self.confirm_delete_local_data = true;
                    }
                    let notice_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Aviso de seguridad y privacidad…",
                        crate::i18n::Lang::En => "Safety & privacy notice…",
                    };
                    if ui.button(notice_lbl).clicked() {
                        self.show_first_run_notice = true;
                    }
                });
                if self.confirm_delete_local_data {
                    ui.add_space(4.0);
                    let confirm_msg = match self.lang {
                        crate::i18n::Lang::Es => "¿Eliminar la base de datos, los estudios administrados y los registros?",
                        crate::i18n::Lang::En => "Delete the database, managed studies and logs?",
                    };
                    ui.colored_label(crate::ui::theme::error_color(ui), confirm_msg);
                    ui.horizontal(|ui| {
                        let yes_lbl = match self.lang {
                            crate::i18n::Lang::Es => "Confirmar eliminación",
                            crate::i18n::Lang::En => "Confirm delete",
                        };
                        if ui.button(yes_lbl).clicked() {
                            self.confirm_delete_local_data = false;
                            let _ = self.db.take();
                            self.index = None;
                            if data_dir.exists() {
                                let _ = trash::delete(&data_dir);
                            }
                            let _ = std::fs::create_dir_all(&data_dir);
                            self.db = crate::db::Database::open_default().ok();
                        }
                        let cancel_lbl = match self.lang {
                            crate::i18n::Lang::Es => "Cancelar",
                            crate::i18n::Lang::En => "Cancel",
                        };
                        if ui.button(cancel_lbl).clicked() {
                            self.confirm_delete_local_data = false;
                        }
                    });
                }
                // A faint "⋯" in the corner, for whoever is looking for it: opens the
                // folder with the settings, library database and log.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = match self.lang {
                        crate::i18n::Lang::Es => "Abrir la carpeta de datos del programa (ajustes, biblioteca y registro)",
                        crate::i18n::Lang::En => "Open the program's data folder (settings, library and log)",
                    };
                    let dots = egui::RichText::new("⋯")
                        .size(10.0)
                        .color(crate::ui::theme::dim_text(ui).gamma_multiply(0.6));
                    if ui
                        .add(egui::Label::new(dots).sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(tip)
                        .clicked()
                    {
                        open_in_file_manager(&data_dir);
                    }
                });
            });
        if !open {
            self.show_settings_modal = false;
            self.confirm_delete_local_data = false;
        }
    }

    /// Settings → accent colour swatches, custom colour and optional toolbar tint.
    fn draw_accent_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::ui::theme;
        let es = self.lang == crate::i18n::Lang::Es;
        let current = theme::accent();
        let mut chosen: Option<Color32> = None;
        let mut tint = theme::tint_chrome();
        ui.horizontal(|ui| {
            ui.label(if es { "Color de acento:" } else { "Accent colour:" });
            for (en, es_name, color) in theme::ACCENT_PRESETS {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                let selected = current == color;
                ui.painter().circle_filled(rect.center(), 8.0, color);
                if selected {
                    ui.painter().circle_stroke(
                        rect.center(),
                        10.0,
                        egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                    );
                } else if resp.hovered() {
                    ui.painter().circle_stroke(
                        rect.center(),
                        10.0,
                        egui::Stroke::new(1.0, ui.visuals().text_color()),
                    );
                }
                if resp.on_hover_text(if es { es_name } else { en }).clicked() {
                    chosen = Some(color);
                }
            }
            ui.add_space(6.0);
            let mut custom = current;
            if ui
                .color_edit_button_srgba(&mut custom)
                .on_hover_text(if es { "Color personalizado" } else { "Custom colour" })
                .changed()
            {
                chosen = Some(Color32::from_rgb(custom.r(), custom.g(), custom.b()));
            }
            if current != theme::DEFAULT_ACCENT
                && ui
                    .small_button(if es { "Restablecer" } else { "Reset" })
                    .clicked()
            {
                chosen = Some(theme::DEFAULT_ACCENT);
            }
        });
        let tint_changed = ui
            .checkbox(
                &mut tint,
                if es {
                    "Teñir la barra de herramientas y la barra de estado con el color de acento"
                } else {
                    "Tint the toolbar and status bar with the accent colour"
                },
            )
            .changed();
        if chosen.is_some() || tint_changed {
            let color = chosen.unwrap_or(current);
            theme::set_accent(ctx, color, tint);
            if let Some(ref db) = self.db {
                let _ = db.set_setting("accent_color", &theme::to_hex(color));
                let _ = db.set_setting("tint_chrome", if tint { "1" } else { "0" });
            }
        }
    }

    fn select_studies_folder(&mut self, dir: std::path::PathBuf) {
        if let Some(ref db) = self.db {
            crate::settings::AppSettings::new(db).set_custom_studies_dir(Some(&dir));
        }
        crate::db::set_custom_studies_dir(Some(dir.clone()));
        if dir.is_dir()
            && std::fs::read_dir(&dir)
                .ok()
                .and_then(|mut it| it.next())
                .is_some()
        {
            self.start_scan(vec![dir]);
        }
    }

    pub(super) fn draw_first_run_notice_modal(&mut self, ctx: &egui::Context) {
        if !self.show_first_run_notice {
            return;
        }
        let title = match self.lang {
            crate::i18n::Lang::Es => "Aviso importante — SekhmetX",
            crate::i18n::Lang::En => "Important Notice — SekhmetX",
        };
        let data_dir = crate::crash_log::app_data_dir();
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let lang_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Idioma / Language:",
                        crate::i18n::Lang::En => "Language / Idioma:",
                    };
                    ui.strong(lang_lbl);
                    if crate::ui::icons::choice(ui, self.lang == crate::i18n::Lang::En, "English")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::En;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_language(crate::i18n::Lang::En);
                        }
                    }
                    if crate::ui::icons::choice(ui, self.lang == crate::i18n::Lang::Es, "Español")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::Es;
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db).set_language(crate::i18n::Lang::Es);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let (theme_lbl, lbl_dark, lbl_light, lbl_sys) = match self.lang {
                        crate::i18n::Lang::Es => ("Tema:", "Oscuro", "Claro", "Sistema"),
                        crate::i18n::Lang::En => ("Theme:", "Dark", "Light", "System"),
                    };
                    ui.label(theme_lbl);
                    for (mode, lbl) in [
                        (super::ThemeMode::Dark, lbl_dark),
                        (super::ThemeMode::Oled, "OLED"),
                        (super::ThemeMode::Light, lbl_light),
                        (super::ThemeMode::System, lbl_sys),
                    ] {
                        if crate::ui::icons::choice(ui, self.theme_mode == mode, lbl).clicked() {
                            self.theme_mode = mode;
                            mode.apply(ctx);
                            if let Some(ref db) = self.db {
                                let _ = db.set_setting("theme_mode", mode.as_str());
                            }
                        }
                    }
                });
                ui.separator();
                let bullets: &[&str] = match self.lang {
                    crate::i18n::Lang::Es => &[
                        "• SekhmetX NO es un dispositivo médico certificado ni sustituye la evaluación diagnóstica clínica.",
                        "• Los reportes de fallos enviados a GitHub son públicos (revise la vista previa antes de enviar).",
                    ],
                    crate::i18n::Lang::En => &[
                        "• SekhmetX is NOT a certified medical device and is not intended for primary diagnostic use.",
                        "• Crash reports submitted to GitHub are public (review the scrubbed preview before submitting).",
                    ],
                };
                for b in bullets {
                    ui.label(*b);
                    ui.add_space(4.0);
                }
                let loc_lbl = match self.lang {
                    crate::i18n::Lang::Es => "• Los datos locales de pacientes se guardan en:",
                    crate::i18n::Lang::En => "• Local patient data is stored at:",
                };
                ui.label(loc_lbl);
                crate::ui::icons::path_box(ui, &data_dir.display().to_string());
                ui.add_space(6.0);
                let studies_lbl = match self.lang {
                    crate::i18n::Lang::Es => "• Carpeta de la biblioteca de estudios (donde se guardan sus estudios):",
                    crate::i18n::Lang::En => "• Studies library folder (where your studies are kept):",
                };
                ui.label(studies_lbl);
                let tr = crate::i18n::tr(self.lang);
                ui.horizontal(|ui| {
                    let cur_studies = crate::db::managed_studies_dir();
                    crate::ui::icons::path_box(ui, &cur_studies.display().to_string());
                    if ui.button(tr.choose_folder).clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.select_studies_folder(dir);
                    }
                });
                ui.add_space(8.0);
                let ack_lbl = match self.lang {
                    crate::i18n::Lang::Es => "Entendido",
                    crate::i18n::Lang::En => "I understand",
                };
                if ui.button(ack_lbl).clicked() {
                    if let Some(ref db) = self.db {
                        crate::settings::AppSettings::new(db).set_first_run_notice_ack(true);
                    }
                    self.show_first_run_notice = false;
                }
            });
    }

    /// Stage 4: Interactive Histogram & Draggable 1D Transfer-Function Curve Editor for 3D VR.
    pub(super) fn draw_vr_tf_editor(&mut self, ctx: &egui::Context) {
        if self.show_database_view
            || !self.show_vr_tf_editor
            || !self.active_cell().vr_enabled
            || self.active_cell().vr_mip
        {
            return;
        }
        let vp_idx = self.active_viewport;
        let Some(vol) = self.viewport_volume(vp_idx).cloned() else {
            return;
        };
        let fallback_win = Window::new(300.0, 1200.0);
        let win = self.viewports[vp_idx]
            .window
            .or_else(|| self.viewport_frame(vp_idx).map(|f| self.effective_window(vp_idx, f)))
            .unwrap_or(fallback_win);
        let low = win.low();
        let high = win.high();
        let span = (high - low).max(1.0);

        // Compute/reuse 128-bin log-scaled HU histogram for [low, high]
        const BINS: usize = 128;
        let cache_hit = self.vr_histogram_cache.as_ref().is_some_and(|(id, lb, hb, _)| {
            *id == vol.gpu_id && *lb == low.to_bits() && *hb == high.to_bits()
        });
        if !cache_hit {
            let mut counts = vec![0u64; BINS];
            let step = (vol.voxels.len() / 450_000).max(1);
            for &q in vol.voxels.iter().step_by(step) {
                let hu = q as f32 * vol.voxel_scale + vol.voxel_offset;
                let t = (hu - low) / span;
                if (0.0..=1.0).contains(&t) {
                    let b = ((t * (BINS - 1) as f32).round() as usize).min(BINS - 1);
                    counts[b] += 1;
                }
            }
            let max_c = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
            let log_max = (1.0 + max_c).ln().max(1e-5);
            let norm_bins: Vec<f32> = counts
                .into_iter()
                .map(|c| ((1.0 + c as f32).ln() / log_max).clamp(0.0, 1.0))
                .collect();
            self.vr_histogram_cache = Some((vol.gpu_id, low.to_bits(), high.to_bits(), norm_bins));
        }

        let mut open = self.show_vr_tf_editor;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Función de transferencia 3D (LUT 1D)",
            crate::i18n::Lang::En => "3D transfer function editor (1D LUT)",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(true)
            .resizable(false)
            .default_width(384.0)
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-14.0, -34.0))
            .show(ctx, |ui| {
                // Quick preset strip
                ui.horizontal_wrapped(|ui| {
                    for preset in crate::imaging::vr_presets::VrPreset::ALL {
                        let sel = self.viewports[vp_idx].vr_preset == preset;
                        if ui.selectable_label(sel, preset.label()).clicked() {
                            self.viewports[vp_idx].vr_preset = preset;
                            self.viewports[vp_idx].window = Some(preset.default_window());
                            self.viewports[vp_idx].vr_custom_stops = preset.default_stops();
                            self.vr_tf_selected_stop = 0;
                        }
                    }
                });
                ui.add_space(4.0);

                let canvas_size = vec2(360.0, 140.0);
                let (rect, resp) = ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, Color32::from_rgb(14, 17, 22));

                // Subtle grid
                for frac in [0.25f32, 0.5, 0.75] {
                    let y = rect.bottom() - frac * rect.height();
                    painter.line_segment(
                        [pos2(rect.left(), y), pos2(rect.right(), y)],
                        Stroke::new(1.0, Color32::from_gray(32)),
                    );
                    let x = rect.left() + frac * rect.width();
                    painter.line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                        Stroke::new(1.0, Color32::from_gray(32)),
                    );
                }

                // Draw cached log-scaled HU histogram
                if let Some((_, _, _, ref bins)) = self.vr_histogram_cache {
                    let bw = rect.width() / BINS as f32;
                    for (i, &h) in bins.iter().enumerate() {
                        if h <= 0.005 {
                            continue;
                        }
                        let x0 = rect.left() + i as f32 * bw;
                        let bar_h = h * rect.height() * 0.92;
                        let bar_rect = Rect::from_min_max(
                            pos2(x0, rect.bottom() - bar_h),
                            pos2(x0 + bw.max(1.0), rect.bottom()),
                        );
                        painter.rect_filled(
                            bar_rect,
                            0.0,
                            Color32::from_rgba_unmultiplied(70, 155, 220, 80),
                        );
                    }
                }

                let clut = self.active_clut_table(vp_idx);
                if self.viewports[vp_idx].vr_custom_stops.len() < 2 {
                    self.viewports[vp_idx].vr_custom_stops =
                        self.viewports[vp_idx].vr_preset.default_stops();
                }
                let stops = &mut self.viewports[vp_idx].vr_custom_stops;
                if self.vr_tf_selected_stop >= stops.len() {
                    self.vr_tf_selected_stop = stops.len() - 1;
                }

                let to_screen = |t: f32, a: f32| -> Pos2 {
                    pos2(
                        rect.left() + t.clamp(0.0, 1.0) * rect.width(),
                        rect.bottom() - a.clamp(0.0, 1.0) * rect.height(),
                    )
                };
                let from_screen = |p: Pos2| -> (f32, f32) {
                    let t = ((p.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
                    let a = ((rect.bottom() - p.y) / rect.height().max(1.0)).clamp(0.0, 1.0);
                    (t, a)
                };

                // Pointer interaction: select/drag stop, double-click to add, right-click to remove
                if resp.drag_started()
                    && let Some(pos) = resp.interact_pointer_pos()
                    && let Some((idx, _)) = stops
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i, to_screen(s.t, s.a).distance(pos)))
                        .filter(|&(_, d)| d <= 14.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    self.vr_tf_selected_stop = idx;
                }
                if resp.dragged_by(egui::PointerButton::Primary)
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let idx = self.vr_tf_selected_stop;
                    let (t_raw, a_raw) = from_screen(pos);
                    let len = stops.len();
                    if idx == 0 {
                        stops[idx].t = 0.0;
                    } else if idx + 1 == len {
                        stops[idx].t = 1.0;
                    } else {
                        let min_t = stops[idx - 1].t + 0.01;
                        let max_t = stops[idx + 1].t - 0.01;
                        stops[idx].t = t_raw.clamp(min_t, max_t.max(min_t));
                    }
                    stops[idx].a = a_raw;
                } else if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                    && let Some((idx, _)) = stops
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i, to_screen(s.t, s.a).distance(pos)))
                        .filter(|&(_, d)| d <= 12.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    self.vr_tf_selected_stop = idx;
                }
                if resp.double_clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let (t_new, a_new) = from_screen(pos);
                    let ins = stops.partition_point(|s| s.t < t_new).clamp(1, stops.len() - 1);
                    let s0 = stops[ins - 1];
                    let s1 = stops[ins];
                    let f = ((t_new - s0.t) / (s1.t - s0.t).max(1e-5)).clamp(0.0, 1.0);
                    stops.insert(
                        ins,
                        crate::imaging::vr_presets::TfStop {
                            t: t_new,
                            r: s0.r + (s1.r - s0.r) * f,
                            g: s0.g + (s1.g - s0.g) * f,
                            b: s0.b + (s1.b - s0.b) * f,
                            a: a_new,
                        },
                    );
                    self.vr_tf_selected_stop = ins;
                }
                if resp.secondary_clicked()
                    && stops.len() > 2
                    && let Some(pos) = resp.interact_pointer_pos()
                    && let Some((idx, _)) = stops
                        .iter()
                        .enumerate()
                        .filter(|&(i, _)| i > 0 && i + 1 < stops.len())
                        .map(|(i, s)| (i, to_screen(s.t, s.a).distance(pos)))
                        .filter(|&(_, d)| d <= 12.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                {
                    stops.remove(idx);
                    self.vr_tf_selected_stop = self.vr_tf_selected_stop.min(stops.len() - 1);
                }

                // Draw opacity curve polyline
                let pts: Vec<Pos2> = stops.iter().map(|s| to_screen(s.t, s.a)).collect();
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(2.0, Color32::from_gray(235)));
                }

                // Draw control points
                for (i, s) in stops.iter().enumerate() {
                    let c = to_screen(s.t, s.a);
                    let fill = Color32::from_rgb(
                        (s.r * 255.0).round() as u8,
                        (s.g * 255.0).round() as u8,
                        (s.b * 255.0).round() as u8,
                    );
                    let selected = i == self.vr_tf_selected_stop;
                    painter.circle_filled(c, if selected { 6.5 } else { 5.0 }, fill);
                    painter.circle_stroke(
                        c,
                        if selected { 6.5 } else { 5.0 },
                        Stroke::new(
                            if selected { 2.0 } else { 1.2 },
                            if selected {
                                Color32::from_rgb(255, 205, 75)
                            } else {
                                Color32::WHITE
                            },
                        ),
                    );
                }
                painter.rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(1.0, Color32::from_gray(65)),
                    egui::StrokeKind::Inside,
                );

                // Color gradient ramp bar directly under the curve
                ui.add_space(3.0);
                let (ramp_rect, _) =
                    ui.allocate_exact_size(vec2(360.0, 14.0), egui::Sense::hover());
                let ramp_painter = ui.painter_at(ramp_rect);
                let lut = crate::imaging::vr_presets::build_lut_from_stops(stops, clut.as_ref());
                let seg_w = ramp_rect.width() / 64.0;
                for i in 0..64 {
                    let idx = (i * 4).min(255) * 4;
                    let col = Color32::from_rgb(lut[idx], lut[idx + 1], lut[idx + 2]);
                    let r = Rect::from_min_max(
                        pos2(ramp_rect.left() + i as f32 * seg_w, ramp_rect.top()),
                        pos2(ramp_rect.left() + (i + 1) as f32 * seg_w + 0.5, ramp_rect.bottom()),
                    );
                    ramp_painter.rect_filled(r, 0.0, col);
                }
                ramp_painter.rect_stroke(
                    ramp_rect,
                    2.0,
                    Stroke::new(1.0, Color32::from_gray(65)),
                    egui::StrokeKind::Inside,
                );

                // Selected stop inspector
                ui.add_space(4.0);
                let sel_idx = self.vr_tf_selected_stop.min(stops.len() - 1);
                let hu = low + stops[sel_idx].t * span;
                let alpha_pct = stops[sel_idx].a * 100.0;
                let mut rgb = [stops[sel_idx].r, stops[sel_idx].g, stops[sel_idx].b];
                ui.horizontal(|ui| {
                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                        stops[sel_idx].r = rgb[0];
                        stops[sel_idx].g = rgb[1];
                        stops[sel_idx].b = rgb[2];
                    }
                    ui.monospace(format!("HU: {hu:.0}  α: {alpha_pct:.0}%"));
                    if sel_idx > 0
                        && sel_idx + 1 < stops.len()
                        && crate::ui::icons::small_tool_button(ui, crate::ui::icons::Icon::Trash, false)
                            .on_hover_text(if self.lang == crate::i18n::Lang::Es { "Quitar punto" } else { "Remove stop" })
                            .clicked()
                    {
                        stops.remove(sel_idx);
                        self.vr_tf_selected_stop = sel_idx.saturating_sub(1);
                    }
                });
                let hint = match self.lang {
                    crate::i18n::Lang::Es => {
                        "Arrastrar punto · Doble clic para agregar · Clic derecho para quitar"
                    }
                    crate::i18n::Lang::En => {
                        "Drag point · Double-click to add · Right-click to remove"
                    }
                };
                ui.small(egui::RichText::new(hint).color(crate::ui::theme::dim_text(ui)));
            });
        self.show_vr_tf_editor = open;
    }

    pub(super) fn draw_calibrate_modal(&mut self, ctx: &egui::Context) {
        if !self.show_calibrate_modal {
            return;
        }
        let mut open = self.show_calibrate_modal;
        let mut should_close = false;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Calibrar espaciado de píxeles",
            crate::i18n::Lang::En => "Calibrate Pixel Spacing",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let measured_lbl = match self.lang {
                    crate::i18n::Lang::Es => {
                        format!("Longitud medida: {:.1} px", self.calibrate_last_len_px)
                    }
                    crate::i18n::Lang::En => {
                        format!("Measured length: {:.1} px", self.calibrate_last_len_px)
                    }
                };
                ui.label(measured_lbl);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let known_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Longitud conocida (mm):",
                        crate::i18n::Lang::En => "Known length (mm):",
                    };
                    ui.label(known_lbl);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.calibrate_known_mm).desired_width(90.0),
                    );
                });
                let parsed_mm = self
                    .calibrate_known_mm
                    .trim()
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
                    .filter(|&v| v > 0.0 && v.is_finite());
                if let Some(mm) = parsed_mm
                    && self.calibrate_last_len_px > 0.0
                {
                    let sp = mm / self.calibrate_last_len_px;
                    ui.small(
                        egui::RichText::new(format!("-> {:.4} mm/px", sp))
                            .color(crate::ui::theme::dim_text(ui)),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let apply_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Aplicar",
                        crate::i18n::Lang::En => "Apply",
                    };
                    let cancel_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Cancelar",
                        crate::i18n::Lang::En => "Cancel",
                    };
                    let valid = parsed_mm.is_some() && self.calibrate_last_len_px > 0.0;
                    if ui
                        .add_enabled(valid, egui::Button::new(apply_lbl))
                        .clicked()
                        && let Some(mm) = parsed_mm
                    {
                        let sp = mm / self.calibrate_last_len_px;
                        if let Some(sel) = self.active_cell().selected {
                            self.series_spacing_override.insert(sel, (sp, sp));
                        }
                        should_close = true;
                    }
                    if ui.button(cancel_lbl).clicked() {
                        should_close = true;
                    }
                });
            });
        self.show_calibrate_modal = open && !should_close;
    }

    pub(super) fn draw_arrow_edit_modal(&mut self, ctx: &egui::Context) {
        let Some((sp, sfr, idx)) = self.arrow_edit_target.clone() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Texto de flecha",
            crate::i18n::Lang::En => "Arrow Annotation Text",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.arrow_edit_text).desired_width(220.0),
                );
                resp.request_focus();
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let save_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Guardar",
                        crate::i18n::Lang::En => "Save",
                    };
                    let cancel_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Cancelar",
                        crate::i18n::Lang::En => "Cancel",
                    };
                    if ui.button(save_lbl).clicked() {
                        save = true;
                    }
                    if ui.button(cancel_lbl).clicked() {
                        cancel = true;
                    }
                });
            });
        if save {
            let k = (sp, sfr);
            if let Some(list) = self.measures.get_mut(&k)
                && let Some(crate::app::Measure::Arrow { text, .. }) = list.get_mut(idx)
            {
                let trimmed = self.arrow_edit_text.trim();
                *text = if trimmed.is_empty() {
                    "Note".to_string()
                } else {
                    trimmed.to_string()
                };
            }
            self.persist_measures_for(&k);
            self.arrow_edit_target = None;
        } else if !open || cancel {
            self.arrow_edit_target = None;
        }
    }

    pub(super) fn draw_roi_histogram_modal(&mut self, ctx: &egui::Context) {
        if !self.show_roi_histogram || self.show_database_view {
            return;
        }
        let vp_idx = self.active_viewport;
        let mut open = self.show_roi_histogram;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Histograma / Perfil ROI",
            crate::i18n::Lang::En => "ROI Histogram / Profile",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_size(egui::vec2(300.0, 170.0))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -34.0))
            .show(ctx, |ui| {
                let Some(k) = self.viewport_measure_key(vp_idx) else {
                    ui.small(if self.lang == crate::i18n::Lang::Es { "No hay imagen activa." } else { "No active slice." });
                    return;
                };
                let Some(frame) = self.viewport_frame(vp_idx) else {
                    ui.small(if self.lang == crate::i18n::Lang::Es { "Imagen aún no cargada." } else { "Frame not loaded." });
                    return;
                };
                let list = self.measures.get(&k);
                let target = list.and_then(|v| {
                    if let Some((ref sp, sfr, idx)) = self.selected_roi
                        && *sp == k.0
                        && sfr == k.1
                        && let Some(m) = v.get(idx)
                    {
                        return Some(m);
                    }
                    v.iter().rev().find(|m| {
                        matches!(
                            m,
                            crate::app::Measure::Length(_, _)
                                | crate::app::Measure::Oval(_, _)
                                | crate::app::Measure::Rect(_, _)
                                | crate::app::Measure::Polygon(_)
                                | crate::app::Measure::Pencil(_)
                        )
                    })
                });
                let Some(m) = target else {
                    let hint = match self.lang {
                        crate::i18n::Lang::Es => "Selecciona o dibuja un ROI de longitud o área.",
                        crate::i18n::Lang::En => "Select or draw a Length or Area ROI.",
                    };
                    ui.small(hint);
                    return;
                };

                let unit = frame.value_unit;
                match m {
                    crate::app::Measure::Length(a, b) => {
                        let samples = crate::roi::sample_line_profile(
                            frame,
                            crate::roi::RoiPoint::new(a.x as f64, a.y as f64),
                            crate::roi::RoiPoint::new(b.x as f64, b.y as f64),
                            96,
                        );
                        if samples.len() < 2 {
                            ui.small(if self.lang == crate::i18n::Lang::Es { "Sin muestras." } else { "No samples." });
                            return;
                        }
                        let (min_v, max_v, mean_v) = summarize_samples(&samples);
                        ui.small(format!(
                            "Line Profile  Min: {min_v:.1}  Mean: {mean_v:.1}  Max: {max_v:.1} {unit}"
                        ));
                        let desired = egui::vec2(ui.available_width().max(180.0), 92.0);
                        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
                        let p = ui.painter_at(rect);
                        p.rect_filled(rect, 4.0, Color32::from_gray(22));
                        let span = (max_v - min_v).max(1e-3);
                        let pts: Vec<Pos2> = samples
                            .iter()
                            .enumerate()
                            .map(|(i, &v)| {
                                let tx = i as f32 / (samples.len() - 1) as f32;
                                let ty = (v - min_v) / span;
                                egui::pos2(
                                    rect.left() + 4.0 + tx * (rect.width() - 8.0),
                                    rect.bottom() - 4.0 - ty * (rect.height() - 8.0),
                                )
                            })
                            .collect();
                        p.add(egui::Shape::line(
                            pts,
                            Stroke::new(1.5, Color32::from_rgb(80, 220, 140)),
                        ));
                    }
                    crate::app::Measure::Oval(a, b)
                    | crate::app::Measure::Rect(a, b) => {
                        let is_oval = matches!(m, crate::app::Measure::Oval(_, _));
                        let r = crate::roi::RoiBoundingBox::rect(
                            crate::roi::RoiPoint::new(a.x as f64, a.y as f64),
                            crate::roi::RoiPoint::new(b.x as f64, b.y as f64),
                            is_oval,
                        );
                        let outline = if is_oval {
                            crate::roi::RoiPlanarGeometry::outline_of_oval(r, 64)
                        } else {
                            crate::roi::RoiPlanarGeometry::outline_of_rectangle(r).to_vec()
                        };
                        draw_histogram_bars(ui, frame, &outline, unit, self.lang);
                    }
                    crate::app::Measure::Polygon(pts) => {
                        let outline: Vec<crate::roi::RoiPoint> = pts
                            .iter()
                            .map(|p| crate::roi::RoiPoint::new(p.x as f64, p.y as f64))
                            .collect();
                        draw_histogram_bars(ui, frame, &outline, unit, self.lang);
                    }
                    crate::app::Measure::Pencil(pts) => {
                        let raw: Vec<crate::roi::RoiPoint> = pts
                            .iter()
                            .map(|p| crate::roi::RoiPoint::new(p.x as f64, p.y as f64))
                            .collect();
                        let (smoothed, _) = crate::roi::RoiSpline::smooth(&raw, 1.5);
                        let outline = if smoothed.len() >= 3 { &smoothed } else { &raw };
                        draw_histogram_bars(ui, frame, outline, unit, self.lang);
                    }
                    _ => {
                        ui.small(if self.lang == crate::i18n::Lang::Es { "Histograma disponible para ROI de longitud o de área." } else { "Histogram available for Length / Area ROIs." });
                    }
                }
            });
        self.show_roi_histogram = open;
    }

    pub(super) fn draw_remove_study_modal(&mut self, ctx: &egui::Context) {
        let Some((uid, label)) = self.pending_remove_study.clone() else {
            return;
        };
        let file_count = self.study_file_count(&uid);
        let mut open = true;
        let mut action: Option<bool> = None; // Some(false) = library only, Some(true) = also delete files
        let mut cancel = false;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Eliminar estudio",
            crate::i18n::Lang::En => "Remove Study",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&label).strong());
                ui.add_space(4.0);
                let msg = match self.lang {
                    crate::i18n::Lang::Es => format!(
                        "¿Deseas eliminar este estudio solo de la biblioteca o también enviar sus {file_count} archivo(s) a la papelera?"
                    ),
                    crate::i18n::Lang::En => format!(
                        "Do you want to remove this study from the library only, or also move its {file_count} file(s) to the Recycle Bin?"
                    ),
                };
                ui.label(msg);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let lib_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Solo de la biblioteca",
                        crate::i18n::Lang::En => "Library Only",
                    };
                    let disk_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Enviar a la papelera",
                        crate::i18n::Lang::En => "Move to Recycle Bin",
                    };
                    let cancel_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Cancelar",
                        crate::i18n::Lang::En => "Cancel",
                    };
                    if ui.button(lib_lbl).clicked() {
                        action = Some(false);
                    }
                    if ui
                        .button(
                            egui::RichText::new(disk_lbl)
                                .color(crate::ui::theme::error_color(ui)),
                        )
                        .clicked()
                    {
                        action = Some(true);
                    }
                    if ui.button(cancel_lbl).clicked() {
                        cancel = true;
                    }
                });
            });
        if let Some(delete_files) = action {
            self.pending_remove_study = None;
            self.execute_remove_study(&uid, delete_files);
        } else if !open || cancel {
            self.pending_remove_study = None;
        }
    }

    pub(super) fn draw_clear_library_modal(&mut self, ctx: &egui::Context) {
        if !self.pending_clear_library {
            return;
        }
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let title = match self.lang {
            crate::i18n::Lang::Es => "Limpiar biblioteca",
            crate::i18n::Lang::En => "Clear Library",
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let msg = match self.lang {
                    crate::i18n::Lang::Es => {
                        "¿Limpiar todos los estudios y ROIs de la biblioteca de SekhmetX?\nTus archivos y carpetas en el equipo NO se eliminarán."
                    }
                    crate::i18n::Lang::En => {
                        "Clear all studies and ROIs from the SekhmetX library?\nYour files and folders on disk will NOT be deleted."
                    }
                };
                ui.label(msg);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let yes_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Limpiar biblioteca",
                        crate::i18n::Lang::En => "Clear Library",
                    };
                    let cancel_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Cancelar",
                        crate::i18n::Lang::En => "Cancel",
                    };
                    if ui.button(yes_lbl).clicked() {
                        confirm = true;
                    }
                    if ui.button(cancel_lbl).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.pending_clear_library = false;
            if let Some(db) = &self.db {
                let _ = db.clear_all();
            }
            self.index = None;
            self.selected_db_study = None;
            self.scan_gen = self.scan_gen.wrapping_add(1);
            self.thumbnails.clear();
            self.thumbnails_queued.clear();
            self.volumes.clear();
            self.series_spacing_override.clear();
            for vp in &mut self.viewports {
                vp.reset_series_state(None);
            }
        } else if !open || cancel {
            self.pending_clear_library = false;
        }
    }

    pub(super) fn draw_pacs_modal(&mut self, ctx: &egui::Context) {
        if !self.show_pacs_modal {
            return;
        }
        let es = self.lang == crate::i18n::Lang::Es;
        let title = if es {
            "PACS / DICOMweb — Consulta y Descarga"
        } else {
            "PACS / DICOMweb — Query & Retrieve"
        };
        let mut open = self.show_pacs_modal;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .default_height(460.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if crate::ui::icons::choice(ui, !self.pacs_tab_dimse, "DICOMweb (QIDO / WADO)")
                        .clicked()
                    {
                        self.pacs_tab_dimse = false;
                    }
                    if crate::ui::icons::choice(ui, self.pacs_tab_dimse, "DICOM PACS (DIMSE)")
                        .clicked()
                    {
                        self.pacs_tab_dimse = true;
                    }
                });
                ui.separator();

                if !self.pacs_tab_dimse {
                    self.draw_dicomweb_tab(ui, ctx, es);
                } else {
                    self.draw_dimse_tab(ui, ctx, es);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if self.pacs_busy {
                        ui.spinner();
                        ui.label(if es { "Conectando…" } else { "Connecting…" });
                    } else if let Some(ref st) = self.pacs_status {
                        ui.label(st);
                    }
                });
            });
        self.show_pacs_modal = open;
    }

    fn draw_dicomweb_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, es: bool) {
        if !self.dicomweb_servers.is_empty() && self.selected_dicomweb >= self.dicomweb_servers.len()
        {
            self.selected_dicomweb = 0;
        }
        ui.horizontal(|ui| {
            ui.label(if es { "Servidor:" } else { "Server:" });
            let cur_label = self
                .dicomweb_servers
                .get(self.selected_dicomweb)
                .map(|s| format!("{} ({})", s.name, s.base_url))
                .unwrap_or_else(|| {
                    if es {
                        "Ninguno configurado".to_string()
                    } else {
                        "None configured".to_string()
                    }
                });
            egui::ComboBox::from_id_salt("dicomweb_server_combo")
                .selected_text(cur_label)
                .width(340.0)
                .show_ui(ui, |ui| {
                    for (i, s) in self.dicomweb_servers.iter().enumerate() {
                        let lbl = format!("{} ({})", s.name, s.base_url);
                        ui.selectable_value(&mut self.selected_dicomweb, i, lbl);
                    }
                });
            if !self.dicomweb_servers.is_empty()
                && ui
                    .button(if es { "Eliminar" } else { "Remove" })
                    .clicked()
            {
                self.dicomweb_servers.remove(self.selected_dicomweb);
                if self.selected_dicomweb >= self.dicomweb_servers.len() {
                    self.selected_dicomweb = self.dicomweb_servers.len().saturating_sub(1);
                }
                if let Some(ref db) = self.db {
                    crate::settings::AppSettings::new(db)
                        .set_dicomweb_servers(&self.dicomweb_servers);
                }
            }
        });

        let needs_secret = self
            .dicomweb_servers
            .get(self.selected_dicomweb)
            .is_some_and(|s| s.auth != crate::net::DicomWebAuth::None);
        if needs_secret {
            ui.horizontal(|ui| {
                ui.label(if es {
                    "Token / Credencial (solo sesión):"
                } else {
                    "Token / Credential (session only):"
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.dicomweb_secret)
                        .password(true)
                        .desired_width(240.0),
                );
            });
        }

        ui.collapsing(
            if es {
                "Agregar servidor DICOMweb"
            } else {
                "Add DICOMweb Server"
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(if es { "Nombre:" } else { "Name:" });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_dw_name)
                            .desired_width(120.0)
                            .hint_text("Orthanc"),
                    );
                    ui.label("URL:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_dw_url)
                            .desired_width(240.0)
                            .hint_text("https://host/dicom-web"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(if es { "Autenticación:" } else { "Auth:" });
                    for (auth, lbl) in [
                        (crate::net::DicomWebAuth::None, if es { "Ninguna" } else { "None" }),
                        (crate::net::DicomWebAuth::BearerToken, "Bearer"),
                        (crate::net::DicomWebAuth::Basic, "Basic"),
                    ] {
                        if crate::ui::icons::choice(ui, self.new_dw_auth == auth, lbl).clicked() {
                            self.new_dw_auth = auth;
                        }
                    }
                    let can_add = !self.new_dw_name.trim().is_empty()
                        && !self.new_dw_url.trim().is_empty();
                    if ui
                        .add_enabled(can_add, egui::Button::new(if es { "Agregar" } else { "Add" }))
                        .clicked()
                    {
                        self.dicomweb_servers.push(crate::net::DicomWebServer {
                            name: self.new_dw_name.trim().to_string(),
                            base_url: self.new_dw_url.trim().to_string(),
                            auth: self.new_dw_auth,
                        });
                        self.selected_dicomweb = self.dicomweb_servers.len() - 1;
                        self.new_dw_name.clear();
                        self.new_dw_url.clear();
                        if let Some(ref db) = self.db {
                            crate::settings::AppSettings::new(db)
                                .set_dicomweb_servers(&self.dicomweb_servers);
                        }
                    }
                });
            },
        );

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(if es { "Paciente:" } else { "Patient:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.patient_name)
                    .desired_width(120.0)
                    .hint_text("DOE*"),
            );
            ui.label("ID:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.patient_id).desired_width(80.0),
            );
            ui.label("Mod:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.modality)
                    .desired_width(48.0)
                    .hint_text("CT"),
            );
            ui.label(if es { "Fecha:" } else { "Date:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.study_date)
                    .desired_width(90.0)
                    .hint_text("YYYYMMDD"),
            );
            let can_search = !self.pacs_busy && !self.dicomweb_servers.is_empty();
            if ui
                .add_enabled(
                    can_search,
                    egui::Button::new(if es { "Buscar" } else { "Search" }),
                )
                .clicked()
            {
                let srv = self.dicomweb_servers[self.selected_dicomweb].clone();
                let filter = self.pacs_filter.clone();
                let secret = if self.dicomweb_secret.trim().is_empty() {
                    None
                } else {
                    Some(self.dicomweb_secret.clone())
                };
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = None;
                std::thread::spawn(move || {
                    let res = crate::net::dicomweb::qido_search_studies(
                        &srv,
                        secret.as_deref(),
                        &filter,
                    )
                    .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Qido(res));
                    ctx_c.request_repaint();
                });
            }
            let study_files = self.active_study_file_paths();
            let can_stow = !self.pacs_busy && !self.dicomweb_servers.is_empty() && !study_files.is_empty();
            if ui
                .add_enabled(
                    can_stow,
                    egui::Button::new(if es { "Enviar (STOW)" } else { "Send (STOW)" }),
                )
                .clicked()
            {
                let srv = self.dicomweb_servers[self.selected_dicomweb].clone();
                let secret = if self.dicomweb_secret.trim().is_empty() {
                    None
                } else {
                    Some(self.dicomweb_secret.clone())
                };
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = Some(if es {
                    format!("Enviando {} archivos por STOW-RS…", study_files.len())
                } else {
                    format!("Uploading {} file(s) via STOW-RS…", study_files.len())
                });
                std::thread::spawn(move || {
                    let res = crate::net::dicomweb::stow_store_files(
                        &srv,
                        secret.as_deref(),
                        &study_files,
                    )
                    .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Store(res));
                    ctx_c.request_repaint();
                });
            }
        });

        ui.add_space(4.0);
        let mut retrieve_uid: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("pacs_results_scroll")
            .max_height(220.0)
            .show(ui, |ui| {
                if self.pacs_results.is_empty() {
                    ui.label(
                        egui::RichText::new(if es {
                            "Sin resultados. Use Buscar para consultar estudios por QIDO-RS."
                        } else {
                            "No results yet. Click Search to query studies via QIDO-RS."
                        })
                        .weak(),
                    );
                } else {
                    for st in &self.pacs_results {
                        ui.horizontal(|ui| {
                            let label = format!(
                                "{}  |  {}  |  {}  |  {}",
                                if st.patient_name.is_empty() {
                                    "—"
                                } else {
                                    &st.patient_name
                                },
                                st.study_date,
                                st.modalities,
                                st.description
                            );
                            ui.label(label);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !self.pacs_busy,
                                            egui::Button::new(if es {
                                                "Descargar y Abrir"
                                            } else {
                                                "Retrieve & Open"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        retrieve_uid = Some(st.study_uid.clone());
                                    }
                                },
                            );
                        });
                        ui.separator();
                    }
                }
            });

        if let Some(uid) = retrieve_uid
            && let Some(srv) = self.dicomweb_servers.get(self.selected_dicomweb).cloned()
        {
            let secret = if self.dicomweb_secret.trim().is_empty() {
                None
            } else {
                Some(self.dicomweb_secret.clone())
            };
            let safe_uid: String = uid
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
                .collect();
            let dest_dir = crate::db::managed_studies_dir()
                .join("pacs_retrieved")
                .join(safe_uid);
            let tx = self.pacs_tx.clone();
            let ctx_c = ctx.clone();
            self.pacs_busy = true;
            self.pacs_status = Some(if es {
                "Descargando estudio por WADO-RS…".to_string()
            } else {
                "Retrieving study via WADO-RS…".to_string()
            });
            std::thread::spawn(move || {
                let res = crate::net::dicomweb::wado_retrieve_study(
                    &srv,
                    secret.as_deref(),
                    &uid,
                    &dest_dir,
                )
                .map(|n| (dest_dir, n))
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(crate::app::PacsBgResult::Wado(res));
                ctx_c.request_repaint();
            });
        }
    }

    fn draw_dimse_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, es: bool) {
        if !self.pacs_nodes.is_empty() && self.selected_pacs_node >= self.pacs_nodes.len() {
            self.selected_pacs_node = 0;
        }
        ui.horizontal(|ui| {
            ui.label(if es { "Nodo PACS:" } else { "PACS Node:" });
            let cur_label = self
                .pacs_nodes
                .get(self.selected_pacs_node)
                .map(|n| format!("{} ({}@{}:{})", n.name, n.ae_title, n.host, n.port))
                .unwrap_or_else(|| {
                    if es {
                        "Ninguno configurado".to_string()
                    } else {
                        "None configured".to_string()
                    }
                });
            egui::ComboBox::from_id_salt("pacs_node_combo")
                .selected_text(cur_label)
                .width(300.0)
                .show_ui(ui, |ui| {
                    for (i, n) in self.pacs_nodes.iter().enumerate() {
                        let lbl = format!("{} ({}@{}:{})", n.name, n.ae_title, n.host, n.port);
                        ui.selectable_value(&mut self.selected_pacs_node, i, lbl);
                    }
                });
            let can_echo = !self.pacs_busy && !self.pacs_nodes.is_empty();
            if ui
                .add_enabled(
                    can_echo,
                    egui::Button::new(if es {
                        "Probar (C-ECHO)"
                    } else {
                        "Test (C-ECHO)"
                    }),
                )
                .clicked()
            {
                let node = self.pacs_nodes[self.selected_pacs_node].clone();
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = None;
                std::thread::spawn(move || {
                    let res = crate::net::dimse::c_echo(&node).map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Echo(res));
                    ctx_c.request_repaint();
                });
            }
            if !self.pacs_nodes.is_empty()
                && ui
                    .button(if es { "Eliminar" } else { "Remove" })
                    .clicked()
            {
                self.pacs_nodes.remove(self.selected_pacs_node);
                if self.selected_pacs_node >= self.pacs_nodes.len() {
                    self.selected_pacs_node = self.pacs_nodes.len().saturating_sub(1);
                }
                if let Some(ref db) = self.db {
                    crate::settings::AppSettings::new(db).set_pacs_nodes(&self.pacs_nodes);
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label(if es { "Paciente:" } else { "Patient:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.patient_name)
                    .desired_width(110.0)
                    .hint_text("DOE*"),
            );
            ui.label("Mod:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.modality)
                    .desired_width(48.0)
                    .hint_text("CT"),
            );
            ui.label(if es { "Fecha:" } else { "Date:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.pacs_filter.study_date)
                    .desired_width(90.0)
                    .hint_text("YYYYMMDD"),
            );
            let can_find = !self.pacs_busy && !self.pacs_nodes.is_empty();
            if ui
                .add_enabled(
                    can_find,
                    egui::Button::new(if es { "Buscar (C-FIND)" } else { "Search (C-FIND)" }),
                )
                .clicked()
            {
                let node = self.pacs_nodes[self.selected_pacs_node].clone();
                let filter = self.pacs_filter.clone();
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = None;
                std::thread::spawn(move || {
                    let res =
                        crate::net::dimse::c_find_studies(&node, &filter).map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Find(res));
                    ctx_c.request_repaint();
                });
            }
            let study_files = self.active_study_file_paths();
            let can_store = !self.pacs_busy && !self.pacs_nodes.is_empty() && !study_files.is_empty();
            if ui
                .add_enabled(
                    can_store,
                    egui::Button::new(if es { "Enviar (C-STORE)" } else { "Send (C-STORE)" }),
                )
                .clicked()
            {
                let node = self.pacs_nodes[self.selected_pacs_node].clone();
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = Some(if es {
                    format!("Enviando {} archivos por C-STORE…", study_files.len())
                } else {
                    format!("Sending {} file(s) via C-STORE…", study_files.len())
                });
                std::thread::spawn(move || {
                    let res = crate::net::dimse::c_store_files(&node, &study_files)
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Store(res));
                    ctx_c.request_repaint();
                });
            }
        });

        if !self.pacs_results.is_empty() {
            let mut move_uid: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("dimse_results_scroll")
                .max_height(130.0)
                .show(ui, |ui| {
                    for st in &self.pacs_results {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{}  |  {}  |  {}  |  {}",
                                if st.patient_name.is_empty() {
                                    "—"
                                } else {
                                    &st.patient_name
                                },
                                st.study_date,
                                st.modalities,
                                st.description
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !self.pacs_busy && !self.pacs_nodes.is_empty(),
                                            egui::Button::new(if es {
                                                "Traer (C-MOVE)"
                                            } else {
                                                "Retrieve (C-MOVE)"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        move_uid = Some(st.study_uid.clone());
                                    }
                                },
                            );
                        });
                    }
                });
            if let Some(uid) = move_uid
                && let Some(node) = self.pacs_nodes.get(self.selected_pacs_node).cloned()
            {
                let dest_ae = node.local_ae_title.clone();
                let tx = self.pacs_tx.clone();
                let ctx_c = ctx.clone();
                self.pacs_busy = true;
                self.pacs_status = Some(if es {
                    "Solicitando estudio por C-MOVE…".to_string()
                } else {
                    "Requesting study via C-MOVE…".to_string()
                });
                std::thread::spawn(move || {
                    let res = crate::net::dimse::c_move_study(&node, &uid, &dest_ae)
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(crate::app::PacsBgResult::Move(res));
                    ctx_c.request_repaint();
                });
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            let listening = self.scp_stop.is_some();
            ui.label(if es {
                "Receptor local C-STORE SCP (puerto):"
            } else {
                "Local C-STORE SCP Receiver (port):"
            });
            ui.add_enabled(
                !listening,
                egui::TextEdit::singleline(&mut self.scp_port).desired_width(60.0),
            );
            if listening {
                if ui
                    .button(if es { "Detener receptor" } else { "Stop Listener" })
                    .clicked()
                {
                    if let Some(flag) = self.scp_stop.take() {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.pacs_status = Some(if es {
                        "Receptor C-STORE detenido.".into()
                    } else {
                        "C-STORE listener stopped.".into()
                    });
                }
            } else if let Ok(port) = self.scp_port.trim().parse::<u16>()
                && port > 0
                && ui
                    .button(if es { "Iniciar receptor" } else { "Start Listener" })
                    .clicked()
            {
                match std::net::TcpListener::bind(("0.0.0.0", port)) {
                    Ok(listener) => {
                        let _ = listener.set_nonblocking(true);
                        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        self.scp_stop = Some(stop.clone());
                        let tx = self.pacs_tx.clone();
                        let ctx_c = ctx.clone();
                        let base_dir = crate::db::managed_studies_dir().join("pacs_scp_incoming");
                        self.pacs_status = Some(if es {
                            format!("Escuchando C-STORE SCP en puerto {port}…")
                        } else {
                            format!("Listening for C-STORE SCP on port {port}…")
                        });
                        std::thread::spawn(move || {
                            let mut batch = 0usize;
                            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                                match listener.accept() {
                                    Ok((stream, _)) => {
                                        let _ = stream.set_nonblocking(false);
                                        batch += 1;
                                        let dest = base_dir.join(format!("recv_{batch:04}"));
                                        if let Ok(n) =
                                            crate::net::dimse::handle_scp_connection(stream, &dest)
                                            && n > 0
                                        {
                                            let _ = tx.send(
                                                crate::app::PacsBgResult::ScpReceived(dest, n),
                                            );
                                            ctx_c.request_repaint();
                                        }
                                    }
                                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                        std::thread::sleep(std::time::Duration::from_millis(120));
                                    }
                                    Err(_) => break,
                                }
                            }
                        });
                    }
                    Err(e) => {
                        self.pacs_status = Some(format!("SCP bind error: {e}"));
                    }
                }
            }
        });

        ui.separator();
        ui.label(
            egui::RichText::new(if es {
                "Agregar nodo DICOM (DIMSE)"
            } else {
                "Add DICOM (DIMSE) Node"
            })
            .strong(),
        );
        ui.horizontal(|ui| {
            ui.label(if es { "Nombre:" } else { "Name:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.new_pacs_name)
                    .desired_width(110.0)
                    .hint_text("Local PACS"),
            );
            ui.label("Called AE:");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_pacs_ae)
                    .desired_width(90.0)
                    .hint_text("ORTHANC"),
            );
            ui.label("Local AE:");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_pacs_local_ae).desired_width(90.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Host:");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_pacs_host)
                    .desired_width(180.0)
                    .hint_text("127.0.0.1"),
            );
            ui.label(if es { "Puerto:" } else { "Port:" });
            ui.add(
                egui::TextEdit::singleline(&mut self.new_pacs_port).desired_width(60.0),
            );
            ui.checkbox(&mut self.new_pacs_tls, "TLS");
            let port_ok = self.new_pacs_port.trim().parse::<u16>().ok().filter(|&p| p > 0);
            let candidate = port_ok.map(|port| crate::net::PacsNode {
                name: self.new_pacs_name.trim().to_string(),
                ae_title: self.new_pacs_ae.trim().to_string(),
                host: self.new_pacs_host.trim().to_string(),
                port,
                local_ae_title: self.new_pacs_local_ae.trim().to_string(),
                use_tls: self.new_pacs_tls,
                retrieve_method: crate::net::RetrieveMethod::CMove,
            });
            let valid = candidate
                .as_ref()
                .is_some_and(|n| !n.name.is_empty() && n.is_valid());
            if ui
                .add_enabled(valid, egui::Button::new(if es { "Agregar" } else { "Add" }))
                .clicked()
                && let Some(node) = candidate
            {
                self.pacs_nodes.push(node);
                self.selected_pacs_node = self.pacs_nodes.len() - 1;
                self.new_pacs_name.clear();
                self.new_pacs_ae.clear();
                self.new_pacs_host.clear();
                if let Some(ref db) = self.db {
                    crate::settings::AppSettings::new(db).set_pacs_nodes(&self.pacs_nodes);
                }
            }
        });
    }

    pub(super) fn draw_rename_zip_modal(&mut self, ctx: &egui::Context) {
        let Some((study_uid, zip_path)) = self.rename_zip_target.clone() else {
            return;
        };
        let es = self.lang == crate::i18n::Lang::Es;
        let title = if es {
            "Renombrar archivo .zip"
        } else {
            "Rename .zip file"
        };
        let mut open = true;
        let mut do_rename = false;
        let mut cancel = false;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                let cur_name = zip_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("archive.zip");
                ui.label(format!(
                    "{} {cur_name}",
                    if es { "Archivo actual:" } else { "Current file:" }
                ));
                ui.add_space(4.0);
                ui.label(if es {
                    "Nuevo nombre (sin .zip):"
                } else {
                    "New name (without .zip):"
                });
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.rename_zip_text)
                        .desired_width(f32::INFINITY),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    do_rename = true;
                }
                ui.add_space(4.0);
                ui.small(if es {
                    "Renombrar manualmente evita que el renombrado automático sobrescriba este archivo."
                } else {
                    "Manual renaming prevents auto-rename from overwriting this file's name."
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.rename_zip_text.trim().is_empty(),
                            egui::Button::new(if es { "Renombrar" } else { "Rename" }),
                        )
                        .clicked()
                    {
                        do_rename = true;
                    }
                    if ui
                        .button(if es { "Cancelar" } else { "Cancel" })
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });

        if do_rename {
            match crate::dicom_index::rename_zip_file(&zip_path, &self.rename_zip_text) {
                Ok(new_path) => {
                    self.manually_renamed_zips.insert(study_uid);
                    if let Some(ref db) = self.db {
                        crate::settings::AppSettings::new(db)
                            .set_manually_renamed_zips(&self.manually_renamed_zips);
                    }
                    let fname = new_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("archive.zip");
                    self.message = Some(if es {
                        format!("ZIP renombrado manualmente a: {fname}")
                    } else {
                        format!("Renamed ZIP to: {fname}")
                    });
                    self.rename_zip_target = None;
                }
                Err(e) => {
                    self.message = Some(format!("ZIP rename failed: {e}"));
                }
            }
        } else if !open || cancel {
            self.rename_zip_target = None;
        }
    }
}

/// Opens `dir` in the system file manager (Explorer, Finder, or the desktop's own).
fn open_in_file_manager(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

fn summarize_samples(samples: &[f32]) -> (f32, f32, f32) {
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    let mut sum = 0.0_f64;
    for &v in samples {
        min_v = min_v.min(v);
        max_v = max_v.max(v);
        sum += v as f64;
    }
    let mean_v = (sum / samples.len().max(1) as f64) as f32;
    (min_v, max_v, mean_v)
}

fn draw_histogram_bars(
    ui: &mut egui::Ui,
    frame: &crate::frame::Frame,
    outline: &[crate::roi::RoiPoint],
    unit: &str,
    lang: crate::i18n::Lang,
) {
    let samples = crate::roi::sample_region_values(frame, outline);
    if samples.is_empty() {
        ui.small(if lang == crate::i18n::Lang::Es { "Sin píxeles dentro de la ROI." } else { "No interior pixels." });
        return;
    }
    let (min_v, max_v, mean_v) = summarize_samples(&samples);
    ui.small(format!(
        "N: {}  Min: {min_v:.1}  Mean: {mean_v:.1}  Max: {max_v:.1} {unit}",
        samples.len()
    ));
    const BINS: usize = 28;
    let mut counts = [0_u32; BINS];
    let span = (max_v - min_v).max(1e-3);
    for &v in &samples {
        let norm = ((v - min_v) / span).clamp(0.0, 0.9999);
        let b = (norm * BINS as f32) as usize;
        counts[b.min(BINS - 1)] += 1;
    }
    let max_c = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let desired = egui::vec2(ui.available_width().max(180.0), 92.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::from_gray(22));
    let inner = rect.shrink(4.0);
    let bw = inner.width() / BINS as f32;
    for (i, &c) in counts.iter().enumerate() {
        let h = (c as f32 / max_c) * inner.height();
        let x0 = inner.left() + i as f32 * bw;
        let bar = Rect::from_min_max(
            egui::pos2(x0 + 0.5, inner.bottom() - h),
            egui::pos2(x0 + bw - 0.5, inner.bottom()),
        );
        p.rect_filled(bar, 1.0, Color32::from_rgb(90, 175, 255));
    }
}

