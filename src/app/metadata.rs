use std::path::{Path, PathBuf};

use dicom_core::dictionary::DataDictionary;
use dicom_core::value::Value;
use dicom_dictionary_std::{tags, StandardDataDictionary};
use dicom_object::{InMemDicomObject, OpenFileOptions};
use eframe::egui::{self, Color32};

use super::ViewerApp;

#[derive(Clone, Debug)]
pub(crate) struct DicomTagEntry {
    pub(crate) tag_str: String,
    pub(crate) name: String,
    pub(crate) vr: String,
    pub(crate) size_str: String,
    pub(crate) value: String,
    pub(crate) depth: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct MetadataCache {
    pub(crate) path: PathBuf,
    pub(crate) file_size_bytes: u64,
    pub(crate) entries: Vec<DicomTagEntry>,
    pub(crate) error: Option<String>,
}

fn format_bytes(len: usize) -> String {
    if len >= 1024 {
        format!("{:.1} KB", len as f64 / 1024.0)
    } else {
        format!("{len} B")
    }
}

fn format_tag_name(tag: dicom_core::Tag) -> String {
    StandardDataDictionary
        .by_tag(tag)
        .map(|entry| entry.alias.to_string())
        .unwrap_or_else(|| {
            if tag.0 % 2 == 1 {
                "PrivateTag".to_string()
            } else {
                "UnknownTag".to_string()
            }
        })
}

fn collect_dataset_entries(
    obj: &InMemDicomObject,
    depth: usize,
    out: &mut Vec<DicomTagEntry>,
) {
    if depth > 4 {
        return;
    }
    for elem in obj.iter() {
        let tag = elem.header().tag;
        let tag_str = format!("{:04X},{:04X}", tag.0, tag.1);
        let vr = elem.vr().to_string().to_owned();
        let name = format_tag_name(tag);

        if tag == tags::PIXEL_DATA {
            let len_str = elem
                .header()
                .len
                .get()
                .map(|l| format_bytes(l as usize))
                .unwrap_or_else(|| "encapsulated".to_string());
            out.push(DicomTagEntry {
                tag_str,
                name,
                vr,
                size_str: len_str.clone(),
                value: format!("<Pixel Data ({len_str})>"),
                depth,
            });
            continue;
        }

        match elem.value() {
            Value::Sequence(seq) => {
                let items = seq.items();
                out.push(DicomTagEntry {
                    tag_str,
                    name,
                    vr,
                    size_str: format!("{} items", items.len()),
                    value: format!("({} sequence item{})", items.len(), if items.len() == 1 { "" } else { "s" }),
                    depth,
                });
                for (idx, item) in items.iter().enumerate().take(16) {
                    out.push(DicomTagEntry {
                        tag_str: "FFFE,E000".to_string(),
                        name: format!("Item #{}", idx + 1),
                        vr: "--".to_string(),
                        size_str: String::new(),
                        value: String::new(),
                        depth: depth + 1,
                    });
                    collect_dataset_entries(item, depth + 2, out);
                }
            }
            Value::PixelSequence(px_seq) => {
                let n = px_seq.fragments().len();
                out.push(DicomTagEntry {
                    tag_str,
                    name,
                    vr,
                    size_str: format!("{n} frags"),
                    value: format!("<Pixel Sequence ({n} fragments)>"),
                    depth,
                });
            }
            Value::Primitive(prim) => {
                let raw = prim.to_str();
                let byte_len = elem
                    .header()
                    .len
                    .get()
                    .map(|l| format_bytes(l as usize))
                    .unwrap_or_else(|| format_bytes(raw.len()));
                let display = if raw.len() > 200 {
                    format!("{}…", raw.chars().take(197).collect::<String>())
                } else {
                    raw.into_owned()
                };
                out.push(DicomTagEntry {
                    tag_str,
                    name,
                    vr,
                    size_str: byte_len,
                    value: display,
                    depth,
                });
            }
        }
    }
}

pub(crate) fn parse_dicom_metadata(path: &Path) -> MetadataCache {
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    match OpenFileOptions::new()
        .read_until(tags::PIXEL_DATA)
        .open_file(path)
    {
        Ok(obj) => {
            let mut entries = Vec::new();
            let meta = obj.meta();
            let mut push_meta = |tag_str: &str, name: &str, vr: &str, val: &str| {
                if !val.is_empty() {
                    entries.push(DicomTagEntry {
                        tag_str: tag_str.to_string(),
                        name: name.to_string(),
                        vr: vr.to_string(),
                        size_str: format_bytes(val.len()),
                        value: val.to_string(),
                        depth: 0,
                    });
                }
            };
            push_meta("0002,0002", "MediaStorageSOPClassUID", "UI", &meta.media_storage_sop_class_uid);
            push_meta("0002,0003", "MediaStorageSOPInstanceUID", "UI", &meta.media_storage_sop_instance_uid);
            push_meta("0002,0010", "TransferSyntaxUID", "UI", &meta.transfer_syntax);
            push_meta("0002,0012", "ImplementationClassUID", "UI", &meta.implementation_class_uid);
            if let Some(ref ivn) = meta.implementation_version_name {
                push_meta("0002,0013", "ImplementationVersionName", "SH", ivn);
            }
            collect_dataset_entries(&obj, 0, &mut entries);
            MetadataCache {
                path: path.to_path_buf(),
                file_size_bytes,
                entries,
                error: None,
            }
        }
        Err(err) => MetadataCache {
            path: path.to_path_buf(),
            file_size_bytes,
            entries: Vec::new(),
            error: Some(format!("Failed to read DICOM metadata: {err}")),
        },
    }
}

impl ViewerApp {
    pub(crate) fn metadata_inspector_panel(&mut self, ui: &mut egui::Ui) {
        let es = self.lang == crate::i18n::Lang::Es;
        let vp_idx = self.active_viewport;
        let slice_path = self
            .viewport_series(vp_idx)
            .and_then(|s| s.slices.get(self.viewports[vp_idx].slice))
            .map(|sr| sr.path.clone());
        let series_title = self
            .viewport_series(vp_idx)
            .map(|s| format!("{} (Se {})", s.description, s.number))
            .unwrap_or_else(|| (if es { "Ninguna serie activa" } else { "No active series" }).to_string());

        if let Some(ref path) = slice_path
            && self.metadata_cache.as_ref().map(|c| &c.path) != Some(path)
        {
            self.metadata_cache = Some(parse_dicom_metadata(path));
        }

        // Header bar (OsiriX Meta-Data Inspector style)
        ui.horizontal(|ui| {
            let size_kb = self
                .metadata_cache
                .as_ref()
                .map(|c| c.file_size_bytes as f64 / 1024.0)
                .unwrap_or(0.0);
            ui.label(
                egui::RichText::new(format!("{}: {series_title} ({size_kb:.0} KB)", if es { "Metadatos" } else { "Meta-Data" }))
                    .strong()
                    .size(12.5),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui::icons::small_tool_button(ui, crate::ui::icons::Icon::Close, false).clicked() {
                    self.show_metadata_inspector = false;
                }
                if let Some(cache) = &self.metadata_cache
                    && !cache.entries.is_empty()
                    && ui.small_button(if es { "Exportar texto" } else { "Export text" }).clicked()
                    && let Some(save_path) = rfd::FileDialog::new()
                        .set_file_name("dicom_metadata.txt")
                        .save_file()
                {
                    let mut out = String::new();
                    for e in &cache.entries {
                        let indent = "  ".repeat(e.depth);
                        out.push_str(&format!(
                            "{indent}{}  {:<36}  {:<2}  {:<10}  {}\n",
                            e.tag_str, e.name, e.vr, e.size_str, e.value
                        ));
                    }
                    let _ = std::fs::write(save_path, out);
                }
            });
        });

        ui.separator();

        // Search / Filter bar
        ui.horizontal(|ui| {
            let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), egui::Sense::hover());
            crate::ui::icons::paint(
                ui.painter(),
                egui::Rect::from_center_size(icon_rect.center(), egui::vec2(13.0, 13.0)),
                crate::ui::icons::Icon::Search,
                crate::ui::theme::dim_text(ui),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.metadata_filter)
                    .hint_text(if es { "Buscar etiqueta, campo o valor…" } else { "Search tag, field name or value…" })
                    .desired_width(ui.available_width() - 60.0),
            );
            if !self.metadata_filter.is_empty() && ui.small_button(if es { "Limpiar" } else { "Clear" }).clicked() {
                self.metadata_filter.clear();
            }
        });

        ui.separator();

        let Some(cache) = &self.metadata_cache else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(if es { "Seleccione una imagen para ver sus etiquetas DICOM." } else { "Select an image to inspect its DICOM tags." })
                        .color(Color32::from_gray(150)),
                );
            });
            return;
        };

        if let Some(err) = &cache.error {
            ui.colored_label(Color32::from_rgb(240, 110, 110), err);
            return;
        }

        let needle = self.metadata_filter.trim().to_ascii_lowercase();
        let edit_path = cache.path.clone();
        let mut save_tag: Option<(dicom_core::Tag, dicom_core::VR, String)> = None;
        if edit_path.is_file() {
            const EDITABLE: [(&str, dicom_core::Tag, dicom_core::VR); 5] = [
                ("StudyDescription", tags::STUDY_DESCRIPTION, dicom_core::VR::LO),
                ("SeriesDescription", tags::SERIES_DESCRIPTION, dicom_core::VR::LO),
                ("PatientName", tags::PATIENT_NAME, dicom_core::VR::PN),
                ("AccessionNumber", tags::ACCESSION_NUMBER, dicom_core::VR::SH),
                ("InstitutionName", tags::INSTITUTION_NAME, dicom_core::VR::LO),
            ];
            ui.collapsing(
                if es {
                    "Editar etiqueta DICOM"
                } else {
                    "Edit DICOM Tag"
                },
                |ui| {
                    ui.horizontal(|ui| {
                        let cur_idx = self.meta_edit_field.min(EDITABLE.len() - 1);
                        egui::ComboBox::from_id_salt("meta_edit_tag_combo")
                            .selected_text(EDITABLE[cur_idx].0)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for (i, (name, _, _)) in EDITABLE.iter().enumerate() {
                                    ui.selectable_value(&mut self.meta_edit_field, i, *name);
                                }
                            });
                        ui.add(
                            egui::TextEdit::singleline(&mut self.meta_edit_value)
                                .desired_width(140.0)
                                .hint_text(if es { "Nuevo valor…" } else { "New value…" }),
                        );
                        if ui
                            .button(if es { "Guardar" } else { "Save" })
                            .clicked()
                        {
                            let (_, tag, vr) = EDITABLE[cur_idx];
                            save_tag = Some((tag, vr, self.meta_edit_value.trim().to_string()));
                        }
                    });
                },
            );
        }
        let filtered: Vec<&DicomTagEntry> = cache
            .entries
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.tag_str.to_ascii_lowercase().contains(&needle)
                    || e.name.to_ascii_lowercase().contains(&needle)
                    || e.vr.to_ascii_lowercase().contains(&needle)
                    || e.value.to_ascii_lowercase().contains(&needle)
            })
            .collect();

        ui.label(
            egui::RichText::new(format!("{} {}", filtered.len(), if es { "etiquetas" } else { "tags" }))
                .size(10.5)
                .color(Color32::from_gray(145)),
        );

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("dicom_metadata_grid")
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(if es { "Campo" } else { "Field name" }).strong().size(11.0));
                        ui.label(egui::RichText::new(if es { "Etiqueta" } else { "Tag" }).strong().size(11.0));
                        ui.label(egui::RichText::new("VR").strong().size(11.0));
                        ui.label(egui::RichText::new(if es { "Tamaño" } else { "Size" }).strong().size(11.0));
                        ui.label(egui::RichText::new(if es { "Contenido" } else { "Content" }).strong().size(11.0));
                        ui.end_row();

                        for entry in filtered {
                            let indent = "  ".repeat(entry.depth);
                            let name_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{indent}{}", entry.name))
                                        .size(11.0)
                                        .color(if entry.depth > 0 {
                                            Color32::from_gray(180)
                                        } else {
                                            Color32::from_gray(230)
                                        }),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if name_resp.clicked() {
                                ui.ctx().copy_text(format!(
                                    "{}  {}  [{}]  {}",
                                    entry.tag_str, entry.name, entry.vr, entry.value
                                ));
                            }
                            name_resp.on_hover_text(if es { "Clic para copiar esta línea" } else { "Click to copy this line" });

                            ui.label(
                                egui::RichText::new(&entry.tag_str)
                                    .monospace()
                                    .size(10.5)
                                    .color(Color32::from_rgb(150, 200, 255)),
                            );
                            ui.label(
                                egui::RichText::new(&entry.vr)
                                    .monospace()
                                    .size(10.5)
                                    .color(Color32::from_gray(165)),
                            );
                            ui.label(
                                egui::RichText::new(&entry.size_str)
                                    .monospace()
                                    .size(10.5)
                                    .color(Color32::from_gray(155)),
                            );
                            ui.label(
                                egui::RichText::new(&entry.value)
                                    .monospace()
                                    .size(10.5)
                                    .color(Color32::from_gray(225)),
                            );
                            ui.end_row();
                        }
                    });
            });

        if let Some((tag, vr, val)) = save_tag {
            let res: Result<(), String> = (|| {
                let mut obj = dicom_object::open_file(&edit_path).map_err(|e| e.to_string())?;
                obj.put(dicom_core::DataElement::new(
                    tag,
                    vr,
                    dicom_core::PrimitiveValue::from(val.as_str()),
                ));
                obj.write_to_file(&edit_path).map_err(|e| e.to_string())
            })();
            match res {
                Ok(()) => {
                    self.metadata_cache = None;
                    self.message = Some(if es {
                        "Etiqueta DICOM guardada en el archivo.".into()
                    } else {
                        "Saved DICOM tag to file.".into()
                    });
                }
                Err(e) => {
                    self.message = Some(format!("DICOM tag save failed: {e}"));
                }
            }
        }
    }
}
