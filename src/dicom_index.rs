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

/// One displayable image: a file, plus a frame number for multi-frame files.
#[derive(Debug, Clone)]
pub struct SliceRef {
    pub path: PathBuf,
    pub frame: u32,
    pub instance: i64,
    pub position: Option<[f64; 3]>,
    pub slice_location: Option<f64>,
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
    pub slices: Vec<SliceRef>,
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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
    slice_location: Option<f64>,
}

const NON_IMAGE_MODALITIES: &[&str] = &["SR", "PR", "KO", "REG", "SEG", "RTSTRUCT", "RTPLAN", "RTDOSE", "DOC"];

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

    Some(Header {
        path: path.to_path_buf(),
        patient_name: get_str(&obj, tags::PATIENT_NAME)
            .map(|s| format_person_name(&s))
            .unwrap_or_else(|| "(no name)".into()),
        patient_id: get_str(&obj, tags::PATIENT_ID).unwrap_or_default(),
        birth_date: get_str(&obj, tags::PATIENT_BIRTH_DATE)
            .map(|s| format_date(&s))
            .unwrap_or_default(),
        study_uid: get_str(&obj, tags::STUDY_INSTANCE_UID).unwrap_or_else(|| "unknown-study".into()),
        study_date: get_str(&obj, tags::STUDY_DATE)
            .map(|s| format_date(&s))
            .unwrap_or_default(),
        study_desc: get_str(&obj, tags::STUDY_DESCRIPTION).unwrap_or_default(),
        accession: get_str(&obj, tags::ACCESSION_NUMBER).unwrap_or_default(),
        series_uid: get_str(&obj, tags::SERIES_INSTANCE_UID).unwrap_or_else(|| "unknown-series".into()),
        series_number: get_int(&obj, tags::SERIES_NUMBER).unwrap_or(0),
        series_desc: get_str(&obj, tags::SERIES_DESCRIPTION).unwrap_or_default(),
        modality,
        rows,
        columns,
        frames: get_int(&obj, tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as u32,
        instance: get_int(&obj, tags::INSTANCE_NUMBER).unwrap_or(0),
        position,
        orientation,
        slice_location: get_f64(&obj, tags::SLICE_LOCATION),
    })
}

/// Collects candidate files from a mix of files and folders.
pub fn collect_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            for entry in walkdir::WalkDir::new(input).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy();
                if name.starts_with('.') || name.eq_ignore_ascii_case("DICOMDIR") {
                    continue;
                }
                out.push(entry.into_path());
            }
        } else if input.is_file() {
            out.push(input.clone());
        }
    }
    out
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

        let normal = h
            .orientation
            .map(|o| cross([o[0], o[1], o[2]], [o[3], o[4], o[5]]));

        let series = series_map.entry(key).or_insert_with(|| Series {
            uid: h.series_uid.clone(),
            number: h.series_number,
            description: h.series_desc.clone(),
            modality: h.modality.clone(),
            rows: h.rows,
            columns: h.columns,
            normal,
            slices: Vec::new(),
        });

        for frame in 0..h.frames {
            series.slices.push(SliceRef {
                path: h.path.clone(),
                frame,
                instance: h.instance,
                position: h.position,
                slice_location: h.slice_location,
            });
        }
    }

    let mut out: Vec<Study> = studies
        .into_values()
        .map(|(mut study, series_map)| {
            let mut series: Vec<Series> = series_map.into_values().collect();
            for s in &mut series {
                sort_slices(s);
            }
            series.sort_by(|a, b| {
                a.number
                    .cmp(&b.number)
                    .then_with(|| b.slices.len().cmp(&a.slices.len()))
            });
            // Disambiguate split series that share a description.
            let mut seen: HashMap<(i64, String), usize> = HashMap::new();
            for s in &mut series {
                let n = seen.entry((s.number, s.description.clone())).or_insert(0);
                *n += 1;
                if *n > 1 {
                    s.description = format!("{} ({})", s.description, n);
                }
            }
            study.series = series;
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
