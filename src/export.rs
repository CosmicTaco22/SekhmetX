use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dicom_core::value::Value as DicomValue;
use dicom_core::{DataElement, PrimitiveValue, Tag, VR};
use dicom_dictionary_std::tags;
use dicom_object::{open_file, InMemDicomObject};

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

/// Exports `slices` as an animated GIF cine movie (`anonymize-export`), downscaling
/// frames to at most `512` px per side so files stay compact and responsive.
pub fn export_series_gif(
    slices: &[SliceRef],
    window: Option<Window>,
    invert: bool,
    clut_name: &str,
    conv_idx: Option<usize>,
    fps: f32,
    out_path: &Path,
) -> Result<usize> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame as GifFrame, RgbaImage};

    let file = std::fs::File::create(out_path)
        .with_context(|| format!("failed to create GIF {}", out_path.display()))?;
    let mut encoder = GifEncoder::new_with_speed(file, 10);
    encoder.set_repeat(Repeat::Infinite)?;

    let delay_ms = (1000.0 / fps.clamp(1.0, 60.0)).round() as u32;
    let clut = ColourTableBook::table_named(clut_name);
    let mut count = 0usize;

    for sref in slices {
        let Ok(raw_frame) = load_frame(&sref.path, sref.frame) else {
            continue;
        };
        let display_frame = conv_idx
            .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
            .map(|k| raw_frame.with_convolution(&k.to_convolution()));
        let src = display_frame.as_ref().unwrap_or(&raw_frame);
        let w = window.unwrap_or_else(|| src.default_window());
        let rgba = src.render_with_clut(w, invert != src.inverts, clut.as_ref());
        let Some(img) = RgbaImage::from_raw(src.width, src.height, rgba) else {
            continue;
        };
        let resized = if src.width > 512 || src.height > 512 {
            let scale = 512.0 / src.width.max(src.height) as f32;
            let nw = ((src.width as f32 * scale).round() as u32).max(1);
            let nh = ((src.height as f32 * scale).round() as u32).max(1);
            image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        let frame = GifFrame::from_parts(resized, 0, 0, Delay::from_numer_denom_ms(delay_ms, 1));
        encoder.encode_frame(frame)?;
        count += 1;
    }
    Ok(count)
}

/// Writes an uncompressed 24-bit BGR RIFF AVI movie (zero external codecs/tools needed).
pub fn write_avi_rgb24(out_path: &Path, width: u32, height: u32, fps: u32, frames_rgba: &[Vec<u8>]) -> Result<()> {
    use std::io::Write;
    let fps = fps.clamp(1, 60);
    let n_frames = frames_rgba.len() as u32;
    if n_frames == 0 || width == 0 || height == 0 {
        anyhow::bail!("empty video");
    }
    let row_stride = (width * 3).div_ceil(4) * 4;
    let frame_bytes = row_stride * height;
    let usec_per_frame = 1_000_000 / fps;

    // Build hdrl LIST
    let mut avih = Vec::with_capacity(56);
    avih.extend_from_slice(&usec_per_frame.to_le_bytes());
    avih.extend_from_slice(&(frame_bytes * fps).to_le_bytes()); // max bytes/sec
    avih.extend_from_slice(&0u32.to_le_bytes());
    avih.extend_from_slice(&0x10u32.to_le_bytes()); // AVIF_HASINDEX
    avih.extend_from_slice(&n_frames.to_le_bytes());
    avih.extend_from_slice(&0u32.to_le_bytes());
    avih.extend_from_slice(&1u32.to_le_bytes()); // 1 stream
    avih.extend_from_slice(&frame_bytes.to_le_bytes());
    avih.extend_from_slice(&width.to_le_bytes());
    avih.extend_from_slice(&height.to_le_bytes());
    avih.extend_from_slice(&[0u8; 16]);

    let mut strh = Vec::with_capacity(56);
    strh.extend_from_slice(b"vids");
    strh.extend_from_slice(b"DIB ");
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&1u32.to_le_bytes()); // scale
    strh.extend_from_slice(&fps.to_le_bytes()); // rate
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&n_frames.to_le_bytes());
    strh.extend_from_slice(&frame_bytes.to_le_bytes());
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&0u32.to_le_bytes());
    strh.extend_from_slice(&[0u8; 8]); // rcFrame

    let mut strf = Vec::with_capacity(40);
    strf.extend_from_slice(&40u32.to_le_bytes());
    strf.extend_from_slice(&(width as i32).to_le_bytes());
    strf.extend_from_slice(&(height as i32).to_le_bytes());
    strf.extend_from_slice(&1u16.to_le_bytes()); // planes
    strf.extend_from_slice(&24u16.to_le_bytes()); // bpp
    strf.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    strf.extend_from_slice(&frame_bytes.to_le_bytes());
    strf.extend_from_slice(&[0u8; 16]);

    let mut strl = Vec::new();
    strl.extend_from_slice(b"strl");
    strl.extend_from_slice(b"strh");
    strl.extend_from_slice(&(strh.len() as u32).to_le_bytes());
    strl.extend_from_slice(&strh);
    strl.extend_from_slice(b"strf");
    strl.extend_from_slice(&(strf.len() as u32).to_le_bytes());
    strl.extend_from_slice(&strf);

    let mut hdrl = Vec::new();
    hdrl.extend_from_slice(b"hdrl");
    hdrl.extend_from_slice(b"avih");
    hdrl.extend_from_slice(&(avih.len() as u32).to_le_bytes());
    hdrl.extend_from_slice(&avih);
    hdrl.extend_from_slice(b"LIST");
    hdrl.extend_from_slice(&(strl.len() as u32).to_le_bytes());
    hdrl.extend_from_slice(&strl);

    let movi_data_len = 4 + n_frames * (8 + frame_bytes);
    let idx1_len = n_frames * 16;
    let riff_len = 4 + (8 + hdrl.len() as u32) + (8 + movi_data_len) + (8 + idx1_len);

    let mut f = std::fs::File::create(out_path)
        .with_context(|| format!("failed to create AVI {}", out_path.display()))?;
    f.write_all(b"RIFF")?;
    f.write_all(&riff_len.to_le_bytes())?;
    f.write_all(b"AVI ")?;
    f.write_all(b"LIST")?;
    f.write_all(&(hdrl.len() as u32).to_le_bytes())?;
    f.write_all(&hdrl)?;
    f.write_all(b"LIST")?;
    f.write_all(&movi_data_len.to_le_bytes())?;
    f.write_all(b"movi")?;

    let mut bgr_buf = vec![0u8; frame_bytes as usize];
    for rgba in frames_rgba {
        bgr_buf.fill(0);
        for y in 0..height as usize {
            let src_y = (height as usize - 1) - y; // BMP/AVI bottom-up
            for x in 0..width as usize {
                let s = (src_y * width as usize + x) * 4;
                let d = y * row_stride as usize + x * 3;
                if s + 2 < rgba.len() && d + 2 < bgr_buf.len() {
                    bgr_buf[d] = rgba[s + 2];
                    bgr_buf[d + 1] = rgba[s + 1];
                    bgr_buf[d + 2] = rgba[s];
                }
            }
        }
        f.write_all(b"00db")?;
        f.write_all(&frame_bytes.to_le_bytes())?;
        f.write_all(&bgr_buf)?;
    }

    f.write_all(b"idx1")?;
    f.write_all(&idx1_len.to_le_bytes())?;
    let mut offset = 4u32;
    for _ in 0..n_frames {
        f.write_all(b"00db")?;
        f.write_all(&0x10u32.to_le_bytes())?; // AVIIF_KEYFRAME
        f.write_all(&offset.to_le_bytes())?;
        f.write_all(&frame_bytes.to_le_bytes())?;
        offset += 8 + frame_bytes;
    }
    Ok(())
}

/// Exports `slices` as an `.mp4` (via `ffmpeg` if installed on PATH) or pure-Rust `.avi` movie.
pub fn export_series_video(
    slices: &[SliceRef],
    window: Option<Window>,
    invert: bool,
    clut_name: &str,
    conv_idx: Option<usize>,
    fps: f32,
    out_path: &Path,
) -> Result<usize> {
    use image::RgbaImage;
    let clut = ColourTableBook::table_named(clut_name);
    let mut frames = Vec::new();
    let mut dims: Option<(u32, u32)> = None;

    for sref in slices {
        let Ok(raw_frame) = load_frame(&sref.path, sref.frame) else {
            continue;
        };
        let display_frame = conv_idx
            .and_then(|i| FACTORY_CONVOLUTIONS.get(i))
            .map(|k| raw_frame.with_convolution(&k.to_convolution()));
        let src = display_frame.as_ref().unwrap_or(&raw_frame);
        let w = window.unwrap_or_else(|| src.default_window());
        let rgba = src.render_with_clut(w, invert != src.inverts, clut.as_ref());
        let Some(img) = RgbaImage::from_raw(src.width, src.height, rgba) else {
            continue;
        };
        let (tw, th) = if let Some(d) = dims {
            d
        } else {
            let max_side = 512.0;
            let (nw, nh) = if src.width > 512 || src.height > 512 {
                let scale = max_side / src.width.max(src.height) as f32;
                (
                    (((src.width as f32 * scale).round() as u32) & !1).max(2),
                    (((src.height as f32 * scale).round() as u32) & !1).max(2),
                )
            } else {
                ((src.width & !1).max(2), (src.height & !1).max(2))
            };
            dims = Some((nw, nh));
            (nw, nh)
        };
        let resized = if img.width() != tw || img.height() != th {
            image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        frames.push(resized.into_raw());
    }
    let Some((tw, th)) = dims else {
        anyhow::bail!("no decodable frames to export");
    };
    let fps_u32 = fps.clamp(1.0, 60.0).round() as u32;
    let is_mp4 = out_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    if is_mp4 {
        let tmp = tempfile::TempDir::new()?;
        let avi_tmp = tmp.path().join("tmp.avi");
        write_avi_rgb24(&avi_tmp, tw, th, fps_u32, &frames)?;
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &avi_tmp.to_string_lossy(),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                &out_path.to_string_lossy(),
            ])
            .status();
        match status {
            Ok(s) if s.success() => return Ok(frames.len()),
            _ => {
                // Fallback: save as .avi beside the requested path so export never fails without ffmpeg
                let fallback = out_path.with_extension("avi");
                write_avi_rgb24(&fallback, tw, th, fps_u32, &frames)?;
                return Ok(frames.len());
            }
        }
    }
    write_avi_rgb24(out_path, tw, th, fps_u32, &frames)?;
    Ok(frames.len())
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

/// Returns `true` if the DICOM file may contain burned-in patient text in pixel data
/// (e.g. ultrasound, secondary capture, or `BurnedInAnnotation = YES`).
#[allow(dead_code)]
pub fn has_likely_burned_in_text(src: &Path) -> bool {
    let Ok(obj) = open_file(src) else {
        return false;
    };
    if obj
        .element(tags::BURNED_IN_ANNOTATION)
        .ok()
        .and_then(|e| e.to_str().ok())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("YES"))
    {
        return true;
    }
    if obj
        .element(tags::MODALITY)
        .ok()
        .and_then(|e| e.to_str().ok())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("US"))
    {
        return true;
    }
    if obj
        .element(tags::SOP_CLASS_UID)
        .ok()
        .and_then(|e| e.to_str().ok())
        .is_some_and(|v| v.trim().starts_with("1.2.840.10008.5.1.4.1.1.7"))
    {
        return true;
    }
    false
}

#[allow(deprecated)]
fn anonymize_dataset(
    ds: &mut InMemDicomObject,
    opts: &AnonymizeOptions,
    uid_map: &mut HashMap<String, String>,
    uid_counter: &mut usize,
    is_root: bool,
) {
    // 1. Remove private tags (odd group numbers) and overlay groups (0x6000..=0x60FF).
    let to_remove: Vec<Tag> = ds
        .tags()
        .filter(|t| {
            (opts.strip_private_tags && (t.group() & 1) == 1)
                || (t.group() & 0xFF00) == 0x6000
        })
        .collect();
    for t in to_remove {
        ds.remove_element(t);
    }

    // 2. Replace core patient / institution / physician identifiers.
    let replace_if_present = |obj: &mut InMemDicomObject, tag: Tag, vr: VR, val: &str| {
        if obj.element(tag).is_ok() {
            obj.put(DataElement::new(tag, vr, PrimitiveValue::from(val)));
        }
    };

    if is_root || ds.element(tags::PATIENT_NAME).is_ok() {
        ds.put(DataElement::new(
            tags::PATIENT_NAME,
            VR::PN,
            PrimitiveValue::from(opts.patient_name.as_str()),
        ));
    }
    if is_root || ds.element(tags::PATIENT_ID).is_ok() {
        ds.put(DataElement::new(
            tags::PATIENT_ID,
            VR::LO,
            PrimitiveValue::from(opts.patient_id.as_str()),
        ));
    }

    replace_if_present(ds, tags::OTHER_PATIENT_I_DS, VR::LO, &opts.patient_id);
    replace_if_present(ds, tags::ACCESSION_NUMBER, VR::SH, "");
    replace_if_present(ds, tags::INSTITUTION_NAME, VR::LO, "ANONYMIZED");
    replace_if_present(ds, tags::REFERRING_PHYSICIAN_NAME, VR::PN, "");
    replace_if_present(ds, tags::PERFORMING_PHYSICIAN_NAME, VR::PN, "");
    replace_if_present(ds, tags::OPERATORS_NAME, VR::PN, "");
    replace_if_present(ds, tags::STUDY_ID, VR::SH, "ANON");

    // 3. Remove DICOM PS3.15 Annex E Basic Profile identifying / free-text attributes.
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
        tags::PHYSICIANS_OF_RECORD,
        tags::DEVICE_SERIAL_NUMBER,
        tags::STATION_NAME,
        tags::INSTITUTIONAL_DEPARTMENT_NAME,
        tags::PROTOCOL_NAME,
        tags::STUDY_DESCRIPTION,
        tags::SERIES_DESCRIPTION,
        tags::PERFORMED_PROCEDURE_STEP_DESCRIPTION,
        tags::REQUESTED_PROCEDURE_DESCRIPTION,
        tags::PATIENT_AGE,
        tags::PATIENT_SIZE,
        tags::PATIENT_WEIGHT,
        tags::PATIENT_SEX,
        tags::COUNTRY_OF_RESIDENCE,
        tags::REGION_OF_RESIDENCE,
        tags::PERSON_NAME,
        tags::PERSON_TELEPHONE_NUMBERS,
        tags::PERSON_ADDRESS,
    ];
    for t in strip_tags {
        ds.remove_element(t);
    }

    if opts.clear_dates {
        for (t, vr, v) in [
            (tags::PATIENT_BIRTH_DATE, VR::DA, "19000101"),
            (tags::STUDY_DATE, VR::DA, "19000101"),
            (tags::SERIES_DATE, VR::DA, "19000101"),
            (tags::ACQUISITION_DATE, VR::DA, "19000101"),
            (tags::CONTENT_DATE, VR::DA, "19000101"),
            (tags::STUDY_TIME, VR::TM, "120000"),
            (tags::SERIES_TIME, VR::TM, "120000"),
            (tags::ACQUISITION_TIME, VR::TM, "120000"),
            (tags::CONTENT_TIME, VR::TM, "120000"),
        ] {
            replace_if_present(ds, t, vr, v);
        }
    }

    // 4. Remap all non-standard UID elements (keeps 1.2.840.10008.* SOP Class / Transfer Syntax UIDs).
    if opts.regenerate_uids {
        let ui_tags: Vec<(Tag, String)> = ds
            .tags()
            .filter_map(|t| {
                let el = ds.element(t).ok()?;
                if el.vr() != VR::UI {
                    return None;
                }
                let s = el.to_str().ok()?.trim().to_string();
                if s.is_empty() || s.starts_with("1.2.840.10008.") {
                    return None;
                }
                Some((t, s))
            })
            .collect();
        for (t, old_uid) in ui_tags {
            let new_uid = map_uid(uid_map, &old_uid, uid_counter);
            ds.put(DataElement::new(t, VR::UI, PrimitiveValue::from(new_uid.as_str())));
        }
    }

    // 5. Walk all nested sequences recursively.
    let seq_tags: Vec<Tag> = ds
        .tags()
        .filter(|&t| {
            ds.element(t)
                .ok()
                .is_some_and(|el| matches!(el.value(), DicomValue::Sequence(_)))
        })
        .collect();
    for t in seq_tags {
        if let Ok(el) = ds.take_element(t) {
            let vr = el.vr();
            if let DicomValue::Sequence(mut seq) = el.into_value() {
                for item in seq.items_mut() {
                    anonymize_dataset(item, opts, uid_map, uid_counter, false);
                }
                ds.put(DataElement::new(t, vr, DicomValue::Sequence(seq)));
            }
        }
    }

    if is_root {
        ds.put(DataElement::new(
            tags::PATIENT_IDENTITY_REMOVED,
            VR::CS,
            PrimitiveValue::from("YES"),
        ));
        ds.put(DataElement::new(
            tags::DEIDENTIFICATION_METHOD,
            VR::LO,
            PrimitiveValue::from("DICOM PS3.15 Annex E Basic Profile"),
        ));
    }
}

/// De-identifies a single DICOM file following DICOM PS3.15 Annex E Basic Profile and writes it to `dst`.
#[allow(deprecated)]
pub fn anonymize_dicom_file(
    src: &Path,
    dst: &Path,
    opts: &AnonymizeOptions,
    uid_map: &mut HashMap<String, String>,
    uid_counter: &mut usize,
) -> Result<()> {
    let obj = open_file(src).with_context(|| format!("open DICOM {}", src.display()))?;
    let meta = obj.meta().clone();
    let mut ds = obj.into_inner();
    anonymize_dataset(&mut ds, opts, uid_map, uid_counter, true);

    let sop_instance_uid = if opts.regenerate_uids {
        ds.element(tags::SOP_INSTANCE_UID)
            .ok()
            .and_then(|el| el.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| meta.media_storage_sop_instance_uid.clone())
    } else {
        meta.media_storage_sop_instance_uid.clone()
    };
    let meta_builder = dicom_object::FileMetaTableBuilder::new()
        .transfer_syntax(&meta.transfer_syntax)
        .media_storage_sop_class_uid(&meta.media_storage_sop_class_uid)
        .media_storage_sop_instance_uid(&sop_instance_uid);
    let out_obj = ds.with_meta(meta_builder)?;

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    out_obj
        .write_to_file(dst)
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
        assert_eq!(
            out.element(tags::PATIENT_IDENTITY_REMOVED).unwrap().to_str().unwrap(),
            "YES"
        );
        assert!(out.element(tags::DEIDENTIFICATION_METHOD).is_ok());
        assert_ne!(
            out.element(tags::STUDY_INSTANCE_UID).unwrap().to_str().unwrap(),
            "1.2.3.4.5"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn anonymize_dicom_strips_sequence_names_and_remaps_sequence_uids() {
        use dicom_core::value::DataSetSequence;
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("seq_orig.dcm");
        let dst = dir.path().join("seq_anon.dcm");

        let mut item = InMemDicomObject::new_empty();
        item.put(InMemElement::new(tags::PATIENT_NAME, VR::PN, PrimitiveValue::from("SECRET^PATIENT")));
        item.put(InMemElement::new(tags::PERSON_NAME, VR::PN, PrimitiveValue::from("DR^SECRET")));
        item.put(InMemElement::new(tags::REFERENCED_SOP_INSTANCE_UID, VR::UI, PrimitiveValue::from("1.2.3.4.7")));
        item.put(InMemElement::new(Tag(0x0011, 0x0010), VR::LO, PrimitiveValue::from("NESTED_PRIVATE")));

        let mut obj = InMemDicomObject::new_empty();
        obj.put(InMemElement::new(tags::PATIENT_NAME, VR::PN, PrimitiveValue::from("DOE^JANE")));
        obj.put(InMemElement::new(tags::PATIENT_ID, VR::LO, PrimitiveValue::from("123456")));
        obj.put(InMemElement::new(tags::SOP_INSTANCE_UID, VR::UI, PrimitiveValue::from("1.2.3.4.7")));
        obj.put(DataElement::new(
            tags::REFERENCED_IMAGE_SEQUENCE,
            VR::SQ,
            DicomValue::Sequence(DataSetSequence::from(vec![item])),
        ));
        let file_obj: FileDicomObject<InMemDicomObject> = obj
            .with_meta(
                dicom_object::meta::FileMetaTableBuilder::new()
                    .transfer_syntax(dicom_dictionary_std::uids::EXPLICIT_VR_LITTLE_ENDIAN)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                    .media_storage_sop_instance_uid("1.2.3.4.7"),
            )
            .unwrap();
        file_obj.write_to_file(&src).unwrap();

        let mut uid_map = HashMap::new();
        let mut counter = 0usize;
        anonymize_dicom_file(&src, &dst, &AnonymizeOptions::default(), &mut uid_map, &mut counter).unwrap();

        let out = open_file(&dst).unwrap();
        let root_sop = out.element(tags::SOP_INSTANCE_UID).unwrap().to_str().unwrap().to_string();
        assert_ne!(root_sop, "1.2.3.4.7");
        let seq = out.element(tags::REFERENCED_IMAGE_SEQUENCE).unwrap().items().unwrap();
        assert_eq!(seq.len(), 1);
        let nested = &seq[0];
        assert_eq!(nested.element(tags::PATIENT_NAME).unwrap().to_str().unwrap(), "ANONYMIZED");
        assert!(nested.element(tags::PERSON_NAME).is_err());
        assert!(nested.element(Tag(0x0011, 0x0010)).is_err());
        assert_eq!(
            nested.element(tags::REFERENCED_SOP_INSTANCE_UID).unwrap().to_str().unwrap(),
            root_sop
        );
    }

    #[test]
    fn write_avi_rgb24_produces_valid_riff_header() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("cine.avi");
        let f0 = vec![128u8; 4 * 4 * 4];
        let f1 = vec![200u8; 4 * 4 * 4];
        write_avi_rgb24(&path, 4, 4, 15, &[f0, f1]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"AVI ");
        assert!(bytes.windows(4).any(|w| w == b"movi"));
        assert!(bytes.windows(4).any(|w| w == b"idx1"));
    }
}
