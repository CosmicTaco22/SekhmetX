//! Virtual Endoscopy / Fly-Through perspective surface ray-caster
//! (`endoscopy-flythru` area in `PORTING_MAP.md`).
//!
//! Renders an internal perspective view from a camera positioned along a
//! Curved MPR centerline (`pos_lps`), looking along `forward` with `up`,
//! finding the first iso-surface crossing of `threshold_hu` (e.g. `-500 HU`
//! for air-filled colon/bronchi, `+150 HU` for contrast vessels) and shading
//! with a warm endoscopic headlight (ambient + diffuse + specular + depth fog).

use rayon::prelude::*;

use crate::volume::{add3, cross3, dot3, normalize3, scale3, Volume3D};

/// Renders a `dim × dim` RGBA8 virtual endoscopy perspective frame from inside `vol`.
#[allow(clippy::too_many_arguments)]
pub fn render_flythrough_rgba(
    vol: &Volume3D,
    pos_lps: [f64; 3],
    forward: [f64; 3],
    up_hint: [f64; 3],
    fov_deg: f64,
    threshold_hu: f32,
    bright_wall: bool,
    dim: usize,
) -> Vec<u8> {
    let dim = dim.clamp(32, 320);
    let fwd = normalize3(forward);
    let mut right = cross3(fwd, up_hint);
    if dot3(right, right) < 1e-9 {
        right = cross3(fwd, if fwd[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
    }
    let right = normalize3(right);
    let up = normalize3(cross3(right, fwd));

    let half_tan = (fov_deg.clamp(30.0, 120.0).to_radians() * 0.5).tan();
    let step_mm = vol.spacing[0].min(vol.spacing[1]).min(vol.spacing[2]).clamp(0.4, 1.5);
    let max_dist_mm = 120.0_f64;
    let max_steps = (max_dist_mm / step_mm).ceil() as usize;
    let grad_eps = step_mm;

    let mut rgba = vec![0_u8; dim * dim * 4];
    let row_bytes = dim * 4;
    rgba.par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(y, row)| {
            let ndc_y = 1.0 - 2.0 * ((y as f64 + 0.5) / dim as f64);
            for x in 0..dim {
                let ndc_x = 2.0 * ((x as f64 + 0.5) / dim as f64) - 1.0;
                let dir = normalize3(add3(
                    fwd,
                    add3(scale3(right, ndc_x * half_tan), scale3(up, ndc_y * half_tan)),
                ));

                let mut hit_dist = None;
                let mut hit_pt = pos_lps;
                let mut t = step_mm * 1.5;
                for _ in 0..max_steps {
                    let p = add3(pos_lps, scale3(dir, t));
                    let Some(v) = vol.sample_lps(p) else {
                        break;
                    };
                    let hit = if bright_wall {
                        v >= threshold_hu
                    } else {
                        v <= threshold_hu
                    };
                    if hit {
                        hit_dist = Some(t);
                        hit_pt = p;
                        break;
                    }
                    t += step_mm;
                }

                let o = x * 4;
                if let Some(d) = hit_dist {
                    // Central-difference gradient normal at hit point.
                    let s = |dx: f64, dy: f64, dz: f64| -> f64 {
                        vol.sample_lps([hit_pt[0] + dx, hit_pt[1] + dy, hit_pt[2] + dz])
                            .unwrap_or(threshold_hu) as f64
                    };
                    let gx = s(grad_eps, 0.0, 0.0) - s(-grad_eps, 0.0, 0.0);
                    let gy = s(0.0, grad_eps, 0.0) - s(0.0, -grad_eps, 0.0);
                    let gz = s(0.0, 0.0, grad_eps) - s(0.0, 0.0, -grad_eps);
                    let mut n = normalize3([gx, gy, gz]);
                    // Face normal toward the camera (headlight).
                    let view_dir = scale3(dir, -1.0);
                    if dot3(n, view_dir) < 0.0 {
                        n = scale3(n, -1.0);
                    }
                    let ndotl = dot3(n, view_dir).clamp(0.0, 1.0);
                    let spec = ndotl.powf(24.0) * 0.45;
                    let atten = (1.0 - (d / max_dist_mm).clamp(0.0, 1.0)).powf(1.3);
                    let lit = (0.18 + 0.82 * ndotl) * atten;

                    // Warm mucosal tissue tint (like Horos virtual endoscopy).
                    let r = ((230.0 * lit + 255.0 * spec * atten).clamp(0.0, 255.0)) as u8;
                    let g = ((150.0 * lit + 235.0 * spec * atten).clamp(0.0, 255.0)) as u8;
                    let b = ((125.0 * lit + 225.0 * spec * atten).clamp(0.0, 255.0)) as u8;
                    row[o] = r;
                    row[o + 1] = g;
                    row[o + 2] = b;
                    row[o + 3] = 255;
                } else {
                    // Deep lumen background
                    row[o] = 14;
                    row[o + 1] = 10;
                    row[o + 2] = 12;
                    row[o + 3] = 255;
                }
            }
        });
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicom_index::{DominantPlane, Series, SliceRef};
    use crate::frame::{Frame, Pixels};
    use crate::imaging::Window;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn flythrough_renders_hollow_lumen_with_lit_wall() {
        // Build a 32x32x32 CT phantom: solid wall (+100 HU) with a hollow air tube (-1000 HU) along Z.
        let mut frames = Vec::new();
        let mut slices = Vec::new();
        for z in 0..32 {
            let mut px = vec![100.0_f32; 32 * 32];
            for y in 0..32 {
                for x in 0..32 {
                    let dx = x as f64 - 16.0;
                    let dy = y as f64 - 16.0;
                    if dx * dx + dy * dy < 36.0 {
                        px[y * 32 + x] = -1000.0;
                    }
                }
            }
            frames.push(Arc::new(Frame {
                width: 32,
                height: 32,
                pixels: Pixels::Gray(px),
                stored_window: None,
                full_window: Window::new(-450.0, 1100.0),
                bits_window: None,
                inverts: false,
                voi_lut_applied: false,
                slope: 1.0,
                spacing: Some((1.0, 1.0)),
                pixel_ratio: 1.0,
                origin: Some([0.0, 0.0, z as f64]),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                frame_of_reference_uid: None,
                slice_thickness: Some(1.0),
                slice_location: Some(z as f64),
                value_unit: "HU",
                suv_factor: None,
            }));
            slices.push(SliceRef {
                path: PathBuf::from(format!("t{z}.dcm")),
                frame: 0,
                instance: z as i64,
                position: Some([0.0, 0.0, z as f64]),
                orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                is_3d_plane: true,
                spacing: Some((1.0, 1.0)),
                slice_location: Some(z as f64),
                frame_of_reference_uid: None,
            });
        }
        let series = Series {
            uid: "tube".into(),
            number: 1,
            modality: "CT".into(),
            description: "Tube".into(),
            rows: 32,
            columns: 32,
            convolution_kernel: None,
            window_center: None,
            window_width: None,
            acquisition_time_secs: None,
            acquisition_number: None,
            frame_of_reference_uid: None,
            dominant_plane: DominantPlane::Axial,
            normal: Some([0.0, 0.0, 1.0]),
            mpr_group_id: None,
            slices,
        };
        let vol = Volume3D::from_series(&series, &frames).expect("tube volume");
        let rgba = render_flythrough_rgba(
            &vol,
            [16.0, 16.0, 4.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            75.0,
            -500.0,
            true,
            64,
        );
        assert_eq!(rgba.len(), 64 * 64 * 4);
        // Center pixel looks straight down the open lumen (dark); edge pixel hits the lit tube wall (brighter red).
        let center_r = rgba[(32 * 64 + 32) * 4];
        let wall_r = rgba[(32 * 64 + 4) * 4];
        assert!(wall_r > center_r, "expected wall ({wall_r}) brighter than open lumen ({center_r})");
    }
}
