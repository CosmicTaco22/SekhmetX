use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dicom_core::{DataElement, PrimitiveValue, Tag, VR};
use dicom_dictionary_std::tags;
use dicom_object::open_file;

use crate::dicom_index::SliceRef;
use crate::frame::{load_frame, Frame};
use crate::imaging::clut::ColourTableBook;
use crate::imaging::convolution::FACTORY_CONVOLUTIONS;
use crate::imaging::Window;

/// Options controlling DICOM de-identification (`anonymize-export`).
#[derive(Clone, Debug)]
pub struct AnonymizeOptions {
    pub patient_name: String,
    pub patient_id: String,
    pub strip_private_tags: bool,
    pub regenerate_uids: bool,
    pub clear_dates: bool,
}

impl Default for AnonymizeOptions {
    fn default() -> Self {
        Self {
            patient_name: "ANONYMIZED".to_string(),
            patient_id: "ANON0001".to_string(),
            strip_private_tags: true,
            regenerate_uids: true,
            clear_dates: true,
        }
    }
}

/// Renders a single `Frame` with the given presentation settings and writes a PNG file.
pub fn export_frame_png(
    frame: &Frame,
    window: Window,
    invert: bool,
    clut_name: &str,
    conv_idx: Option<usize>,
    out_path: &Path,
) -> Result<()> {
    let display_frame = conv_idx
        .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
        .map(|k| frame.with_convolution(&k.to_convolution()));
    let src = display_frame.as_ref().unwrap_or(frame);
    let clut = ColourTableBook::table_named(clut_name);
    let rgba = src.render_with_clut(window, invert != src.inverts, clut.as_ref());
    image::save_buffer(out_path, &rgba, src.width, src.height, image::ColorType::Rgba8)
        .with_context(|| format!("failed to write PNG {}", out_path.display()))
}

/// Exports every slice in `slices` as sequentially numbered PNGs inside `out_dir`.
pub fn export_series_png(
    slices: &[SliceRef],
    window: Option<Window>,
    invert: bool,
    clut_name: &str,
    conv_idx: Option<usize>,
    out_dir: &Path,
) -> Result<usize> {
    std::fs::create_dir_all(out_dir)?;
    let mut count = 0usize;
    for (idx, sl) in slices.iter().enumerate() {
        let frame = load_frame(&sl.path, sl.frame)?;
        let w = match window {
            Some(req) => frame.requested(req).unwrap_or(frame.full_window),
            None => frame.default_window(),
        };
        let file = out_dir.join(format!("slice_{:04}.png", idx + 1));
        export_frame_png(&frame, w, invert, clut_name, conv_idx, &file)?;
        count += 1;
    }
    Ok(count)
}

/// Generates a deterministic-looking valid DICOM UID under the SekhmetX OID root.
fn make_uid(counter: usize) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("2.25.{nanos}.{counter}")
}

fn map_uid(uid_map: &mut HashMap<String, String>, old: &str, counter: &mut usize) -> String {
    if let Some(existing) = uid_map.get(old) {
        return existing.clone();
    }
    *counter += 1;
    let new_uid = make_uid(*counter);
    uid_map.insert(old.to_string(), new_uid.clone());
    new_uid
}

/// De-identifies a single DICOM file following Horos/IsiX anonymization rules and writes it to `dst`.
#[allow(deprecated)]
pub fn anonymize_dicom_file(
    src: &Path,
    dst: &Path,
    opts: &AnonymizeOptions,
    uid_map: &mut HashMap<String, String>,
    uid_counter: &mut usize,
) -> Result<()> {
    let mut obj = open_file(src).with_context(|| format!("open DICOM {}", src.display()))?;

    // 1. Remove private tags (odd group numbers) if requested.
    if opts.strip_private_tags {
        let private_tags: Vec<Tag> = obj
            .tags()
            .filter(|t| (t.group() & 1) == 1)
            .collect();
        for t in private_tags {
            obj.remove_element(t);
        }
    }

    // 2. Replace core patient / institution / physician identifiers.
    let replace_str = |obj: &mut dicom_object::DefaultDicomObject, tag: Tag, vr: VR, val: &str| {
        if obj.element(tag).is_ok() {
            obj.put(DataElement::new(tag, vr, PrimitiveValue::from(val)));
        }
    };

    obj.put(DataElement::new(
        tags::PATIENT_NAME,
        VR::PN,
        PrimitiveValue::from(opts.patient_name.as_str()),
    ));
    obj.put(DataElement::new(
        tags::PATIENT_ID,
        VR::LO,
        PrimitiveValue::from(opts.patient_id.as_str()),
    ));

    replace_str(&mut obj, tags::OTHER_PATIENT_I_DS, VR::LO, &opts.patient_id);
    replace_str(&mut obj, tags::ACCESSION_NUMBER, VR::SH, "");
    replace_str(&mut obj, tags::INSTITUTION_NAME, VR::LO, "ANONYMIZED");
    replace_str(&mut obj, tags::REFERRING_PHYSICIAN_NAME, VR::PN, "");
    replace_str(&mut obj, tags::PERFORMING_PHYSICIAN_NAME, VR::PN, "");
    replace_str(&mut obj, tags::OPERATORS_NAME, VR::PN, "");
    replace_str(&mut obj, tags::STUDY_ID, VR::SH, "ANON");

    // 3. Remove sensitive free-text / contact attributes.
    let strip_tags = [
        tags::PATIENT_ADDRESS,
        tags::PATIENT_TELEPHONE_NUMBERS,
        tags::INSTITUTION_ADDRESS,
        tags::OTHER_PATIENT_NAMES,
        tags::PATIENT_MOTHER_BIRTH_NAME,
        tags::MEDICAL_RECORD_LOCATOR,
        tags::ETHNIC_GROUP,
        tags::OCCUPATION,
        tags::ADDITIONAL_PATIENT_HISTORY,
        tags::PATIENT_COMMENTS,
        tags::REQUESTING_PHYSICIAN,
        tags::NAME_OF_PHYSICIANS_READING_STUDY,
    ];
    for t in strip_tags {
        obj.remove_element(t);
    }

    if opts.clear_dates {
        replace_str(&mut obj, tags::PATIENT_BIRTH_DATE, VR::DA, "19000101");
        replace_str(&mut obj, tags::STUDY_DATE, VR::DA, "19000101");
        replace_str(&mut obj, tags::SERIES_DATE, VR::DA, "19000101");
        replace_str(&mut obj, tags::ACQUISITION_DATE, VR::DA, "19000101");
        replace_str(&mut obj, tags::CONTENT_DATE, VR::DA, "19000101");
        replace_str(&mut obj, tags::STUDY_TIME, VR::TM, "120000");
        replace_str(&mut obj, tags::SERIES_TIME, VR::TM, "120000");
        replace_str(&mut obj, tags::ACQUISITION_TIME, VR::TM, "120000");
        replace_str(&mut obj, tags::CONTENT_TIME, VR::TM, "120000");
    }

    // 4. Remap Study/Series/SOP Instance UIDs consistently across the exported study/series.
    if opts.regenerate_uids {
        for (tag, update_meta) in [
            (tags::STUDY_INSTANCE_UID, false),
            (tags::SERIES_INSTANCE_UID, false),
            (tags::SOP_INSTANCE_UID, true),
            (tags::FRAME_OF_REFERENCE_UID, false),
        ] {
            if let Ok(el) = obj.element(tag) {
                if let Ok(old_uid) = el.to_str() {
                    let old_clean = old_uid.trim().to_string();
                    if !old_clean.is_empty() {
                        let new_uid = map_uid(uid_map, &old_clean, uid_counter);
                        obj.put(DataElement::new(tag, VR::UI, PrimitiveValue::from(new_uid.as_str())));
                        if update_meta {
                            obj.update_meta(|meta| {
                                meta.media_storage_sop_instance_uid = new_uid.clone();
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    obj.write_to_file(dst)
        .with_context(|| format!("write anonymized DICOM {}", dst.display()))?;
    Ok(())
}

/// De-identifies all unique DICOM files in `slices` into `out_dir`, preserving UID consistency.
pub fn anonymize_series(
    slices: &[SliceRef],
    out_dir: &Path,
    opts: &AnonymizeOptions,
) -> Result<usize> {
    std::fs::create_dir_all(out_dir)?;
    let mut uid_map = HashMap::new();
    let mut uid_counter = 0usize;
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut written = 0usize;

    for (idx, sl) in slices.iter().enumerate() {
        if !seen_paths.insert(sl.path.clone()) {
            continue;
        }
        let dst = out_dir.join(format!("anon_{:04}.dcm", idx + 1));
        anonymize_dicom_file(&sl.path, &dst, opts, &mut uid_map, &mut uid_counter)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_object::{FileDicomObject, InMemDicomObject};
    use dicom_object::mem::InMemElement;

    fn make_test_dicom(path: &Path) {
        let mut obj = InMemDicomObject::new_empty();
        obj.put(InMemElement::new(tags::PATIENT_NAME, VR::PN, PrimitiveValue::from("DOE^JANE")));
        obj.put(InMemElement::new(tags::PATIENT_ID, VR::LO, PrimitiveValue::from("123456")));
        obj.put(InMemElement::new(tags::PATIENT_BIRTH_DATE, VR::DA, PrimitiveValue::from("19850412")));
        obj.put(InMemElement::new(tags::PATIENT_ADDRESS, VR::LO, PrimitiveValue::from("123 Main St")));
        obj.put(InMemElement::new(tags::INSTITUTION_NAME, VR::LO, PrimitiveValue::from("General Hospital")));
        obj.put(InMemElement::new(tags::ACCESSION_NUMBER, VR::SH, PrimitiveValue::from("ACC999")));
        obj.put(InMemElement::new(tags::STUDY_INSTANCE_UID, VR::UI, PrimitiveValue::from("1.2.3.4.5")));
        obj.put(InMemElement::new(tags::SERIES_INSTANCE_UID, VR::UI, PrimitiveValue::from("1.2.3.4.6")));
        obj.put(InMemElement::new(tags::SOP_INSTANCE_UID, VR::UI, PrimitiveValue::from("1.2.3.4.7")));
        // Private tag (group 0x0009 is odd)
        obj.put(InMemElement::new(Tag(0x0009, 0x0010), VR::LO, PrimitiveValue::from("SECRET_VENDOR")));
        let file_obj: FileDicomObject<InMemDicomObject> = obj
            .with_meta(
                dicom_object::meta::FileMetaTableBuilder::new()
                    .transfer_syntax(dicom_dictionary_std::uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                    .media_storage_sop_instance_uid("1.2.3.4.7"),
            )
            .unwrap();
        file_obj.write_to_file(path).unwrap();
    }

    #[test]
    fn anonymize_dicom_strips_private_tags_and_patient_identifiers() {
        let dir = std::env::temp_dir().join(format!("sekhmetx-anon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("orig.dcm");
        let dst = dir.join("anon.dcm");
        make_test_dicom(&src);

        let mut uid_map = HashMap::new();
        let mut counter = 0usize;
        let opts = AnonymizeOptions::default();
        anonymize_dicom_file(&src, &dst, &opts, &mut uid_map, &mut counter).unwrap();

        let out = open_file(&dst).unwrap();
        assert_eq!(out.element(tags::PATIENT_NAME).unwrap().to_str().unwrap(), "ANONYMIZED");
        assert_eq!(out.element(tags::PATIENT_ID).unwrap().to_str().unwrap(), "ANON0001");
        assert_eq!(out.element(tags::PATIENT_BIRTH_DATE).unwrap().to_str().unwrap(), "19000101");
        assert_eq!(out.element(tags::INSTITUTION_NAME).unwrap().to_str().unwrap(), "ANONYMIZED");
        assert!(out.element(tags::PATIENT_ADDRESS).is_err());
        assert!(out.element(Tag(0x0009, 0x0010)).is_err());
        assert_ne!(
            out.element(tags::STUDY_INSTANCE_UID).unwrap().to_str().unwrap(),
            "1.2.3.4.5"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
