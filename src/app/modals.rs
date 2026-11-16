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
                    ui.colored_label(Color32::from_rgb(255, 200, 80), err.reason());
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
                    .color(Color32::from_gray(170)),
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
                                .color(Color32::from_rgb(130, 220, 155)),
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
                                .color(Color32::from_rgb(130, 210, 255)),
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
                            egui::RichText::new(
                                "Note: Release binaries are unsigned; integrity is verified via SHA256SUMS.",
                            )
                            .small()
                            .color(Color32::from_gray(160)),
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
                            egui::RichText::new(format!(
                                "SekhmetX {tag} installed! Restart to launch the new version."
                            ))
                            .strong()
                            .color(Color32::from_rgb(130, 220, 155)),
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
                            egui::RichText::new(format!("Update check failed: {err}"))
                                .color(Color32::from_rgb(240, 120, 120)),
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
                    .checkbox(&mut self.check_updates_on_startup, "Check for updates on startup")
                    .changed()
                    && let Some(ref db) = self.db {
                        let _ = db.set_setting(
                            "check_updates_on_startup",
                            if self.check_updates_on_startup { "1" } else { "0" },
                        );
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
                    ui.colored_label(Color32::from_rgb(230, 180, 60), pub_warn);
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
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr.settings_language);
                    if ui
                        .selectable_label(self.lang == crate::i18n::Lang::En, "English")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::En;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("lang", "en");
                        }
                    }
                    if ui
                        .selectable_label(self.lang == crate::i18n::Lang::Es, "Español")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::Es;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("lang", "es");
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let (theme_lbl, lbl_dark, lbl_light, lbl_sys) = match self.lang {
                        crate::i18n::Lang::Es => ("Tema:", "🌙 Oscuro", "☀ Claro", "Sistema"),
                        crate::i18n::Lang::En => ("Theme:", "🌙 Dark", "☀ Light", "System"),
                    };
                    ui.label(theme_lbl);
                    for (mode, lbl) in [
                        (super::ThemeMode::Dark, lbl_dark),
                        (super::ThemeMode::Light, lbl_light),
                        (super::ThemeMode::System, lbl_sys),
                    ] {
                        if ui.selectable_label(self.theme_mode == mode, lbl).clicked() {
                            self.theme_mode = mode;
                            mode.apply(ctx);
                            if let Some(ref db) = self.db {
                                let _ = db.set_setting("theme_mode", mode.as_str());
                            }
                        }
                    }
                });
                self.draw_accent_settings(ui, ctx);

                ui.add_space(6.0);
                ui.separator();
                ui.label(tr.settings_studies_folder);
                ui.horizontal(|ui| {
                    let cur = crate::db::managed_studies_dir();
                    ui.monospace(cur.display().to_string());
                    if ui.button(tr.choose_folder).clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.select_studies_folder(dir);
                    }
                    if ui.button(tr.settings_reset_default).clicked() {
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("custom_studies_dir", "");
                        }
                        crate::db::set_custom_studies_dir(None);
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                ui.strong(tr.settings_on_open);
                ui.horizontal(|ui| {
                    ui.label(tr.settings_add_to_db);
                    for (policy, label) in [
                        (crate::db::CopyToLibraryPolicy::Ask, tr.settings_policy_ask),
                        (crate::db::CopyToLibraryPolicy::Always, tr.settings_policy_always),
                        (crate::db::CopyToLibraryPolicy::Never, tr.settings_policy_never),
                    ] {
                        if ui.selectable_label(self.copy_policy == policy, label).clicked() {
                            self.set_copy_policy(policy);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr.settings_storage_format);
                    if ui
                        .selectable_label(!self.study_storage_zip, tr.settings_format_unzip)
                        .clicked()
                    {
                        self.study_storage_zip = false;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("study_storage_format", "unzip");
                        }
                    }
                    if ui
                        .selectable_label(self.study_storage_zip, tr.settings_format_zip)
                        .clicked()
                    {
                        self.study_storage_zip = true;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("study_storage_format", "zip");
                        }
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                let (upd_hdr, lbl_auto, lbl_manual, lbl_check_now) = match self.lang {
                    crate::i18n::Lang::Es => (
                        "Actualizaciones",
                        "Buscar al iniciar",
                        "Solo manual",
                        "🔄 Buscar ahora",
                    ),
                    crate::i18n::Lang::En => (
                        "Updates",
                        "Check on startup",
                        "Manual only",
                        "🔄 Check now",
                    ),
                };
                ui.strong(upd_hdr);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.check_updates_on_startup, lbl_auto)
                        .clicked()
                    {
                        self.check_updates_on_startup = true;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("check_updates_on_startup", "1");
                        }
                    }
                    if ui
                        .selectable_label(!self.check_updates_on_startup, lbl_manual)
                        .clicked()
                    {
                        self.check_updates_on_startup = false;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("check_updates_on_startup", "0");
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
                                Color32::from_rgb(120, 225, 150),
                                format!("⬆ {} available", info.tag_name),
                            );
                            if ui.small_button("Install…").clicked() {
                                self.show_update_modal = true;
                            }
                        });
                    }
                    crate::updater::UpdateStatus::Installed(tag) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                Color32::from_rgb(120, 225, 150),
                                format!("✅ {tag} installed"),
                            );
                            if ui.small_button("Restart…").clicked() {
                                self.show_update_modal = true;
                            }
                        });
                    }
                    crate::updater::UpdateStatus::UpToDate(tag) => {
                        ui.small(format!("✓ v{} ({tag})", env!("CARGO_PKG_VERSION")));
                    }
                    crate::updater::UpdateStatus::Checking => {
                        ui.small("Checking GitHub for updates…");
                    }
                    crate::updater::UpdateStatus::Installing => {
                        ui.small("Downloading and installing update…");
                    }
                    crate::updater::UpdateStatus::Error(err) => {
                        ui.colored_label(Color32::from_rgb(240, 120, 120), err);
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

                ui.add_space(6.0);
                ui.separator();
                let data_lbl = match self.lang {
                    crate::i18n::Lang::Es => "Carpeta de datos locales (base de datos y registros):",
                    crate::i18n::Lang::En => "Local data folder (database and logs):",
                };
                ui.label(data_lbl);
                let data_dir = crate::crash_log::app_data_dir();
                ui.monospace(data_dir.display().to_string());
                ui.horizontal(|ui| {
                    let open_lbl = match self.lang {
                        crate::i18n::Lang::Es => "Abrir carpeta",
                        crate::i18n::Lang::En => "Open folder",
                    };
                    if ui.button(open_lbl).clicked() {
                        let _ = std::fs::create_dir_all(&data_dir);
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("explorer").arg(&data_dir).spawn();
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&data_dir).spawn();
                        #[cfg(all(unix, not(target_os = "macos")))]
                        let _ = std::process::Command::new("xdg-open").arg(&data_dir).spawn();
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
                    ui.colored_label(Color32::from_rgb(230, 90, 90), confirm_msg);
                    ui.horizontal(|ui| {
                        let yes_lbl = match self.lang {
                            crate::i18n::Lang::Es => "Confirmar eliminación",
                            crate::i18n::Lang::En => "Confirm delete",
                        };
                        if ui.button(yes_lbl).clicked() {
                            self.confirm_delete_local_data = false;
                            let _ = self.db.take();
                            self.index = None;
                            let _ = std::fs::remove_dir_all(crate::db::managed_studies_dir());
                            let _ = std::fs::remove_dir_all(&data_dir);
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
            let _ = db.set_setting("custom_studies_dir", &dir.to_string_lossy());
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
                    if ui
                        .selectable_label(self.lang == crate::i18n::Lang::En, "English")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::En;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("lang", "en");
                        }
                    }
                    if ui
                        .selectable_label(self.lang == crate::i18n::Lang::Es, "Español")
                        .clicked()
                    {
                        self.lang = crate::i18n::Lang::Es;
                        if let Some(ref db) = self.db {
                            let _ = db.set_setting("lang", "es");
                        }
                    }
                });

                ui.horizontal(|ui| {
                    let (theme_lbl, lbl_dark, lbl_light, lbl_sys) = match self.lang {
                        crate::i18n::Lang::Es => ("Tema:", "🌙 Oscuro", "☀ Claro", "Sistema"),
                        crate::i18n::Lang::En => ("Theme:", "🌙 Dark", "☀ Light", "System"),
                    };
                    ui.label(theme_lbl);
                    for (mode, lbl) in [
                        (super::ThemeMode::Dark, lbl_dark),
                        (super::ThemeMode::Light, lbl_light),
                        (super::ThemeMode::System, lbl_sys),
                    ] {
                        if ui.selectable_label(self.theme_mode == mode, lbl).clicked() {
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
                ui.monospace(data_dir.display().to_string());
                ui.add_space(6.0);
                let studies_lbl = match self.lang {
                    crate::i18n::Lang::Es => "• Carpeta de la biblioteca de estudios (donde se guardan sus estudios):",
                    crate::i18n::Lang::En => "• Studies library folder (where your studies are kept):",
                };
                ui.label(studies_lbl);
                let tr = crate::i18n::tr(self.lang);
                ui.horizontal(|ui| {
                    let cur_studies = crate::db::managed_studies_dir();
                    ui.monospace(cur_studies.display().to_string());
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
                        let _ = db.set_setting("first_run_notice_ack", "1");
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
            crate::i18n::Lang::Es => "📈 Función de Transferencia 3D (LUT 1D)",
            crate::i18n::Lang::En => "📈 3D Transfer Function Editor (1D LUT)",
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
                        && ui.small_button("🗑").on_hover_text("Remove stop").clicked()
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
                ui.small(egui::RichText::new(hint).color(Color32::from_gray(145)));
            });
        self.show_vr_tf_editor = open;
    }
}

