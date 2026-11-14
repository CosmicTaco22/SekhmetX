//! 3D Volume representation, In-Place Single-Viewport Oblique MPR Slicer,
//! Hybrid Multi-Series (AX/COR/SAG) Fusion, and Computational 3D Rigid Registration.

use std::sync::Arc;

use rayon::prelude::*;

use crate::dicom_index::Series;
use crate::frame::{Frame, Pixels};
use crate::imaging::Window;

/// A contiguous 3D voxel volume in patient LPS physical coordinates (millimeters).
pub struct Volume3D {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    /// Calibrated greyscale values (`raw * slope + intercept`, e.g. HU for CT) in `z -> y -> x` order.
    pub voxels: Vec<f32>,
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
}

impl Volume3D {
    /// Builds a `Volume3D` from a `Series` and its decoded `Frame`s (one per slice).
    pub fn from_series(series: &Series, frames: &[Arc<Frame>]) -> Option<Arc<Self>> {
        Self::from_series_with_force(series, frames, false)
    }

    /// Builds a `Volume3D` from a `Series` and its decoded `Frame`s.
    /// When `force` is `true`, even 1-2 slice stacks or RGB/non-standard series are
    /// promoted into a 3D volume using fallback slice thickness / orientation.
    pub fn from_series_with_force(
        series: &Series,
        frames: &[Arc<Frame>],
        force: bool,
    ) -> Option<Arc<Self>> {
        let min_frames = if force { 1 } else { 3 };
        if frames.len() < min_frames || frames.len() != series.slices.len() {
            return None;
        }
        let first = &frames[0];
        let last = &frames[frames.len() - 1];
        let w = first.width as usize;
        let h = first.height as usize;
        let d = frames.len();
        if w == 0 || h == 0 {
            return None;
        }
        // Verify all slices have matching dimensions (and greyscale unless forced).
        for f in frames {
            if f.width as usize != w || f.height as usize != h {
                return None;
            }
            if !force && !matches!(f.pixels, Pixels::Gray(_)) {
                return None;
            }
        }

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
        let mut voxels = vec![0.0f32; slice_len * d];
        let mut min_val = f32::INFINITY;
        let mut max_val = f32::NEG_INFINITY;

        for (z, f) in frames.iter().enumerate() {
            let dst = &mut voxels[z * slice_len..(z + 1) * slice_len];
            match &f.pixels {
                Pixels::Gray(src) => dst.copy_from_slice(src),
                Pixels::Rgba(rgba) => {
                    for (i, px) in rgba.chunks_exact(4).take(slice_len).enumerate() {
                        dst[i] =
                            0.299 * (px[0] as f32) + 0.587 * (px[1] as f32) + 0.114 * (px[2] as f32);
                    }
                }
            }
            let fw_min = (f.full_window.level - 0.5 * f.full_window.width) as f32;
            let fw_max = (f.full_window.level + 0.5 * f.full_window.width) as f32;
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
                    let v = voxels[row_base + x];
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

        Some(Arc::new(Self {
            width: w,
            height: h,
            depth: d,
            voxels,
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
        }))
    }

    /// Converts a 3D point in patient LPS coordinates (mm) to continuous voxel coordinates `(fx, fy, fz)`.
    #[inline]
    pub fn lps_to_voxel(&self, pt_lps: [f64; 3]) -> [f64; 3] {
        let rel = sub3(pt_lps, self.origin_lps);
        [
            dot3(rel, self.row_dir) / self.spacing[0],
            dot3(rel, self.col_dir) / self.spacing[1],
            dot3(rel, self.slice_dir) / self.spacing[2],
        ]
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

        let c000 = self.voxels[idx(x0, y0, z0)];
        let c100 = self.voxels[idx(x1, y0, z0)];
        let c010 = self.voxels[idx(x0, y1, z0)];
        let c110 = self.voxels[idx(x1, y1, z0)];
        let c001 = self.voxels[idx(x0, y0, z1)];
        let c101 = self.voxels[idx(x1, y0, z1)];
        let c011 = self.voxels[idx(x0, y1, z1)];
        let c111 = self.voxels[idx(x1, y1, z1)];

        let c00 = c000 + tx * (c100 - c000);
        let c10 = c010 + tx * (c110 - c010);
        let c01 = c001 + tx * (c101 - c001);
        let c11 = c011 + tx * (c111 - c011);

        let c0 = c00 + ty * (c10 - c00);
        let c1 = c01 + ty * (c11 - c01);

        Some(c0 + tz * (c1 - c0))
    }

    /// Background air value used outside the volume bounds during oblique reslicing.
    pub fn background_value(&self) -> f32 {
        (self.full_window.level - 0.5 * self.full_window.width) as f32
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

    /// Reslices the 3D volume (and any sibling orthogonal volumes from the same acquisition group)
    /// along an arbitrary 3D plane `plane`, returning a calibrated `Arc<Frame>`.
    pub fn reslice_plane(&self, siblings: &[Arc<Volume3D>], plane: &MprPlane3D) -> Arc<Frame> {
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

        let u_step = scale3(plane.u_axis, out_sp_u);
        let v_step = scale3(plane.v_axis, out_sp_v);
        let bg = self.background_value();

        // Compute resolution weights for this volume and any sibling orthogonal volumes (AX/COR/SAG).
        // A stack with thick through-plane spacing `dz` has high resolution when `plane.normal`
        // aligns with `vol.slice_dir`, and lower resolution when viewed edge-on.
        let mut sources: Vec<(&Volume3D, f32)> = Vec::with_capacity(1 + siblings.len());
        let weight_for = |v: &Volume3D| -> f32 {
            let align = dot3(v.slice_dir, plane.normal).abs();
            let eff_sp = (1.0 - align) * v.spacing[2] + align * v.spacing[0].min(v.spacing[1]);
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

        let mut pixels = vec![bg; out_w * out_h];
        let slab_mode = plane.slab_mode;
        pixels
            .par_chunks_mut(out_w)
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

        let eff_thickness = if slab_offsets.is_empty() {
            out_sp
        } else {
            plane.slab_thickness_mm
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

        Arc::new(Frame {
            width: out_w as u32,
            height: out_h as u32,
            pixels: Pixels::Gray(pixels),
            stored_window: self.stored_window,
            full_window: self.full_window,
            bits_window: self.bits_window,
            inverts: self.inverts,
            voi_lut_applied: false,
            slope: self.slope,
            spacing: Some((out_sp_v, out_sp_u)),
            pixel_ratio: 1.0,
            origin: Some(top_left_lps),
            orientation,
            is_3d_plane: true,
            frame_of_reference_uid: self.frame_of_reference_uid.clone(),
            slice_thickness: Some(eff_thickness),
            slice_location,
            value_unit: self.value_unit,
        })
    }
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
                    let val = source.voxels[z * wh + y * source.width + x];
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
}
