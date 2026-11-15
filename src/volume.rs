//! 3D Volume representation, In-Place Single-Viewport Oblique MPR Slicer,
//! Hybrid Multi-Series (AX/COR/SAG) Fusion, and Computational 3D Rigid Registration.

use std::sync::Arc;

use rayon::prelude::*;

use crate::dicom_index::Series;
use crate::frame::{Frame, Pixels};
use crate::imaging::Window;

/// A contiguous 3D voxel volume in patient LPS physical coordinates (millimeters).
pub struct Volume3D {
    /// Monotonic unique ID for GPU 3D texture caching.
    pub gpu_id: u64,
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    /// Quantized 16-bit voxel values in `z -> y -> x` order.
    /// Decode to calibrated greyscale (`raw * slope + intercept`, e.g. HU for CT) via
    /// `q as f32 * voxel_scale + voxel_offset` (or `self.voxel_at(idx)`).
    pub voxels: Vec<i16>,
    /// Linear scale factor to decode `voxels` back to calibrated `f32` values.
    pub voxel_scale: f32,
    /// Linear offset to decode `voxels` back to calibrated `f32` values.
    pub voxel_offset: f32,
    /// Patient LPS position `(X, Y, Z)` of voxel `(0, 0, 0)` in mm.
    pub origin_lps: [f64; 3],
    /// Unit vector along `+x` (column index `0..width`).
    pub row_dir: [f64; 3],
    /// Unit vector along `+y` (row index `0..height`).
    pub col_dir: [f64; 3],
    /// Unit vector along `+z` (slice index `0..depth`).
    pub slice_dir: [f64; 3],
    /// Voxel spacing `[dx, dy, dz]` in mm along `(row_dir, col_dir, slice_dir)`.
    pub spacing: [f64; 3],
    /// Physical 3D center of the volume bounding box in patient LPS mm.
    pub center_lps: [f64; 3],
    /// Intensity-weighted 3D centroid of foreground anatomy (excluding air/background) in LPS mm.
    pub foreground_centroid_lps: [f64; 3],
    /// Half-diagonal radius of the physical bounding box in mm.
    #[allow(dead_code)]
    pub bounding_radius_mm: f64,
    /// Native 9-element direction cosine matrix of the stack.
    pub native_orientation: [f64; 9],
    pub stored_window: Option<Window>,
    pub full_window: Window,
    pub bits_window: Option<Window>,
    pub inverts: bool,
    pub slope: f32,
    pub frame_of_reference_uid: Option<String>,
    pub value_unit: &'static str,
    /// Background air threshold estimated from the volume intensity distribution.
    pub foreground_threshold: f32,
    /// `true` when slice spacing, orientation, and dimensions passed validation without forcing.
    pub geometry_verified: bool,
    /// `true` when `PixelSpacing` was missing (measurements must use pixels, not mm).
    pub uncalibrated: bool,
}

/// Port of IsiX `SliceIntervalRegularity` (measured along the slice normal).
#[derive(Clone, Debug, PartialEq)]
pub struct SliceRegularity {
    pub minimum_interval: f64,
    pub maximum_interval: f64,
    pub mean_interval: f64,
    pub largest_grid_deviation: f64,
    pub is_irregular: bool,
}

impl SliceRegularity {
    const RELATIVE_TOLERANCE: f64 = 0.10;

    pub fn evaluate(origins: &[[f64; 3]], normal: [f64; 3], pixel_spacing: f64) -> Self {
        let count = origins.len();
        if count < 2 {
            return Self {
                minimum_interval: 0.0,
                maximum_interval: 0.0,
                mean_interval: 0.0,
                largest_grid_deviation: 0.0,
                is_irregular: false,
            };
        }
        let mut n = normal;
        let mut length = len3(n);
        if length < 0.5 {
            n = sub3(origins[count - 1], origins[0]);
            length = len3(n);
        }
        if length <= 0.0 {
            return Self {
                minimum_interval: 0.0,
                maximum_interval: 0.0,
                mean_interval: 0.0,
                largest_grid_deviation: 0.0,
                is_irregular: false,
            };
        }
        n = scale3(n, 1.0 / length);
        let positions: Vec<f64> = origins.iter().map(|&o| dot3(o, n)).collect();
        let intervals: Vec<f64> = (1..count)
            .map(|i| (positions[i] - positions[i - 1]).abs())
            .collect();
        let minimum = intervals.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = intervals.iter().copied().fold(0.0_f64, f64::max);
        let span = positions[count - 1] - positions[0];
        let mean = span.abs() / (count - 1) as f64;
        if mean <= 1e-6 {
            return Self {
                minimum_interval: minimum,
                maximum_interval: maximum,
                mean_interval: mean,
                largest_grid_deviation: 0.0,
                is_irregular: maximum > 1e-3,
            };
        }
        let step = span / (count - 1) as f64;
        let deviation = (0..count)
            .map(|i| (positions[i] - (positions[0] + step * (i as f64))).abs())
            .fold(0.0_f64, f64::max);
        let uneven = intervals
            .iter()
            .any(|&iv| (iv - mean).abs() > Self::RELATIVE_TOLERANCE * mean);
        let off_grid = pixel_spacing > 0.0 && deviation > pixel_spacing / 2.0;
        Self {
            minimum_interval: minimum,
            maximum_interval: maximum,
            mean_interval: mean,
            largest_grid_deviation: deviation,
            is_irregular: uneven || off_grid,
        }
    }
}

/// Specific reason a series failed strict 3D volume validation.
#[derive(Clone, Debug, PartialEq)]
pub enum VolumeError {
    TooFewSlices { count: usize },
    IncompleteFrames { loaded: usize, expected: usize },
    EmptyDimensions,
    DimensionMismatch { slice: usize, expected: (u32, u32), got: (u32, u32) },
    NonGrayscale { slice: usize },
    MissingGeometry,
    IrregularSpacing {
        reason: String,
        min_mm: f64,
        max_mm: f64,
        mean_mm: f64,
        max_deviation_mm: f64,
    },
}

impl VolumeError {
    pub fn reason(&self) -> String {
        match self {
            Self::TooFewSlices { count } => {
                format!("Series has only {count} slice(s); at least 3 slices are required for 3D.")
            }
            Self::IncompleteFrames { loaded, expected } => {
                format!("Only {loaded} of {expected} slices are loaded.")
            }
            Self::EmptyDimensions => "Image dimensions are 0×0.".to_string(),
            Self::DimensionMismatch { slice, expected, got } => format!(
                "Slice {} has dimensions {}×{}, expected {}×{}.",
                slice + 1,
                got.0,
                got.1,
                expected.0,
                expected.1
            ),
            Self::NonGrayscale { slice } => {
                format!("Slice {} is RGB/color rather than calibrated greyscale.", slice + 1)
            }
            Self::MissingGeometry => {
                "Slices are missing 3D ImagePositionPatient / ImageOrientationPatient metadata."
                    .to_string()
            }
            Self::IrregularSpacing { reason, .. } => reason.clone(),
        }
    }
}

impl Volume3D {
    /// Builds a `Volume3D` from a `Series` and its decoded `Frame`s (one per slice).
    #[allow(dead_code)]
    pub fn from_series(series: &Series, frames: &[Arc<Frame>]) -> Option<Arc<Self>> {
        Self::from_series_checked(series, frames, false).ok()
    }

    /// Builds a `Volume3D` from a `Series` and its decoded `Frame`s.
    /// When `force` is `true`, even 1-2 slice stacks or RGB/irregular series are
    /// promoted into a 3D volume using fallback slice thickness / orientation,
    /// with `geometry_verified = false`.
    #[allow(dead_code)]
    pub fn from_series_with_force(
        series: &Series,
        frames: &[Arc<Frame>],
        force: bool,
    ) -> Option<Arc<Self>> {
        Self::from_series_checked(series, frames, force).ok()
    }

    pub fn from_series_checked(
        series: &Series,
        frames: &[Arc<Frame>],
        force: bool,
    ) -> Result<Arc<Self>, VolumeError> {
        let min_frames = if force { 1 } else { 3 };
        if frames.len() != series.slices.len() {
            return Err(VolumeError::IncompleteFrames {
                loaded: frames.len(),
                expected: series.slices.len(),
            });
        }
        if frames.len() < min_frames {
            return Err(VolumeError::TooFewSlices {
                count: frames.len(),
            });
        }
        let first = &frames[0];
        let last = &frames[frames.len() - 1];
        let w = first.width as usize;
        let h = first.height as usize;
        let d = frames.len();
        if w == 0 || h == 0 {
            return Err(VolumeError::EmptyDimensions);
        }
        // Verify all slices have matching dimensions (and greyscale unless forced).
        let mut has_non_gray = false;
        for (i, f) in frames.iter().enumerate() {
            if f.width as usize != w || f.height as usize != h {
                return Err(VolumeError::DimensionMismatch {
                    slice: i,
                    expected: (w as u32, h as u32),
                    got: (f.width, f.height),
                });
            }
            if !matches!(f.pixels, Pixels::Gray(_)) {
                if !force {
                    return Err(VolumeError::NonGrayscale { slice: i });
                }
                has_non_gray = true;
            }
        }

        let uncalibrated = frames.iter().any(|f| f.spacing.is_none());
        let (row_sp, col_sp) = first.spacing.unwrap_or((1.0, 1.0));
        let dx = col_sp.max(0.1);
        let dy = row_sp.max(0.1);

        let orient = first.orientation;
        let raw_row = [orient[0], orient[1], orient[2]];
        let raw_col = [orient[3], orient[4], orient[5]];
        let row_dir = if len3(raw_row) > 1e-6 {
            normalize3(raw_row)
        } else {
            [1.0, 0.0, 0.0]
        };
        let col_dir = if len3(raw_col) > 1e-6 {
            normalize3(raw_col)
        } else {
            [0.0, 1.0, 0.0]
        };
        let raw_norm = cross3(row_dir, col_dir);
        let normal = if len3(raw_norm) > 1e-6 {
            normalize3(raw_norm)
        } else {
            [0.0, 0.0, 1.0]
        };

        let mut missing_geom = false;
        let origins: Vec<[f64; 3]> = frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                f.origin
                    .or_else(|| series.slices.get(i).and_then(|s| s.position))
                    .unwrap_or_else(|| {
                        missing_geom = true;
                        [0.0, 0.0, i as f64]
                    })
            })
            .collect();
        if missing_geom || frames.iter().any(|f| !f.is_3d_plane) {
            missing_geom = true;
            if !force {
                return Err(VolumeError::MissingGeometry);
            }
        }

        let mut irregular_geom = false;
        if d >= 2 && !missing_geom {
            let min_px_sp = dx.min(dy);
            let reg = SliceRegularity::evaluate(&origins, normal, min_px_sp);
            if reg.is_irregular {
                irregular_geom = true;
                if !force {
                    let reason = if reg.mean_interval <= 1e-6 {
                        "Duplicate or coincident slice positions detected.".to_string()
                    } else if reg.maximum_interval > reg.mean_interval * 1.5 {
                        format!(
                            "Missing slice or gap detected (interval {:.2}–{:.2} mm, mean {:.2} mm).",
                            reg.minimum_interval, reg.maximum_interval, reg.mean_interval
                        )
                    } else {
                        format!(
                            "Variable slice intervals ({:.2}–{:.2} mm, max grid deviation {:.2} mm).",
                            reg.minimum_interval, reg.maximum_interval, reg.largest_grid_deviation
                        )
                    };
                    return Err(VolumeError::IrregularSpacing {
                        reason,
                        min_mm: reg.minimum_interval,
                        max_mm: reg.maximum_interval,
                        mean_mm: reg.mean_interval,
                        max_deviation_mm: reg.largest_grid_deviation,
                    });
                }
            }
        }
        let geometry_verified = !missing_geom && !irregular_geom && !has_non_gray && d >= 3;

        let origin0 = first
            .origin
            .or_else(|| series.slices.first().and_then(|s| s.position))
            .unwrap_or([0.0, 0.0, 0.0]);
        let origin_last = last
            .origin
            .or_else(|| series.slices.last().and_then(|s| s.position))
            .unwrap_or([
                origin0[0] + normal[0] * (d.saturating_sub(1) as f64),
                origin0[1] + normal[1] * (d.saturating_sub(1) as f64),
                origin0[2] + normal[2] * (d.saturating_sub(1) as f64),
            ]);

        let span_vec = sub3(origin_last, origin0);
        let span_len = len3(span_vec);
        let (slice_dir, dz) = if span_len > 1e-3 && d > 1 {
            (
                scale3(span_vec, 1.0 / span_len),
                (span_len / (d - 1) as f64).max(0.1),
            )
        } else {
            let fallback_dz = first.slice_thickness.unwrap_or(1.0).max(0.2);
            (normal, fallback_dz)
        };

        let slice_len = w * h;
        let mut min_val = f32::INFINITY;
        let mut max_val = f32::NEG_INFINITY;
        for f in frames {
            let fw_min = f.full_window.level - 0.5 * f.full_window.width;
            let fw_max = f.full_window.level + 0.5 * f.full_window.width;
            if fw_min < min_val {
                min_val = fw_min;
            }
            if fw_max > max_val {
                max_val = fw_max;
            }
        }
        if !min_val.is_finite() || !max_val.is_finite() || max_val <= min_val {
            min_val = -1024.0;
            max_val = 3071.0;
        }

        // Use exact 1:1 integer storage when all calibrated pixels fit in [-32767, 32767]
        // as integers (standard CT HU); otherwise quantize uniformly into [-32767, 32767].
        let fits_exact_i16 = min_val >= -32767.0
            && max_val <= 32767.0
            && frames.iter().all(|f| match &f.pixels {
                Pixels::Gray(src) => src
                    .iter()
                    .all(|&v| v.is_finite() && (v - v.round()).abs() <= 1e-4),
                Pixels::Rgba(_) => false,
            });
        let (voxel_scale, voxel_offset) = if fits_exact_i16 {
            (1.0f32, 0.0f32)
        } else {
            let offset = 0.5 * (min_val + max_val);
            let scale = ((max_val - min_val) / 65534.0).max(1e-9);
            (scale, offset)
        };
        let inv_scale = 1.0 / voxel_scale;

        let mut voxels = vec![0i16; slice_len * d];
        for (z, f) in frames.iter().enumerate() {
            let dst = &mut voxels[z * slice_len..(z + 1) * slice_len];
            match &f.pixels {
                Pixels::Gray(src) => {
                    if fits_exact_i16 {
                        for (d_px, &s_px) in dst.iter_mut().zip(src.iter()) {
                            *d_px = s_px.round().clamp(-32767.0, 32767.0) as i16;
                        }
                    } else {
                        for (d_px, &s_px) in dst.iter_mut().zip(src.iter()) {
                            *d_px = ((s_px - voxel_offset) * inv_scale)
                                .round()
                                .clamp(-32767.0, 32767.0) as i16;
                        }
                    }
                }
                Pixels::Rgba(rgba) => {
                    for (i, px) in rgba.as_chunks::<4>().0.iter().take(slice_len).enumerate() {
                        let luma =
                            0.299 * (px[0] as f32) + 0.587 * (px[1] as f32) + 0.114 * (px[2] as f32);
                        dst[i] = ((luma - voxel_offset) * inv_scale)
                            .round()
                            .clamp(-32767.0, 32767.0) as i16;
                    }
                }
            }
        }

        let extent_x = (w.saturating_sub(1) as f64) * dx;
        let extent_y = (h.saturating_sub(1) as f64) * dy;
        let extent_z = (d.saturating_sub(1) as f64) * dz;

        let center_lps = add3(
            origin0,
            add3(
                scale3(row_dir, 0.5 * extent_x),
                add3(
                    scale3(col_dir, 0.5 * extent_y),
                    scale3(slice_dir, 0.5 * extent_z),
                ),
            ),
        );
        let bounding_radius_mm = 0.5
            * (extent_x * extent_x + extent_y * extent_y + extent_z * extent_z)
                .sqrt()
                .max(10.0);

        // Estimate foreground threshold (e.g., tissue/bone vs surrounding air) and compute
        // the 3D anatomical center of mass so cross-study alignment ignores empty air margins.
        let fg_thresh = min_val + 0.22 * (max_val - min_val);
        let mut sum_w = 0.0f64;
        let mut sum_pos = [0.0f64; 3];
        let step_z = (d / 32).max(1);
        let step_y = (h / 64).max(1);
        let step_x = (w / 64).max(1);
        for z in (0..d).step_by(step_z) {
            let z_mm = z as f64 * dz;
            for y in (0..h).step_by(step_y) {
                let y_mm = y as f64 * dy;
                let row_base = z * slice_len + y * w;
                for x in (0..w).step_by(step_x) {
                    let v = voxels[row_base + x] as f32 * voxel_scale + voxel_offset;
                    if v > fg_thresh {
                        let weight = (v - fg_thresh) as f64;
                        let x_mm = x as f64 * dx;
                        let pt = add3(
                            origin0,
                            add3(
                                scale3(row_dir, x_mm),
                                add3(scale3(col_dir, y_mm), scale3(slice_dir, z_mm)),
                            ),
                        );
                        sum_pos[0] += pt[0] * weight;
                        sum_pos[1] += pt[1] * weight;
                        sum_pos[2] += pt[2] * weight;
                        sum_w += weight;
                    }
                }
            }
        }
        let foreground_centroid_lps = if sum_w > 1e-3 {
            [
                sum_pos[0] / sum_w,
                sum_pos[1] / sum_w,
                sum_pos[2] / sum_w,
            ]
        } else {
            center_lps
        };

        let ref_idx = d / 2;
        let ref_frame = &frames[ref_idx];
        let full_window = Window {
            level: 0.5 * (min_val + max_val),
            width: (max_val - min_val).max(1.0),
        };

        static NEXT_VOLUME_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Ok(Arc::new(Self {
            gpu_id: NEXT_VOLUME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            width: w,
            height: h,
            depth: d,
            voxels,
            voxel_scale,
            voxel_offset,
            origin_lps: origin0,
            row_dir,
            col_dir,
            slice_dir,
            spacing: [dx, dy, dz],
            center_lps,
            foreground_centroid_lps,
            bounding_radius_mm,
            native_orientation: [
                row_dir[0],
                row_dir[1],
                row_dir[2],
                col_dir[0],
                col_dir[1],
                col_dir[2],
                slice_dir[0],
                slice_dir[1],
                slice_dir[2],
            ],
            stored_window: ref_frame.stored_window,
            full_window,
            bits_window: ref_frame.bits_window,
            inverts: ref_frame.inverts,
            slope: ref_frame.slope,
            frame_of_reference_uid: series.frame_of_reference_uid.clone(),
            value_unit: ref_frame.value_unit,
            foreground_threshold: fg_thresh,
            geometry_verified,
            uncalibrated,
        }))
    }

    /// Converts a 3D point in patient LPS coordinates (mm) to continuous voxel coordinates `(fx, fy, fz)`.
    /// Uses full 3×3 matrix inversion of `(row_dir*sx, col_dir*sy, slice_dir*sz)` so tilted gantries
    /// round-trip accurately with `voxel_to_lps`.
    #[inline]
    pub fn lps_to_voxel(&self, pt_lps: [f64; 3]) -> [f64; 3] {
        let rel = sub3(pt_lps, self.origin_lps);
        let a = scale3(self.row_dir, self.spacing[0]);
        let b = scale3(self.col_dir, self.spacing[1]);
        let c = scale3(self.slice_dir, self.spacing[2]);
        let b_x_c = cross3(b, c);
        let det = dot3(a, b_x_c);
        if det.abs() > 1e-12 {
            let inv_det = 1.0 / det;
            let c_x_a = cross3(c, a);
            let a_x_b = cross3(a, b);
            [
                dot3(rel, b_x_c) * inv_det,
                dot3(rel, c_x_a) * inv_det,
                dot3(rel, a_x_b) * inv_det,
            ]
        } else {
            [
                dot3(rel, self.row_dir) / self.spacing[0],
                dot3(rel, self.col_dir) / self.spacing[1],
                dot3(rel, self.slice_dir) / self.spacing[2],
            ]
        }
    }

    /// Converts continuous voxel coordinates `(fx, fy, fz)` to patient LPS coordinates (mm).
    #[inline]
    pub fn voxel_to_lps(&self, vox: [f64; 3]) -> [f64; 3] {
        add3(
            self.origin_lps,
            add3(
                scale3(self.row_dir, vox[0] * self.spacing[0]),
                add3(
                    scale3(self.col_dir, vox[1] * self.spacing[1]),
                    scale3(self.slice_dir, vox[2] * self.spacing[2]),
                ),
            ),
        )
    }

    /// Trilinearly samples the volume at a 3D patient LPS point `(X, Y, Z)` in mm.
    #[inline]
    pub fn sample_lps(&self, pt_lps: [f64; 3]) -> Option<f32> {
        let [fx, fy, fz] = self.lps_to_voxel(pt_lps);
        if fx < -0.5
            || fy < -0.5
            || fz < -0.5
            || fx > (self.width as f64 - 0.5)
            || fy > (self.height as f64 - 0.5)
            || fz > (self.depth as f64 - 0.5)
        {
            return None;
        }
        let cx = fx.clamp(0.0, (self.width - 1) as f64);
        let cy = fy.clamp(0.0, (self.height - 1) as f64);
        let cz = fz.clamp(0.0, (self.depth - 1) as f64);

        let x0 = cx.floor() as usize;
        let y0 = cy.floor() as usize;
        let z0 = cz.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let z1 = (z0 + 1).min(self.depth - 1);

        let tx = (cx - x0 as f64) as f32;
        let ty = (cy - y0 as f64) as f32;
        let tz = (cz - z0 as f64) as f32;

        let w = self.width;
        let wh = w * self.height;
        let idx = |x: usize, y: usize, z: usize| z * wh + y * w + x;

        let c000 = self.voxel_at(idx(x0, y0, z0));
        let c100 = self.voxel_at(idx(x1, y0, z0));
        let c010 = self.voxel_at(idx(x0, y1, z0));
        let c110 = self.voxel_at(idx(x1, y1, z0));
        let c001 = self.voxel_at(idx(x0, y0, z1));
        let c101 = self.voxel_at(idx(x1, y0, z1));
        let c011 = self.voxel_at(idx(x0, y1, z1));
        let c111 = self.voxel_at(idx(x1, y1, z1));

        let c00 = c000 + tx * (c100 - c000);
        let c10 = c010 + tx * (c110 - c010);
        let c01 = c001 + tx * (c101 - c001);
        let c11 = c011 + tx * (c111 - c011);

        let c0 = c00 + ty * (c10 - c00);
        let c1 = c01 + ty * (c11 - c01);

        Some(c0 + tz * (c1 - c0))
    }

    /// Decode a quantized voxel at linear index `idx` into calibrated greyscale (`f32`).
    #[inline]
    pub fn voxel_at(&self, idx: usize) -> f32 {
        self.voxels[idx] as f32 * self.voxel_scale + self.voxel_offset
    }

    /// Background air value used outside the volume bounds during oblique reslicing.
    pub fn background_value(&self) -> f32 {
        self.full_window.level - 0.5 * self.full_window.width
    }

    /// Computes the half-span (in mm) of the volume projected along any unit direction `dir_lps`.
    pub fn projected_half_span_mm(&self, dir_lps: [f64; 3]) -> f64 {
        let ex = (self.width.saturating_sub(1) as f64) * self.spacing[0];
        let ey = (self.height.saturating_sub(1) as f64) * self.spacing[1];
        let ez = (self.depth.saturating_sub(1) as f64) * self.spacing[2];
        0.5 * (ex * dot3(self.row_dir, dir_lps).abs()
            + ey * dot3(self.col_dir, dir_lps).abs()
            + ez * dot3(self.slice_dir, dir_lps).abs())
        .max(10.0)
    }

    /// Precomputed 3D plane geometry and voxel-space affine mapping for CPU or GPU MPR reslicing.
    pub fn mpr_plane_geometry(&self, plane: &MprPlane3D) -> MprPlaneGeometry {
        let native_aligned = dot3(plane.u_axis, self.row_dir) > 0.999
            && dot3(plane.v_axis, self.col_dir) > 0.999
            && dot3(plane.normal, self.slice_dir).abs() > 0.999;

        let (out_w, out_h, out_sp_u, out_sp_v) = if native_aligned {
            (self.width, self.height, self.spacing[0], self.spacing[1])
        } else {
            let ex = (self.width.saturating_sub(1) as f64) * self.spacing[0];
            let ey = (self.height.saturating_sub(1) as f64) * self.spacing[1];
            let ez = (self.depth.saturating_sub(1) as f64) * self.spacing[2];
            let fov_u = (ex * dot3(self.row_dir, plane.u_axis).abs()
                + ey * dot3(self.col_dir, plane.u_axis).abs()
                + ez * dot3(self.slice_dir, plane.u_axis).abs())
            .max(40.0);
            let fov_v = (ex * dot3(self.row_dir, plane.v_axis).abs()
                + ey * dot3(self.col_dir, plane.v_axis).abs()
                + ez * dot3(self.slice_dir, plane.v_axis).abs())
            .max(40.0);
            let min_sp = self.spacing[0]
                .min(self.spacing[1])
                .min(self.spacing[2])
                .clamp(0.25, 1.5);
            let out_sp = min_sp.max(fov_u.max(fov_v) / 1023.0);
            let w = ((fov_u / out_sp).round() as usize + 1).clamp(128, 1024);
            let h = ((fov_v / out_sp).round() as usize + 1).clamp(128, 1024);
            (w, h, out_sp, out_sp)
        };
        let out_sp = out_sp_u.min(out_sp_v);
        let min_sp = out_sp.clamp(0.25, 1.5);

        let slice_center = add3(plane.center_lps, scale3(plane.normal, plane.offset_mm));
        let half_ext_u = 0.5 * (out_w.saturating_sub(1) as f64) * out_sp_u;
        let half_ext_v = 0.5 * (out_h.saturating_sub(1) as f64) * out_sp_v;
        let top_left_lps = sub3(
            sub3(slice_center, scale3(plane.u_axis, half_ext_u)),
            scale3(plane.v_axis, half_ext_v),
        );
        let u_step_lps = scale3(plane.u_axis, out_sp_u);
        let v_step_lps = scale3(plane.v_axis, out_sp_v);

        let vox_origin_d = self.lps_to_voxel(top_left_lps);
        let vox_u_d = sub3(self.lps_to_voxel(add3(top_left_lps, u_step_lps)), vox_origin_d);
        let vox_v_d = sub3(self.lps_to_voxel(add3(top_left_lps, v_step_lps)), vox_origin_d);

        let (slab_steps, vox_slab_start, vox_slab_delta) =
            if plane.slab_mode != SlabMode::Thin && plane.slab_thickness_mm > out_sp {
                let step_mm = (min_sp * 0.85).clamp(0.5, 1.5);
                let half_slab = 0.5 * plane.slab_thickness_mm;
                let steps = ((plane.slab_thickness_mm / step_mm).ceil() as usize).clamp(3, 65);
                let start_lps = scale3(plane.normal, -half_slab);
                let delta_lps =
                    scale3(plane.normal, plane.slab_thickness_mm / (steps - 1) as f64);
                let start_d =
                    sub3(self.lps_to_voxel(add3(top_left_lps, start_lps)), vox_origin_d);
                let delta_d =
                    sub3(self.lps_to_voxel(add3(top_left_lps, delta_lps)), vox_origin_d);
                (
                    steps as u32,
                    [start_d[0] as f32, start_d[1] as f32, start_d[2] as f32],
                    [delta_d[0] as f32, delta_d[1] as f32, delta_d[2] as f32],
                )
            } else {
                (1u32, [0.0f32; 3], [0.0f32; 3])
            };

        let eff_thickness = if slab_steps > 1 {
            plane.slab_thickness_mm
        } else if native_aligned {
            self.spacing[2]
        } else {
            out_sp
        };

        let orientation = [
            plane.u_axis[0],
            plane.u_axis[1],
            plane.u_axis[2],
            plane.v_axis[0],
            plane.v_axis[1],
            plane.v_axis[2],
            plane.normal[0],
            plane.normal[1],
            plane.normal[2],
        ];
        let slice_location = Some(dot3(slice_center, plane.normal));

        MprPlaneGeometry {
            width: out_w as u32,
            height: out_h as u32,
            out_sp_u,
            out_sp_v,
            top_left_lps,
            u_step_lps,
            v_step_lps,
            slice_center,
            native_aligned,
            vox_origin: [vox_origin_d[0] as f32, vox_origin_d[1] as f32, vox_origin_d[2] as f32],
            vox_u_step: [vox_u_d[0] as f32, vox_u_d[1] as f32, vox_u_d[2] as f32],
            vox_v_step: [vox_v_d[0] as f32, vox_v_d[1] as f32, vox_v_d[2] as f32],
            slab_mode: plane.slab_mode,
            slab_steps,
            vox_slab_start,
            vox_slab_delta,
            eff_thickness,
            orientation,
            slice_location,
        }
    }

    /// Constructs a `Frame` with the geometry and windowing metadata of `geom` and the given `pixels`.
    pub fn frame_with_pixels(&self, geom: &MprPlaneGeometry, pixels: Vec<f32>) -> Frame {
        Frame {
            width: geom.width,
            height: geom.height,
            pixels: Pixels::Gray(pixels),
            stored_window: self.stored_window,
            full_window: self.full_window,
            bits_window: self.bits_window,
            inverts: self.inverts,
            voi_lut_applied: false,
            slope: self.slope,
            spacing: if self.uncalibrated {
                None
            } else {
                Some((geom.out_sp_v, geom.out_sp_u))
            },
            pixel_ratio: 1.0,
            origin: Some(geom.top_left_lps),
            orientation: geom.orientation,
            is_3d_plane: true,
            frame_of_reference_uid: self.frame_of_reference_uid.clone(),
            slice_thickness: Some(geom.eff_thickness),
            slice_location: geom.slice_location,
            value_unit: self.value_unit,
        }
    }

    /// Samples a single pixel `(px, py)` on a GPU-resliced MPR plane for live cursor readout.
    pub fn sample_plane_at(&self, geom: &MprPlaneGeometry, px: f64, py: f64) -> Option<f32> {
        if px < 0.0 || py < 0.0 || px >= geom.width as f64 || py >= geom.height as f64 {
            return None;
        }
        let pt = add3(
            geom.top_left_lps,
            add3(scale3(geom.u_step_lps, px), scale3(geom.v_step_lps, py)),
        );
        self.sample_lps(pt)
    }

    /// Reslices the 3D volume (and any sibling orthogonal volumes from the same acquisition group)
    /// along an arbitrary 3D plane `plane`, returning a calibrated `Arc<Frame>`.
    pub fn reslice_plane(&self, siblings: &[Arc<Volume3D>], plane: &MprPlane3D) -> Arc<Frame> {
        let geom = self.mpr_plane_geometry(plane);
        let out_w = geom.width as usize;
        let out_h = geom.height as usize;
        let out_sp_u = geom.out_sp_u;
        let out_sp_v = geom.out_sp_v;
        let out_sp = out_sp_u.min(out_sp_v);
        let min_sp = out_sp.clamp(0.25, 1.5);
        let slice_center = geom.slice_center;
        let top_left_lps = geom.top_left_lps;
        let u_step = geom.u_step_lps;
        let v_step = geom.v_step_lps;
        let native_aligned = geom.native_aligned;
        let bg = self.background_value();

        // Fast exact path (A1): when the plane is native-aligned, Thin-slab, and sits on a
        // source slice position, return the source slice's calibrated values bit-identically
        // with zero trilinear resampling and zero sibling blending.
        let exact_native_slice = if native_aligned && plane.slab_mode == SlabMode::Thin {
            let vox = self.lps_to_voxel(slice_center);
            let z_round = vox[2].round();
            if (vox[2] - z_round).abs() < 1e-3
                && z_round >= 0.0
                && (z_round as usize) < self.depth
            {
                Some(z_round as usize)
            } else {
                None
            }
        } else {
            None
        };

        // Precompute thick-slab normal offsets when `slab_mode != Thin` and `slab_thickness_mm > out_sp`.
        let slab_offsets: Vec<[f64; 3]> =
            if plane.slab_mode != SlabMode::Thin && plane.slab_thickness_mm > out_sp {
                let step_mm = (min_sp * 0.85).clamp(0.5, 1.5);
                let half_slab = 0.5 * plane.slab_thickness_mm;
                let steps = ((plane.slab_thickness_mm / step_mm).ceil() as usize).clamp(3, 65);
                (0..steps)
                    .map(|i| {
                        let t = if steps > 1 {
                            (i as f64 / (steps - 1) as f64) * 2.0 - 1.0
                        } else {
                            0.0
                        };
                        scale3(plane.normal, t * half_slab)
                    })
                    .collect()
            } else {
                Vec::new()
            };

        let pixels = if let Some(z_idx) = exact_native_slice {
            let slice_len = self.width * self.height;
            let start = z_idx * slice_len;
            if self.voxel_scale == 1.0 && self.voxel_offset == 0.0 {
                self.voxels[start..start + slice_len]
                    .iter()
                    .map(|&q| q as f32)
                    .collect()
            } else {
                self.voxels[start..start + slice_len]
                    .iter()
                    .map(|&q| q as f32 * self.voxel_scale + self.voxel_offset)
                    .collect()
            }
        } else {
            // Compute resolution weights for this volume and any explicitly passed sibling volumes.
            let mut sources: Vec<(&Volume3D, f32)> = Vec::with_capacity(1 + siblings.len());
            let weight_for = |v: &Volume3D| -> f32 {
                let align = dot3(v.slice_dir, plane.normal).abs();
                let eff_sp =
                    (1.0 - align) * v.spacing[2] + align * v.spacing[0].min(v.spacing[1]);
                (1.0 / (eff_sp * eff_sp).max(0.05)) as f32
            };
            sources.push((self, weight_for(self)));
            for sib in siblings {
                if !std::ptr::eq(sib.as_ref(), self) {
                    sources.push((sib.as_ref(), weight_for(sib)));
                }
            }

            let sample_hybrid = |pt: [f64; 3]| -> Option<f32> {
                if sources.len() == 1 {
                    self.sample_lps(pt)
                } else {
                    let mut w_sum = 0.0f32;
                    let mut v_sum = 0.0f32;
                    for &(vol, w) in &sources {
                        if let Some(val) = vol.sample_lps(pt) {
                            v_sum += val * w;
                            w_sum += w;
                        }
                    }
                    if w_sum > 0.0 {
                        Some(v_sum / w_sum)
                    } else {
                        None
                    }
                }
            };

            let mut buf = vec![bg; out_w * out_h];
            let slab_mode = plane.slab_mode;
            buf.par_chunks_mut(out_w)
                .enumerate()
                .for_each(|(y, row_buf)| {
                    let row_start = add3(top_left_lps, scale3(v_step, y as f64));
                    for (x, dst) in row_buf.iter_mut().enumerate() {
                        let pt = add3(row_start, scale3(u_step, x as f64));
                        if slab_offsets.is_empty() {
                            if let Some(val) = sample_hybrid(pt) {
                                *dst = val;
                            }
                        } else {
                            match slab_mode {
                                SlabMode::Thin => {
                                    if let Some(val) = sample_hybrid(pt) {
                                        *dst = val;
                                    }
                                }
                                SlabMode::Mip => {
                                    let mut best = f32::MIN;
                                    let mut any = false;
                                    for &off in &slab_offsets {
                                        if let Some(val) = sample_hybrid(add3(pt, off)) {
                                            if val > best {
                                                best = val;
                                            }
                                            any = true;
                                        }
                                    }
                                    if any {
                                        *dst = best;
                                    }
                                }
                                SlabMode::MinIp => {
                                    let mut best = f32::MAX;
                                    let mut any = false;
                                    for &off in &slab_offsets {
                                        if let Some(val) = sample_hybrid(add3(pt, off)) {
                                            if val < best {
                                                best = val;
                                            }
                                            any = true;
                                        }
                                    }
                                    if any {
                                        *dst = best;
                                    }
                                }
                                SlabMode::Mean => {
                                    let mut sum = 0.0f32;
                                    let mut cnt = 0u32;
                                    for &off in &slab_offsets {
                                        if let Some(val) = sample_hybrid(add3(pt, off)) {
                                            sum += val;
                                            cnt += 1;
                                        }
                                    }
                                    if cnt > 0 {
                                        *dst = sum / (cnt as f32);
                                    }
                                }
                            }
                        }
                    }
                });
            buf
        };

        Arc::new(self.frame_with_pixels(&geom, pixels))
    }
}

/// Precomputed 3D plane geometry and voxel-space affine mapping for CPU or GPU MPR reslicing.
#[derive(Clone, Debug)]
pub struct MprPlaneGeometry {
    pub width: u32,
    pub height: u32,
    pub out_sp_u: f64,
    pub out_sp_v: f64,
    pub top_left_lps: [f64; 3],
    pub u_step_lps: [f64; 3],
    pub v_step_lps: [f64; 3],
    pub slice_center: [f64; 3],
    pub native_aligned: bool,
    pub vox_origin: [f32; 3],
    pub vox_u_step: [f32; 3],
    pub vox_v_step: [f32; 3],
    pub slab_mode: SlabMode,
    pub slab_steps: u32,
    pub vox_slab_start: [f32; 3],
    pub vox_slab_delta: [f32; 3],
    pub eff_thickness: f64,
    pub orientation: [f64; 9],
    pub slice_location: Option<f64>,
}

/// Thick-slab projection mode for 3D MPR (`IsiXImaging` / `ViewerReconstruction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SlabMode {
    #[default]
    Thin,
    Mip,
    MinIp,
    Mean,
}

impl SlabMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Thin => "Thin",
            Self::Mip => "MIP",
            Self::MinIp => "MinIP",
            Self::Mean => "Mean",
        }
    }
}

/// Interactive 3D slicing plane state for a single viewport in In-Place 3D MPR mode.
#[derive(Clone, Debug, PartialEq)]
pub struct MprPlane3D {
    pub center_lps: [f64; 3],
    pub u_axis: [f64; 3],
    pub v_axis: [f64; 3],
    pub normal: [f64; 3],
    pub offset_mm: f64,
    pub slab_mode: SlabMode,
    pub slab_thickness_mm: f64,
}

impl MprPlane3D {
    /// Initializes the 3D plane from a volume's native acquisition orientation,
    /// aligning `normal` with `vol.slice_dir` so slice `0` stays `0` and `depth - 1` stays `depth - 1`.
    pub fn from_volume_native(vol: &Volume3D) -> Self {
        let o = vol.native_orientation;
        let u = normalize3([o[0], o[1], o[2]]);
        let v = normalize3([o[3], o[4], o[5]]);
        let cross_n = normalize3(cross3(u, v));
        let n = if dot3(cross_n, vol.slice_dir) < 0.0 {
            scale3(cross_n, -1.0)
        } else {
            cross_n
        };
        Self {
            center_lps: vol.center_lps,
            u_axis: u,
            v_axis: v,
            normal: n,
            offset_mm: 0.0,
            slab_mode: SlabMode::Thin,
            slab_thickness_mm: 10.0,
        }
    }

    /// Standard radiological Axial plane (`u = +X [R->L]`, `v = +Y [A->P]`, `n = +Z [I->S]`).
    pub fn axial(center_lps: [f64; 3]) -> Self {
        Self {
            center_lps,
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            offset_mm: 0.0,
            slab_mode: SlabMode::Thin,
            slab_thickness_mm: 10.0,
        }
    }

    /// Standard radiological Coronal plane (`u = +X [R->L]`, `v = -Z [S->I]`, `n = +Y [A->P]`).
    pub fn coronal(center_lps: [f64; 3]) -> Self {
        Self {
            center_lps,
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, -1.0],
            normal: [0.0, 1.0, 0.0],
            offset_mm: 0.0,
            slab_mode: SlabMode::Thin,
            slab_thickness_mm: 10.0,
        }
    }

    /// Standard radiological Sagittal plane (`u = +Y [A->P]`, `v = -Z [S->I]`, `n = -X [L->R]`).
    pub fn sagittal(center_lps: [f64; 3]) -> Self {
        Self {
            center_lps,
            u_axis: [0.0, 1.0, 0.0],
            v_axis: [0.0, 0.0, -1.0],
            normal: [-1.0, 0.0, 0.0],
            offset_mm: 0.0,
            slab_mode: SlabMode::Thin,
            slab_thickness_mm: 10.0,
        }
    }

    /// Rotates the 3D slicing plane in-place (3D trackball / orbit):
    /// - `yaw_deg`: horizontal drag rotates around the plane's vertical axis `v_axis`
    /// - `pitch_deg`: vertical drag rotates around the plane's horizontal axis `u_axis`
    /// - `roll_deg`: in-plane tilt around `normal`
    pub fn orbit_3d(&mut self, yaw_deg: f64, pitch_deg: f64, roll_deg: f64) {
        if yaw_deg.abs() > 1e-6 {
            let r = yaw_deg.to_radians();
            self.u_axis = rotate_around_axis(self.u_axis, self.v_axis, r);
            self.normal = rotate_around_axis(self.normal, self.v_axis, r);
        }
        if pitch_deg.abs() > 1e-6 {
            let r = pitch_deg.to_radians();
            self.v_axis = rotate_around_axis(self.v_axis, self.u_axis, r);
            self.normal = rotate_around_axis(self.normal, self.u_axis, r);
        }
        if roll_deg.abs() > 1e-6 {
            let r = roll_deg.to_radians();
            self.u_axis = rotate_around_axis(self.u_axis, self.normal, r);
            self.v_axis = rotate_around_axis(self.v_axis, self.normal, r);
        }
        self.orthonormalize();
    }

    pub fn orthonormalize(&mut self) {
        self.u_axis = normalize3(self.u_axis);
        let dot_uv = dot3(self.v_axis, self.u_axis);
        self.v_axis = normalize3(sub3(self.v_axis, scale3(self.u_axis, dot_uv)));
        self.normal = normalize3(cross3(self.u_axis, self.v_axis));
    }

    /// Current 3D point at the center of the active slice (`center_lps + offset_mm * normal`).
    pub fn slice_center_lps(&self) -> [f64; 3] {
        add3(self.center_lps, scale3(self.normal, self.offset_mm))
    }
}

/// 6-Degree-of-Freedom 3D Rigid Registration transform mapping a 3D plane or point
/// in `source` (Study A) to the corresponding anatomical 3D plane or point in `target` (Study B).
#[derive(Clone, Debug)]
pub struct RigidTransform3D {
    pub pivot_source_lps: [f64; 3],
    pub pivot_target_lps: [f64; 3],
    /// Orthogonal 3×3 rotation matrix mapping directions in `source` LPS to `target` LPS.
    pub rotation: [[f64; 3]; 3],
    /// Normalized cross-correlation score (`-1.0..=1.0`) achieved after registration.
    pub correlation: f64,
}

impl RigidTransform3D {
    /// Exact identity transform (e.g., when both volumes share `FrameOfReferenceUID`).
    pub fn identity(correlation: f64) -> Self {
        Self {
            pivot_source_lps: [0.0, 0.0, 0.0],
            pivot_target_lps: [0.0, 0.0, 0.0],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            correlation,
        }
    }

    /// Exact inverse transform (`target -> source`) using transpose of the orthogonal rotation.
    pub fn inverse(&self) -> Self {
        let r = self.rotation;
        Self {
            pivot_source_lps: self.pivot_target_lps,
            pivot_target_lps: self.pivot_source_lps,
            rotation: [
                [r[0][0], r[1][0], r[2][0]],
                [r[0][1], r[1][1], r[2][1]],
                [r[0][2], r[1][2], r[2][2]],
            ],
            correlation: self.correlation,
        }
    }

    /// Returns `true` when the registration score is high enough to trust 3D alignment.
    pub fn is_reliable(&self) -> bool {
        self.correlation >= 0.45
    }

    /// Identity alignment around the volumes' foreground centroids (translation-only baseline).
    pub fn centroid_only(source: &Volume3D, target: &Volume3D) -> Self {
        Self {
            pivot_source_lps: source.foreground_centroid_lps,
            pivot_target_lps: target.foreground_centroid_lps,
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            correlation: 0.0,
        }
    }

    /// Automatically registers `target` (Volume B) to `source` (Volume A) in 3D space
    /// by optimizing 3D translation `(dx, dy, dz)` and 3D rotation `(rx, ry, rz)`
    /// to maximize Normalized Cross-Correlation (NCC) of foreground voxels.
    pub fn register(source: &Volume3D, target: &Volume3D) -> Self {
        if let (Some(a), Some(b)) = (
            source.frame_of_reference_uid.as_deref(),
            target.frame_of_reference_uid.as_deref(),
        )
            && !a.is_empty() && a == b {
                return Self::identity(1.0);
            }
        let pivot_a = source.foreground_centroid_lps;
        let pivot_b_init = target.foreground_centroid_lps;

        // Collect a representative set of ~2,500 foreground sample points in `source`
        // (relative to `pivot_a`) and their intensities.
        let mut samples: Vec<([f64; 3], f32)> = Vec::with_capacity(3000);
        let step_z = (source.depth / 14).max(1);
        let step_y = (source.height / 18).max(1);
        let step_x = (source.width / 18).max(1);
        let wh = source.width * source.height;
        for z in (0..source.depth).step_by(step_z) {
            for y in (0..source.height).step_by(step_y) {
                for x in (0..source.width).step_by(step_x) {
                    let val = source.voxel_at(z * wh + y * source.width + x);
                    if val > source.foreground_threshold {
                        let pt_lps = source.voxel_to_lps([x as f64, y as f64, z as f64]);
                        samples.push((sub3(pt_lps, pivot_a), val));
                    }
                }
            }
        }

        if samples.len() < 32 {
            return Self::centroid_only(source, target);
        }

        // Precompute mean and variance of source samples for fast NCC evaluation.
        let n = samples.len() as f64;
        let mean_a: f64 = samples.iter().map(|(_, v)| *v as f64).sum::<f64>() / n;
        let var_a: f64 = samples
            .iter()
            .map(|(_, v)| {
                let d = *v as f64 - mean_a;
                d * d
            })
            .sum::<f64>()
            .max(1e-6);

        // Evaluates Normalized Cross-Correlation for parameter vector `[tx, ty, tz, rx_deg, ry_deg, rz_deg]`.
        let eval_ncc = |params: &[f64; 6]| -> f64 {
            let rot = euler_deg_to_matrix(params[3], params[4], params[5]);
            let trans = [
                pivot_b_init[0] + params[0],
                pivot_b_init[1] + params[1],
                pivot_b_init[2] + params[2],
            ];
            let mut sum_b = 0.0f64;
            let mut sum_b2 = 0.0f64;
            let mut sum_ab = 0.0f64;
            let mut valid = 0usize;
            let bg_b = target.background_value() as f64;

            for &(rel_a, val_a) in &samples {
                let da = val_a as f64 - mean_a;
                let rot_p = mat3_mul_vec(rot, rel_a);
                let pt_b = add3(trans, rot_p);
                let vb = match target.sample_lps(pt_b) {
                    Some(v) => {
                        valid += 1;
                        v as f64
                    }
                    None => bg_b,
                };
                sum_b += vb;
                sum_b2 += vb * vb;
                sum_ab += da * vb;
            }
            if valid < samples.len() / 3 {
                return -1.0;
            }
            let mean_b = sum_b / n;
            let var_b = (sum_b2 - n * mean_b * mean_b).max(1e-6);
            // Note: sum(da) == 0, so sum((a - mean_a)*(b - mean_b)) == sum(da * b).
            sum_ab / (var_a * var_b).sqrt()
        };

        // Multi-scale coarse-to-fine coordinate pattern search over (tx, ty, tz, rx, ry, rz).
        let mut best_params = [0.0f64; 6];
        let mut best_score = eval_ncc(&best_params);

        let scales: &[(f64, f64, usize)] = &[
            (10.0, 8.0, 2), // Coarse: 10 mm translation step, 8° rotation step
            (4.0, 3.5, 2),  // Medium: 4 mm translation step, 3.5° rotation step
            (1.5, 1.2, 2),  // Fine: 1.5 mm translation step, 1.2° rotation step
            (0.5, 0.4, 1),  // Sub-millimeter / sub-degree polish
        ];

        for &(t_step, r_step, passes) in scales {
            let steps = [t_step, t_step, t_step, r_step, r_step, r_step];
            for _ in 0..passes {
                let mut improved = false;
                for dim in 0..6 {
                    let s = steps[dim];
                    let mut cand_plus = best_params;
                    cand_plus[dim] += s;
                    let score_plus = eval_ncc(&cand_plus);

                    let mut cand_minus = best_params;
                    cand_minus[dim] -= s;
                    let score_minus = eval_ncc(&cand_minus);

                    if score_plus > best_score && score_plus >= score_minus {
                        best_score = score_plus;
                        best_params = cand_plus;
                        improved = true;
                    } else if score_minus > best_score {
                        best_score = score_minus;
                        best_params = cand_minus;
                        improved = true;
                    }
                }
                if !improved {
                    break;
                }
            }
        }

        let rotation = euler_deg_to_matrix(best_params[3], best_params[4], best_params[5]);
        let pivot_target_lps = [
            pivot_b_init[0] + best_params[0],
            pivot_b_init[1] + best_params[1],
            pivot_b_init[2] + best_params[2],
        ];

        Self {
            pivot_source_lps: pivot_a,
            pivot_target_lps,
            rotation,
            correlation: best_score,
        }
    }

    /// Maps a 3D point in `source` LPS coordinates to the registered 3D point in `target` LPS coordinates.
    pub fn transform_point(&self, pt_source_lps: [f64; 3]) -> [f64; 3] {
        let rel = sub3(pt_source_lps, self.pivot_source_lps);
        add3(self.pivot_target_lps, mat3_mul_vec(self.rotation, rel))
    }

    /// Maps a 3D slicing plane `source_plane` from `source` (Study A) into the corresponding
    /// 3D slicing plane in `target` (Study B), preserving both 3D slice position and 3D orientation.
    pub fn transform_plane(&self, source_plane: &MprPlane3D) -> MprPlane3D {
        let center_target = self.transform_point(source_plane.center_lps);
        let u_target = normalize3(mat3_mul_vec(self.rotation, source_plane.u_axis));
        let v_target = normalize3(mat3_mul_vec(self.rotation, source_plane.v_axis));
        let n_target = normalize3(cross3(u_target, v_target));
        MprPlane3D {
            center_lps: center_target,
            u_axis: u_target,
            v_axis: v_target,
            normal: n_target,
            offset_mm: source_plane.offset_mm,
            slab_mode: source_plane.slab_mode,
            slab_thickness_mm: source_plane.slab_thickness_mm,
        }
    }
}

// ---------------------------------------------------------------------------
// 3D Vector & Matrix Helpers
// ---------------------------------------------------------------------------

#[inline]
pub fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub fn len3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

#[inline]
pub fn normalize3(a: [f64; 3]) -> [f64; 3] {
    let l = len3(a);
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// Rodrigues' rotation formula: rotates `v` around unit axis `k` by `angle_rad`.
#[inline]
fn rotate_around_axis(v: [f64; 3], k: [f64; 3], angle_rad: f64) -> [f64; 3] {
    let axis = normalize3(k);
    let (s, c) = angle_rad.sin_cos();
    let k_cross_v = cross3(axis, v);
    let k_dot_v = dot3(axis, v);
    [
        v[0] * c + k_cross_v[0] * s + axis[0] * k_dot_v * (1.0 - c),
        v[1] * c + k_cross_v[1] * s + axis[1] * k_dot_v * (1.0 - c),
        v[2] * c + k_cross_v[2] * s + axis[2] * k_dot_v * (1.0 - c),
    ]
}

#[inline]
pub fn mat3_mul_vec(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[inline]
fn euler_deg_to_matrix(rx_deg: f64, ry_deg: f64, rz_deg: f64) -> [[f64; 3]; 3] {
    let (sx, cx) = rx_deg.to_radians().sin_cos();
    let (sy, cy) = ry_deg.to_radians().sin_cos();
    let (sz, cz) = rz_deg.to_radians().sin_cos();
    [
        [cy * cz, cz * sx * sy - cx * sz, cx * cz * sy + sx * sz],
        [cy * sz, cx * cz + sx * sy * sz, cx * sy * sz - cz * sx],
        [-sy, cy * sx, cx * cy],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_3d_preserves_orthonormality() {
        let mut p = MprPlane3D::axial([0.0, 0.0, 0.0]);
        p.orbit_3d(23.5, -14.2, 7.8);
        assert!((len3(p.u_axis) - 1.0).abs() < 1e-9);
        assert!((len3(p.v_axis) - 1.0).abs() < 1e-9);
        assert!((len3(p.normal) - 1.0).abs() < 1e-9);
        assert!(dot3(p.u_axis, p.v_axis).abs() < 1e-9);
        assert!(dot3(p.u_axis, p.normal).abs() < 1e-9);
        assert!(dot3(p.v_axis, p.normal).abs() < 1e-9);
    }

    #[test]
    fn forced_volume_accepts_single_slice_while_standard_requires_three() {
        let frame = Arc::new(Frame {
            width: 4,
            height: 4,
            pixels: Pixels::Gray(vec![0.0; 16]),
            stored_window: None,
            full_window: Window {
                level: 0.0,
                width: 100.0,
            },
            bits_window: None,
            inverts: false,
            voi_lut_applied: false,
            slope: 1.0,
            spacing: Some((1.0, 1.0)),
            pixel_ratio: 1.0,
            origin: Some([0.0, 0.0, 0.0]),
            orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            is_3d_plane: true,
            frame_of_reference_uid: None,
            slice_thickness: Some(2.5),
            slice_location: Some(0.0),
            value_unit: "HU",
        });
        let series = Series {
            uid: "1.2.3".into(),
            number: 1,
            description: "Single Slice".into(),
            modality: "CT".into(),
            rows: 4,
            columns: 4,
            normal: None,
            dominant_plane: crate::dicom_index::DominantPlane::Axial,
            frame_of_reference_uid: None,
            acquisition_time_secs: None,
            acquisition_number: None,
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            mpr_group_id: None,
            slices: vec![crate::dicom_index::SliceRef {
                path: std::path::PathBuf::from("dummy.dcm"),
                frame: 0,
                instance: 1,
                position: Some([0.0, 0.0, 0.0]),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                spacing: Some((1.0, 1.0)),
                slice_location: Some(0.0),
                frame_of_reference_uid: None,
            }],
        };
        let frames = vec![frame];
        assert!(Volume3D::from_series(&series, &frames).is_none());
        let forced = Volume3D::from_series_with_force(&series, &frames, true)
            .expect("forced volume should build for 1 slice");
        assert_eq!(forced.depth, 1);
        assert_eq!(forced.width, 4);
        assert_eq!(forced.height, 4);
    }

    #[test]
    fn native_plane_mpr_is_bit_identical_to_source_slices_even_with_siblings() {
        let make_frame = |z: usize, val_offset: f32| {
            let pixels = (0..16)
                .map(|i| val_offset + (z * 100 + i) as f32)
                .collect::<Vec<_>>();
            Arc::new(Frame {
                width: 4,
                height: 4,
                pixels: Pixels::Gray(pixels),
                stored_window: None,
                full_window: Window {
                    level: 50.0,
                    width: 400.0,
                },
                bits_window: None,
                inverts: false,
                voi_lut_applied: false,
                slope: 1.0,
                spacing: Some((0.7, 0.7)),
                pixel_ratio: 1.0,
                origin: Some([0.0, 0.0, z as f64 * 1.5]),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                frame_of_reference_uid: None,
                slice_thickness: Some(1.5),
                slice_location: Some(z as f64 * 1.5),
                value_unit: "HU",
            })
        };
        let frames: Vec<Arc<Frame>> = (0..4).map(|z| make_frame(z, 0.0)).collect();
        let sib_frames: Vec<Arc<Frame>> = (0..4).map(|z| make_frame(z, 5000.0)).collect();
        let make_series = |fs: &[Arc<Frame>]| Series {
            uid: "1.2.3".into(),
            number: 1,
            description: "Test".into(),
            modality: "CT".into(),
            rows: 4,
            columns: 4,
            normal: Some([0.0, 0.0, 1.0]),
            dominant_plane: crate::dicom_index::DominantPlane::Axial,
            frame_of_reference_uid: None,
            acquisition_time_secs: None,
            acquisition_number: None,
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            mpr_group_id: None,
            slices: fs
                .iter()
                .enumerate()
                .map(|(z, f)| crate::dicom_index::SliceRef {
                    path: std::path::PathBuf::from(format!("s{z}.dcm")),
                    frame: 0,
                    instance: z as i64 + 1,
                    position: f.origin,
                    orientation: f.orientation,
                    is_3d_plane: true,
                    spacing: f.spacing,
                    slice_location: f.slice_location,
                    frame_of_reference_uid: None,
                })
                .collect(),
        };
        let vol = Volume3D::from_series(&make_series(&frames), &frames).unwrap();
        let sib = Volume3D::from_series(&make_series(&sib_frames), &sib_frames).unwrap();

        for (k, frame) in frames.iter().enumerate().take(4) {
            let mut plane = MprPlane3D::from_volume_native(&vol);
            let z_lps = vol.voxel_to_lps([2.0, 2.0, k as f64]);
            plane.offset_mm = dot3(sub3(z_lps, plane.center_lps), plane.normal);
            let resliced = vol.reslice_plane(std::slice::from_ref(&sib), &plane);
            let Pixels::Gray(out) = &resliced.pixels else {
                panic!("expected gray");
            };
            let Pixels::Gray(src) = &frame.pixels else {
                panic!("expected gray");
            };
            assert_eq!(out, src, "slice {k} must be bit-identical");
            assert_eq!(resliced.slice_thickness, Some(1.5));
        }

        let mut oblique = MprPlane3D::from_volume_native(&vol);
        oblique.orbit_3d(25.0, 15.0, 0.0);
        let no_sib = vol.reslice_plane(&[], &oblique);
        let with_empty = vol.reslice_plane(&[], &oblique);
        let (Pixels::Gray(a), Pixels::Gray(b)) = (&no_sib.pixels, &with_empty.pixels) else {
            panic!("expected gray");
        };
        assert_eq!(a, b);
    }

    #[test]
    fn three_series_group_switches_to_native_sagittal_and_reslices_when_absent() {
        let make_frame = |vals: Vec<f32>, orig: [f64; 3], orient: [f64; 9]| {
            Arc::new(Frame {
                width: 4,
                height: 4,
                pixels: Pixels::Gray(vals),
                stored_window: None,
                full_window: Window { level: 50.0, width: 100.0 },
                bits_window: None,
                inverts: false,
                voi_lut_applied: false,
                slope: 1.0,
                spacing: Some((1.0, 1.0)),
                pixel_ratio: 1.0,
                origin: Some(orig),
                orientation: orient,
                is_3d_plane: true,
                frame_of_reference_uid: None,
                slice_thickness: Some(1.0),
                slice_location: None,
                value_unit: "HU",
            })
        };
        let make_series = |fs: &[Arc<Frame>], dp: crate::dicom_index::DominantPlane| Series {
            uid: "1.2.3".into(),
            number: 1,
            description: String::new(),
            modality: "CT".into(),
            rows: 4,
            columns: 4,
            normal: None,
            dominant_plane: dp,
            frame_of_reference_uid: None,
            acquisition_time_secs: None,
            acquisition_number: None,
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            mpr_group_id: Some(1),
            slices: fs
                .iter()
                .enumerate()
                .map(|(i, f)| crate::dicom_index::SliceRef {
                    path: std::path::PathBuf::from(format!("s_{i}.dcm")),
                    frame: 0,
                    instance: i as i64 + 1,
                    position: f.origin,
                    orientation: f.orientation,
                    is_3d_plane: true,
                    spacing: f.spacing,
                    slice_location: None,
                    frame_of_reference_uid: None,
                })
                .collect(),
        };

        let ax_orient = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let cor_orient = [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0];
        let sag_orient = [0.0, 1.0, 0.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0];

        let ax_frames: Vec<_> = (0..4)
            .map(|k| make_frame(vec![10.0 + k as f32; 16], [0.0, 0.0, k as f64], ax_orient))
            .collect();
        let cor_frames: Vec<_> = (0..4)
            .map(|k| make_frame(vec![200.0 + k as f32; 16], [0.0, k as f64, 3.0], cor_orient))
            .collect();
        let sag_frames: Vec<_> = (0..4)
            .map(|k| {
                let vals: Vec<f32> = (0..16).map(|p| 500.0 + (k * 16 + p) as f32).collect();
                make_frame(vals, [k as f64, 0.0, 3.0], sag_orient)
            })
            .collect();

        let ax_vol = Volume3D::from_series(&make_series(&ax_frames, crate::dicom_index::DominantPlane::Axial), &ax_frames).unwrap();
        let cor_vol = Volume3D::from_series(&make_series(&cor_frames, crate::dicom_index::DominantPlane::Coronal), &cor_frames).unwrap();
        let sag_vol = Volume3D::from_series(&make_series(&sag_frames, crate::dicom_index::DominantPlane::Sagittal), &sag_frames).unwrap();

        let mut sag_plane = MprPlane3D::from_volume_native(&sag_vol);
        let target_lps = sag_vol.voxel_to_lps([1.0, 1.0, 2.0]);
        sag_plane.offset_mm = dot3(sub3(target_lps, sag_plane.center_lps), sag_plane.normal);

        // With sagittal sibling present in the group, selecting Sagittal uses `sag_vol` directly
        // and produces pixels bit-identical to opening the sagittal series alone.
        let with_group = sag_vol.reslice_plane(&[ax_vol.clone(), cor_vol.clone()], &sag_plane);
        let alone = sag_vol.reslice_plane(&[], &sag_plane);
        let Pixels::Gray(got_native) = &with_group.pixels else { panic!("gray"); };
        let Pixels::Gray(got_alone) = &alone.pixels else { panic!("gray"); };
        let Pixels::Gray(expected_sag) = &sag_frames[2].pixels else { panic!("gray"); };
        assert_eq!(got_native, expected_sag);
        assert_eq!(got_native, got_alone);

        // Without sagittal sibling, reslicing falls back to the axial volume.
        let without_sag = ax_vol.reslice_plane(&[cor_vol], &sag_plane);
        let Pixels::Gray(got_resliced) = &without_sag.pixels else { panic!("gray"); };
        assert_ne!(got_resliced, expected_sag);
    }

    #[test]
    fn registration_recovers_10_deg_rotation_and_15_mm_shift() {
        let dim = 24usize;
        let sp = 2.0f64;
        let phantom_at = |lps: [f64; 3]| -> f32 {
            let dx = (lps[0] - 24.0) / 14.0;
            let dy = (lps[1] - 24.0) / 9.0;
            let dz = (lps[2] - 24.0) / 6.0;
            let ell = dx * dx + dy * dy + dz * dz;
            let in_bar = (lps[0] - 32.0).abs() < 3.0 && (lps[1] - 27.0).abs() < 3.0 && (lps[2] - 24.0).abs() < 4.0;
            if in_bar {
                800.0
            } else if ell <= 1.0 {
                400.0 * (1.0 - ell as f32) + 100.0
            } else {
                0.0
            }
        };

        let true_rot = euler_deg_to_matrix(0.0, 0.0, 10.0);
        let true_rot_inv = [
            [true_rot[0][0], true_rot[1][0], true_rot[2][0]],
            [true_rot[0][1], true_rot[1][1], true_rot[2][1]],
            [true_rot[0][2], true_rot[1][2], true_rot[2][2]],
        ];
        let true_shift = [12.0, -9.0, 0.0];
        let build_vol = |is_target: bool| {
            let mut frames = Vec::new();
            let mut slices = Vec::new();
            for z in 0..dim {
                let mut pix = vec![0.0f32; dim * dim];
                for y in 0..dim {
                    for x in 0..dim {
                        let lps = [x as f64 * sp, y as f64 * sp, z as f64 * sp];
                        let q = if is_target {
                            let rel = sub3(lps, add3([24.0, 24.0, 24.0], true_shift));
                            add3([24.0, 24.0, 24.0], mat3_mul_vec(true_rot_inv, rel))
                        } else {
                            lps
                        };
                        pix[y * dim + x] = phantom_at(q);
                    }
                }
                let orig = [0.0, 0.0, z as f64 * sp];
                let orient = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
                frames.push(Arc::new(Frame {
                    width: dim as u32,
                    height: dim as u32,
                    pixels: Pixels::Gray(pix),
                    stored_window: None,
                    full_window: Window { level: 300.0, width: 800.0 },
                    bits_window: None,
                    inverts: false,
                    voi_lut_applied: false,
                    slope: 1.0,
                    spacing: Some((sp, sp)),
                    pixel_ratio: 1.0,
                    origin: Some(orig),
                    orientation: orient,
                    is_3d_plane: true,
                    frame_of_reference_uid: None,
                    slice_thickness: Some(sp),
                    slice_location: None,
                    value_unit: "HU",
                }));
                slices.push(crate::dicom_index::SliceRef {
                    path: std::path::PathBuf::from(format!("r_{z}.dcm")),
                    frame: 0,
                    instance: z as i64 + 1,
                    position: Some(orig),
                    orientation: orient,
                    is_3d_plane: true,
                    spacing: Some((sp, sp)),
                    slice_location: None,
                    frame_of_reference_uid: None,
                });
            }
            let series = Series {
                uid: if is_target { "2" } else { "1" }.into(),
                number: 1,
                description: String::new(),
                modality: "CT".into(),
                rows: dim as u32,
                columns: dim as u32,
                normal: None,
                dominant_plane: crate::dicom_index::DominantPlane::Axial,
                frame_of_reference_uid: None,
                acquisition_time_secs: None,
                acquisition_number: None,
                convolution_kernel: None,
                window_center: None,
                window_width: None,
                mpr_group_id: None,
                slices,
            };
            Volume3D::from_series(&series, &frames).unwrap()
        };

        let src_vol = build_vol(false);
        let dst_vol = build_vol(true);
        let reg = RigidTransform3D::register(&src_vol, &dst_vol);
        assert!(reg.correlation > 0.90, "correlation = {}", reg.correlation);

        let expected_pt = |p: [f64; 3]| add3(add3([24.0, 24.0, 24.0], true_shift), mat3_mul_vec(true_rot, sub3(p, [24.0, 24.0, 24.0])));
        for test_p in [[24.0, 24.0, 24.0], [34.0, 24.0, 24.0], [24.0, 32.0, 26.0]] {
            let got = add3(reg.pivot_target_lps, mat3_mul_vec(reg.rotation, sub3(test_p, reg.pivot_source_lps)));
            let exp = expected_pt(test_p);
            let err = ((got[0] - exp[0]).powi(2) + (got[1] - exp[1]).powi(2) + (got[2] - exp[2]).powi(2)).sqrt();
            assert!(err < 1.0, "registration error {err:.3} mm for {test_p:?}");
        }
    }

    #[test]
    fn lps_to_voxel_round_trips_under_15_degree_gantry_tilt() {
        let tilt = 15.0_f64.to_radians();
        let slice_dir = [0.0, tilt.sin(), tilt.cos()];
        let origins = [
            [0.0, 0.0, 0.0],
            [0.0, slice_dir[1] * 2.0, slice_dir[2] * 2.0],
            [0.0, slice_dir[1] * 4.0, slice_dir[2] * 4.0],
        ];
        let mut frames = Vec::new();
        let mut slices = Vec::new();
        for (i, &orig) in origins.iter().enumerate() {
            frames.push(Arc::new(Frame {
                width: 8,
                height: 8,
                pixels: Pixels::Gray(vec![0.0; 64]),
                stored_window: None,
                full_window: Window { level: 0.0, width: 100.0 },
                bits_window: None,
                inverts: false,
                voi_lut_applied: false,
                slope: 1.0,
                spacing: Some((0.5, 0.5)),
                pixel_ratio: 1.0,
                origin: Some(orig),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                frame_of_reference_uid: None,
                slice_thickness: Some(2.0),
                slice_location: Some(orig[2]),
                value_unit: "HU",
            }));
            slices.push(crate::dicom_index::SliceRef {
                path: std::path::PathBuf::from(format!("tilt_{i}.dcm")),
                frame: 0,
                instance: (i + 1) as i64,
                position: Some(orig),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                spacing: Some((0.5, 0.5)),
                slice_location: Some(orig[2]),
                frame_of_reference_uid: None,
            });
        }
        let series = Series {
            uid: "tilt".into(),
            number: 1,
            description: "Tilted".into(),
            modality: "CT".into(),
            rows: 8,
            columns: 8,
            slices,
            normal: Some([0.0, 0.0, 1.0]),
            dominant_plane: crate::dicom_index::DominantPlane::Axial,
            frame_of_reference_uid: None,
            acquisition_time_secs: None,
            acquisition_number: None,
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            mpr_group_id: None,
        };
        let vol = Volume3D::from_series(&series, &frames).expect("tilted regular stack");
        for target in [[1.25, 3.5, 0.75], [6.0, 2.0, 1.9], [0.0, 0.0, 2.0]] {
            let lps = vol.voxel_to_lps(target);
            let round = vol.lps_to_voxel(lps);
            for axis in 0..3 {
                assert!(
                    (round[axis] - target[axis]).abs() < 1e-6,
                    "axis {axis}: expected {}, got {}",
                    target[axis],
                    round[axis]
                );
            }
        }
    }

    #[test]
    fn regularity_detects_missing_and_duplicate_slices_and_allows_tilted_gantry() {
        let regular_tilted = [
            [0.0, 0.0, 0.0],
            [0.0, 0.5, 1.9],
            [0.0, 1.0, 3.8],
            [0.0, 1.5, 5.7],
        ];
        assert!(!SliceRegularity::evaluate(&regular_tilted, [0.0, 0.0, 1.0], 0.5).is_irregular);

        let missing_slice = [[0.0, 0.0, 0.0], [0.0, 0.0, 2.0], [0.0, 0.0, 6.0]];
        assert!(SliceRegularity::evaluate(&missing_slice, [0.0, 0.0, 1.0], 0.5).is_irregular);

        let duplicate_slice = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 2.0]];
        assert!(SliceRegularity::evaluate(&duplicate_slice, [0.0, 0.0, 1.0], 0.5).is_irregular);
    }

    #[test]
    fn rigid_transform_inverse_round_trips_points() {
        let t = RigidTransform3D {
            pivot_source_lps: [10.0, -5.0, 20.0],
            pivot_target_lps: [-3.0, 12.0, 40.0],
            rotation: euler_deg_to_matrix(12.0, -7.0, 23.0),
            correlation: 0.88,
        };
        let inv = t.inverse();
        let pt = [25.0, 14.0, -8.0];
        let fwd = t.transform_point(pt);
        let back = inv.transform_point(fwd);
        for axis in 0..3 {
            assert!((back[axis] - pt[axis]).abs() < 1e-9);
        }
    }
}
