//! Persistent local SQLite study/series index (WAL mode, metadata + file paths only).
//! Modeled on Horos / IsiX's `Database.sql` pattern: atomic transactions, no pixel blobs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::dicom_index::{build_mpr_groups, DominantPlane, Index, Series, SliceRef, Study};

const SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CopyToLibraryPolicy {
    #[default]
    Ask,
    Always,
    Never,
}

impl CopyToLibraryPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "always" => Self::Always,
            "never" => Self::Never,
            _ => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PersistedMeasure {
    Length([f32; 2], [f32; 2]),
    Angle([f32; 2], [f32; 2], [f32; 2]),
    Cobb([f32; 2], [f32; 2], [f32; 2], [f32; 2]),
    Arrow {
        tip: [f32; 2],
        tail: [f32; 2],
        text: String,
    },
    Oval([f32; 2], [f32; 2]),
    Rect([f32; 2], [f32; 2]),
    Polygon(Vec<[f32; 2]>),
    Pencil(Vec<[f32; 2]>),
    Point([f32; 2]),
}

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (or creates) the default SekhmetX library database in the user's app-data directory.
    pub fn open_default() -> Result<Self> {
        let dir = default_db_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create DB directory {}", dir.display()))?;
        Self::open_at(&dir.join("sekhmetx.db"))
    }

    /// Opens (or creates) a SekhmetX SQLite database at `path` with WAL mode & crash-safe pragmas.
    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database {}", path.display()))?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS studies (
                uid TEXT PRIMARY KEY,
                patient_name TEXT NOT NULL,
                patient_id TEXT NOT NULL,
                birth_date TEXT NOT NULL,
                study_date TEXT NOT NULL,
                description TEXT NOT NULL,
                accession TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS series (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                study_uid TEXT NOT NULL REFERENCES studies(uid) ON DELETE CASCADE,
                uid TEXT NOT NULL,
                number INTEGER NOT NULL,
                description TEXT NOT NULL,
                modality TEXT NOT NULL,
                rows INTEGER NOT NULL,
                columns INTEGER NOT NULL,
                normal_x REAL,
                normal_y REAL,
                normal_z REAL,
                dominant_plane INTEGER NOT NULL,
                frame_of_reference_uid TEXT,
                acquisition_time_secs REAL,
                acquisition_number INTEGER,
                convolution_kernel TEXT,
                window_center REAL,
                window_width REAL
            );
            CREATE TABLE IF NOT EXISTS slices (
                series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
                idx INTEGER NOT NULL,
                path TEXT NOT NULL,
                frame INTEGER NOT NULL,
                instance INTEGER NOT NULL,
                pos_x REAL,
                pos_y REAL,
                pos_z REAL,
                orient TEXT NOT NULL,
                is_3d_plane INTEGER NOT NULL,
                spacing_r REAL,
                spacing_c REAL,
                slice_location REAL,
                frame_of_reference_uid TEXT,
                PRIMARY KEY (series_id, idx)
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS key_images (
                file_path TEXT NOT NULL,
                frame INTEGER NOT NULL,
                PRIMARY KEY (file_path, frame)
            );
            CREATE TABLE IF NOT EXISTS cpr_paths (
                series_uid TEXT NOT NULL,
                name TEXT NOT NULL,
                nodes_json TEXT NOT NULL,
                plane_normal_json TEXT NOT NULL,
                PRIMARY KEY (series_uid, name)
            );
            ",
        )?;
        Self::migrate_if_needed(&conn)?;
        Ok(Self { conn })
    }

    fn migrate_if_needed(conn: &Connection) -> Result<()> {
        let ver: i64 = conn.query_row("PRAGMA user_version;", [], |r| r.get(0))?;
        if ver < 2 {
            let has_col = |table: &str, col: &str| -> Result<bool> {
                let mut stmt = conn.prepare(&format!("PRAGMA table_info({table});"))?;
                let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
                for n in names {
                    if n? == col {
                        return Ok(true);
                    }
                }
                Ok(false)
            };
            for (col, ddl) in [
                ("modality", "ALTER TABLE studies ADD COLUMN modality TEXT NOT NULL DEFAULT '';"),
                ("image_count", "ALTER TABLE studies ADD COLUMN image_count INTEGER NOT NULL DEFAULT 0;"),
                ("comments", "ALTER TABLE studies ADD COLUMN comments TEXT NOT NULL DEFAULT '';"),
                ("date_added", "ALTER TABLE studies ADD COLUMN date_added INTEGER NOT NULL DEFAULT 0;"),
                ("last_opened", "ALTER TABLE studies ADD COLUMN last_opened INTEGER NOT NULL DEFAULT 0;"),
                ("managed_dir", "ALTER TABLE studies ADD COLUMN managed_dir TEXT;"),
                ("source_paths", "ALTER TABLE studies ADD COLUMN source_paths TEXT;"),
            ] {
                if !has_col("studies", col)? {
                    conn.execute(ddl, [])?;
                }
            }
            conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS rois (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_path TEXT NOT NULL,
                    frame INTEGER NOT NULL,
                    data_json TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_rois_file_frame ON rois(file_path, frame);
                PRAGMA user_version = 2;
                ",
            )?;
        } else if ver != SCHEMA_VERSION {
            conn.execute(&format!("PRAGMA user_version = {SCHEMA_VERSION};"), [])?;
        }
        Ok(())
    }

    /// Atomically upserts all non-temporary studies/series from `index` into SQLite.
    /// Temporary `.zip` cache paths are skipped so stale temp files are never persisted.
    pub fn save_index(&mut self, index: &Index) -> Result<()> {
        let tx = self.conn.transaction()?;
        for study in &index.studies {
            // Skip studies whose files all live in the ephemeral zip cache.
            let has_persistent = study.series.iter().any(|se| {
                se.slices.first().is_some_and(|sl| {
                    let s = sl.path.to_string_lossy();
                    !s.contains("rdicom-zip-cache") && !s.contains("sekhmetx-zip-")
                })
            });
            if !has_persistent {
                continue;
            }

            let total_imgs: i64 = study.series.iter().map(|s| s.slices.len() as i64).sum();
            let first_mod = study
                .series
                .first()
                .map(|s| s.modality.as_str())
                .unwrap_or("");
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let managed_dir = {
                let candidate = study_managed_dir(&study.uid);
                candidate
                    .exists()
                    .then(|| candidate.to_string_lossy().into_owned())
            };

            let managed_root = managed_studies_dir();
            let mut src_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for se in &study.series {
                for sl in &se.slices {
                    if let Some(zip_p) = crate::dicom_index::original_zip_for_extracted_path(&sl.path)
                    {
                        src_set.insert(zip_p.to_string_lossy().into_owned());
                    } else if !sl.path.starts_with(&managed_root) {
                        let p_str = sl.path.to_string_lossy();
                        if !p_str.contains("sekhmetx-zip-") && !p_str.contains("rdicom-zip-cache") {
                            src_set.insert(p_str.into_owned());
                        }
                    }
                }
            }
            let source_paths_json = if src_set.is_empty() {
                None
            } else {
                let vec: Vec<String> = src_set.into_iter().collect();
                serde_json::to_string(&vec).ok()
            };

            tx.execute(
                "INSERT INTO studies (uid, patient_name, patient_id, birth_date, study_date, description, accession, modality, image_count, date_added, last_opened, managed_dir, source_paths)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?12)
                 ON CONFLICT(uid) DO UPDATE SET
                   patient_name = excluded.patient_name,
                   patient_id = excluded.patient_id,
                   birth_date = excluded.birth_date,
                   study_date = excluded.study_date,
                   description = excluded.description,
                   accession = excluded.accession,
                   modality = excluded.modality,
                   image_count = excluded.image_count,
                   last_opened = excluded.last_opened,
                   managed_dir = COALESCE(excluded.managed_dir, studies.managed_dir),
                   source_paths = COALESCE(excluded.source_paths, studies.source_paths)",
                params![
                    study.uid,
                    study.patient_name,
                    study.patient_id,
                    study.birth_date,
                    study.date,
                    study.description,
                    study.accession,
                    first_mod,
                    total_imgs,
                    now_secs,
                    managed_dir,
                    source_paths_json,
                ],
            )?;

            // Replace series & slices for this study atomically (ON DELETE CASCADE cleans slices).
            tx.execute("DELETE FROM series WHERE study_uid = ?1", params![study.uid])?;

            for se in &study.series {
                let (nx, ny, nz) = match se.normal {
                    Some([x, y, z]) => (Some(x), Some(y), Some(z)),
                    None => (None, None, None),
                };
                tx.execute(
                    "INSERT INTO series (
                        study_uid, uid, number, description, modality, rows, columns,
                        normal_x, normal_y, normal_z, dominant_plane,
                        frame_of_reference_uid, acquisition_time_secs, acquisition_number,
                        convolution_kernel, window_center, window_width
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                    params![
                        study.uid,
                        se.uid,
                        se.number,
                        se.description,
                        se.modality,
                        se.rows as i64,
                        se.columns as i64,
                        nx,
                        ny,
                        nz,
                        plane_to_int(se.dominant_plane),
                        se.frame_of_reference_uid,
                        se.acquisition_time_secs,
                        se.acquisition_number,
                        se.convolution_kernel,
                        se.window_center,
                        se.window_width,
                    ],
                )?;
                let series_id = tx.last_insert_rowid();
                let mut stmt = tx.prepare_cached(
                    "INSERT INTO slices (
                        series_id, idx, path, frame, instance,
                        pos_x, pos_y, pos_z, orient, is_3d_plane,
                        spacing_r, spacing_c, slice_location, frame_of_reference_uid
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                )?;
                for (idx, sl) in se.slices.iter().enumerate() {
                    let (px, py, pz) = match sl.position {
                        Some([x, y, z]) => (Some(x), Some(y), Some(z)),
                        None => (None, None, None),
                    };
                    let (sr, sc) = match sl.spacing {
                        Some((r, c)) => (Some(r), Some(c)),
                        None => (None, None),
                    };
                    let orient_str = sl
                        .orientation
                        .iter()
                        .map(|v| format!("{v:.6}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    stmt.execute(params![
                        series_id,
                        idx as i64,
                        sl.path.to_string_lossy().as_ref(),
                        sl.frame as i64,
                        sl.instance,
                        px,
                        py,
                        pz,
                        orient_str,
                        if sl.is_3d_plane { 1i64 } else { 0i64 },
                        sr,
                        sc,
                        sl.slice_location,
                        sl.frame_of_reference_uid,
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads all saved studies whose files still exist on disk.
    pub fn load_index(&self) -> Result<Index> {
        let mut study_stmt = self.conn.prepare(
            "SELECT uid, patient_name, patient_id, birth_date, study_date, description, accession
             FROM studies ORDER BY study_date DESC, patient_name ASC",
        )?;
        let study_rows = study_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut studies = Vec::new();
        let mut total_images = 0usize;

        for sr in study_rows {
            let (uid, patient_name, patient_id, birth_date, date, description, accession) = sr?;
            let mut series_stmt = self.conn.prepare(
                "SELECT id, uid, number, description, modality, rows, columns,
                        normal_x, normal_y, normal_z, dominant_plane,
                        frame_of_reference_uid, acquisition_time_secs, acquisition_number,
                        convolution_kernel, window_center, window_width
                 FROM series WHERE study_uid = ?1 ORDER BY number ASC",
            )?;
            let series_rows = series_stmt.query_map(params![uid], |row| {
                let nx: Option<f64> = row.get(7)?;
                let ny: Option<f64> = row.get(8)?;
                let nz: Option<f64> = row.get(9)?;
                let normal = match (nx, ny, nz) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                };
                Ok((
                    row.get::<_, i64>(0)?,
                    Series {
                        uid: row.get(1)?,
                        number: row.get(2)?,
                        description: row.get(3)?,
                        modality: row.get(4)?,
                        rows: row.get::<_, i64>(5)? as u32,
                        columns: row.get::<_, i64>(6)? as u32,
                        normal,
                        dominant_plane: int_to_plane(row.get(10)?),
                        frame_of_reference_uid: row.get(11)?,
                        acquisition_time_secs: row.get(12)?,
                        acquisition_number: row.get(13)?,
                        convolution_kernel: row.get(14)?,
                        window_center: row.get(15)?,
                        window_width: row.get(16)?,
                        mpr_group_id: None,
                        slices: Vec::new(),
                    },
                ))
            })?;

            let mut series_list = Vec::new();
            for ser_res in series_rows {
                let (series_id, mut se) = ser_res?;
                let mut slice_stmt = self.conn.prepare_cached(
                    "SELECT path, frame, instance, pos_x, pos_y, pos_z, orient,
                            is_3d_plane, spacing_r, spacing_c, slice_location, frame_of_reference_uid
                     FROM slices WHERE series_id = ?1 ORDER BY idx ASC",
                )?;
                let slice_rows = slice_stmt.query_map(params![series_id], |row| {
                    let path_str: String = row.get(0)?;
                    let px: Option<f64> = row.get(3)?;
                    let py: Option<f64> = row.get(4)?;
                    let pz: Option<f64> = row.get(5)?;
                    let position = match (px, py, pz) {
                        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                        _ => None,
                    };
                    let orient_str: String = row.get(6)?;
                    let orientation = parse_orient(&orient_str);
                    let sr: Option<f64> = row.get(8)?;
                    let sc: Option<f64> = row.get(9)?;
                    let spacing = match (sr, sc) {
                        (Some(r), Some(c)) => Some((r, c)),
                        _ => None,
                    };
                    Ok(SliceRef {
                        path: PathBuf::from(path_str),
                        frame: row.get::<_, i64>(1)? as u32,
                        instance: row.get(2)?,
                        position,
                        orientation,
                        is_3d_plane: row.get::<_, i64>(7)? != 0,
                        spacing,
                        slice_location: row.get(10)?,
                        frame_of_reference_uid: row.get(11)?,
                    })
                })?;

                for sl in slice_rows {
                    se.slices.push(sl?);
                }
                total_images += se.slices.len();
                series_list.push(se);
            }

            if !series_list.is_empty() {
                let any_present = series_list
                    .iter()
                    .any(|se| se.slices.first().is_some_and(|s| s.path.exists()));
                for se in &mut series_list {
                    crate::dicom_index::refine_plane(se);
                }
                let mpr_groups = build_mpr_groups(&mut series_list, &description);
                crate::dicom_index::infer_missing_planes(&mut series_list);
                let reformats = crate::dicom_index::find_reconstructions(&series_list);
                studies.push(Study {
                    uid,
                    patient_name,
                    patient_id,
                    birth_date,
                    date,
                    description,
                    accession,
                    series: series_list,
                    reformats,
                    mpr_groups,
                    files_missing: !any_present,
                });
            }
        }

        Ok(Index {
            studies,
            files_seen: total_images,
            files_skipped: 0,
        })
    }

    /// Loads slices for a single series on demand.
    #[allow(dead_code)]
    pub fn load_series_slices(&self, series_id: i64) -> Result<Vec<SliceRef>> {
        let mut slice_stmt = self.conn.prepare_cached(
            "SELECT path, frame, instance, pos_x, pos_y, pos_z, orient,
                    is_3d_plane, spacing_r, spacing_c, slice_location, frame_of_reference_uid
             FROM slices WHERE series_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = slice_stmt.query_map(params![series_id], |row| {
            let path_str: String = row.get(0)?;
            let px: Option<f64> = row.get(3)?;
            let py: Option<f64> = row.get(4)?;
            let pz: Option<f64> = row.get(5)?;
            let position = match (px, py, pz) {
                (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                _ => None,
            };
            let orient_str: String = row.get(6)?;
            let orientation = parse_orient(&orient_str);
            let sr: Option<f64> = row.get(8)?;
            let sc: Option<f64> = row.get(9)?;
            let spacing = match (sr, sc) {
                (Some(r), Some(c)) => Some((r, c)),
                _ => None,
            };
            Ok(SliceRef {
                path: PathBuf::from(path_str),
                frame: row.get::<_, i64>(1)? as u32,
                instance: row.get(2)?,
                position,
                orientation,
                is_3d_plane: row.get::<_, i64>(7)? != 0,
                spacing,
                slice_location: row.get(10)?,
                frame_of_reference_uid: row.get(11)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn remove_study(&self, study_uid: &str) -> Result<()> {
        let managed: Option<String> = self
            .conn
            .query_row(
                "SELECT managed_dir FROM studies WHERE uid = ?1",
                params![study_uid],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        self.conn
            .execute("DELETE FROM studies WHERE uid = ?1", params![study_uid])?;
        let root = managed_studies_dir();
        if let Some(dir) = managed {
            let p = PathBuf::from(&dir);
            if p.starts_with(&root) && p != root && p.exists() {
                let _ = trash::delete(&p);
            }
        }
        let fallback = study_managed_dir(study_uid);
        if fallback.starts_with(&root) && fallback != root && fallback.exists() {
            let _ = trash::delete(&fallback);
        }
        Ok(())
    }

    pub fn set_study_source_paths(&self, study_uid: &str, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        let vec: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if let Ok(json) = serde_json::to_string(&vec) {
            let _ = self.conn.execute(
                "UPDATE studies SET source_paths = ?2 WHERE uid = ?1",
                params![study_uid, json],
            );
        }
    }

    pub fn study_source_paths(&self, study_uid: &str) -> Vec<PathBuf> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT source_paths FROM studies WHERE uid = ?1",
                params![study_uid],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        json.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect()
    }

    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM studies", [])?;
        self.conn.execute("DELETE FROM rois", [])?;
        Ok(())
    }

    pub fn save_rois_for_slice(
        &self,
        path: &Path,
        frame: u32,
        rois: &[PersistedMeasure],
    ) -> Result<()> {
        let key = path.to_string_lossy();
        self.conn.execute(
            "DELETE FROM rois WHERE file_path = ?1 AND frame = ?2",
            params![key.as_ref(), frame as i64],
        )?;
        for roi in rois {
            let json = serde_json::to_string(roi)?;
            self.conn.execute(
                "INSERT INTO rois (file_path, frame, data_json) VALUES (?1, ?2, ?3)",
                params![key.as_ref(), frame as i64, json],
            )?;
        }
        Ok(())
    }

    pub fn load_all_rois(&self) -> Result<std::collections::HashMap<(PathBuf, u32), Vec<PersistedMeasure>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_path, frame, data_json FROM rois")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u32,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out: std::collections::HashMap<(PathBuf, u32), Vec<PersistedMeasure>> =
            std::collections::HashMap::new();
        for r in rows {
            let (p, f, j) = r?;
            if let Ok(m) = serde_json::from_str::<PersistedMeasure>(&j) {
                out.entry((PathBuf::from(p), f)).or_default().push(m);
            }
        }
        Ok(out)
    }

    pub fn set_key_image(&self, file_path: &Path, frame: u32, is_key: bool) -> Result<()> {
        let p = file_path.to_string_lossy();
        if is_key {
            self.conn.execute(
                "INSERT OR IGNORE INTO key_images (file_path, frame) VALUES (?1, ?2)",
                params![p, frame as i64],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM key_images WHERE file_path = ?1 AND frame = ?2",
                params![p, frame as i64],
            )?;
        }
        Ok(())
    }

    pub fn load_all_key_images(&self) -> Result<std::collections::HashSet<(PathBuf, u32)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_path, frame FROM key_images")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32))
        })?;
        let mut out = std::collections::HashSet::new();
        for r in rows {
            let (p, f) = r?;
            out.insert((PathBuf::from(p), f));
        }
        Ok(out)
    }

    /// Saves or overwrites a named Curved MPR centerline path for `series_uid`.
    pub fn save_cpr_path(
        &self,
        series_uid: &str,
        name: &str,
        nodes: &[[f64; 3]],
        plane_normal: [f64; 3],
    ) -> Result<()> {
        let nodes_json = serde_json::to_string(nodes)?;
        let normal_json = serde_json::to_string(&plane_normal)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO cpr_paths (series_uid, name, nodes_json, plane_normal_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![series_uid, name, nodes_json, normal_json],
        )?;
        Ok(())
    }

    /// Loads all saved Curved MPR paths for `series_uid`.
    #[allow(clippy::type_complexity)]
    pub fn load_cpr_paths(&self, series_uid: &str) -> Result<Vec<(String, Vec<[f64; 3]>, [f64; 3])>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, nodes_json, plane_normal_json FROM cpr_paths
             WHERE series_uid = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![series_uid], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (name, nj, pj) = r?;
            if let (Ok(nodes), Ok(normal)) = (
                serde_json::from_str::<Vec<[f64; 3]>>(&nj),
                serde_json::from_str::<[f64; 3]>(&pj),
            ) {
                out.push((name, nodes, normal));
            }
        }
        Ok(out)
    }

    /// Deletes a named Curved MPR path for `series_uid`.
    pub fn delete_cpr_path(&self, series_uid: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM cpr_paths WHERE series_uid = ?1 AND name = ?2",
            params![series_uid, name],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .ok()
    }

    /// All settings whose key starts with `prefix`, as (key, value) pairs.
    pub fn settings_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let pattern = format!("{}%", prefix.replace('%', "\\%").replace('_', "\\_"));
        let Ok(mut stmt) = self
            .conn
            .prepare("SELECT key, value FROM settings WHERE key LIKE ?1 ESCAPE '\\'")
        else {
            return Vec::new();
        };
        stmt.query_map(params![pattern], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

static CUSTOM_STUDIES_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

pub fn set_custom_studies_dir(dir: Option<PathBuf>) {
    if let Ok(mut guard) = CUSTOM_STUDIES_DIR.write() {
        *guard = dir.filter(|p| !p.as_os_str().is_empty());
    }
}

pub fn managed_studies_dir() -> PathBuf {
    if let Ok(guard) = CUSTOM_STUDIES_DIR.read()
        && let Some(ref custom) = *guard {
            return custom.clone();
        }
    default_db_dir().join("Studies")
}

pub fn study_managed_dir(study_uid: &str) -> PathBuf {
    let safe: String = study_uid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    managed_studies_dir().join(if safe.is_empty() { "unknown" } else { &safe })
}

/// Copies all slice files of `study` into `<data dir>/SekhmetX/Studies/<StudyUID>/`
/// and updates slice paths in-place so ZIP/removable-drive studies persist in the library.
pub fn copy_study_into_library(study: &mut Study) -> Result<()> {
    copy_study_into_dir(study, &study_managed_dir(&study.uid))
}

pub fn copy_study_into_dir(study: &mut Study, target_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(target_dir)?;
    let mut copied_map: std::collections::HashMap<PathBuf, PathBuf> =
        std::collections::HashMap::new();
    for (se_idx, se) in study.series.iter_mut().enumerate() {
        for (sl_idx, sl) in se.slices.iter_mut().enumerate() {
            if let Some(new_path) = copied_map.get(&sl.path) {
                sl.path = new_path.clone();
                continue;
            }
            if !sl.path.exists() {
                continue;
            }
            let file_name = sl
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("slice.dcm");
            let dest = target_dir.join(format!("s{se_idx:03}_{sl_idx:05}_{file_name}"));
            if !dest.exists() {
                std::fs::copy(&sl.path, &dest)?;
            }
            copied_map.insert(sl.path.clone(), dest.clone());
            sl.path = dest;
        }
    }
    study.files_missing = false;
    Ok(())
}

/// Returns cached total disk usage in bytes of all managed studies, refreshing in the
/// background at most once every 15 seconds (or when invalidated) so UI frames never block.
pub fn library_disk_usage_bytes() -> u64 {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    static CACHED_BYTES: AtomicU64 = AtomicU64::new(0);
    static REFRESHING: AtomicBool = AtomicBool::new(false);
    static LAST_STATE: OnceLock<Mutex<Option<(PathBuf, Instant)>>> = OnceLock::new();

    let dir = managed_studies_dir();
    let state = LAST_STATE.get_or_init(|| Mutex::new(None));
    let needs_refresh = if let Ok(guard) = state.lock() {
        match &*guard {
            Some((prev_dir, prev_at)) => {
                *prev_dir != dir || prev_at.elapsed() >= Duration::from_secs(15)
            }
            None => true,
        }
    } else {
        false
    };

    if needs_refresh && !REFRESHING.swap(true, Ordering::AcqRel) {
        std::thread::spawn(move || {
            let bytes = dir_size_bytes(&dir);
            CACHED_BYTES.store(bytes, Ordering::Relaxed);
            if let Ok(mut guard) = LAST_STATE.get_or_init(|| Mutex::new(None)).lock() {
                *guard = Some((dir, Instant::now()));
            }
            REFRESHING.store(false, Ordering::Release);
        });
    }

    CACHED_BYTES.load(Ordering::Relaxed)
}

fn dir_size_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total = total.saturating_add(dir_size_bytes(&p));
        } else if let Ok(m) = entry.metadata() {
            total = total.saturating_add(m.len());
        }
    }
    total
}

fn default_db_dir() -> PathBuf {
    crate::crash_log::app_data_dir()
}

fn plane_to_int(p: DominantPlane) -> i64 {
    match p {
        DominantPlane::Axial => 0,
        DominantPlane::Coronal => 1,
        DominantPlane::Sagittal => 2,
        DominantPlane::Oblique => 3,
        DominantPlane::Unknown => 4,
    }
}

fn int_to_plane(v: i64) -> DominantPlane {
    match v {
        0 => DominantPlane::Axial,
        1 => DominantPlane::Coronal,
        2 => DominantPlane::Sagittal,
        3 => DominantPlane::Oblique,
        _ => DominantPlane::Unknown,
    }
}

fn parse_orient(s: &str) -> [f64; 9] {
    let mut out = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    for (i, part) in s.split(',').take(9).enumerate() {
        if let Ok(v) = part.parse::<f64>() {
            out[i] = v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_wal_roundtrip_save_load_and_remove() {
        let dir = std::env::temp_dir().join(format!("sekhmetx-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dummy_dcm = dir.join("slice1.dcm");
        std::fs::write(&dummy_dcm, b"DICM").unwrap();

        let db_path = dir.join("test.db");
        let mut db = Database::open_at(&db_path).unwrap();

        let mode: String = db
            .conn
            .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");

        let index = Index {
            studies: vec![Study {
                uid: "1.2.840.100".into(),
                patient_name: "DOE^JANE".into(),
                patient_id: "P001".into(),
                birth_date: "1980-01-01".into(),
                date: "2026-11-14".into(),
                description: "CT ABDOMEN".into(),
                accession: "ACC1".into(),
                mpr_groups: Vec::new(),
                reformats: Vec::new(),
                files_missing: false,
                series: vec![Series {
                    uid: "1.2.840.100.1".into(),
                    number: 4,
                    description: "Angio ABD".into(),
                    modality: "CT".into(),
                    rows: 512,
                    columns: 512,
                    normal: Some([0.0, 0.0, 1.0]),
                    dominant_plane: DominantPlane::Axial,
                    frame_of_reference_uid: Some("1.2.3".into()),
                    acquisition_time_secs: Some(1234.5),
                    acquisition_number: Some(1),
                    convolution_kernel: Some("B30f".into()),
                    window_center: Some(50.0),
                    window_width: Some(350.0),
                    mpr_group_id: None,
                    slices: vec![SliceRef {
                        path: dummy_dcm.clone(),
                        frame: 0,
                        instance: 1,
                        position: Some([0.0, 0.0, -100.0]),
                        orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                        is_3d_plane: true,
                        spacing: Some((0.7, 0.7)),
                        slice_location: Some(-100.0),
                        frame_of_reference_uid: Some("1.2.3".into()),
                    }],
                }],
            }],
            files_seen: 1,
            files_skipped: 0,
        };

        db.save_index(&index).unwrap();
        let loaded = db.load_index().unwrap();
        assert_eq!(loaded.studies.len(), 1);
        assert!(!loaded.studies[0].files_missing);
        assert_eq!(loaded.studies[0].patient_name, "DOE^JANE");
        assert_eq!(loaded.studies[0].series[0].slices.len(), 1);

        let user_ver: i64 = db
            .conn
            .query_row("PRAGMA user_version;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(user_ver, SCHEMA_VERSION);

        // Save and reload ROIs for the slice.
        let rois = vec![
            PersistedMeasure::Length([10.0, 20.0], [30.0, 40.0]),
            PersistedMeasure::Oval([5.0, 5.0], [15.0, 15.0]),
        ];
        db.save_rois_for_slice(&dummy_dcm, 0, &rois).unwrap();
        let loaded_rois = db.load_all_rois().unwrap();
        assert_eq!(loaded_rois.get(&(dummy_dcm.clone(), 0)), Some(&rois));

        // Save, reload, and remove Key Images.
        db.set_key_image(&dummy_dcm, 0, true).unwrap();
        assert!(db.load_all_key_images().unwrap().contains(&(dummy_dcm.clone(), 0)));
        db.set_key_image(&dummy_dcm, 0, false).unwrap();
        assert!(!db.load_all_key_images().unwrap().contains(&(dummy_dcm.clone(), 0)));

        // Removing the file from disk marks the study as `files_missing` instead of hiding it.
        std::fs::remove_file(&dummy_dcm).unwrap();
        let missing_loaded = db.load_index().unwrap();
        assert_eq!(missing_loaded.studies.len(), 1);
        assert!(missing_loaded.studies[0].files_missing);

        db.remove_study("1.2.840.100").unwrap();
        let after_remove = db.load_index().unwrap();
        assert!(after_remove.studies.is_empty());

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
