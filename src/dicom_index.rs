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
    pub files_missing: bool,
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

const MAX_ZIP_ENTRIES: usize = 20_000;
const MAX_ZIP_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn process_zip_cache_dir() -> PathBuf {
    std::env::temp_dir().join(format!("sekhmetx-zip-{}", std::process::id()))
}

/// Removes temporary directories created while opening `.zip` archives (current process + legacy/stale).
pub fn cleanup_zip_cache() {
    let _ = std::fs::remove_dir_all(process_zip_cache_dir());
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("rdicom-zip-cache"));
    let _ = std::fs::remove_dir_all(PathBuf::from(".rdicom-zip-cache"));
}

/// Cleans up stale `sekhmetx-zip-*` directories left behind by previous crashed runs (older than 24h)
/// as well as legacy `rdicom-zip-cache` directories.
pub fn cleanup_stale_zip_caches() {
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("rdicom-zip-cache"));
    let _ = std::fs::remove_dir_all(PathBuf::from(".rdicom-zip-cache"));
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let own_name = format!("sekhmetx-zip-{}", std::process::id());
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !name_str.starts_with("sekhmetx-zip-") || name_str == own_name {
            continue;
        }
        let is_old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age.as_secs() >= 86_400);
        if is_old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Extracts a `.zip` archive into a per-process temporary directory under `std::env::temp_dir()`
/// using streaming `zip::ZipArchive` with path-traversal and size/entry caps.
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
    let dest_dir = process_zip_cache_dir().join(format!("zip-{hash:016x}"));
    let marker = dest_dir.join(".extracted_ok");
    if marker.exists() {
        return Some(dest_dir);
    }
    std::fs::create_dir_all(&dest_dir).ok()?;

    if extract_zip_streaming(
        zip_path,
        &dest_dir,
        MAX_ZIP_ENTRIES,
        MAX_ZIP_ENTRY_BYTES,
        MAX_ZIP_TOTAL_BYTES,
    )
    .is_ok()
    {
        let _ = std::fs::write(&marker, b"ok");
        Some(dest_dir)
    } else {
        let _ = std::fs::remove_dir_all(&dest_dir);
        None
    }
}

fn extract_zip_streaming(
    zip_path: &Path,
    dest_dir: &Path,
    max_entries: usize,
    max_entry_bytes: u64,
    max_total_bytes: u64,
) -> anyhow::Result<()> {
    use std::io::{BufReader, Read};

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))?;
    if archive.len() > max_entries {
        anyhow::bail!("zip archive exceeds maximum entry count ({max_entries})");
    }

    let mut total_written: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(safe_rel) = entry.enclosed_name() else {
            anyhow::bail!("unsafe zip entry path rejected: {}", entry.name());
        };
        let rel_str = safe_rel.to_string_lossy();
        if rel_str.contains("__MACOSX") || rel_str.contains(".git") {
            continue;
        }
        if entry.size() > max_entry_bytes {
            anyhow::bail!("zip entry `{}` exceeds per-file size limit", entry.name());
        }
        if total_written.saturating_add(entry.size()) > max_total_bytes {
            anyhow::bail!("zip archive exceeds total extracted size limit");
        }

        let out_path = dest_dir.join(&safe_rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        let mut limited = entry.take(max_entry_bytes + 1);
        let copied = std::io::copy(&mut limited, &mut out)?;
        if copied > max_entry_bytes {
            anyhow::bail!("zip entry stream exceeded per-file size limit");
        }
        total_written = total_written.saturating_add(copied);
        if total_written > max_total_bytes {
            anyhow::bail!("zip archive stream exceeded total extracted size limit");
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
                    files_missing: false,
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

fn series_is_mip(desc: &str) -> bool {
    let lower = desc.to_ascii_lowercase();
    lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|tok| {
        matches!(tok, "mip" | "minip")
    })
}

/// Strips plane tokens (`ax`, `axial`, `cor`, `coronal`, `sag`, `sagittal`, `obl`, `mpr`)
/// and slice-thickness tokens (e.g. `1`, `0`, `mm`, `5.0mm`)
/// so we can tell whether two series are orthogonal planes of the same reconstruction.
fn series_base_name(desc: &str) -> String {
    desc.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|tok| {
            !tok.is_empty()
                && !tok.chars().all(|c| c.is_ascii_digit())
                && !tok
                    .strip_suffix("mm")
                    .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
                && !matches!(
                    *tok,
                    "ax" | "axial" | "cor" | "coronal" | "sag" | "sagittal" | "obl" | "mpr" | "mm" | "cm"
                )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns `true` when two series from the same acquisition represent compatible
/// reconstructions (same kernel / contrast / window class) and can safely share MPR reslicing.
fn series_compatible_for_mpr(a: &Series, b: &Series) -> bool {
    if series_is_mip(&a.description) != series_is_mip(&b.description) {
        return false;
    }
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
    // If either series is already a full standalone 3D stack (>= 40 slices), only group
    // it with another series if they share the same base reconstruction name (e.g. Brain AX/COR/SAG).
    if a.slices.len() >= 40 || b.slices.len() >= 40 {
        let ba = series_base_name(&a.description);
        let bb = series_base_name(&b.description);
        if !ba.is_empty() && !bb.is_empty() && ba != bb {
            return false;
        }
    }
    true
}

/// Automatically detects separated series (e.g. Axial, Coronal, and Sagittal reconstructions)
/// that originate from the same acquisition time / FrameOfReferenceUID and groups them into
/// unified MPR-enabled groups without mixing incompatible reconstruction kernels.
pub fn build_mpr_groups(series: &mut [Series], study_desc: &str) -> Vec<MprGroup> {
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

    #[test]
    fn angio_standalone_stack_never_merges_with_mip_reconstructions() {
        let mut series = vec![
            make_series(4, "Angio ABD", "CT", DominantPlane::Axial, 686, None, None),
            make_series(602, "MIP AX", "CT", DominantPlane::Axial, 94, None, None),
            make_series(603, "MIP COR", "CT", DominantPlane::Coronal, 53, None, None),
            make_series(604, "MIP SAG", "CT", DominantPlane::Sagittal, 73, None, None),
        ];
        let groups = build_mpr_groups(&mut series, "ANGIO_ABDOMINAL");
        assert_eq!(groups.len(), 2);
        // Angio ABD stays standalone with 686 images, never 906.
        assert_eq!(groups[0].series_indices, vec![0]);
        // MIP AX/COR/SAG group cleanly together.
        assert_eq!(groups[1].series_indices, vec![1, 2, 3]);
    }

    #[test]
    fn zip_extraction_rejects_traversal_enforces_caps_and_extracts_valid_entries() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let base = std::env::temp_dir().join(format!("sekhmetx-zip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // 1. Path traversal must be rejected.
        let trav_zip = base.join("traversal.zip");
        {
            let f = std::fs::File::create(&trav_zip).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            zw.start_file("../evil.dcm", SimpleFileOptions::default()).unwrap();
            zw.write_all(b"bad").unwrap();
            zw.finish().unwrap();
        }
        let out_trav = base.join("out_trav");
        assert!(extract_zip_streaming(&trav_zip, &out_trav, 100, 1024, 4096).is_err());

        // 2. Entry count and size caps must be enforced.
        let valid_zip = base.join("valid.zip");
        {
            let f = std::fs::File::create(&valid_zip).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            zw.start_file("study/s1.dcm", SimpleFileOptions::default()).unwrap();
            zw.write_all(b"DICM_DATA_1").unwrap();
            zw.start_file("study/s2.dcm", SimpleFileOptions::default()).unwrap();
            zw.write_all(b"DICM_DATA_2").unwrap();
            zw.finish().unwrap();
        }
        let out_cap = base.join("out_cap");
        assert!(extract_zip_streaming(&valid_zip, &out_cap, 1, 1024, 4096).is_err());
        assert!(extract_zip_streaming(&valid_zip, &out_cap, 100, 5, 4096).is_err());
        assert!(extract_zip_streaming(&valid_zip, &out_cap, 100, 1024, 15).is_err());

        // 3. Valid extraction succeeds.
        let out_ok = base.join("out_ok");
        extract_zip_streaming(&valid_zip, &out_ok, 100, 1024, 4096).unwrap();
        assert_eq!(std::fs::read(out_ok.join("study/s1.dcm")).unwrap(), b"DICM_DATA_1");
        assert_eq!(std::fs::read(out_ok.join("study/s2.dcm")).unwrap(), b"DICM_DATA_2");

        let _ = std::fs::remove_dir_all(&base);
    }
}
