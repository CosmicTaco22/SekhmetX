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

/// Largest gap between slices (mm) for a series to count as a complete volume that
/// shows axial, coronal and sagittal planes without losing detail.
pub const FULL_VOLUME_MAX_GAP_MM: f64 = 2.0;
/// Fewest slices for a complete volume.
pub const FULL_VOLUME_MIN_SLICES: usize = 30;

impl Series {
    pub fn is_mpr_capable(&self) -> bool {
        (self.slices.len() >= 6 && self.normal.is_some()) || self.mpr_group_id.is_some()
    }

    /// Typical distance between neighbouring slices along the slice normal, mm
    /// (the median gap, so one missing slice does not change it).
    pub fn slice_gap_mm(&self) -> Option<f64> {
        let n = self.normal?;
        let mut d: Vec<f64> = self
            .slices
            .iter()
            .filter_map(|s| s.position)
            .map(|p| p[0] * n[0] + p[1] * n[1] + p[2] * n[2])
            .collect();
        if d.len() < 3 {
            return None;
        }
        d.sort_by(|a, b| a.total_cmp(b));
        let mut gaps: Vec<f64> = d.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 1e-3).collect();
        if gaps.is_empty() {
            return None;
        }
        gaps.sort_by(|a, b| a.total_cmp(b));
        Some(gaps[gaps.len() / 2])
    }

    /// A complete volume: enough slices, close enough together (at most 2 mm, and
    /// not more than four pixels apart), to show any plane without losing detail.
    pub fn is_full_volume(&self) -> bool {
        if self.slices.len() < FULL_VOLUME_MIN_SLICES {
            return false;
        }
        let Some(gap) = self.slice_gap_mm() else {
            return false;
        };
        let pixel = self
            .slices
            .first()
            .and_then(|s| s.spacing)
            .map_or(1.0, |(r, c)| r.min(c));
        gap <= FULL_VOLUME_MAX_GAP_MM + 0.05 && gap <= 4.0 * pixel + 0.05
    }

    /// The badge shown on the series: `MPR` for a complete volume, otherwise its
    /// plane (`AX`, `COR`, `SAG`, `OBL`, `2D`).
    pub fn cut_badge(&self) -> &'static str {
        if self.is_full_volume() {
            "MPR"
        } else {
            self.dominant_plane.badge()
        }
    }
}

/// Normalised series name for matching siblings: lower case, trimmed, without the
/// " (2)" added to split series that share a name.
fn series_name_key(desc: &str) -> String {
    let d = desc.trim();
    let d = match d.rfind(" (") {
        Some(i) if d.ends_with(')') && d[i + 2..d.len() - 1].chars().all(|c| c.is_ascii_digit()) => &d[..i],
        _ => d,
    };
    d.to_lowercase()
}

/// For a series whose images carry an orientation but no position (so it cannot be
/// placed in 3D, e.g. a straightened or curved reconstruction saved as pictures), the
/// plane is read from the orientation alone; it stays out of 3D logic (`normal`
/// stays `None`).
pub fn refine_plane(s: &mut Series) {
    if s.dominant_plane != DominantPlane::Unknown || s.normal.is_some() || s.slices.len() < 3 {
        return;
    }
    let o = s.slices[0].orientation;
    if o == crate::geometry::image_plane::ImagePlaneGeometry::DEFAULT_ORIENTATION {
        return; // no orientation recorded
    }
    s.dominant_plane = DominantPlane::from_normal(Some([o[6], o[7], o[8]]), s.slices.len());
}

/// Dose reports, protocol pages, scouts and other single pictures that belong in
/// "Single images / Scouts" rather than among the image series.
pub fn is_auxiliary(s: &Series) -> bool {
    if s.slices.len() <= 1 {
        return true;
    }
    if matches!(s.modality.as_str(), "SR" | "OT" | "PR" | "KO" | "DOC") {
        return true;
    }
    // Pictures that cannot be placed and have no pixel size (dose pages), or a few
    // pictures numbered 900 and above (where scanners put dose reports).
    if s.slices.len() <= 5
        && s.normal.is_none()
        && (s.slices.first().is_none_or(|sl| sl.spacing.is_none()) || s.number >= 900)
    {
        return true;
    }
    let d = s.description.to_lowercase();
    let words = [
        "dose", "dosis", "protocol", "report", "informe", "topogram", "topograma", "scout",
        "localizer", "localizador", "surview", "scanogram",
    ];
    s.slices.len() <= 5 && words.iter().any(|w| d.contains(w))
}

/// Runs of neighbouring series (in series-number order, scouts and reports left out)
/// that carry the same name and modality. Scanners write each block of
/// reconstructions in turn (thoracic soft-tissue AX, SAG, COR; then bone; then the
/// lumbar ones), so a run is one block even when another block has the same name.
fn name_runs(series: &[Series]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..series.len()).filter(|&i| !is_auxiliary(&series[i])).collect();
    order.sort_by_key(|&i| series[i].number);
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for i in order {
        let key = series_name_key(&series[i].description);
        let joins = runs.last().is_some_and(|run| {
            let last = &series[*run.last().unwrap_or(&i)];
            !key.is_empty() && last.modality == series[i].modality && series_name_key(&last.description) == key
        });
        if joins {
            if let Some(run) = runs.last_mut() {
                run.push(i);
            }
        } else {
            runs.push(vec![i]);
        }
    }
    runs
}

/// A series with no recorded plane in a run that already has the other two planes
/// (e.g. AX and SAG) is the missing one (COR): the plane is set for its badge; it
/// still cannot be placed in 3D.
pub fn infer_missing_planes(series: &mut [Series]) {
    for run in name_runs(series) {
        let unknown: Vec<usize> = run
            .iter()
            .copied()
            .filter(|&i| series[i].normal.is_none() && series[i].dominant_plane == DominantPlane::Unknown && series[i].slices.len() >= 3)
            .collect();
        if unknown.len() != 1 {
            continue;
        }
        let planes = [DominantPlane::Axial, DominantPlane::Coronal, DominantPlane::Sagittal];
        let missing: Vec<DominantPlane> = planes
            .into_iter()
            .filter(|p| !run.iter().any(|&i| series[i].dominant_plane == *p))
            .collect();
        if missing.len() == 1 {
            series[unknown[0]].dominant_plane = missing[0];
        }
    }
}

/// The badge for series `ri` of a study. A complete volume shows `MPR`; when the
/// same reconstruction also exists as a complete volume in another plane (same MPR
/// group or same name), the plane is added: `MPR-A`, `MPR-C`, `MPR-S`.
pub fn study_cut_badge(series: &[Series], ri: usize) -> &'static str {
    let s = &series[ri];
    if !s.is_full_volume() {
        return s.cut_badge();
    }
    let key = series_name_key(&s.description);
    let twin = series.iter().enumerate().any(|(j, o)| {
        j != ri
            && o.dominant_plane != s.dominant_plane
            && o.modality == s.modality
            && ((s.mpr_group_id.is_some() && o.mpr_group_id == s.mpr_group_id)
                || series_name_key(&o.description) == key)
            && o.is_full_volume()
    });
    if !twin {
        return "MPR";
    }
    match s.dominant_plane {
        DominantPlane::Axial => "MPR-A",
        DominantPlane::Coronal => "MPR-C",
        DominantPlane::Sagittal => "MPR-S",
        _ => "MPR",
    }
}

/// Reconstructions made from a complete volume (for example separate coronal and
/// sagittal series, or thicker axial ones): for each complete volume, the series
/// that lie inside its space, are not complete volumes themselves, have the same
/// modality and frame of reference, and were not acquired at another moment (a
/// separate contrast phase). When several volumes qualify, the one with the same
/// kernel (soft tissue, or sharp for lung and bone), the closest acquisition time
/// and the most similar name wins. Returns `(volume, reconstructions)` pairs.
pub fn find_reconstructions(series: &[Series]) -> Vec<(usize, Vec<usize>)> {
    let volumes: Vec<usize> = (0..series.len()).filter(|&i| series[i].is_full_volume()).collect();
    if volumes.is_empty() {
        return Vec::new();
    }
    let boxes: Vec<Option<([f64; 3], [f64; 3])>> = volumes.iter().map(|&v| series_box(&series[v])).collect();
    // Lung and bone reconstructions both use sharp kernels (Siemens B70 serves both).
    let kernel = |s: &Series| {
        s.convolution_kernel
            .as_deref()
            .and_then(series_kernel_class)
            .or_else(|| series_kernel_class(&s.description))
            .map(|k| if matches!(k, "bone" | "lung") { "sharp" } else { k })
    };
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    for (c, child) in series.iter().enumerate() {
        if child.is_full_volume() || child.slices.len() < 3 || child.normal.is_none() {
            continue;
        }
        let mut best: Option<(f64, usize)> = None;
        for (k, &v) in volumes.iter().enumerate() {
            let vol = &series[v];
            if vol.modality != child.modality {
                continue;
            }
            match (&vol.frame_of_reference_uid, &child.frame_of_reference_uid) {
                (Some(a), Some(b)) if a != b => continue,
                _ => {}
            }
            let dt = match (vol.acquisition_time_secs, child.acquisition_time_secs) {
                (Some(a), Some(b)) => Some((a - b).abs()),
                _ => None,
            };
            if dt.is_some_and(|dt| dt > 20.0) {
                continue; // another moment: a separate phase
            }
            let Some((lo, hi)) = boxes[k] else {
                continue;
            };
            if inside_fraction(child, lo, hi, 15.0) < 0.8 {
                continue;
            }
            let mut score = 0.0;
            match (kernel(vol), kernel(child)) {
                (Some(a), Some(b)) if a == b => score += 4.0,
                (Some(_), Some(_)) => score -= 2.0,
                _ => {}
            }
            if let Some(dt) = dt {
                score += 3.0 - dt / 10.0;
            }
            if series_base_name(&vol.description) == series_base_name(&child.description)
                && !series_base_name(&vol.description).is_empty()
            {
                score += 1.0;
            }
            score += vol.slices.len() as f64 * 1e-6;
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, v));
            }
        }
        if let Some((_, v)) = best {
            match out.iter_mut().find(|(p, _)| *p == v) {
                Some((_, kids)) => kids.push(c),
                None => out.push((v, vec![c])),
            }
        }
    }
    // Series that cannot be placed in 3D (no image position: straightened or curved
    // reconstructions saved as pictures) join their own block: the run of
    // neighbouring same-named series (`name_runs`). They go under the volume that
    // block belongs to (a complete volume in the run, or the volume a run member is
    // filed under); a block with no volume keeps them under its first series that can
    // be placed. Name alone is not enough: thoracic and lumbar blocks share names.
    for run in name_runs(series) {
        for &c in &run {
            let child = &series[c];
            if child.normal.is_some() || child.slices.len() < 3 {
                continue;
            }
            if out.iter().any(|(_, kids)| kids.contains(&c)) {
                continue;
            }
            let parent = run
                .iter()
                .copied()
                .find(|&r| series[r].is_full_volume())
                .or_else(|| {
                    out.iter()
                        .find(|(_, kids)| kids.iter().any(|k| *k != c && run.contains(k)))
                        .map(|(p, _)| *p)
                })
                .or_else(|| run.iter().copied().find(|&r| r != c && series[r].normal.is_some()));
            // Cross-check with the acquisition time: a block's pictures keep the time
            // of the acquisition they were made from. A volume of another moment is
            // wrong; then take the volume acquired at the same moment (same kernel
            // family first).
            let at = |v: usize| match (series[v].acquisition_time_secs, child.acquisition_time_secs) {
                (Some(a), Some(b)) => Some((a - b).abs()),
                _ => None,
            };
            let parent = match parent {
                Some(v) if series[v].is_full_volume() && at(v).is_some_and(|dt| dt > 20.0) => None,
                other => other,
            }
            .or_else(|| {
                let mut same_time: Vec<usize> = volumes
                    .iter()
                    .copied()
                    .filter(|&v| series[v].modality == child.modality && at(v).is_some_and(|dt| dt <= 2.0))
                    .collect();
                same_time.sort_by_key(|&v| (kernel(&series[v]) != kernel(child)) as u8);
                same_time.first().copied()
            });
            if let Some(v) = parent {
                match out.iter_mut().find(|(p, _)| *p == v) {
                    Some((_, kids)) => kids.push(c),
                    None => out.push((v, vec![c])),
                }
            }
        }
    }
    out
}

/// Corners of each slice image of `s`, in patient coordinates (LPS, mm).
fn slice_corners(s: &Series, slice: &SliceRef) -> Option<[[f64; 3]; 4]> {
    let p = slice.position?;
    let (rs, cs) = slice.spacing.unwrap_or((1.0, 1.0));
    let o = slice.orientation;
    let w = s.columns.max(1) as f64 - 1.0;
    let h = s.rows.max(1) as f64 - 1.0;
    let along = |base: [f64; 3], d: [f64; 3], len: f64| [base[0] + d[0] * len, base[1] + d[1] * len, base[2] + d[2] * len];
    let row = [o[0], o[1], o[2]];
    let col = [o[3], o[4], o[5]];
    let a = along(p, row, w * cs);
    Some([p, a, along(p, col, h * rs), along(a, col, h * rs)])
}

/// The box (minimum and maximum corner, LPS mm) holding every image of `s`.
fn series_box(s: &Series) -> Option<([f64; 3], [f64; 3])> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for slice in [s.slices.first()?, s.slices.last()?] {
        for c in slice_corners(s, slice)? {
            for k in 0..3 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
    }
    Some((lo, hi))
}

/// Share of `s`'s image centres that lie within the box `lo`…`hi` grown by `margin` mm.
fn inside_fraction(s: &Series, lo: [f64; 3], hi: [f64; 3], margin: f64) -> f64 {
    let mut n = 0usize;
    let mut inside = 0usize;
    for slice in &s.slices {
        let Some(c) = slice_corners(s, slice) else {
            continue;
        };
        let centre: [f64; 3] = std::array::from_fn(|k| 0.5 * (c[0][k] + c[3][k]));
        n += 1;
        if (0..3).all(|k| centre[k] >= lo[k] - margin && centre[k] <= hi[k] + margin) {
            inside += 1;
        }
    }
    if n == 0 { 0.0 } else { inside as f64 / n as f64 }
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
    /// `(complete volume, reconstructions made from it)`, from `find_reconstructions`.
    pub reformats: Vec<(usize, Vec<usize>)>,
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

fn zip_origins() -> &'static std::sync::Mutex<HashMap<PathBuf, PathBuf>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, PathBuf>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn record_zip_origin(dest_dir: &Path, zip_path: &Path) {
    if let Ok(mut guard) = zip_origins().lock() {
        guard.insert(dest_dir.to_path_buf(), zip_path.to_path_buf());
    }
}

/// If `path` was extracted from a `.zip` archive during this session, returns the original `.zip` path.
pub fn original_zip_for_extracted_path(path: &Path) -> Option<PathBuf> {
    let guard = zip_origins().lock().ok()?;
    for (dir, zip_path) in guard.iter() {
        if path.starts_with(dir) {
            return Some(zip_path.clone());
        }
    }
    None
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
        record_zip_origin(&dest_dir, zip_path);
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
        record_zip_origin(&dest_dir, zip_path);
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
        if entry.unix_mode().is_some_and(|m| (m & 0o170000) == 0o120000) {
            continue;
        }
        let entry_name = entry.name()?.into_owned();
        let Some(safe_rel) = entry.enclosed_name() else {
            anyhow::bail!("unsafe zip entry path rejected: {entry_name}");
        };
        let rel_str = safe_rel.to_string_lossy();
        if rel_str.contains("__MACOSX") || rel_str.contains(".git") {
            continue;
        }
        if entry.size() > max_entry_bytes {
            anyhow::bail!("zip entry `{entry_name}` exceeds per-file size limit");
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
                    reformats: Vec::new(),
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
                refine_plane(s);
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
            infer_missing_planes(&mut series);
            study.reformats = find_reconstructions(&series);
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
    // Canon / Toshiba kernel codes: FC01-FC29 body soft tissue, FC30-FC49 bone,
    // FC50-FC59 lung, FC80 and above sharp bone.
    if let Some(i) = lower.find("fc") {
        let digits: String = lower[i + 2..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let starts_word = i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric();
        if starts_word && let Ok(n) = digits.parse::<u32>() {
            return Some(match n {
                0..=29 => "soft",
                30..=49 | 80.. => "bone",
                _ => "lung",
            });
        }
    }
    let has_any = |kws: &[&str]| kws.iter().any(|k| lower.contains(k));

    if has_any(&[
        "bone", "osso", "osseo", "ósseo", "oseo", "óseo", "hueso", "osteo", "b60", "b70", "b80",
        "h60", "h70", "u70", "hrct", "sharp",
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
        "blando",
        "blandas",
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
    if let (Some(ka), Some(kb)) = (&a.convolution_kernel, &b.convolution_kernel)
        && !ka.trim().eq_ignore_ascii_case(kb.trim()) {
            return false;
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
            // Match by acquisition time (within 15 seconds) and acquisition number,
            // never merging separate contrast phases (e.g. arterial vs. venous) or same-plane
            // acquisitions taken more than 2 seconds apart.
            let time_delta = match (series[i].acquisition_time_secs, series[j].acquisition_time_secs) {
                (Some(ta), Some(tb)) => Some((ta - tb).abs()),
                _ => None,
            };
            let same_acq_num = match (series[i].acquisition_number, series[j].acquisition_number) {
                (Some(na), Some(nb)) if na > 0 && nb > 0 => na == nb,
                _ => false,
            };
            let diff_acq_num = match (series[i].acquisition_number, series[j].acquisition_number) {
                (Some(na), Some(nb)) if na > 0 && nb > 0 => na != nb,
                _ => false,
            };
            let same_plane = series[i].dominant_plane != DominantPlane::Unknown
                && series[i].dominant_plane == series[j].dominant_plane;
            let separate_phase = (same_plane
                && (diff_acq_num || time_delta.is_some_and(|dt| dt > 2.0)))
                || (diff_acq_num && time_delta.is_some_and(|dt| dt > 15.0));
            let should_merge = !separate_phase
                && if same_acq_num {
                    time_delta.is_none_or(|dt| dt <= 180.0)
                } else {
                    !diff_acq_num && time_delta.is_none_or(|dt| dt <= 90.0)
                };
            if should_merge {
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
                DominantPlane::Sagittal
                    if sagittal_best.is_none_or(|b| series[idx].slices.len() > series[b].slices.len()) => {
                        sagittal_best = Some(idx);
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

    /// A series with real geometry: 512×512 images of 0.78 mm pixels (400 mm across),
    /// `count` slices `gap` mm apart starting at `start` mm along the plane's normal.
    /// Axial slices start at the bottom; coronal and sagittal images hang down from
    /// height 0.
    #[allow(clippy::too_many_arguments)]
    fn placed_series(
        number: i64,
        description: &str,
        plane: DominantPlane,
        count: usize,
        gap: f64,
        start: f64,
        time: Option<f64>,
        frame_of_reference: &str,
    ) -> Series {
        let (orientation, normal): ([f64; 9], [f64; 3]) = match plane {
            DominantPlane::Coronal => ([1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
            DominantPlane::Sagittal => ([0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            _ => ([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
        };
        let slices = (0..count)
            .map(|i| {
                let t = start + i as f64 * gap;
                let position = match plane {
                    DominantPlane::Coronal => [-200.0, t, 0.0],
                    DominantPlane::Sagittal => [t, -200.0, 0.0],
                    _ => [-200.0, -200.0, t],
                };
                SliceRef {
                    path: PathBuf::from(format!("placed_{number}_{i}.dcm")),
                    frame: 0,
                    instance: i as i64 + 1,
                    position: Some(position),
                    orientation,
                    is_3d_plane: true,
                    spacing: Some((0.78, 0.78)),
                    slice_location: Some(t),
                    frame_of_reference_uid: Some(frame_of_reference.into()),
                }
            })
            .collect();
        Series {
            uid: format!("2.25.{number}"),
            number,
            description: description.into(),
            modality: "CT".into(),
            rows: 512,
            columns: 512,
            normal: Some(normal),
            dominant_plane: plane,
            frame_of_reference_uid: Some(frame_of_reference.into()),
            acquisition_time_secs: time,
            acquisition_number: Some(1),
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            mpr_group_id: None,
            slices,
        }
    }

    #[test]
    fn canon_kernel_codes_are_understood() {
        assert_eq!(series_kernel_class("FC08"), Some("soft"));
        assert_eq!(series_kernel_class("FC30"), Some("bone"));
        assert_eq!(series_kernel_class("FC52"), Some("lung"));
        assert_eq!(series_kernel_class("FC09"), Some("soft"));
        assert_eq!(series_kernel_class("AFC"), None);
    }

    #[test]
    fn complete_volumes_get_the_mpr_badge_and_thick_series_keep_their_plane() {
        let ax = DominantPlane::Axial;
        let thin = placed_series(1, "TAP 1.0", ax, 300, 1.0, -300.0, None, "F");
        let two = placed_series(2, "TAP 2.0", ax, 200, 2.0, -400.0, None, "F");
        let thick = placed_series(3, "TAP 5.0", ax, 80, 5.0, -400.0, None, "F");
        let few = placed_series(4, "Short 1.0", ax, 12, 1.0, 0.0, None, "F");
        let cor = placed_series(5, "COR 3.0", DominantPlane::Coronal, 60, 3.0, -90.0, None, "F");
        assert_eq!(thin.cut_badge(), "MPR");
        assert_eq!(two.cut_badge(), "MPR");
        assert_eq!(thick.cut_badge(), "AX");
        assert_eq!(few.cut_badge(), "AX");
        assert_eq!(cor.cut_badge(), "COR");
        assert!((thick.slice_gap_mm().unwrap() - 5.0).abs() < 1e-9);
    }

    /// Coronal and sagittal reconstructions go under the volume they were made from:
    /// the lung one under the lung-kernel volume, a later phase's under that phase,
    /// and nothing from another scan (frame of reference) or another moment.
    #[test]
    fn reconstructions_are_filed_under_their_source_volume() {
        let ax = DominantPlane::Axial;
        let cor = DominantPlane::Coronal;
        let sag = DominantPlane::Sagittal;
        let series = vec![
            placed_series(4, "TAP+C 2.0 B31s", ax, 280, 2.0, -560.0, Some(1000.0), "F"), // 0
            placed_series(6, "TAP 2.0 B70s lung", ax, 280, 2.0, -560.0, Some(1000.0), "F"), // 1
            placed_series(602, "COR CC", cor, 110, 3.0, -160.0, Some(1000.0), "F"), // 2
            placed_series(603, "COR LUNG", cor, 90, 3.0, -130.0, Some(1000.0), "F"), // 3
            placed_series(604, "SAG CC", sag, 80, 3.0, -120.0, Some(1001.0), "F"), // 4
            placed_series(7, "Arterial 1.0", ax, 400, 1.0, -400.0, Some(900.0), "F"), // 5
            placed_series(605, "COR arterial", cor, 70, 3.0, -100.0, Some(901.0), "F"), // 6
            placed_series(8, "Other scan", cor, 50, 3.0, -75.0, None, "G"), // 7
            placed_series(9, "Delayed COR", cor, 50, 3.0, -75.0, Some(1300.0), "F"), // 8
        ];
        let found = find_reconstructions(&series);
        let kids_of = |v: usize| -> Vec<usize> {
            found.iter().find(|(p, _)| *p == v).map(|(_, k)| k.clone()).unwrap_or_default()
        };
        assert_eq!(kids_of(0), vec![2, 4], "{found:?}");
        assert_eq!(kids_of(1), vec![3], "{found:?}");
        assert_eq!(kids_of(5), vec![6], "{found:?}");
        let filed: Vec<usize> = found.iter().flat_map(|(_, k)| k.clone()).collect();
        assert!(!filed.contains(&7), "another frame of reference was filed: {found:?}");
        assert!(!filed.contains(&8), "another moment was filed: {found:?}");
    }

    /// The layout of a whole-spine study: thin bone and soft-tissue volumes of the
    /// thoracic spine, same-named thoracic and lumbar reconstruction blocks (AX,
    /// SAG, and a coronal saved without position or orientation), a dose report,
    /// and cervical volumes in two planes.
    #[test]
    fn spine_study_reconstructions_dose_report_and_twin_volumes() {
        let ax = DominantPlane::Axial;
        let sag = DominantPlane::Sagittal;
        let cor = DominantPlane::Coronal;
        // Saved as pictures: no position and no orientation.
        let no_geometry = |mut s: Series| {
            for sl in &mut s.slices {
                sl.position = None;
                sl.orientation = crate::geometry::image_plane::ImagePlaneGeometry::DEFAULT_ORIENTATION;
                sl.is_3d_plane = false;
            }
            s.normal = None;
            s.dominant_plane = DominantPlane::Unknown;
            refine_plane(&mut s);
            s
        };
        let lower = |mut s: Series, dz: f64| {
            for sl in &mut s.slices {
                if let Some(p) = sl.position.as_mut() {
                    p[2] += dz;
                }
            }
            s
        };
        let mut dose = placed_series(99, "Dose Report", ax, 2, 1.0, 0.0, None, "F");
        dose.normal = None;
        // Canon dose page: three pictures, no position, no pixel size, number 9000.
        let mut dose_pages = no_geometry(placed_series(9000, "4", ax, 3, 1.0, 0.0, None, "F"));
        for sl in &mut dose_pages.slices {
            sl.spacing = None;
        }
        let kernel = |mut s: Series, k: &str| {
            s.convolution_kernel = Some(k.into());
            s
        };
        let t = Some(500.0);
        let tl = Some(612.5); // the lumbar acquisition, 112.5 s later
        let mut series = vec![
            placed_series(12, "BLANDO Columna 2.0", ax, 300, 2.0, -600.0, t, "F"), // 0
            placed_series(13, "OSEO Hueso 2.0", ax, 300, 2.0, -600.0, t, "F"), // 1
            placed_series(28, "BLANDO Columna 2.000", ax, 160, 3.0, -500.0, t, "F"), // 2
            placed_series(29, "BLANDO Columna 2.000", sag, 52, 3.0, -75.0, t, "F"), // 3
            no_geometry(placed_series(30, "BLANDO Columna 2.000", cor, 60, 3.0, -90.0, t, "F")), // 4
            placed_series(31, "OSEO Hueso 2.000", ax, 159, 3.0, -500.0, t, "F"), // 5
            placed_series(32, "OSEO Hueso 2.000", sag, 45, 3.0, -65.0, t, "F"), // 6
            no_geometry(placed_series(33, "OSEO Hueso 2.000", cor, 57, 3.0, -90.0, t, "F")), // 7
            // Lumbar block, below the thoracic volume, same names, later acquisition.
            kernel(placed_series(34, "BLANDO Columna 2.000", ax, 80, 3.0, -1150.0, tl, "F"), "FC08"), // 8
            kernel(lower(placed_series(35, "BLANDO Columna 2.000", sag, 50, 3.0, -75.0, tl, "F"), -700.0), "FC08"), // 9
            kernel(no_geometry(placed_series(36, "BLANDO Columna 2.000", cor, 69, 3.0, -90.0, tl, "F")), "FC08"), // 10
            dose, // 11
            placed_series(22, "CERVICAL PB Columna 1.000", ax, 166, 1.0, 0.0, Some(400.0), "C"), // 12
            placed_series(23, "CERVICAL PB Columna 1.000", sag, 96, 1.0, -48.0, Some(400.0), "C"), // 13
            kernel(placed_series(16, "OSEO Hueso 2.0", ax, 251, 2.0, -1196.0, tl, "F"), "FC30"), // 14
            dose_pages, // 15
        ];
        infer_missing_planes(&mut series);
        for i in [4, 7, 10] {
            assert_eq!(series[i].dominant_plane, DominantPlane::Coronal, "series {i}: the plane its block lacks");
            assert!(series[i].normal.is_none());
        }
        let found = find_reconstructions(&series);
        let kids_of = |v: usize| -> Vec<usize> {
            let mut k = found.iter().find(|(p, _)| *p == v).map(|(_, k)| k.clone()).unwrap_or_default();
            k.sort();
            k
        };
        assert_eq!(kids_of(0), vec![2, 3, 4], "thoracic soft tissue: {found:?}");
        assert_eq!(kids_of(1), vec![5, 6, 7], "thoracic bone: {found:?}");
        assert_eq!(kids_of(14), vec![8, 9, 10], "lumbar block under the lumbar volume: {found:?}");
        assert!(is_auxiliary(&series[15]), "Canon dose pages go with the scouts");
        assert!(is_auxiliary(&series[11]), "dose report goes with the scouts");
        assert!(!found.iter().any(|(_, k)| k.contains(&11)));
        assert_eq!(study_cut_badge(&series, 0), "MPR");
        assert_eq!(study_cut_badge(&series, 12), "MPR-A");
        assert_eq!(study_cut_badge(&series, 13), "MPR-S");
        assert_eq!(study_cut_badge(&series, 10), "COR");
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
