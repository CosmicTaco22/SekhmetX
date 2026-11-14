//! Scans folders for DICOM files and groups them into patients/studies/series.
//!
//! Only the header is read here (everything before Pixel Data), so indexing
//! thousands of files is fast. Pixel data is decoded later, on demand.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::{DefaultDicomObject, OpenFileOptions};
use rayon::prelude::*;

use crate::geometry::{ImagePlaneCoordinates, ImagePlaneGeometry};

/// Dominant anatomical plane of a series computed from its 3D slice normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DominantPlane {
    Axial,
    Coronal,
    Sagittal,
    Oblique,
    Unknown,
}

impl DominantPlane {
    pub fn from_normal(normal: Option<[f64; 3]>, slice_count: usize) -> Self {
        let Some([nx, ny, nz]) = normal else {
            return Self::Unknown;
        };
        if slice_count <= 2 {
            return Self::Unknown;
        }
        let (ax, ay, az) = (nx.abs(), ny.abs(), nz.abs());
        if az >= ax && az >= ay && az > 0.65 {
            Self::Axial
        } else if ay >= ax && ay >= az && ay > 0.65 {
            Self::Coronal
        } else if ax >= ay && ax >= az && ax > 0.65 {
            Self::Sagittal
        } else {
            Self::Oblique
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Axial => "AX",
            Self::Coronal => "COR",
            Self::Sagittal => "SAG",
            Self::Oblique => "OBL",
            Self::Unknown => "2D",
        }
    }
}

/// A group of series acquired together (e.g. separated Axial, Coronal, and Sagittal
/// reconstructions from the same acquisition) that form a unified MPR-enabled dataset.
#[derive(Debug, Clone)]
pub struct MprGroup {
    pub id: usize,
    pub label: String,
    pub planes_badge: String,
    pub series_indices: Vec<usize>,
    pub primary_series: usize,
    pub axial_series: Option<usize>,
    pub coronal_series: Option<usize>,
    pub sagittal_series: Option<usize>,
}

/// One displayable image: a file, plus a frame number for multi-frame files.
#[derive(Debug, Clone)]
pub struct SliceRef {
    pub path: PathBuf,
    pub frame: u32,
    pub instance: i64,
    pub position: Option<[f64; 3]>,
    pub orientation: [f64; 9],
    pub is_3d_plane: bool,
    pub spacing: Option<(f64, f64)>,
    pub slice_location: Option<f64>,
    #[allow(dead_code)]
    pub frame_of_reference_uid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Series {
    #[allow(dead_code)]
    pub uid: String,
    pub number: i64,
    pub description: String,
    pub modality: String,
    pub rows: u32,
    pub columns: u32,
    /// Slice normal (row × column direction), when known.
    pub normal: Option<[f64; 3]>,
    pub dominant_plane: DominantPlane,
    pub frame_of_reference_uid: Option<String>,
    pub acquisition_time_secs: Option<f64>,
    pub acquisition_number: Option<i64>,
    pub convolution_kernel: Option<String>,
    pub window_center: Option<f64>,
    pub window_width: Option<f64>,
    pub mpr_group_id: Option<usize>,
    pub slices: Vec<SliceRef>,
}

impl Series {
    pub fn is_mpr_capable(&self) -> bool {
        (self.slices.len() >= 6 && self.normal.is_some()) || self.mpr_group_id.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct Study {
    pub uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub birth_date: String,
    pub date: String,
    pub description: String,
    pub accession: String,
    pub series: Vec<Series>,
    pub mpr_groups: Vec<MprGroup>,
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    pub studies: Vec<Study>,
    pub files_seen: usize,
    pub files_skipped: usize,
}

impl Index {
    pub fn image_count(&self) -> usize {
        self.studies
            .iter()
            .flat_map(|s| &s.series)
            .map(|s| s.slices.len())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Attribute helpers (shared with the frame loader)
// ---------------------------------------------------------------------------

pub fn get_str(obj: &DefaultDicomObject, tag: Tag) -> Option<String> {
    let e = obj.element_opt(tag).ok().flatten()?;
    let s = e.to_str().ok()?;
    let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!s.is_empty()).then(|| s.to_string())
}

pub fn get_f64s(obj: &DefaultDicomObject, tag: Tag) -> Option<Vec<f64>> {
    let e = obj.element_opt(tag).ok().flatten()?;
    let v = e.to_multi_float64().ok()?;
    (!v.is_empty()).then_some(v)
}

pub fn get_f64(obj: &DefaultDicomObject, tag: Tag) -> Option<f64> {
    get_f64s(obj, tag).and_then(|v| v.first().copied())
}

pub fn get_int(obj: &DefaultDicomObject, tag: Tag) -> Option<i64> {
    let e = obj.element_opt(tag).ok().flatten()?;
    e.to_int::<i64>()
        .ok()
        .or_else(|| e.to_float64().ok().map(|f| f as i64))
}

/// "DOE^JOHN" -> "DOE, JOHN"
pub fn format_person_name(raw: &str) -> String {
    let parts: Vec<&str> = raw
        .split('^')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        _ => format!("{}, {}", parts[0], parts[1..].join(" ")),
    }
}

/// "20240131" -> "2024-01-31"
pub fn format_date(raw: &str) -> String {
    let d: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 8 {
        format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
    } else {
        raw.to_string()
    }
}

/// Parses a DICOM `TM` string (`"HHMMSS.ffffff"`) into seconds since midnight.
pub fn parse_dicom_time_secs(raw: &str) -> Option<f64> {
    let s = raw.trim();
    if s.len() < 4 {
        return None;
    }
    let hh: f64 = s.get(0..2)?.parse().ok()?;
    let mm: f64 = s.get(2..4)?.parse().ok()?;
    let ss: f64 = if s.len() >= 6 {
        s.get(4..)?.parse().unwrap_or(0.0)
    } else {
        0.0
    };
    Some(hh * 3600.0 + mm * 60.0 + ss)
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

struct Header {
    path: PathBuf,
    patient_name: String,
    patient_id: String,
    birth_date: String,
    study_uid: String,
    study_date: String,
    study_desc: String,
    accession: String,
    series_uid: String,
    series_number: i64,
    series_desc: String,
    modality: String,
    rows: u32,
    columns: u32,
    frames: u32,
    instance: i64,
    position: Option<[f64; 3]>,
    orientation: Option<[f64; 6]>,
    spacing: Option<(f64, f64)>,
    slice_location: Option<f64>,
    frame_of_reference_uid: Option<String>,
    acquisition_time_secs: Option<f64>,
    acquisition_number: Option<i64>,
    convolution_kernel: Option<String>,
    window_center: Option<f64>,
    window_width: Option<f64>,
}

const NON_IMAGE_MODALITIES: &[&str] = &[
    "SR", "PR", "KO", "REG", "SEG", "RTSTRUCT", "RTPLAN", "RTDOSE", "DOC",
];

fn read_header(path: &Path) -> Option<Header> {
    let obj = OpenFileOptions::new()
        .read_until(tags::PIXEL_DATA)
        .open_file(path)
        .ok()?;

    let rows = get_int(&obj, tags::ROWS)? as u32;
    let columns = get_int(&obj, tags::COLUMNS)? as u32;
    if rows == 0 || columns == 0 {
        return None;
    }
    let modality = get_str(&obj, tags::MODALITY).unwrap_or_else(|| "OT".into());
    if NON_IMAGE_MODALITIES.contains(&modality.as_str()) {
        return None;
    }

    let position = get_f64s(&obj, tags::IMAGE_POSITION_PATIENT)
        .filter(|v| v.len() >= 3)
        .map(|v| [v[0], v[1], v[2]]);
    let orientation = get_f64s(&obj, tags::IMAGE_ORIENTATION_PATIENT)
        .filter(|v| v.len() >= 6)
        .map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]]);
    let spacing = get_f64s(&obj, tags::PIXEL_SPACING)
        .or_else(|| get_f64s(&obj, tags::IMAGER_PIXEL_SPACING))
        .filter(|v| v.len() >= 2 && v[0] > 0.0 && v[1] > 0.0)
        .map(|v| {
            let settled = ImagePlaneGeometry::settled_pixel_spacing(
                v[1],
                v[0],
                rows as i32,
                columns as i32,
                false,
            );
            (settled.y, settled.x)
        });

    Some(Header {
        path: path.to_path_buf(),
        patient_name: get_str(&obj, tags::PATIENT_NAME)
            .map(|s| format_person_name(&s))
            .unwrap_or_else(|| "(no name)".into()),
        patient_id: get_str(&obj, tags::PATIENT_ID).unwrap_or_default(),
        birth_date: get_str(&obj, tags::PATIENT_BIRTH_DATE)
            .map(|s| format_date(&s))
            .unwrap_or_default(),
        study_uid: get_str(&obj, tags::STUDY_INSTANCE_UID)
            .unwrap_or_else(|| "unknown-study".into()),
        study_date: get_str(&obj, tags::STUDY_DATE)
            .map(|s| format_date(&s))
            .unwrap_or_default(),
        study_desc: get_str(&obj, tags::STUDY_DESCRIPTION).unwrap_or_default(),
        accession: get_str(&obj, tags::ACCESSION_NUMBER).unwrap_or_default(),
        series_uid: get_str(&obj, tags::SERIES_INSTANCE_UID)
            .unwrap_or_else(|| "unknown-series".into()),
        series_number: get_int(&obj, tags::SERIES_NUMBER).unwrap_or(0),
        series_desc: get_str(&obj, tags::SERIES_DESCRIPTION).unwrap_or_default(),
        modality,
        rows,
        columns,
        frames: get_int(&obj, tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as u32,
        instance: get_int(&obj, tags::INSTANCE_NUMBER).unwrap_or(0),
        position,
        orientation,
        spacing,
        slice_location: get_f64(&obj, tags::SLICE_LOCATION),
        frame_of_reference_uid: get_str(&obj, tags::FRAME_OF_REFERENCE_UID),
        acquisition_time_secs: get_str(&obj, tags::ACQUISITION_TIME)
            .or_else(|| get_str(&obj, tags::CONTENT_TIME))
            .or_else(|| get_str(&obj, tags::SERIES_TIME))
            .and_then(|s| parse_dicom_time_secs(&s)),
        acquisition_number: get_int(&obj, tags::ACQUISITION_NUMBER),
        convolution_kernel: get_str(&obj, tags::CONVOLUTION_KERNEL),
        window_center: get_f64s(&obj, tags::WINDOW_CENTER).and_then(|v| v.first().copied()),
        window_width: get_f64s(&obj, tags::WINDOW_WIDTH).and_then(|v| v.first().copied()),
    })
}

/// Collects candidate files from a mix of files, folders, and `.zip` archives.
pub fn collect_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for entry in walkdir::WalkDir::new(input).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.into_path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.starts_with('.') || name.eq_ignore_ascii_case("DICOMDIR") {
                    continue;
                }
                if is_zip_file(&path) {
                    if let Some(dir) = extract_zip_to_temp(&path) {
                        out.extend(collect_files(&[dir]));
                    }
                } else {
                    out.push(path);
                }
            }
        } else if input.is_file() {
            if is_zip_file(input) {
                if let Some(dir) = extract_zip_to_temp(input) {
                    out.extend(collect_files(&[dir]));
                }
            } else {
                out.push(input.clone());
            }
        }
    }
    out
}

fn is_zip_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

/// Removes temporary directories created while opening `.zip` archives.
pub fn cleanup_zip_cache() {
    let primary = std::env::temp_dir().join("rdicom-zip-cache");
    let _ = std::fs::remove_dir_all(primary);
    let fallback = PathBuf::from(".rdicom-zip-cache");
    let _ = std::fs::remove_dir_all(fallback);
}

/// Extracts a `.zip` archive into a deterministic temporary directory under `std::env::temp_dir()`
/// so all extracted DICOM slices can be indexed and decoded on demand.
fn extract_zip_to_temp(zip_path: &Path) -> Option<PathBuf> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let meta = std::fs::metadata(zip_path).ok()?;
    let mut hasher = DefaultHasher::new();
    zip_path.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    if let Ok(modified) = meta.modified() {
        modified.hash(&mut hasher);
    }
    let hash = hasher.finish();
    let sub = format!("zip-{hash:016x}");
    let primary = std::env::temp_dir().join("rdicom-zip-cache").join(&sub);
    let dest_dir = if std::fs::create_dir_all(&primary).is_ok() {
        primary
    } else {
        let fallback = PathBuf::from(".rdicom-zip-cache").join(&sub);
        std::fs::create_dir_all(&fallback).ok()?;
        fallback
    };
    let marker = dest_dir.join(".extracted_ok");
    if marker.exists() {
        return Some(dest_dir);
    }

    let data = std::fs::read(zip_path).ok()?;
    // Parse ZIP local file headers or End of Central Directory (EOCD).
    if extract_zip_bytes(&data, &dest_dir).is_ok() {
        let _ = std::fs::write(&marker, b"ok");
        return Some(dest_dir);
    }

    // Fallback to system `tar -xf` (available on Windows 10/11, macOS, and Linux) for ZIP64/unusual archives.
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(zip_path)
        .arg("-C")
        .arg(&dest_dir)
        .status()
        .ok()?;
    if status.success() {
        let _ = std::fs::write(&marker, b"ok");
        return Some(dest_dir);
    }
    None
}

fn extract_zip_bytes(data: &[u8], dest_dir: &Path) -> anyhow::Result<()> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    // Locate End of Central Directory (PK\x05\x06) scanning backwards from the end.
    let min_eocd = 22usize;
    if data.len() < min_eocd {
        anyhow::bail!("zip too short");
    }
    let search_start = data.len().saturating_sub(65557);
    let mut eocd_pos = None;
    for i in (search_start..=data.len() - min_eocd).rev() {
        if &data[i..i + 4] == b"PK\x05\x06" {
            eocd_pos = Some(i);
            break;
        }
    }
    let eocd = eocd_pos.ok_or_else(|| anyhow::anyhow!("EOCD not found"))?;
    let total_entries = u16::from_le_bytes([data[eocd + 10], data[eocd + 11]]) as usize;
    let cd_size = u32::from_le_bytes([
        data[eocd + 12],
        data[eocd + 13],
        data[eocd + 14],
        data[eocd + 15],
    ]) as usize;
    let cd_offset = u32::from_le_bytes([
        data[eocd + 16],
        data[eocd + 17],
        data[eocd + 18],
        data[eocd + 19],
    ]) as usize;
    if cd_offset == 0xFFFF_FFFF || cd_size == 0xFFFF_FFFF {
        anyhow::bail!("ZIP64 requires tar fallback");
    }
    if cd_offset.saturating_add(cd_size) > data.len() {
        anyhow::bail!("invalid central directory bounds");
    }

    let mut pos = cd_offset;
    for _ in 0..total_entries {
        if pos + 46 > data.len() || &data[pos..pos + 4] != b"PK\x01\x02" {
            anyhow::bail!("invalid central directory entry");
        }
        let method = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
        let comp_size = u32::from_le_bytes([
            data[pos + 20],
            data[pos + 21],
            data[pos + 22],
            data[pos + 23],
        ]) as usize;
        let uncomp_size = u32::from_le_bytes([
            data[pos + 24],
            data[pos + 25],
            data[pos + 26],
            data[pos + 27],
        ]) as usize;
        let name_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([data[pos + 30], data[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([data[pos + 32], data[pos + 33]]) as usize;
        let lfh_offset = u32::from_le_bytes([
            data[pos + 42],
            data[pos + 43],
            data[pos + 44],
            data[pos + 45],
        ]) as usize;

        let name_bytes = &data[pos + 46..pos + 46 + name_len];
        let raw_name = String::from_utf8_lossy(name_bytes);
        pos += 46 + name_len + extra_len + comment_len;

        if raw_name.ends_with('/')
            || raw_name.ends_with('\\')
            || raw_name.contains("__MACOSX")
            || raw_name.contains("/.git/")
            || raw_name.contains("\\.git\\")
        {
            continue;
        }

        // Sanitize relative path components to prevent path traversal.
        let mut rel = PathBuf::new();
        for part in raw_name.split(['/', '\\']) {
            if part.is_empty() || part == "." || part == ".." || part.contains(':') {
                continue;
            }
            rel.push(part);
        }
        if rel.as_os_str().is_empty() {
            continue;
        }

        if lfh_offset + 30 > data.len() || &data[lfh_offset..lfh_offset + 4] != b"PK\x03\x04" {
            anyhow::bail!("invalid local file header");
        }
        let lfh_name_len =
            u16::from_le_bytes([data[lfh_offset + 26], data[lfh_offset + 27]]) as usize;
        let lfh_extra_len =
            u16::from_le_bytes([data[lfh_offset + 28], data[lfh_offset + 29]]) as usize;
        let payload_start = lfh_offset + 30 + lfh_name_len + lfh_extra_len;
        let payload_end = payload_start.saturating_add(comp_size);
        if payload_end > data.len() {
            anyhow::bail!("truncated zip entry");
        }
        let payload = &data[payload_start..payload_end];

        let out_path = dest_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match method {
            0 => {
                // Stored (uncompressed)
                std::fs::write(&out_path, payload)?;
            }
            8 => {
                // Deflate
                let mut decoder = DeflateDecoder::new(payload);
                let mut out_buf = Vec::with_capacity(uncomp_size);
                decoder.read_to_end(&mut out_buf)?;
                std::fs::write(&out_path, &out_buf)?;
            }
            _ => anyhow::bail!("unsupported zip compression method {method}"),
        }
    }
    Ok(())
}

/// Scans the given files and folders. `done`/`total` let the UI show progress.
pub fn scan(inputs: &[PathBuf], done: &AtomicUsize, total: &AtomicUsize) -> Index {
    let files = collect_files(inputs);
    total.store(files.len(), Ordering::Relaxed);

    let headers: Vec<Header> = files
        .par_iter()
        .filter_map(|p| {
            let h = read_header(p);
            done.fetch_add(1, Ordering::Relaxed);
            h
        })
        .collect();

    let files_seen = files.len();
    let files_skipped = files_seen - headers.len();
    let mut index = build_index(headers);
    index.files_seen = files_seen;
    index.files_skipped = files_skipped;
    index
}

fn build_index(headers: Vec<Header>) -> Index {
    // study uid -> (study info, series key -> series)
    let mut studies: HashMap<String, (Study, HashMap<String, Series>)> = HashMap::new();

    for h in headers {
        let (study, series_map) = studies.entry(h.study_uid.clone()).or_insert_with(|| {
            (
                Study {
                    uid: h.study_uid.clone(),
                    patient_name: h.patient_name.clone(),
                    patient_id: h.patient_id.clone(),
                    birth_date: h.birth_date.clone(),
                    date: h.study_date.clone(),
                    description: h.study_desc.clone(),
                    accession: h.accession.clone(),
                    series: Vec::new(),
                    mpr_groups: Vec::new(),
                },
                HashMap::new(),
            )
        });
        if study.description.is_empty() && !h.study_desc.is_empty() {
            study.description = h.study_desc.clone();
        }

        // Split a series by image size and orientation so that, for example,
        // a localizer stored in the same series doesn't get mixed into the stack.
        let orient_key = h
            .orientation
            .map(|o| {
                o.iter()
                    .map(|v| format!("{:.2}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let key = format!("{}|{}x{}|{}", h.series_uid, h.rows, h.columns, orient_key);

        let (norm_orientation, is_3d_plane) = match h.orientation {
            Some(raw) => {
                let norm = ImagePlaneCoordinates::normalized_orientation(&raw);
                let has_plane = h.position.is_some()
                    && (norm[6] != 0.0 || norm[7] != 0.0 || norm[8] != 0.0);
                (norm, has_plane)
            }
            None => (ImagePlaneGeometry::DEFAULT_ORIENTATION, false),
        };
        let normal = is_3d_plane.then_some([
            norm_orientation[6],
            norm_orientation[7],
            norm_orientation[8],
        ]);
        let computed_slice_loc = h.slice_location.or_else(|| {
            h.position
                .and_then(|pos| ImagePlaneGeometry::slice_location(pos, &norm_orientation))
        });

        let series = series_map.entry(key).or_insert_with(|| Series {
            uid: h.series_uid.clone(),
            number: h.series_number,
            description: h.series_desc.clone(),
            modality: h.modality.clone(),
            rows: h.rows,
            columns: h.columns,
            normal,
            dominant_plane: DominantPlane::Unknown,
            frame_of_reference_uid: h.frame_of_reference_uid.clone(),
            acquisition_time_secs: h.acquisition_time_secs,
            acquisition_number: h.acquisition_number,
            convolution_kernel: h.convolution_kernel.clone(),
            window_center: h.window_center,
            window_width: h.window_width,
            mpr_group_id: None,
            slices: Vec::new(),
        });
        if series.frame_of_reference_uid.is_none() && h.frame_of_reference_uid.is_some() {
            series.frame_of_reference_uid = h.frame_of_reference_uid.clone();
        }
        if series.acquisition_time_secs.is_none() && h.acquisition_time_secs.is_some() {
            series.acquisition_time_secs = h.acquisition_time_secs;
        }
        if series.acquisition_number.is_none() && h.acquisition_number.is_some() {
            series.acquisition_number = h.acquisition_number;
        }
        if series.convolution_kernel.is_none() && h.convolution_kernel.is_some() {
            series.convolution_kernel = h.convolution_kernel.clone();
        }
        if series.window_center.is_none() && h.window_center.is_some() {
            series.window_center = h.window_center;
        }
        if series.window_width.is_none() && h.window_width.is_some() {
            series.window_width = h.window_width;
        }

        for frame in 0..h.frames {
            series.slices.push(SliceRef {
                path: h.path.clone(),
                frame,
                instance: h.instance,
                position: h.position,
                orientation: norm_orientation,
                is_3d_plane,
                spacing: h.spacing,
                slice_location: computed_slice_loc,
                frame_of_reference_uid: h.frame_of_reference_uid.clone(),
            });
        }
    }

    let mut out: Vec<Study> = studies
        .into_values()
        .map(|(mut study, series_map)| {
            let mut series: Vec<Series> = series_map.into_values().collect();
            for s in &mut series {
                sort_slices(s);
                s.dominant_plane = DominantPlane::from_normal(s.normal, s.slices.len());
            }
            series.sort_by(|a, b| {
                a.number
                    .cmp(&b.number)
                    .then_with(|| b.slices.len().cmp(&a.slices.len()))
            });
            // Disambiguate split series that share a description.
            let mut seen: HashMap<(i64, String), usize> = HashMap::new();
            for s in &mut series {
                let n = seen
                    .entry((s.number, s.description.clone()))
                    .or_insert(0);
                *n += 1;
                if *n > 1 {
                    s.description = format!("{} ({})", s.description, n);
                }
            }
            let mpr_groups = build_mpr_groups(&mut series, &study.description);
            study.series = series;
            study.mpr_groups = mpr_groups;
            study
        })
        .collect();

    out.sort_by(|a, b| {
        a.patient_name
            .cmp(&b.patient_name)
            .then_with(|| b.date.cmp(&a.date))
    });

    Index {
        studies: out,
        ..Default::default()
    }
}

/// Orders slices by instance number, then by position along the slice normal,
/// then by frame number.
fn sort_slices(series: &mut Series) {
    let normal = series.normal;
    let key = |s: &SliceRef| -> f64 {
        match (s.position, normal) {
            (Some(p), Some(n)) => dot(p, n),
            _ => s.slice_location.unwrap_or(0.0),
        }
    };
    series.slices.sort_by(|a, b| {
        a.instance
            .cmp(&b.instance)
            .then_with(|| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.frame.cmp(&b.frame))
    });
}

/// Classifies a series description into a coarse reconstruction/kernel/sequence family
/// so incompatible reconstructions (e.g. CT Brain soft-tissue vs Bone kernel, or MR T1 vs T2)
/// are never merged into the same MPR group.
fn series_kernel_class(desc: &str) -> Option<&'static str> {
    let lower = desc.to_ascii_lowercase();
    let has_any = |kws: &[&str]| kws.iter().any(|k| lower.contains(k));

    if has_any(&[
        "bone", "osso", "osseo", "ósseo", "osteo", "b60", "b70", "b80", "h60", "h70", "u70",
        "hrct", "sharp",
    ]) {
        return Some("bone");
    }
    if has_any(&["lung", "pulm"]) {
        return Some("lung");
    }
    if has_any(&[
        "brain",
        "cereb",
        "cranio",
        "crânio",
        "soft",
        "stnd",
        "std",
        "standard",
        "mediast",
        "parenchym",
        "parenquima",
        "partes moles",
    ]) {
        return Some("soft");
    }
    if has_any(&["flair"]) {
        return Some("flair");
    }
    if has_any(&["dwi", "diff"]) {
        return Some("dwi");
    }
    if has_any(&["adc"]) {
        return Some("adc");
    }
    if has_any(&["swi"]) {
        return Some("swi");
    }
    if has_any(&["stir"]) {
        return Some("stir");
    }
    if has_any(&["t2"]) {
        return Some("t2");
    }
    if has_any(&["t1", "mprage", "bravo", "spgr"]) {
        return Some("t1");
    }
    None
}

/// Returns `true` when two series from the same acquisition represent compatible
/// reconstructions (same kernel / contrast / window class) and can safely share MPR reslicing.
fn series_compatible_for_mpr(a: &Series, b: &Series) -> bool {
    if let (Some(ka), Some(kb)) = (&a.convolution_kernel, &b.convolution_kernel) {
        if !ka.trim().eq_ignore_ascii_case(kb.trim()) {
            return false;
        }
    }
    if let (Some(wla), Some(wwa), Some(wlb), Some(wwb)) =
        (a.window_center, a.window_width, b.window_center, b.window_width)
    {
        if (wla - wlb).abs() > 100.0 {
            return false;
        }
        let min_ww = wwa.abs().min(wwb.abs()).max(1.0);
        let max_ww = wwa.abs().max(wwb.abs());
        if max_ww / min_ww > 2.0 {
            return false;
        }
    }
    match (series_kernel_class(&a.description), series_kernel_class(&b.description)) {
        (Some(ca), Some(cb)) if ca != cb => return false,
        (Some("bone"), None) | (None, Some("bone")) => return false,
        (Some("lung"), None) | (None, Some("lung")) => return false,
        _ => {}
    }
    true
}

/// Automatically detects separated series (e.g. Axial, Coronal, and Sagittal reconstructions)
/// that originate from the same acquisition time / FrameOfReferenceUID and groups them into
/// unified MPR-enabled groups without mixing incompatible reconstruction kernels.
fn build_mpr_groups(series: &mut [Series], study_desc: &str) -> Vec<MprGroup> {
    let n = series.len();
    let mut visited = vec![false; n];
    let mut groups = Vec::new();

    for i in 0..n {
        if visited[i] || series[i].slices.len() < 5 || series[i].normal.is_none() {
            continue;
        }
        let mut cluster = vec![i];
        visited[i] = true;

        for j in (i + 1)..n {
            if visited[j] || series[j].slices.len() < 5 || series[j].normal.is_none() {
                continue;
            }
            if series[i].modality != series[j].modality {
                continue;
            }
            if !series_compatible_for_mpr(&series[i], &series[j]) {
                continue;
            }
            let same_for = match (&series[i].frame_of_reference_uid, &series[j].frame_of_reference_uid) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                _ => false,
            };
            if !same_for {
                continue;
            }
            // Match by acquisition time (within 180 seconds) or acquisition number,
            // or if times are absent/identical, group orthogonal planes in the same FOR.
            let time_match = match (series[i].acquisition_time_secs, series[j].acquisition_time_secs) {
                (Some(ta), Some(tb)) => (ta - tb).abs() <= 180.0,
                _ => true,
            };
            let acq_num_match = match (series[i].acquisition_number, series[j].acquisition_number) {
                (Some(na), Some(nb)) if na > 0 && nb > 0 => na == nb,
                _ => false,
            };
            if time_match || acq_num_match {
                cluster.push(j);
                visited[j] = true;
            }
        }

        // Check if this cluster spans multiple anatomical planes (e.g. AX + COR + SAG)
        // or is a multi-series acquisition.
        let mut axial_best: Option<usize> = None;
        let mut coronal_best: Option<usize> = None;
        let mut sagittal_best: Option<usize> = None;
        for &idx in &cluster {
            match series[idx].dominant_plane {
                DominantPlane::Axial => {
                    if axial_best.is_none_or(|b| series[idx].slices.len() > series[b].slices.len()) {
                        axial_best = Some(idx);
                    }
                }
                DominantPlane::Coronal => {
                    if coronal_best.is_none_or(|b| series[idx].slices.len() > series[b].slices.len()) {
                        coronal_best = Some(idx);
                    }
                }
                DominantPlane::Sagittal => {
                    if sagittal_best.is_none_or(|b| series[idx].slices.len() > series[b].slices.len()) {
                        sagittal_best = Some(idx);
                    }
                }
                _ => {}
            }
        }

        let plane_count = usize::from(axial_best.is_some())
            + usize::from(coronal_best.is_some())
            + usize::from(sagittal_best.is_some());

        if plane_count >= 2 || (cluster.len() == 1 && series[cluster[0]].slices.len() >= 12) {
            let group_id = groups.len();
            for &idx in &cluster {
                series[idx].mpr_group_id = Some(group_id);
            }
            let primary_series = axial_best
                .or(coronal_best)
                .or(sagittal_best)
                .unwrap_or_else(|| {
                    cluster
                        .iter()
                        .copied()
                        .max_by_key(|&idx| series[idx].slices.len())
                        .unwrap_or(cluster[0])
                });

            let mut planes = Vec::new();
            if axial_best.is_some() {
                planes.push("AX");
            }
            if coronal_best.is_some() {
                planes.push("COR");
            }
            if sagittal_best.is_some() {
                planes.push("SAG");
            }
            if planes.is_empty() {
                planes.push("3D");
            }
            let planes_badge = planes.join("·");

            let base_desc = if !series[primary_series].description.is_empty() {
                series[primary_series].description.clone()
            } else if !study_desc.is_empty() {
                study_desc.to_string()
            } else {
                format!("{} Acquisition {}", series[primary_series].modality, group_id + 1)
            };

            groups.push(MprGroup {
                id: group_id,
                label: base_desc,
                planes_badge,
                series_indices: cluster,
                primary_series,
                axial_series: axial_best,
                coronal_series: coronal_best,
                sagittal_series: sagittal_best,
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_series(
        number: i64,
        description: &str,
        modality: &str,
        plane: DominantPlane,
        slice_count: usize,
        kernel: Option<&str>,
        wl_ww: Option<(f64, f64)>,
    ) -> Series {
        let normal = match plane {
            DominantPlane::Axial => Some([0.0, 0.0, 1.0]),
            DominantPlane::Coronal => Some([0.0, 1.0, 0.0]),
            DominantPlane::Sagittal => Some([1.0, 0.0, 0.0]),
            _ => Some([0.0, 0.0, 1.0]),
        };
        let slices = (0..slice_count)
            .map(|i| SliceRef {
                path: PathBuf::from(format!("slice_{number}_{i}.dcm")),
                frame: 0,
                instance: i as i64 + 1,
                position: Some([0.0, 0.0, i as f64]),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                spacing: Some((0.5, 0.5)),
                slice_location: Some(i as f64),
                frame_of_reference_uid: Some("1.2.3.4".into()),
            })
            .collect();
        Series {
            uid: format!("1.2.3.{number}"),
            number,
            description: description.into(),
            modality: modality.into(),
            rows: 512,
            columns: 512,
            normal,
            dominant_plane: plane,
            frame_of_reference_uid: Some("1.2.3.4".into()),
            acquisition_time_secs: Some(36000.0),
            acquisition_number: Some(1),
            convolution_kernel: kernel.map(str::to_string),
            window_center: wl_ww.map(|(wl, _)| wl),
            window_width: wl_ww.map(|(_, ww)| ww),
            mpr_group_id: None,
            slices,
        }
    }

    #[test]
    fn brain_and_bone_kernels_never_merge_into_same_mpr_group() {
        let mut series = vec![
            make_series(
                1,
                "Brain 5.0mm Ax",
                "CT",
                DominantPlane::Axial,
                32,
                Some("STANDARD"),
                Some((40.0, 80.0)),
            ),
            make_series(
                2,
                "Bone 1.0mm Ax",
                "CT",
                DominantPlane::Axial,
                160,
                Some("BONE"),
                Some((500.0, 2500.0)),
            ),
            make_series(
                3,
                "Bone Coronal MPR",
                "CT",
                DominantPlane::Coronal,
                120,
                Some("BONE"),
                Some((500.0, 2500.0)),
            ),
            make_series(
                4,
                "Bone Sagittal MPR",
                "CT",
                DominantPlane::Sagittal,
                120,
                Some("BONE"),
                Some((500.0, 2500.0)),
            ),
        ];

        let groups = build_mpr_groups(&mut series, "HEAD CT");
        assert_eq!(groups.len(), 2);
        // Brain stays in its own group and is never mixed with Bone.
        assert_eq!(groups[0].series_indices, vec![0]);
        assert_eq!(groups[0].primary_series, 0);
        // Bone AX+COR+SAG are grouped together with Bone Axial as primary.
        assert_eq!(groups[1].series_indices, vec![1, 2, 3]);
        assert_eq!(groups[1].primary_series, 1);
    }

    #[test]
    fn same_kernel_orthogonal_series_group_with_axial_primary() {
        let mut series = vec![
            make_series(
                1,
                "Brain Coronal",
                "CT",
                DominantPlane::Coronal,
                90,
                Some("H30s"),
                Some((40.0, 80.0)),
            ),
            make_series(
                2,
                "Brain Axial",
                "CT",
                DominantPlane::Axial,
                40,
                Some("H30s"),
                Some((40.0, 80.0)),
            ),
            make_series(
                3,
                "Brain Sagittal",
                "CT",
                DominantPlane::Sagittal,
                90,
                Some("H30s"),
                Some((40.0, 80.0)),
            ),
        ];

        let groups = build_mpr_groups(&mut series, "HEAD CT");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].series_indices, vec![0, 1, 2]);
        assert_eq!(groups[0].primary_series, 1);
    }

    #[test]
    fn mr_t1_and_t2_never_merge_into_same_mpr_group() {
        let mut series = vec![
            make_series(1, "T1 AX", "MR", DominantPlane::Axial, 24, None, None),
            make_series(2, "T2 COR", "MR", DominantPlane::Coronal, 24, None, None),
        ];
        let groups = build_mpr_groups(&mut series, "BRAIN MR");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].series_indices, vec![0]);
        assert_eq!(groups[1].series_indices, vec![1]);
    }
}
