//! Persistent local SQLite study/series index (WAL mode, metadata + file paths only).
//! Modeled on Horos / IsiX's `Database.sql` pattern: atomic transactions, no pixel blobs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::dicom_index::{build_mpr_groups, DominantPlane, Index, Series, SliceRef, Study};

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
            ",
        )?;
        Ok(Self { conn })
    }

    /// Atomically upserts all non-temporary studies/series from `index` into SQLite.
    /// Temporary `.zip` cache paths are skipped so stale temp files are never persisted.
    pub fn save_index(&mut self, index: &Index) -> Result<()> {
        let tx = self.conn.transaction()?;
        for study in &index.studies {
            // Skip studies whose files all live in the ephemeral zip cache.
            let has_persistent = study.series.iter().any(|se| {
                se.slices.first().is_some_and(|sl| {
                    !sl.path.to_string_lossy().contains("rdicom-zip-cache")
                })
            });
            if !has_persistent {
                continue;
            }

            tx.execute(
                "INSERT INTO studies (uid, patient_name, patient_id, birth_date, study_date, description, accession)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(uid) DO UPDATE SET
                   patient_name = excluded.patient_name,
                   patient_id = excluded.patient_id,
                   birth_date = excluded.birth_date,
                   study_date = excluded.study_date,
                   description = excluded.description,
                   accession = excluded.accession",
                params![
                    study.uid,
                    study.patient_name,
                    study.patient_id,
                    study.birth_date,
                    study.date,
                    study.description,
                    study.accession,
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
                if se.slices.first().is_some_and(|s| s.path.exists()) {
                    total_images += se.slices.len();
                    series_list.push(se);
                }
            }

            if !series_list.is_empty() {
                let mpr_groups = build_mpr_groups(&mut series_list, &description);
                studies.push(Study {
                    uid,
                    patient_name,
                    patient_id,
                    birth_date,
                    date,
                    description,
                    accession,
                    series: series_list,
                    mpr_groups,
                });
            }
        }

        Ok(Index {
            studies,
            files_seen: total_images,
            files_skipped: 0,
        })
    }

    pub fn remove_study(&self, study_uid: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM studies WHERE uid = ?1", params![study_uid])?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM studies", [])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .ok()
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

fn default_db_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("SekhmetX");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("SekhmetX");
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("SekhmetX");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("SekhmetX");
    }
    std::env::temp_dir().join("SekhmetX")
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
        assert_eq!(loaded.studies[0].patient_name, "DOE^JANE");
        assert_eq!(loaded.studies[0].series[0].slices.len(), 1);

        db.remove_study("1.2.840.100").unwrap();
        let after_remove = db.load_index().unwrap();
        assert!(after_remove.studies.is_empty());

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
