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
            // Frames that all sit at one place are a cine, not a volume
            // (IsiX 6.0.4 `SliceIntervalCorrection`): a zero interval is never a stack.
            if reg.is_irregular || reg.mean_interval <= 1e-6 {
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

    /// The rotation that turned `before`'s axes into `self`'s: `R = B_self · B_beforeᵀ`,
    /// where `B` has the plane's `u`, `v` and normal as columns.
    pub fn rotation_since(&self, before: &MprPlane3D) -> [[f64; 3]; 3] {
        let (a, b) = ([before.u_axis, before.v_axis, before.normal], [self.u_axis, self.v_axis, self.normal]);
        let mut r = [[0.0f64; 3]; 3];
        for (row, r_row) in r.iter_mut().enumerate() {
            for (col, cell) in r_row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| b[k][row] * a[k][col]).sum();
            }
        }
        r
    }

    /// Turns the plane's axes by `rotation`, keeping the point it currently shows on
    /// the plane (the slice moves with the axes, not to another place).
    pub fn rotate_axes(&mut self, rotation: [[f64; 3]; 3]) {
        let shown = self.slice_center_lps();
        self.u_axis = normalize3(mat3_mul_vec(rotation, self.u_axis));
        self.v_axis = normalize3(mat3_mul_vec(rotation, self.v_axis));
        self.normal = normalize3(mat3_mul_vec(rotation, self.normal));
        self.offset_mm = dot3(sub3(shown, self.center_lps), self.normal);
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
    /// Aligned by the body outline and height profiles because the voxel match was
    /// weak (interior changes between dates); trusted, shown as approximate.
    pub outline_based: bool,
}

impl RigidTransform3D {
    /// Exact identity transform (e.g., when both volumes share `FrameOfReferenceUID`).
    pub fn identity(correlation: f64) -> Self {
        Self {
            pivot_source_lps: [0.0, 0.0, 0.0],
            pivot_target_lps: [0.0, 0.0, 0.0],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            correlation,
            outline_based: false,
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
            outline_based: self.outline_based,
        }
    }

    /// Returns `true` when the registration score is high enough to trust 3D alignment.
    pub fn is_reliable(&self) -> bool {
        self.outline_based || self.correlation >= 0.45
    }

    /// Identity alignment around the volumes' foreground centroids (translation-only baseline).
    pub fn centroid_only(source: &Volume3D, target: &Volume3D) -> Self {
        Self {
            pivot_source_lps: source.foreground_centroid_lps,
            pivot_target_lps: target.foreground_centroid_lps,
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            correlation: 0.0,
            outline_based: false,
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
        let whole = Self::register_whole_volume(source, target);
        let Some(body) = Self::register_by_body(source, target) else {
            return whole;
        };
        // A trusted result beats an untrusted one; between equals, the better match.
        match (body.is_reliable(), whole.is_reliable()) {
            (true, false) => body,
            (false, true) => whole,
            _ if body.correlation >= whole.correlation => body,
            _ => whole,
        }
    }

    /// Registration built for two CT scans of one patient on different dates, where
    /// the patient may lie rolled, the table sits at another height, and one scan may
    /// cover more of the body than the other (thorax-abdomen-pelvis against abdomen).
    ///
    /// 1. On each axial slice the body is found: the largest connected region above
    ///    -350 HU (which leaves the CT table out), with lungs and bowel gas filled in.
    /// 2. The two scans are matched in height by comparing slice-by-slice profiles of
    ///    body area, air content and bone content.
    /// 3. The roll is the difference in the tilt of the body outline at matching
    ///    heights; the shift is the difference of the body centres.
    /// 4. That starting point is refined by correlating voxels inside the body only.
    ///
    /// Returns `None` when the volumes are not axial CT or do not overlap enough.
    fn register_by_body(source: &Volume3D, target: &Volume3D) -> Option<Self> {
        let prof_s = body_profile(source)?;
        let prof_t = body_profile(target)?;
        let (shift_z, profile_score, z_lo, z_hi) = match_profiles(&prof_s, &prof_t)?;

        // Roll and centres over the matched heights.
        let mut rolls = Vec::new();
        let (mut cs, mut ct) = (Vec::new(), Vec::new());
        let mut z = z_lo;
        while z <= z_hi {
            if let (Some(a), Some(b)) = (profile_at(&prof_s, z), profile_at(&prof_t, z + shift_z)) {
                cs.push((a.cx, a.cy));
                ct.push((b.cx, b.cy));
                if a.elongation > 1.15 && b.elongation > 1.15 {
                    let mut d = (b.angle - a.angle).to_degrees();
                    while d > 90.0 {
                        d -= 180.0;
                    }
                    while d < -90.0 {
                        d += 180.0;
                    }
                    rolls.push(d);
                }
            }
            z += 5.0;
        }
        if cs.len() < 4 {
            return None;
        }
        let roll_count = rolls.len();
        let roll = median(&mut rolls).unwrap_or(0.0);
        // Spread of the per-slice roll estimates (interquartile range), degrees.
        let roll_spread = if roll_count >= 4 {
            rolls[roll_count * 3 / 4] - rolls[roll_count / 4]
        } else {
            f64::INFINITY
        };
        let z_mid = 0.5 * (z_lo + z_hi);
        let pivot_a = [
            median(&mut cs.iter().map(|c| c.0).collect::<Vec<_>>())?,
            median(&mut cs.iter().map(|c| c.1).collect::<Vec<_>>())?,
            z_mid,
        ];
        let pivot_b = [
            median(&mut ct.iter().map(|c| c.0).collect::<Vec<_>>())?,
            median(&mut ct.iter().map(|c| c.1).collect::<Vec<_>>())?,
            z_mid + shift_z,
        ];

        // Voxels inside the source body, over the matched heights only.
        let samples = body_samples(source, z_lo, z_hi, 40_000);
        if samples.len() < 64 {
            return None;
        }
        let samples: Vec<([f64; 3], f32)> =
            samples.into_iter().map(|(p, v)| (sub3(p, pivot_a), v)).collect();
        let scales: &[(f64, f64, usize)] = &[
            (6.0, 4.0, 2),
            (3.0, 2.0, 2),
            (1.5, 1.0, 2),
            (0.6, 0.4, 1),
        ];
        let start = [0.0, 0.0, 0.0, 0.0, 0.0, roll];
        let (params, score) = refine_ncc(&samples, target, pivot_b, start, scales);
        // When the voxel match is weak (bowel, contrast phase and breathing differ
        // between dates) but the height profiles agree and the outline gives one
        // consistent roll, trust the outline. Keep the refinement only if it stayed
        // close to the outline's answer.
        let outline_strong = outline_is_trustworthy(profile_score, roll_count, roll_spread);
        let near_outline = (params[5] - roll).abs() <= 6.0
            && params[3].abs() <= 8.0
            && params[4].abs() <= 8.0
            && params[2].abs() <= 15.0;
        let outline_based = score < 0.45 && outline_strong;
        let params = if outline_based && !near_outline { start } else { params };
        crate::crash_log::log_line(
            "INFO",
            &format!(
                "Body registration: profile {profile_score:.2}, height shift {shift_z:.1} mm, \
                 outline roll {roll:.1}° (spread {roll_spread:.1}°, {roll_count} slices), \
                 voxel score {score:.2}, outline-based {outline_based}"
            ),
        );
        Some(Self {
            pivot_source_lps: pivot_a,
            pivot_target_lps: add3(pivot_b, [params[0], params[1], params[2]]),
            rotation: euler_deg_to_matrix(params[3], params[4], params[5]),
            correlation: if outline_based { ncc_at(&samples, target, pivot_b, &params) } else { score },
            outline_based,
        })
    }

    /// The original whole-volume search, kept as a second opinion for volumes the
    /// body-based method cannot read (non-axial or non-CT acquisitions).
    fn register_whole_volume(source: &Volume3D, target: &Volume3D) -> Self {
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

        // Coarse (tz, rz) sweep so partial-overlap + rolled scans find the matching Z range and
        // axial roll angle before 6-DOF refinement.
        let mut best_params = [0.0f64; 6];
        let mut best_score = ncc_at(&samples, target, pivot_b_init, &best_params);
        let max_tz = ((source.projected_half_span_mm([0.0, 0.0, 1.0])
            + target.projected_half_span_mm([0.0, 0.0, 1.0]))
        .min(350.0)) as i32;
        for rz in [-24.0_f64, -12.0, 0.0, 12.0, 24.0] {
            for tz_i in (-max_tz..=max_tz).step_by(35) {
                let cand = [0.0, 0.0, tz_i as f64, 0.0, 0.0, rz];
                let sc = ncc_at(&samples, target, pivot_b_init, &cand);
                if sc > best_score {
                    best_score = sc;
                    best_params = cand;
                }
            }
        }
        let scales: &[(f64, f64, usize)] = &[
            (25.0, 12.0, 2), // Wide coarse: up to ±50 mm translation, ±24° rotation
            (10.0, 8.0, 2),  // Coarse: 10 mm translation step, 8° rotation step
            (4.0, 3.5, 2),   // Medium: 4 mm translation step, 3.5° rotation step
            (1.5, 1.2, 2),   // Fine: 1.5 mm translation step, 1.2° rotation step
            (0.5, 0.4, 1),   // Sub-millimeter / sub-degree polish
        ];
        let (best_params, best_score) = refine_ncc(&samples, target, pivot_b_init, best_params, scales);

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
            outline_based: false,
        }
    }

    /// Maps a 3D point in `source` LPS coordinates to the registered 3D point in `target` LPS coordinates.
    pub fn transform_point(&self, pt_source_lps: [f64; 3]) -> [f64; 3] {
        let rel = sub3(pt_source_lps, self.pivot_source_lps);
        add3(self.pivot_target_lps, mat3_mul_vec(self.rotation, rel))
    }

    /// Maps a 3D slicing plane `source_plane` from `source` (Study A) into the corresponding
    /// 3D slicing plane in `target` (Study B), preserving both 3D slice position and 3D orientation.
    #[allow(dead_code)]
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

/// Normalized cross-correlation between `samples` (source points relative to the
/// source pivot, and their values) and `target` under `params`
/// `[tx, ty, tz, rx°, ry°, rz°]` about `pivot_b`. Only samples that land inside the
/// target count; values are clamped to -250…900 so air and metal do not dominate.
fn ncc_at(samples: &[([f64; 3], f32)], target: &Volume3D, pivot_b: [f64; 3], params: &[f64; 6]) -> f64 {
    let rot = euler_deg_to_matrix(params[3], params[4], params[5]);
    let trans = [pivot_b[0] + params[0], pivot_b[1] + params[1], pivot_b[2] + params[2]];
    let (mut sa, mut sa2, mut sb, mut sb2, mut sab) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut valid = 0usize;
    for &(rel_a, val_a) in samples {
        if let Some(v) = target.sample_lps(add3(trans, mat3_mul_vec(rot, rel_a))) {
            let va = (val_a as f64).clamp(-250.0, 900.0);
            let vb = (v as f64).clamp(-250.0, 900.0);
            valid += 1;
            sa += va;
            sa2 += va * va;
            sb += vb;
            sb2 += vb * vb;
            sab += va * vb;
        }
    }
    if valid < (samples.len() / 5).max(32) {
        return -1.0;
    }
    let n = valid as f64;
    let (ma, mb) = (sa / n, sb / n);
    let va = (sa2 - n * ma * ma).max(1e-6);
    let vb = (sb2 - n * mb * mb).max(1e-6);
    (sab - n * ma * mb) / (va * vb).sqrt()
}

/// Whether the body outline alone can be trusted for alignment: the height profiles
/// of the two scans agree (mean correlation ≥ 0.75), and at least six matched slices
/// give a roll whose middle half lies within 6°.
fn outline_is_trustworthy(profile_score: f64, roll_slices: usize, roll_spread_deg: f64) -> bool {
    profile_score >= 0.75 && roll_slices >= 6 && roll_spread_deg <= 6.0
}

/// Coordinate pattern search over the six parameters, coarse to fine, from `start`.
fn refine_ncc(
    samples: &[([f64; 3], f32)],
    target: &Volume3D,
    pivot_b: [f64; 3],
    start: [f64; 6],
    scales: &[(f64, f64, usize)],
) -> ([f64; 6], f64) {
    let mut best = start;
    let mut best_score = ncc_at(samples, target, pivot_b, &best);
    for &(t_step, r_step, passes) in scales {
        let steps = [t_step, t_step, t_step, r_step, r_step, r_step];
        for _ in 0..passes {
            let mut improved = false;
            for (dim, &step) in steps.iter().enumerate() {
                let mut plus = best;
                plus[dim] += step;
                let s_plus = ncc_at(samples, target, pivot_b, &plus);
                let mut minus = best;
                minus[dim] -= step;
                let s_minus = ncc_at(samples, target, pivot_b, &minus);
                if s_plus > best_score && s_plus >= s_minus {
                    best_score = s_plus;
                    best = plus;
                    improved = true;
                } else if s_minus > best_score {
                    best_score = s_minus;
                    best = minus;
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }
    }
    (best, best_score)
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

/// What one axial slice says about the body: its height (patient z, mm), area,
/// air and bone content, centre, and the tilt of its outline.
#[derive(Clone, Copy, Debug)]
struct BodySlice {
    z: f64,
    area: f64,
    air: f64,
    bone: f64,
    cx: f64,
    cy: f64,
    /// Angle of the outline's long axis in the axial plane, radians.
    angle: f64,
    /// Long axis over short axis of the outline.
    elongation: f64,
}

/// Body threshold: -350 HU for CT (fat and soft tissue above, air below); for
/// other data, the volume's own foreground threshold.
fn body_threshold(vol: &Volume3D) -> Option<f32> {
    let ct_like = vol.value_unit == "HU" && vol.full_window.low() < -500.0;
    ct_like.then_some(-350.0)
}

/// The body mask of slice `z` on a grid of every `step`-th voxel: the largest
/// connected region above `thresh` (after one erosion, so a table touching the back
/// by a thin pad is cut away), grown back, with enclosed holes (lungs, bowel gas)
/// filled. Returns the grid width, height and mask.
fn body_mask(vol: &Volume3D, z: usize, step: usize, thresh: f32) -> (usize, usize, Vec<bool>) {
    let gw = vol.width.div_ceil(step);
    let gh = vol.height.div_ceil(step);
    let wh = vol.width * vol.height;
    let above: Vec<bool> = (0..gw * gh)
        .map(|i| {
            let (gx, gy) = (i % gw, i / gw);
            let (x, y) = ((gx * step).min(vol.width - 1), (gy * step).min(vol.height - 1));
            vol.voxel_at(z * wh + y * vol.width + x) > thresh
        })
        .collect();
    let at = |m: &Vec<bool>, x: isize, y: isize| -> bool {
        x >= 0 && y >= 0 && (x as usize) < gw && (y as usize) < gh && m[y as usize * gw + x as usize]
    };
    // Erode once.
    let eroded: Vec<bool> = (0..gw * gh)
        .map(|i| {
            let (x, y) = ((i % gw) as isize, (i / gw) as isize);
            at(&above, x, y) && at(&above, x - 1, y) && at(&above, x + 1, y) && at(&above, x, y - 1) && at(&above, x, y + 1)
        })
        .collect();
    // Largest 4-connected component of the eroded mask.
    let mut label = vec![0u32; gw * gh];
    let (mut best_label, mut best_size, mut next) = (0u32, 0usize, 1u32);
    for start in 0..gw * gh {
        if !eroded[start] || label[start] != 0 {
            continue;
        }
        let mut stack = vec![start];
        label[start] = next;
        let mut size = 0usize;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % gw, i / gw);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < gw && ny < gh {
                    let j = ny * gw + nx;
                    if eroded[j] && label[j] == 0 {
                        label[j] = next;
                        stack.push(j);
                    }
                }
            }
        }
        if size > best_size {
            best_size = size;
            best_label = next;
        }
        next += 1;
    }
    let mut body = vec![false; gw * gh];
    if best_size == 0 {
        return (gw, gh, body);
    }
    // Grow back by one, within the original mask.
    for i in 0..gw * gh {
        let (x, y) = ((i % gw) as isize, (i / gw) as isize);
        let core = |xx: isize, yy: isize| {
            xx >= 0 && yy >= 0 && (xx as usize) < gw && (yy as usize) < gh && label[yy as usize * gw + xx as usize] == best_label
        };
        body[i] = above[i] && (core(x, y) || core(x - 1, y) || core(x + 1, y) || core(x, y - 1) || core(x, y + 1));
    }
    // Fill holes: everything not reachable from the border without crossing the body.
    let mut outside = vec![false; gw * gh];
    let mut stack: Vec<usize> = (0..gw * gh)
        .filter(|&i| {
            let (x, y) = (i % gw, i / gw);
            (x == 0 || y == 0 || x == gw - 1 || y == gh - 1) && !body[i]
        })
        .collect();
    for &i in &stack {
        outside[i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % gw, i / gw);
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < gw && ny < gh {
                let j = ny * gw + nx;
                if !body[j] && !outside[j] {
                    outside[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    (gw, gh, outside.iter().map(|o| !o).collect())
}

/// One `BodySlice` per (sub-sampled) axial slice; `None` unless the volume is CT
/// acquired roughly axially.
fn body_profile(vol: &Volume3D) -> Option<Vec<BodySlice>> {
    let thresh = body_threshold(vol)?;
    if vol.slice_dir[2].abs() < 0.8 || vol.depth < 6 {
        return None;
    }
    let step = (vol.width.max(vol.height) / 128).max(1);
    let z_step = (vol.depth / 220).max(1);
    let wh = vol.width * vol.height;
    let cell = vol.spacing[0] * vol.spacing[1] * (step * step) as f64;
    let mut out = Vec::new();
    for z in (0..vol.depth).step_by(z_step) {
        let (gw, gh, mask) = body_mask(vol, z, step, thresh);
        let (mut n, mut air, mut bone) = (0usize, 0usize, 0usize);
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for gy in 0..gh {
            for gx in 0..gw {
                if !mask[gy * gw + gx] {
                    continue;
                }
                let (x, y) = ((gx * step).min(vol.width - 1), (gy * step).min(vol.height - 1));
                let v = vol.voxel_at(z * wh + y * vol.width + x);
                let p = vol.voxel_to_lps([x as f64, y as f64, z as f64]);
                n += 1;
                air += (v < -400.0) as usize;
                bone += (v > 200.0) as usize;
                sx += p[0];
                sy += p[1];
                sxx += p[0] * p[0];
                syy += p[1] * p[1];
                sxy += p[0] * p[1];
            }
        }
        if n < 20 {
            continue;
        }
        let nf = n as f64;
        let (cx, cy) = (sx / nf, sy / nf);
        let (vxx, vyy, vxy) = (sxx / nf - cx * cx, syy / nf - cy * cy, sxy / nf - cx * cy);
        let angle = 0.5 * (2.0 * vxy).atan2(vxx - vyy);
        let tr = vxx + vyy;
        let disc = ((vxx - vyy).powi(2) + 4.0 * vxy * vxy).sqrt();
        let (l1, l2) = (0.5 * (tr + disc), (0.5 * (tr - disc)).max(1e-6));
        out.push(BodySlice {
            z: vol.voxel_to_lps([vol.width as f64 * 0.5, vol.height as f64 * 0.5, z as f64])[2],
            area: nf * cell,
            air: air as f64 / nf,
            bone: bone as f64 / nf,
            cx,
            cy,
            angle,
            elongation: (l1 / l2).sqrt(),
        });
    }
    out.sort_by(|a, b| a.z.total_cmp(&b.z));
    (out.len() >= 6).then_some(out)
}

/// The profile at height `z`, interpolated between the nearest two slices.
fn profile_at(p: &[BodySlice], z: f64) -> Option<BodySlice> {
    let (first, last) = (p.first()?, p.last()?);
    if z < first.z || z > last.z {
        return None;
    }
    let i = p.partition_point(|s| s.z < z).clamp(1, p.len() - 1);
    let (a, b) = (p[i - 1], p[i]);
    let t = if (b.z - a.z).abs() < 1e-9 { 0.0 } else { (z - a.z) / (b.z - a.z) };
    let lerp = |u: f64, v: f64| u + t * (v - u);
    Some(BodySlice {
        z,
        area: lerp(a.area, b.area),
        air: lerp(a.air, b.air),
        bone: lerp(a.bone, b.bone),
        cx: lerp(a.cx, b.cx),
        cy: lerp(a.cy, b.cy),
        angle: if t < 0.5 { a.angle } else { b.angle },
        elongation: lerp(a.elongation, b.elongation),
    })
}

fn ncc_1d(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len() as f64;
    if a.len() < 8 {
        return None;
    }
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    // A flat feature says nothing about the height; leave it out.
    (saa > 1e-9 * n && sbb > 1e-9 * n).then(|| sab / (saa * sbb).sqrt())
}

/// The height shift `s` (target z = source z + s) at which the two body profiles
/// agree best, the agreement (mean correlation of area, air and bone profiles), and
/// the overlapping source heights `[z_lo, z_hi]`.
fn match_profiles(ps: &[BodySlice], pt: &[BodySlice]) -> Option<(f64, f64, f64, f64)> {
    let (s_lo, s_hi) = (ps.first()?.z, ps.last()?.z);
    let (t_lo, t_hi) = (pt.first()?.z, pt.last()?.z);
    let min_overlap = (0.35 * (s_hi - s_lo).min(t_hi - t_lo)).max(50.0);
    let score_at = |shift: f64| -> Option<(f64, f64, f64)> {
        let lo = s_lo.max(t_lo - shift);
        let hi = s_hi.min(t_hi - shift);
        if hi - lo < min_overlap {
            return None;
        }
        let mut cols: [(Vec<f64>, Vec<f64>); 3] = Default::default();
        let mut z = lo;
        while z <= hi {
            if let (Some(a), Some(b)) = (profile_at(ps, z), profile_at(pt, z + shift)) {
                cols[0].0.push(a.area);
                cols[0].1.push(b.area);
                cols[1].0.push(a.air);
                cols[1].1.push(b.air);
                cols[2].0.push(a.bone);
                cols[2].1.push(b.bone);
            }
            z += 2.5;
        }
        let scores: Vec<f64> = cols.iter().filter_map(|(a, b)| ncc_1d(a, b)).collect();
        if scores.is_empty() {
            return None;
        }
        Some((scores.iter().sum::<f64>() / scores.len() as f64, lo, hi))
    };
    let mut best: Option<(f64, f64, f64, f64)> = None;
    let consider = |shift: f64, best: &mut Option<(f64, f64, f64, f64)>| {
        if let Some((sc, lo, hi)) = score_at(shift)
            && best.is_none_or(|b| sc > b.1)
        {
            *best = Some((shift, sc, lo, hi));
        }
    };
    let mut shift = (t_lo - s_hi).floor();
    while shift <= (t_hi - s_lo).ceil() {
        consider(shift, &mut best);
        shift += 2.5;
    }
    let coarse = best?.0;
    let mut fine = coarse - 2.5;
    while fine <= coarse + 2.5 {
        consider(fine, &mut best);
        fine += 0.5;
    }
    best
}

/// Up to `max` voxels inside the body of `vol`, between heights `z_lo` and `z_hi`,
/// as (patient position, value).
fn body_samples(vol: &Volume3D, z_lo: f64, z_hi: f64, max: usize) -> Vec<([f64; 3], f32)> {
    let Some(thresh) = body_threshold(vol) else {
        return Vec::new();
    };
    let step = (vol.width.max(vol.height) / 128).max(1);
    let wh = vol.width * vol.height;
    let slices: Vec<usize> = (0..vol.depth)
        .filter(|&z| {
            let zz = vol.voxel_to_lps([vol.width as f64 * 0.5, vol.height as f64 * 0.5, z as f64])[2];
            zz >= z_lo && zz <= z_hi
        })
        .collect();
    if slices.is_empty() {
        return Vec::new();
    }
    let take = slices.len().min(40);
    let mut out = Vec::new();
    for k in 0..take {
        let z = slices[k * slices.len() / take];
        let (gw, gh, mask) = body_mask(vol, z, step, thresh);
        let inside: Vec<usize> = (0..gw * gh).filter(|&i| mask[i]).collect();
        let per_slice = (max / take).max(1);
        let stride = (inside.len() / per_slice).max(1);
        for &i in inside.iter().step_by(stride) {
            let (x, y) = (((i % gw) * step).min(vol.width - 1), ((i / gw) * step).min(vol.height - 1));
            out.push((vol.voxel_to_lps([x as f64, y as f64, z as f64]), vol.voxel_at(z * wh + y * vol.width + x)));
        }
    }
    out
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

    // IsiX 6.0.4 `SliceIntervalCorrection`: frames that all say they are at one
    // place are a cine, and are not stacked into a volume.
    #[test]
    fn frames_at_one_position_are_a_cine_not_a_volume() {
        let frame = Arc::new(Frame {
            width: 4,
            height: 4,
            pixels: Pixels::Gray(vec![0.0; 16]),
            stored_window: None,
            full_window: Window { level: 0.0, width: 100.0 },
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
            value_unit: "",
        });
        let slice = crate::dicom_index::SliceRef {
            path: std::path::PathBuf::from("cine.dcm"),
            frame: 0,
            instance: 1,
            position: Some([0.0, 0.0, 0.0]),
            orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            is_3d_plane: true,
            spacing: Some((1.0, 1.0)),
            slice_location: Some(0.0),
            frame_of_reference_uid: None,
        };
        let series = Series {
            uid: "1.2.3.4".into(),
            number: 1,
            description: "Cine".into(),
            modality: "XA".into(),
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
            slices: vec![slice.clone(), slice.clone(), slice.clone(), slice],
        };
        let frames = vec![frame.clone(), frame.clone(), frame.clone(), frame];
        assert!(matches!(
            Volume3D::from_series_checked(&series, &frames, false),
            Err(VolumeError::IrregularSpacing { .. })
        ));
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

    /// Two CT "dates" of one synthetic patient, like a thorax-abdomen-pelvis scan
    /// against an abdomen-only one: the later scan covers far more of the body, the
    /// patient is rolled 17° and shifted, the frames of reference differ by 430 mm in
    /// height, the CT table (which does not roll with the patient) sits lower, and the
    /// later scan has noise, another contrast phase, moved bowel gas and arms down.
    fn two_date_phantoms(interior_noise_hu: f32) -> (Arc<Volume3D>, Arc<Volume3D>, f64, [f64; 3]) {
        let roll_deg = 17.0f64;
        // A large height offset between the two scans' frames of reference is common.
        let (tx, ty, tz) = (14.0f64, -22.0f64, 430.0f64);
        let (rs, rc) = roll_deg.to_radians().sin_cos();
        // Patient anatomy in body coordinates (u: right→left, v: front→back, w: feet→head), HU.
        // `later`: a later date with another contrast phase, moved bowel gas and the arms
        // down beside the body.
        let anatomy = |u: f64, v: f64, w: f64, later: bool| -> f32 {
            let arm = |cu: f64| ((u - cu) / 32.0).powi(2) + ((v + 5.0) / 40.0).powi(2) < 1.0;
            if later && w < 330.0 && (arm(-205.0) || arm(205.0)) {
                return if ((u.abs() - 205.0) / 10.0).powi(2) + ((v + 5.0) / 12.0).powi(2) < 1.0 { 700.0 } else { 40.0 };
            }
            let a = 150.0 + 25.0 * (w / 90.0).sin();
            let b = 105.0 + 10.0 * (w / 70.0).cos();
            let e = (u / a).powi(2) + (v / b).powi(2);
            if e > 1.0 {
                return -1000.0;
            }
            let blob = |cu: f64, cv: f64, ru: f64, rv: f64| ((u - cu) / ru).powi(2) + ((v - cv) / rv).powi(2) < 1.0;
            if blob(0.0, 70.0, 18.0, 18.0) {
                return 500.0; // spine
            }
            if w > 260.0 && (blob(-65.0, 10.0, 50.0, 70.0) || blob(65.0, 10.0, 50.0, 70.0)) {
                return -850.0; // lungs
            }
            if w < 40.0 && (blob(-95.0, 30.0, 35.0, 22.0) || blob(95.0, 30.0, 35.0, 22.0)) {
                return 650.0; // pelvic bone
            }
            if (150.0..270.0).contains(&w) && blob(-70.0, -10.0, 60.0, 55.0) {
                return if later { 115.0 } else { 70.0 }; // liver, patient's right; another contrast phase
            }
            if (80.0..150.0).contains(&w) && (blob(-55.0, 45.0, 22.0, 30.0) || blob(55.0, 45.0, 22.0, 30.0)) {
                return 160.0; // kidneys
            }
            let gas_u = if later { -10.0 + 0.3 * w } else { 30.0 + 0.2 * w };
            if (60.0..200.0).contains(&w) && blob(gas_u, -40.0, 14.0, 10.0) {
                return -900.0; // bowel gas, moved between dates
            }
            if e > 0.82 {
                return -100.0; // subcutaneous fat
            }
            30.0
        };
        let dim = 96usize;
        let sp = 4.0f64;
        let half = dim as f64 * sp * 0.5;
        let build = |later: bool, z_from: f64, z_to: f64| -> Arc<Volume3D> {
            let mut frames = Vec::new();
            let mut slices = Vec::new();
            let n = ((z_to - z_from) / 5.0) as usize + 1;
            for k in 0..n {
                let z = z_from + k as f64 * 5.0;
                let mut pix = vec![-1000.0f32; dim * dim];
                for yi in 0..dim {
                    for xi in 0..dim {
                        let (x, y) = (xi as f64 * sp - half, yi as f64 * sp - half);
                        // The table: flat, under the patient, not rolled.
                        let table_y = if later { 150.0 } else { 135.0 };
                        if (table_y..table_y + 8.0).contains(&y) && x.abs() < 170.0 {
                            pix[yi * dim + xi] = 250.0;
                            continue;
                        }
                        let (u, v, w) = if later {
                            let (dx, dy) = (x - tx, y - ty);
                            (rc * dx + rs * dy, -rs * dx + rc * dy, z - tz)
                        } else {
                            (x, y, z)
                        };
                        // Image noise (a fixed pseudo-random pattern, ±40 HU).
                        let h = ((xi * 73856093) ^ (yi * 19349663) ^ (k * 83492791) ^ later as usize) % 1000;
                        let noise = (h as f32 / 1000.0 - 0.5) * 80.0;
                        let mut value = anatomy(u, v, w, later);
                        // Structural change inside the body between dates: soft tissue
                        // (not lungs, bone or fat) gets an unrelated smooth pattern.
                        if later && interior_noise_hu > 0.0 && (0.0..200.0).contains(&value) {
                            let pattern = ((u / 23.0 + w / 31.0).sin() * (v / 19.0 - w / 47.0).cos()) as f32;
                            value = 40.0 + pattern * interior_noise_hu;
                        }
                        pix[yi * dim + xi] = value + noise;
                    }
                }
                let orig = [-half, -half, z];
                let orient = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
                frames.push(Arc::new(Frame {
                    width: dim as u32,
                    height: dim as u32,
                    pixels: Pixels::Gray(pix),
                    stored_window: None,
                    full_window: Window { level: 0.0, width: 2000.0 },
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
                    slice_thickness: Some(5.0),
                    slice_location: None,
                    value_unit: "HU",
                }));
                slices.push(crate::dicom_index::SliceRef {
                    path: std::path::PathBuf::from(format!("b_{later}_{k}.dcm")),
                    frame: 0,
                    instance: k as i64 + 1,
                    position: Some(orig),
                    orientation: orient,
                    is_3d_plane: true,
                    spacing: Some((sp, sp)),
                    slice_location: None,
                    frame_of_reference_uid: None,
                });
            }
            let series = Series {
                uid: if later { "later" } else { "earlier" }.into(),
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
        // Abdomen only (w 0…240) against thorax-abdomen-pelvis (w -110…405).
        let earlier = build(false, 0.0, 240.0);
        let later = build(true, tz - 110.0, tz + 405.0);

        (earlier, later, roll_deg, [tx, ty, tz])
    }

    fn check_two_dates(reg: &RigidTransform3D, roll_deg: f64, shift: [f64; 3], tol_mm: f64, tol_deg: f64) {
        let (rs, rc) = roll_deg.to_radians().sin_cos();
        let expect = |p: [f64; 3]| [rc * p[0] - rs * p[1] + shift[0], rs * p[0] + rc * p[1] + shift[1], p[2] + shift[2]];
        for p in [[0.0, 0.0, 120.0], [-70.0, -10.0, 200.0], [55.0, 45.0, 100.0], [0.0, 70.0, 30.0]] {
            let got = reg.transform_point(p);
            let exp = expect(p);
            let err = len3(sub3(got, exp));
            assert!(err < tol_mm, "error {err:.1} mm at {p:?} (got {got:?}, expected {exp:?})");
        }
        let got_roll = reg.rotation[1][0].atan2(reg.rotation[0][0]).to_degrees();
        assert!((got_roll - roll_deg).abs() < tol_deg, "roll {got_roll:.1}° instead of {roll_deg}°");
    }

    #[test]
    fn body_registration_handles_roll_partial_overlap_and_table() {
        let (earlier, later, roll, shift) = two_date_phantoms(0.0);
        let reg = RigidTransform3D::register(&earlier, &later);
        assert!(reg.is_reliable(), "not reliable, score {:.3}", reg.correlation);
        check_two_dates(&reg, roll, shift, 5.0, 2.0);
    }

    /// Organs that look different between dates (an unrelated pattern replaces the
    /// later scan's soft tissue): alignment still comes out right.
    #[test]
    fn body_registration_survives_interior_changes() {
        let (earlier, later, roll, shift) = two_date_phantoms(250.0);
        let reg = RigidTransform3D::register(&earlier, &later);
        assert!(reg.is_reliable(), "not reliable, score {:.3}", reg.correlation);
        // About one 5 mm slice of error is acceptable with scrambled organs.
        check_two_dates(&reg, roll, shift, 8.0, 2.5);
    }

    #[test]
    fn outline_is_trusted_only_when_profiles_and_roll_agree() {
        assert!(outline_is_trustworthy(0.85, 20, 3.0));
        assert!(!outline_is_trustworthy(0.60, 20, 3.0)); // heights do not match well
        assert!(!outline_is_trustworthy(0.85, 4, 3.0)); // too few slices
        assert!(!outline_is_trustworthy(0.85, 20, 12.0)); // roll estimates disagree
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

    /// Rotating one viewport turns a linked one by the same amount and keeps the
    /// orientation difference between them (e.g. a manual roll correction).
    #[test]
    fn plane_rotation_delta_preserves_the_relative_orientation() {
        let base = MprPlane3D {
            center_lps: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            offset_mm: 12.0,
            slab_mode: SlabMode::Thin,
            slab_thickness_mm: 1.0,
        };
        let mut leader = base.clone();
        // Follower already corrected by 15° of roll relative to the leader.
        let mut follower = base.clone();
        follower.orbit_3d(0.0, 0.0, 15.0);
        // The follower's axes as seen from the leader's own axes (B_leaderᵀ · B_follower):
        // this is what the eye compares between the two viewports.
        let seen_from = |l: &MprPlane3D, f: &MprPlane3D| -> [[f64; 3]; 3] {
            let (lb, fb) = ([l.u_axis, l.v_axis, l.normal], [f.u_axis, f.v_axis, f.normal]);
            let mut m = [[0.0f64; 3]; 3];
            for (i, row) in m.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    *cell = dot3(lb[i], fb[j]);
                }
            }
            m
        };
        let relative_before = seen_from(&leader, &follower);

        let before = leader.clone();
        leader.orbit_3d(20.0, -10.0, 5.0);
        let delta = leader.rotation_since(&before);
        let shown_before = follower.slice_center_lps();
        follower.rotate_axes(delta);

        let relative_after = seen_from(&leader, &follower);
        for r in 0..3 {
            for c in 0..3 {
                assert!((relative_before[r][c] - relative_after[r][c]).abs() < 1e-9);
            }
        }
        // The point the follower showed is still on its (turned) plane.
        let off_plane = dot3(sub3(shown_before, follower.center_lps), follower.normal) - follower.offset_mm;
        assert!(off_plane.abs() < 1e-9);
        // The delta is the identity when nothing turned.
        let still = leader.rotation_since(&leader);
        for (r, row) in still.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                assert!((cell - if r == c { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn rigid_transform_inverse_round_trips_points() {
        let t = RigidTransform3D {
            pivot_source_lps: [10.0, -5.0, 20.0],
            pivot_target_lps: [-3.0, 12.0, 40.0],
            rotation: euler_deg_to_matrix(12.0, -7.0, 23.0),
            correlation: 0.88,
            outline_based: false,
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
