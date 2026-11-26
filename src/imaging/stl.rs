//! 3D iso-surface mesh extraction and binary STL export from a `Volume3D`.
//!
//! Uses a fast Surface Nets / dual-contouring cell pass in patient LPS
//! millimeter coordinates so bone/vessel/organ surfaces export cleanly to
//! `.stl` for 3D printing or external mesh tools.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::volume::Volume3D;

pub type Triangle3D = ([f32; 3], [f32; 3], [f32; 3]);

/// Extracts an iso-surface triangle mesh at `threshold` (in volume units / HU)
/// from `vol`, automatically striding if any axis exceeds `max_dim`.
pub fn extract_isosurface_triangles(
    vol: &Volume3D,
    threshold: f32,
    max_dim: usize,
) -> Vec<Triangle3D> {
    let max_dim = max_dim.max(8);
    let nx = vol.width;
    let ny = vol.height;
    let nz = vol.depth;
    let sx = nx.div_ceil(max_dim).max(1);
    let sy = ny.div_ceil(max_dim).max(1);
    let sz = nz.div_ceil(max_dim).max(1);

    let gx = nx / sx;
    let gy = ny / sy;
    let gz = nz / sz;
    if gx < 2 || gy < 2 || gz < 2 {
        return Vec::new();
    }

    let sample = |ix: usize, iy: usize, iz: usize| -> f32 {
        let vx = (ix * sx).min(nx - 1);
        let vy = (iy * sy).min(ny - 1);
        let vz = (iz * sz).min(nz - 1);
        vol.voxel_at((vz * ny + vy) * nx + vx)
    };

    let voxel_to_lps = |vx: f64, vy: f64, vz: f64| -> [f32; 3] {
        let px = vol.origin_lps[0]
            + vx * vol.spacing[0] * vol.row_dir[0]
            + vy * vol.spacing[1] * vol.col_dir[0]
            + vz * vol.spacing[2] * vol.slice_dir[0];
        let py = vol.origin_lps[1]
            + vx * vol.spacing[0] * vol.row_dir[1]
            + vy * vol.spacing[1] * vol.col_dir[1]
            + vz * vol.spacing[2] * vol.slice_dir[1];
        let pz = vol.origin_lps[2]
            + vx * vol.spacing[0] * vol.row_dir[2]
            + vy * vol.spacing[1] * vol.col_dir[2]
            + vz * vol.spacing[2] * vol.slice_dir[2];
        [px as f32, py as f32, pz as f32]
    };

    let cell_nx = gx - 1;
    let cell_ny = gy - 1;
    let cell_nz = gz - 1;

    // 1. Compute one dual vertex per sign-changing cell (Surface Nets).
    let mut cell_vert: Vec<Option<[f32; 3]>> = vec![None; cell_nx * cell_ny * cell_nz];
    let cell_idx = |cx: usize, cy: usize, cz: usize| (cz * cell_ny + cy) * cell_nx + cx;

    const CORNERS: [[usize; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [0, 1, 0],
        [1, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [0, 1, 1],
        [1, 1, 1],
    ];
    const EDGES: [[usize; 2]; 12] = [
        [0, 1],
        [2, 3],
        [4, 5],
        [6, 7],
        [0, 2],
        [1, 3],
        [4, 6],
        [5, 7],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ];

    for cz in 0..cell_nz {
        for cy in 0..cell_ny {
            for cx in 0..cell_nx {
                let mut vals = [0.0f32; 8];
                let mut mask = 0u8;
                for (i, c) in CORNERS.iter().enumerate() {
                    let v = sample(cx + c[0], cy + c[1], cz + c[2]);
                    vals[i] = v;
                    if v >= threshold {
                        mask |= 1 << i;
                    }
                }
                if mask == 0 || mask == 0xFF {
                    continue;
                }
                let mut sum = [0.0f64; 3];
                let mut count = 0u32;
                for &[a, b] in &EDGES {
                    let inside_a = (mask & (1 << a)) != 0;
                    let inside_b = (mask & (1 << b)) != 0;
                    if inside_a == inside_b {
                        continue;
                    }
                    let va = vals[a];
                    let vb = vals[b];
                    let denom = vb - va;
                    let t = if denom.abs() > 1e-6 {
                        ((threshold - va) / denom).clamp(0.0, 1.0) as f64
                    } else {
                        0.5
                    };
                    let pa = CORNERS[a];
                    let pb = CORNERS[b];
                    sum[0] += (cx + pa[0]) as f64 + t * (pb[0] as f64 - pa[0] as f64);
                    sum[1] += (cy + pa[1]) as f64 + t * (pb[1] as f64 - pa[1] as f64);
                    sum[2] += (cz + pa[2]) as f64 + t * (pb[2] as f64 - pa[2] as f64);
                    count += 1;
                }
                if count > 0 {
                    let vx = (sum[0] / count as f64) * sx as f64;
                    let vy = (sum[1] / count as f64) * sy as f64;
                    let vz = (sum[2] / count as f64) * sz as f64;
                    cell_vert[cell_idx(cx, cy, cz)] = Some(voxel_to_lps(vx, vy, vz));
                }
            }
        }
    }

    // 2. Emit two triangles (one quad) for every interior grid edge that crosses `threshold`.
    let mut tris = Vec::new();
    let mut push_quad = |v0: Option<[f32; 3]>,
                         v1: Option<[f32; 3]>,
                         v2: Option<[f32; 3]>,
                         v3: Option<[f32; 3]>,
                         flip: bool| {
        if let (Some(a), Some(b), Some(c), Some(d)) = (v0, v1, v2, v3) {
            if flip {
                tris.push((a, c, b));
                tris.push((a, d, c));
            } else {
                tris.push((a, b, c));
                tris.push((a, c, d));
            }
        }
    };

    for cz in 1..cell_nz {
        for cy in 1..cell_ny {
            for cx in 1..cell_nx {
                let v000 = sample(cx, cy, cz) >= threshold;
                // X-axis edge (cx, cy, cz) -> (cx+1, cy, cz)
                if cx + 1 < gx {
                    let v100 = sample(cx + 1, cy, cz) >= threshold;
                    if v000 != v100 {
                        push_quad(
                            cell_vert[cell_idx(cx, cy - 1, cz - 1)],
                            cell_vert[cell_idx(cx, cy, cz - 1)],
                            cell_vert[cell_idx(cx, cy, cz)],
                            cell_vert[cell_idx(cx, cy - 1, cz)],
                            !v000,
                        );
                    }
                }
                // Y-axis edge
                if cy + 1 < gy {
                    let v010 = sample(cx, cy + 1, cz) >= threshold;
                    if v000 != v010 {
                        push_quad(
                            cell_vert[cell_idx(cx - 1, cy, cz - 1)],
                            cell_vert[cell_idx(cx - 1, cy, cz)],
                            cell_vert[cell_idx(cx, cy, cz)],
                            cell_vert[cell_idx(cx, cy, cz - 1)],
                            !v000,
                        );
                    }
                }
                // Z-axis edge
                if cz + 1 < gz {
                    let v001 = sample(cx, cy, cz + 1) >= threshold;
                    if v000 != v001 {
                        push_quad(
                            cell_vert[cell_idx(cx - 1, cy - 1, cz)],
                            cell_vert[cell_idx(cx, cy - 1, cz)],
                            cell_vert[cell_idx(cx, cy, cz)],
                            cell_vert[cell_idx(cx - 1, cy, cz)],
                            !v000,
                        );
                    }
                }
            }
        }
    }

    tris
}

/// Writes `triangles` as a standard binary STL file (84-byte header + 50 bytes/triangle).
pub fn write_binary_stl(path: &Path, triangles: &[Triangle3D]) -> Result<()> {
    let mut file =
        std::fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut header = [0u8; 80];
    let banner = b"SekhmetX 3D Surface Export (mm LPS)";
    header[..banner.len()].copy_from_slice(banner);
    file.write_all(&header)?;
    file.write_all(&(triangles.len() as u32).to_le_bytes())?;

    for &(a, b, c) in triangles {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let nx = ab[1] * ac[2] - ab[2] * ac[1];
        let ny = ab[2] * ac[0] - ab[0] * ac[2];
        let nz = ab[0] * ac[1] - ab[1] * ac[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-8);
        let n = [nx / len, ny / len, nz / len];
        for v in [n, a, b, c] {
            file.write_all(&v[0].to_le_bytes())?;
            file.write_all(&v[1].to_le_bytes())?;
            file.write_all(&v[2].to_le_bytes())?;
        }
        file.write_all(&0u16.to_le_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_sphere_extracts_valid_binary_stl() {
        let n = 16usize;
        let mut voxels = vec![-1000_i16; n * n * n];
        let c = 7.5f32;
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let dx = x as f32 - c;
                    let dy = y as f32 - c;
                    let dz = z as f32 - c;
                    if dx * dx + dy * dy + dz * dz <= 16.0 {
                        voxels[(z * n + y) * n + x] = 500;
                    }
                }
            }
        }
        let vol = Volume3D {
            gpu_id: 1,
            width: n,
            height: n,
            depth: n,
            voxels,
            voxel_scale: 1.0,
            voxel_offset: 0.0,
            origin_lps: [0.0, 0.0, 0.0],
            row_dir: [1.0, 0.0, 0.0],
            col_dir: [0.0, 1.0, 0.0],
            slice_dir: [0.0, 0.0, 1.0],
            spacing: [1.0, 1.0, 1.0],
            center_lps: [7.5, 7.5, 7.5],
            foreground_centroid_lps: [7.5, 7.5, 7.5],
            native_orientation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            stored_window: None,
            full_window: crate::imaging::Window::new(0.0, 1500.0),
            bits_window: None,
            inverts: false,
            slope: 1.0,
            frame_of_reference_uid: None,
            value_unit: "HU",
            suv_factor: None,
            foreground_threshold: -200.0,
            foreground_points: Vec::new(),
            foreground_cell_mm: 4.0,
            geometry_verified: true,
            uncalibrated: false,
        };
        let tris = extract_isosurface_triangles(&vol, 0.0, 64);
        assert!(tris.len() > 20, "expected sphere surface triangles, got {}", tris.len());

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sphere.stl");
        write_binary_stl(&path, &tris).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.len() as usize, 84 + 50 * tris.len());
    }
}
